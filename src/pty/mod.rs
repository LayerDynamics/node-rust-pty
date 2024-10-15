// src/pty/mod.rs
pub mod command_handler;
pub mod commands;
pub mod handle;
pub mod multiplexer;
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
