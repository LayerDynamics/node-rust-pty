use napi::bindgen_prelude::*;
use napi_derive::napi;
use std::ffi::CString;
use tokio::task;
use tokio::task::JoinError; 
use std::sync::{Arc, Mutex, Once};
use log::{info, warn, error};
use std::io;
use std::time::Duration;
use tokio::time::{timeout, error::Elapsed};

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

type PidT = i32;

#[derive(Debug)]
struct PtyProcess {
    master_fd: i32,
    slave_fd: i32,
    pid: PidT,
}

#[napi]
pub struct PtyHandle {
    pty: Arc<Mutex<Option<PtyProcess>>>,
}

static INIT: Once = Once::new();

fn initialize_logging() {
    INIT.call_once(|| {
        env_logger::init();
    });
}

#[napi]
impl PtyHandle {
    #[napi(constructor)]
    pub fn new(shell: String, args: Vec<String>) -> Result<Self> {
        initialize_logging();
        info!("Initializing new PTY handle.");

        unsafe {
            let mut master: i32 = 0;
            let mut slave: i32 = 0;
            let mut name: [i8; 64] = [0; 64];
            let mut termios: libc::termios = std::mem::zeroed();
            let mut winsize: libc::winsize = std::mem::zeroed();

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

                if libc::execvp(shell_c.as_ptr(), args_ptr.as_ptr()) == -1 {
                    let err = io::Error::last_os_error();
                    error!("Failed to execute shell '{}': {}", shell, err);
                    libc::exit(1);
                }
            }

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

    #[napi]
    pub async fn read(&self) -> Result<String> {
        let pty_lock = Arc::clone(&self.pty);
        info!("Initiating read from PTY.");
        let read_timeout = Duration::from_secs(5);

        let result = timeout(read_timeout, task::spawn_blocking(move || -> Result<String> {
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
                    })?
            };

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
        }))
        .await
        .map_err(|e: Elapsed| {
            error!("Read operation timed out after {:?}", read_timeout);
            Error::new(Status::GenericFailure, format!("Read operation timed out: {}", e))
        })?
        .map_err(|e| {
            error!("Task join error during read: {}", e);
            Error::new(Status::GenericFailure, format!("Task join error: {}", e))
        })?;

        Ok(result?)
    }

    #[napi]
    pub async fn write(&self, data: String) -> Result<()> {
        let pty_lock = Arc::clone(&self.pty);
        let data_bytes = data.into_bytes();
        info!("Initiating write to PTY: {}", String::from_utf8_lossy(&data_bytes));
        let write_timeout = Duration::from_secs(5);

        let _result = timeout(write_timeout, task::spawn_blocking(move || -> Result<()> {
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
                    })?
            };

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
        }))
        .await
        .map_err(|e: Elapsed| {
            error!("Write operation timed out after {:?}", write_timeout);
            Error::new(Status::GenericFailure, format!("Write operation timed out: {}", e))
        })?
        .map_err(|e| {
            error!("Task join error during write: {}", e);
            Error::new(Status::GenericFailure, format!("Task join error: {}", e))
        })?;

        Ok(())
    }

    #[napi]
    pub async fn resize(&self, cols: u16, rows: u16) -> Result<()> {
        let pty_lock = Arc::clone(&self.pty);
        info!("Initiating resize of PTY to cols: {}, rows: {}", cols, rows);
        let resize_timeout = Duration::from_secs(5);

        let _result = timeout(resize_timeout, task::spawn_blocking(move || -> Result<()> {
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
                    })?
            };

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
        }))
        .await
        .map_err(|e: Elapsed| {
            error!("Resize operation timed out after {:?}", resize_timeout);
            Error::new(Status::GenericFailure, format!("Resize operation timed out: {}", e))
        })?
        .map_err(|e| {
            error!("Task join error during resize: {}", e);
            Error::new(Status::GenericFailure, format!("Task join error: {}", e))
        })?;

        Ok(())
    }

#[napi]
pub async fn close(&self) -> Result<()> {
    let pty_lock = Arc::clone(&self.pty);
    info!("Initiating graceful shutdown of PTY.");
    let close_timeout = Duration::from_secs(10);

    let result = timeout(close_timeout, task::spawn_blocking(move || -> Result<()> {
        let mut pty_option = pty_lock.lock().map_err(|e| {
            error!("Failed to acquire PTY lock: {}", e);
            Error::new(Status::GenericFailure, "Failed to acquire PTY lock")
        })?;

        if let Some(pty) = pty_option.take() {
            info!("Sending SIGTERM to child process {}.", pty.pid);
            let kill_ret = unsafe { libc::kill(pty.pid, libc::SIGTERM) };

            if kill_ret != 0 {
                let err = io::Error::last_os_error();
                error!("Failed to send SIGTERM to process {}: {}", pty.pid, err);
            } else {
                info!("SIGTERM sent successfully. Waiting for child process {} to terminate.", pty.pid);
                std::thread::sleep(Duration::from_secs(2)); // Short wait for process to handle SIGTERM
                
                let wait_ret = unsafe { libc::waitpid(pty.pid, std::ptr::null_mut(), 0) };
                if wait_ret == -1 {
                    let err = io::Error::last_os_error();
                    error!("Process {} did not terminate gracefully: {}", pty.pid, err);
                } else {
                    info!("Process {} terminated gracefully.", pty.pid);
                }
            }

            // If process is still running, send SIGKILL to force termination
            if unsafe { libc::kill(pty.pid, 0) } == 0 {
                info!("Process {} still alive. Sending SIGKILL.", pty.pid);
                let kill_ret = unsafe { libc::kill(pty.pid, libc::SIGKILL) };
                if kill_ret != 0 {
                    let err = io::Error::last_os_error();
                    error!("Failed to send SIGKILL to process {}: {}", pty.pid, err);
                } else {
                    info!("SIGKILL sent to process {}.", pty.pid);
                }
            }

            // Close master file descriptor
            if unsafe { libc::close(pty.master_fd) } != 0 {
                let err = io::Error::last_os_error();
                error!("Failed to close master_fd {}: {}", pty.master_fd, err);
            } else {
                info!("Master_fd {} closed successfully.", pty.master_fd);
            }

            // Close slave file descriptor
            if unsafe { libc::close(pty.slave_fd) } != 0 {
                let err = io::Error::last_os_error();
                error!("Failed to close slave_fd {}: {}", pty.slave_fd, err);
            } else {
                info!("Slave_fd {} closed successfully.", pty.slave_fd);
            }

            info!("PTY session with PID {} closed.", pty.pid);
            Ok(())
        } else {
            warn!("PTY not initialized or already closed.");
            Ok(())
        }
    }))
    .await
    .map_err(|e: Elapsed| {
        error!("Close operation timed out after {:?}", close_timeout);
        Error::new(Status::GenericFailure, format!("Close operation timed out: {}", e))
    })?
    .map_err(|e: JoinError| {
        error!("Task join error during close: {}", e);
        Error::new(Status::GenericFailure, format!("Task join error: {}", e))
    })?;

    Ok(())
}

}

impl Drop for PtyHandle {
    fn drop(&mut self) {
        let pty_lock = Arc::clone(&self.pty);
        info!("Dropping PtyHandle, initiating cleanup.");

        task::block_in_place(|| {
            let mut pty_option = match pty_lock.lock() {
                Ok(guard) => guard,
                Err(e) => {
                    error!("Failed to acquire PTY lock during Drop: {}", e);
                    return;
                }
            };

            if let Some(pty) = pty_option.take() {
                info!("Dropping: Sending SIGTERM to child process {}.", pty.pid);
                let kill_ret = unsafe { libc::kill(pty.pid, libc::SIGTERM) };
                if kill_ret != 0 {
                    let err = io::Error::last_os_error();
                    error!("Dropping: Failed to send SIGTERM to process {}: {}", pty.pid, err);
                } else {
                    std::thread::sleep(Duration::from_secs(2));
                    let wait_ret = unsafe { libc::waitpid(pty.pid, std::ptr::null_mut(), 0) };
                    if wait_ret == -1 {
                        let err = io::Error::last_os_error();
                        error!("Dropping: Process {} did not terminate gracefully: {}", pty.pid, err);
                    } else {
                        info!("Dropping: Process {} terminated gracefully.", pty.pid);
                    }
                }

                info!("Dropping: Sending SIGKILL to child process {} as a fallback.", pty.pid);
                let kill_ret = unsafe { libc::kill(pty.pid, libc::SIGKILL) };
                if kill_ret != 0 {
                    let err = io::Error::last_os_error();
                    error!("Dropping: Failed to send SIGKILL to process {}: {}", pty.pid, err);
                } else {
                    info!("Dropping: SIGKILL sent to process {}.", pty.pid);
                }

                if unsafe { libc::close(pty.master_fd) } != 0 {
                    let err = io::Error::last_os_error();
                    error!("Dropping: Failed to close master_fd {}: {}", pty.master_fd, err);
                } else {
                    info!("Dropping: Master_fd {} closed successfully.", pty.master_fd);
                }

                if unsafe { libc::close(pty.slave_fd) } != 0 {
                    let err = io::Error::last_os_error();
                    error!("Dropping: Failed to close slave_fd {}: {}", pty.slave_fd, err);
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
