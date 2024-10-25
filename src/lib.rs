#![allow(unused_imports)]
// src/lib.rs
use crate::pty::emulator::Emulator;
use crate::pty::pty_renderer::PtyRenderer;
use crossterm::execute;
use crossterm::terminal::{
  disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use napi::bindgen_prelude::*;
use napi_derive::napi;
use std::io::stdout;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
// Re-exporting modules for easier access
pub mod path;
pub mod platform;
pub mod pty;
mod test;
pub mod utils;
pub mod virtual_dom;
pub mod worker;

pub use path::{expand_path, get_default_shell, get_home_dir};
pub use pty::handle::PtyHandle;
pub use worker::PtyWorker;
// pub use worker::PtyWorker; // Commented out as 'worker' module is unresolved
#[cfg(target_os = "linux")]
pub use platform::linux::PtyProcess;

#[cfg(target_os = "macos")]
pub use platform::macos::PtyProcess;

#[cfg(not(any(target_os = "linux", target_os = "macos")))]


#[napi]
pub fn run_terminal() -> napi::Result<()> {
  // Initialize the PTY Renderer
  let term_pty_renderer = Arc::new(Mutex::new(PtyRenderer::new()));

  // Initialize the Emulator
  let mut emulator = Emulator::new();

  // Create a channel for communication
  let (tx, rx) = std::sync::mpsc::channel();

  // Spawn the input handling thread
  let input_handle = std::thread::spawn(move || -> napi::Result<()> {
    loop {
      // Handle input here
      match rx.recv() {
        Ok(pty::commands::PtyCommand::CreateSession(_)) => {
          // Handle CreateSession command
        }
        Ok(pty::commands::PtyCommand::RemoveSession(_, _)) => {
          // Handle RemoveSession command
        }
        Ok(pty::commands::PtyCommand::CloseAllSessions(_)) => {
          // Handle CloseAllSessions command
        }
        Ok(pty::commands::PtyCommand::OtherCommand) => {
          // Handle OtherCommand
        }
        Err(_) | Ok(_) => {
          // Handle error or exit
          break;
        }
        Err(_) => {
          // Handle error
          break;
        }
      }

      // For demonstration, just sleep
      std::thread::sleep(std::time::Duration::from_secs(1));
    }
    Ok(())
  });

  // Main loop
  loop {
    // For demonstration, output a styled message every second
    std::thread::sleep(std::time::Duration::from_secs(1));
    let output = b"Hello from PTY!\n";
    term_pty_renderer.lock().unwrap().render(output)?;

    // Check for user interrupt (e.g., Ctrl+C)
    if ctrlc::try_recv().is_ok() {
      break;
    }
    // Signal the input thread to exit and wait for it
    tx.send(pty::commands::PtyCommand::OtherCommand).unwrap();
    input_handle.join().unwrap();

    Ok(())
  }
}

#[napi]
pub fn init_module() -> napi::Result<()> {
  // Perform any necessary initialization here
  Ok(())
}
