// src/lib.rs
// Re-exporting modules for easier access
pub mod platform;
pub mod pty;
pub mod utils;
pub mod test; // Including the test module

// Conditional compilation for the platform-specific `PtyProcess`
#[cfg(target_os = "linux")]
pub use platform::linux::PtyProcess;

#[cfg(target_os = "macos")]
pub use platform::macos::PtyProcess;

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
pub use platform::unsupported::PtyProcess;

// Expose the main handle to the library user
pub use pty::handle::PtyHandle;

// Expose the multiplexer handle to the library user
pub use pty::multiplexer_handle::MultiplexerHandle;

// Include tests for PTY handle
#[cfg(test)]
mod tests {
    use crate::test::pty_handle_tests::*;
}
