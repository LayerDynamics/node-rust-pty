use napi::bindgen_prelude::*;
use napi_derive::napi;
use tokio::task;
use std::sync::{Arc, Once};
use log::{info, warn, error};
use std::io;
use std::time::Duration;
use tokio::time::{timeout, error::Elapsed};
use parking_lot::Mutex;
use bytes::Bytes;
use crossbeam_channel::{bounded, Sender, Receiver};
use std::cell::RefCell;

#[cfg(target_os = "linux")]
use libc::TIOCSWINSZ;

#[cfg(target_os = "macos")]
const TIOCSWINSZ: u64 = 0x40087467;

thread_local! {
    static READ_BUFFER: RefCell<[u8; 1024]> = RefCell::new([0u8; 1024]);
}

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
    sender: Sender<PtyCommand>,
    receiver: Receiver<PtyResult>,
    write_buffer: Vec<u8>,
    last_write: std::time::Instant,
}

enum PtyCommand {
    Write(Bytes),
    Resize(u16, u16),
}

enum PtyResult {
    Success,
    Failure(String),
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
    pub fn new() -> Self {
        initialize_logging();

        let (sender, receiver) = bounded(100);
        Self {
            pty: Arc::new(Mutex::new(None)),
            sender,
            receiver,
            write_buffer: Vec::new(),
            last_write: std::time::Instant::now(),
        }
    }

    #[napi]
    pub async fn read(&self) -> Result<String> {
        let weak_binding = Arc::downgrade(&self.pty);
        info!("Initiating read from PTY.");
        let read_timeout = Duration::from_secs(5);

        let result = timeout(read_timeout, task::spawn_blocking(move || -> Result<String> {
            let binding = weak_binding.upgrade().unwrap(); // Upgrade the weak binding
            let pty_guard = binding.lock(); // Now we lock the binding
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
            })
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
        let weak_binding = Arc::downgrade(&self.pty);
        let data_bytes = Bytes::from(data.into_bytes());
        info!("Initiating write to PTY.");
        let write_timeout = Duration::from_secs(5);

        let _result = timeout(write_timeout, task::spawn_blocking(move || -> Result<()> {
            let binding = weak_binding.upgrade().unwrap(); // Upgrade the weak binding
            let pty_guard = binding.lock(); // Now we lock the binding
            let master_fd_opt = pty_guard
                .as_ref()
                .map(|pty| pty.master_fd)
                .ok_or_else(|| {
                    error!("PTY not initialized");
                    Error::new(Status::GenericFailure, "PTY not initialized")
                })?;

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
        let weak_binding = Arc::downgrade(&self.pty);
        info!("Initiating resize of PTY to cols: {}, rows: {}", cols, rows);
        let resize_timeout = Duration::from_secs(5);

        let _result = timeout(resize_timeout, task::spawn_blocking(move || -> Result<()> {
            let binding = weak_binding.upgrade().unwrap(); // Upgrade the weak binding
            let pty_guard = binding.lock(); // Now we lock the binding
            let mut winsize: libc::winsize = unsafe { std::mem::zeroed() };
            winsize.ws_col = cols;
            winsize.ws_row = rows;
            winsize.ws_xpixel = 0;
            winsize.ws_ypixel = 0;

            let master_fd_opt = pty_guard
                .as_ref()
                .map(|pty| pty.master_fd)
                .ok_or_else(|| {
                    error!("PTY not initialized");
                    Error::new(Status::GenericFailure, "PTY not initialized")
                })?;

            let ret = unsafe { libc::ioctl(master_fd_opt, TIOCSWINSZ, &winsize) };
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
        let weak_binding = Arc::downgrade(&self.pty);
        info!("Initiating graceful shutdown of PTY.");
        let graceful_timeout = Duration::from_secs(10);
        let force_timeout = Duration::from_secs(5);

        let close_result = timeout(graceful_timeout, task::spawn_blocking(move || -> std::result::Result<(), String> {
            let binding = weak_binding.upgrade().unwrap(); // Upgrade the weak binding
            let mut pty_option = binding.lock(); // Now we lock the binding

            if let Some(pty) = pty_option.take() {
                info!("Sending SIGTERM to child process {}.", pty.pid);
                let kill_ret = unsafe { libc::kill(pty.pid, libc::SIGTERM) };

                if kill_ret != 0 {
                    let err = io::Error::last_os_error();
                    error!("Failed to send SIGTERM to process {}: {}", pty.pid, err);
                } else {
                    info!("SIGTERM sent successfully.");
                    let wait_ret = unsafe { libc::waitpid(pty.pid, std::ptr::null_mut(), libc::WNOHANG) };
                    if wait_ret == 0 {
                        error!("Process {} did not terminate immediately after SIGTERM.", pty.pid);
                    } else if wait_ret == -1 {
                        let err = io::Error::last_os_error();
                        error!("Error waiting for process {}: {}", pty.pid, err);
                    } else {
                        info!("Process {} terminated gracefully.", pty.pid);
                    }
                }

                // Close file descriptors
                unsafe {
                    libc::close(pty.master_fd);
                    libc::close(pty.slave_fd);
                }
                info!("Closed master and slave file descriptors.");

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
                Ok(())
            }
            Ok(Err(e)) => {
                error!("Error during graceful shutdown: {}", e);
                Err(Error::new(Status::GenericFailure, e))
            }
            Err(e) => {
                warn!("Graceful shutdown timed out. Initiating force kill. Error: {}", e);
                self.force_kill(force_timeout).await
            }
        }
    }

    async fn force_kill(&self, force_timeout: Duration) -> Result<()> {
        let weak_binding = Arc::downgrade(&self.pty);

        let force_kill_result = timeout(force_timeout, task::spawn_blocking(move || -> std::result::Result<(), String> {
            let binding = weak_binding.upgrade().unwrap(); // Upgrade the weak binding
            let mut pty_option = binding.lock(); // Now we lock the binding

            if let Some(pty) = pty_option.take() {
                info!("Sending SIGKILL to child process {}.", pty.pid);
                let kill_ret = unsafe { libc::kill(pty.pid, libc::SIGKILL) };

                if kill_ret != 0 {
                    let err = io::Error::last_os_error();
                    error!("Failed to send SIGKILL to process {}: {}", pty.pid, err);
                } else {
                    info!("SIGKILL sent successfully to process {}.", pty.pid);
                    unsafe { libc::waitpid(pty.pid, std::ptr::null_mut(), 0) };
                }

                // Close file descriptors
                unsafe {
                    libc::close(pty.master_fd);
                    libc::close(pty.slave_fd);
                }
                info!("Closed master and slave file descriptors.");

                Ok(())
            } else {
                warn!("PTY not initialized or already closed.");
                Ok(())
            }
        }))
        .await;

        match force_kill_result {
            Ok(Ok(_)) => {
                info!("PTY forcefully terminated.");
                Ok(())
            }
            Ok(Err(e)) => {
                error!("Error during force kill: {}", e);
                Err(Error::new(Status::GenericFailure, e))
            }
            Err(e) => {
                error!("Force kill operation failed: {}", e);
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
        let weak_binding = Arc::downgrade(&self.pty);
        info!("Dropping PtyHandle, initiating cleanup.");

        task::block_in_place(|| {
            let binding = weak_binding.upgrade().unwrap(); // Upgrade the weak binding
            let mut pty_option = binding.lock(); // Now we lock the binding

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
                    info!("Dropping: Master_fd closed successfully.");
                }

                if unsafe { libc::close(pty.slave_fd) } != 0 {
                    let err = io::Error::last_os_error();
                    error!("Dropping: Failed to close slave_fd {}: {}", pty.slave_fd, err);
                } else {
                    info!("Dropping: Slave_fd closed successfully.");
                }

                info!("Dropping: PTY session closed via Drop.");
            } else {
                warn!("Dropping: PTY not initialized or already closed.");
            }
        });
    }
}