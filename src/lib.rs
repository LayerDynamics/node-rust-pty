// src/lib.rs
// Re-exporting modules for easier access
pub mod platform;
pub mod pty;
pub mod utils;
pub mod test;
pub use pty::handle::PtyHandle;
pub use pty::handle::MultiplexerHandle;

// Conditional compilation for the platform-specific `PtyProcess`
#[cfg(target_os = "linux")]
pub use platform::linux::PtyProcess;

#[cfg(target_os = "macos")]
pub use platform::macos::PtyProcess;

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
pub use platform::unsupported::PtyProcess;
