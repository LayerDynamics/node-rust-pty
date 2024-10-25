use libc::{openpty, winsize};
use std::ffi::CString;
use std::io;

fn main() -> io::Result<()> {
  let mut master_fd: i32 = 0;
  let mut slave_fd: i32 = 0;
  let mut ws: winsize = unsafe { std::mem::zeroed() };
  ws.ws_row = 24;
  ws.ws_col = 80;

  let term = CString::new("xterm-256color").unwrap();

  let ret = unsafe {
    openpty(
      &mut master_fd,
      &mut slave_fd,
      term.as_ptr(),
      std::ptr::null_mut(),
      &mut ws,
    )
  };

  if ret != 0 {
    eprintln!("Failed to open PTY: {}", io::Error::last_os_error());
    return Err(io::Error::last_os_error());
  }

  println!(
    "PTY successfully opened. Master FD: {}, Slave FD: {}",
    master_fd, slave_fd
  );

  // Clean up
  unsafe {
    libc::close(master_fd);
    libc::close(slave_fd);
  }

  Ok(())
}
