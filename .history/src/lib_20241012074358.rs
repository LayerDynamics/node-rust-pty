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
    command_sender: Sender<PtyCommand>,
    command_receiver: Receiver<PtyCommand>,
    result_sender: Sender<PtyResult>,
    result_receiver: Receiver<PtyResult>,
}

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
                            libc::write(pty_process.master_fd, data.as_ptr() as *const libc::c_void, data.len()) 
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
                        winsize.ws_xpixel = 0;
                        winsize.ws_ypixel = 0;

                        let ret = unsafe { libc::ioctl(pty_process.master_fd, TIOCSWINSZ, &winsize) };
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

        let (command_sender, command_receiver) = bounded(100);
        let (result_sender, result_receiver) = bounded(100);

        let handle = PtyHandle {
            pty: Arc::new(Mutex::new(None)),
            command_sender,
            command_receiver,
            result_sender,
            result_receiver,
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

        let result = timeout(read_timeout, task::spawn_blocking(move || -> Result<String> {
            let binding = weak_binding.upgrade().ok_or_else(|| Error::new(Status::GenericFailure, "Failed to upgrade weak reference"))?;
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

        self.send_result(PtyResult::Success("Read operation successful".into()));

        Ok(result?)
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
                let kill_ret = unsafe { libc::kill(pty.pid, libc::SIGKILL) };

                if kill_ret != 0 {
                    let err = io::Error::last_os_error();
                    error!("Failed to send SIGKILL to process {}: {}", pty.pid, err);
                } else {
                    info!("SIGKILL sent successfully to process {}.", pty.pid);
                    unsafe { libc::waitpid(pty.pid, std::ptr::null_mut(), 0) };
                }

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
                self.send_result(PtyResult::Success("PTY forcefully terminated".into()));
                Ok(())
            }
            Ok(Err(e)) => {
                error!("Error during force kill: {}", e);
                self.send_result(PtyResult::Failure(format!("Error during force kill: {}", e)));
                Err(Error::new(Status::GenericFailure, e))
            }
            Err(e) => {
                error!("Force kill operation failed: {}", e);
                self.send_result(PtyResult::Failure(format!("Force kill operation failed: {}", e)));
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
                        let _ = unsafe { libc::waitpid(pty.pid, std::ptr::null_mut(), 0) };
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
            } else {
                warn!("Dropping: Failed to upgrade weak reference to PTY.");
            }
        });
    }
}

#[napi]
pub async fn test_pty_handle_creation() -> Result<bool> {
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