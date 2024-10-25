// src/platform/windows.rs

use bytes::Bytes;
use log::{debug, error, info};
use napi::{Env, Error as NapiError, JsNumber, JsObject, JsString, Result as NapiResult};
use napi_derive::napi;
use std::ffi::OsString;
use std::io::{self, Read, Write};
use std::process::{Child, Command, Stdio};
use std::sync::Arc;
use winpty::Winpty;

/// Type alias for Process ID
pub type PidT = u32;

/// Represents a multiplexer for managing multiple PTY sessions.
#[derive(Debug, Clone)]
pub struct Multiplexer {
    sessions: std::collections::HashMap<u32, Arc<PtyProcess>>, // Map of session IDs to PTY processes
}

impl Multiplexer {
    /// Creates a new Multiplexer instance.
    pub fn new() -> Self {
        Multiplexer {
            sessions: std::collections::HashMap::new(),
        }
    }

    /// Sends data to a specific session.
    ///
    /// # Arguments
    ///
    /// * `session_id` - The ID of the session to send data to.
    /// * `buffer` - The data buffer to send.
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` on success or a `napi::Error` on failure.
    pub fn send_to_session(
        &self,
        session_id: u32,
        buffer: napi::bindgen_prelude::Buffer,
    ) -> napi::Result<()> {
        if let Some(pty_process) = self.sessions.get(&session_id) {
            pty_process.write_data(&Bytes::from(buffer.as_ref()))?;
            Ok(())
        } else {
            Err(NapiError::from_reason(format!(
                "Session ID {} not found",
                session_id
            )))
        }
    }

    /// Converts the `Multiplexer` instance into a JavaScript object.
    ///
    /// # Arguments
    ///
    /// * `env` - A reference to the N-API environment.
    ///
    /// # Returns
    ///
    /// A `JsObject` representing the `Multiplexer`.
    ///
    /// # Errors
    ///
    /// Returns a `NapiError` if the object creation fails.
    pub fn into_js_object(&self, env: &Env) -> NapiResult<JsObject> {
        let mut js_obj = env.create_object()?;
        // Add properties and methods to the JS object as needed
        js_obj.set("type", "Multiplexer")?;
        Ok(js_obj)
    }

    /// Adds a new PTY session to the multiplexer.
    ///
    /// # Arguments
    ///
    /// * `session_id` - The ID of the session.
    /// * `pty_process` - The PTY process to add.
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` on success or a `napi::Error` on failure.
    pub fn add_session(&mut self, session_id: u32, pty_process: Arc<PtyProcess>) -> napi::Result<()> {
        if self.sessions.contains_key(&session_id) {
            return Err(NapiError::from_reason(format!(
                "Session ID {} already exists",
                session_id
            )));
        }
        self.sessions.insert(session_id, pty_process);
        Ok(())
    }

    /// Removes a PTY session from the multiplexer.
    ///
    /// # Arguments
    ///
    /// * `session_id` - The ID of the session to remove.
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` on success or a `napi::Error` if the session does not exist.
    pub fn remove_session(&mut self, session_id: u32) -> napi::Result<()> {
        if self.sessions.remove(&session_id).is_some() {
            Ok(())
        } else {
            Err(NapiError::from_reason(format!(
                "Session ID {} does not exist",
                session_id
            )))
        }
    }
}

/// Represents a PTY process on Windows.
#[derive(Debug)]
pub struct PtyProcess {
    pub winpty: Winpty,
    pub child: Child,
    pub multiplexer: Arc<Multiplexer>, // Multiplexer field
    pub command: String,               // Command field
}

impl PtyProcess {
    /// Creates a new PTY process on Windows.
    ///
    /// This function initializes a Winpty agent, spawns a child process attached to the PTY,
    /// and sets up the necessary handles.
    ///
    /// # Errors
    ///
    /// Returns an `io::Error` if the PTY cannot be initialized or if the process fails to spawn.
    pub fn new() -> io::Result<Self> {
        debug!("Creating new PtyProcess on Windows");

        // Initialize Winpty with default size
        let config = winpty::Config::new(
            winpty::MouseMode::Auto,
            winpty::AgentConfig::default(),
            80,
            24,
        )?;
        let winpty = Winpty::open(&config)?;

        // Prepare command to execute
        let command = "cmd.exe"; // Default shell on Windows
        let mut cmd = Command::new(command);

        cmd.stdin(Stdio::from(winpty.conin()?));
        cmd.stdout(Stdio::from(winpty.conout()?));
        cmd.stderr(Stdio::from(winpty.conerr()?));

        // Spawn the child process
        let child = cmd.spawn()?;
        debug!("Spawned child process with PID: {}", child.id());

        Ok(PtyProcess {
            winpty,
            child,
            multiplexer: Arc::new(Multiplexer::new()),
            command: command.to_string(),
        })
    }

    /// Sends data to the PTY process.
    ///
    /// # Arguments
    ///
    /// * `multiplexer` - The multiplexer managing PTY sessions.
    /// * `data` - The data buffer to send.
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` on success or a `napi::Error` on failure.
    pub fn send_to_pty(
        multiplexer: Arc<Multiplexer>,
        data: napi::bindgen_prelude::Buffer,
    ) -> napi::Result<()> {
        let buffer = napi::bindgen_prelude::Buffer::from(data.to_vec());

        // Assuming session_id 0 is reserved for global commands.
        multiplexer.send_to_session(0, buffer)
    }

    /// Sends data to the PTY process from JavaScript.
    ///
    /// # Arguments
    ///
    /// * `env` - The N-API environment.
    /// * `multiplexer_js` - The JavaScript object representing the Multiplexer.
    /// * `data` - The data buffer to send.
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` if the data was successfully sent, or an `Err` containing a `napi::Error`.
    #[napi]
    pub fn send_to_pty_js(
        env: Env,
        multiplexer_js: JsObject,
        data: napi::bindgen_prelude::Buffer,
    ) -> napi::Result<()> {
        let multiplexer: Arc<Multiplexer> =
            Arc::new(env.unwrap::<&mut Multiplexer>(&multiplexer_js)?.clone());
        PtyProcess::send_to_pty(multiplexer, data)
    }

    /// Constructs a `PtyProcess` instance from a JavaScript object.
    ///
    /// # Arguments
    ///
    /// * `env` - A reference to the N-API environment.
    /// * `js_obj` - The JavaScript object containing the process information.
    ///
    /// # Returns
    ///
    /// A `PtyProcess` instance.
    ///
    /// # Errors
    ///
    /// Returns a `NapiError` if the object does not contain the required fields or if field extraction fails.
    pub fn from_js_object(env: &Env, js_obj: &JsObject) -> NapiResult<Self> {
        let pid_js = js_obj.get::<&str, JsNumber>("pid")?;
        let pid = pid_js
            .ok_or_else(|| NapiError::from_reason("Missing 'pid' field"))?
            .get_uint32()?;

        let command_js = js_obj.get::<&str, JsString>("command")?;
        let command = command_js
            .ok_or_else(|| NapiError::from_reason("Missing 'command' field"))?
            .into_utf8()?
            .into_owned()?;

        // Extract multiplexer if present
        let multiplexer_js = js_obj.get::<&str, JsObject>("multiplexer")?;
        let multiplexer = if let Some(mux_js) = multiplexer_js {
            Arc::new(env.unwrap::<&mut Multiplexer>(&mux_js)?.clone())
        } else {
            Arc::new(Multiplexer::new())
        };

        // Since we cannot reconstruct the Winpty and Child from JS object,
        // this method is not fully implementable without additional context.
        // Returning an error for now.
        Err(NapiError::from_reason(
            "from_js_object is not fully implemented for Windows",
        ))
    }

    /// Converts the `PtyProcess` instance into a JavaScript object.
    ///
    /// # Arguments
    ///
    /// * `env` - A reference to the N-API environment.
    ///
    /// # Returns
    ///
    /// A `JsObject` representing the `PtyProcess`.
    ///
    /// # Errors
    ///
    /// Returns a `NapiError` if the object creation or field setting fails.
    pub fn into_js_object(&self, env: &Env) -> NapiResult<JsObject> {
        let mut js_obj = env.create_object()?;
        js_obj.set("pid", self.child.id() as i32)?;
        js_obj.set("command", self.command.clone())?;
        js_obj.set("multiplexer", self.get_multiplexer_js(env)?)?;
        Ok(js_obj)
    }

    /// Retrieves the multiplexer as a JavaScript object.
    ///
    /// # Arguments
    ///
    /// * `env` - A reference to the N-API environment.
    ///
    /// # Returns
    ///
    /// A `JsObject` representing the `Multiplexer`.
    ///
    /// # Errors
    ///
    /// Returns a `NapiError` if the conversion fails.
    pub fn get_multiplexer_js(&self, env: &Env) -> NapiResult<JsObject> {
        self.multiplexer.clone().into_js_object(env)
    }

    /// Writes data to the PTY.
    ///
    /// # Arguments
    ///
    /// * `data` - A reference to the data to write.
    ///
    /// # Returns
    ///
    /// The number of bytes written.
    ///
    /// # Errors
    ///
    /// Returns an `io::Error` if the write operation fails.
    pub fn write_data(&self, data: &Bytes) -> io::Result<usize> {
        let mut conin = self.winpty.conin()?;
        conin.write_all(data)?;
        Ok(data.len())
    }

    /// Reads data from the PTY.
    ///
    /// # Returns
    ///
    /// A `Vec<u8>` containing the data read.
    ///
    /// # Errors
    ///
    /// Returns an `io::Error` if the read operation fails.
    pub fn read_data(&self) -> io::Result<Vec<u8>> {
        let mut conout = self.winpty.conout()?;
        let mut buffer = Vec::new();
        conout.read_to_end(&mut buffer)?;
        Ok(buffer)
    }

    /// Resizes the PTY window.
    ///
    /// # Arguments
    ///
    /// * `cols` - The number of columns.
    /// * `rows` - The number of rows.
    ///
    /// # Returns
    ///
    /// An `io::Result` indicating success or failure.
    pub fn resize(&self, cols: u16, rows: u16) -> io::Result<()> {
        self.winpty.set_size(cols as i32, rows as i32)?;
        debug!("Successfully resized PTY to cols: {}, rows: {}", cols, rows);
        Ok(())
    }

    /// Sends a signal to the child process.
    ///
    /// On Windows, this function attempts to terminate the process.
    ///
    /// # Arguments
    ///
    /// * `signal` - The signal number to send (ignored on Windows).
    ///
    /// # Returns
    ///
    /// An `io::Result` indicating success or failure.
    pub fn kill_process(&self, _signal: i32) -> io::Result<()> {
        self.child.kill()?;
        Ok(())
    }

    /// Waits for the child process to exit.
    ///
    /// # Returns
    ///
    /// An `io::Result` indicating success or failure.
    pub fn wait(&mut self) -> io::Result<()> {
        self.child.wait()?;
        Ok(())
    }

    /// Closes the PTY handles.
    ///
    /// # Returns
    ///
    /// An `io::Result` indicating success or failure.
    pub fn close_handles(&self) -> io::Result<()> {
        // Handles are automatically closed when Winpty and Child are dropped.
        Ok(())
    }

    /// Forcefully kills the PTY process.
    ///
    /// # Returns
    ///
    /// An `io::Result` indicating success or failure.
    pub fn force_kill(&self) -> io::Result<()> {
        self.kill_process(0)
    }

    /// Sets an environment variable for the PTY process.
    ///
    /// **Note:** Setting environment variables at runtime for the child PTY process is not straightforward.
    /// As a workaround, this method sends a set command to the shell.
    ///
    /// # Arguments
    ///
    /// * `key` - The environment variable key.
    /// * `value` - The environment variable value.
    ///
    /// # Returns
    ///
    /// An `io::Result` indicating success or failure.
    pub fn set_env(&mut self, key: String, value: String) -> io::Result<()> {
        info!("Setting environment variable: {}={}", key, value);

        if self.is_running()? {
            // Send the set command to the PTY shell.
            let command = format!("set {}={}\r\n", key, value);
            self.write_data(&Bytes::copy_from_slice(command.as_bytes()))?;

            info!(
                "Successfully set environment variable in the PTY shell: {}={}",
                key, value
            );
            Ok(())
        } else {
            Err(io::Error::new(
                io::ErrorKind::NotConnected,
                "PTY process is not running",
            ))
        }
    }

    /// Changes the shell for the PTY process.
    ///
    /// # Arguments
    ///
    /// * `shell_path` - The path to the new shell executable.
    ///
    /// # Returns
    ///
    /// An `io::Result` indicating success or failure.
    ///
    /// # Errors
    ///
    /// Returns an `io::Error` if terminating the old shell or spawning the new shell fails.
    pub fn change_shell(&mut self, shell_path: String) -> io::Result<()> {
        info!("Changing shell to {}", shell_path);
        // Terminate the current process
        self.kill_process(0)?;

        // Wait for the process to terminate
        self.wait()?;

        // Spawn a new shell
        self.spawn_new_shell(shell_path)?;
        Ok(())
    }

    /// Retrieves the status of the PTY process.
    ///
    /// # Returns
    ///
    /// A `String` indicating whether the process is "Running" or "Not Running".
    pub fn status(&self) -> io::Result<String> {
        if self.is_running()? {
            Ok("Running".to_string())
        } else {
            Ok("Not Running".to_string())
        }
    }

    /// Sets the log level for the PTY process.
    ///
    /// # Arguments
    ///
    /// * `level` - The desired log level as a `String` (e.g., "error", "warn", "info", "debug", "trace").
    ///
    /// # Returns
    ///
    /// An `io::Result` indicating success or failure.
    pub fn set_log_level(&self, level: String) -> io::Result<()> {
        info!("Setting log level to {}", level);
        let level_filter = match level.to_lowercase().as_str() {
            "error" => log::LevelFilter::Error,
            "warn" => log::LevelFilter::Warn,
            "info" => log::LevelFilter::Info,
            "debug" => log::LevelFilter::Debug,
            "trace" => log::LevelFilter::Trace,
            _ => log::LevelFilter::Info,
        };
        log::set_max_level(level_filter);
        Ok(())
    }

    /// Shuts down the PTY process gracefully.
    ///
    /// # Returns
    ///
    /// An `io::Result` indicating success or failure.
    pub fn shutdown_pty(&mut self) -> io::Result<()> {
        info!("Shutting down PTY process with PID {}", self.child.id());
        // Send termination signal
        self.kill_process(0)?;
        // Wait for the process to terminate
        self.wait()?;
        // Close handles
        self.close_handles()?;
        Ok(())
    }

    /// Retrieves the process ID (PID) of the PTY process.
    ///
    /// This function is exposed to JavaScript via N-API.
    ///
    /// # Arguments
    ///
    /// * `env` - The N-API environment.
    /// * `js_obj` - The JavaScript object representing the PtyProcess.
    ///
    /// # Returns
    ///
    /// Returns the PID of the PTY process.
    ///
    /// # Errors
    ///
    /// Returns a `napi::Error` if any error occurs.
    #[napi]
    pub fn pid(env: Env, js_obj: JsObject) -> napi::Result<i32> {
        let pty_process: PtyProcess = PtyProcess::from_js_object(&env, &js_obj)?;
        Ok(pty_process.child.id() as i32)
    }

    /// Spawns a new shell process for the PTY.
    ///
    /// # Arguments
    ///
    /// * `shell_path` - The path to the new shell executable.
    ///
    /// # Returns
    ///
    /// An `io::Result` indicating success or failure.
    ///
    /// # Errors
    ///
    /// Returns an `io::Error` if spawning the new shell fails.
    fn spawn_new_shell(&mut self, shell_path: String) -> io::Result<()> {
        info!("Spawning new shell: {}", shell_path);

        // Re-initialize Winpty
        let config = winpty::Config::new(
            winpty::MouseMode::Auto,
            winpty::AgentConfig::default(),
            80,
            24,
        )?;
        self.winpty = Winpty::open(&config)?;

        // Prepare command to execute
        let mut cmd = Command::new(shell_path.clone());

        cmd.stdin(Stdio::from(self.winpty.conin()?));
        cmd.stdout(Stdio::from(self.winpty.conout()?));
        cmd.stderr(Stdio::from(self.winpty.conerr()?));

        // Spawn the child process
        self.child = cmd.spawn()?;
        debug!("Spawned new child process with PID: {}", self.child.id());

        self.command = shell_path;
        Ok(())
    }

    /// Checks if the PTY process is still running.
    ///
    /// # Returns
    ///
    /// `true` if the process is running, `false` otherwise.
    fn is_running(&self) -> io::Result<bool> {
        match self.child.try_wait()? {
            Some(_status) => Ok(false),
            None => Ok(true),
        }
    }
}

/// Maps a generic error into a `napi::Error`.
///
/// # Arguments
///
/// * `err` - The error to map.
///
/// # Returns
///
/// A `napi::Error` with the provided error message.
fn map_to_napi_error<E: std::fmt::Display>(err: E) -> NapiError {
    NapiError::from_reason(err.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use bytes::Bytes;
    use std::sync::Once;

    static INIT: Once = Once::new();

    fn init_logger() {
        INIT.call_once(|| {
            env_logger::builder().is_test(true).try_init().ok();
        });
    }

    #[test]
    fn test_pty_process_creation() {
        init_logger();
        let result = PtyProcess::new();
        assert!(result.is_ok());
        let mut pty = result.unwrap();
        assert!(pty.child.id() > 0);
        assert_eq!(pty.command, "cmd.exe".to_string());

        // Cleanup
        let _ = pty.shutdown_pty();
    }

    #[test]
    fn test_write_and_read_data() {
        init_logger();
        let mut pty = PtyProcess::new().expect("Failed to create PtyProcess");

        let test_data = Bytes::from("echo Hello, PTY!\r\n");
        let write_result = pty.write_data(&test_data);
        assert!(write_result.is_ok());

        // Allow some time for the command to execute
        std::thread::sleep(std::time::Duration::from_millis(100));

        let read_result = pty.read_data();
        assert!(read_result.is_ok());
        let output = String::from_utf8_lossy(&read_result.unwrap());
        assert!(output.contains("Hello, PTY!"));

        // Cleanup
        let _ = pty.shutdown_pty();
    }

    #[test]
    fn test_resize_pty() {
        init_logger();
        let mut pty = PtyProcess::new().expect("Failed to create PtyProcess");

        let resize_result = pty.resize(100, 40);
        assert!(resize_result.is_ok());

        // Since cmd.exe does not provide a straightforward way to verify the terminal size,
        // we'll assume the resize was successful if no error was returned.

        // Cleanup
        let _ = pty.shutdown_pty();
    }

    #[test]
    fn test_set_env() {
        init_logger();
        let mut pty = PtyProcess::new().expect("Failed to create PtyProcess");

        let key = "TEST_ENV_VAR".to_string();
        let value = "12345".to_string();
        let set_env_result = pty.set_env(key.clone(), value.clone());
        assert!(set_env_result.is_ok());

        // Send a command to print the environment variable
        let test_command = Bytes::from(format!("echo %{}%\r\n", key));
        let write_result = pty.write_data(&test_command);
        assert!(write_result.is_ok());

        // Allow some time for the command to execute
        std::thread::sleep(std::time::Duration::from_millis(100));

        let read_result = pty.read_data();
        assert!(read_result.is_ok());
        let output = String::from_utf8_lossy(&read_result.unwrap());
        assert!(output.contains(&value));

        // Cleanup
        let _ = pty.shutdown_pty();
    }

    #[test]
    fn test_status() {
        init_logger();
        let mut pty = PtyProcess::new().expect("Failed to create PtyProcess");

        let status_result = pty.status();
        assert!(status_result.is_ok());
        let status = status_result.unwrap();
        assert_eq!(status, "Running");

        // Cleanup
        let _ = pty.shutdown_pty();

        // After shutdown, status should indicate the process is not running
        let status_after = pty.status();
        assert!(status_after.is_ok());
        assert_eq!(status_after.unwrap(), "Not Running");
    }

    #[test]
    fn test_set_log_level() {
        init_logger();
        let pty = PtyProcess::new().expect("Failed to create PtyProcess");

        let levels = vec![
            "error".to_string(),
            "warn".to_string(),
            "info".to_string(),
            "debug".to_string(),
            "trace".to_string(),
            "invalid".to_string(),
        ];

        for level in levels {
            let set_level_result = pty.set_log_level(level.clone());
            assert!(set_level_result.is_ok());
        }

        // Cleanup
        let _ = pty.shutdown_pty();
    }

    #[test]
    fn test_change_shell() {
        init_logger();
        let mut pty = PtyProcess::new().expect("Failed to create PtyProcess");

        // Change to PowerShell
        let change_shell_result = pty.change_shell("powershell.exe".to_string());
        assert!(change_shell_result.is_ok());
        assert_eq!(pty.command, "powershell.exe".to_string());

        // Send a command to verify the shell has changed
        let test_command = Bytes::from("Write-Host 'Shell Changed'\r\n");
        let write_result = pty.write_data(&test_command);
        assert!(write_result.is_ok());

        // Allow some time for the command to execute
        std::thread::sleep(std::time::Duration::from_millis(100));

        let read_result = pty.read_data();
        assert!(read_result.is_ok());
        let output = String::from_utf8_lossy(&read_result.unwrap());
        assert!(output.contains("Shell Changed"));

        // Cleanup
        let _ = pty.shutdown_pty();
    }

    #[test]
    fn test_into_js_object_and_from_js_object() {
        // Skipping implementation as it requires a real N-API environment.
    }
}
