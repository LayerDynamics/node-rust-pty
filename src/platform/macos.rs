// src/platform/macos.rs

use libc::{
    _exit, close, dup2, execle, fork, ioctl, kill, openpty, read, setsid, waitpid, winsize, write,
    SIGKILL, TIOCSWINSZ,
};
use log::{debug, error, info, warn};
use std::ffi::CString;
use std::io;
use std::ptr;
use bytes::Bytes; // Added import for bytes::Bytes

extern "C" {
    pub static environ: *const *const libc::c_char;
}

/// Type alias for Process ID
pub type PidT = i32;

/// Represents a PTY process on macOS.
#[derive(Debug, Clone)]
pub struct PtyProcess {
    pub master_fd: i32,
    pub pid: PidT,
}

impl PtyProcess {
    /// Creates a new PTY process on macOS.
    pub fn new() -> io::Result<Self> {
        debug!("Creating new PtyProcess on macOS");

        let (master_fd, slave_fd) = Self::open_pty()?;

        let pid = unsafe { fork() };
        match pid {
            -1 => {
                // Fork failed
                error!("Fork failed: {}", io::Error::last_os_error());
                Self::cleanup_fd(master_fd, slave_fd);
                return Err(io::Error::last_os_error());
            }
            0 => {
                // Child process
                Self::setup_child(slave_fd).unwrap_or_else(|e| {
                    error!("Failed to setup child: {}", e);
                    std::process::exit(1);
                });
                unreachable!(); // Ensures the child process does not continue
            }
            _ => {
                // Parent process
                debug!("In parent process on macOS, child PID: {}", pid);
                // Close slave_fd in parent
                unsafe { close(slave_fd) };
                return Ok(PtyProcess { master_fd, pid });
            }
        }

        // This line is technically unreachable because the child process exits,
        // but it's needed to satisfy the function's return type.
        Ok(PtyProcess { master_fd, pid })
    }

    /// Opens a PTY (master and slave).
    fn open_pty() -> io::Result<(i32, i32)> {
        let mut master_fd: i32 = 0;
        let mut slave_fd: i32 = 0;
        let mut ws: winsize = unsafe { std::mem::zeroed() };
        ws.ws_row = 24;
        ws.ws_col = 80;

        let ret = unsafe {
            openpty(
                &mut master_fd,
                &mut slave_fd,
                ptr::null_mut(),
                ptr::null_mut(),
                &mut ws,
            )
        };
        if ret != 0 {
            error!("Failed to open PTY: {}", io::Error::last_os_error());
            return Err(io::Error::last_os_error());
        }

        debug!(
            "Opened PTY master_fd: {}, slave_fd: {}",
            master_fd, slave_fd
        );
        Ok((master_fd, slave_fd))
    }

    /// Sets up the child process.
    fn setup_child(slave_fd: i32) -> io::Result<()> {
        debug!("In child process on macOS");

        // Create a new session
        if unsafe { setsid() } < 0 {
            error!("setsid failed");
            unsafe { _exit(1) };
        }

        // Set slave PTY as controlling terminal
        let mut ws: winsize = unsafe { std::mem::zeroed() };
        ws.ws_row = 24;
        ws.ws_col = 80;

        if unsafe { ioctl(slave_fd, TIOCSWINSZ, &ws) } < 0 {
            error!("ioctl TIOCSWINSZ failed");
            unsafe { _exit(1) };
        }

        Self::duplicate_fds(slave_fd)?;

        // Close unused file descriptors
        unsafe {
            close(slave_fd);
        }

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
    }

    /// Duplicates file descriptors for stdin, stdout, stderr.
    fn duplicate_fds(slave_fd: i32) -> io::Result<()> {
        if unsafe { dup2(slave_fd, libc::STDIN_FILENO) } < 0 {
            error!("dup2 failed for STDIN");
            unsafe { _exit(1) };
        }

        if unsafe { dup2(slave_fd, libc::STDOUT_FILENO) } < 0 {
            error!("dup2 failed for STDOUT");
            unsafe { _exit(1) };
        }

        if unsafe { dup2(slave_fd, libc::STDERR_FILENO) } < 0 {
            error!("dup2 failed for STDERR");
            unsafe { _exit(1) };
        }

        Ok(())
    }

    /// Cleans up file descriptors in case of errors.
    fn cleanup_fd(master_fd: i32, slave_fd: i32) {
        unsafe {
            close(master_fd);
            close(slave_fd);
        }
    }

    /// Writes data to the PTY.
    pub fn write_data(&self, data: &Bytes) -> io::Result<usize> {
        let bytes_written = unsafe {
            write(
                self.master_fd,
                data.as_ptr() as *const libc::c_void,
                data.len(),
            )
        };

        if bytes_written < 0 {
            Err(io::Error::last_os_error())
        } else {
            Ok(bytes_written as usize)
        }
    }

    /// Reads data from the PTY.
    pub fn read_data(&self, buffer: &mut [u8]) -> io::Result<usize> {
        let bytes_read = unsafe {
            read(
                self.master_fd,
                buffer.as_mut_ptr() as *mut libc::c_void,
                buffer.len(),
            )
        };

        if bytes_read < 0 {
            Err(io::Error::last_os_error())
        } else {
            Ok(bytes_read as usize)
        }
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

    /// Forcefully kills the PTY process.
    pub fn force_kill(&self) -> io::Result<()> {
        info!("Forcefully killing PTY process {}.", self.pid);
        self.kill_process(SIGKILL)
    }

    /// Sets an environment variable for the PTY process.
    pub fn set_env(&mut self, key: String, value: String) -> io::Result<()> {
        info!("Setting environment variable: {}={}", key, value);
        // This is a placeholder. Implement logic to set environment variables for the PTY process.
        // For example, you might need to store environment variables within the PtyProcess struct
        // and apply them when spawning new shells.
        // Here's a simple example using std::env, but note that this affects the entire process.
        std::env::set_var(key, value);
        Ok(())
    }

    /// Changes the shell for the PTY process.
    pub fn change_shell(&mut self, shell_path: String) -> io::Result<()> {
        info!("Changing shell to {}", shell_path);
        // This is a placeholder. Implement logic to change the shell for the PTY process.
        // For example, you might need to kill the current shell and spawn a new one with the specified shell.
        self.kill_process(libc::SIGTERM)?;
        self.spawn_new_shell(shell_path)?;
        Ok(())
    }

    /// Retrieves the status of the PTY process.
    pub fn status(&self) -> io::Result<String> {
        // This is a placeholder. Implement logic to retrieve the status of the PTY process.
        // For example, check if the process is still running.
        let status = if self.is_running()? {
            "Running".to_string()
        } else {
            "Not Running".to_string()
        };
        Ok(status)
    }

    /// Sets the log level for the PTY process.
    pub fn set_log_level(&self, level: String) -> io::Result<()> {
        info!("Setting log level to {}", level);
        // This is a placeholder. Implement logic to set the log level for the PTY process.
        // Depending on your logging framework, you might need to interface with it accordingly.
        // For example, using the `log` crate:
        use log::LevelFilter;
        let level_filter = match level.to_lowercase().as_str() {
            "error" => LevelFilter::Error,
            "warn" => LevelFilter::Warn,
            "info" => LevelFilter::Info,
            "debug" => LevelFilter::Debug,
            "trace" => LevelFilter::Trace,
            _ => LevelFilter::Info, // Default level
        };
        log::set_max_level(level_filter);
        Ok(())
    }

    /// Shuts down the PTY process gracefully.
    pub fn shutdown_pty(&mut self) -> io::Result<()> {
        info!("Shutting down PTY process {}", self.pid);
        // Implement logic to gracefully shut down the PTY process.
        // For example, send SIGTERM and wait for the process to exit.
        self.kill_process(libc::SIGTERM)?;
        // Optionally, wait for the process to exit
        match self.waitpid(0) {
            Ok(pid) => {
                info!("PTY process {} terminated with PID {}", self.pid, pid);
                self.close_master_fd()?;
                Ok(())
            }
            Err(e) => {
                error!("Failed to wait for PTY process {}: {}", self.pid, e);
                Err(e)
            }
        }
    }

    /// Spawns a new shell process for the PTY.
    fn spawn_new_shell(&mut self, shell_path: String) -> io::Result<()> {
        info!("Spawning new shell: {}", shell_path);
        // Close existing master_fd
        self.close_master_fd()?;

        // Open new PTY
        let (new_master_fd, new_slave_fd) = Self::open_pty()?;
        self.master_fd = new_master_fd;

        let pid = unsafe { fork() };
        match pid {
            -1 => {
                error!("Fork failed: {}", io::Error::last_os_error());
                Self::cleanup_fd(new_master_fd, new_slave_fd);
                return Err(io::Error::last_os_error());
            }
            0 => {
                // Child process
                Self::setup_child(new_slave_fd).unwrap_or_else(|e| {
                    error!("Failed to setup child: {}", e);
                    std::process::exit(1);
                });
                unreachable!(); // Ensures the child process does not continue
            }
            _ => {
                // Parent process
                debug!("In parent process on macOS, new child PID: {}", pid);
                // Close slave_fd in parent
                unsafe { close(new_slave_fd) };
                self.pid = pid;
                Ok(())
            }
        }
    }

    /// Checks if the PTY process is still running.
    fn is_running(&self) -> io::Result<bool> {
        match unsafe { kill(self.pid, 0) } {
            0 => Ok(true),
            -1 => {
                if io::Error::last_os_error().raw_os_error() == Some(libc::ESRCH) {
                    Ok(false)
                } else {
                    Err(io::Error::last_os_error())
                }
            }
            _ => Ok(false),
        }
    }
}
