// src/platform/mod.rs

// Conditionally compile platform-specific modules
#[cfg(target_os = "linux")]
pub mod linux;

#[cfg(target_os = "macos")]
pub mod macos;

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
pub mod unsupported;

// Re-export the appropriate `PtyProcess` type depending on the platform
#[cfg(target_os = "linux")]
pub use linux::PtyProcess;

#[cfg(target_os = "macos")]
pub use macos::PtyProcess;

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
pub use unsupported::PtyProcess;
