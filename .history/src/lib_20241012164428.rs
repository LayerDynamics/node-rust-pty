use napi::bindgen_prelude::*;
use napi_derive::napi;
use tokio::task;
use std::sync::{Arc, Once};
use log::{info, warn, error, debug};
use std::io;
use std::time::Duration;
use tokio::time::timeout;
use parking_lot::Mutex;
use bytes::Bytes;
use crossbeam_channel::{bounded, Sender, Receiver};
use std::cell::RefCell;
use std::ffi::CString;
use std::ptr;

// Import necessary libc constants and functions
#[cfg(target_os = "linux")]
use libc::{TIOCSWINSZ, fork, setsid, execle, dup2, close, read, write, ioctl, grantpt, unlockpt, posix_openpt, ptsname, open, kill, waitpid, _exit, O_RDWR, O_NOCTTY, WNOHANG, SIGTERM, SIGKILL, C_IFLAG};

// Define Pid type
type PidT = i32;

// Thread-local buffer for reading
thread_local! {
    static READ_BUFFER: RefCell<[u8; 1024]> = RefCell::new([0u8; 1024]);
}

// Structure representing the PTY process
#[derive(Debug)]
struct PtyProcess {
    master_fd: i32,
    pid: PidT,
}

impl PtyProcess {
    fn new() -> io::Result<Self> {
        debug!("Creating new PtyProcess");

        #[cfg(target_os = "linux")]
        {
            // Step 1: Open PTY master
            let master_fd = unsafe { posix_openpt(O_RDWR | O_NOCTTY) };
            if master_fd < 0 {
                return Err(io::Error::last_os_error());
            }
            debug!("Opened PTY master_fd: {}", master_fd);

            // Step 2: Grant access to slave PTY
            if unsafe { grantpt(master_fd) } != 0 {
                unsafe { close(master_fd) };
                return Err(io::Error::last_os_error());
            }
            debug!("Granted PTY slave");

            // Step 3: Unlock PTY master
            if unsafe { unlockpt(master_fd) } != 0 {
                unsafe { close(master_fd) };
                return Err(io::Error::last_os_error());
            }
            debug!("Unlocked PTY master");

            // Step 4: Get slave PTY name
            let slave_name_ptr = unsafe { ptsname(master_fd) };
            if slave_name_ptr.is_null() {
                unsafe { close(master_fd) };
                return Err(io::Error::last_os_error());
            }
            let slave_name = unsafe { 
                std::ffi::CStr::from_ptr(slave_name_ptr) 
            }
            .to_string_lossy()
            .into_owned();
            debug!("Slave PTY name: {}", slave_name);

            // Step 5: Open slave PTY
            let slave_fd = unsafe { open(slave_name.as_ptr() as *const i8, O_RDWR) };
            if slave_fd < 0 {
                unsafe { close(master_fd) };
                return Err(io::Error::last_os_error());
            }
            debug!("Opened PTY slave_fd: {}", slave_fd);

            // Step 6: Fork the process
            let pid = unsafe { fork() };
            if pid < 0 {
                // Fork failed
                unsafe { close(master_fd) };
                unsafe { close(slave_fd) };
                return Err(io::Error::last_os_error());
            } else if pid == 0 {
                // Child process
                debug!("In child process");
                // Step 7a: Create a new session
                if unsafe { setsid() } < 0 {
                    error!("setsid failed");
                    unsafe { _exit(1) };
                }

                // Step 7b: Set slave PTY as controlling terminal
                if unsafe { ioctl(slave_fd, TIOCSWINSZ, &libc::winsize {
                    ws_row: 24,
                    ws_col: 80,
                    ws_xpixel: 0,
                    ws_ypixel: 0,
                }) } < 0 {
                    error!("ioctl TIOCSWINSZ failed");
                    unsafe { _exit(1) };
                }

                if unsafe { dup2(slave_fd, libc::STDIN_FILENO) } < 0 ||
                   unsafe { dup2(slave_fd, libc::STDOUT_FILENO) } < 0 ||
                   unsafe { dup2(slave_fd, libc::STDERR_FILENO) } < 0 {
                    error!("dup2 failed");
                    unsafe { _exit(1) };
                }

                // Close unused file descriptors
                unsafe { close(master_fd) };
                unsafe { close(slave_fd) };

                // Execute shell
                let shell = CString::new("/bin/bash").unwrap();
                let shell_arg = CString::new("bash").unwrap();
                unsafe {
                    execle(shell.as_ptr(), shell_arg.as_ptr(), ptr::null(), ptr::null());
                    // If execle fails
                    error!("execle failed");
                    _exit(1);
                }
            } else {
                // Parent process
                debug!("In parent process, child PID: {}", pid);
                // Close slave_fd in parent
                unsafe { close(slave_fd) };
                // Return master_fd and child PID
                Ok(PtyProcess { master_fd, pid })
            }
        }

        #[cfg(target_os = "macos")]
        {
            // Implement macOS PTY creation similarly
            // Placeholder implementation
            unimplemented!("macOS PTY creation not implemented")
        }

        #[cfg(not(any(target_os = "linux", target_os = "macos")))]
        {
            Err(io::Error::new(io::ErrorKind::Other, "Unsupported platform"))
        }
    }
}

#[napi]
#[derive(Clone)]
pub struct PtyHandle {
    pty: Arc<Mutex<Option<PtyProcess>>>,
    command_sender: Sender<PtyCommand>,
    command_receiver: Receiver<PtyCommand>,
    result_sender: Sender<PtyResult>,
    result_receiver: Receiver<PtyResult>,
}

#[derive(Debug)]
enum PtyCommand {
    Write(Bytes),
    Resize(u16, u16),
}

#[derive(Debug)]
enum PtyResult {
    Success(String),
    Failure(String),
}

static INIT: Once = Once::new();

fn initialize_logging() {
    INIT.call_once(|| {
        env_logger::init();
    });
}

impl PtyHandle {
    fn handle_commands(&self) {
        let pty = self.pty.clone();
        let receiver = self.command_receiver.clone();

        tokio::spawn(async move {
            info!("Command handler started");

            while let Ok(command) = receiver.recv() {
                debug!("Received command: {:?}", command);
                let pty_guard = pty.lock();

                let pty_process = match pty_guard.as_ref() {
                    Some(pty) => pty,
                    None => {
                        error!("PTY process not initialized.");
                        continue;
                    }
                };

                match command {
                    PtyCommand::Write(data) => {
                        debug!("Executing write command with {} bytes", data.len());
                        let bytes_written = unsafe {
                            write(pty_process.master_fd, data.as_ptr() as *const libc::c_void, data.len())
                        };
                        if bytes_written < 0 {
                            let err = io::Error::last_os_error();
                            error!("Failed to write data to PTY: {}", err);
                        } else {
                            info!("Wrote {} bytes to PTY.", bytes_written);
                        }
                    }
                    PtyCommand::Resize(cols, rows) => {
                        debug!("Executing resize command: cols={}, rows={}", cols, rows);
                        let mut winsize: libc::winsize = unsafe { std::mem::zeroed() };
                        winsize.ws_col = cols;
                        winsize.ws_row = rows;

                        let ret = unsafe { ioctl(pty_process.master_fd, TIOCSWINSZ, &winsize) };
                        if ret != 0 {
                            let err = io::Error::last_os_error();
                            error!("Failed to resize PTY: {}", err);
                        } else {
                            info!("Successfully resized PTY to cols: {}, rows: {}", cols, rows);
                        }
                    }
                }
            }
            warn!("Command receiver channel closed");
        });
    }

    fn send_result(&self, result: PtyResult) {
        let sender = self.result_sender.clone();

        tokio::spawn(async move {
            debug!("Sending result: {:?}", result);
            if let Err(err) = sender.send(result) {
                error!("Failed to send result: {}", err);
            } else {
                debug!("Result sent successfully");
            }
        });
    }

    fn listen_for_results(&self) {
        let receiver = self.result_receiver.clone();

        tokio::spawn(async move {
            info!("Result listener started");

            while let Ok(result) = receiver.recv() {
                debug!("Received result: {:?}", result);
                match result {
                    PtyResult::Success(message) => {
                        info!("Operation successful: {}", message);
                    }
                    PtyResult::Failure(message) => {
                        error!("Operation failed: {}", message);
                    }
                }
            }
            warn!("Result receiver channel closed");
        });
    }
}

#[napi]
impl PtyHandle {
    #[napi]
    pub async fn new() -> Result<Self> {
        initialize_logging();
        info!("Creating new PtyHandle");

        let creation_timeout = Duration::from_secs(60);
        let handle_result = timeout(creation_timeout, task::spawn_blocking(|| {
            debug!("Initializing PtyProcess");
            let pty_process = PtyProcess::new()?;
            debug!("PtyProcess initialized successfully");

            debug!("Setting up command channels");
            let (command_sender, command_receiver) = bounded(100);
            let (result_sender, result_receiver) = bounded(100);
            debug!("Command channels set up successfully");

            Ok::<_, io::Error>(PtyHandle {
                pty: Arc::new(Mutex::new(Some(pty_process))),
                command_sender,
                command_receiver,
                result_sender,
                result_receiver,
            })
        }))
        .await;

        // Handle the timeout and the result of spawn_blocking
        let handle = match handle_result {
            Ok(inner_result) => inner_result.map_err(|e| {
                Error::new(Status::GenericFailure, format!("Failed to create PtyHandle: {:?}", e))
            })?,
            Err(e) => {
                return Err(Error::new(
                    Status::GenericFailure,
                    format!("PtyHandle creation timed out: {:?}", e),
                ));
            }
        };

        handle.handle_commands();
        handle.listen_for_results();
        Ok(handle)
    }

    #[napi]
    pub async fn read(&self) -> Result<String> {
        initialize_logging();
        let weak_binding = Arc::downgrade(&self.pty);
        info!("Initiating read from PTY.");
        let read_timeout = Duration::from_secs(5);

        let read_future = task::spawn_blocking(move || -> std::result::Result<String, napi::Error> {
            let binding = weak_binding.upgrade().ok_or_else(|| {
                error!("Failed to upgrade weak reference");
                Error::new(Status::GenericFailure, "Failed to upgrade weak reference")
            })?;
            let pty_guard = binding.lock();
            let master_fd_opt = pty_guard
                .as_ref()
                .map(|pty| pty.master_fd)
                .ok_or_else(|| {
                    error!("PTY not initialized");
                    Error::new(Status::GenericFailure, "PTY not initialized")
                })?;

            READ_BUFFER.with(|buffer_cell| {
                let mut buffer = buffer_cell.borrow_mut();
                let bytes_read = unsafe {
                    read(
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
            })
        });

        match timeout(read_timeout, read_future).await {
            Ok(inner_result) => inner_result,
            Err(e) => {
                error!("Read operation timed out: {}", e);
                Err(Error::new(
                    Status::GenericFailure,
                    format!("Read operation timed out: {}", e),
                ))
            }
        }
    }

    #[napi]
    pub async fn write(&self, data: String) -> Result<()> {
        initialize_logging();
        let data_bytes = Bytes::from(data.into_bytes());

        info!("Initiating write to PTY.");
        self.command_sender.send(PtyCommand::Write(data_bytes)).map_err(|e| {
            error!("Failed to send write command: {}", e);
            Error::new(Status::GenericFailure, format!("Failed to send write command: {}", e))
        })?;

        self.send_result(PtyResult::Success("Write operation successful".into()));

        Ok(())
    }

    #[napi]
    pub async fn resize(&self, cols: u16, rows: u16) -> Result<()> {
        initialize_logging();
        info!("Initiating resize of PTY to cols: {}, rows: {}", cols, rows);

        self.command_sender.send(PtyCommand::Resize(cols, rows)).map_err(|e| {
            error!("Failed to send resize command: {}", e);
            Error::new(Status::GenericFailure, format!("Failed to send resize command: {}", e))
        })?;

        self.send_result(PtyResult::Success("Resize operation successful".into()));

        Ok(())
    }

    #[napi]
    pub async fn close(&self) -> Result<()> {
        initialize_logging();
        let weak_binding = Arc::downgrade(&self.pty);
        info!("Initiating graceful shutdown of PTY.");
        let graceful_timeout = Duration::from_secs(10);
        let force_timeout = Duration::from_secs(5);

        let close_result = timeout(graceful_timeout, task::spawn_blocking(move || -> std::result::Result<(), String> {
            let binding = weak_binding.upgrade().ok_or_else(|| "Failed to upgrade weak reference".to_string())?;
            let mut pty_option = binding.lock();

            if let Some(pty) = pty_option.take() {
                info!("Sending SIGTERM to child process {}.", pty.pid);
                let kill_ret = unsafe { kill(pty.pid, SIGTERM) };

                if kill_ret != 0 {
                    let err = io::Error::last_os_error();
                    error!("Failed to send SIGTERM to process {}: {}", pty.pid, err);
                } else {
                    info!("SIGTERM sent successfully.");
                    let wait_ret = unsafe { waitpid(pty.pid, ptr::null_mut(), WNOHANG) };
                    if wait_ret == 0 {
                        error!("Process {} did not terminate immediately after SIGTERM.", pty.pid);
                    } else if wait_ret == -1 {
                        let err = io::Error::last_os_error();
                        error!("Error waiting for process {}: {}", pty.pid, err);
                    } else {
                        info!("Process {} terminated gracefully.", pty.pid);
                    }
                }

                unsafe {
                    close(pty.master_fd);
                }
                info!("Closed master_fd {}.", pty.master_fd);

                Ok(())
            } else {
                warn!("PTY not initialized or already closed.");
                Ok(())
            }
        }))
        .await;

        match close_result {
            Ok(Ok(_)) => {
                info!("PTY closed successfully.");
                self.send_result(PtyResult::Success("PTY closed successfully".into()));
                Ok(())
            }
            Ok(Err(e)) => {
                error!("Error during graceful shutdown: {}", e);
                self.send_result(PtyResult::Failure(format!("Error during graceful shutdown: {}", e)));
                Err(Error::new(Status::GenericFailure, e))
            }
            Err(e) => {
                warn!("Graceful shutdown timed out. Initiating force kill. Error: {}", e);
                self.send_result(PtyResult::Failure(format!("Graceful shutdown timed out: {}", e)));
                self.force_kill(force_timeout).await
            }
        }
    }

    async fn force_kill(&self, force_timeout: Duration) -> Result<()> {
        initialize_logging();
        let weak_binding = Arc::downgrade(&self.pty);

        let force_kill_result = timeout(force_timeout, task::spawn_blocking(move || -> std::result::Result<(), String> {
            let binding = weak_binding.upgrade().ok_or_else(|| "Failed to upgrade weak reference".to_string())?;
            let mut pty_option = binding.lock();

            if let Some(pty) = pty_option.take() {
                info!("Sending SIGKILL to child process {}.", pty.pid);
                let kill_ret = unsafe { kill(pty.pid, SIGKILL) };

                if kill_ret != 0 {
                    let err = io::Error::last_os_error();
                    error!("Failed to send SIGKILL to process {}: {}", pty.pid, err);
                    Err(format!("Failed to send SIGKILL: {}", err))
                } else {
                    info!("SIGKILL sent successfully to process {}.", pty.pid);
                    unsafe { waitpid(pty.pid, ptr::null_mut(), 0) };

                    unsafe {
                        close(pty.master_fd);
                    }
                    info!("Closed master_fd {}.", pty.master_fd);
                    Ok(())
                }
            } else {
                warn!("PTY not initialized or already closed.");
                Ok(())
            }
        }))
        .await;

        match force_kill_result {
            Ok(Ok(_)) => {
                info!("PTY forcefully terminated.");
                self.send_result(PtyResult::Success("PTY forcefully terminated".into()));
                Ok(())
            }
            Ok(Err(e)) => {
                error!("Error during force kill: {}", e);
                self.send_result(PtyResult::Failure(format!("Error during force kill: {}", e)));
                Err(Error::new(Status::GenericFailure, e))
            }
            Err(e) => {
                error!("Force kill operation timed out: {}", e);
                self.send_result(PtyResult::Failure(format!("Force kill operation timed out: {}", e)));
                Err(Error::new(
                    Status::GenericFailure,
                    format!("Failed to terminate PTY process: {}", e),
                ))
            }
        }
    }
}

impl Drop for PtyHandle {
    fn drop(&mut self) {
        initialize_logging();
        let weak_binding = Arc::downgrade(&self.pty);
        info!("Dropping PtyHandle, initiating cleanup.");

        task::block_in_place(|| {
            if let Some(binding) = weak_binding.upgrade() {
                let mut pty_option = binding.lock();

                if let Some(pty) = pty_option.take() {
                    info!("Dropping: Sending SIGTERM to child process {}.", pty.pid);
                    let kill_ret = unsafe { kill(pty.pid, SIGTERM) };
                    if kill_ret != 0 {
                        let err = io::Error::last_os_error();
                        error!("Dropping: Failed to send SIGTERM to process {}: {}", pty.pid, err);
                    } else {
                        std::thread::sleep(Duration::from_secs(2));
                        let wait_ret = unsafe { waitpid(pty.pid, ptr::null_mut(), 0) };
                        if wait_ret == -1 {
                            let err = io::Error::last_os_error();
                            error!("Dropping: Process {} did not terminate gracefully: {}", pty.pid, err);
                        } else {
                            info!("Dropping: Process {} terminated gracefully.", pty.pid);
                        }
                    }

                    info!("Dropping: Sending SIGKILL to child process {} as a fallback.", pty.pid);
                    let kill_ret = unsafe { kill(pty.pid, SIGKILL) };
                    if kill_ret != 0 {
                        let err = io::Error::last_os_error();
                        error!("Dropping: Failed to send SIGKILL to process {}: {}", pty.pid, err);
                    } else {
                        info!("Dropping: SIGKILL sent to process {}.", pty.pid);
                        let _ = unsafe { waitpid(pty.pid, ptr::null_mut(), 0) };
                    }

                    if pty.master_fd >= 0 {
                        if unsafe { close(pty.master_fd) } != 0 {
                            let err = io::Error::last_os_error();
                            error!("Dropping: Failed to close master_fd {}: {}", pty.master_fd, err);
                        } else {
                            info!("Dropping: Master_fd {} closed successfully.", pty.master_fd);
                        }
                    }

                    info!("Dropping: PTY session closed via Drop.");
                } else {
                    warn!("Dropping: PTY not initialized or already closed.");
                }
            } else {
                warn!("Dropping: Failed to upgrade weak reference to PTY.");
            }
        });
    }
}

#[napi]
pub async fn test_pty_handle_creation() -> Result<bool> {
    initialize_logging();
    info!("Starting test_pty_handle_creation");
    let pty_handle = PtyHandle::new().await?;

    // Use the pty_handle to perform a simple operation
    pty_handle.resize(80, 24).await?;

    // Read from the PTY (this might block if there's no data, so we'll add a timeout)
    let read_result = tokio::time::timeout(Duration::from_secs(1), pty_handle.read()).await;
    match read_result {
        Ok(Ok(data)) => info!("Read from PTY: {}", data),
        Ok(Err(e)) => warn!("Error reading from PTY: {}", e),
        Err(_) => info!("No data read from PTY within timeout"),
    }

    // Write to the PTY
    pty_handle.write("echo Hello, PTY!\n".to_string()).await?;

    // Close the PTY
    pty_handle.close().await?;

    info!("PtyHandle created, used, and closed successfully");
    Ok(true)
}
