// src/platform/linux.rs

use bytes::Bytes;
use libc::{
  _exit, close, dup2, execle, fork, grantpt, ioctl, kill, open, posix_openpt, ptsname, read,
  setsid, unlockpt, waitpid, winsize, write, O_NOCTTY, O_RDWR, SIGKILL, SIGTERM, TIOCSWINSZ,
  WNOHANG,
};
use log::{debug, error, info, warn};
use std::ffi::CString;
use std::io;
use std::ptr;

/// Extern declaration for environment variables
extern "C" {
  pub static environ: *const *const libc::c_char;
}

/// Type alias for Process ID
pub type PidT = i32;

/// Represents a PTY process on Linux.
#[derive(Debug)]
pub struct PtyProcess {
  pub master_fd: i32,
  pub pid: PidT,
}

impl PtyProcess {
  /// Creates a new PTY process on Linux.
  pub fn new() -> io::Result<Self> {
    debug!("Creating new PtyProcess on Linux");

    // Step 1: Open PTY master
    let master_fd = unsafe { posix_openpt(O_RDWR | O_NOCTTY) };
    if master_fd < 0 {
      error!("Failed to open PTY master: {}", io::Error::last_os_error());
      return Err(io::Error::last_os_error());
    }
    debug!("Opened PTY master_fd: {}", master_fd);

    // Step 2: Grant access to slave PTY
    if unsafe { grantpt(master_fd) } != 0 {
      error!(
        "Failed to grant PTY slave access: {}",
        io::Error::last_os_error()
      );
      unsafe { close(master_fd) };
      return Err(io::Error::last_os_error());
    }
    debug!("Granted PTY slave");

    // Step 3: Unlock PTY master
    if unsafe { unlockpt(master_fd) } != 0 {
      error!(
        "Failed to unlock PTY master: {}",
        io::Error::last_os_error()
      );
      unsafe { close(master_fd) };
      return Err(io::Error::last_os_error());
    }
    debug!("Unlocked PTY master");

    // Step 4: Get slave PTY name
    let slave_name_ptr = unsafe { ptsname(master_fd) };
    if slave_name_ptr.is_null() {
      error!(
        "Failed to get slave PTY name: {}",
        io::Error::last_os_error()
      );
      unsafe { close(master_fd) };
      return Err(io::Error::last_os_error());
    }
    let slave_name = unsafe { std::ffi::CStr::from_ptr(slave_name_ptr) }
      .to_string_lossy()
      .into_owned();
    debug!("Slave PTY name: {}", slave_name);

    // Step 5: Open slave PTY
    let slave_fd = unsafe { open(slave_name.as_ptr() as *const i8, O_RDWR) };
    if slave_fd < 0 {
      error!(
        "Failed to open PTY slave_fd: {}",
        io::Error::last_os_error()
      );
      unsafe { close(master_fd) };
      return Err(io::Error::last_os_error());
    }
    debug!("Opened PTY slave_fd: {}", slave_fd);

    // Step 6: Fork the process
    let pid = unsafe { fork() };
    if pid < 0 {
      // Fork failed
      error!("Fork failed: {}", io::Error::last_os_error());
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

      if unsafe { dup2(slave_fd, libc::STDIN_FILENO) } < 0
        || unsafe { dup2(slave_fd, libc::STDOUT_FILENO) } < 0
        || unsafe { dup2(slave_fd, libc::STDERR_FILENO) } < 0
      {
        error!("dup2 failed");
        unsafe { _exit(1) };
      }

      // Close unused file descriptors
      unsafe { close(master_fd) };
      unsafe { close(slave_fd) };

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
    } else {
      // Parent process
      debug!("In parent process on Linux, child PID: {}", pid);
      // Close slave_fd in parent
      unsafe { close(slave_fd) };
      // Return master_fd and child PID
      Ok(PtyProcess { master_fd, pid })
    }
  }

  /// Writes data to the PTY.
  pub fn write_data(&self, data: &[u8]) -> io::Result<usize> {
    unsafe {
      write(
        self.master_fd,
        data.as_ptr() as *const libc::c_void,
        data.len(),
      )
    }
    .try_into()
    .map_err(|_| {
      io::Error::new(
        io::ErrorKind::Other,
        "Failed to convert bytes_written to usize",
      )
    })
  }

  /// Reads data from the PTY.
  pub fn read_data(&self, buffer: &mut [u8]) -> io::Result<usize> {
    unsafe {
      read(
        self.master_fd,
        buffer.as_mut_ptr() as *mut libc::c_void,
        buffer.len(),
      )
    }
    .try_into()
    .map_err(|_| {
      io::Error::new(
        io::ErrorKind::Other,
        "Failed to convert bytes_read to usize",
      )
    })
  }

  /// Resizes the PTY window.
  pub fn resize(&self, cols: u16, rows: u16) -> io::Result<()> {
    let mut ws: winsize = unsafe { std::mem::zeroed() };
    ws.ws_col = cols;
    ws.ws_row = rows;
    ws.ws_xpixel = 0;
    ws.ws_ypixel = 0;

    let ret = unsafe { ioctl(self.master_fd, TIOCSWINSZ, &ws) };
    if ret != 0 {
      error!("Failed to resize PTY: {}", io::Error::last_os_error());
      return Err(io::Error::last_os_error());
    }
    debug!("Successfully resized PTY to cols: {}, rows: {}", cols, rows);
    Ok(())
  }

  /// Sends a signal to the child process.
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
  pub fn force_kill(&self) -> io::Result<()> {
    info!("Forcefully killing PTY process {}.", self.pid);
    self.kill_process(SIGKILL)
  }

  /// Sets an environment variable for the PTY process.
  pub fn set_env(&mut self, key: String, value: String) -> io::Result<()> {
    info!("Setting environment variable: {}={}", key, value);
    // Note: Setting environment variables at runtime for the child process is non-trivial.
    // Typically, environment variables should be set before spawning the child process.
    // This method currently sets the environment variable for the current process.
    // To affect the child PTY process, consider restarting the shell with updated environment.
    std::env::set_var(&key, &value);
    Ok(())
  }

  /// Changes the shell for the PTY process.
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

  /// Spawns a new shell process for the PTY.
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
        Ok(())
      }
    }
  }

  /// Checks if the PTY process is still running.
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
