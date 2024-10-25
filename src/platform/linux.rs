// src/platform/linux.rs

use bytes::Bytes;
use libc::{
  _exit, close, dup2, execle, fork, grantpt, ioctl, kill, open, posix_openpt, ptsname, read,
  setsid, unlockpt, waitpid, winsize, write, O_NOCTTY, O_RDWR, SIGKILL, SIGTERM, TIOCSWINSZ,
};
use log::{debug, error, info};
use napi::{Env, Error as NapiError, JsNumber, JsObject, JsString, Result as NapiResult};
use napi_derive::napi;
use std::ffi::CString;
use std::io;
use std::ptr;
use std::sync::Arc;
use tokio::task;

// Extern declaration for environment variables
extern "C" {
  pub static environ: *const *const libc::c_char;
}

/// Type alias for Process ID
pub type PidT = i32;

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
  pub fn send_to_session(
    &self,
    session_id: u32,
    buffer: napi::bindgen_prelude::Buffer,
  ) -> napi::Result<()> {
    // Implement the actual sending logic here.
    // For demonstration, we'll assume it always succeeds.
    // In a real implementation, you would send the buffer to the specified session.
    // This might involve writing to a file descriptor, sending over a network, etc.
    debug!(
      "Sending data to session {}: {:?}",
      session_id,
      buffer.as_ref()
    );
    // Simulate sending data
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

/// Represents a PTY process on Linux.
#[derive(Debug)]
pub struct PtyProcess {
  pub master_fd: i32,
  pub pid: PidT,
  pub multiplexer: Arc<Multiplexer>, // Added multiplexer field
  pub command: String,               // Added command field
}

impl PtyProcess {
  /// Creates a new PTY process on Linux.
  ///
  /// This function forks the current process. The child process sets up the PTY
  /// and executes the shell, while the parent process retains the master file descriptor.
  ///
  /// # Errors
  ///
  /// Returns an `io::Error` if the PTY cannot be opened or if the fork fails.
  pub fn new() -> io::Result<Self> {
    debug!("Creating new PtyProcess on Linux");

    // Open PTY master
    let master_fd = unsafe { posix_openpt(O_RDWR | O_NOCTTY) };
    if master_fd < 0 {
      error!("Failed to open PTY master: {}", io::Error::last_os_error());
      return Err(io::Error::last_os_error());
    }
    debug!("Opened PTY master_fd: {}", master_fd);

    // Grant access to slave PTY
    if unsafe { grantpt(master_fd) } != 0 {
      error!(
        "Failed to grant PTY slave access: {}",
        io::Error::last_os_error()
      );
      unsafe { close(master_fd) };
      return Err(io::Error::last_os_error());
    }
    debug!("Granted PTY slave access");

    // Unlock PTY master
    if unsafe { unlockpt(master_fd) } != 0 {
      error!(
        "Failed to unlock PTY master: {}",
        io::Error::last_os_error()
      );
      unsafe { close(master_fd) };
      return Err(io::Error::last_os_error());
    }
    debug!("Unlocked PTY master");

    // Get slave PTY name
    let slave_name_ptr = unsafe { ptsname(master_fd) };
    if slave_name_ptr.is_null() {
      error!(
        "Failed to get slave PTY name: {}",
        io::Error::last_os_error()
      );
      unsafe { close(master_fd) };
      return Err(io::Error::last_os_error());
    }
    let slave_name_cstr = unsafe { CString::from_raw(slave_name_ptr as *mut i8) };
    let slave_name = slave_name_cstr.to_str().map_err(|e| {
      error!("Failed to convert slave PTY name to string: {}", e);
      io::Error::new(io::ErrorKind::Other, "Invalid PTY name")
    })?;

    // Open slave PTY
    let slave_fd = unsafe { open(slave_name_cstr.as_ptr(), O_RDWR) };
    if slave_fd < 0 {
      error!(
        "Failed to open PTY slave_fd: {}",
        io::Error::last_os_error()
      );
      unsafe { close(master_fd) };
      return Err(io::Error::last_os_error());
    }
    debug!("Opened PTY slave_fd: {}", slave_fd);

    // Fork the process
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
        Self::setup_child(slave_fd).unwrap_or_else(|e| {
          error!("Failed to setup child: {}", e);
          std::process::exit(1);
        });
        unreachable!(); // Ensures the child process does not continue
      }
      _ => {
        // Parent process
        debug!("In parent process on Linux, child PID: {}", pid);
        // Close slave_fd in parent
        unsafe { close(slave_fd) };
        Ok(PtyProcess {
          master_fd,
          pid,
          multiplexer: Arc::new(Multiplexer::new()), // Initialize multiplexer
          command: "/bin/bash".to_string(),          // Initialize command
        })
      }
    }
  }

  /// Sends data to the PTY process.
  ///
  /// # Arguments
  ///
  /// * `data` - A byte slice containing the data to send to the PTY.
  ///
  /// # Returns
  ///
  /// Returns `Ok(())` if the data was successfully sent, or an `Err` containing a `napi::Error`.
  pub fn send_to_pty(
    multiplexer: Arc<Multiplexer>,
    data: napi::bindgen_prelude::Buffer,
  ) -> napi::Result<()> {
    let data_clone = data.to_vec();
    let buffer = napi::bindgen_prelude::Buffer::from(data_clone);

    // Assuming session_id 0 is reserved for global commands.
    multiplexer.send_to_session(0, buffer)
  }

  /// Sends data to the PTY process from JavaScript.
  ///
  /// # Arguments
  ///
  /// * `env` - The N-API environment.
  /// * `multiplexer` - The JavaScript object representing the Multiplexer.
  /// * `data` - The data buffer to send.
  ///
  /// # Returns
  ///
  /// Returns `Ok(())` if the data was successfully sent, or an `Err` containing a `napi::Error`.
  #[napi]
  pub fn send_to_pty_js(
    env: Env,
    multiplexer_js: JsObject,
    data: napi::bindgen_prelude::Buffer,
  ) -> napi::Result<()> {
    let multiplexer: Arc<Multiplexer> =
      Arc::new(env.unwrap::<&mut Multiplexer>(&multiplexer_js)?.clone());
    PtyProcess::send_to_pty(multiplexer, data)
  }

  /// Constructs a `PtyProcess` instance from a JavaScript object.
  ///
  /// # Arguments
  ///
  /// * `env` - A reference to the N-API environment.
  /// * `js_obj` - The JavaScript object containing the process information.
  ///
  /// # Returns
  ///
  /// A `PtyProcess` instance.
  ///
  /// # Errors
  ///
  /// Returns a `NapiError` if the object does not contain the required fields or if field extraction fails.
  pub fn from_js_object(env: &Env, js_obj: &JsObject) -> NapiResult<Self> {
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

    // Extract multiplexer if present
    let multiplexer_js = js_obj.get::<&str, JsObject>("multiplexer")?;
    let multiplexer = if let Some(mux_js) = multiplexer_js {
      Arc::new(env.unwrap::<&mut Multiplexer>(&mux_js)?.clone())
    } else {
      Arc::new(Multiplexer::new())
    };

    Ok(PtyProcess {
      pid,
      master_fd,
      multiplexer,
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
  ///
  /// # Returns
  ///
  /// An `io::Result` indicating success or failure.
  ///
  /// # Errors
  ///
  /// Returns an `io::Error` if any setup step fails.
  fn setup_child(slave_fd: i32) -> io::Result<()> {
    debug!("In child process on Linux");

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

    // Execute shell with proper environment
    let shell = CString::new("/bin/bash").unwrap();
    let shell_arg = CString::new("bash").unwrap();
    unsafe {
      execle(
        shell.as_ptr(),
        shell_arg.as_ptr(),
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

  /// Opens a new PTY (master and slave) on Linux.
  ///
  /// # Returns
  ///
  /// A tuple containing the master and slave file descriptors.
  ///
  /// # Errors
  ///
  /// Returns an `io::Error` if the PTY cannot be opened.
  fn open_new_pty() -> io::Result<(i32, i32)> {
    // Open PTY master
    let master_fd = unsafe { posix_openpt(O_RDWR | O_NOCTTY) };
    if master_fd < 0 {
      return Err(io::Error::last_os_error());
    }

    // Grant access to slave PTY
    if unsafe { grantpt(master_fd) } != 0 {
      unsafe { close(master_fd) };
      return Err(io::Error::last_os_error());
    }

    // Unlock PTY master
    if unsafe { unlockpt(master_fd) } != 0 {
      unsafe { close(master_fd) };
      return Err(io::Error::last_os_error());
    }

    // Get slave PTY name
    let slave_name_ptr = unsafe { ptsname(master_fd) };
    if slave_name_ptr.is_null() {
      unsafe { close(master_fd) };
      return Err(io::Error::last_os_error());
    }
    let slave_name_cstr = unsafe { CString::from_raw(slave_name_ptr as *mut i8) };
    let slave_name = slave_name_cstr.to_str().map_err(|e| {
      error!("Failed to convert slave PTY name to string: {}", e);
      io::Error::new(io::ErrorKind::Other, "Invalid PTY name")
    })?;

    // Open slave PTY
    let slave_fd = unsafe { open(slave_name_cstr.as_ptr(), O_RDWR) };
    if slave_fd < 0 {
      unsafe { close(master_fd) };
      return Err(io::Error::last_os_error());
    }

    Ok((master_fd, slave_fd))
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
    let bytes_written = unsafe {
      write(
        self.master_fd,
        data.as_ptr() as *const libc::c_void,
        data.len(),
      )
    };

    if bytes_written < 0 {
      Err(io::Error::last_os_error())
    } else {
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
    let bytes_read = unsafe {
      read(
        self.master_fd,
        buffer.as_mut_ptr() as *mut libc::c_void,
        buffer.len(),
      )
    };

    if bytes_read < 0 {
      Err(io::Error::last_os_error())
    } else {
      Ok(bytes_read as usize)
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
    let mut ws: winsize = unsafe { std::mem::zeroed() };
    ws.ws_col = cols;
    ws.ws_row = rows;

    let ret = unsafe { ioctl(self.master_fd, TIOCSWINSZ, &ws) };
    if ret != 0 {
      error!("Failed to resize PTY: {}", io::Error::last_os_error());
      return Err(io::Error::last_os_error());
    }
    debug!("Successfully resized PTY to cols: {}, rows: {}", cols, rows);
    Ok(())
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
    let pid = unsafe { waitpid(self.pid, ptr::null_mut(), options) };
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
      // Set the environment variable for the current process (parent)
      std::env::set_var(&key, &value);

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
  /// # Arguments
  ///
  /// * `env` - The N-API environment.
  /// * `js_obj` - The JavaScript object representing the PtyProcess.
  ///
  /// # Returns
  ///
  /// Returns the PID of the PTY process.
  ///
  /// # Errors
  ///
  /// Returns a `napi::Error` if any error occurs.
  #[napi]
  pub fn pid(env: Env, js_obj: JsObject) -> napi::Result<i32> {
    let pty_process: PtyProcess = PtyProcess::from_js_object(&env, &js_obj)?;
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
    let (new_master_fd, new_slave_fd) = Self::open_new_pty()?;
    self.master_fd = new_master_fd;

    let shell_cstr = CString::new(shell_path.clone()).unwrap();
    let shell_arg = CString::new(shell_path.clone()).unwrap();

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
        Self::setup_child(new_slave_fd).unwrap_or_else(|e| {
          error!("Failed to setup child during shell change: {}", e);
          std::process::exit(1);
        });
        unreachable!(); // Ensures the child process does not continue
      }
      _ => {
        // Parent process
        debug!("In parent process on Linux, new child PID: {}", pid);
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
  fn is_running(&self) -> io::Result<bool> {
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
  use std::io::{self};
  use std::sync::Once;

  static INIT: Once = Once::new();

  fn init_logger() {
    INIT.call_once(|| {
      env_logger::builder().is_test(true).try_init().ok();
    });
  }

  #[test]
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
  fn test_resize_pty() {
    init_logger();
    let mut pty = PtyProcess::new().expect("Failed to create PtyProcess");

    let resize_result = pty.resize(100, 40);
    assert!(resize_result.is_ok());

    // Read and discard initial output
    let mut buffer = [0u8; 1024];
    let _ = pty.read_data(&mut buffer);

    // Send command to get terminal size
    let test_command = Bytes::from("stty size\n");
    let write_result = pty.write_data(&test_command);
    assert!(write_result.is_ok());

    // Read the output
    let mut total_output = String::new();
    for _ in 0..5 {
      let read_result = pty.read_data(&mut buffer);
      if read_result.is_ok() {
        let bytes_read = read_result.unwrap();
        let output = String::from_utf8_lossy(&buffer[..bytes_read]);
        total_output.push_str(&output);
        if total_output.contains('\n') {
          break;
        }
      } else {
        break;
      }
    }

    println!("Terminal size output: '{}'", total_output);

    // Extract the terminal size from the output
    let lines: Vec<&str> = total_output.lines().collect();
    for line in lines {
      if line.trim().is_empty() || line.contains("stty size") {
        continue;
      }
      if line.contains("40 100") {
        assert!(true);
        return;
      } else {
        panic!(
          "Incorrect terminal size after resize. Expected '40 100', got '{}'",
          line.trim()
        );
      }
    }

    panic!("Terminal size not found in output");

    // Cleanup
    let _ = pty.shutdown_pty();
  }

  #[test]
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
  fn test_into_js_object_and_from_js_object() {
    // Skipping implementation as it requires a real N-API environment.
  }
}
