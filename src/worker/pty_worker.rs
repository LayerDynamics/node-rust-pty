// src/worker/pty_worker.rs

use log::{error, info};
use napi::bindgen_prelude::*;
use napi::threadsafe_function::{
  ThreadSafeCallContext, ThreadsafeFunction, ThreadsafeFunctionCallMode,
};
use napi::{JsFunction, JsUnknown, Result};
use napi_derive::napi;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io::{self, Read, Write};
use std::net::{Shutdown, TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

/// Enum representing messages that can be sent from the worker to the parent thread.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum WorkerMessage {
  Ready,
  Error(String),
  DataReceived(u32, Vec<u8>),
}

/// Struct representing the data passed to the worker.
///
/// Contains the names of the conout and worker pipes for establishing socket connections.
#[derive(Debug, Clone)]
#[napi(object)]
pub struct WorkerData {
  pub conout_pipe_name: String,
  pub worker_pipe_name: String,
}

/// Represents a session within the worker.
///
/// Each `WorkerSession` maintains a unique session ID and a shared `TcpStream` to the worker socket.
struct WorkerSession {
  session_id: u32,
  worker_socket: Arc<Mutex<TcpStream>>,
  data_buffer: Mutex<Vec<u8>>,
}

/// Shared state for the `PtyWorker`.
struct PtyWorkerState {
  sessions: Mutex<HashMap<u32, WorkerSession>>,
  tsfn: Mutex<Option<Arc<ThreadsafeFunction<WorkerMessage>>>>,
  conout_socket: Mutex<Option<TcpStream>>,
  shutdown_flag: AtomicBool,
  session_counter: AtomicU32,
}

#[napi]
pub struct PtyWorker {
  state: Arc<PtyWorkerState>,
}

#[napi]
impl PtyWorker {
  /// Creates a new instance of `PtyWorker`.
  #[napi(constructor)]
  pub fn new() -> Self {
    PtyWorker {
      state: Arc::new(PtyWorkerState {
        sessions: Mutex::new(HashMap::new()),
        tsfn: Mutex::new(None),
        conout_socket: Mutex::new(None),
        shutdown_flag: AtomicBool::new(false),
        session_counter: AtomicU32::new(1),
      }),
    }
  }

  /// Starts the worker by setting up the necessary connections and threads.
  ///
  /// # Arguments
  ///
  /// - `worker_data`: The data required to establish connections.
  /// - `callback`: A JavaScript function to receive messages from the worker.
  ///
  /// # Returns
  ///
  /// - `Result<()>`: An empty result indicating success or failure.
  #[napi]
  pub fn start_worker(&self, worker_data: WorkerData, callback: JsFunction) -> Result<()> {
    let conout_pipe_name = worker_data.conout_pipe_name.clone();
    let worker_pipe_name = worker_data.worker_pipe_name.clone();
    let state = Arc::clone(&self.state);

    // Initialize a threadsafe function to communicate with JavaScript
    let threadsafe_fn = {
      let tsfn: ThreadsafeFunction<WorkerMessage> =
        callback.create_threadsafe_function(0, |ctx: ThreadSafeCallContext<WorkerMessage>| {
          let worker_message = ctx.value;

          // Create a JavaScript object to send as a message
          let env = ctx.env;
          let js_obj = match worker_message {
            WorkerMessage::Ready => {
              let mut obj = env.create_object()?;
              obj.set("message_type", "READY")?;
              obj.set("payload", env.get_null()?)?;
              obj
            }
            WorkerMessage::Error(err) => {
              let mut obj = env.create_object()?;
              obj.set("message_type", "ERROR")?;
              obj.set("payload", env.create_string(&err)?)?;
              obj
            }
            WorkerMessage::DataReceived(session_id, data) => {
              let mut obj = env.create_object()?;
              obj.set("message_type", "DATA_RECEIVED")?;
              let mut payload = env.create_object()?;
              payload.set("session_id", session_id)?;
              let js_buffer = env.create_buffer_with_data(data)?.into_raw();
              payload.set("data", js_buffer)?;
              obj.set("payload", payload)?;
              obj
            }
          };

          // Return the JavaScript object wrapped in a Vec<JsUnknown>
          Ok(vec![js_obj.into_unknown()])
        })?;

      Arc::new(tsfn)
    };

    // Store the threadsafe function in the state
    {
      let mut tsfn_lock = state.tsfn.lock().unwrap();
      *tsfn_lock = Some(threadsafe_fn.clone());
    }

    // Clone for the worker thread
    let state_clone = Arc::clone(&state);
    let conout_pipe_name_clone = conout_pipe_name.clone();
    let worker_pipe_name_clone = worker_pipe_name.clone();

    // Spawn the worker thread
    thread::spawn(move || {
      // Connect to the conout_pipe
      let conout_socket = match TcpStream::connect(&conout_pipe_name_clone) {
        Ok(stream) => {
          info!("Connected to conout pipe: {}", conout_pipe_name_clone);
          stream
        }
        Err(e) => {
          error!("Failed to connect to conout pipe: {}", e);
          let tsfn_option = state_clone.tsfn.lock().unwrap();
          if let Some(ref tsfn) = *tsfn_option {
            let message = WorkerMessage::Error(format!("Failed to connect to conout pipe: {}", e));
            let _ = tsfn.call(Ok(message), ThreadsafeFunctionCallMode::Blocking);
          }
          return;
        }
      };

      // Set conout_socket to non-blocking
      if let Err(e) = conout_socket.set_nonblocking(true) {
        error!("Failed to set conout_socket to non-blocking: {}", e);
        let tsfn_option = state_clone.tsfn.lock().unwrap();
        if let Some(ref tsfn) = *tsfn_option {
          let message = WorkerMessage::Error(format!(
            "Failed to set conout_socket to non-blocking: {}",
            e
          ));
          let _ = tsfn.call(Ok(message), ThreadsafeFunctionCallMode::Blocking);
        }
        return;
      }

      // Store the conout_socket in the state
      {
        let mut conout_lock = state_clone.conout_socket.lock().unwrap();
        *conout_lock = Some(conout_socket.try_clone().unwrap());
      }

      // Create a TCP listener for the worker_pipe_name
      let listener = match TcpListener::bind(&worker_pipe_name_clone) {
        Ok(l) => {
          info!("Worker listening on: {}", worker_pipe_name_clone);
          l
        }
        Err(e) => {
          error!("Failed to bind worker pipe: {}", e);
          let tsfn_option = state_clone.tsfn.lock().unwrap();
          if let Some(ref tsfn) = *tsfn_option {
            let message = WorkerMessage::Error(format!("Failed to bind worker pipe: {}", e));
            let _ = tsfn.call(Ok(message), ThreadsafeFunctionCallMode::Blocking);
          }
          return;
        }
      };

      // Notify readiness
      {
        let tsfn_option = state_clone.tsfn.lock().unwrap();
        if let Some(ref tsfn) = *tsfn_option {
          let message = WorkerMessage::Ready;
          let _ = tsfn.call(Ok(message), ThreadsafeFunctionCallMode::Blocking);
        }
      }

      // Accept worker socket connections
      for stream in listener.incoming() {
        if state_clone.shutdown_flag.load(Ordering::SeqCst) {
          info!("Shutdown flag set. Stopping acceptance of new connections.");
          break;
        }

        match stream {
          Ok(worker_socket) => {
            let session_id = state_clone.session_counter.fetch_add(1, Ordering::SeqCst);
            info!("Worker socket connected with session ID: {}", session_id);

            // Wrap worker_socket in Arc for shared ownership
            let worker_socket = Arc::new(Mutex::new(worker_socket));

            // Create a new WorkerSession
            let session = WorkerSession {
              session_id,
              worker_socket: Arc::clone(&worker_socket),
              data_buffer: Mutex::new(Vec::new()),
            };

            // Insert the session into the sessions map
            {
              let mut sessions = state_clone.sessions.lock().unwrap();
              sessions.insert(session_id, session);
            }

            let conout_clone = {
              let conout_lock = state_clone.conout_socket.lock().unwrap();
              conout_lock.as_ref().and_then(|s| s.try_clone().ok())
            };

            let tsfn_option = state_clone.tsfn.lock().unwrap();
            let tsfn = tsfn_option.as_ref().cloned();

            let state_for_read = Arc::clone(&state_clone);
            let state_for_write = Arc::clone(&state_clone);

            // Clone worker_socket for reading from conout to worker
            let worker_socket_clone = Arc::clone(&worker_socket);
            // Clone worker_socket for writing from worker to conout
            let worker_socket_clone_for_write = Arc::clone(&worker_socket);

            // Clone the session_id for the threads
            let session_id_clone = session_id;
            let session_id_clone_write = session_id;

            // Spawn a thread to handle data piping from conout_socket to worker_socket
            let conout_clone_thread = conout_clone.as_ref().and_then(|s| s.try_clone().ok());
            let tsfn_for_read = tsfn.clone();
            thread::spawn(move || {
              let mut conout_socket = match conout_clone_thread {
                Some(s) => s,
                None => {
                  error!(
                    "Conout socket is not available for session {}",
                    session_id_clone
                  );
                  if let Some(ref tsfn) = tsfn_for_read {
                    let message = WorkerMessage::Error(format!(
                      "Conout socket is not available for session {}",
                      session_id_clone
                    ));
                    let _ = tsfn.call(Ok(message), ThreadsafeFunctionCallMode::Blocking);
                  }
                  return;
                }
              };

              let mut buffer = [0u8; 4096];
              loop {
                if state_for_read.shutdown_flag.load(Ordering::SeqCst) {
                  info!(
                    "Shutdown flag set. Stopping data piping for session {}.",
                    session_id_clone
                  );
                  let _ = worker_socket_clone.lock().unwrap().shutdown(Shutdown::Both);
                  break;
                }

                match conout_socket.read(&mut buffer) {
                  Ok(0) => {
                    info!("Conout socket closed for session {}", session_id_clone);
                    let _ = worker_socket_clone.lock().unwrap().shutdown(Shutdown::Both);
                    break;
                  }
                  Ok(n) => {
                    if let Err(e) = worker_socket_clone.lock().unwrap().write_all(&buffer[..n]) {
                      error!(
                        "Failed to write to worker socket for session {}: {}",
                        session_id_clone, e
                      );
                      if let Some(ref tsfn) = tsfn_for_read {
                        let message = WorkerMessage::Error(format!(
                          "Failed to write to worker socket for session {}: {}",
                          session_id_clone, e
                        ));
                        let _ = tsfn.call(Ok(message), ThreadsafeFunctionCallMode::Blocking);
                      }
                      let _ = worker_socket_clone.lock().unwrap().shutdown(Shutdown::Both);
                      break;
                    }
                    // Send DataReceived message
                    let data = buffer[..n].to_vec();
                    let message = WorkerMessage::DataReceived(session_id_clone, data);
                    if let Some(ref tsfn) = tsfn_for_read {
                      let _ = tsfn.call(Ok(message), ThreadsafeFunctionCallMode::NonBlocking);
                    }
                  }
                  Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {
                    // No data available
                    thread::sleep(Duration::from_millis(100));
                    continue;
                  }
                  Err(e) => {
                    error!(
                      "Error reading from conout socket for session {}: {}",
                      session_id_clone, e
                    );
                    if let Some(ref tsfn) = tsfn_for_read {
                      let message = WorkerMessage::Error(format!(
                        "Error reading from conout socket for session {}: {}",
                        session_id_clone, e
                      ));
                      let _ = tsfn.call(Ok(message), ThreadsafeFunctionCallMode::Blocking);
                    }
                    let _ = worker_socket_clone.lock().unwrap().shutdown(Shutdown::Both);
                    break;
                  }
                }
              }

              // Remove the session from the sessions map
              {
                let mut sessions = state_for_read.sessions.lock().unwrap();
                sessions.remove(&session_id_clone);
              }
            });

            // Spawn a thread to handle data piping from worker_socket to conout_socket
            let conout_clone_for_write = conout_clone.as_ref().and_then(|s| s.try_clone().ok());
            let tsfn_for_write = tsfn.clone();
            thread::spawn(move || {
              let mut conout_socket = match conout_clone_for_write {
                Some(s) => s,
                None => {
                  error!(
                    "Conout socket is not available for writing for session {}",
                    session_id_clone_write
                  );
                  if let Some(ref tsfn) = tsfn_for_write {
                    let message = WorkerMessage::Error(format!(
                      "Conout socket is not available for writing for session {}",
                      session_id_clone_write
                    ));
                    let _ = tsfn.call(Ok(message), ThreadsafeFunctionCallMode::Blocking);
                  }
                  return;
                }
              };

              let mut buffer = [0u8; 4096];
              loop {
                if state_for_write.shutdown_flag.load(Ordering::SeqCst) {
                  info!(
                    "Shutdown flag set. Stopping data piping for session {}.",
                    session_id_clone_write
                  );
                  let _ = conout_socket.shutdown(Shutdown::Both);
                  break;
                }

                match worker_socket_clone_for_write
                  .lock()
                  .unwrap()
                  .read(&mut buffer)
                {
                  Ok(0) => {
                    info!(
                      "Worker socket closed for session {}",
                      session_id_clone_write
                    );
                    let _ = conout_socket.shutdown(Shutdown::Both);
                    break;
                  }
                  Ok(n) => {
                    if let Err(e) = conout_socket.write_all(&buffer[..n]) {
                      error!(
                        "Failed to write to conout socket for session {}: {}",
                        session_id_clone_write, e
                      );
                      if let Some(ref tsfn) = tsfn_for_write {
                        let message = WorkerMessage::Error(format!(
                          "Failed to write to conout socket for session {}: {}",
                          session_id_clone_write, e
                        ));
                        let _ = tsfn.call(Ok(message), ThreadsafeFunctionCallMode::Blocking);
                      }
                      let _ = conout_socket.shutdown(Shutdown::Both);
                      break;
                    }
                    // Store the received data in the session's buffer
                    if let Some(session) = state_for_write
                      .sessions
                      .lock()
                      .unwrap()
                      .get_mut(&session_id_clone_write)
                    {
                      let mut buffer_lock = session.data_buffer.lock().unwrap();
                      buffer_lock.extend_from_slice(&buffer[..n]);
                    }

                    // Send DataReceived message
                    let data = buffer[..n].to_vec();
                    let message = WorkerMessage::DataReceived(session_id_clone_write, data);
                    if let Some(ref tsfn) = tsfn_for_write {
                      let _ = tsfn.call(Ok(message), ThreadsafeFunctionCallMode::NonBlocking);
                    }
                  }
                  Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {
                    // No data available
                    thread::sleep(Duration::from_millis(100));
                    continue;
                  }
                  Err(e) => {
                    error!(
                      "Error reading from worker socket for session {}: {}",
                      session_id_clone_write, e
                    );
                    if let Some(ref tsfn) = tsfn_for_write {
                      let message = WorkerMessage::Error(format!(
                        "Error reading from worker socket for session {}: {}",
                        session_id_clone_write, e
                      ));
                      let _ = tsfn.call(Ok(message), ThreadsafeFunctionCallMode::Blocking);
                    }
                    let _ = conout_socket.shutdown(Shutdown::Both);
                    break;
                  }
                }
              }

              // Remove the session from the sessions map
              {
                let mut sessions = state_for_write.sessions.lock().unwrap();
                sessions.remove(&session_id_clone_write);
              }
            });
          }
          Err(e) => {
            error!("Worker socket connection failed: {}", e);
            let tsfn_option = state_clone.tsfn.lock().unwrap();
            if let Some(ref tsfn) = *tsfn_option {
              let message = WorkerMessage::Error(format!("Worker socket connection failed: {}", e));
              let _ = tsfn.call(Ok(message), ThreadsafeFunctionCallMode::Blocking);
            }
          }
        }
      }

      // Cleanup on exit
      info!("Worker thread exiting.");
    });

    Ok(())
  }

  /// Shuts down the worker by closing all connections and stopping all threads.
  ///
  /// # Returns
  ///
  /// - `Result<()>`: An empty result indicating success or failure.
  #[napi]
  pub fn shutdown_worker(&self) -> Result<()> {
    // Set the shutdown flag
    self.state.shutdown_flag.store(true, Ordering::SeqCst);

    // Close the conout socket
    {
      let mut conout_lock = self.state.conout_socket.lock().unwrap();
      if let Some(ref mut conout_socket) = *conout_lock {
        let _ = conout_socket.shutdown(Shutdown::Both);
      }
      *conout_lock = None;
    }

    // Close all worker sockets
    {
      let sessions = self.state.sessions.lock().unwrap();
      for session in sessions.values() {
        let _ = session
          .worker_socket
          .lock()
          .unwrap()
          .shutdown(Shutdown::Both);
      }
    }

    // Notify JavaScript about shutdown if needed
    {
      let tsfn_option = self.state.tsfn.lock().unwrap();
      if let Some(ref tsfn) = *tsfn_option {
        let message = WorkerMessage::Ready; // Alternatively, define a specific shutdown message
        let _ = tsfn.call(Ok(message), ThreadsafeFunctionCallMode::NonBlocking);
      }
    }

    info!("shutdown_worker called. All connections have been closed.");
    Ok(())
  }

  /// Lists all active session IDs.
  ///
  /// # Returns
  ///
  /// - `Result<Vec<u32>>`: A list of active session IDs.
  #[napi]
  pub fn list_sessions(&self) -> Result<Vec<u32>> {
    let sessions = self.state.sessions.lock().unwrap();
    let session_ids = sessions.keys().cloned().collect();
    Ok(session_ids)
  }

  /// Retrieves the data buffer for a specific session.
  ///
  /// # Arguments
  ///
  /// - `session_id`: The ID of the session.
  ///
  /// # Returns
  ///
  /// - `Result<Buffer>`: The data buffer associated with the session.
  #[napi]
  pub fn get_session_data(&self, session_id: u32) -> Result<Buffer> {
    let sessions = self.state.sessions.lock().unwrap();
    if let Some(session) = sessions.get(&session_id) {
      let data = session.data_buffer.lock().unwrap().clone();
      Ok(Buffer::from(data))
    } else {
      Err(Error::new(
        Status::InvalidArg,
        format!("Session ID {} does not exist.", session_id),
      ))
    }
  }
}
