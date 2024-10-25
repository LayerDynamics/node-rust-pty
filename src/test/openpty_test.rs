// src/test/openpty_test.rs

use crate::platform::macos::PtyProcess; // Adjust the path as necessary
use libc::{openpty, termios, winsize, B9600};
use serial_test::serial;
use std::ffi::CStr;
use std::io;
use std::mem::zeroed;
use std::ptr;

#[test]
#[serial]
fn test_manual_openpty() {
  // Initialize terminal attributes
  let mut termp: termios = unsafe { zeroed() };
  termp.c_iflag = 0;
  termp.c_oflag = 0;
  termp.c_cflag = libc::CREAD | libc::CS8 | libc::CLOCAL;
  termp.c_lflag = 0;

  // Properly initialize control characters
  termp.c_cc[libc::VMIN] = 1;
  termp.c_cc[libc::VTIME] = 0;

  // Set input and output baud rates
  termp.c_ispeed = B9600;
  termp.c_ospeed = B9600;

  let mut ws: winsize = unsafe { zeroed() };
  ws.ws_row = 24;
  ws.ws_col = 80;
  ws.ws_xpixel = 0;
  ws.ws_ypixel = 0;

  let mut master_fd: i32 = 0;
  let mut slave_fd: i32 = 0;

  // Allocate buffer for PTY name
  let mut namebuf = [0 as libc::c_char; 64];

  // Correctly pass mutable references
  let ret = unsafe {
    openpty(
      &mut master_fd,
      &mut slave_fd,
      namebuf.as_mut_ptr(),
      &mut termp, // Mutable reference
      &mut ws,    // Mutable reference
    )
  };

  assert_eq!(
    ret,
    0,
    "openpty failed with error: {}",
    io::Error::last_os_error()
  );

  // Convert PTY name to Rust string
  let name = unsafe {
    CStr::from_ptr(namebuf.as_ptr())
      .to_str()
      .unwrap_or("Unknown")
      .to_string()
  };

  println!(
    "Opened PTY: master_fd={}, slave_fd={}, name={}",
    master_fd, slave_fd, name
  );

  // Close file descriptors to prevent resource leaks
  unsafe {
    libc::close(master_fd);
    libc::close(slave_fd);
  }
}

#[test]
#[serial]
fn test_pty_process_creation() {
  // Initialize logger if necessary
  // init_logger(); // Uncomment if you have a logger setup

  // Ensure PtyProcess::new() works correctly
  let result = PtyProcess::new();
  assert!(result.is_ok(), "PtyProcess::new() failed");

  // Declare pty as mutable to allow calling mutable methods
  let mut pty = result.unwrap();
  assert!(pty.master_fd > 0, "Invalid master_fd");
  assert!(pty.pid > 0, "Invalid pid");
  assert_eq!(pty.command, "/bin/bash".to_string(), "Incorrect command");

  // Cleanup by shutting down the PTY process
  let shutdown_result = pty.shutdown_pty();
  assert!(shutdown_result.is_ok(), "shutdown_pty() failed");
}

#[test]
#[serial]
fn test_shutdown_pty() {
  // Initialize logger if necessary
  // init_logger(); // Uncomment if you have a logger setup

  // Create a new PtyProcess
  let mut pty = PtyProcess::new().expect("Failed to create PtyProcess");

  // Ensure the PTY process is running
  let status = pty.status().expect("Failed to get PTY status");
  assert_eq!(status, "Running", "PTY process should be running");

  // Shutdown the PTY process
  pty.shutdown_pty().expect("Failed to shutdown PTY");

  // Verify that the PTY process is no longer running
  let status_after = pty
    .status()
    .expect("Failed to get PTY status after shutdown");
  assert_eq!(
    status_after, "Not Running",
    "PTY process should not be running after shutdown"
  );
}
