// src/pty/mod.rs
pub mod command_handler;
pub mod commands;
pub mod emulator;
pub mod handle;
pub mod multiplexer;
pub mod output_handler;
pub mod pty_renderer;

pub use crate::pty::emulator::Emulator;
pub use output_handler::PtyOutputHandler;
pub use pty_renderer::PtyRenderer;

#[cfg(target_os = "linux")]
pub mod platform {
  pub use crate::platform::linux::PtyProcess;
}

#[cfg(target_os = "macos")]
pub mod platform {
  pub use crate::platform::macos::PtyProcess;
}

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
pub mod platform {
  pub use crate::platform::unsupported::PtyProcess;
}
