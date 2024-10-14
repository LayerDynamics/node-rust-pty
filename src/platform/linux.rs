// src/platform/linux.rs

use libc::{
  _exit, close, dup2, execle, fork, grantpt, ioctl, kill, open, posix_openpt, ptsname, read,
  setsid, unlockpt, waitpid, winsize, write, O_NOCTTY, O_RDWR, SIGKILL, SIGTERM, TIOCSWINSZ,
  WNOHANG,
};
use log::{debug, error};
use std::ffi::CString;
use std::io;
use std::ptr;

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
}
