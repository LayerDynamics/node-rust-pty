// src/pty/handle.rs

use crate::pty::command_handler::handle_commands;
use crate::pty::commands::{PtyCommand, PtyResult};
use crate::pty::multiplexer;
use crate::pty::multiplexer::PtyMultiplexer;
use crate::pty::platform::PtyProcess;
use crate::utils::logging::initialize_logging;
use bytes::Bytes;
use crossbeam_channel::{bounded, Sender};
use log::{error, info};
use napi::bindgen_prelude::*;
use napi_derive::napi;
use std::env;
use std::sync::Arc;
use tokio::sync::oneshot;
use tokio::sync::Mutex as TokioMutex;
use tokio::time::{timeout, Duration};

/// Helper function to convert any error that implements `std::fmt::Display` into a `napi::Error`.
///
/// This function is used to map Rust errors into JavaScript-compatible errors for N-API bindings.
///
/// # Parameters
///
/// - `e`: The error to be converted, which must implement the `Display` trait.
///
/// # Returns
///
/// A `napi::Error` containing the string representation of the original error.
fn map_to_napi_error<E: std::fmt::Display>(e: E) -> napi::Error {
  napi::Error::from_reason(e.to_string())
}

/// Represents the result of splitting a session into two separate sessions.
///
/// This struct is exposed to JavaScript and provides the IDs of the newly created sessions.
///
/// # Fields
///
/// - `session1`: The ID of the first new session.
/// - `session2`: The ID of the second new session.
#[napi(object)]
#[derive(Debug, Clone)]
pub struct SplitSessionResult {
  pub session1: u32,
  pub session2: u32,
}

/// Represents the data associated with a specific session.
///
/// This struct is exposed to JavaScript and includes the session ID and the data payload.
///
/// # Fields
///
/// - `session_id`: The unique identifier for the session.
/// - `data`: The data associated with the session, represented as a `String`.
#[napi(object)]
#[derive(Debug, Clone)]
pub struct SessionData {
  pub session_id: u32,
  pub data: String,
}

/// Handles multiplexer operations for PTY sessions.
///
/// This struct manages the `PtyMultiplexer` internally and provides methods to interact with PTY sessions.
///
/// The internal `multiplexer` field is kept private to prevent direct exposure and ensure encapsulation.
///
/// # Example
///
/// ```javascript
/// const { MultiplexerHandle } = require('your-module');
///
/// async function example() {
///   const multiplexer = MultiplexerHandle.new();
///   const sessionId = await multiplexer.createSession();
///   await multiplexer.sendToSession(sessionId, Buffer.from('Hello, PTY!'));
/// }
/// ```
#[napi]
pub struct MultiplexerHandle {
  multiplexer: Arc<PtyMultiplexer>,
}

#[napi]
impl MultiplexerHandle {
  /// Creates a new `MultiplexerHandle` with a fresh PTY process.
  ///
  /// Initializes logging, sets up the PTY process, and starts a background task to handle PTY output.
  ///
  /// # Errors
  ///
  /// Returns a `napi::Error` if the PTY process fails to initialize.
  ///
  /// # Example
  ///
  /// ```javascript
  /// const { MultiplexerHandle } = require('your-module');
  ///
  /// async function initialize() {
  ///   const multiplexer = await MultiplexerHandle.new();
  ///   // Use multiplexer methods...
  /// }
  /// ```
  #[napi(factory)]
  pub fn new(env: Env) -> Result<Self> {
    initialize_logging();
    info!("Creating new MultiplexerHandle");

    // Initialize the PTY process without arguments.
    let pty_process = PtyProcess::new().map_err(|e| {
      Error::new(
        Status::GenericFailure,
        format!("Failed to create PtyProcess: {}", e),
      )
    })?;

    // Convert PtyProcess to a JsObject.
    let pty_process_js = pty_process.into_js_object(&env)?;
    let multiplexer_result = PtyMultiplexer::new(pty_process_js)?;
    let multiplexer_arc = Arc::new(multiplexer_result);

    let multiplexer_clone = Arc::clone(&multiplexer_arc);

    tokio::spawn(async move {
      if let Some(multiplexer) = Some(multiplexer_clone.as_ref()) {
        loop {
          match multiplexer.read_from_pty().await {
            Ok(output) => {
              if let Err(e) = multiplexer.dispatch_output(output).await {
                error!("Failed to dispatch output: {}", e);
              }
            }
            Err(e) => {
              error!("Failed to read from PTY: {}", e);
              break;
            }
          }
        }
        info!("Background PTY reading task terminated");
      } else {
        error!("Failed to initialize multiplexer");
      }
    });

    Ok(MultiplexerHandle {
      multiplexer: Arc::clone(&multiplexer_arc),
    })
  }
  /// Creates a new session within the multiplexer.
  ///
  /// Each session represents an isolated environment within the PTY process.
  ///
  /// # Returns
  ///
  /// - `Ok(u32)`: The unique identifier of the newly created session.
  /// - `Err(napi::Error)`: If the session creation fails.
  ///
  /// # Example
  ///
  /// ```javascript
  /// const sessionId = await multiplexer.createSession();
  /// ```
  #[napi]
  pub async fn create_session(&self) -> Result<u32> {
    let session_id = {
      let multiplexer = Arc::clone(&self.multiplexer);
      tokio::task::spawn_blocking(move || Ok(multiplexer.create_session()))
        .await
        .map_err(|e| map_to_napi_error(format!("Task Join Error: {}", e)))?
        .map_err(|e: String| map_to_napi_error(e))?
    };
    Ok(session_id)
  }

  /// Sends data to a specific session within the multiplexer.
  ///
  /// This allows targeted communication with a particular session.
  ///
  /// # Parameters
  ///
  /// - `session_id`: The ID of the session to send data to.
  /// - `data`: The data to be sent, represented as a vector of bytes.
  ///
  /// # Errors
  ///
  /// Returns a `napi::Error` if sending data fails.
  ///
  /// # Example
  ///
  /// ```javascript
  /// const data = Buffer.from('Hello, Session!');
  /// await multiplexer.sendToSession(sessionId, data);
  /// ```
  #[napi]
  pub async fn send_to_session(&self, session_id: u32, data: Vec<u8>) -> Result<()> {
    let multiplexer = Arc::clone(&self.multiplexer);
    let data_clone = data.clone();
    let buffer = napi::bindgen_prelude::Buffer::from(data_clone);
    tokio::task::spawn_blocking(move || multiplexer.send_to_session(session_id, buffer))
      .await
      .map_err(|e| map_to_napi_error(format!("Task Join Error: {}", e)))?
      .map_err(|e: napi::Error| map_to_napi_error(e))?;
    Ok(())
  }

  /// Sends data directly to the PTY without targeting a specific session.
  ///
  /// Useful for sending global commands or data that affect all sessions.
  ///
  /// # Parameters
  ///
  /// - `data`: The data to be sent, represented as a slice of bytes.
  ///
  /// # Errors
  ///
  /// Returns a `napi::Error` if sending data fails.
  ///
  /// # Example
  ///
  /// ```javascript
  /// const data = Buffer.from('Global Command');
  /// await multiplexer.sendToPty(data);
  /// ```
  #[napi]
  pub async fn send_to_pty(&self, data: &[u8]) -> Result<()> {
    let multiplexer = Arc::clone(&self.multiplexer);
    let data_clone = data.to_vec();
    tokio::task::spawn_blocking(move || {
      multiplexer.send_to_session(0, napi::bindgen_prelude::Buffer::from(data_clone))
    })
    .await
    .map_err(|e| map_to_napi_error(format!("Task Join Error: {}", e)))?
    .map_err(|e: napi::Error| map_to_napi_error(e))?;
    Ok(())
  }

  /// Broadcasts data to all active sessions within the multiplexer.
  ///
  /// This method sends the same data to every session, allowing for synchronized communication.
  ///
  /// # Parameters
  ///
  /// - `data`: The data to be broadcasted, represented as a slice of bytes.
  ///
  /// # Errors
  ///
  /// Returns a `napi::Error` if the broadcast fails.
  ///
  /// # Example
  ///
  /// ```javascript
  /// const data = Buffer.from('Broadcast Message');
  /// await multiplexer.broadcast(data);
  /// ```
  #[napi]
  pub async fn broadcast(&self, data: &[u8]) -> Result<()> {
    let multiplexer = Arc::clone(&self.multiplexer);
    let data_clone = data.to_vec();
    let buffer = napi::bindgen_prelude::Buffer::from(data_clone);
    tokio::task::spawn_blocking(move || multiplexer.broadcast(buffer))
      .await
      .map_err(|e| map_to_napi_error(format!("Task Join Error: {}", e)))?
      .map_err(|e: napi::Error| map_to_napi_error(e))?;
    Ok(())
  }

  /// Reads data from a specific session.
  ///
  /// Retrieves the latest data available from the specified session.
  ///
  /// # Parameters
  ///
  /// - `session_id`: The ID of the session to read data from.
  ///
  /// # Returns
  ///
  /// - `Ok(String)`: The data read from the session, decoded as a UTF-8 string.
  /// - `Err(napi::Error)`: If reading data fails.
  ///
  /// # Example
  ///
  /// ```javascript
  /// const data = await multiplexer.readFromSession(sessionId);
  /// console.log(data);
  /// ```
  #[napi]
  pub async fn read_from_session(&self, session_id: u32) -> Result<String> {
    let multiplexer = Arc::clone(&self.multiplexer);
    let data = tokio::task::spawn_blocking(move || multiplexer.read_from_session(session_id))
      .await
      .map_err(|e| map_to_napi_error(format!("Task Join Error: {}", e)))?
      .map_err(|e: napi::Error| map_to_napi_error(e))?;
    Ok(String::from_utf8_lossy(&data).to_string())
  }

  /// Reads data from all active sessions.
  ///
  /// Retrieves the latest data available from every session managed by the multiplexer.
  ///
  /// # Returns
  ///
  /// - `Ok(Vec<SessionData>)`: A vector containing `SessionData` for each session.
  /// - `Err(napi::Error)`: If reading data fails.
  ///
  /// # Example
  ///
  /// ```javascript
  /// const allData = await multiplexer.readAllSessions();
  /// allData.forEach(session => {
  ///   console.log(`Session ${session.session_id}: ${session.data}`);
  /// });
  /// ```
  #[napi]
  pub async fn read_all_sessions(&self) -> Result<Vec<SessionData>> {
    let multiplexer = Arc::clone(&self.multiplexer);
    let sessions_vec: Vec<multiplexer::SessionData> =
      tokio::task::spawn_blocking(move || multiplexer.read_all_sessions())
        .await
        .map_err(|e| map_to_napi_error(format!("Task Join Error: {}", e)))?
        .map_err(|e: napi::Error| map_to_napi_error(e))?;

    let sessions: Vec<SessionData> = sessions_vec
      .into_iter()
      .map(|session_data| SessionData {
        session_id: session_data.session_id,
        data: String::from_utf8_lossy(&session_data.data).to_string(),
      })
      .collect();

    Ok(sessions)
  }

  /// Removes a specific session from the multiplexer.
  ///
  /// This operation cleans up resources associated with the session.
  ///
  /// # Parameters
  ///
  /// - `session_id`: The ID of the session to be removed.
  ///
  /// # Errors
  ///
  /// Returns a `napi::Error` if the removal fails.
  ///
  /// # Example
  ///
  /// ```javascript
  /// await multiplexer.removeSession(sessionId);
  /// ```
  #[napi]
  pub async fn remove_session(&self, session_id: u32) -> Result<()> {
    let multiplexer = Arc::clone(&self.multiplexer);
    tokio::task::spawn_blocking(move || multiplexer.remove_session(session_id))
      .await
      .map_err(|e| map_to_napi_error(format!("Task Join Error: {}", e)))?
      .map_err(|e: napi::Error| map_to_napi_error(e))?;
    Ok(())
  }

  /// Merges multiple sessions into a single session.
  ///
  /// This operation consolidates the specified sessions, combining their data streams.
  ///
  /// # Parameters
  ///
  /// - `session_ids`: A vector of session IDs to be merged.
  ///
  /// # Errors
  ///
  /// Returns a `napi::Error` if the merge operation fails or if no session IDs are provided.
  ///
  /// # Example
  ///
  /// ```javascript
  /// await multiplexer.mergeSessions([sessionId1, sessionId2]);
  /// ```
  #[napi]
  pub async fn merge_sessions(&self, session_ids: Vec<u32>) -> Result<()> {
    if session_ids.is_empty() {
      return Err(Error::new(
        Status::InvalidArg,
        "No session IDs provided for merging",
      ));
    }

    let multiplexer = Arc::clone(&self.multiplexer);
    tokio::task::spawn_blocking(move || multiplexer.merge_sessions(session_ids))
      .await
      .map_err(|e| map_to_napi_error(format!("Task Join Error: {}", e)))?
      .map_err(|e: napi::Error| map_to_napi_error(e))?;
    Ok(())
  }

  /// Splits a session into two separate sessions.
  ///
  /// This operation creates two new sessions from an existing one, allowing for parallel operations.
  ///
  /// # Parameters
  ///
  /// - `session_id`: The ID of the session to be split.
  ///
  /// # Returns
  ///
  /// - `Ok(SplitSessionResult)`: Contains the IDs of the two newly created sessions.
  /// - `Err(napi::Error)`: If the split operation fails.
  ///
  /// # Example
  ///
  /// ```javascript
  /// const result = await multiplexer.splitSession(sessionId);
  /// console.log(`New sessions: ${result.session1}, ${result.session2}`);
  /// ```
  #[napi]
  pub async fn split_session(&self, session_id: u32) -> Result<SplitSessionResult> {
    let multiplexer = Arc::clone(&self.multiplexer);
    let [session1, session2] =
      tokio::task::spawn_blocking(move || multiplexer.split_session(session_id))
        .await
        .map_err(|e| map_to_napi_error(format!("Task Join Error: {}", e)))?
        .map_err(|e: napi::Error| map_to_napi_error(e))?;
    Ok(SplitSessionResult { session1, session2 })
  }

  /// Sets an environment variable for the PTY process.
  ///
  /// This allows customization of the PTY environment by specifying key-value pairs.
  ///
  /// # Parameters
  ///
  /// - `key`: The name of the environment variable.
  /// - `value`: The value to assign to the environment variable.
  ///
  /// # Errors
  ///
  /// Returns a `napi::Error` if setting the environment variable fails.
  ///
  /// # Example
  ///
  /// ```javascript
  /// await multiplexer.setEnv('PATH', '/usr/local/bin');
  /// ```
  #[napi]
  pub async fn set_env(&self, key: String, value: String) -> Result<()> {
    let multiplexer = Arc::clone(&self.multiplexer);
    tokio::task::spawn_blocking(move || multiplexer.set_env(key, value))
      .await
      .map_err(|e| map_to_napi_error(format!("Task Join Error: {}", e)))?
      .map_err(|e: napi::Error| map_to_napi_error(e))?;
    Ok(())
  }

  /// Changes the shell executable of the PTY process.
  ///
  /// This allows switching between different shell environments (e.g., bash, zsh).
  ///
  /// # Parameters
  ///
  /// - `shell_path`: The filesystem path to the new shell executable.
  ///
  /// # Errors
  ///
  /// Returns a `napi::Error` if changing the shell fails.
  ///
  /// # Example
  ///
  /// ```javascript
  /// await multiplexer.changeShell('/bin/zsh');
  /// ```
  #[napi]
  pub async fn change_shell(&self, shell_path: String) -> Result<()> {
    let multiplexer = Arc::clone(&self.multiplexer);
    tokio::task::spawn_blocking(move || multiplexer.change_shell(shell_path))
      .await
      .map_err(|e| map_to_napi_error(format!("Task Join Error: {}", e)))?
      .map_err(|e: napi::Error| map_to_napi_error(e))?;
    Ok(())
  }

  /// Retrieves the current status of the PTY process.
  ///
  /// This provides information about the PTY's operational state.
  ///
  /// # Returns
  ///
  /// - `Ok(String)`: A string describing the PTY status.
  /// - `Err(napi::Error)`: If retrieving the status fails.
  ///
  /// # Example
  ///
  /// ```javascript
  /// const status = await multiplexer.status();
  /// console.log(status);
  /// ```
  #[napi]
  pub async fn status(&self) -> Result<String> {
    let multiplexer = Arc::clone(&self.multiplexer);
    let status = tokio::task::spawn_blocking(move || multiplexer.status())
      .await
      .map_err(|e| map_to_napi_error(format!("Task Join Error: {}", e)))?
      .map_err(|e: napi::Error| map_to_napi_error(e))?;
    Ok(status)
  }

  /// Adjusts the logging level of the multiplexer.
  ///
  /// This allows dynamic control over the verbosity of logs for debugging or monitoring purposes.
  ///
  /// # Parameters
  ///
  /// - `level`: The desired logging level (e.g., "info", "debug", "error").
  ///
  /// # Errors
  ///
  /// Returns a `napi::Error` if setting the log level fails.
  ///
  /// # Example
  ///
  /// ```javascript
  /// await multiplexer.setLogLevel('debug');
  /// ```
  #[napi]
  pub async fn set_log_level(&self, level: String) -> Result<()> {
    let multiplexer = Arc::clone(&self.multiplexer);
    tokio::task::spawn_blocking(move || multiplexer.set_log_level(level))
      .await
      .map_err(|e| map_to_napi_error(format!("Task Join Error: {}", e)))?
      .map_err(|e: napi::Error| map_to_napi_error(e))?;
    Ok(())
  }

  /// Closes all active sessions within the multiplexer.
  ///
  /// This operation terminates every session, effectively resetting the multiplexer.
  ///
  /// # Errors
  ///
  /// Returns a `napi::Error` if closing sessions fails.
  ///
  /// # Example
  ///
  /// ```javascript
  /// await multiplexer.closeAllSessions();
  /// ```
  #[napi]
  pub async fn close_all_sessions(&self) -> Result<()> {
    let multiplexer = Arc::clone(&self.multiplexer);
    tokio::task::spawn_blocking(move || multiplexer.close_all_sessions())
      .await
      .map_err(|e| map_to_napi_error(format!("Task Join Error: {}", e)))?
      .map_err(|e: napi::Error| map_to_napi_error(e))?;
    Ok(())
  }

  /// Gracefully shuts down the PTY process and closes all sessions.
  ///
  /// This ensures that all resources are properly released and that the PTY process terminates cleanly.
  ///
  /// # Errors
  ///
  /// Returns a `napi::Error` if the shutdown process fails.
  ///
  /// # Example
  ///
  /// ```javascript
  /// await multiplexer.shutdownPty();
  /// ```
  #[napi]
  pub async fn shutdown_pty(&self) -> Result<()> {
    let multiplexer = Arc::clone(&self.multiplexer);
    tokio::task::spawn_blocking(move || multiplexer.shutdown_pty())
      .await
      .map_err(|e| map_to_napi_error(format!("Task Join Error: {}", e)))?
      .map_err(|e: napi::Error| map_to_napi_error(e))?;
    Ok(())
  }
}

/// Represents the main PTY handle exposed to JavaScript.
///
/// This struct manages the PTY process, the multiplexer, and command handling through channels.
///
/// # Example
///
/// ```javascript
/// const { PtyHandle } = require('your-module');
///
/// async function main() {
///   const pty = await PtyHandle.new();
///   const multiplexer = pty.multiplexerHandle();
///   // Interact with PTY and multiplexer...
/// }
/// ```
#[napi]
pub struct PtyHandle {
  pty: Arc<TokioMutex<PtyProcess>>,
  multiplexer: Arc<PtyMultiplexer>,
  command_sender: Sender<PtyCommand>,
}

#[napi]
impl PtyHandle {
  /// Creates a new `PtyHandle` with logging enabled.
  ///
  /// Initializes the PTY process, sets up the multiplexer, and starts the command handler.
  ///
  /// # Errors
  ///
  /// Returns a `napi::Error` if initialization fails.
  ///
  /// # Example
  ///
  /// ```javascript
  /// const pty = await PtyHandle.new();
  /// ```
  #[napi(factory)]
  pub fn new(env: Env) -> Result<Self> {
    initialize_logging();
    info!("Creating new PtyHandle");

    // Create bounded channels for commands and results.
    let (command_sender, command_receiver) = bounded::<PtyCommand>(100);
    let (result_sender, _result_receiver) = bounded::<PtyResult>(100);

    // Initialize the PTY process without arguments.
    let pty_process = PtyProcess::new()
      .map_err(|e| map_to_napi_error(format!("Failed to initialize PTY process: {}", e)))?;
    let pty = Arc::new(TokioMutex::new(pty_process.clone()));

    // Convert the PtyProcess into a JsObject for the multiplexer
    let pty_js_object = pty_process.into_js_object(&env)?;

    // Initialize the multiplexer with the JsObject.
    let multiplexer = PtyMultiplexer::new(pty_js_object)
      .map_err(|e| map_to_napi_error(format!("Failed to initialize multiplexer: {}", e)))?;
    let multiplexer_arc = Arc::new(multiplexer);

    // Instantiate PtyHandle.
    let handle = PtyHandle {
      pty: Arc::clone(&pty),
      multiplexer: Arc::clone(&multiplexer_arc),
      command_sender,
    };

    // Start the command handler using spawn_blocking to prevent runtime nesting.
    {
      let pty_clone = Arc::clone(&pty);
      let multiplexer_clone = Arc::clone(&multiplexer_arc);
      tokio::task::spawn_blocking(move || {
        let pty_locked = pty_clone.blocking_lock().clone();
        handle_commands(
          Arc::new(parking_lot::Mutex::new(Some(pty_locked.clone()))),
          Arc::new(parking_lot::Mutex::new(Some((*multiplexer_clone).clone()))),
          command_receiver,
          result_sender,
        );
      });
    }

    Ok(handle)
  }

  /// Retrieves the `MultiplexerHandle` associated with this PTY.
  ///
  /// This allows interaction with the multiplexer for managing sessions and data.
  ///
  /// # Returns
  ///
  /// A `MultiplexerHandle` instance.
  ///
  /// # Example
  ///
  /// ```javascript
  /// const multiplexer = pty.multiplexerHandle();
  /// ```
  #[napi(getter)]
  pub fn multiplexer_handle(&self) -> MultiplexerHandle {
    MultiplexerHandle {
      multiplexer: Arc::clone(&self.multiplexer),
    }
  }

  /// Asynchronously reads data from the PTY.
  ///
  /// Initiates a read operation and retrieves the data output from the PTY process.
  ///
  /// # Returns
  ///
  /// - `Ok(String)`: The data read from the PTY, decoded as a UTF-8 string.
  /// - `Err(napi::Error)`: If the read operation fails or times out.
  ///
  /// # Example
  ///
  /// ```javascript
  /// const data = await pty.read();
  /// console.log(data);
  /// ```
  #[napi]
  pub async fn read(&self) -> Result<String> {
    info!("Initiating read from PTY.");
    let read_timeout = Duration::from_secs(5);

    // Create a oneshot channel to receive the read result.
    let (sender, receiver) = oneshot::channel();

    // Send the Read command.
    self
      .command_sender
      .send(PtyCommand::Read(sender))
      .map_err(|e| {
        error!("Failed to send read command: {}", e);
        map_to_napi_error(format!("Failed to send read command: {}", e))
      })?;

    // Await the result with a timeout.
    match timeout(read_timeout, receiver).await {
      Ok(Ok(PtyResult::Data(bytes))) => {
        let data = String::from_utf8_lossy(&bytes).to_string();
        info!("Read operation successful: {}", data);
        Ok(data)
      }
      Ok(Ok(PtyResult::Success(msg))) => {
        info!("Read operation successful: {}", msg);
        Ok(msg)
      }
      Ok(Ok(PtyResult::Failure(msg))) => {
        error!("Read operation failed: {}", msg);
        Err(Error::new(Status::Unknown, msg))
      }
      Ok(Ok(_)) => {
        error!("Read operation returned unexpected result");
        Err(Error::new(
          Status::Unknown,
          "Read operation returned unexpected result",
        ))
      }
      Ok(Err(_)) => {
        error!("Read operation: Receiver dropped");
        Err(Error::new(
          Status::Unknown,
          "Read operation receiver dropped",
        ))
      }
      Err(_) => {
        error!("Read operation timed out");
        Err(Error::new(Status::Unknown, "Read operation timed out"))
      }
    }
  }

  /// Asynchronously writes data to the PTY.
  ///
  /// Sends the provided data to the PTY process for execution or processing.
  ///
  /// # Parameters
  ///
  /// - `data`: The data to be written, represented as a `String`.
  ///
  /// # Returns
  ///
  /// - `Ok(())`: If the write operation is successful.
  /// - `Err(napi::Error)`: If the write operation fails or times out.
  ///
  /// # Example
  ///
  /// ```javascript
  /// await pty.write('ls -la');
  /// ```
  #[napi]
  pub async fn write(&self, data: String) -> Result<()> {
    let data_bytes = Bytes::from(data.into_bytes());

    info!("Initiating write to PTY.");

    // Create a oneshot channel to receive the write result.
    let (sender, receiver) = oneshot::channel();

    // Send the write command along with the responder.
    self
      .command_sender
      .send(PtyCommand::Write(data_bytes, sender))
      .map_err(|e| {
        error!("Failed to send write command: {}", e);
        map_to_napi_error(format!("Failed to send write command: {}", e))
      })?;

    // Await the result with a timeout.
    match timeout(Duration::from_secs(5), receiver).await {
      Ok(Ok(PtyResult::Success(msg))) => {
        info!("Write operation successful: {}", msg);
        Ok(())
      }
      Ok(Ok(PtyResult::Failure(msg))) => {
        error!("Write operation failed: {}", msg);
        Err(Error::new(Status::Unknown, msg))
      }
      Ok(Ok(PtyResult::Data(_))) => {
        error!("Unexpected data returned in write operation");
        Err(Error::new(
          Status::Unknown,
          "Unexpected data returned in write operation",
        ))
      }
      Ok(Ok(_)) => {
        error!("Write operation returned unexpected result");
        Err(Error::new(
          Status::Unknown,
          "Write operation returned unexpected result",
        ))
      }
      Ok(Err(_)) => {
        error!("Write operation: Receiver dropped");
        Err(Error::new(
          Status::Unknown,
          "Write operation receiver dropped",
        ))
      }
      Err(_) => {
        error!("Write operation timed out");
        Err(Error::new(Status::Unknown, "Write operation timed out"))
      }
    }
  }

  /// Asynchronously resizes the PTY window.
  ///
  /// Adjusts the number of columns and rows of the PTY, effectively changing the terminal size.
  ///
  /// # Parameters
  ///
  /// - `cols`: The number of columns for the PTY window.
  /// - `rows`: The number of rows for the PTY window.
  ///
  /// # Returns
  ///
  /// - `Ok(())`: If the resize operation is successful.
  /// - `Err(napi::Error)`: If the resize operation fails or times out.
  ///
  /// # Example
  ///
  /// ```javascript
  /// await pty.resize(120, 40);
  /// ```
  #[napi]
  pub async fn resize(&self, cols: u16, rows: u16) -> Result<()> {
    info!("Initiating resize of PTY to cols: {}, rows: {}", cols, rows);

    // Create a oneshot channel to receive the resize result.
    let (sender, receiver) = oneshot::channel();

    // Send the resize command along with the responder.
    self
      .command_sender
      .send(PtyCommand::Resize { cols, rows, sender })
      .map_err(|e| {
        error!("Failed to send resize command: {}", e);
        map_to_napi_error(format!("Failed to send resize command: {}", e))
      })?;

    // Await the result with a timeout.
    match timeout(Duration::from_secs(5), receiver).await {
      Ok(Ok(PtyResult::Success(msg))) => {
        info!("Resize operation successful: {}", msg);
        Ok(())
      }
      Ok(Ok(PtyResult::Failure(msg))) => {
        error!("Resize operation failed: {}", msg);
        // Attempt to force kill if resize fails.
        self.force_kill(5000).await
      }
      Ok(Ok(PtyResult::Data(_))) => {
        error!("Unexpected data returned in resize operation");
        Err(Error::new(
          Status::Unknown,
          "Unexpected data returned in resize operation",
        ))
      }
      Ok(Ok(_)) => {
        error!("Resize operation returned unexpected result");
        Err(Error::new(
          Status::Unknown,
          "Resize operation returned unexpected result",
        ))
      }
      Ok(Err(_)) => {
        error!("Resize operation: Receiver dropped");
        Err(Error::new(
          Status::Unknown,
          "Resize operation receiver dropped",
        ))
      }
      Err(_) => {
        error!("Resize operation timed out");
        // Attempt to force kill if resize times out.
        self.force_kill(5000).await
      }
    }
  }

  /// Asynchronously executes a command within the PTY.
  ///
  /// This allows running shell commands programmatically within the PTY environment.
  ///
  /// # Parameters
  ///
  /// - `command`: The shell command to be executed, represented as a `String`.
  ///
  /// # Returns
  ///
  /// - `Ok(String)`: The output or result of the executed command.
  /// - `Err(napi::Error)`: If the execute operation fails or times out.
  ///
  /// # Example
  ///
  /// ```javascript
  /// const output = await pty.execute('echo "Hello, World!"');
  /// console.log(output);
  /// ```
  #[napi]
  pub async fn execute(&self, command: String) -> Result<String> {
    info!("Executing command in PTY: {}", command);

    // Create a oneshot channel to receive the execute result.
    let (sender, receiver) = oneshot::channel();

    // Send the execute command along with the responder.
    self
      .command_sender
      .send(PtyCommand::Execute(command.clone(), sender))
      .map_err(|e| {
        error!("Failed to send execute command: {}", e);
        map_to_napi_error(format!("Failed to send execute command: {}", e))
      })?;

    // Await the result with a timeout.
    match timeout(Duration::from_secs(5), receiver).await {
      Ok(Ok(PtyResult::Success(msg))) => {
        info!("Execute operation successful: {}", msg);
        Ok(msg)
      }
      Ok(Ok(PtyResult::Failure(msg))) => {
        error!("Execute operation failed: {}", msg);
        Err(Error::new(Status::Unknown, msg))
      }
      Ok(Ok(PtyResult::Data(bytes))) => {
        let data = String::from_utf8_lossy(&bytes).to_string();
        info!("Execute operation returned data: {}", data);
        Ok(data)
      }
      Ok(Ok(_)) => {
        error!("Execute operation returned unexpected result");
        Err(Error::new(
          Status::Unknown,
          "Execute operation returned unexpected result",
        ))
      }
      Ok(Err(_)) => {
        error!("Execute operation: Receiver dropped");
        Err(Error::new(
          Status::Unknown,
          "Execute operation receiver dropped",
        ))
      }
      Err(_) => {
        error!("Execute operation timed out");
        Err(Error::new(Status::Unknown, "Execute operation timed out"))
      }
    }
  }

  /// Asynchronously closes the PTY gracefully.
  ///
  /// Attempts to terminate the PTY process without abrupt interruption, ensuring resources are freed.
  ///
  /// # Returns
  ///
  /// - `Ok(())`: If the close operation is successful.
  /// - `Err(napi::Error)`: If the close operation fails or times out.
  ///
  /// # Example
  ///
  /// ```javascript
  /// await pty.close();
  /// ```
  #[napi]
  pub async fn close(&self) -> Result<()> {
    info!("Initiating graceful shutdown of PTY.");

    // Create a oneshot channel to receive the close result.
    let (sender, receiver) = oneshot::channel();

    // Send the Close command.
    self
      .command_sender
      .send(PtyCommand::Close(sender))
      .map_err(|e| {
        error!("Failed to send close command: {}", e);
        map_to_napi_error(format!("Failed to send close command: {}", e))
      })?;

    // Await the result with a timeout.
    match timeout(Duration::from_secs(10), receiver).await {
      Ok(Ok(PtyResult::Success(msg))) => {
        info!("Close operation successful: {}", msg);
        Ok(())
      }
      Ok(Ok(PtyResult::Failure(msg))) => {
        error!("Close operation failed: {}", msg);
        // Attempt to force kill if close fails.
        self.force_kill(5000).await
      }
      Ok(Ok(PtyResult::Data(_))) => {
        error!("Unexpected data returned in close operation");
        Err(Error::new(
          Status::Unknown,
          "Unexpected data returned in close operation",
        ))
      }
      Ok(Ok(_)) => {
        error!("Close operation returned unexpected result");
        Err(Error::new(
          Status::Unknown,
          "Close operation returned unexpected result",
        ))
      }
      Ok(Err(_)) => {
        error!("Close operation: Receiver dropped");
        Err(Error::new(
          Status::Unknown,
          "Close operation receiver dropped",
        ))
      }
      Err(_) => {
        error!("Close operation timed out");
        // Attempt to force kill if close times out.
        self.force_kill(5000).await
      }
    }
  }

  /// Asynchronously forcefully kills the PTY process.
  ///
  /// This method is used as a last resort to terminate the PTY process if graceful shutdown fails.
  ///
  /// # Parameters
  ///
  /// - `force_timeout_ms`: The timeout in milliseconds before forcing the kill.
  ///
  /// # Returns
  ///
  /// - `Ok(())`: If the force kill operation is successful.
  /// - `Err(napi::Error)`: If the force kill operation fails or times out.
  ///
  /// # Example
  ///
  /// ```javascript
  /// await pty.forceKill(5000);
  /// ```
  #[napi]
  pub async fn force_kill(&self, force_timeout_ms: u32) -> Result<()> {
    info!("Initiating force kill of PTY.");

    // Create a oneshot channel to receive the result.
    let (sender, receiver) = oneshot::channel();

    // Send the ForceKill command.
    self
      .command_sender
      .send(PtyCommand::ForceKill(sender))
      .map_err(|e| {
        error!("Failed to send force kill command: {}", e);
        map_to_napi_error(format!("Failed to send force kill command: {}", e))
      })?;

    // Await the result with a timeout.
    match timeout(Duration::from_millis(force_timeout_ms as u64), receiver).await {
      Ok(Ok(PtyResult::Success(msg))) => {
        info!("Force kill operation successful: {}", msg);
        Ok(())
      }
      Ok(Ok(PtyResult::Failure(msg))) => {
        error!("Force kill operation failed: {}", msg);
        Err(Error::new(Status::Unknown, msg))
      }
      Ok(Ok(PtyResult::Data(_))) => {
        error!("Unexpected data returned in force kill operation");
        Err(Error::new(
          Status::Unknown,
          "Unexpected data returned in force kill operation",
        ))
      }
      Ok(Ok(_)) => {
        error!("Force kill operation returned unexpected result");
        Err(Error::new(
          Status::Unknown,
          "Force kill operation returned unexpected result",
        ))
      }
      Ok(Err(_)) => {
        error!("Force kill operation: Receiver dropped");
        Err(Error::new(
          Status::Unknown,
          "Force kill operation receiver dropped",
        ))
      }
      Err(_) => {
        error!("Force kill operation timed out");
        Err(Error::new(
          Status::Unknown,
          "Force kill operation timed out",
        ))
      }
    }
  }

  /// Retrieves the process ID (PID) of the PTY process.
  ///
  /// This can be used for monitoring or managing the PTY process externally.
  ///
  /// # Returns
  ///
  /// - `Ok(i32)`: The PID of the PTY process.
  /// - `Err(napi::Error)`: If retrieving the PID fails.
  ///
  /// # Example
  ///
  /// ```javascript
  /// const pid = await pty.pid();
  /// console.log(`PTY PID: ${pid}`);
  /// ```
  #[napi]
  pub async fn pid(&self) -> Result<i32> {
    let pty_guard = self.pty.lock().await;
    Ok(pty_guard.pid as i32)
  }

  /// Sends a signal to the PTY process.
  ///
  /// This allows external control over the PTY process, such as terminating or pausing it.
  ///
  /// # Parameters
  ///
  /// - `signal`: The signal number to send to the PTY process.
  ///
  /// # Errors
  ///
  /// Returns a `napi::Error` if sending the signal fails.
  ///
  /// # Example
  ///
  /// ```javascript
  /// // Send SIGTERM to the PTY process
  /// await pty.killProcess(15);
  /// ```
  #[napi]
  pub async fn kill_process(&self, signal: i32) -> Result<()> {
    info!("Initiating kill process with signal {}.", signal);

    // Create a oneshot channel to receive the result.
    let (sender, receiver) = oneshot::channel();

    // Send the KillProcess command.
    self
      .command_sender
      .send(PtyCommand::KillProcess(signal, sender))
      .map_err(|e| {
        error!("Failed to send kill process command: {}", e);
        map_to_napi_error(format!("Failed to send kill process command: {}", e))
      })?;

    // Await the result with a timeout.
    match timeout(Duration::from_secs(5), receiver).await {
      Ok(Ok(PtyResult::Success(msg))) => {
        info!("Kill process operation successful: {}", msg);
        Ok(())
      }
      Ok(Ok(PtyResult::Failure(msg))) => {
        error!("Kill process operation failed: {}", msg);
        Err(Error::new(Status::Unknown, msg))
      }
      Ok(Ok(PtyResult::Data(_))) => {
        error!("Unexpected data returned in kill process operation");
        Err(Error::new(
          Status::Unknown,
          "Unexpected data returned in kill process operation",
        ))
      }
      Ok(Ok(_)) => {
        error!("Kill process operation returned unexpected result");
        Err(Error::new(
          Status::Unknown,
          "Kill process operation returned unexpected result",
        ))
      }
      Ok(Err(_)) => {
        error!("Kill process operation: Receiver dropped");
        Err(Error::new(
          Status::Unknown,
          "Kill process operation receiver dropped",
        ))
      }
      Err(_) => {
        error!("Kill process operation timed out");
        Err(Error::new(
          Status::Unknown,
          "Kill process operation timed out",
        ))
      }
    }
  }

  /// Waits for the PTY process to change state.
  ///
  /// This can be used to wait for the PTY process to exit or for specific signals.
  ///
  /// # Parameters
  ///
  /// - `options`: Options that modify the behavior of `waitpid`.
  ///
  /// # Returns
  ///
  /// - `Ok(i32)`: The PID returned by `waitpid`.
  /// - `Err(napi::Error)`: If the `waitpid` operation fails or times out.
  ///
  /// # Example
  ///
  /// ```javascript
  /// const pid = await pty.waitpid(0);
  /// console.log(`Process exited with PID: ${pid}`);
  /// ```
  #[napi]
  pub async fn waitpid(&self, options: i32) -> Result<i32> {
    info!("Initiating waitpid with options {}.", options);

    // Create a oneshot channel to receive the result.
    let (sender, receiver) = oneshot::channel();

    // Send the WaitPid command.
    self
      .command_sender
      .send(PtyCommand::WaitPid(options, sender))
      .map_err(|e| {
        error!("Failed to send waitpid command: {}", e);
        map_to_napi_error(format!("Failed to send waitpid command: {}", e))
      })?;

    // Await the result with a timeout.
    match timeout(Duration::from_secs(5), receiver).await {
      Ok(Ok(PtyResult::Success(pid_str))) => {
        let pid = pid_str.parse::<i32>().map_err(|e| {
          error!("Failed to parse PID from response: {}", e);
          Error::new(Status::Unknown, "Failed to parse PID")
        })?;
        info!("Waitpid operation successful, PID: {}", pid);
        Ok(pid)
      }
      Ok(Ok(PtyResult::Failure(msg))) => {
        error!("Waitpid operation failed: {}", msg);
        Err(Error::new(Status::Unknown, msg))
      }
      Ok(Ok(PtyResult::Data(_))) => {
        error!("Unexpected data returned in waitpid operation");
        Err(Error::new(
          Status::Unknown,
          "Unexpected data returned in waitpid operation",
        ))
      }
      Ok(Ok(_)) => {
        error!("Waitpid operation returned unexpected result");
        Err(Error::new(
          Status::Unknown,
          "Waitpid operation returned unexpected result",
        ))
      }
      Ok(Err(_)) => {
        error!("Waitpid operation: Receiver dropped");
        Err(Error::new(
          Status::Unknown,
          "Waitpid operation receiver dropped",
        ))
      }
      Err(_) => {
        error!("Waitpid operation timed out");
        Err(Error::new(Status::Unknown, "Waitpid operation timed out"))
      }
    }
  }

  /// Closes the master file descriptor of the PTY process.
  ///
  /// This operation is used to release the master end of the PTY, preventing further communication.
  ///
  /// # Returns
  ///
  /// - `Ok(())`: If the operation is successful.
  /// - `Err(napi::Error)`: If the operation fails or times out.
  ///
  /// # Example
  ///
  /// ```javascript
  /// await pty.closeMasterFd();
  /// ```
  #[napi]
  pub async fn close_master_fd(&self) -> Result<()> {
    info!("Initiating close_master_fd.");

    // Create a oneshot channel to receive the result.
    let (sender, receiver) = oneshot::channel();

    // Send the CloseMasterFd command.
    self
      .command_sender
      .send(PtyCommand::CloseMasterFd(sender))
      .map_err(|e| {
        error!("Failed to send close_master_fd command: {}", e);
        map_to_napi_error(format!("Failed to send close_master_fd command: {}", e))
      })?;

    // Await the result with a timeout.
    match timeout(Duration::from_secs(5), receiver).await {
      Ok(Ok(PtyResult::Success(msg))) => {
        info!("Close master_fd operation successful: {}", msg);
        Ok(())
      }
      Ok(Ok(PtyResult::Failure(msg))) => {
        error!("Close master_fd operation failed: {}", msg);
        Err(Error::new(Status::Unknown, msg))
      }
      Ok(Ok(PtyResult::Data(_))) => {
        error!("Unexpected data returned in close_master_fd operation");
        Err(Error::new(
          Status::Unknown,
          "Unexpected data returned in close_master_fd operation",
        ))
      }
      Ok(Ok(_)) => {
        error!("Close master_fd operation returned unexpected result");
        Err(Error::new(
          Status::Unknown,
          "Close master_fd operation returned unexpected result",
        ))
      }
      Ok(Err(_)) => {
        error!("Close master_fd operation: Receiver dropped");
        Err(Error::new(
          Status::Unknown,
          "Close master_fd operation receiver dropped",
        ))
      }
      Err(_) => {
        error!("Close master_fd operation timed out");
        Err(Error::new(
          Status::Unknown,
          "Close master_fd operation timed out",
        ))
      }
    }
  }
  /// Creates a new `PtyHandle` specifically for testing purposes.
  ///
  /// This method is used in test environments to instantiate a `PtyHandle` without requiring
  /// the presence of a `napi::Env` object. It mirrors the behavior of the standard `new`
  /// constructor, but is tailored for use in unit tests and integration tests. The function
  /// sets up a fully initialized PTY process and multiplexer, along with command and result
  /// channels for PTY operations.
  ///
  /// The method initializes logging, creates the necessary PTY process, and sets up channels
  /// for communication between the PTY process and the command handler. It then spawns a
  /// background task to handle commands, ensuring the PTY process operates correctly throughout
  /// the test.
  ///
  /// # Returns
  ///
  /// This method returns a `Result<Self>`, where:
  /// - `Ok(Self)` indicates successful creation of the `PtyHandle` for testing.
  /// - `Err(napi::Error)` occurs if initialization of the PTY process or other components fails.
  ///
  /// # Panics
  ///
  /// This function will panic if it fails to initialize any component required to
  /// set up the PTY handle, including the PTY process itself.
  ///
  /// # Example
  ///
  /// ```rust
  /// #[tokio::test]
  /// async fn test_create_pty_handle() {
  ///     let pty_handle = PtyHandle::new_for_test().expect("Failed to create PtyHandle");
  ///
  ///     // Perform operations with `pty_handle`
  ///     pty_handle.close().await.expect("Failed to close PtyHandle");
  /// }
  /// ```
  ///
  /// This example demonstrates how the `new_for_test` method can be used in a test to create
  /// a `PtyHandle` instance and execute operations with it.
  ///
  /// # Notes
  ///
  /// - This function is only available in test builds as it is marked with `#[cfg(test)]`.
  /// - It is specifically designed to be used in tests and should not be used in production code.
  ///
  /// # Usage Context
  ///
  /// The `new_for_test` function is useful in scenarios where you need to instantiate the
  /// `PtyHandle` struct without access to a full runtime environment that provides `napi::Env`.
  /// It is often used in conjunction with test frameworks such as `tokio::test` for
  /// asynchronous Rust tests.
  #[cfg(test)]
  pub(crate) fn new_for_test() -> Result<Self> {
    use crate::pty::multiplexer::PtyMultiplexer;
    use crate::pty::platform::PtyProcess;
    use crate::utils::logging::initialize_logging;
    use crossbeam_channel::bounded;
    use std::sync::Arc;
    use tokio::sync::Mutex as TokioMutex;

    initialize_logging();
    info!("Creating new PtyHandle for test.");

    // Create bounded channels for commands and results.
    let (command_sender, command_receiver) = bounded::<PtyCommand>(100);
    let (result_sender, _result_receiver) = bounded::<PtyResult>(100);

    // Initialize the PTY process without arguments.
    let pty_process = PtyProcess::new()
      .map_err(|e| napi::Error::from_reason(format!("Failed to initialize PTY process: {}", e)))?;
    let pty = Arc::new(TokioMutex::new(pty_process.clone()));

    // Initialize the multiplexer without `Env`, directly using PtyProcess.
    let multiplexer = PtyMultiplexer {
      pty_process: Arc::new(pty_process),
      sessions: Arc::new(parking_lot::Mutex::new(std::collections::HashMap::new())),
    };
    let multiplexer_arc = Arc::new(multiplexer);

    // Instantiate PtyHandle.
    let handle = PtyHandle {
      pty: Arc::clone(&pty),
      multiplexer: Arc::clone(&multiplexer_arc),
      command_sender,
    };

    // Start the command handler using `spawn_blocking` to avoid runtime nesting.
    {
      let pty_clone = Arc::clone(&pty);
      let multiplexer_clone = Arc::clone(&multiplexer_arc);
      tokio::task::spawn_blocking(move || {
        let pty_locked = pty_clone.blocking_lock().clone();
        handle_commands(
          Arc::new(parking_lot::Mutex::new(Some(pty_locked.clone()))),
          Arc::new(parking_lot::Mutex::new(Some((*multiplexer_clone).clone()))),
          command_receiver,
          result_sender,
        );
      });
    }

    Ok(handle)
  }

  /// Sets an environment variable using the provided key and value.
  ///
  /// This method directly interacts with the system's environment variables.
  ///
  /// # Parameters
  ///
  /// - `key`: The name of the environment variable to set.
  /// - `value`: The value to assign to the environment variable.
  ///
  /// # Returns
  ///
  /// - `Ok(())`: If the environment variable is set successfully.
  /// - `Err(napi::Error)`: If setting the environment variable fails or verification fails.
  ///
  /// # Example
  ///
  /// ```javascript
  /// await pty.setEnv('EDITOR', 'vim');
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
        Err(Error::new(
          Status::Unknown,
          format!("Failed to correctly set environment variable: {}", key),
        ))
      }
      Err(e) => {
        error!(
          "Error setting environment variable: {} = {}. Error: {}",
          key, value, e
        );
        Err(Error::new(
          Status::Unknown,
          format!(
            "Error setting environment variable: {} = {}. Error: {}",
            key, value, e
          ),
        ))
      }
    }
  }
}

impl Drop for PtyHandle {
  /// Cleans up resources when a `PtyHandle` is dropped.
  ///
  /// Initiates a shutdown of the PTY process and sends a shutdown command through the channel.
  fn drop(&mut self) {
    initialize_logging();
    info!("Dropping PtyHandle, initiating cleanup.");

    // Create a oneshot channel to receive the shutdown result.
    let (sender, receiver) = oneshot::channel();

    // Send the ShutdownPty command.
    let _ = self.command_sender.send(PtyCommand::ShutdownPty(sender));

    // Spawn a task to await the shutdown result without blocking.
    tokio::spawn(async move {
      let _ = receiver.await;
    });
  }
}
