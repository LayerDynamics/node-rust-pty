use napi::bindgen_prelude::*;
use napi_derive::napi;
use std::ffi::CString;
use tokio::task;
use std::sync::{Arc, Mutex, Once};
use log::{info, warn, error};
use std::io;
use std::time::Duration;
use tokio::time::timeout;

#[link(name = "util")]
extern "C" {
    fn openpty(
        amaster: *mut i32,
        aslave: *mut i32,
        name: *mut i8,
        termp: *mut libc::termios,
        winp: *mut libc::winsize,
    ) -> i32;

    fn forkpty(
        amaster: *mut i32,
        name: *mut i8,
        termp: *mut libc::termios,
        winp: *mut libc::winsize,
    ) -> PidT;
}

/// Alias for process ID type
type PidT = i32;

/// Structure to hold PTY information
///
/// This structure stores both the master and slave file descriptors
/// and the PID of the child process after forking.
#[derive(Debug)]
struct PtyProcess {
    master_fd: i32,
    slave_fd: i32,
    pid: PidT,
}

/// Exposed PTY handle for Node.js
///
/// The `PtyHandle` structure is exposed to the JavaScript environment
/// via NAPI. It allows operations such as reading, writing, resizing, and
/// closing PTY sessions.
#[napi]
pub struct PtyHandle {
    pty: Arc<Mutex<Option<PtyProcess>>>,
}

static INIT: Once = Once::new();

/// Initializes the logging system.
///
/// Ensures that `env_logger::init()` is called only once to prevent multiple initializations.
fn initialize_logging() {
    INIT.call_once(|| {
        env_logger::init();
    });
}

#[napi]
impl PtyHandle {
    /// Constructor for `PtyHandle` to create a new PTY process
    ///
    /// This function forks the process to create a PTY, initializing a child
    /// process that executes the specified shell with arguments.
    ///
    /// # Parameters
    /// - `shell`: The shell executable path to be invoked (e.g., `/bin/bash`).
    /// - `args`: Arguments passed to the shell upon execution.
    ///
    /// # Returns
    /// `PtyHandle` containing the process information or an error if PTY creation fails.
    ///
    /// # Examples
    /// ```javascript
    /// const { PtyHandle } = require('node-rust-pty');
    /// const pty = new PtyHandle('/bin/bash', ['-l']);
    /// ```
    #[napi(constructor)]
    pub fn new(shell: String, args: Vec<String>) -> Result<Self> {
        // Initialize logging system
        initialize_logging();
        info!("Initializing new PTY handle.");

        unsafe {
            let mut master: i32 = 0;
            let mut slave: i32 = 0;
            let mut name: [i8; 64] = [0; 64];
            let mut termios: libc::termios = std::mem::zeroed();
            let mut winsize: libc::winsize = std::mem::zeroed();

            // Open a new PTY pair (master/slave)
            let ret = openpty(
                &mut master as *mut i32,
                &mut slave as *mut i32,
                name.as_mut_ptr(),
                &mut termios as *mut libc::termios,
                &mut winsize as *mut libc::winsize,
            );

            if ret != 0 {
                let err = io::Error::last_os_error();
                error!("Failed to open PTY: {}", err);
                return Err(Error::new(
                    Status::GenericFailure,
                    format!("Failed to open PTY: {}", err),
                ));
            }
            info!(
                "Successfully opened PTY with master_fd: {}, slave_fd: {}",
                master, slave
            );

            // Fork the process into a child and parent process, connecting the PTY
            let pid = forkpty(
                &mut master as *mut i32,
                name.as_mut_ptr(),
                &mut termios as *mut libc::termios,
                &mut winsize as *mut libc::winsize,
            );

            if pid < 0 {
                let err = io::Error::last_os_error();
                error!("Failed to fork PTY: {}", err);
                return Err(Error::new(
                    Status::GenericFailure,
                    format!("Failed to fork PTY: {}", err),
                ));
            } else if pid == 0 {
                // Child process: execute the shell with provided arguments
                info!("In child process. Executing shell: {}", shell);
                let shell_c = match CString::new(shell.clone()) {
                    Ok(cstr) => cstr,
                    Err(e) => {
                        error!("Invalid shell string: {}", e);
                        libc::exit(1);
                    }
                };

                let args_c: Vec<CString> = args
                    .iter()
                    .map(|arg| {
                        CString::new(arg.as_str()).unwrap_or_else(|_| {
                            error!("Invalid argument string: {}", arg);
                            CString::new("").unwrap()
                        })
                    })
                    .collect();

                let mut args_ptr: Vec<*const i8> =
                    args_c.iter().map(|arg| arg.as_ptr()).collect();
                args_ptr.push(std::ptr::null());

                // Execute the shell command using execvp
                if libc::execvp(shell_c.as_ptr(), args_ptr.as_ptr()) == -1 {
                    let err = io::Error::last_os_error();
                    error!("Failed to execute shell '{}': {}", shell, err);
                    libc::exit(1);
                }
            }

            // Parent process: log the PID of the spawned PTY
            info!("PTY spawned with PID: {}", pid);
            Ok(Self {
                pty: Arc::new(Mutex::new(Some(PtyProcess {
                    master_fd: master,
                    slave_fd: slave,
                    pid,
                }))),
            })
        }
    }

    /// Reads data from the master PTY asynchronously with a timeout
    ///
    /// This function attempts to read from the master PTY within a specified timeout duration.
    /// If the read operation does not complete within the timeout, it returns an error.
    ///
    /// # Returns
    /// A `Result` containing the output string or an error if the operation fails or times out.
    ///
    /// # Examples
    /// ```javascript
    /// const output = await pty.read();
    /// console.log(output);
    /// ```
    #[napi]
    pub async fn read(&self) -> Result<String> {
        let pty_lock = Arc::clone(&self.pty);
        info!("Initiating read from PTY.");
        // Set a timeout duration for the read operation
        let read_timeout = Duration::from_secs(5);

        let data = match timeout(read_timeout, task::spawn_blocking(move || -> Result<String> {
            let master_fd_opt = {
                let pty_guard = pty_lock.lock().map_err(|e| {
                    error!("Failed to acquire PTY lock: {}", e);
                    Error::new(Status::GenericFailure, "Failed to acquire PTY lock")
                })?;
                pty_guard
                    .as_ref()
                    .map(|pty| pty.master_fd)
                    .ok_or_else(|| {
                        error!("PTY not initialized");
                        Error::new(Status::GenericFailure, "PTY not initialized")
                    })
            }?;

            let mut buffer = [0u8; 1024];
            let bytes_read = unsafe {
                libc::read(
                    master_fd_opt,
                    buffer.as_mut_ptr() as *mut libc::c_void,
                    buffer.len(),
                )
            };

            if bytes_read > 0 {
                let data = String::from_utf8_lossy(&buffer[..bytes_read as usize]).to_string();
                info!("Read {} bytes from PTY.", bytes_read);
                Ok(data)
            } else if bytes_read == 0 {
                // EOF reached
                warn!("EOF reached on PTY.");
                Ok(String::new())
            } else {
                let err = io::Error::last_os_error();
                error!("Failed to read from PTY: {}", err);
                Err(Error::new(
                    Status::GenericFailure,
                    format!("Failed to read from PTY: {}", err),
                ))
            }
        })).await {
            Ok(join_result) => match join_result {
                Ok(result) => result,
                Err(join_error) => {
                    error!("Task join error during read: {}", join_error);
                    return Err(Error::new(
                        Status::GenericFailure,
                        "Task join error during read.".to_string(),
                    ));
                }
            },
            Err(_) => {
                error!("Read operation timed out after {:?}", read_timeout);
                return Err(Error::new(
                    Status::GenericFailure,
                    "Read operation timed out.".to_string(),
                ));
            }
        };

        Ok(data) // `data` is of type `String`
    }

    /// Writes data to the master PTY asynchronously with a timeout
    ///
    /// This function writes the provided string to the master PTY within a specified timeout duration.
    ///
    /// # Parameters
    /// - `data`: The string to write to the PTY.
    ///
    /// # Returns
    /// A `Result` indicating success or failure.
    ///
    /// # Examples
    /// ```javascript
    /// await pty.write('ls -la\n');
    /// ```
    #[napi]
    pub async fn write(&self, data: String) -> Result<()> {
        let pty_lock = Arc::clone(&self.pty);
        let data_bytes = data.into_bytes();
        info!("Initiating write to PTY: {}", String::from_utf8_lossy(&data_bytes));
        // Set a timeout duration for the write operation
        let write_timeout = Duration::from_secs(5);

        match timeout(write_timeout, task::spawn_blocking(move || -> Result<()> {
            let master_fd_opt = {
                let pty_guard = pty_lock.lock().map_err(|e| {
                    error!("Failed to acquire PTY lock: {}", e);
                    Error::new(Status::GenericFailure, "Failed to acquire PTY lock")
                })?;
                pty_guard
                    .as_ref()
                    .map(|pty| pty.master_fd)
                    .ok_or_else(|| {
                        error!("PTY not initialized");
                        Error::new(Status::GenericFailure, "PTY not initialized")
                    })
            }?;

            let write_ptr = data_bytes.as_ptr() as *const libc::c_void;
            let write_len = data_bytes.len();

            let bytes_written = unsafe { libc::write(master_fd_opt, write_ptr, write_len) };
            if bytes_written == -1 {
                let err = io::Error::last_os_error();
                error!("Failed to write to PTY: {}", err);
                Err(Error::new(
                    Status::GenericFailure,
                    format!("Failed to write to PTY: {}", err),
                ))
            } else {
                info!("Successfully wrote {} bytes to PTY.", bytes_written);
                Ok(())
            }
        })).await {
            Ok(join_result) => match join_result {
                Ok(result) => result,
                Err(join_error) => {
                    error!("Task join error during write: {}", join_error);
                    return Err(Error::new(
                        Status::GenericFailure,
                        "Task join error during write.".to_string(),
                    ));
                }
            },
            Err(_) => {
                error!("Write operation timed out after {:?}", write_timeout);
                return Err(Error::new(
                    Status::GenericFailure,
                    "Write operation timed out.".to_string(),
                ));
            }
        };

        Ok(())
    }

    /// Resizes the PTY terminal window asynchronously with a timeout
    ///
    /// This method changes the terminal's dimensions (rows and columns) within a specified timeout duration.
    ///
    /// # Parameters
    /// - `cols`: Number of columns.
    /// - `rows`: Number of rows.
    ///
    /// # Returns
    /// A `Result` indicating success or failure.
    ///
    /// # Examples
    /// ```javascript
    /// await pty.resize(100, 30);
    /// ```
    #[napi]
    pub async fn resize(&self, cols: u16, rows: u16) -> Result<()> {
        let pty_lock = Arc::clone(&self.pty);
        info!("Initiating resize of PTY to cols: {}, rows: {}", cols, rows);
        // Set a timeout duration for the resize operation
        let resize_timeout = Duration::from_secs(5);

        match timeout(resize_timeout, task::spawn_blocking(move || -> Result<()> {
            let mut winsize: libc::winsize = unsafe { std::mem::zeroed() };
            winsize.ws_col = cols;
            winsize.ws_row = rows;
            winsize.ws_xpixel = 0;
            winsize.ws_ypixel = 0;

            let master_fd_opt = {
                let pty_guard = pty_lock.lock().map_err(|e| {
                    error!("Failed to acquire PTY lock: {}", e);
                    Error::new(Status::GenericFailure, "Failed to acquire PTY lock")
                })?;
                pty_guard
                    .as_ref()
                    .map(|pty| pty.master_fd)
                    .ok_or_else(|| {
                        error!("PTY not initialized");
                        Error::new(Status::GenericFailure, "PTY not initialized")
                    })
            }?;

            // Resize the PTY terminal using the ioctl system call
            let ret = unsafe { libc::ioctl(master_fd_opt, libc::TIOCSWINSZ, &winsize) };
            if ret != 0 {
                let err = io::Error::last_os_error();
                error!("Failed to resize PTY: {}", err);
                Err(Error::new(
                    Status::GenericFailure,
                    format!("Failed to resize PTY: {}", err),
                ))
            } else {
                info!("Successfully resized PTY to cols: {}, rows: {}", cols, rows);
                Ok(())
            }
        })).await {
            Ok(join_result) => match join_result {
                Ok(result) => result,
                Err(join_error) => {
                    error!("Task join error during resize: {}", join_error);
                    return Err(Error::new(
                        Status::GenericFailure,
                        "Task join error during resize.".to_string(),
                    ));
                }
            },
            Err(_) => {
                error!("Resize operation timed out after {:?}", resize_timeout);
                return Err(Error::new(
                    Status::GenericFailure,
                    "Resize operation timed out.".to_string(),
                ));
            }
        };

        Ok(())
    }

    /// Closes the PTY session asynchronously with a timeout
    ///
    /// This function attempts to gracefully terminate the PTY by sending a `SIGTERM` signal.
    /// If the process does not terminate within a specified timeout, it forcefully kills the process using `SIGKILL`.
    ///
    /// # Returns
    /// A `Result` indicating success or failure.
    ///
    /// # Examples
    /// ```javascript
    /// await pty.close();
    /// ```
    #[napi]
    pub async fn close(&self) -> Result<()> {
        let pty_lock = Arc::clone(&self.pty);
        info!("Initiating graceful shutdown of PTY.");
        // Set a timeout duration for the close operation
        let close_timeout = Duration::from_secs(10);

        match timeout(close_timeout, task::spawn_blocking(move || -> Result<()> {
            let mut pty_option = pty_lock.lock().map_err(|e| {
                error!("Failed to acquire PTY lock: {}", e);
                Error::new(Status::GenericFailure, "Failed to acquire PTY lock")
            })?;
            if let Some(pty) = pty_option.take() {
                // Attempt graceful shutdown by sending SIGTERM
                info!("Sending SIGTERM to child process {}.", pty.pid);
                let kill_ret = unsafe { libc::kill(pty.pid, libc::SIGTERM) };
                if kill_ret != 0 {
                    let err = io::Error::last_os_error();
                    error!("Failed to send SIGTERM to process {}: {}", pty.pid, err);
                    // Proceed to hard kill
                } else {
                    // Wait for the process to terminate gracefully
                    std::thread::sleep(Duration::from_secs(2));
                    // Check if the process has terminated
                    let wait_ret = unsafe { libc::waitpid(pty.pid, std::ptr::null_mut(), 0) };
                    if wait_ret == -1 {
                        let err = io::Error::last_os_error();
                        error!("Process {} did not terminate gracefully: {}", pty.pid, err);
                        // Proceed to hard kill
                    } else {
                        info!("Process {} terminated gracefully.", pty.pid);
                        // Proceed to close file descriptors
                    }
                }

                // If process is still alive, force kill
                // Note: In a real-world scenario, you might want to check if the process is still alive before sending SIGKILL
                info!("Sending SIGKILL to child process {} as a fallback.", pty.pid);
                let kill_ret = unsafe { libc::kill(pty.pid, libc::SIGKILL) };
                if kill_ret != 0 {
                    let err = io::Error::last_os_error();
                    error!("Failed to send SIGKILL to process {}: {}", pty.pid, err);
                    // Continue to close file descriptors regardless
                } else {
                    info!("SIGKILL sent to process {}.", pty.pid);
                }

                // Close master file descriptor
                if unsafe { libc::close(pty.master_fd) } != 0 {
                    let err = io::Error::last_os_error();
                    error!(
                        "Failed to close master_fd {}: {}",
                        pty.master_fd, err
                    );
                    // Continue to close slave_fd
                } else {
                    info!("Master_fd {} closed successfully.", pty.master_fd);
                }

                // Close slave file descriptor
                if unsafe { libc::close(pty.slave_fd) } != 0 {
                    let err = io::Error::last_os_error();
                    error!(
                        "Failed to close slave_fd {}: {}",
                        pty.slave_fd, err
                    );
                } else {
                    info!("Slave_fd {} closed successfully.", pty.slave_fd);
                }

                info!("PTY session with PID {} closed.", pty.pid);
                Ok(())
            } else {
                warn!("PTY not initialized or already closed.");
                Ok(())
            }
        })).await {
            Ok(join_result) => match join_result {
                Ok(result) => result,
                Err(join_error) => {
                    error!("Task join error during close: {}", join_error);
                    return Err(Error::new(
                        Status::GenericFailure,
                        "Task join error during close.".to_string(),
                    ));
                }
            },
            Err(_) => {
                error!("Close operation timed out after {:?}", close_timeout);
                return Err(Error::new(
                    Status::GenericFailure,
                    "Close operation timed out.".to_string(),
                ));
            }
        };

        Ok(())
    }
}

/// Implements the `Drop` trait for `PtyHandle`
///
/// Ensures that the PTY session is properly closed when the handle goes out of scope,
/// attempting to kill the process gracefully and falling back to a hard kill if necessary.
impl Drop for PtyHandle {
    fn drop(&mut self) {
        // Attempt to close the PTY if it's still open
        let pty_lock = Arc::clone(&self.pty);
        info!("Dropping PtyHandle, initiating cleanup.");

        // Using a blocking call here; ensure it's safe and won't cause deadlocks
        task::block_in_place(|| {
            let mut pty_option = match pty_lock.lock() {
                Ok(guard) => guard,
                Err(e) => {
                    error!("Failed to acquire PTY lock during Drop: {}", e);
                    return;
                }
            };

            if let Some(pty) = pty_option.take() {
                // Attempt graceful shutdown by sending SIGTERM
                info!("Dropping: Sending SIGTERM to child process {}.", pty.pid);
                let kill_ret = unsafe { libc::kill(pty.pid, libc::SIGTERM) };
                if kill_ret != 0 {
                    let err = io::Error::last_os_error();
                    error!("Dropping: Failed to send SIGTERM to process {}: {}", pty.pid, err);
                    // Proceed to hard kill
                } else {
                    // Wait for the process to terminate gracefully
                    std::thread::sleep(Duration::from_secs(2));
                    let wait_ret = unsafe { libc::waitpid(pty.pid, std::ptr::null_mut(), 0) };
                    if wait_ret == -1 {
                        let err = io::Error::last_os_error();
                        error!("Dropping: Process {} did not terminate gracefully: {}", pty.pid, err);
                        // Proceed to hard kill
                    } else {
                        info!("Dropping: Process {} terminated gracefully.", pty.pid);
                        // Proceed to close file descriptors
                    }
                }

                // If process is still alive, force kill
                info!("Dropping: Sending SIGKILL to child process {} as a fallback.", pty.pid);
                let kill_ret = unsafe { libc::kill(pty.pid, libc::SIGKILL) };
                if kill_ret != 0 {
                    let err = io::Error::last_os_error();
                    error!("Dropping: Failed to send SIGKILL to process {}: {}", pty.pid, err);
                } else {
                    info!("Dropping: SIGKILL sent to process {}.", pty.pid);
                }

                // Close master file descriptor
                if unsafe { libc::close(pty.master_fd) } != 0 {
                    let err = io::Error::last_os_error();
                    error!(
                        "Dropping: Failed to close master_fd {}: {}",
                        pty.master_fd, err
                    );
                } else {
                    info!("Dropping: Master_fd {} closed successfully.", pty.master_fd);
                }

                // Close slave file descriptor
                if unsafe { libc::close(pty.slave_fd) } != 0 {
                    let err = io::Error::last_os_error();
                    error!(
                        "Dropping: Failed to close slave_fd {}: {}",
                        pty.slave_fd, err
                    );
                } else {
                    info!("Dropping: Slave_fd {} closed successfully.", pty.slave_fd);
                }

                info!("Dropping: PTY session with PID {} closed via Drop.", pty.pid);
            } else {
                warn!("Dropping: PTY not initialized or already closed.");
            }
        });
    }
}