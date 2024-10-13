// src/lib.rs

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

// Platform-specific modules
#[cfg(target_os = "linux")]
mod platform {
    #![allow(unused_imports)]
    use super::*;
    use libc::{
        TIOCSWINSZ, fork, setsid, execle, dup2, close, read, write, ioctl, grantpt,
        unlockpt, posix_openpt, ptsname, open, kill, waitpid, _exit, O_RDWR, O_NOCTTY, WNOHANG, SIGTERM, SIGKILL, winsize,
    };

    pub type PidT = i32;

    #[derive(Debug)]
    pub struct PtyProcess {
        pub master_fd: i32,
        pub pid: PidT,
    }

    impl PtyProcess {
        pub fn new() -> io::Result<Self> {
            debug!("Creating new PtyProcess on Linux");

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
                debug!("In child process on Linux");
                // Step 7a: Create a new session
                if unsafe { setsid() } < 0 {
                    error!("setsid failed");
                    unsafe { _exit(1) };
                }

                // Step 7b: Set slave PTY as controlling terminal
                let mut ws: winsize = unsafe { std::mem::zeroed() };
                ws.ws_row = 24;
                ws.ws_col = 80;
                ws.ws_xpixel = 0;
                ws.ws_ypixel = 0;

                if unsafe { ioctl(slave_fd, TIOCSWINSZ, &ws) } < 0 {
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
                    execle(
                        shell.as_ptr(),
                        shell_arg.as_ptr(),
                        ptr::null::<*const libc::c_char>(),
                        ptr::null::<*const libc::c_char>(),
                    );
                    // If execle fails
                    error!("execle failed");
                    _exit(1);
                }
            } else {
                // Parent process
                debug!("In parent process on Linux, child PID: {}", pid);
                // Close slave_fd in parent
                unsafe { close(slave_fd) };
                // Return master_fd and child PID
                Ok(PtyProcess { master_fd, pid })
            }
        }

        pub fn write_data(&self, data: &[u8]) -> io::Result<usize> {
            unsafe {
                write(
                    self.master_fd,
                    data.as_ptr() as *const libc::c_void,
                    data.len(),
                )
            }.try_into().map_err(|_| io::Error::new(io::ErrorKind::Other, "Failed to convert bytes_written to usize"))
        }

        pub fn read_data(&self, buffer: &mut [u8]) -> io::Result<usize> {
            unsafe {
                read(
                    self.master_fd,
                    buffer.as_mut_ptr() as *mut libc::c_void,
                    buffer.len(),
                )
            }.try_into().map_err(|_| io::Error::new(io::ErrorKind::Other, "Failed to convert bytes_read to usize"))
        }

        pub fn resize(&self, cols: u16, rows: u16) -> io::Result<()> {
            let mut ws: winsize = unsafe { std::mem::zeroed() };
            ws.ws_col = cols;
            ws.ws_row = rows;

            let ret = unsafe { ioctl(self.master_fd, TIOCSWINSZ, &ws) };
            if ret != 0 {
                return Err(io::Error::last_os_error());
            }
            Ok(())
        }

        pub fn kill_process(&self, signal: i32) -> io::Result<()> {
            let ret = unsafe { kill(self.pid, signal) };
            if ret != 0 {
                return Err(io::Error::last_os_error());
            }
            Ok(())
        }

        pub fn waitpid(&self, options: i32) -> io::Result<i32> {
            let pid = unsafe { waitpid(self.pid, ptr::null_mut(), options) };
            if pid == -1 {
                return Err(io::Error::last_os_error());
            }
            Ok(pid)
        }

        pub fn close_master_fd(&self) -> io::Result<()> {
            let ret = unsafe { close(self.master_fd) };
            if ret != 0 {
                return Err(io::Error::last_os_error());
            }
            Ok(())
        }
    }

    #[cfg(target_os = "macos")]
    mod platform {
        #![allow(unused_imports)]
        use super::*;
        use libc::{openpty, fork, setsid, execle, dup2, close, read, write, ioctl, kill, waitpid, _exit, SIGTERM, SIGKILL, WNOHANG, winsize, TIOCSWINSZ};
        use libc::{O_RDWR, O_NOCTTY};

        pub type PidT = i32;
        
        #[derive(Debug)]
        pub struct PtyProcess {
            pub master_fd: i32,
            pub pid: PidT,
        }
        
        impl PtyProcess {
            pub fn new() -> io::Result<Self> {
                debug!("Creating new PtyProcess on macOS");

                // Step 1: Open PTY master and slave using openpty
                let mut master_fd: i32 = 0;
                let mut slave_fd: i32 = 0;
                let mut ws: winsize = unsafe { std::mem::zeroed() };
                ws.ws_row = 24;
                ws.ws_col = 80;
                ws.ws_xpixel = 0;
                ws.ws_ypixel = 0;

                let ret = unsafe { openpty(&mut master_fd, &mut slave_fd, ptr::null_mut(), ptr::null_mut(), &mut ws) };
                if ret != 0 {
                    return Err(io::Error::last_os_error());
                }
                debug!("Opened PTY master_fd: {}, slave_fd: {}", master_fd, slave_fd);

                // Step 2: Fork the process
                let pid = unsafe { fork() };
                if pid < 0 {
                    // Fork failed
                    unsafe { close(master_fd) };
                    unsafe { close(slave_fd) };
                    return Err(io::Error::last_os_error());
                } else if pid == 0 {
                    // Child process
                    debug!("In child process on macOS");
                    // Step 3a: Create a new session
                    if unsafe { setsid() } < 0 {
                        error!("setsid failed");
                        unsafe { _exit(1) };
                    }

                    // Step 3b: Set slave PTY as controlling terminal
                    if unsafe { ioctl(slave_fd, TIOCSWINSZ, &ws) } < 0 {
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
                        execle(
                            shell.as_ptr(),
                            shell_arg.as_ptr(),
                            ptr::null::<*const libc::c_char>(),
                            ptr::null::<*const libc::c_char>(),
                        );
                        // If execle fails
                        error!("execle failed");
                        _exit(1);
                    }
                } else {
                    // Parent process
                    debug!("In parent process on macOS, child PID: {}", pid);
                    // Close slave_fd in parent
                    unsafe { close(slave_fd) };
                    // Return master_fd and child PID
                    Ok(PtyProcess { master_fd, pid })
                }
            }

            pub fn write_data(&self, data: &[u8]) -> io::Result<usize> {
                unsafe {
                    write(
                        self.master_fd,
                        data.as_ptr() as *const libc::c_void,
                        data.len(),
                    )
                }.try_into().map_err(|_| io::Error::new(io::ErrorKind::Other, "Failed to convert bytes_written to usize"))
            }

            pub fn read_data(&self, buffer: &mut [u8]) -> io::Result<usize> {
                unsafe {
                    read(
                        self.master_fd,
                        buffer.as_mut_ptr() as *mut libc::c_void,
                        buffer.len(),
                    )
                }.try_into().map_err(|_| io::Error::new(io::ErrorKind::Other, "Failed to convert bytes_read to usize"))
            }

            pub fn resize(&self, cols: u16, rows: u16) -> io::Result<()> {
                let mut ws: winsize = unsafe { std::mem::zeroed() };
                ws.ws_col = cols;
                ws.ws_row = rows;

                let ret = unsafe { ioctl(self.master_fd, TIOCSWINSZ, &ws) };
                if ret != 0 {
                    return Err(io::Error::last_os_error());
                }
                Ok(())
            }

            pub fn kill_process(&self, signal: i32) -> io::Result<()> {
                let ret = unsafe { kill(self.pid, signal) };
                if ret != 0 {
                    return Err(io::Error::last_os_error());
                }
                Ok(())
            }

            pub fn waitpid(&self, options: i32) -> io::Result<i32> {
                let pid = unsafe { waitpid(self.pid, ptr::null_mut(), options) };
                if pid == -1 {
                    return Err(io::Error::last_os_error());
                }
                Ok(pid)
            }

            pub fn close_master_fd(&self) -> io::Result<()> {
                let ret = unsafe { close(self.master_fd) };
                if ret != 0 {
                    return Err(io::Error::last_os_error());
                }
                Ok(())
            }
        }
    }

    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    mod platform {
        use super::*;
        use std::io;

        pub type PidT = i32;

        #[derive(Debug)]
        pub struct PtyProcess;

        impl PtyProcess {
            pub fn new() -> io::Result<Self> {
                Err(io::Error::new(io::ErrorKind::Other, "Unsupported platform"))
            }

            pub fn write_data(&self, _data: &[u8]) -> io::Result<usize> {
                Err(io::Error::new(io::ErrorKind::Other, "Unsupported platform"))
            }

            pub fn read_data(&self, _buffer: &mut [u8]) -> io::Result<usize> {
                Err(io::Error::new(io::ErrorKind::Other, "Unsupported platform"))
            }

            pub fn resize(&self, _cols: u16, _rows: u16) -> io::Result<()> {
                Err(io::Error::new(io::ErrorKind::Other, "Unsupported platform"))
            }

            pub fn kill_process(&self, _signal: i32) -> io::Result<()> {
                Err(io::Error::new(io::ErrorKind::Other, "Unsupported platform"))
            }

            pub fn waitpid(&self, _options: i32) -> io::Result<i32> {
                Err(io::Error::new(io::ErrorKind::Other, "Unsupported platform"))
            }

            pub fn close_master_fd(&self) -> io::Result<()> {
                Err(io::Error::new(io::ErrorKind::Other, "Unsupported platform"))
            }
        }
    }

    // Import the platform-specific PtyProcess
    use platform::PtyProcess;

    // Thread-local buffer for reading
    thread_local! {
        static READ_BUFFER: RefCell<[u8; 1024]> = RefCell::new([0u8; 1024]);
    }

    // Define Pid type
    type PidT = platform::PidT;

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

    #[napi]
    impl PtyHandle {
        /// Asynchronous factory method to create a new PtyHandle instance.
        #[napi(static_method)]
        pub async fn create() -> Result<Self> {
            initialize_logging();
            info!("Creating new PtyHandle");

            let creation_timeout = Duration::from_secs(60);
            let handle_future = task::spawn_blocking(|| {
                debug!("Initializing PtyProcess");
                let pty_process = PtyProcess::new().map_err(|e| {
                    Error::new(Status::GenericFailure, format!("Failed to initialize PtyProcess: {}", e))
                })?;
                debug!("PtyProcess initialized successfully");

                debug!("Setting up command channels");
                let (command_sender, command_receiver) = bounded(100);
                let (result_sender, result_receiver) = bounded(100);
                debug!("Command channels set up successfully");

                Ok::<PtyHandle, napi::Error>(PtyHandle {
                    pty: Arc::new(Mutex::new(Some(pty_process))),
                    command_sender,
                    command_receiver,
                    result_sender,
                    result_receiver,
                })
            });

            let handle_result = timeout(creation_timeout, handle_future).await;

            // Properly unwrap the Result before calling methods
            let pty_handle = match handle_result {
                Ok(inner_result) => inner_result?,
                Err(e) => {
                    return Err(Error::new(
                        Status::GenericFailure,
                        format!("PtyHandle creation timed out: {}", e),
                    ));
                }
            };

            pty_handle.handle_commands();
            pty_handle.listen_for_results();
            Ok(pty_handle)
        }

        /// Asynchronously reads data from the PTY.
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
                let pty_process = pty_guard.as_ref().ok_or_else(|| {
                    error!("PTY not initialized");
                    Error::new(Status::GenericFailure, "PTY not initialized")
                })?;

                READ_BUFFER.with(|buffer_cell| {
                    let mut buffer = buffer_cell.borrow_mut();
                    let bytes_read = pty_process.read_data(&mut buffer[..]).map_err(|e| {
                        error!("Failed to read from PTY: {}", e);
                        Error::new(
                            Status::GenericFailure,
                            format!("Failed to read from PTY: {}", e),
                        )
                    })?;

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
                Ok(inner_result) => inner_result?,
                Err(e) => {
                    error!("Read operation timed out: {}", e);
                    Err(Error::new(
                        Status::GenericFailure,
                        format!("Read operation timed out: {}", e),
                    ))
                }
            }
        }

        /// Asynchronously writes data to the PTY.
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

        /// Asynchronously resizes the PTY.
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

        /// Asynchronously closes the PTY gracefully.
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
                    if let Err(e) = pty.kill_process(libc::SIGTERM) {
                        let err = format!("Failed to send SIGTERM to process {}: {}", pty.pid, e);
                        error!("{}", err);
                    } else {
                        info!("SIGTERM sent successfully.");
                        pty.waitpid(libc::WNOHANG).map_err(|e| e.to_string())?;
                    }

                    if let Err(e) = pty.close_master_fd() {
                        error!("Failed to close master_fd {}: {}", pty.master_fd, e);
                    } else {
                        info!("Closed master_fd {}.", pty.master_fd);
                    }

                    Ok(())
                } else {
                    warn!("PTY not initialized or already closed.");
                    Ok(())
                }
            }))
            .await;

            match close_result {
                Ok(inner_result) => match inner_result {
                    Ok(_) => {
                        info!("PTY closed successfully.");
                        self.send_result(PtyResult::Success("PTY closed successfully".into()));
                        Ok(())
                    }
                    Err(e) => {
                        error!("Error during graceful shutdown: {}", e);
                        self.send_result(PtyResult::Failure(format!("Error during graceful shutdown: {}", e)));
                        Err(Error::new(Status::GenericFailure, e))
                    }
                },
                Err(e) => {
                    warn!("Graceful shutdown timed out. Initiating force kill. Error: {}", e);
                    self.send_result(PtyResult::Failure(format!("Graceful shutdown timed out: {}", e)));
                    self.force_kill(force_timeout).await
                }
            }
        }

        /// Handles sending results back through the result channel.
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

        /// Listens for commands and processes them.
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
                            let write_result = pty_process.write_data(&data).map_err(|e| e.to_string());
                            match write_result {
                                Ok(bytes_written) => {
                                    info!("Wrote {} bytes to PTY.", bytes_written);
                                }
                                Err(err) => {
                                    error!("Failed to write data to PTY: {}", err);
                                }
                            }
                        }
                        PtyCommand::Resize(cols, rows) => {
                            debug!("Executing resize command: cols={}, rows={}", cols, rows);
                            if let Err(e) = pty_process.resize(cols, rows) {
                                error!("Failed to resize PTY: {}", e);
                            } else {
                                info!("Successfully resized PTY to cols: {}, rows: {}", cols, rows);
                            }
                        }
                    }
                }
                warn!("Command receiver channel closed");
            });
        }

        /// Listens for results from the result channel.
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

        /// Forces the termination of the PTY process.
        async fn force_kill(&self, force_timeout: Duration) -> Result<()> {
            initialize_logging();
            let weak_binding = Arc::downgrade(&self.pty);

            let force_kill_result = timeout(force_timeout, task::spawn_blocking(move || -> std::result::Result<(), String> {
                let binding = weak_binding.upgrade().ok_or_else(|| "Failed to upgrade weak reference".to_string())?;
                let mut pty_option = binding.lock();

                if let Some(pty) = pty_option.take() {
                    info!("Sending SIGKILL to child process {}.", pty.pid);
                    if let Err(e) = pty.kill_process(libc::SIGKILL) {
                        let err = format!("Failed to send SIGKILL to process {}: {}", pty.pid, e);
                        error!("{}", err);
                        return Err(format!("Failed to send SIGKILL: {}", e));
                    } else {
                        info!("SIGKILL sent successfully to process {}.", pty.pid);
                        pty.waitpid(0).map(|_| ()).map_err(|e| e.to_string())?;
                        pty.close_master_fd().map_err(|e| e.to_string())?;
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
                Ok(inner_result) => match inner_result {
                    Ok(_) => {
                        info!("PTY forcefully terminated.");
                        self.send_result(PtyResult::Success("PTY forcefully terminated".into()));
                        Ok(())
                    }
                    Err(e) => {
                        error!("Error during force kill: {}", e);
                        self.send_result(PtyResult::Failure(format!("Error during force kill: {}", e)));
                        Err(Error::new(Status::GenericFailure, e))
                    }
                },
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
                        if let Err(e) = pty.kill_process(libc::SIGTERM) {
                            let err = format!("Dropping: Failed to send SIGTERM to process {}: {}", pty.pid, e);
                            error!("{}", err);
                        } else {
                            std::thread::sleep(Duration::from_secs(2));
                            match pty.waitpid(0) {
                                Ok(pid) => {
                                    if pid == 0 {
                                        error!("Dropping: Process {} did not terminate gracefully.", pty.pid);
                                    } else {
                                        info!("Dropping: Process {} terminated gracefully.", pty.pid);
                                    }
                                }
                                Err(e) => {
                                    let err = format!("Dropping: Process {} did not terminate gracefully: {}", pty.pid, e);
                                    error!("{}", err);
                                }
                            }
                        }

                        info!("Dropping: Sending SIGKILL to child process {} as a fallback.", pty.pid);
                        if let Err(e) = pty.kill_process(libc::SIGKILL) {
                            let err = format!("Dropping: Failed to send SIGKILL to process {}: {}", pty.pid, e);
                            error!("{}", err);
                        } else {
                            info!("Dropping: SIGKILL sent to process {}.", pty.pid);
                            let _ = pty.waitpid(0);
                        }

                        if pty.master_fd >= 0 {
                            if let Err(e) = pty.close_master_fd() {
                                let err = format!("Dropping: Failed to close master_fd {}: {}", pty.master_fd, e);
                                error!("{}", err);
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
        let pty_handle = PtyHandle::create().await?;

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
}
