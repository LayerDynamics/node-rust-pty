// src/pty/multiplexer.rs

use crate::pty::platform::PtyProcess;
use bytes::Bytes;
use libc::{SIGKILL, SIGTERM, WNOHANG};
use log::{error, info};
use napi::bindgen_prelude::*;
use napi::JsObject;
use napi_derive::napi;
use std::collections::HashMap;
use std::env;
use std::io;
use std::sync::Arc;
use tokio::sync::{broadcast, Mutex as TokioMutex};

/// Helper function to convert `io::Error` to a `napi::Error`.
///
/// This function is used to map Rust `io::Error` into JavaScript-compatible `napi::Error`.
///
/// # Parameters
///
/// - `err`: The `io::Error` to be converted.
///
/// # Returns
///
/// A `napi::Error` containing the error message.
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
  /// Shared buffer for storing output data, protected by an asynchronous mutex for thread-safe access.
  pub output_buffer: Arc<TokioMutex<Vec<u8>>>,
}

/// Struct to represent session data returned to JavaScript.
#[napi(object)]
pub struct SessionData {
  /// The unique identifier of the session.
  pub session_id: u32,
  /// The data associated with the session, represented as a `Buffer`.
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
  /// The PTY process managed by the multiplexer.
  #[napi(skip)]
  pub pty_process: Arc<PtyProcess>,
  /// A thread-safe hash map of active sessions.
  #[napi(skip)]
  pub sessions: Arc<TokioMutex<HashMap<u32, PtySession>>>,
  /// Sender for shutdown signals to background tasks.
  #[napi(skip)]
  pub shutdown_sender: broadcast::Sender<()>,
}

#[napi]
impl PtyMultiplexer {
  /// Creates a new `PtyMultiplexer` with the given `PtyProcess`.
  ///
  /// # Parameters
  ///
  /// - `env`: The N-API environment.
  /// - `pty_process`: The `JsObject` representing the `PtyProcess`.
  ///
  /// # Errors
  ///
  /// Returns a `napi::Error` if the PTY process fails to initialize.
  ///
  /// # Examples
  ///
  /// ```javascript
  /// const { PtyMultiplexer } = require('./path_to_generated_napi_module');
  /// const ptyProcess = new PtyProcess();
  /// const multiplexer = new PtyMultiplexer(ptyProcess);
  /// ```
  #[napi(constructor)]
  pub fn new(env: Env, pty_process: JsObject) -> Result<Self> {
    // Convert JsObject to PtyProcess
    let pty_process: PtyProcess = PtyProcess::from_js_object(&env, &pty_process)?;

    // Initialize broadcast channel for shutdown signals
    let (shutdown_sender, _) = broadcast::channel(1);

    let multiplexer = Self {
      pty_process: Arc::new(pty_process),
      sessions: Arc::new(TokioMutex::new(HashMap::new())),
      shutdown_sender: shutdown_sender.clone(),
    };

    // Spawn a background task to continuously read from PTY and dispatch to sessions
    let multiplexer_clone = multiplexer.clone();
    tokio::spawn(async move {
      let mut shutdown_receiver = multiplexer_clone.shutdown_sender.subscribe();
      loop {
        tokio::select! {
            res = multiplexer_clone.read_and_dispatch() => {
                if let Err(e) = res {
                    error!("Error in background PTY read task: {}", e);
                    break;
                }
            },
            _ = shutdown_receiver.recv() => {
                info!("Shutdown signal received. Terminating background PTY read task.");
                break;
            },
        }
      }
      info!("Background PTY read task terminated.");
    });

    Ok(multiplexer)
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
  pub async fn create_session(&self) -> Result<u32> {
    let mut sessions = self.sessions.lock().await;
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
        output_buffer: Arc::new(TokioMutex::new(Vec::new())),
      },
    );
    info!("Created new session with ID: {}", stream_id);
    Ok(stream_id)
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
  pub async fn send_to_session(&self, session_id: u32, data: Buffer) -> Result<()> {
    let data_slice = data.as_ref().to_vec();
    let mut sessions = self.sessions.lock().await;
    if let Some(session) = sessions.get_mut(&session_id) {
      session.input_buffer.extend_from_slice(&data_slice);
      drop(sessions); // Release the lock before writing to PTY

      // Convert Vec<u8> to Bytes
      let bytes_data = Bytes::copy_from_slice(&data_slice);

      self
        .pty_process
        .write_data(&bytes_data)
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
  pub async fn broadcast(&self, data: Buffer) -> Result<()> {
    let data_slice = data.as_ref().to_vec();
    let mut sessions = self.sessions.lock().await;
    for session in sessions.values_mut() {
      session.input_buffer.extend_from_slice(&data_slice);
    }
    drop(sessions); // Release the lock before writing to PTY

    // Convert Vec<u8> to Bytes
    let bytes_data = Bytes::copy_from_slice(&data_slice);

    self
      .pty_process
      .write_data(&bytes_data)
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
  pub async fn read_from_session(&self, session_id: u32) -> Result<Buffer> {
    let mut sessions = self.sessions.lock().await;
    if let Some(session) = sessions.get_mut(&session_id) {
      let mut output = session.output_buffer.lock().await;
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
  /// allData.forEach(session => {
  ///     console.log(`Session ${session.session_id}: ${session.data.toString()}`);
  /// });
  /// ```
  #[napi]
  pub async fn read_all_sessions(&self) -> Result<Vec<SessionData>> {
    let sessions = self.sessions.lock().await;
    let mut result = Vec::new();
    for (id, session) in sessions.iter() {
      let output = session.output_buffer.lock().await.clone();
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
  pub async fn remove_session(&self, session_id: u32) -> Result<()> {
    let mut sessions = self.sessions.lock().await;
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
  pub async fn merge_sessions(&self, session_ids: Vec<u32>) -> Result<()> {
    if session_ids.is_empty() {
      return Err(napi::Error::from_reason(
        "No session IDs provided for merging".to_string(),
      ));
    }
    info!("Merging sessions: {:?}", session_ids);

    let mut sessions = self.sessions.lock().await;

    let primary_session_id = session_ids[0];
    let primary_session = sessions
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
      let mut primary_session_input = primary_session.input_buffer.clone();
      primary_session_input.extend_from_slice(&session.input_buffer);
      drop(session.input_buffer); // Release ownership

      // Merge output buffers
      let mut primary_output_buffer = primary_session.output_buffer.lock().await;
      let session_output_buffer = session.output_buffer.lock().await;
      primary_output_buffer.extend_from_slice(&session_output_buffer);
      drop(session_output_buffer); // Release lock before next iteration

      info!("Merged and removed session with ID: {}", session_id);
    }

    // Update the primary session's input buffer
    if let Some(updated_primary) = sessions.get_mut(&primary_session_id) {
      updated_primary.input_buffer = primary_session.input_buffer.clone();
      let mut primary_output_buffer = updated_primary.output_buffer.lock().await;
      *primary_output_buffer = primary_session.output_buffer.lock().await.clone();
    }

    info!(
      "Successfully merged sessions {:?} into session {}",
      session_ids, primary_session_id
    );

    Ok(())
  }

  /// Splits a session into two separate sessions.
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
  pub async fn split_session(&self, session_id: u32) -> Result<[u32; 2]> {
    info!("Splitting session: {}", session_id);

    let mut sessions = self.sessions.lock().await;

    let session = sessions
      .get(&session_id)
      .ok_or_else(|| napi::Error::from_reason(format!("Session ID {} not found", session_id)))?
      .clone();

    let input_half_size = session.input_buffer.len() / 2;
    let output_half_size = {
      let output = session.output_buffer.lock().await;
      output.len() / 2
    };

    // Create first new session
    let new_session_1_id = sessions.keys().max().cloned().unwrap_or(0) + 1;
    let new_session_1 = PtySession {
      stream_id: new_session_1_id,
      input_buffer: session.input_buffer[..input_half_size].to_vec(),
      output_buffer: Arc::new(TokioMutex::new(
        session.output_buffer.lock().await[..output_half_size].to_vec(),
      )),
    };
    sessions.insert(new_session_1_id, new_session_1);

    // Create second new session
    let new_session_2_id = new_session_1_id + 1;
    let new_session_2 = PtySession {
      stream_id: new_session_2_id,
      input_buffer: session.input_buffer[input_half_size..].to_vec(),
      output_buffer: Arc::new(TokioMutex::new(
        session.output_buffer.lock().await[output_half_size..].to_vec(),
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
  pub async fn set_env(&self, key: String, value: String) -> Result<()> {
    // Attempt to set the environment variable.
    env::set_var(&key, &value);
    info!("Environment variable set: {} = {}", key, value);

    // Verify that the environment variable was set correctly.
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
  pub async fn change_shell(&self, shell_path: String) -> Result<()> {
    let command = format!("exec {}\n", shell_path);

    // Convert String to Bytes
    let bytes_command = Bytes::copy_from_slice(command.as_bytes());

    self
      .pty_process
      .write_data(&bytes_command)
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
  pub async fn status(&self) -> Result<String> {
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
  pub async fn set_log_level(&self, level: String) -> Result<()> {
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
  pub async fn close_all_sessions(&self) -> Result<()> {
    let mut sessions = self.sessions.lock().await;
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
  pub async fn shutdown_pty(&self) -> Result<()> {
    self
      .pty_process
      .kill_process(SIGTERM)
      .map_err(convert_io_error)?;
    self
      .pty_process
      .waitpid(WNOHANG)
      .map_err(convert_io_error)?;
    self.close_all_sessions().await?;
    info!("PTY has been gracefully shut down.");
    // Send shutdown signal to background tasks
    let _ = self.shutdown_sender.send(());
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
  pub async fn force_shutdown_pty(&self) -> Result<()> {
    self
      .pty_process
      .kill_process(SIGKILL)
      .map_err(convert_io_error)?;
    self
      .pty_process
      .waitpid(WNOHANG)
      .map_err(convert_io_error)?;
    self.close_all_sessions().await?;
    info!("PTY has been forcefully shut down.");
    // Send shutdown signal to background tasks
    let _ = self.shutdown_sender.send(());
    Ok(())
  }

  /// Resizes the PTY window.
  ///
  /// This method adjusts the number of columns and rows of the PTY, effectively changing the terminal size.
  ///
  /// # Parameters
  ///
  /// - `cols`: The number of columns for the PTY window.
  /// - `rows`: The number of rows for the PTY window.
  ///
  /// # Returns
  ///
  /// - `Result<(), napi::Error>`: Returns `Ok` if the resize operation is successful, otherwise returns an error.
  ///
  /// # Examples
  ///
  /// ```javascript
  /// multiplexer.resize(120, 40);
  /// ```
  #[napi]
  pub async fn resize(&self, cols: u16, rows: u16) -> Result<()> {
    self
      .pty_process
      .resize(cols, rows)
      .map_err(convert_io_error)?;
    info!("Resized PTY to cols: {}, rows: {}", cols, rows);
    Ok(())
  }

  /// Dispatches the provided output data to all active sessions' output buffers.
  ///
  /// # Parameters
  ///
  /// - `data`: A reference to the data to be dispatched.
  ///
  /// # Returns
  ///
  /// - `Result<(), napi::Error>`: Returns `Ok` if the operation is successful, otherwise returns an error.
  pub async fn dispatch_output(&self, data: &[u8]) -> Result<()> {
    let sessions = self.sessions.lock().await;
    for session in sessions.values() {
      let mut output_buffer = session.output_buffer.lock().await;
      output_buffer.extend_from_slice(data);
    }
    info!("Dispatched {} bytes to all sessions.", data.len());
    Ok(())
  }
  /// Lists all active session IDs.
  ///
  /// This method retrieves the unique identifiers of all active sessions managed by the multiplexer.
  ///
  /// # Returns
  ///
  /// - `Result<Vec<u32>, napi::Error>`: Returns a vector of session IDs if successful, otherwise returns an error.
  ///
  /// # Examples
  ///
  /// ```javascript
  /// const sessionIds = multiplexer.listSessions();
  /// console.log(`Active sessions: ${sessionIds.join(', ')}`);
  /// ```
  #[napi]
  pub async fn list_sessions(&self) -> Result<Vec<u32>> {
    let sessions = self.sessions.lock().await;
    let session_ids: Vec<u32> = sessions.keys().cloned().collect();
    info!("Listing all active sessions: {:?}", session_ids);
    Ok(session_ids)
  }

  /// Handles the creation of a new session.
  ///
  /// This method is a wrapper around the `create_session` method, providing an asynchronous
  /// interface for creating a new session.
  ///
  /// # Returns
  ///
  /// - `Result<u32, napi::Error>`: Returns the unique stream ID of the newly created session if successful, otherwise returns an error.
  ///
  /// # Examples
  ///
  /// ```javascript
  /// const sessionId = await multiplexer.handleCreateSession();
  /// console.log(`Created session with ID: ${sessionId}`);
  /// ```
  #[napi]
  pub async fn handle_create_session(&self) -> Result<u32> {
    self.create_session().await
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

  /// Reads data from the PTY and dispatches it to all active sessions.
  ///
  /// This asynchronous method is intended to be run within a background task.
  ///
  /// # Returns
  ///
  /// - `Result<(), io::Error>`: Returns `Ok` if the operation is successful, otherwise returns an error.
  pub async fn read_and_dispatch(&self) -> Result<()> {
    let mut buffer = [0u8; 4096];
    match self.pty_process.read_data(&mut buffer) {
      Ok(n) if n > 0 => {
        let data = buffer[..n].to_vec();
        info!("Read {} bytes from PTY.", n);
        self.dispatch_output(&data).await?;
      }
      Ok(_) => {
        // No more data to read at the moment
      }
      Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {
        // No data available right now
      }
      Err(e) => {
        error!("Error reading from PTY: {}", e);
        return Err(convert_io_error(e));
      }
    }
    Ok(())
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use std::time::Duration;
  use tokio::time;

  use once_cell::sync::OnceCell;

  static INIT: OnceCell<()> = OnceCell::new();

  fn init_logger() {
    INIT.get_or_init(|| {
      env_logger::builder()
        .filter_level(log::LevelFilter::Debug)
        .is_test(true)
        .try_init()
        .ok();
    });
  }

  #[tokio::test]
  async fn test_create_pty_multiplexer() {
    init_logger();
    // Create a mock PtyProcess
    let pty_process = PtyProcess::new().expect("Failed to create PtyProcess");
    let multiplexer = PtyMultiplexer {
      pty_process: Arc::new(pty_process),
      sessions: Arc::new(TokioMutex::new(HashMap::new())),
      shutdown_sender: broadcast::channel(1).0, // Dummy sender for testing
    };

    // Spawn the background read and dispatch task
    let multiplexer_clone = multiplexer.clone();
    tokio::spawn(async move {
      let mut shutdown_receiver = multiplexer_clone.shutdown_sender.subscribe();
      loop {
        tokio::select! {
            res = multiplexer_clone.read_and_dispatch() => {
                if let Err(e) = res {
                    error!("Error in background PTY read task: {}", e);
                    break;
                }
            },
            _ = shutdown_receiver.recv() => {
                info!("Shutdown signal received. Terminating background PTY read task.");
                break;
            },
        }
      }
      info!("Background PTY read task terminated.");
    });

    // Create a session
    let session_id = multiplexer
      .create_session()
      .await
      .expect("Failed to create session");
    assert_eq!(session_id, 1);

    // Send data to the session
    multiplexer
      .send_to_session(session_id, Buffer::from("echo test\n".as_bytes()))
      .await
      .expect("Failed to send data to session");

    // Allow some time for the background task to read and dispatch
    time::sleep(Duration::from_millis(100)).await;

    // Read from the session
    let output = multiplexer
      .read_from_session(session_id)
      .await
      .expect("Failed to read from session");
    let output_str = String::from_utf8_lossy(&output).to_string();

    // The output should contain both the echoed command and the response
    assert!(output_str.contains("echo test"));
    // Depending on shell configuration, the response may vary

    // Shutdown multiplexer
    multiplexer.shutdown_sender.send(()).ok();
  }

  #[tokio::test]
  async fn test_resize_pty() {
    init_logger();
    let pty_process = PtyProcess::new().expect("Failed to create PtyProcess");
    let multiplexer = PtyMultiplexer {
      pty_process: Arc::new(pty_process),
      sessions: Arc::new(TokioMutex::new(HashMap::new())),
      shutdown_sender: broadcast::channel(1).0, // Dummy sender for testing
    };

    // Spawn the background read and dispatch task
    let multiplexer_clone = multiplexer.clone();
    tokio::spawn(async move {
      let mut shutdown_receiver = multiplexer_clone.shutdown_sender.subscribe();
      loop {
        tokio::select! {
            res = multiplexer_clone.read_and_dispatch() => {
                if let Err(e) = res {
                    error!("Error in background PTY read task: {}", e);
                    break;
                }
            },
            _ = shutdown_receiver.recv() => {
                info!("Shutdown signal received. Terminating background PTY read task.");
                break;
            },
        }
      }
      info!("Background PTY read task terminated.");
    });

    // Create a session
    let session_id = multiplexer
      .create_session()
      .await
      .expect("Failed to create session");
    assert_eq!(session_id, 1);

    // Resize the PTY
    multiplexer
      .resize(40, 100)
      .await
      .expect("Failed to resize PTY");

    // Send 'stty size' command to verify resize
    multiplexer
      .send_to_session(session_id, Buffer::from("stty size\n".as_bytes()))
      .await
      .expect("Failed to send 'stty size' command");

    // Allow some time for the background task to read and dispatch
    time::sleep(Duration::from_millis(100)).await;

    // Read from the session
    let output = multiplexer
      .read_from_session(session_id)
      .await
      .expect("Failed to read from session");
    let output_str = String::from_utf8_lossy(&output).to_string();

    // Split the output into lines
    let lines: Vec<&str> = output_str.trim().split('\n').collect();
    // The first line is the echoed command, the second line should be the size
    if lines.len() >= 2 {
      let size_output = lines[1];
      assert!(
        size_output.contains("40 100"),
        "Incorrect terminal size after resize. Expected '40 100', got '{}'",
        size_output
      );
    } else {
      panic!(
        "Terminal size output incomplete. Expected at least 2 lines, got {}",
        lines.len()
      );
    }

    // Shutdown multiplexer
    multiplexer.shutdown_sender.send(()).ok();
  }

  #[tokio::test]
  async fn test_multiplexer_basic() {
    init_logger();
    let pty_process = PtyProcess::new().expect("Failed to create PtyProcess");
    let multiplexer = PtyMultiplexer {
      pty_process: Arc::new(pty_process),
      sessions: Arc::new(TokioMutex::new(HashMap::new())),
      shutdown_sender: broadcast::channel(1).0, // Dummy sender for testing
    };

    // Spawn the background read and dispatch task
    let multiplexer_clone = multiplexer.clone();
    tokio::spawn(async move {
      let mut shutdown_receiver = multiplexer_clone.shutdown_sender.subscribe();
      loop {
        tokio::select! {
            res = multiplexer_clone.read_and_dispatch() => {
                if let Err(e) = res {
                    error!("Error in background PTY read task: {}", e);
                    break;
                }
            },
            _ = shutdown_receiver.recv() => {
                info!("Shutdown signal received. Terminating background PTY read task.");
                break;
            },
        }
      }
      info!("Background PTY read task terminated.");
    });

    // Create two sessions
    let session1 = multiplexer
      .create_session()
      .await
      .expect("Failed to create session 1");
    let session2 = multiplexer
      .create_session()
      .await
      .expect("Failed to create session 2");
    assert_eq!(session1, 1);
    assert_eq!(session2, 2);

    // Send different commands to each session
    multiplexer
      .send_to_session(session1, Buffer::from("echo session1 data\n".as_bytes()))
      .await
      .expect("Failed to send to session 1");
    multiplexer
      .send_to_session(session2, Buffer::from("echo session2 data\n".as_bytes()))
      .await
      .expect("Failed to send to session 2");

    // Allow some time for the background task to read and dispatch
    time::sleep(Duration::from_millis(200)).await;

    // Read from session 1
    let output1 = multiplexer
      .read_from_session(session1)
      .await
      .expect("Failed to read from session 1");
    let output1_str = String::from_utf8_lossy(&output1).to_string();

    // Read from session 2
    let output2 = multiplexer
      .read_from_session(session2)
      .await
      .expect("Failed to read from session 2");
    let output2_str = String::from_utf8_lossy(&output2).to_string();

    // Since outputs are broadcasted, both sessions should see both commands and their outputs
    assert!(output1_str.contains("echo session1 data"));
    assert!(output1_str.contains("session1 data"));
    assert!(output1_str.contains("echo session2 data"));
    assert!(output1_str.contains("session2 data"));

    assert!(output2_str.contains("echo session1 data"));
    assert!(output2_str.contains("session1 data"));
    assert!(output2_str.contains("echo session2 data"));
    assert!(output2_str.contains("session2 data"));

    // Shutdown multiplexer
    multiplexer.shutdown_sender.send(()).ok();
  }

  #[tokio::test]
  async fn test_set_env() {
    init_logger();
    let pty_process = PtyProcess::new().expect("Failed to create PtyProcess");
    let multiplexer = PtyMultiplexer {
      pty_process: Arc::new(pty_process),
      sessions: Arc::new(TokioMutex::new(HashMap::new())),
      shutdown_sender: broadcast::channel(1).0, // Dummy sender for testing
    };

    // Spawn the background read and dispatch task
    let multiplexer_clone = multiplexer.clone();
    tokio::spawn(async move {
      let mut shutdown_receiver = multiplexer_clone.shutdown_sender.subscribe();
      loop {
        tokio::select! {
            res = multiplexer_clone.read_and_dispatch() => {
                if let Err(e) = res {
                    error!("Error in background PTY read task: {}", e);
                    break;
                }
            },
            _ = shutdown_receiver.recv() => {
                info!("Shutdown signal received. Terminating background PTY read task.");
                break;
            },
        }
      }
      info!("Background PTY read task terminated.");
    });

    // Create a session
    let session_id = multiplexer
      .create_session()
      .await
      .expect("Failed to create session");
    assert_eq!(session_id, 1);

    // Set environment variable
    multiplexer
      .set_env("TEST_ENV_VAR".to_string(), "test_value".to_string())
      .await
      .expect("Failed to set environment variable");

    // Send command to print the environment variable
    multiplexer
      .send_to_session(session_id, Buffer::from("echo $TEST_ENV_VAR\n".as_bytes()))
      .await
      .expect("Failed to send echo command");

    // Allow some time for the background task to read and dispatch
    time::sleep(Duration::from_millis(100)).await;

    // Read from the session
    let output = multiplexer
      .read_from_session(session_id)
      .await
      .expect("Failed to read from session");
    let output_str = String::from_utf8_lossy(&output).to_string();

    // The output should contain both the echoed command and the environment variable value
    assert!(output_str.contains("echo $TEST_ENV_VAR"));
    assert!(output_str.contains("test_value"));

    // Shutdown multiplexer
    multiplexer.shutdown_sender.send(()).ok();
  }

  #[tokio::test]
  async fn test_change_shell() {
    init_logger();
    let pty_process = PtyProcess::new().expect("Failed to create PtyProcess");
    let multiplexer = PtyMultiplexer {
      pty_process: Arc::new(pty_process),
      sessions: Arc::new(TokioMutex::new(HashMap::new())),
      shutdown_sender: broadcast::channel(1).0, // Dummy sender for testing
    };

    // Spawn the background read and dispatch task
    let multiplexer_clone = multiplexer.clone();
    tokio::spawn(async move {
      let mut shutdown_receiver = multiplexer_clone.shutdown_sender.subscribe();
      loop {
        tokio::select! {
            res = multiplexer_clone.read_and_dispatch() => {
                if let Err(e) = res {
                    error!("Error in background PTY read task: {}", e);
                    break;
                }
            },
            _ = shutdown_receiver.recv() => {
                info!("Shutdown signal received. Terminating background PTY read task.");
                break;
            },
        }
      }
      info!("Background PTY read task terminated.");
    });

    // Create a session
    let session_id = multiplexer
      .create_session()
      .await
      .expect("Failed to create session");
    assert_eq!(session_id, 1);

    // Change shell to /bin/sh
    multiplexer
      .change_shell("/bin/sh".to_string())
      .await
      .expect("Failed to change shell");

    // Send a command to verify shell change
    multiplexer
      .send_to_session(session_id, Buffer::from("echo Shell Changed\n".as_bytes()))
      .await
      .expect("Failed to send echo command");

    // Allow some time for the background task to read and dispatch
    time::sleep(Duration::from_millis(200)).await;

    // Read from the session
    let output = multiplexer
      .read_from_session(session_id)
      .await
      .expect("Failed to read from session");
    let output_str = String::from_utf8_lossy(&output).to_string();

    // The output should contain both the echoed command and the response
    assert!(output_str.contains("echo Shell Changed"));
    assert!(output_str.contains("Shell Changed"));

    // Shutdown multiplexer
    multiplexer.shutdown_sender.send(()).ok();
  }

  #[tokio::test]
  async fn test_shutdown_pty() {
    init_logger();
    let pty_process = PtyProcess::new().expect("Failed to create PtyProcess");
    let multiplexer = PtyMultiplexer {
      pty_process: Arc::new(pty_process),
      sessions: Arc::new(TokioMutex::new(HashMap::new())),
      shutdown_sender: broadcast::channel(1).0, // Dummy sender for testing
    };

    // Spawn the background read and dispatch task
    let multiplexer_clone = multiplexer.clone();
    tokio::spawn(async move {
      let mut shutdown_receiver = multiplexer_clone.shutdown_sender.subscribe();
      loop {
        tokio::select! {
            res = multiplexer_clone.read_and_dispatch() => {
                if let Err(e) = res {
                    error!("Error in background PTY read task: {}", e);
                    break;
                }
            },
            _ = shutdown_receiver.recv() => {
                info!("Shutdown signal received. Terminating background PTY read task.");
                break;
            },
        }
      }
      info!("Background PTY read task terminated.");
    });

    // Create a session
    let session_id = multiplexer
      .create_session()
      .await
      .expect("Failed to create session");
    assert_eq!(session_id, 1);

    // Send a command
    multiplexer
      .send_to_session(
        session_id,
        Buffer::from("echo Before Shutdown\n".as_bytes()),
      )
      .await
      .expect("Failed to send command");

    // Allow some time for the background task to read and dispatch
    time::sleep(Duration::from_millis(100)).await;

    // Shutdown PTY gracefully
    multiplexer
      .shutdown_pty()
      .await
      .expect("Failed to shutdown PTY");

    // Attempt to send another command after shutdown
    let send_result = multiplexer
      .send_to_session(session_id, Buffer::from("echo After Shutdown\n".as_bytes()))
      .await;
    assert!(
      send_result.is_err(),
      "Should not be able to send data after shutdown"
    );

    // Read from the session
    let output = multiplexer
      .read_from_session(session_id)
      .await
      .expect("Failed to read from session after shutdown");
    let output_str = String::from_utf8_lossy(&output).to_string();

    // The output should contain the command before shutdown
    assert!(output_str.contains("echo Before Shutdown"));
    assert!(output_str.contains("Before Shutdown"));
  }
}
