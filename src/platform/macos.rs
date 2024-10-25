// src/platform/macos.rs

use bytes::Bytes;
use libc::{
  _exit, close, dup2, execle, fork, ioctl, kill, openpty, read, setsid, tcgetattr, termios,
  waitpid, winsize, write, SIGKILL, SIGTERM, SIGWINCH, TIOCSWINSZ, VMIN, VTIME,
};
use log::{debug, error, info};
use napi::{
  bindgen_prelude::Buffer, Env, Error as NapiError, JsNumber, JsObject, JsString,
  Result as NapiResult,
};
use napi_derive::napi;
use nix::fcntl::{fcntl, FcntlArg, OFlag};
use nix::sys::select::{select, FdSet};
use nix::sys::signal::{self, Signal};
use nix::sys::time::{TimeVal, TimeValLike};
use nix::unistd::Pid;
use std::env;
use std::ffi::{CStr, CString};
use std::io;
use std::mem::MaybeUninit;
use std::os::unix::io::BorrowedFd;
use std::ptr;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::task;

extern "C" {
  pub static environ: *const *const libc::c_char;
}

/// Type alias for Process ID
pub type PidT = i32;

/// Represents a PTY process on macOS.
#[derive(Debug, Clone)]
pub struct PtyProcess {
  pub master_fd: i32,
  pub pid: PidT,
  pub multiplexer: Arc<Multiplexer>, // Assuming a Multiplexer type exists
  pub command: String,               // Added missing `command` field
}

/// Implementations for PtyProcess
impl PtyProcess {
  /// Creates a new PTY process on macOS.
  ///
  /// This function forks the current process. The child process sets up the PTY
  /// and executes the shell, while the parent process retains the master file descriptor.
  ///
  /// # Errors
  ///
  /// Returns an `io::Error` if the PTY cannot be opened or if the fork fails.
  pub fn new() -> io::Result<Self> {
    debug!("Creating new PtyProcess on macOS");

    let (master_fd, slave_fd) = Self::open_pty()?;

    let pid = unsafe { fork() };
    match pid {
      -1 => {
        // Fork failed
        error!("Fork failed: {}", io::Error::last_os_error());
        Self::cleanup_fd(master_fd, slave_fd);
        return Err(io::Error::last_os_error());
      }
      0 => {
        // Child process
        Self::setup_child(slave_fd, "/bin/bash").unwrap_or_else(|e| {
          error!("Failed to setup child: {}", e);
          std::process::exit(1);
        });
        unreachable!(); // Ensures the child process does not continue
      }
      _ => {
        // Parent process
        debug!("In parent process on macOS, child PID: {}", pid);
        // Close slave_fd in parent
        unsafe { close(slave_fd) };
        return Ok(PtyProcess {
          master_fd,
          pid,
          multiplexer: Arc::new(Multiplexer::new()), // Initialize as needed
          command: "/bin/bash".to_string(),          // Initialize `command`
        });
      }
    }
  }

  /// Opens a PTY (master and slave) on macOS.
  ///
  /// # Returns
  ///
  /// A tuple containing the master and slave file descriptors.
  ///
  /// # Errors
  ///
  /// Returns an `io::Error` if the PTY cannot be opened.
  fn open_pty() -> io::Result<(i32, i32)> {
    let mut master_fd: libc::c_int = -1;
    let mut slave_fd: libc::c_int = -1;

    // Initialize terminal attributes
    let mut termp: termios = unsafe { std::mem::zeroed() };
    unsafe {
        // Get base terminal attributes from stdin if possible
        if libc::tcgetattr(0, &mut termp) != 0 {
            // If that fails, set some sensible defaults
            termp.c_iflag = libc::ICRNL;
            termp.c_oflag = libc::OPOST | libc::ONLCR;
            termp.c_lflag = libc::ISIG | libc::ICANON | libc::ECHO |
                           libc::ECHOE | libc::ECHOK | libc::IEXTEN;
        }

        // Set input/output speed
        libc::cfsetispeed(&mut termp, libc::B38400);
        libc::cfsetospeed(&mut termp, libc::B38400);

        // Set control characters
        termp.c_cc[VMIN] = 1;
        termp.c_cc[VTIME] = 0;
    }

    // Initialize with larger default window size
    let mut ws = winsize {
        ws_row: 40,  // Start with our desired size
        ws_col: 100, // Start with our desired size
        ws_xpixel: 0,
        ws_ypixel: 0,
    };

    // Open the PTY
    let ret = unsafe {
        openpty(
            &mut master_fd,
            &mut slave_fd,
            std::ptr::null_mut(),
            &mut termp,
            &mut ws,
        )
    };

    if ret != 0 {
        let err = io::Error::last_os_error();
        error!("openpty failed with error: {}", err);
        return Err(err);
    }

    // Double-check the window size was set
    unsafe {
        // Set size on both master and slave
        if ioctl(master_fd, TIOCSWINSZ, &ws) != 0 {
            error!("Failed to set master PTY size");
            return Err(io::Error::last_os_error());
        }
        if ioctl(slave_fd, TIOCSWINSZ, &ws) != 0 {
            error!("Failed to set slave PTY size");
            return Err(io::Error::last_os_error());
        }
    }

    // Set both FDs to non-blocking mode
    unsafe {
        let flags = libc::fcntl(master_fd, libc::F_GETFL);
        if flags < 0 || libc::fcntl(master_fd, libc::F_SETFL, flags | libc::O_NONBLOCK) < 0 {
            return Err(io::Error::last_os_error());
        }
    }

    Ok((master_fd, slave_fd))
	}
  /// Sends data to the PTY process.
  ///
  /// This function spawns a blocking task to send data to the PTY using the multiplexer.
  /// It handles potential errors from the task and from the multiplexer.
  ///
  /// # Arguments
  ///
  /// * `multiplexer` - An `Arc` reference to the `Multiplexer`.
  /// * `data` - A byte slice containing the data to send to the PTY.
  ///
  /// # Returns
  ///
  /// Returns `Ok(())` if the data was successfully sent, or an `Err` containing a `napi::Error`.
  pub async fn send_to_pty(multiplexer: Arc<Multiplexer>, data: Buffer) -> napi::Result<()> {
    let data_clone = data.to_vec();
    let buffer = Buffer::from(data_clone);

    // Spawn a blocking task to send data to the PTY
    let send_future = task::spawn_blocking(move || {
      // Assuming session_id 0 is reserved for global commands.
      multiplexer.send_to_session(0, buffer)
    });

    // Await the task and handle potential JoinError
    let send_result = send_future
      .await
      .map_err(|e| map_to_napi_error(format!("Task Join Error: {}", e)))?; // Handle JoinError

    // Handle the Result from send_to_session
    send_result.map_err(map_to_napi_error)?; // Handle napi::Error

    Ok(())
  }

  /// Sends data to the PTY process from JavaScript.
  ///
  /// # Arguments
  ///
  /// * `env` - The N-API environment.
  /// * `multiplexer` - The JavaScript object representing the multiplexer.
  /// * `data` - The data buffer to send.
  ///
  /// # Returns
  ///
  /// Returns `Ok(())` if the data was successfully sent, or an `Err` containing a `napi::Error`.
  pub async fn send_to_pty_js(env: Env, multiplexer: JsObject, data: Buffer) -> napi::Result<()> {
    let multiplexer: Arc<Multiplexer> =
      Arc::new(env.unwrap::<&mut Multiplexer>(&multiplexer)?.clone());
    PtyProcess::send_to_pty(multiplexer, data).await
  }

  /// Constructs a `PtyProcess` instance from a JavaScript object.
  ///
  /// # Arguments
  ///
  /// * `js_obj` - The JavaScript object containing the process information.
  ///
  /// # Returns
  ///
  /// A `PtyProcess` instance.
  ///
  /// # Errors
  ///
  /// Returns a `NapiError` if the object does not contain the required fields or if field extraction fails.
  pub fn from_js_object(js_obj: &JsObject) -> NapiResult<Self> {
    let pid_js = js_obj.get::<&str, JsNumber>("pid")?;
    let pid = pid_js
      .ok_or_else(|| NapiError::from_reason("Missing 'pid' field"))?
      .get_int32()?;

    let master_fd_js = js_obj.get::<&str, JsNumber>("master_fd")?;
    let master_fd = master_fd_js
      .ok_or_else(|| NapiError::from_reason("Missing 'master_fd' field"))?
      .get_int32()?;

    let command_js = js_obj.get::<&str, JsString>("command")?;
    let command = command_js
      .ok_or_else(|| NapiError::from_reason("Missing 'command' field"))?
      .into_utf8()?
      .into_owned()?;

    // Add extraction for other fields if necessary

    Ok(PtyProcess {
      pid,
      master_fd,
      multiplexer: Arc::new(Multiplexer::new()), // Initialize as needed
      command,
    })
  }

  /// Converts the `PtyProcess` instance into a JavaScript object.
  ///
  /// # Arguments
  ///
  /// * `env` - A reference to the N-API environment.
  ///
  /// # Returns
  ///
  /// A `JsObject` representing the `PtyProcess`.
  ///
  /// # Errors
  ///
  /// Returns a `NapiError` if the object creation or field setting fails.
  pub fn into_js_object(&self, env: &Env) -> NapiResult<JsObject> {
    let mut js_obj = env.create_object()?;
    js_obj.set("pid", self.pid)?;
    js_obj.set("master_fd", self.master_fd)?;
    js_obj.set("command", self.command.clone())?; // Set the `command` field
    js_obj.set("multiplexer", self.get_multiplexer_js(env)?)?; // Set the `multiplexer` field
                                                               // Set other fields as necessary
    Ok(js_obj)
  }

  /// Retrieves the multiplexer as a JavaScript object.
  ///
  /// # Arguments
  ///
  /// * `env` - A reference to the N-API environment.
  ///
  /// # Returns
  ///
  /// A `JsObject` representing the `Multiplexer`.
  ///
  /// # Errors
  ///
  /// Returns a `NapiError` if the conversion fails.
  pub fn get_multiplexer_js(&self, env: &Env) -> NapiResult<JsObject> {
    self.multiplexer.clone().into_js_object(env)
  }

  /// Sets up the child process after a successful fork.
  ///
  /// This includes creating a new session, setting the slave PTY as the controlling terminal,
  /// duplicating file descriptors, and executing the shell.
  ///
  /// # Arguments
  ///
  /// * `slave_fd` - The file descriptor for the slave side of the PTY.
  /// * `shell_path` - The path to the shell executable to execute.
  ///
  /// # Returns
  ///
  /// An `io::Result` indicating success or failure.
  ///
  /// # Errors
  ///
  /// Returns an `io::Error` if any setup step fails.
  fn setup_child(slave_fd: i32, shell_path: &str) -> io::Result<()> {
    debug!("In child process on macOS");

    // Create a new session
    if unsafe { setsid() } < 0 {
      error!("setsid failed");
      unsafe { _exit(1) };
    }

    // Set slave PTY as controlling terminal
    let mut ws: winsize = unsafe { std::mem::zeroed() };
    ws.ws_row = 24;
    ws.ws_col = 80;

    if unsafe { ioctl(slave_fd, TIOCSWINSZ, &ws) } < 0 {
      error!("ioctl TIOCSWINSZ failed");
      unsafe { _exit(1) };
    }

    Self::duplicate_fds(slave_fd)?;

    // Close unused file descriptors
    unsafe {
      close(slave_fd);
    }

    // Execute shell with proper environment and in interactive mode
    let shell = CString::new(shell_path).unwrap();
    let shell_arg = CString::new(
      std::path::Path::new(shell_path)
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("shell"),
    )
    .unwrap();
    let option_i = CString::new("-i").unwrap(); // Add interactive flag
    unsafe {
      execle(
        shell.as_ptr(),
        shell_arg.as_ptr(),
        option_i.as_ptr(),                  // Pass the -i flag
        ptr::null::<*const libc::c_char>(), // NULL terminates argv
        environ,                            // Pass the environment
      );
      // If execle fails
      error!("execle failed");
      _exit(1);
    }
  }

  /// Duplicates file descriptors for stdin, stdout, and stderr.
  ///
  /// # Arguments
  ///
  /// * `slave_fd` - The file descriptor to duplicate.
  ///
  /// # Returns
  ///
  /// An `io::Result` indicating success or failure.
  ///
  /// # Errors
  ///
  /// Returns an `io::Error` if duplicating any file descriptor fails.
  fn duplicate_fds(slave_fd: i32) -> io::Result<()> {
    if unsafe { dup2(slave_fd, libc::STDIN_FILENO) } < 0 {
      error!("dup2 failed for STDIN");
      unsafe { _exit(1) };
    }

    if unsafe { dup2(slave_fd, libc::STDOUT_FILENO) } < 0 {
      error!("dup2 failed for STDOUT");
      unsafe { _exit(1) };
    }

    if unsafe { dup2(slave_fd, libc::STDERR_FILENO) } < 0 {
      error!("dup2 failed for STDERR");
      unsafe { _exit(1) };
    }

    Ok(())
  }

  /// Cleans up file descriptors in case of errors.
  ///
  /// # Arguments
  ///
  /// * `master_fd` - The master file descriptor to close.
  /// * `slave_fd` - The slave file descriptor to close.
  fn cleanup_fd(master_fd: i32, slave_fd: i32) {
    unsafe {
      close(master_fd);
      close(slave_fd);
    }
  }

  /// Writes data to the PTY.
  ///
  /// # Arguments
  ///
  /// * `data` - A reference to the data to write.
  ///
  /// # Returns
  ///
  /// The number of bytes written.
  ///
  /// # Errors
  ///
  /// Returns an `io::Error` if the write operation fails.
  pub fn write_data(&self, data: &Bytes) -> io::Result<usize> {
    debug!("Writing data to master_fd {}: {:?}", self.master_fd, data);
    let bytes_written = unsafe {
      write(
        self.master_fd,
        data.as_ptr() as *const libc::c_void,
        data.len(),
      )
    };

    if bytes_written < 0 {
      let err = io::Error::last_os_error();
      error!("Write failed: {}", err);
      Err(err)
    } else {
      debug!(
        "Wrote {} bytes to master_fd {}",
        bytes_written, self.master_fd
      );
      Ok(bytes_written as usize)
    }
  }

  /// Reads data from the PTY.
  ///
  /// # Arguments
  ///
  /// * `buffer` - A mutable slice to store the read data.
  ///
  /// # Returns
  ///
  /// The number of bytes read.
  ///
  /// # Errors
  ///
  /// Returns an `io::Error` if the read operation fails.
  pub fn read_data(&self, buffer: &mut [u8]) -> io::Result<usize> {
    debug!("Attempting to read data from master_fd {}", self.master_fd);
    let bytes_read = unsafe {
      read(
        self.master_fd,
        buffer.as_mut_ptr() as *mut libc::c_void,
        buffer.len(),
      )
    };

    if bytes_read < 0 {
      let err = io::Error::last_os_error();
      error!("Read failed: {}", err);
      Err(err)
    } else {
      debug!(
        "Read {} bytes from master_fd {}",
        bytes_read, self.master_fd
      );
      Ok(bytes_read as usize)
    }
  }

  /// Reads data from the PTY in a non-blocking manner with a timeout.
  ///
  /// # Arguments
  ///
  /// * `buffer` - A mutable slice to store the read data.
  /// * `timeout` - The duration to wait for data before timing out.
  ///
  /// # Returns
  ///
  /// Returns `Ok(Some(bytes_read))` if data was read, `Ok(None)` if timed out, or `Err` on error.
  pub fn read_data_non_blocking(
    &self,
    buffer: &mut [u8],
    timeout: Duration,
  ) -> io::Result<Option<usize>> {
    // Set the file descriptor to non-blocking
    let flags = fcntl(self.master_fd, FcntlArg::F_GETFL).map_err(|e| io::Error::from(e))?;
    let new_flags = OFlag::from_bits_truncate(flags) | OFlag::O_NONBLOCK;
    fcntl(self.master_fd, FcntlArg::F_SETFL(new_flags)).map_err(|e| io::Error::from(e))?;

    // Prepare for select
    let mut read_fds = FdSet::new();
    read_fds.insert(unsafe { BorrowedFd::borrow_raw(self.master_fd) });
    let timeout_secs = timeout.as_secs() as i64;
    let timeout_micros = timeout.subsec_micros() as i64;

    // Create a TimeVal instance using the from_secs and from_micros methods
    let mut timeval = TimeVal::seconds(timeout_secs) + TimeVal::microseconds(timeout_micros);

    let ready = select(
      self.master_fd + 1,
      &mut read_fds,
      None,
      None,
      Some(&mut timeval),
    )
    .map_err(|e| io::Error::from(e))?;

    // Restore the file descriptor flags
    let restored_flags = flags;
    fcntl(
      self.master_fd,
      FcntlArg::F_SETFL(OFlag::from_bits_truncate(restored_flags)),
    )
    .map_err(|e| io::Error::from(e))?;

    if ready > 0 && unsafe { read_fds.contains(BorrowedFd::borrow_raw(self.master_fd)) } {
      // Read available data
      let bytes_read = self.read_data(buffer)?;
      Ok(Some(bytes_read))
    } else {
      // No data available within timeout
      Ok(None)
    }
  }

  /// Resizes the PTY window.
  ///
  /// # Arguments
  ///
  /// * `cols` - The number of columns.
  /// * `rows` - The number of rows.
  ///
  /// # Returns
  ///
  /// An `io::Result` indicating success or failure.
  ///
  /// # Errors
  ///
  /// Returns an `io::Error` if the resize operation fails.
  pub fn resize(&self, cols: u16, rows: u16) -> io::Result<()> {
    debug!("Attempting to resize PTY to {}x{}", cols, rows);

    let ws = winsize {
        ws_row: rows,
        ws_col: cols,
        ws_xpixel: 0,
        ws_ypixel: 0,
    };

    // Set the window size on the master PTY
    let ret = unsafe { ioctl(self.master_fd, TIOCSWINSZ, &ws) };
    if ret != 0 {
        let err = io::Error::last_os_error();
        error!("Failed to set window size on master: {}", err);
        return Err(err);
    }

    // Send SIGWINCH to the child process
    let pid = Pid::from_raw(self.pid);
    signal::kill(pid, Signal::SIGWINCH).map_err(|e| {
        error!("Failed to send SIGWINCH: {}", e);
        io::Error::new(io::ErrorKind::Other, e)
    })?;

    // Wait for the change to take effect
    std::thread::sleep(std::time::Duration::from_millis(200));

    // Verify the size change
    let mut current_ws: winsize = unsafe { std::mem::zeroed() };
    let ret = unsafe { ioctl(self.master_fd, libc::TIOCGWINSZ, &mut current_ws) };
    if ret != 0 {
        let err = io::Error::last_os_error();
        error!("Failed to get current window size: {}", err);
        return Err(err);
    }

    if current_ws.ws_row != rows || current_ws.ws_col != cols {
        error!(
            "Window size mismatch after resize: got {}x{}, expected {}x{}",
            current_ws.ws_col, current_ws.ws_row, cols, rows
        );
        return Err(io::Error::new(
            io::ErrorKind::Other,
            "Failed to verify window size after resize",
        ));
    }

    debug!("Successfully resized PTY to {}x{}", cols, rows);
    Ok(())
	}

  /// Retrieves the current window size of the PTY.
  ///
  /// # Returns
  ///
  /// A tuple containing the number of rows and columns.
  ///
  /// # Errors
  ///
  /// Returns an `io::Error` if the ioctl call fails.
  pub fn get_window_size(&self) -> io::Result<(u16, u16)> {
    let mut ws = MaybeUninit::<winsize>::zeroed();
    let ret = unsafe { ioctl(self.master_fd, libc::TIOCGWINSZ, ws.as_mut_ptr()) };
    if ret != 0 {
      let err = io::Error::last_os_error();
      error!("Failed to get window size: {}", err);
      return Err(err);
    }
    let ws = unsafe { ws.assume_init() };
    Ok((ws.ws_row, ws.ws_col))
  }

  /// Sends a signal to the child process.
  ///
  /// # Arguments
  ///
  /// * `signal` - The signal number to send.
  ///
  /// # Returns
  ///
  /// An `io::Result` indicating success or failure.
  ///
  /// # Errors
  ///
  /// Returns an `io::Error` if the signal cannot be sent.
  pub fn kill_process(&self, signal: i32) -> io::Result<()> {
    let ret = unsafe { kill(self.pid, signal) };
    if ret != 0 {
      error!(
        "Failed to send signal to process {}: {}",
        self.pid,
        io::Error::last_os_error()
      );
      return Err(io::Error::last_os_error());
    }
    debug!(
      "Successfully sent signal {} to process {}",
      signal, self.pid
    );
    Ok(())
  }

  /// Waits for the child process to change state.
  ///
  /// # Arguments
  ///
  /// * `options` - Options for `waitpid`.
  ///
  /// # Returns
  ///
  /// The PID of the child process.
  ///
  /// # Errors
  ///
  /// Returns an `io::Error` if waiting fails.
  pub fn waitpid(&self, options: i32) -> io::Result<i32> {
    let mut status: i32 = 0;
    let pid = unsafe { waitpid(self.pid, &mut status, options) };
    if pid == -1 {
      error!(
        "Failed to wait for process {}: {}",
        self.pid,
        io::Error::last_os_error()
      );
      return Err(io::Error::last_os_error());
    }
    debug!("Successfully waited for process {}", self.pid);
    Ok(pid)
  }

  /// Closes the master file descriptor.
  ///
  /// # Returns
  ///
  /// An `io::Result` indicating success or failure.
  ///
  /// # Errors
  ///
  /// Returns an `io::Error` if closing fails.
  pub fn close_master_fd(&self) -> io::Result<()> {
    let ret = unsafe { close(self.master_fd) };
    if ret != 0 {
      error!(
        "Failed to close master_fd {}: {}",
        self.master_fd,
        io::Error::last_os_error()
      );
      return Err(io::Error::last_os_error());
    }
    debug!("Closed master_fd {}", self.master_fd);
    Ok(())
  }

  /// Forcefully kills the PTY process.
  ///
  /// # Returns
  ///
  /// An `io::Result` indicating success or failure.
  ///
  /// # Errors
  ///
  /// Returns an `io::Error` if the kill operation fails.
  pub fn force_kill(&self) -> io::Result<()> {
    info!("Forcefully killing PTY process {}.", self.pid);
    self.kill_process(SIGKILL)
  }

  /// Sets an environment variable for the PTY process.
  ///
  /// **Note:** Setting environment variables at runtime for the child PTY process is not straightforward.
  /// In macOS, it's not possible to directly modify the environment of a running process.
  /// As a workaround, this method sends an export command to the shell.
  ///
  /// # Arguments
  ///
  /// * `key` - The environment variable key.
  /// * `value` - The environment variable value.
  ///
  /// # Returns
  ///
  /// An `io::Result` indicating success or failure.
  ///
  /// # Errors
  ///
  /// Returns an `io::Error` if the PTY process is not running or if writing fails.
  pub fn set_env(&mut self, key: String, value: String) -> io::Result<()> {
    info!("Setting environment variable: {}={}", key, value);

    // Check if the PTY process is still running
    if self.is_running()? {
      // Setting environment variables at runtime for the child PTY process is not straightforward.
      // In macOS, it's not possible to directly modify the environment of a running process.
      // As a workaround, we send an export command to the shell.

      // First, set the environment variable for the current process (parent).
      env::set_var(&key, &value);

      // Send the export command to the PTY shell.
      let command = format!("export {}={}\n", key, value);
      self.write_data(&Bytes::copy_from_slice(command.as_bytes()))?;

      info!(
        "Successfully set environment variable in the PTY shell: {}={}",
        key, value
      );
      Ok(())
    } else {
      Err(io::Error::new(
        io::ErrorKind::NotConnected,
        "PTY process is not running",
      ))
    }
  }

  /// Changes the shell for the PTY process.
  ///
  /// # Arguments
  ///
  /// * `shell_path` - The path to the new shell executable.
  ///
  /// # Returns
  ///
  /// An `io::Result` indicating success or failure.
  ///
  /// # Errors
  ///
  /// Returns an `io::Error` if terminating the old shell or spawning the new shell fails.
  pub fn change_shell(&mut self, shell_path: String) -> io::Result<()> {
    info!("Changing shell to {}", shell_path);
    // Gracefully terminate the current shell
    self.kill_process(SIGTERM)?;

    // Wait for the process to terminate
    match self.waitpid(0) {
      Ok(_) => {
        debug!("Successfully terminated old shell process {}", self.pid);
      }
      Err(e) => {
        error!("Failed to waitpid after SIGTERM: {}", e);
        // Proceeding to attempt to spawn a new shell
      }
    }

    // Spawn a new shell
    self.spawn_new_shell(shell_path)?;
    Ok(())
  }

  /// Retrieves the status of the PTY process.
  ///
  /// # Returns
  ///
  /// A `String` indicating whether the process is "Running", "Not Running", or "Unknown".
  ///
  /// # Errors
  ///
  /// Returns an `io::Error` if checking the process status fails.
  pub fn status(&self) -> io::Result<String> {
    // Check if the process is still running
    match unsafe { kill(self.pid, 0) } {
      0 => Ok("Running".to_string()),
      -1 => {
        let err = io::Error::last_os_error();
        if err.raw_os_error() == Some(libc::ESRCH) {
          Ok("Not Running".to_string())
        } else {
          Err(err)
        }
      }
      _ => Ok("Unknown".to_string()),
    }
  }

  /// Sets the log level for the PTY process.
  ///
  /// # Arguments
  ///
  /// * `level` - The desired log level as a `String` (e.g., "error", "warn", "info", "debug", "trace").
  ///
  /// # Returns
  ///
  /// An `io::Result` indicating success or failure.
  ///
  /// # Errors
  ///
  /// Always returns `Ok(())` since setting the log level cannot fail in this context.
  pub fn set_log_level(&self, level: String) -> io::Result<()> {
    info!("Setting log level to {}", level);
    // Map string level to log::LevelFilter
    let level_filter = match level.to_lowercase().as_str() {
      "error" => log::LevelFilter::Error,
      "warn" => log::LevelFilter::Warn,
      "info" => log::LevelFilter::Info,
      "debug" => log::LevelFilter::Debug,
      "trace" => log::LevelFilter::Trace,
      _ => log::LevelFilter::Info, // Default level
    };
    log::set_max_level(level_filter);
    Ok(())
  }

  /// Shuts down the PTY process gracefully.
  ///
  /// Sends a SIGTERM signal to the PTY process and waits for it to terminate.
  /// If the process does not terminate gracefully, it forcefully kills it.
  ///
  /// # Returns
  ///
  /// An `io::Result` indicating success or failure.
  ///
  /// # Errors
  ///
  /// Returns an `io::Error` if terminating the process fails.
  pub fn shutdown_pty(&mut self) -> io::Result<()> {
    info!("Shutting down PTY process {}", self.pid);
    // Send SIGTERM to gracefully terminate the process
    self.kill_process(SIGTERM)?;
    // Wait for the process to terminate
    match self.waitpid(0) {
      Ok(_) => {
        info!("PTY process {} terminated", self.pid);
        // Close the master_fd
        self.close_master_fd()?;
        Ok(())
      }
      Err(e) => {
        error!("Failed to waitpid during shutdown: {}", e);
        // Attempt to force kill
        self.force_kill()?;
        self.close_master_fd()?;
        Err(e)
      }
    }
  }

  /// Retrieves the process ID (PID) of the PTY process.
  ///
  /// This function is exposed to JavaScript via N-API.
  ///
  /// # Returns
  ///
  /// Returns the PID of the PTY process.
  ///
  /// # Errors
  ///
  /// Returns a `napi::Error` if any error occurs.
  pub async fn pid(env: &Env, js_obj: &JsObject) -> napi::Result<i32> {
    let pty_process: &PtyProcess = env.unwrap::<PtyProcess>(js_obj)?;
    Ok(pty_process.pid)
  }

  /// Spawns a new shell process for the PTY.
  ///
  /// # Arguments
  ///
  /// * `shell_path` - The path to the new shell executable.
  ///
  /// # Returns
  ///
  /// An `io::Result` indicating success or failure.
  ///
  /// # Errors
  ///
  /// Returns an `io::Error` if spawning the new shell fails.
  fn spawn_new_shell(&mut self, shell_path: String) -> io::Result<()> {
    info!("Spawning new shell: {}", shell_path);
    // Close existing master_fd
    self.close_master_fd()?;

    // Open new PTY
    let (new_master_fd, new_slave_fd) = Self::open_pty()?;
    self.master_fd = new_master_fd;

    let pid = unsafe { fork() };
    match pid {
      -1 => {
        error!(
          "Fork failed while spawning new shell: {}",
          io::Error::last_os_error()
        );
        Self::cleanup_fd(new_master_fd, new_slave_fd);
        return Err(io::Error::last_os_error());
      }
      0 => {
        // Child process
        Self::setup_child(new_slave_fd, &shell_path).unwrap_or_else(|e| {
          error!("Failed to setup child during shell change: {}", e);
          std::process::exit(1);
        });
        unreachable!(); // Ensures the child process does not continue
      }
      _ => {
        // Parent process
        debug!("In parent process on macOS, new child PID: {}", pid);
        // Close slave_fd in parent
        unsafe { close(new_slave_fd) };
        self.pid = pid;
        self.command = shell_path.clone(); // Update the `command` field
        Ok(())
      }
    }
  }

  /// Checks if the PTY process is still running.
  ///
  /// # Returns
  ///
  /// `true` if the process is running, `false` otherwise.
  ///
  /// # Errors
  ///
  /// Returns an `io::Error` if checking the process status fails.
  pub fn is_running(&self) -> io::Result<bool> {
    match unsafe { kill(self.pid, 0) } {
      0 => Ok(true),
      -1 => {
        if io::Error::last_os_error().raw_os_error() == Some(libc::ESRCH) {
          Ok(false)
        } else {
          Err(io::Error::last_os_error())
        }
      }
      _ => Ok(false),
    }
  }
}

/// Maps a generic error into a `napi::Error`.
///
/// # Arguments
///
/// * `err` - The error to map.
///
/// # Returns
///
/// A `napi::Error` with the provided error message.
fn map_to_napi_error<E: std::fmt::Display>(err: E) -> NapiError {
  NapiError::from_reason(err.to_string())
}

#[cfg(test)]
mod tests {
  use super::*;
  use bytes::Bytes;
  use serial_test::serial;
  use std::io::{self};
  use std::sync::Once;
  use std::time::Duration;

  static INIT: Once = Once::new();

  fn init_logger() {
    INIT.call_once(|| {
      env_logger::builder().is_test(true).try_init().ok();
    });
  }

  #[test]
  #[serial]
  fn test_pty_process_creation() {
    init_logger();
    // Ensures that PtyProcess::new does not return an error under normal conditions.
    let result = PtyProcess::new();
    assert!(result.is_ok());
    let mut pty = result.unwrap();
    assert!(pty.master_fd > 0);
    assert!(pty.pid > 0);
    assert_eq!(pty.command, "/bin/bash".to_string());

    // Cleanup
    let _ = pty.shutdown_pty();
  }

  #[test]
  #[serial]
  fn test_write_and_read_data() {
    init_logger();
    let mut pty = PtyProcess::new().expect("Failed to create PtyProcess");

    let test_data = Bytes::from("echo Hello, PTY!\n");
    let write_result = pty.write_data(&test_data);
    assert!(write_result.is_ok());
    assert_eq!(write_result.unwrap(), test_data.len());

    let mut buffer = [0u8; 1024];
    let read_result = pty.read_data(&mut buffer);
    assert!(read_result.is_ok());
    let bytes_read = read_result.unwrap();
    assert!(bytes_read > 0);
    let output = String::from_utf8_lossy(&buffer[..bytes_read]);
    assert!(output.contains("Hello, PTY!"));

    // Cleanup
    let _ = pty.shutdown_pty();
  }

  #[test]
	#[serial]
	fn test_resize_pty() {
			init_logger();

			// Create new PTY with explicit size
			let pty = PtyProcess::new().expect("Failed to create PtyProcess");

			// Sleep briefly to let the PTY initialize
			std::thread::sleep(std::time::Duration::from_millis(100));

			// Get initial size
			let (initial_rows, initial_cols) = pty.get_window_size()
					.expect("Failed to get initial window size");
			debug!("Initial PTY size: {}x{}", initial_cols, initial_rows);

			// Attempt resize
			let resize_result = pty.resize(100, 40);
			if let Err(e) = &resize_result {
					error!("Resize failed: {}", e);
					// Get current size for debugging
					if let Ok((rows, cols)) = pty.get_window_size() {
							error!("Current size after failed resize: {}x{}", cols, rows);
					}
			}
			assert!(resize_result.is_ok(), "Resize operation failed");

			// Verify new size
			let (rows, cols) = pty.get_window_size()
					.expect("Failed to get window size after resize");
			assert_eq!(rows, 40, "Wrong number of rows after resize");
			assert_eq!(cols, 100, "Wrong number of columns after resize");

			// Additional verification using stty
			let mut buffer = [0u8; 1024];
			pty.write_data(&Bytes::from("stty size\n"))
					.expect("Failed to write stty command");

			// Read with timeout
			let start = std::time::Instant::now();
			let mut total_output = String::new();
			while start.elapsed() < std::time::Duration::from_secs(2) {
					if let Ok(bytes_read) = pty.read_data(&mut buffer) {
							let output = String::from_utf8_lossy(&buffer[..bytes_read]);
							total_output.push_str(&output);
							if total_output.contains("40 100") {
									return; // Success
							}
					}
					std::thread::sleep(std::time::Duration::from_millis(50));
			}

			panic!("Failed to verify terminal size using stty. Output: {}", total_output);
	}

  #[test]
  #[serial]
  fn test_set_env() {
    init_logger();
    let mut pty = PtyProcess::new().expect("Failed to create PtyProcess");

    let key = "TEST_ENV_VAR".to_string();
    let value = "12345".to_string();
    let set_env_result = pty.set_env(key.clone(), value.clone());
    assert!(set_env_result.is_ok());

    // Send a command to print the environment variable
    let test_command = Bytes::from(format!("echo ${}\n", key));
    let write_result = pty.write_data(&test_command);
    assert!(write_result.is_ok());

    let mut buffer = [0u8; 1024];
    let read_result = pty.read_data(&mut buffer);
    assert!(read_result.is_ok());
    let bytes_read = read_result.unwrap();
    let output = String::from_utf8_lossy(&buffer[..bytes_read]);
    assert!(output.contains(&value));

    // Cleanup
    let _ = pty.shutdown_pty();
  }

  #[test]
  #[serial]
  fn test_status() {
    init_logger();
    let mut pty = PtyProcess::new().expect("Failed to create PtyProcess");

    let status_result = pty.status();
    assert!(status_result.is_ok());
    let status = status_result.unwrap();
    assert_eq!(status, "Running");

    // Cleanup
    let _ = pty.shutdown_pty();

    // After shutdown, status should indicate the process is not running
    let status_after = pty.status();
    match status_after {
      Ok(status) => assert_eq!(status, "Not Running"),
      Err(e) => {
        // Ensure error is due to the process not existing
        assert_eq!(e.kind(), io::ErrorKind::Other);
      }
    }
  }

  #[test]
  #[serial]
  fn test_set_log_level() {
    init_logger();
    let mut pty = PtyProcess::new().expect("Failed to create PtyProcess");

    let levels = vec![
      "error".to_string(),
      "warn".to_string(),
      "info".to_string(),
      "debug".to_string(),
      "trace".to_string(),
      "invalid".to_string(),
    ];

    for level in levels {
      let set_level_result = pty.set_log_level(level.clone());
      assert!(set_level_result.is_ok());
    }

    // Cleanup
    let _ = pty.shutdown_pty();
  }

  #[test]
  #[serial]
  fn test_change_shell() {
    init_logger();
    let mut pty = PtyProcess::new().expect("Failed to create PtyProcess");

    // Change to /bin/sh
    let change_shell_result = pty.change_shell("/bin/sh".to_string());
    assert!(change_shell_result.is_ok());
    assert_eq!(pty.command, "/bin/sh".to_string());

    // Send a command to verify the shell has changed
    let test_command = Bytes::from("echo Shell Changed\n");
    let write_result = pty.write_data(&test_command);
    assert!(write_result.is_ok());

    let mut buffer = [0u8; 1024];
    let read_result = pty.read_data(&mut buffer);
    assert!(read_result.is_ok());
    let bytes_read = read_result.unwrap();
    let output = String::from_utf8_lossy(&buffer[..bytes_read]);
    assert!(output.contains("Shell Changed"));

    // Cleanup
    let _ = pty.shutdown_pty();
  }

  #[test]
  #[serial]
  fn test_into_js_object_and_from_js_object() {
    use napi::{Env, JsNumber, JsObject as NapiJsObject, JsString};

    // Mock Env and JsObject would typically be provided by the N-API framework.
    // Here, we mock the environment for testing purposes using N-API testing utilities.

    // For demonstration, we'll assume a valid mock Env and JsObject.
    // Uncomment and implement this section when running with an actual N-API testing framework.

    /*
    let env = napi::Env::default(); // Mock or create a real Env
    let js_obj = env.create_object().unwrap();
    js_obj.set("pid", 1234).unwrap();
    js_obj.set("master_fd", 5).unwrap();
    js_obj.set("command", "bash").unwrap();

    let pty = PtyProcess::from_js_object(&js_obj).expect("Failed to convert from JsObject");
    assert_eq!(pty.pid, 1234);
    assert_eq!(pty.master_fd, 5);
    assert_eq!(pty.command, "bash".to_string());

    let converted_js_obj = pty.into_js_object(&env).expect("Failed to convert to JsObject");
    let pid = converted_js_obj.get::<_, JsNumber>("pid").unwrap().get_int32().unwrap();
    let master_fd = converted_js_obj.get::<_, JsNumber>("master_fd").unwrap().get_int32().unwrap();
    let command = converted_js_obj.get::<_, JsString>("command").unwrap().into_utf8()?.into_owned()?;
    assert_eq!(pid, 1234);
    assert_eq!(master_fd, 5);
    assert_eq!(command, "bash".to_string());
    */
  }
}

/// Represents a multiplexer for managing multiple PTY sessions.
#[derive(Debug, Clone)]
pub struct Multiplexer {
  sessions: std::collections::HashMap<u32, i32>, // Example field: map of session IDs to file descriptors
}

impl Multiplexer {
  /// Creates a new Multiplexer instance.
  pub fn new() -> Self {
    Multiplexer {
      sessions: std::collections::HashMap::new(), // Initialize the sessions map
    }
  }

  /// Sends data to a specific session.
  ///
  /// # Arguments
  ///
  /// * `session_id` - The ID of the session to send data to.
  /// * `buffer` - The data buffer to send.
  ///
  /// # Returns
  ///
  /// Returns `Ok(())` on success or a `napi::Error` on failure.
  pub fn send_to_session(&self, session_id: u32, buffer: Buffer) -> napi::Result<()> {
    // Retrieve the file descriptor for the given session_id
    let fd = self
      .sessions
      .get(&session_id)
      .ok_or_else(|| NapiError::from_reason(format!("Session ID {} not found", session_id)))?;

    // Write the buffer to the file descriptor
    let bytes_written = unsafe { write(*fd, buffer.as_ptr() as *const libc::c_void, buffer.len()) };

    if bytes_written < 0 {
      let err = io::Error::last_os_error();
      error!("Write to session {} failed: {}", session_id, err);
      return Err(NapiError::from_reason(format!(
        "Write to session {} failed: {}",
        session_id, err
      )));
    }

    debug!(
      "Successfully sent {} bytes to session {}",
      bytes_written, session_id
    );
    Ok(())
  }

  /// Converts the `Multiplexer` instance into a JavaScript object.
  ///
  /// # Arguments
  ///
  /// * `env` - A reference to the N-API environment.
  ///
  /// # Returns
  ///
  /// A `JsObject` representing the `Multiplexer`.
  ///
  /// # Errors
  ///
  /// Returns a `NapiError` if the object creation fails.
  pub fn into_js_object(&self, env: &Env) -> NapiResult<JsObject> {
    let mut js_obj = env.create_object()?;
    // Add properties and methods to the JS object as needed
    // For example, you might expose methods to send or receive data
    // Here, we'll add a dummy property for demonstration
    js_obj.set("type", "Multiplexer")?;
    Ok(js_obj)
  }
}
