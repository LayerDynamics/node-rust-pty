// src/platform/unsupported.rs

use std::io;

/// Represents an unsupported PTY process.
#[derive(Debug)]
pub struct PtyProcess;

impl PtyProcess {
    /// Unsupported platform.
    pub fn new() -> io::Result<Self> {
        Err(io::Error::new(io::ErrorKind::Other, "Unsupported platform"))
    }

    pub fn write_data(&self, _data: &[u8]) -> io::Result<usize> {
        Err(io::Error::new(io::ErrorKind::Other, "Unsupported platform"))
    }

    pub fn read_data(&self, _buffer: &mut [u8]) -> io::Result<usize> {
        Err(io::Error::new(io::ErrorKind::Other, "Unsupported platform"))
    }

    pub fn resize(&self, _cols: u16, _rows: u16) -> io::Result<()> {
        Err(io::Error::new(io::ErrorKind::Other, "Unsupported platform"))
    }

    pub fn kill_process(&self, _signal: i32) -> io::Result<()> {
        Err(io::Error::new(io::ErrorKind::Other, "Unsupported platform"))
    }

    pub fn waitpid(&self, _options: i32) -> io::Result<i32> {
        Err(io::Error::new(io::ErrorKind::Other, "Unsupported platform"))
    }

    pub fn close_master_fd(&self) -> io::Result<()> {
        Err(io::Error::new(io::ErrorKind::Other, "Unsupported platform"))
    }
}
