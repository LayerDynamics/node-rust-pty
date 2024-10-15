// src/pty/multiplexer.rs

use crate::pty::platform::PtyProcess;
use bytes::Bytes;
use libc::{SIGKILL, SIGTERM, WNOHANG};
use log::{error, info};
use napi::bindgen_prelude::FromNapiValue;
use napi::bindgen_prelude::*;
use napi::JsObject;
use napi_derive::napi; // Import the napi attribute macro
use parking_lot::Mutex;
use std::collections::HashMap;
use std::env;
use std::io::{self};
use std::sync::Arc; // Import JsObject

/// Helper function to convert `io::Error` to a `napi::Error`.
///
/// This function takes an `io::Error` and converts it into a `napi::Error` for proper error handling.
fn convert_io_error(err: io::Error) -> napi::Error {
  napi::Error::from_reason(err.to_string())
}

/// Represents a session within the multiplexer.
///
/// Each `PtySession` maintains its own input and output buffers, allowing multiple virtual
/// streams to interact with the same PTY process independently.
#[derive(Debug, Clone)]
pub struct PtySession {
  /// Unique identifier for the session.
  pub stream_id: u32,
  /// Buffer for storing input data specific to the session.
  pub input_buffer: Vec<u8>,
  /// Shared buffer for storing output data, protected by a mutex for thread-safe access.
  pub output_buffer: Arc<Mutex<Vec<u8>>>,
}

/// Struct to represent session data returned to JavaScript.
#[napi(object)]
pub struct SessionData {
  pub session_id: u32,
  pub data: Buffer,
}

/// Multiplexer to handle multiple virtual streams on the same PTY.
///
/// The `PtyMultiplexer` manages multiple `PtySession`s, allowing concurrent interactions
/// with a single PTY process. It provides functionalities to create, merge, split, and
/// manage sessions, as well as to handle PTY operations such as reading, writing, and
/// broadcasting data.
#[napi]
#[derive(Debug, Clone)]
pub struct PtyMultiplexer {
  #[napi(skip)]
  pub pty_process: Arc<PtyProcess>,
  #[napi(skip)]
  pub sessions: Arc<Mutex<HashMap<u32, PtySession>>>,
}

#[napi]
impl PtyMultiplexer {
  /// Creates a new `PtyMultiplexer` with the given `PtyProcess`.
  ///
  /// # Parameters
  ///
  /// - `pty_process`: The `PtyProcess` instance to be managed by the multiplexer.
  ///
  /// # Examples
  ///
  /// ```javascript
  /// const { PtyMultiplexer } = require('./path_to_generated_napi_module');
  /// const ptyProcess = new PtyProcess();
  /// const multiplexer = new PtyMultiplexer(ptyProcess);
  /// ```
  #[napi(constructor)]
  pub fn new(pty_process: JsObject) -> Result<Self> {
    // Pass the pty_process as a reference (&pty_process) to from_js_object
    let pty_process: PtyProcess = PtyProcess::from_js_object(&pty_process)?;

    Ok(Self {
      pty_process: Arc::new(pty_process),
      sessions: Arc::new(Mutex::new(HashMap::new())),
    })
  }

  /// Creates a new session and returns its stream ID.
  ///
  /// This method generates a unique stream ID for the new session, initializes its input and
  /// output buffers, and registers it within the multiplexer.
  ///
  /// # Returns
  ///
  /// - `u32`: The unique stream ID of the newly created session.
  ///
  /// # Examples
  ///
  /// ```javascript
  /// const sessionId = multiplexer.createSession();
  /// console.log(`Created session with ID: ${sessionId}`);
  /// ```
  #[napi]
  pub fn create_session(&self) -> u32 {
    let mut sessions = self.sessions.lock();
    let stream_id = if let Some(max_id) = sessions.keys().max() {
      max_id + 1
    } else {
      1
    };
    sessions.insert(
      stream_id,
      PtySession {
        stream_id,
        input_buffer: Vec::new(),
        output_buffer: Arc::new(Mutex::new(Vec::new())),
      },
    );
    info!("Created new session with ID: {}", stream_id);
    stream_id
  }

  /// Sends data to a specific session.
  ///
  /// This method appends the provided data to the input buffer of the specified session
  /// and writes it to the PTY process.
  ///
  /// # Parameters
  ///
  /// - `session_id`: The unique identifier of the session to which data will be sent.
  /// - `data`: The data to be sent to the session.
  ///
  /// # Returns
  ///
  /// - `Result<(), napi::Error>`: Returns `Ok` if the operation is successful, otherwise returns an error.
  ///
  /// # Examples
  ///
  /// ```javascript
  /// multiplexer.sendToSession(sessionId, Buffer.from('Hello, Session!'));
  /// ```
  #[napi]
  pub fn send_to_session(&self, session_id: u32, data: Buffer) -> Result<()> {
    let data_slice = data.as_ref();
    let mut sessions = self.sessions.lock();
    if let Some(session) = sessions.get_mut(&session_id) {
      session.input_buffer.extend_from_slice(data_slice);

      // Convert &[u8] to Bytes
      let bytes_data = Bytes::copy_from_slice(data_slice);

      self
        .pty_process
        .write_data(&bytes_data) // Pass &Bytes
        .map_err(convert_io_error)?;
      info!("Sent data to session {}: {:?}", session_id, data_slice);
      Ok(())
    } else {
      Err(napi::Error::from_reason(format!(
        "Session ID {} not found",
        session_id
      )))
    }
  }

  /// Broadcasts data to all active sessions.
  ///
  /// This method appends the provided data to the input buffer of all active sessions
  /// and writes it to the PTY process.
  ///
  /// # Parameters
  ///
  /// - `data`: The data to be broadcasted to all sessions.
  ///
  /// # Returns
  ///
  /// - `Result<(), napi::Error>`: Returns `Ok` if the operation is successful, otherwise returns an error.
  ///
  /// # Examples
  ///
  /// ```javascript
  /// multiplexer.broadcast(Buffer.from('Hello, All Sessions!'));
  /// ```
  #[napi]
  pub fn broadcast(&self, data: Buffer) -> Result<()> {
    let data_slice = data.as_ref();
    let mut sessions = self.sessions.lock();
    for session in sessions.values_mut() {
      session.input_buffer.extend_from_slice(data_slice);
    }

    // Convert &[u8] to Bytes
    let bytes_data = Bytes::copy_from_slice(data_slice);

    self
      .pty_process
      .write_data(&bytes_data) // Pass &Bytes
      .map_err(convert_io_error)?;
    info!("Broadcasted data to all sessions: {:?}", data_slice);
    Ok(())
  }

  /// Reads data from a specific session.
  ///
  /// This method retrieves and clears the output buffer of the specified session,
  /// returning the accumulated data.
  ///
  /// # Parameters
  ///
  /// - `session_id`: The unique identifier of the session from which data will be read.
  ///
  /// # Returns
  ///
  /// - `Result<Buffer, napi::Error>`: Returns a `Buffer` containing the read data if successful, otherwise returns an error.
  ///
  /// # Examples
  ///
  /// ```javascript
  /// const data = multiplexer.readFromSession(sessionId);
  /// console.log(`Data from session ${sessionId}:`, data.toString());
  /// ```
  #[napi]
  pub fn read_from_session(&self, session_id: u32) -> Result<Buffer> {
    let sessions = self.sessions.lock();
    if let Some(session) = sessions.get(&session_id) {
      let mut output = session.output_buffer.lock();
      let data = output.clone();
      output.clear(); // Clear the buffer after reading
      info!("Read data from session {}: {:?}", session_id, data);
      Ok(Buffer::from(data))
    } else {
      Err(napi::Error::from_reason(format!(
        "Session ID {} not found",
        session_id
      )))
    }
  }

  /// Reads data from all sessions.
  ///
  /// This method retrieves and clears the output buffers of all active sessions,
  /// returning a vector of `SessionData` objects.
  ///
  /// # Returns
  ///
  /// - `Result<Vec<SessionData>, napi::Error>`: Returns a vector of `SessionData` objects mapping session IDs to their read data if successful, otherwise returns an error.
  ///
  /// # Examples
  ///
  /// ```javascript
  /// const allData = multiplexer.readAllSessions();
  /// for (const session of allData) {
  ///     console.log(`Data from session ${session.session_id}:`, session.data.toString());
  /// }
  /// ```
  #[napi]
  pub fn read_all_sessions(&self) -> Result<Vec<SessionData>> {
    let sessions = self.sessions.lock();
    let mut result = Vec::new();
    for (id, session) in sessions.iter() {
      let output = session.output_buffer.lock().clone();
      result.push(SessionData {
        session_id: *id,
        data: Buffer::from(output),
      });
    }
    info!("Read data from all sessions.");
    Ok(result)
  }

  /// Removes a specific session.
  ///
  /// This method deletes the specified session from the multiplexer, freeing up resources.
  ///
  /// # Parameters
  ///
  /// - `session_id`: The unique identifier of the session to be removed.
  ///
  /// # Returns
  ///
  /// - `Result<(), napi::Error>`: Returns `Ok` if the operation is successful, otherwise returns an error.
  ///
  /// # Examples
  ///
  /// ```javascript
  /// multiplexer.removeSession(sessionId);
  /// ```
  #[napi]
  pub fn remove_session(&self, session_id: u32) -> Result<()> {
    let mut sessions = self.sessions.lock();
    if sessions.remove(&session_id).is_some() {
      info!("Removed session with ID: {}", session_id);
      Ok(())
    } else {
      Err(napi::Error::from_reason(format!(
        "Session ID {} not found",
        session_id
      )))
    }
  }

  /// Merges multiple sessions into one.
  ///
  /// This method combines the input and output buffers of the specified sessions into the primary session,
  /// and removes the merged sessions from the multiplexer.
  ///
  /// # Parameters
  ///
  /// - `session_ids`: An array of session IDs to be merged. The first ID in the array is considered the primary session.
  ///
  /// # Returns
  ///
  /// - `Result<(), napi::Error>`: Returns `Ok` if the operation is successful, otherwise returns an error.
  ///
  /// # Examples
  ///
  /// ```javascript
  /// multiplexer.mergeSessions([1, 2, 3]);
  /// ```
  #[napi]
  pub fn merge_sessions(&self, session_ids: Vec<u32>) -> Result<()> {
    if session_ids.is_empty() {
      return Err(napi::Error::from_reason(
        "No session IDs provided for merging".to_string(),
      ));
    }
    info!("Merging sessions: {:?}", session_ids);

    let mut sessions = self.sessions.lock();

    let primary_session_id = session_ids[0];
    let mut primary_session = sessions
      .get_mut(&primary_session_id)
      .ok_or_else(|| {
        napi::Error::from_reason(format!(
          "Primary session ID {} not found",
          primary_session_id
        ))
      })?
      .clone();

    // Collect sessions to be merged
    let sessions_to_merge: Vec<_> = session_ids
      .iter()
      .skip(1)
      .filter_map(|&session_id| {
        sessions
          .remove(&session_id)
          .map(|session| (session_id, session))
      })
      .collect();

    // Perform the merging operations
    for (session_id, session) in sessions_to_merge {
      // Merge input buffers
      primary_session
        .input_buffer
        .extend_from_slice(&session.input_buffer);

      // Merge output buffers
      let mut primary_output_buffer = primary_session.output_buffer.lock();
      let session_output_buffer = session.output_buffer.lock();
      primary_output_buffer.extend_from_slice(&session_output_buffer);
      drop(session_output_buffer); // Release lock before next iteration

      info!("Merged and removed session with ID: {}", session_id);
    }

    // Update the primary session
    sessions.insert(primary_session_id, primary_session);

    info!(
      "Successfully merged sessions {:?} into session {}",
      session_ids, primary_session_id
    );

    Ok(())
  }

  /// Splits a session into multiple sub-sessions.
  ///
  /// This method divides the input and output buffers of the specified session into two roughly equal parts,
  /// creating two new sessions and removing the original session from the multiplexer.
  ///
  /// # Parameters
  ///
  /// - `session_id`: The unique identifier of the session to be split.
  ///
  /// # Returns
  ///
  /// - `Result<[u32; 2], napi::Error>`: Returns an array containing the IDs of the two new sub-sessions if successful, otherwise returns an error.
  ///
  /// # Examples
  ///
  /// ```javascript
  /// const [newSession1, newSession2] = multiplexer.splitSession(sessionId);
  /// console.log(`Split into sessions ${newSession1} and ${newSession2}`);
  /// ```
  #[napi]
  pub fn split_session(&self, session_id: u32) -> Result<[u32; 2]> {
    info!("Splitting session: {}", session_id);

    let mut sessions = self.sessions.lock();

    let session = sessions
      .get(&session_id)
      .ok_or_else(|| napi::Error::from_reason(format!("Session ID {} not found", session_id)))?
      .clone();

    let input_half_size = session.input_buffer.len() / 2;
    let output_half_size = session.output_buffer.lock().len() / 2;

    // Create first new session
    let new_session_1_id = sessions.keys().max().cloned().unwrap_or(0) + 1;
    let new_session_1 = PtySession {
      stream_id: new_session_1_id,
      input_buffer: session.input_buffer[..input_half_size].to_vec(),
      output_buffer: Arc::new(Mutex::new(
        session.output_buffer.lock()[..output_half_size].to_vec(),
      )),
    };
    sessions.insert(new_session_1_id, new_session_1);

    // Create second new session
    let new_session_2_id = new_session_1_id + 1;
    let new_session_2 = PtySession {
      stream_id: new_session_2_id,
      input_buffer: session.input_buffer[input_half_size..].to_vec(),
      output_buffer: Arc::new(Mutex::new(
        session.output_buffer.lock()[output_half_size..].to_vec(),
      )),
    };
    sessions.insert(new_session_2_id, new_session_2);

    // Remove the original session
    sessions.remove(&session_id);

    info!(
      "Successfully split session {} into sessions {} and {}",
      session_id, new_session_1_id, new_session_2_id
    );

    Ok([new_session_1_id, new_session_2_id])
  }

  /// Sets an environment variable for the PTY process.
  ///
  /// This method updates the environment variable specified by `key` with the provided `value`.
  ///
  /// # Parameters
  ///
  /// - `key`: The name of the environment variable to set.
  /// - `value`: The value to assign to the environment variable.
  ///
  /// # Returns
  ///
  /// - `Result<(), napi::Error>`: Returns `Ok` if the operation is successful, otherwise returns an error.
  ///
  /// # Examples
  ///
  /// ```javascript
  /// multiplexer.setEnv('PATH', '/usr/bin');
  /// ```
  #[napi]
  pub fn set_env(&self, key: String, value: String) -> Result<()> {
    env::set_var(&key, &value);
    info!("Environment variable set: {} = {}", key, value);

    // Optionally, verify
    match env::var(&key) {
      Ok(v) if v == value => {
        info!("Successfully set environment variable: {} = {}", key, value);
        Ok(())
      }
      Ok(v) => {
        error!(
          "Mismatch when setting environment variable: {} = {}, but found {}",
          key, value, v
        );
        Err(napi::Error::from_reason(format!(
          "Failed to correctly set environment variable: {}",
          key
        )))
      }
      Err(e) => {
        error!(
          "Error setting environment variable: {} = {}. Error: {}",
          key, value, e
        );
        Err(napi::Error::from_reason(format!(
          "Error setting environment variable: {} = {}. Error: {}",
          key, value, e
        )))
      }
    }
  }

  /// Changes the shell of the PTY process.
  ///
  /// This method sends a command to the PTY process to replace the current shell with the specified one.
  ///
  /// # Parameters
  ///
  /// - `shell_path`: The file system path to the new shell executable.
  ///
  /// # Returns
  ///
  /// - `Result<(), napi::Error>`: Returns `Ok` if the operation is successful, otherwise returns an error.
  ///
  /// # Examples
  ///
  /// ```javascript
  /// multiplexer.changeShell('/bin/zsh');
  /// ```
  #[napi]
  pub fn change_shell(&self, shell_path: String) -> Result<()> {
    let command = format!("exec {}\n", shell_path);

    // Convert &str to Bytes
    let bytes_command = Bytes::from(command);

    self
      .pty_process
      .write_data(&bytes_command) // Pass &Bytes
      .map_err(convert_io_error)?;
    info!("Changed shell to: {}", shell_path);
    Ok(())
  }

  /// Retrieves the status of the PTY process.
  ///
  /// This method checks whether the PTY process is running or has terminated, and returns the status.
  ///
  /// # Returns
  ///
  /// - `Result<String, napi::Error>`: Returns a string indicating the status of the PTY process if successful, otherwise returns an error.
  ///
  /// # Examples
  ///
  /// ```javascript
  /// const status = multiplexer.status();
  /// console.log(`PTY Status: ${status}`);
  /// ```
  #[napi]
  pub fn status(&self) -> Result<String> {
    match self.pty_process.waitpid(WNOHANG) {
      Ok(0) => {
        info!("PTY status: Running");
        Ok("Running".to_string())
      }
      Ok(pid) => {
        info!("PTY terminated with PID {}", pid);
        Ok(format!("Terminated with PID {}", pid))
      }
      Err(e) => {
        error!("Error retrieving PTY status: {}", e);
        Err(convert_io_error(e))
      }
    }
  }

  /// Adjusts the logging level.
  ///
  /// This method sets the global logging level to the specified value.
  /// Valid levels include "error", "warn", "info", "debug", and "trace".
  ///
  /// # Parameters
  ///
  /// - `level`: The desired logging level.
  ///
  /// # Returns
  ///
  /// - `Result<(), napi::Error>`: Returns `Ok` if the operation is successful, otherwise returns an error.
  ///
  /// # Examples
  ///
  /// ```javascript
  /// multiplexer.setLogLevel('debug');
  /// ```
  #[napi]
  pub fn set_log_level(&self, level: String) -> Result<()> {
    crate::utils::logging::initialize_logging();
    match level.to_lowercase().as_str() {
      "error" => log::set_max_level(log::LevelFilter::Error),
      "warn" => log::set_max_level(log::LevelFilter::Warn),
      "info" => log::set_max_level(log::LevelFilter::Info),
      "debug" => log::set_max_level(log::LevelFilter::Debug),
      "trace" => log::set_max_level(log::LevelFilter::Trace),
      _ => {
        error!("Invalid log level: {}", level);
        return Err(napi::Error::from_reason("Invalid log level".to_string()));
      }
    };
    info!("Log level set to: {}", level);
    Ok(())
  }

  /// Closes all sessions within the multiplexer.
  ///
  /// This method removes all active sessions, effectively resetting the multiplexer.
  ///
  /// # Returns
  ///
  /// - `Result<(), napi::Error>`: Returns `Ok` if the operation is successful, otherwise returns an error.
  ///
  /// # Examples
  ///
  /// ```javascript
  /// multiplexer.closeAllSessions();
  /// ```
  #[napi]
  pub fn close_all_sessions(&self) -> Result<()> {
    let mut sessions = self.sessions.lock();
    sessions.clear();
    info!("All sessions have been closed.");
    Ok(())
  }

  /// Gracefully shuts down the PTY process and closes all sessions.
  ///
  /// This method sends a `SIGTERM` signal to the PTY process, waits for it to terminate,
  /// and closes all active sessions.
  ///
  /// # Returns
  ///
  /// - `Result<(), napi::Error>`: Returns `Ok` if the operation is successful, otherwise returns an error.
  ///
  /// # Examples
  ///
  /// ```javascript
  /// multiplexer.shutdownPty();
  /// ```
  #[napi]
  pub fn shutdown_pty(&self) -> Result<()> {
    self
      .pty_process
      .kill_process(SIGTERM)
      .map_err(convert_io_error)?;
    self
      .pty_process
      .waitpid(WNOHANG)
      .map_err(convert_io_error)?;
    self.close_all_sessions()?;
    info!("PTY has been gracefully shut down.");
    Ok(())
  }

  /// Forcefully shuts down the PTY process and closes all sessions.
  ///
  /// This method sends a `SIGKILL` signal to the PTY process, waits for it to terminate,
  /// and closes all active sessions.
  ///
  /// # Returns
  ///
  /// - `Result<(), napi::Error>`: Returns `Ok` if the operation is successful, otherwise returns an error.
  ///
  /// # Examples
  ///
  /// ```javascript
  /// multiplexer.forceShutdownPty();
  /// ```
  #[napi]
  pub fn force_shutdown_pty(&self) -> Result<()> {
    self
      .pty_process
      .kill_process(SIGKILL)
      .map_err(convert_io_error)?;
    self
      .pty_process
      .waitpid(WNOHANG)
      .map_err(convert_io_error)?;
    self.close_all_sessions()?;
    info!("PTY has been forcefully shut down.");
    Ok(())
  }

  /// Distributes output from PTY to all sessions.
  ///
  /// This asynchronous method reads data from the PTY process and appends it to the output buffers
  /// of all active sessions.
  ///
  /// # Returns
  ///
  /// - `Result<(), napi::Error>`: Returns `Ok` if the operation is successful, otherwise returns an error.
  ///
  /// # Examples
  ///
  /// ```javascript
  /// await multiplexer.distributeOutput();
  /// ```
  #[napi]
  pub async fn distribute_output(&self) -> Result<()> {
    let mut buffer = [0u8; 4096];
    loop {
      match self.pty_process.read_data(&mut buffer) {
        Ok(0) => break,
        Ok(n) => {
          let sessions = self.sessions.lock();
          for session in sessions.values() {
            let mut output = session.output_buffer.lock();
            output.extend_from_slice(&buffer[..n]);
          }
        }
        Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => break,
        Err(e) => return Err(convert_io_error(e)),
      }
    }
    info!("Distributed output to all sessions.");
    Ok(())
  }

  /// Asynchronously reads data from PTY.
  ///
  /// This method reads available data from the PTY process and returns it as a byte array.
  ///
  /// # Returns
  ///
  /// - `Result<Buffer, napi::Error>`: Returns a `Buffer` containing the read data if successful, otherwise returns an error.
  ///
  /// # Examples
  ///
  /// ```javascript
  /// const data = await multiplexer.readFromPty();
  /// console.log(`Data from PTY: ${data.toString()}`);
  /// ```
  #[napi]
  pub async fn read_from_pty(&self) -> Result<Buffer> {
    let mut buffer = [0u8; 4096];
    match self.pty_process.read_data(&mut buffer) {
      Ok(n) if n > 0 => {
        let data = buffer[..n].to_vec();
        info!("Read {} bytes from PTY.", n);
        Ok(Buffer::from(data))
      }
      Ok(_) => {
        // No data read
        Ok(Buffer::from(Vec::new()))
      }
      Err(e) => {
        error!("Error reading from PTY: {}", e);
        Err(convert_io_error(e))
      }
    }
  }

  /// Asynchronously dispatches output to the appropriate sessions.
  ///
  /// This method processes the provided output data and appends it to the output buffers
  /// of all active sessions. Currently, it broadcasts the data to all sessions.
  ///
  /// # Parameters
  ///
  /// - `output`: The data to be dispatched to the sessions.
  ///
  /// # Returns
  ///
  /// - `Result<(), napi::Error>`: Returns `Ok` if the operation is successful, otherwise returns an error.
  ///
  /// # Examples
  ///
  /// ```javascript
  /// await multiplexer.dispatchOutput(Buffer.from('New output data'));
  /// ```
  #[napi]
  pub async fn dispatch_output(&self, output: Buffer) -> Result<()> {
    let output_vec = output.as_ref().to_vec();

    // For simplicity, we'll broadcast the output to all sessions.
    // You can implement more sophisticated routing based on your requirements.

    let sessions = self.sessions.lock();
    for session in sessions.values() {
      let mut output_buffer = session.output_buffer.lock();
      output_buffer.extend_from_slice(&output_vec);
    }

    info!("Dispatched output to all sessions.");
    Ok(())
  }
}
