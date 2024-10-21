// // src/pty/handle.rs

// use crate::pty::command_handler::handle_commands;
// use crate::pty::commands::{PtyCommand, PtyResult};
// use crate::pty::multiplexer;
// use crate::pty::multiplexer::PtyMultiplexer;
// use crate::pty::platform::PtyProcess;
// use crate::utils::logging::initialize_logging;
// use bytes::Bytes;
// use crossbeam_channel::{bounded, Sender};
// use futures::TryFutureExt;
// use log::{error, info};
// use napi::bindgen_prelude::*;
// use napi_derive::napi;
// use std::env;
// use std::sync::Arc;
// use tokio::sync::oneshot;
// use tokio::sync::{broadcast, Mutex as TokioMutex};
// use tokio::time::{timeout, Duration};

// fn map_to_napi_error<E: std::fmt::Display>(e: E) -> napi::Error {
//   napi::Error::from_reason(e.to_string())
// }

// #[napi(object)]
// #[derive(Debug, Clone)]
// pub struct SplitSessionResult {
//   pub session1: u32,
//   pub session2: u32,
// }

// #[napi(object)]
// #[derive(Debug, Clone)]
// pub struct SessionData {
//   pub session_id: u32,
//   pub data: String,
// }

// #[napi]
// pub struct MultiplexerHandle {
//   multiplexer: Arc<PtyMultiplexer>,
//   shutdown_sender: broadcast::Sender<()>,
// }

// #[napi]
// impl MultiplexerHandle {

//   #[napi(factory)]
//   pub async fn new() -> Result<Self> {
//     initialize_logging();
//     info!("Creating new MultiplexerHandle");

//     // Initialize the PTY process without arguments.
//     let pty_process = tokio::task::spawn_blocking(|| {
//       PtyProcess::new().map_err(|e| {
//         Error::new(
//           Status::GenericFailure,
//           format!("Failed to create PtyProcess: {}", e),
//         )
//       })
//     })
//     .await
//     .map_err(|e| {
//       Error::new(
//         Status::GenericFailure,
//         format!("Failed to join task: {}", e),
//       )
//     })??;

//     // Removed unnecessary environment initialization

//     let multiplexer_result = PtyMultiplexer::new(pty_process).map_err(|e| {
//       Error::new(
//         Status::GenericFailure,
//         format!("Failed to initialize PtyMultiplexer: {}", e),
//       )
//     })?;

//     let multiplexer_arc = Arc::new(multiplexer_result);

//     // Create a broadcast channel for shutdown signaling.
//     let (shutdown_sender, _) = broadcast::channel(1);

//     let multiplexer_clone = Arc::clone(&multiplexer_arc);
//     let mut shutdown_receiver = shutdown_sender.subscribe();

//     // Start the background read and dispatch task with shutdown handling.
//     tokio::spawn(async move {
//       loop {
//         tokio::select! {
//             res = multiplexer_clone.read_and_dispatch() => {
//                 if let Err(e) = res {
//                     error!("Error in background PTY read task: {}", e);
//                     break;
//                 }
//             },
//             _ = shutdown_receiver.recv() => {
//                 info!("Shutdown signal received. Terminating background PTY read task.");
//                 break;
//             },
//         }
//       }
//       info!("Background PTY read task terminated.");
//     });

//     Ok(MultiplexerHandle {
//       multiplexer: Arc::clone(&multiplexer_arc),
//       shutdown_sender,
//     })
//   }

//   #[napi]
//   pub async fn create_session(&self) -> Result<u32> {
//     let multiplexer = Arc::clone(&self.multiplexer);
//     let _multiplexer_clone = Arc::clone(&multiplexer);
//     let session_id = match tokio::task::spawn_blocking({
//       let multiplexer_clone = Arc::clone(&multiplexer);
//       move || multiplexer_clone.create_session()
//     })
//     .await
//     .map_err(|e| map_to_napi_error(format!("Task Join Error: {}", e)))
//     {
//       Ok(result) => result.await.map_err(map_to_napi_error)?,
//       Err(e) => return Err(e),
//     };
//     Ok(session_id)
//   }

//   #[napi]
//   pub async fn send_to_session(&self, session_id: u32, data: Vec<u8>) -> Result<()> {
//     let multiplexer = Arc::clone(&self.multiplexer);
//     let buffer = Buffer::from(data);
//     let multiplexer_clone = Arc::clone(&multiplexer);
//     tokio::task::spawn_blocking({
//       let multiplexer_clone = Arc::clone(&multiplexer_clone);
//       move || multiplexer_clone.send_to_session(session_id, buffer)
//     })
//     .await
//     .map_err(|e| map_to_napi_error(format!("Task Join Error: {}", e)))?
//     .await
//     .map_err(|e| map_to_napi_error(e))
//     ?;
//     Ok(())
//   }

//   #[napi]
//   pub async fn send_to_pty(&self, data: &[u8]) -> Result<()> {
//     let multiplexer = Arc::clone(&self.multiplexer);
//     let buffer = Buffer::from(data.to_vec());
//     let multiplexer_clone = Arc::clone(&multiplexer);
//     let result = tokio::task::spawn_blocking({
//       let multiplexer_clone = Arc::clone(&multiplexer_clone);
//       move || multiplexer_clone.send_to_session(0, buffer)
//     })
//     .await
//     .map_err(|e| map_to_napi_error(format!("Task Join Error: {}", e)))?;
//     result.map_err(|e| map_to_napi_error(e)).await?;
//     Ok(())
//   }

//   #[napi]
//   pub async fn broadcast(&self, data: &[u8]) -> Result<()> {
//     let multiplexer = Arc::clone(&self.multiplexer);
//     let buffer = Buffer::from(data.to_vec());
//     let result = tokio::task::spawn_blocking(move || multiplexer.broadcast(buffer))
//       .await
//       .map_err(|e| map_to_napi_error(format!("Task Join Error: {}", e)))?;
//     result.map_err(|e| map_to_napi_error(e)).await?;
//     Ok(())
//   }

//   #[napi]
//   pub async fn read_from_session(&self, session_id: u32) -> Result<String> {
//     let multiplexer = Arc::clone(&self.multiplexer);
//     let multiplexer_clone = Arc::clone(&multiplexer);
//     let data = tokio::task::spawn_blocking(move || multiplexer_clone.read_from_session(session_id))
//       .await
//       .map_err(|e| map_to_napi_error(format!("Task Join Error: {}", e)))?
//       .await
//       .map_err(map_to_napi_error)?;
//     Ok(String::from_utf8_lossy(&data).to_string())
//   }

//   #[napi]
//   pub async fn read_all_sessions(&self) -> Result<Vec<SessionData>> {
//     let multiplexer = Arc::clone(&self.multiplexer);
//     let sessions_vec: Vec<multiplexer::SessionData> =
//       let sessions_vec = tokio::task::spawn_blocking(move || multiplexer.read_all_sessions())
//         .await
//         .map_err(|e| map_to_napi_error(format!("Task Join Error: {}", e)))?
//         .map_err(map_to_napi_error)?;
//     let sessions: Vec<SessionData> = sessions_vec
//       .into_iter()
//       .map(|session_data| SessionData {
//         session_id: session_data.session_id,
//         data: String::from_utf8_lossy(&session_data.data).to_string(),
//       })
//       .collect();

//     Ok(sessions)
//   }

//   #[napi]
//   pub async fn remove_session(&self, session_id: u32) -> Result<()> {
//     let multiplexer = Arc::clone(&self.multiplexer);
//     let multiplexer_clone = Arc::clone(&multiplexer);
//     let result = tokio::task::spawn_blocking(move || multiplexer_clone.remove_session(session_id))
//       .await
//       .map_err(|e| map_to_napi_error(format!("Task Join Error: {}", e)))?;

//     result.map_err(|e| map_to_napi_error(e)).await?;
//     Ok(())
//   }

//   #[napi]
//   pub async fn merge_sessions(&self, session_ids: Vec<u32>) -> Result<()> {
//     if session_ids.is_empty() {
//       return Err(Error::new(
//         Status::InvalidArg,
//         "No session IDs provided for merging",
//       ));
//     }

//     let multiplexer = Arc::clone(&self.multiplexer);
//     let result = tokio::task::spawn_blocking(move || multiplexer.merge_sessions(session_ids))
//       .await
//       .map_err(|e| map_to_napi_error(format!("Task Join Error: {}", e)))?;

//     result.map_err(|e| map_to_napi_error(e)).await?;
//     Ok(())
//   }

//   #[napi]
//   pub async fn split_session(&self, session_id: u32) -> Result<SplitSessionResult> {
//     let multiplexer = Arc::clone(&self.multiplexer);
//     let split_result = tokio::task::spawn_blocking(move || multiplexer.split_session(session_id))
//       .await
//       .map_err(|e| map_to_napi_error(format!("Task Join Error: {}", e)))?
//       .map_err(map_to_napi_error)?;

//     Ok(SplitSessionResult {
//       session1: split_result[0],
//       session2: split_result[1],
//     })
//   }

//   #[napi]
//   pub async fn set_env(&self, key: String, value: String) -> Result<()> {
//     let multiplexer = Arc::clone(&self.multiplexer);
//     tokio::task::spawn_blocking(move || multiplexer.set_env(key, value))
//       .await
//       .map_err(|e| map_to_napi_error(format!("Task Join Error: {}", e)))?
//       .map_err(|e| map_to_napi_error(e))?;
//     Ok(())
//   }

//   #[napi]
//   pub async fn change_shell(&self, shell_path: String) -> Result<()> {
//     let multiplexer = Arc::clone(&self.multiplexer);
//     tokio::task::spawn_blocking(move || multiplexer.change_shell(shell_path))
//       .await
//       .map_err(|e| map_to_napi_error(format!("Task Join Error: {}", e)))?
//       .map_err(|e| map_to_napi_error(e))?;
//     Ok(())
//   }

//   #[napi]
//   pub async fn status(&self) -> Result<String> {
//     let multiplexer = Arc::clone(&self.multiplexer);
//     let status = tokio::task::spawn_blocking(move || multiplexer.status())
//       .await
//       .map_err(|e| map_to_napi_error(format!("Task Join Error: {}", e)))??
//       .map_err(|e| map_to_napi_error(e))?;
//     Ok(status)
//   }

//   #[napi]
//   pub async fn set_log_level(&self, level: String) -> Result<()> {
//     let multiplexer = Arc::clone(&self.multiplexer);
//     tokio::task::spawn_blocking(move || multiplexer.set_log_level(level))
//       .await
//       .map_err(|e| map_to_napi_error(format!("Task Join Error: {}", e)))?
//       .map_err(|e| map_to_napi_error(e))?;
//     Ok(())
//   }

//   #[napi]
//   pub async fn close_all_sessions(&self) -> Result<()> {
//     let multiplexer = Arc::clone(&self.multiplexer);
//     tokio::task::spawn_blocking(move || multiplexer.close_all_sessions())
//       .await
//       .map_err(|e| map_to_napi_error(format!("Task Join Error: {}", e)))?
//       .map_err(|e| map_to_napi_error(e))?;
//     Ok(())
//   }

//   #[napi]
//   pub async fn shutdown_pty(&self) -> Result<()> {
//     let multiplexer = Arc::clone(&self.multiplexer);
//     let result = tokio::task::spawn_blocking(move || multiplexer.shutdown_pty())
//       .await
//       .map_err(|e| map_to_napi_error(format!("Task Join Error: {}", e)))?;
//     result.map_err(|e| map_to_napi_error(e))?;

//     // Send shutdown signal to background tasks
//     let _ = self.shutdown_sender.send(());

//     Ok(())
//   }
// }

// #[napi]
// pub struct PtyHandle {
//   pty: Arc<TokioMutex<PtyProcess>>,
//   multiplexer: Arc<PtyMultiplexer>,
//   command_sender: Sender<PtyCommand>,
// }

// #[napi]
// impl PtyHandle {
//   #[napi(factory)]
//   pub async fn new() -> Result<Self> {
//     initialize_logging();
//     info!("Creating new PtyHandle");

//     // Create bounded channels for commands and results.
//     let (command_sender, command_receiver) = bounded::<PtyCommand>(100);
//     let (result_sender, _result_receiver) = bounded::<PtyResult>(100);

//     // Initialize the PTY process without arguments.
//     let pty_process = tokio::task::spawn_blocking(|| {
//       PtyProcess::new().map_err(|e| {
//         Error::new(
//           Status::GenericFailure,
//           format!("Failed to initialize PTY process: {}", e),
//         )
//       })
//     })
//     .await
//     .map_err(|e| {
//       Error::new(
//         Status::GenericFailure,
//         format!("Failed to join task: {}", e),
//       )
//     })??;

//     let pty = Arc::new(TokioMutex::new(pty_process.clone()));

//     // Initialize the multiplexer.
//     let multiplexer = PtyMultiplexer::new(env, pty_process).map_err(|e| {
//       Error::new(
//         Status::GenericFailure,
//         format!("Failed to initialize multiplexer: {}", e),
//       )
//     })?;
//     let multiplexer_arc = Arc::new(multiplexer);

//     // Instantiate PtyHandle.
//     let handle = PtyHandle {
//       pty: Arc::clone(&pty),
//       multiplexer: Arc::clone(&multiplexer_arc),
//       command_sender,
//     };

//     // Start the command handler using `spawn_blocking` to avoid runtime nesting.
//     {
//       let pty_clone = Arc::clone(&pty);
//       let multiplexer_clone = Arc::clone(&multiplexer_arc);
//       let command_receiver = command_receiver.clone();
//       let result_sender = result_sender.clone();
//       tokio::task::spawn_blocking(move || {
//         let pty_locked = pty_clone.blocking_lock().clone();
//         handle_commands(
//           Arc::new(parking_lot::Mutex::new(Some(pty_locked))),
//           Arc::new(parking_lot::Mutex::new(Some((*multiplexer_clone).clone()))),
//           command_receiver,
//           result_sender,
//         );
//       });
//     }

//     Ok(handle)
//   }

//   #[napi(getter)]
//   pub fn multiplexer_handle(&self) -> MultiplexerHandle {
//     MultiplexerHandle {
//       multiplexer: Arc::clone(&self.multiplexer),
//       shutdown_sender: self.multiplexer.get_shutdown_sender(),
//     }
//   }

//   #[napi]
//   pub async fn read(&self) -> Result<String> {
//     info!("Initiating read from PTY.");
//     let read_timeout = Duration::from_secs(5);

//     // Create a oneshot channel to receive the read result.
//     let (sender, receiver) = oneshot::channel();

//     // Send the Read command.
//     self
//       .command_sender
//       .send(PtyCommand::Read(sender))
//       .map_err(|e| {
//         error!("Failed to send read command: {}", e);
//         map_to_napi_error(format!("Failed to send read command: {}", e))
//       })?;

//     // Await the result with a timeout.
//     match timeout(read_timeout, receiver).await {
//       Ok(Ok(PtyResult::Data(bytes))) => {
//         let data = String::from_utf8_lossy(&bytes).to_string();
//         info!("Read operation successful: {}", data);
//         Ok(data)
//       }
//       Ok(Ok(PtyResult::Success(msg))) => {
//         info!("Read operation successful: {}", msg);
//         Ok(msg)
//       }
//       Ok(Ok(PtyResult::Failure(msg))) => {
//         error!("Read operation failed: {}", msg);
//         Err(Error::new(Status::Unknown, msg))
//       }
//       Ok(Ok(_)) => {
//         error!("Read operation returned unexpected result");
//         Err(Error::new(
//           Status::Unknown,
//           "Read operation returned unexpected result",
//         ))
//       }
//       Ok(Err(_)) => {
//         error!("Read operation: Receiver dropped");
//         Err(Error::new(
//           Status::Unknown,
//           "Read operation receiver dropped",
//         ))
//       }
//       Err(_) => {
//         error!("Read operation timed out");
//         Err(Error::new(Status::Unknown, "Read operation timed out"))
//       }
//     }
//   }

//   #[napi]
//   pub async fn write(&self, data: String) -> Result<()> {
//     let data_bytes = Bytes::from(data.into_bytes());

//     info!("Initiating write to PTY.");

//     // Create a oneshot channel to receive the write result.
//     let (sender, receiver) = oneshot::channel();

//     // Send the write command along with the responder.
//     self
//       .command_sender
//       .send(PtyCommand::Write(data_bytes, sender))
//       .map_err(|e| {
//         error!("Failed to send write command: {}", e);
//         map_to_napi_error(format!("Failed to send write command: {}", e))
//       })?;

//     // Await the result with a timeout.
//     match timeout(Duration::from_secs(5), receiver).await {
//       Ok(Ok(PtyResult::Success(msg))) => {
//         info!("Write operation successful: {}", msg);
//         Ok(())
//       }
//       Ok(Ok(PtyResult::Failure(msg))) => {
//         error!("Write operation failed: {}", msg);
//         Err(Error::new(Status::Unknown, msg))
//       }
//       Ok(Ok(PtyResult::Data(_))) => {
//         error!("Unexpected data returned in write operation");
//         Err(Error::new(
//           Status::Unknown,
//           "Unexpected data returned in write operation",
//         ))
//       }
//       Ok(Ok(_)) => {
//         error!("Write operation returned unexpected result");
//         Err(Error::new(
//           Status::Unknown,
//           "Write operation returned unexpected result",
//         ))
//       }
//       Ok(Err(_)) => {
//         error!("Write operation: Receiver dropped");
//         Err(Error::new(
//           Status::Unknown,
//           "Write operation receiver dropped",
//         ))
//       }
//       Err(_) => {
//         error!("Write operation timed out");
//         Err(Error::new(Status::Unknown, "Write operation timed out"))
//       }
//     }
//   }

//   #[napi]
//   pub async fn resize(&self, cols: u16, rows: u16) -> Result<()> {
//     info!("Initiating resize of PTY to cols: {}, rows: {}", cols, rows);

//     // Create a oneshot channel to receive the resize result.
//     let (sender, receiver) = oneshot::channel();

//     // Send the resize command along with the responder.
//     self
//       .command_sender
//       .send(PtyCommand::Resize { cols, rows, sender })
//       .map_err(|e| {
//         error!("Failed to send resize command: {}", e);
//         map_to_napi_error(format!("Failed to send resize command: {}", e))
//       })?;

//     // Await the result with a timeout.
//     match timeout(Duration::from_secs(5), receiver).await {
//       Ok(Ok(PtyResult::Success(msg))) => {
//         info!("Resize operation successful: {}", msg);
//         Ok(())
//       }
//       Ok(Ok(PtyResult::Failure(msg))) => {
//         error!("Resize operation failed: {}", msg);
//         // Attempt to force kill if resize fails.
//         self.force_kill(5000).await
//       }
//       Ok(Ok(PtyResult::Data(_))) => {
//         error!("Unexpected data returned in resize operation");
//         Err(Error::new(
//           Status::Unknown,
//           "Unexpected data returned in resize operation",
//         ))
//       }
//       Ok(Ok(_)) => {
//         error!("Resize operation returned unexpected result");
//         Err(Error::new(
//           Status::Unknown,
//           "Resize operation returned unexpected result",
//         ))
//       }
//       Ok(Err(_)) => {
//         error!("Resize operation: Receiver dropped");
//         Err(Error::new(
//           Status::Unknown,
//           "Resize operation receiver dropped",
//         ))
//       }
//       Err(_) => {
//         error!("Resize operation timed out");
//         // Attempt to force kill if resize times out.
//         self.force_kill(5000).await
//       }
//     }
//   }

//   #[napi]
//   pub async fn execute(&self, command: String) -> Result<String> {
//     info!("Executing command in PTY: {}", command);

//     // Create a oneshot channel to receive the execute result.
//     let (sender, receiver) = oneshot::channel();

//     // Send the execute command along with the responder.
//     self
//       .command_sender
//       .send(PtyCommand::Execute(command.clone(), sender))
//       .map_err(|e| {
//         error!("Failed to send execute command: {}", e);
//         map_to_napi_error(format!("Failed to send execute command: {}", e))
//       })?;

//     // Await the result with a timeout.
//     match timeout(Duration::from_secs(5), receiver).await {
//       Ok(Ok(PtyResult::Success(msg))) => {
//         info!("Execute operation successful: {}", msg);
//         Ok(msg)
//       }
//       Ok(Ok(PtyResult::Failure(msg))) => {
//         error!("Execute operation failed: {}", msg);
//         Err(Error::new(Status::Unknown, msg))
//       }
//       Ok(Ok(PtyResult::Data(bytes))) => {
//         let data = String::from_utf8_lossy(&bytes).to_string();
//         info!("Execute operation returned data: {}", data);
//         Ok(data)
//       }
//       Ok(Ok(_)) => {
//         error!("Execute operation returned unexpected result");
//         Err(Error::new(
//           Status::Unknown,
//           "Execute operation returned unexpected result",
//         ))
//       }
//       Ok(Err(_)) => {
//         error!("Execute operation: Receiver dropped");
//         Err(Error::new(
//           Status::Unknown,
//           "Execute operation receiver dropped",
//         ))
//       }
//       Err(_) => {
//         error!("Execute operation timed out");
//         Err(Error::new(Status::Unknown, "Execute operation timed out"))
//       }
//     }
//   }

//   #[napi]
//   pub async fn close(&self) -> Result<()> {
//     info!("Initiating graceful shutdown of PTY.");

//     // Create a oneshot channel to receive the close result.
//     let (sender, receiver) = oneshot::channel();

//     // Send the Close command.
//     self
//       .command_sender
//       .send(PtyCommand::Close(sender))
//       .map_err(|e| {
//         error!("Failed to send close command: {}", e);
//         map_to_napi_error(format!("Failed to send close command: {}", e))
//       })?;

//     // Await the result with a timeout.
//     match timeout(Duration::from_secs(10), receiver).await {
//       Ok(Ok(PtyResult::Success(msg))) => {
//         info!("Close operation successful: {}", msg);
//         Ok(())
//       }
//       Ok(Ok(PtyResult::Failure(msg))) => {
//         error!("Close operation failed: {}", msg);
//         // Attempt to force kill if close fails.
//         self.force_kill(5000).await
//       }
//       Ok(Ok(PtyResult::Data(_))) => {
//         error!("Unexpected data returned in close operation");
//         Err(Error::new(
//           Status::Unknown,
//           "Unexpected data returned in close operation",
//         ))
//       }
//       Ok(Ok(_)) => {
//         error!("Close operation returned unexpected result");
//         Err(Error::new(
//           Status::Unknown,
//           "Close operation returned unexpected result",
//         ))
//       }
//       Ok(Err(_)) => {
//         error!("Close operation: Receiver dropped");
//         Err(Error::new(
//           Status::Unknown,
//           "Close operation receiver dropped",
//         ))
//       }
//       Err(_) => {
//         error!("Close operation timed out");
//         // Attempt to force kill if close times out.
//         self.force_kill(5000).await
//       }
//     }
//   }

//   #[napi]
//   pub async fn force_kill(&self, force_timeout_ms: u32) -> Result<()> {
//     info!("Initiating force kill of PTY.");

//     // Create a oneshot channel to receive the result.
//     let (sender, receiver) = oneshot::channel();

//     // Send the ForceKill command.
//     self
//       .command_sender
//       .send(PtyCommand::ForceKill(sender))
//       .map_err(|e| {
//         error!("Failed to send force kill command: {}", e);
//         map_to_napi_error(format!("Failed to send force kill command: {}", e))
//       })?;

//     // Await the result with a timeout.
//     match timeout(Duration::from_millis(force_timeout_ms as u64), receiver).await {
//       Ok(Ok(PtyResult::Success(msg))) => {
//         info!("Force kill operation successful: {}", msg);
//         Ok(())
//       }
//       Ok(Ok(PtyResult::Failure(msg))) => {
//         error!("Force kill operation failed: {}", msg);
//         Err(Error::new(Status::Unknown, msg))
//       }
//       Ok(Ok(PtyResult::Data(_))) => {
//         error!("Unexpected data returned in force kill operation");
//         Err(Error::new(
//           Status::Unknown,
//           "Unexpected data returned in force kill operation",
//         ))
//       }
//       Ok(Ok(_)) => {
//         error!("Force kill operation returned unexpected result");
//         Err(Error::new(
//           Status::Unknown,
//           "Force kill operation returned unexpected result",
//         ))
//       }
//       Ok(Err(_)) => {
//         error!("Force kill operation: Receiver dropped");
//         Err(Error::new(
//           Status::Unknown,
//           "Force kill operation receiver dropped",
//         ))
//       }
//       Err(_) => {
//         error!("Force kill operation timed out");
//         Err(Error::new(
//           Status::Unknown,
//           "Force kill operation timed out",
//         ))
//       }
//     }
//   }

//   #[napi]
//   pub async fn pid(&self) -> Result<i32> {
//     let pty_guard = self.pty.lock().await;
//     Ok(pty_guard.pid as i32)
//   }

//   #[napi]
//   pub async fn kill_process(&self, signal: i32) -> Result<()> {
//     info!("Initiating kill process with signal {}.", signal);

//     // Create a oneshot channel to receive the result.
//     let (sender, receiver) = oneshot::channel();

//     // Send the KillProcess command.
//     self
//       .command_sender
//       .send(PtyCommand::KillProcess(signal, sender))
//       .map_err(|e| {
//         error!("Failed to send kill process command: {}", e);
//         map_to_napi_error(format!("Failed to send kill process command: {}", e))
//       })?;

//     // Await the result with a timeout.
//     match timeout(Duration::from_secs(5), receiver).await {
//       Ok(Ok(PtyResult::Success(msg))) => {
//         info!("Kill process operation successful: {}", msg);
//         Ok(())
//       }
//       Ok(Ok(PtyResult::Failure(msg))) => {
//         error!("Kill process operation failed: {}", msg);
//         Err(Error::new(Status::Unknown, msg))
//       }
//       Ok(Ok(PtyResult::Data(_))) => {
//         error!("Unexpected data returned in kill process operation");
//         Err(Error::new(
//           Status::Unknown,
//           "Unexpected data returned in kill process operation",
//         ))
//       }
//       Ok(Ok(_)) => {
//         error!("Kill process operation returned unexpected result");
//         Err(Error::new(
//           Status::Unknown,
//           "Kill process operation returned unexpected result",
//         ))
//       }
//       Ok(Err(_)) => {
//         error!("Kill process operation: Receiver dropped");
//         Err(Error::new(
//           Status::Unknown,
//           "Kill process operation receiver dropped",
//         ))
//       }
//       Err(_) => {
//         error!("Kill process operation timed out");
//         Err(Error::new(
//           Status::Unknown,
//           "Kill process operation timed out",
//         ))
//       }
//     }
//   }

//   #[napi]
//   pub async fn waitpid(&self, options: i32) -> Result<i32> {
//     info!("Initiating waitpid with options {}.", options);

//     // Create a oneshot channel to receive the result.
//     let (sender, receiver) = oneshot::channel();

//     // Send the WaitPid command.
//     self
//       .command_sender
//       .send(PtyCommand::WaitPid(options, sender))
//       .map_err(|e| {
//         error!("Failed to send waitpid command: {}", e);
//         map_to_napi_error(format!("Failed to send waitpid command: {}", e))
//       })?;

//     // Await the result with a timeout.
//     match timeout(Duration::from_secs(5), receiver).await {
//       Ok(Ok(PtyResult::Success(pid_str))) => {
//         let pid = pid_str.parse::<i32>().map_err(|e| {
//           error!("Failed to parse PID from response: {}", e);
//           Error::new(Status::Unknown, "Failed to parse PID")
//         })?;
//         info!("Waitpid operation successful, PID: {}", pid);
//         Ok(pid)
//       }
//       Ok(Ok(PtyResult::Failure(msg))) => {
//         error!("Waitpid operation failed: {}", msg);
//         Err(Error::new(Status::Unknown, msg))
//       }
//       Ok(Ok(PtyResult::Data(_))) => {
//         error!("Unexpected data returned in waitpid operation");
//         Err(Error::new(
//           Status::Unknown,
//           "Unexpected data returned in waitpid operation",
//         ))
//       }
//       Ok(Ok(_)) => {
//         error!("Waitpid operation returned unexpected result");
//         Err(Error::new(
//           Status::Unknown,
//           "Waitpid operation returned unexpected result",
//         ))
//       }
//       Ok(Err(_)) => {
//         error!("Waitpid operation: Receiver dropped");
//         Err(Error::new(
//           Status::Unknown,
//           "Waitpid operation receiver dropped",
//         ))
//       }
//       Err(_) => {
//         error!("Waitpid operation timed out");
//         Err(Error::new(Status::Unknown, "Waitpid operation timed out"))
//       }
//     }
//   }

//   #[napi]
//   pub async fn close_master_fd(&self) -> Result<()> {
//     info!("Initiating close_master_fd.");

//     // Create a oneshot channel to receive the result.
//     let (sender, receiver) = oneshot::channel();

//     // Send the CloseMasterFd command.
//     self
//       .command_sender
//       .send(PtyCommand::CloseMasterFd(sender))
//       .map_err(|e| {
//         error!("Failed to send close_master_fd command: {}", e);
//         map_to_napi_error(format!("Failed to send close_master_fd command: {}", e))
//       })?;

//     // Await the result with a timeout.
//     match timeout(Duration::from_secs(5), receiver).await {
//       Ok(Ok(PtyResult::Success(msg))) => {
//         info!("Close master_fd operation successful: {}", msg);
//         Ok(())
//       }
//       Ok(Ok(PtyResult::Failure(msg))) => {
//         error!("Close master_fd operation failed: {}", msg);
//         Err(Error::new(Status::Unknown, msg))
//       }
//       Ok(Ok(PtyResult::Data(_))) => {
//         error!("Unexpected data returned in close_master_fd operation");
//         Err(Error::new(
//           Status::Unknown,
//           "Unexpected data returned in close_master_fd operation",
//         ))
//       }
//       Ok(Ok(_)) => {
//         error!("Close master_fd operation returned unexpected result");
//         Err(Error::new(
//           Status::Unknown,
//           "Close master_fd operation returned unexpected result",
//         ))
//       }
//       Ok(Err(_)) => {
//         error!("Close master_fd operation: Receiver dropped");
//         Err(Error::new(
//           Status::Unknown,
//           "Close master_fd operation receiver dropped",
//         ))
//       }
//       Err(_) => {
//         error!("Close master_fd operation timed out");
//         Err(Error::new(
//           Status::Unknown,
//           "Close master_fd operation timed out",
//         ))
//       }
//     }
//   }

//   #[napi]
//   pub async fn set_env(&self, key: String, value: String) -> Result<()> {
//     // Attempt to set the environment variable.
//     env::set_var(&key, &value);
//     info!("Environment variable set: {} = {}", key, value);

//     // Verify that the environment variable was set correctly.
//     match env::var(&key) {
//       Ok(v) if v == value => {
//         info!("Successfully set environment variable: {} = {}", key, value);
//         Ok(())
//       }
//       Ok(v) => {
//         error!(
//           "Mismatch when setting environment variable: {} = {}, but found {}",
//           key, value, v
//         );
//         Err(Error::new(
//           Status::Unknown,
//           format!("Failed to correctly set environment variable: {}", key),
//         ))
//       }
//       Err(e) => {
//         error!(
//           "Error setting environment variable: {} = {}. Error: {}",
//           key, value, e
//         );
//         Err(Error::new(
//           Status::Unknown,
//           format!(
//             "Error setting environment variable: {} = {}. Error: {}",
//             key, value, e
//           ),
//         ))
//       }
//     }
//   }
// }

// impl Drop for PtyHandle {
//   /// Cleans up resources when a `PtyHandle` is dropped.
//   ///
//   /// Initiates a shutdown of the PTY process and sends a shutdown command through the channel.
//   fn drop(&mut self) {
//     initialize_logging();
//     info!("Dropping PtyHandle, initiating cleanup.");

//     // Create a oneshot channel to receive the shutdown result.
//     let (sender, receiver) = oneshot::channel();

//     // Send the ShutdownPty command.
//     let _ = self.command_sender.send(PtyCommand::ShutdownPty(sender));

//     // Spawn a task to await the shutdown result without blocking.
//     tokio::spawn(async move {
//       let _ = receiver.await;
//     });
//   }
// }

// #[cfg(test)]
// pub(crate) fn new_for_test() -> Result<Self> {
//   use crate::pty::multiplexer::PtyMultiplexer;
//   use crate::pty::platform::PtyProcess;
//   use crate::utils::logging::initialize_logging;
//   use crossbeam_channel::bounded;
//   use std::sync::Arc;
//   use tokio::sync::Mutex as TokioMutex;

//   initialize_logging();
//   info!("Creating new PtyHandle for test.");

//   // Create bounded channels for commands and results.
//   let (command_sender, command_receiver) = bounded::<PtyCommand>(100);
//   let (result_sender, _result_receiver) = bounded::<PtyResult>(100);

//   // Initialize the PTY process without arguments.
//   let pty_process = PtyProcess::new()
//     .map_err(|e| napi::Error::from_reason(format!("Failed to initialize PTY process: {}", e)))?;
//   let pty = Arc::new(TokioMutex::new(pty_process.clone()));

//   let (shutdown_sender, _) = broadcast::channel(1);
//   let multiplexer = PtyMultiplexer {
//     pty_process: Arc::new(pty_process),
//     sessions: Arc::new(parking_lot::Mutex::new(std::collections::HashMap::new())),
//     shutdown_sender,
//   };

//   let multiplexer_arc = Arc::new(multiplexer);

//   // Instantiate PtyHandle.
//   let handle = PtyHandle {
//     pty: Arc::clone(&pty),
//     multiplexer: Arc::clone(&multiplexer_arc),
//     command_sender,
//   };

//   // Start the command handler using `spawn_blocking` to avoid runtime nesting.
//   {
//     let pty_clone = Arc::clone(&pty);
//     let multiplexer_clone = Arc::clone(&multiplexer_arc);
//     tokio::task::spawn_blocking(move || {
//       let pty_locked = pty_clone.blocking_lock().clone();
//       handle_commands(
//         Arc::new(parking_lot::Mutex::new(Some(pty_locked.clone()))),
//         Arc::new(parking_lot::Mutex::new(Some((*multiplexer_clone).clone()))),
//         command_receiver,
//         result_sender,
//       );
//     });
//   }

//   Ok(handle)
// }

// pub async fn set_env(&self, key: String, value: String) -> Result<()> {
//   // Attempt to set the environment variable.
//   env::set_var(&key, &value);
//   info!("Environment variable set: {} = {}", key, value);

//   // Verify that the environment variable was set correctly.
//   match env::var(&key) {
//     Ok(v) if v == value => {
//       info!("Successfully set environment variable: {} = {}", key, value);
//       Ok(())
//     }
//     Ok(v) => {
//       error!(
//         "Mismatch when setting environment variable: {} = {}, but found {}",
//         key, value, v
//       );
//       Err(Error::new(
//         Status::Unknown,
//         format!("Failed to correctly set environment variable: {}", key),
//       ))
//     }
//     Err(e) => {
//       error!(
//         "Error setting environment variable: {} = {}. Error: {}",
//         key, value, e
//       );
//       Err(Error::new(
//         Status::Unknown,
//         format!(
//           "Error setting environment variable: {} = {}. Error: {}",
//           key, value, e
//         ),
//       ))
//     }
//   }
// }

// // Manually implement the N-API binding for the set_env function
// #[napi]
// pub fn set_env_js(env: napi::Env, info: napi::CallContext) -> napi::Result<napi::JsUndefined> {
//   let key = info
//     .get::<napi::JsString>(0)?
//     .into_utf8()?
//     .as_str()?
//     .to_owned();
//   let value = info
//     .get::<napi::JsString>(1)?
//     .into_utf8()?
//     .as_str()?
//     .to_owned();
//   let handle = env.unwrap::<PtyHandle>(&info.this())?;
//   tokio::spawn(async move {
//     if let Err(e) = handle.set_env(key, value).await {
//       napi::Error::from_reason(e.to_string()).throw_into(env);
//     }
//   });
//   env.get_undefined()
// }

// impl Drop for PtyHandle {
//   /// Cleans up resources when a `PtyHandle` is dropped.
//   ///
//   /// Initiates a shutdown of the PTY process and sends a shutdown command through the channel.
//   fn drop(&mut self) {
//     initialize_logging();
//     info!("Dropping PtyHandle, initiating cleanup.");

//     // Create a oneshot channel to receive the shutdown result.
//     let (sender, receiver) = oneshot::channel();

//     // Send the ShutdownPty command.
//     let _ = self.command_sender.send(PtyCommand::ShutdownPty(sender));

//     // Spawn a task to await the shutdown result without blocking.
//     tokio::spawn(async move {
//       let _ = receiver.await;
//     });
//   }
// }
// src/pty/handle.rs

use crate::pty::command_handler::handle_commands;
use crate::pty::commands::{PtyCommand, PtyResult};
use crate::pty::multiplexer::PtyMultiplexer;
use crate::pty::platform::PtyProcess;
use crate::utils::logging::initialize_logging;
use crate::worker::pty_worker::{PtyWorker, WorkerData};
use bytes::Bytes;
use crossbeam_channel::{bounded, Sender};
use log::{error, info};
use napi::bindgen_prelude::*;
use napi::threadsafe_function::{
  ThreadSafeCallContext, ThreadsafeFunction, ThreadsafeFunctionCallMode,
};
use napi::{JsFunction, JsObject, JsUnknown, Result};
use napi_derive::napi;
use serde::{Deserialize, Serialize};
use std::convert::TryInto;
use std::env;
use std::sync::Arc;
use tokio::sync::{broadcast, oneshot, Mutex as TokioMutex};
use tokio::time::{timeout, Duration};

/// Helper function to convert a `PtyProcess` to a `JsObject`.
///
/// This function maps the necessary fields from `PtyProcess` to a JavaScript object.
/// Adjust the fields based on the actual structure of `PtyProcess`.
fn pty_process_to_js_object(env: &Env, pty_process: &PtyProcess) -> napi::Result<JsObject> {
  let mut js_obj = env.create_object()?;
  // Example field: PID
  js_obj.set("pid", pty_process.pid)?;

  // Example field: Multiplexer (assuming PtyProcess has a multiplexer field)
  // Convert the multiplexer to a string representation or another suitable type
  // Assuming PtyProcess has a method to get a public representation of the multiplexer
  js_obj.set("multiplexer", pty_process.get_multiplexer_js(env))?;
  // Example field: Command
  js_obj.set("command", pty_process.command.clone())?;
  // Add other fields as necessary
  // e.g., process status, environment variables, etc.
  Ok(js_obj)
}

/// Represents the result of splitting a session into two.
#[napi(object)]
#[derive(Debug, Clone)]
pub struct SplitSessionResult {
  pub session1: u32,
  pub session2: u32,
}

/// Represents session data returned to JavaScript.
#[napi(object)]
#[derive(Debug, Clone, Deserialize)]
pub struct SessionData {
  pub session_id: u32,
  pub data: String,
}

/// Handle to interact with the PTY process and manage commands.
#[napi]
pub struct PtyHandle {
  multiplexer: Arc<TokioMutex<PtyMultiplexer>>,
  shutdown_sender: broadcast::Sender<()>,
  command_sender: Sender<PtyCommand>,
  worker: Arc<PtyWorker>, // Added field to manage PtyWorker
}

#[napi]
/// The `PtyHandle` struct provides an interface for managing PTY (Pseudo-Terminal) processes.
/// # Methods
/// - `new(env: Env) -> Result<Self>`: Creates a new `PtyHandle`. Initializes the PTY process, sets up the multiplexer, starts the command handler, and initializes the worker.
/// - `get_pid(&self) -> Result<i32>`: Retrieves the PID of the PTY process.
/// - `log_pid(&self) -> Result<()>`: Example method that utilizes the PID.
/// - `read(&self) -> Result<String>`: Reads data from the PTY asynchronously.
/// - `write(&self, data: String) -> Result<()>`: Writes data to the PTY asynchronously.
/// - `resize(&self, cols: u16, rows: u16) -> Result<()>`: Resizes the PTY window asynchronously.
/// - `execute(&self, command: String) -> Result<String>`: Executes a command in the PTY asynchronously.
/// - `close(&self) -> Result<()>`: Gracefully shuts down the PTY process and closes all sessions.
/// - `force_kill(&self, force_timeout_ms: u32) -> Result<()>`: Forcefully shuts down the PTY process after a specified timeout.
/// - `waitpid(&self, options: i32) -> Result<i32>`: Waits for the PTY process to change state based on the provided options.
/// - `set_env(&self, key: String, value: String) -> Result<()>`: Sets an environment variable for the PTY process asynchronously.
/// - `create_session(&self) -> Result<u32>`: Creates a new session and returns its stream ID.
/// - `close_all_sessions(&self) -> Result<()>`: Closes all active sessions.
/// - `send_to_session(&self, session_id: u32, data: Vec<u8>) -> Result<()>`: Sends data to a specific session.
/// - `broadcast(&self, data: Vec<u8>) -> Result<()>`: Broadcasts data to all active sessions.
/// - `read_from_session(&self, session_id: u32) -> Result<String>`: Reads data from a specific session and returns it as a String.
/// - `read_all_sessions(&self) -> Result<Vec<SessionData>>`: Reads data from all sessions and returns a vector of `SessionData`.
/// - `remove_session(&self, session_id: u32) -> Result<()>`: Removes a specific session.
/// - `merge_sessions(&self, session_ids: Vec<u32>) -> Result<()>`: Merges multiple sessions into one.
/// - `split_session(&self, session_id: u32) -> Result<SplitSessionResult>`: Splits a session into two separate sessions.
/// - `change_shell(&self, shell_path: String) -> Result<()>`: Changes the shell of the PTY process.
/// - `status(&self) -> Result<String>`: Retrieves the status of the PTY process.
/// - `set_log_level(&self, level: String) -> Result<()>`: Adjusts the logging level.
/// - `start_worker(&self, worker_data: WorkerData, callback: JsFunction) -> Result<()>`: Starts a new worker with the provided `WorkerData` and JavaScript callback.
/// - `shutdown_worker(&self) -> Result<()>`: Shuts down the worker gracefully.
impl PtyHandle {
  /// Creates a new `PtyHandle`.
  ///
  /// Initializes the PTY process, sets up the multiplexer, starts the command handler,
  /// and initializes the worker.
  #[napi(factory)]
  pub fn new(env: Env) -> Result<Self> {
    initialize_logging();
    info!("Creating new PtyHandle");

    // Create bounded channels for commands and results.
    let (command_sender, command_receiver) = bounded::<PtyCommand>(100);
    let (result_sender, _result_receiver) = bounded::<PtyResult>(100);

    // Initialize the PTY process synchronously.
    let pty_process = PtyProcess::new().map_err(|e| {
      Error::new(
        Status::GenericFailure,
        format!("Failed to initialize PTY process: {}", e),
      )
    })?;

    // Convert the PTY process to a JsObject.
    let pty_js_object = pty_process_to_js_object(&env, &pty_process)?;

    // Initialize the multiplexer with the PTY process
    let multiplexer = PtyMultiplexer::new(env, pty_js_object).map_err(|e| {
      Error::new(
        Status::GenericFailure,
        format!("Failed to initialize multiplexer: {}", e),
      )
    })?;
    let multiplexer_arc = Arc::new(TokioMutex::new(multiplexer));

    // Create a broadcast channel for shutdown signaling.
    let (shutdown_sender, _) = broadcast::channel(1);

    // Instantiate PtyHandle.
    let handle = PtyHandle {
      multiplexer: Arc::clone(&multiplexer_arc),
      shutdown_sender: shutdown_sender.clone(),
      command_sender: command_sender.clone(),
      worker: Arc::new(PtyWorker::new()), // Initialize the worker
    };

    // Start the command handler in a separate blocking thread.
    {
      let command_receiver_clone = command_receiver.clone();
      let result_sender_clone = result_sender.clone();
      let pty_process_clone = pty_process.clone();
      let multiplexer_clone = Arc::clone(&multiplexer_arc);
      std::thread::spawn(move || {
        let pty_process_locked = Arc::new(parking_lot::Mutex::new(Some(pty_process_clone)));
        let multiplexer_locked = Arc::new(parking_lot::Mutex::new(Some(
          futures::executor::block_on(multiplexer_clone.lock()).clone(),
        )));
        handle_commands(
          pty_process_locked,
          multiplexer_locked,
          command_receiver_clone,
          result_sender_clone,
        );
      });
    }

    // Start the background read and dispatch task with shutdown handling.
    {
      let multiplexer_clone = Arc::clone(&multiplexer_arc);
      let mut shutdown_receiver = shutdown_sender.subscribe();

      tokio::spawn(async move {
        loop {
          // Check for shutdown signal
          if let Ok(_) = shutdown_receiver.try_recv() {
            info!("Shutdown signal received. Terminating background PTY read task.");
            break;
          }

          // Lock the multiplexer and perform read_and_dispatch
          let mut multiplexer_locked = multiplexer_clone.lock().await;
          if let Err(e) = multiplexer_locked.read_and_dispatch().await {
            error!("Error in background PTY read task: {}", e);
            break;
          }
          // Release the lock before the next iteration
          drop(multiplexer_locked);

          // Yield to prevent busy-looping
          tokio::task::yield_now().await;
        }
        info!("Background PTY read task terminated.");
      });
    }

    Ok(handle)
  }

  /// Retrieves the PID of the PTY process.
  #[napi]
  pub async fn get_pid(&self) -> Result<i32> {
    let multiplexer_guard = self.multiplexer.lock().await;
    let pid = multiplexer_guard.pty_process.pid;
    info!("Retrieved PTY Process ID: {}", pid);
    Ok(pid)
  }

  /// Example method that utilizes the PID
  #[napi]
  pub async fn log_pid(&self) -> Result<()> {
    let pid = self.get_pid().await?;
    info!("PTY Process ID: {}", pid);
    Ok(())
  }

  /// Reads data from the PTY asynchronously.
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

  /// Writes data to the PTY asynchronously.
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
      Ok(Ok(PtyResult::Success(_))) => {
        info!("Write operation successful.");
        Ok(())
      }
      Ok(Ok(PtyResult::Failure(msg))) => {
        error!("Write operation failed: {}", msg);
        Err(Error::new(Status::Unknown, msg))
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
      _ => {
        error!("Write operation returned unexpected result");
        Err(Error::new(
          Status::Unknown,
          "Write operation returned unexpected result",
        ))
      }
    }
  }

  /// Resizes the PTY window asynchronously.
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
      Ok(Ok(PtyResult::Success(_))) => {
        info!("Resize operation successful.");
        Ok(())
      }
      Ok(Ok(PtyResult::Failure(msg))) => {
        error!("Resize operation failed: {}", msg);
        // Attempt to force kill if resize fails.
        self.force_kill(5000).await
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
      _ => {
        error!("Resize operation returned unexpected result");
        Err(Error::new(
          Status::Unknown,
          "Resize operation returned unexpected result",
        ))
      }
    }
  }

  /// Executes a command in the PTY asynchronously.
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
      _ => {
        error!("Execute operation returned unexpected result");
        Err(Error::new(
          Status::Unknown,
          "Execute operation returned unexpected result",
        ))
      }
    }
  }

  /// Gracefully shuts down the PTY process and closes all sessions.
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
      Ok(Ok(PtyResult::Success(_))) => {
        info!("Close operation successful.");
        Ok(())
      }
      Ok(Ok(PtyResult::Failure(msg))) => {
        error!("Close operation failed: {}", msg);
        // Attempt to force kill if close fails.
        self.force_kill(5000).await
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
      _ => {
        error!("Close operation returned unexpected result");
        Err(Error::new(
          Status::Unknown,
          "Close operation returned unexpected result",
        ))
      }
    }
  }

  /// Forcefully shuts down the PTY process after a specified timeout.
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
      Ok(Ok(PtyResult::Success(_))) => {
        info!("Force kill operation successful.");
        Ok(())
      }
      Ok(Ok(PtyResult::Failure(msg))) => {
        error!("Force kill operation failed: {}", msg);
        Err(Error::new(Status::Unknown, msg))
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
      _ => {
        error!("Force kill operation returned unexpected result");
        Err(Error::new(
          Status::Unknown,
          "Force kill operation returned unexpected result",
        ))
      }
    }
  }

  /// Waits for the PTY process to change state based on the provided options.
  #[napi]
  pub async fn waitpid(&self, options: i32) -> Result<i32> {
    info!("Initiating waitpid with options {}.", options);

    // Create a oneshot channel to receive the waitpid result.
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
        // Parse the PID string to i32
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
      _ => {
        error!("Waitpid operation returned unexpected result");
        Err(Error::new(
          Status::Unknown,
          "Waitpid operation returned unexpected result",
        ))
      }
    }
  }

  /// Sets an environment variable for the PTY process asynchronously.
  ///
  /// This method sets the environment variable and verifies its correctness.
  #[napi]
  pub async fn set_env(&self, key: String, value: String) -> Result<()> {
    info!("Setting environment variable: {} = {}", key, value);

    // Create a oneshot channel to receive the set_env result.
    let (sender, receiver) = oneshot::channel();

    // Send the SetEnv command.
    self
      .command_sender
      .send(PtyCommand::SetEnv(key.clone(), value.clone(), sender))
      .map_err(|e| {
        error!("Failed to send set_env command: {}", e);
        map_to_napi_error(format!("Failed to send set_env command: {}", e))
      })?;

    // Await the result with a timeout.
    match timeout(Duration::from_secs(5), receiver).await {
      Ok(Ok(PtyResult::Success(_))) => {
        info!("SetEnv operation successful.");
        Ok(())
      }
      Ok(Ok(PtyResult::Failure(msg))) => {
        error!("SetEnv operation failed: {}", msg);
        Err(Error::new(Status::Unknown, msg))
      }
      Ok(Err(_)) => {
        error!("SetEnv operation: Receiver dropped");
        Err(Error::new(
          Status::Unknown,
          "SetEnv operation receiver dropped",
        ))
      }
      Err(_) => {
        error!("SetEnv operation timed out");
        Err(Error::new(Status::Unknown, "SetEnv operation timed out"))
      }
      _ => {
        error!("SetEnv operation returned unexpected result");
        Err(Error::new(
          Status::Unknown,
          "SetEnv operation returned unexpected result",
        ))
      }
    }
  }

  /// Creates a new session and returns its stream ID.
  #[napi]
  pub async fn create_session(&self) -> Result<u32> {
    let (sender, receiver) = oneshot::channel();

    // Send the CreateSession command.
    self
      .command_sender
      .send(PtyCommand::CreateSession(sender))
      .map_err(|e| {
        error!("Failed to send create session command: {}", e);
        map_to_napi_error(format!("Failed to send create session command: {}", e))
      })?;

    // Await the result with a timeout.
    match timeout(Duration::from_secs(5), receiver).await {
      Ok(Ok(PtyResult::Success(session_id_str))) => {
        let session_id = session_id_str.parse::<u32>().map_err(|e| {
          error!("Failed to parse session ID from response: {}", e);
          Error::new(Status::Unknown, "Failed to parse session ID")
        })?;
        info!("Create session operation successful: {}", session_id);
        Ok(session_id)
      }
      Ok(Ok(PtyResult::Failure(msg))) => {
        error!("Create session operation failed: {}", msg);
        Err(Error::new(Status::Unknown, msg))
      }
      Ok(Err(_)) => {
        error!("Create session operation: Receiver dropped");
        Err(Error::new(
          Status::Unknown,
          "Create session operation receiver dropped",
        ))
      }
      Err(_) => {
        error!("Create session operation timed out");
        Err(Error::new(
          Status::Unknown,
          "Create session operation timed out",
        ))
      }
      _ => {
        error!("Create session operation returned unexpected result");
        Err(Error::new(
          Status::Unknown,
          "Create session operation returned unexpected result",
        ))
      }
    }
  }

  /// Closes all active sessions.
  ///
  /// This method sends a command to close all active sessions managed by the multiplexer.
  #[napi]
  pub async fn close_all_sessions(&self) -> Result<()> {
    info!("Closing all active sessions.");

    // Create a oneshot channel to receive the close all sessions result.
    let (sender, receiver) = oneshot::channel();

    // Send the CloseAllSessions command.
    self
      .command_sender
      .send(PtyCommand::CloseAllSessions(sender))
      .map_err(|e| {
        error!("Failed to send close all sessions command: {}", e);
        map_to_napi_error(format!("Failed to send close all sessions command: {}", e))
      })?;

    // Await the result with a timeout.
    match timeout(Duration::from_secs(5), receiver).await {
      Ok(Ok(PtyResult::Success(_))) => {
        info!("Close all sessions operation successful.");
        Ok(())
      }
      Ok(Ok(PtyResult::Failure(msg))) => {
        error!("Close all sessions operation failed: {}", msg);
        Err(Error::new(Status::Unknown, msg))
      }
      Ok(Err(_)) => {
        error!("Close all sessions operation: Receiver dropped");
        Err(Error::new(
          Status::Unknown,
          "Close all sessions operation receiver dropped",
        ))
      }
      Err(_) => {
        error!("Close all sessions operation timed out");
        Err(Error::new(
          Status::Unknown,
          "Close all sessions operation timed out",
        ))
      }
      _ => {
        error!("Close all sessions operation returned unexpected result");
        Err(Error::new(
          Status::Unknown,
          "Close all sessions operation returned unexpected result",
        ))
      }
    }
  }

  /// Sends data to a specific session.
  #[napi]
  pub async fn send_to_session(&self, session_id: u32, data: Vec<u8>) -> Result<()> {
    let data_bytes = Bytes::from(data);
    info!("Sending data to session {}.", session_id);

    // Create a oneshot channel to receive the send result.
    let (sender, receiver) = oneshot::channel();

    // Send the SendToSession command.
    self
      .command_sender
      .send(PtyCommand::SendToSession(session_id, data_bytes, sender))
      .map_err(|e| {
        error!("Failed to send to session command: {}", e);
        map_to_napi_error(format!("Failed to send to session command: {}", e))
      })?;

    // Await the result with a timeout.
    match timeout(Duration::from_secs(5), receiver).await {
      Ok(Ok(PtyResult::Success(_))) => {
        info!("Send to session operation successful.");
        Ok(())
      }
      Ok(Ok(PtyResult::Failure(msg))) => {
        error!("Send to session operation failed: {}", msg);
        Err(Error::new(Status::Unknown, msg))
      }
      Ok(Err(_)) => {
        error!("Send to session operation: Receiver dropped");
        Err(Error::new(
          Status::Unknown,
          "Send to session operation receiver dropped",
        ))
      }
      Err(_) => {
        error!("Send to session operation timed out");
        Err(Error::new(
          Status::Unknown,
          "Send to session operation timed out",
        ))
      }
      _ => {
        error!("Send to session operation returned unexpected result");
        Err(Error::new(
          Status::Unknown,
          "Send to session operation returned unexpected result",
        ))
      }
    }
  }

  /// Broadcasts data to all active sessions.
  #[napi]
  pub async fn broadcast(&self, data: Vec<u8>) -> Result<()> {
    let data_bytes = Bytes::from(data);
    info!("Broadcasting data to all sessions.");

    // Create a oneshot channel to receive the broadcast result.
    let (sender, receiver) = oneshot::channel();

    // Send the Broadcast command.
    self
      .command_sender
      .send(PtyCommand::Broadcast(data_bytes, sender))
      .map_err(|e| {
        error!("Failed to send broadcast command: {}", e);
        map_to_napi_error(format!("Failed to send broadcast command: {}", e))
      })?;

    // Await the result with a timeout.
    match timeout(Duration::from_secs(5), receiver).await {
      Ok(Ok(PtyResult::Success(_))) => {
        info!("Broadcast operation successful.");
        Ok(())
      }
      Ok(Ok(PtyResult::Failure(msg))) => {
        error!("Broadcast operation failed: {}", msg);
        Err(Error::new(Status::Unknown, msg))
      }
      Ok(Err(_)) => {
        error!("Broadcast operation: Receiver dropped");
        Err(Error::new(
          Status::Unknown,
          "Broadcast operation receiver dropped",
        ))
      }
      Err(_) => {
        error!("Broadcast operation timed out");
        Err(Error::new(Status::Unknown, "Broadcast operation timed out"))
      }
      _ => {
        error!("Broadcast operation returned unexpected result");
        Err(Error::new(
          Status::Unknown,
          "Broadcast operation returned unexpected result",
        ))
      }
    }
  }

  /// Reads data from a specific session and returns it as a String.
  #[napi]
  pub async fn read_from_session(&self, session_id: u32) -> Result<String> {
    info!("Reading from session {}.", session_id);

    // Create a oneshot channel to receive the read result.
    let (sender, receiver) = oneshot::channel();

    // Send the ReadFromSession command.
    self
      .command_sender
      .send(PtyCommand::ReadFromSession(session_id, sender))
      .map_err(|e| {
        error!("Failed to send read from session command: {}", e);
        map_to_napi_error(format!("Failed to send read from session command: {}", e))
      })?;

    // Await the result with a timeout.
    match timeout(Duration::from_secs(5), receiver).await {
      Ok(Ok(PtyResult::Success(data))) => {
        info!("Read from session {} successful: {}", session_id, data);
        Ok(data)
      }
      Ok(Ok(PtyResult::Failure(msg))) => {
        error!("Read from session {} failed: {}", session_id, msg);
        Err(Error::new(Status::Unknown, msg))
      }
      Ok(Err(_)) => {
        error!("Read from session {}: Receiver dropped", session_id);
        Err(Error::new(
          Status::Unknown,
          "Read from session receiver dropped",
        ))
      }
      Err(_) => {
        error!("Read from session {} timed out", session_id);
        Err(Error::new(Status::Unknown, "Read from session timed out"))
      }
      _ => {
        error!(
          "Read from session {} returned unexpected result",
          session_id
        );
        Err(Error::new(
          Status::Unknown,
          "Read from session operation returned unexpected result",
        ))
      }
    }
  }

  /// Reads data from all sessions and returns a vector of `SessionData`.
  #[napi]
  pub async fn read_all_sessions(&self) -> Result<Vec<SessionData>> {
    info!("Reading from all sessions.");

    // Create a oneshot channel to receive the read all result.
    let (sender, receiver) = oneshot::channel();

    // Send the ReadAllSessions command.
    self
      .command_sender
      .send(PtyCommand::ReadAllSessions(sender))
      .map_err(|e| {
        error!("Failed to send read all sessions command: {}", e);
        map_to_napi_error(format!("Failed to send read all sessions command: {}", e))
      })?;

    // Await the result with a timeout.
    match timeout(Duration::from_secs(5), receiver).await {
      Ok(Ok(PtyResult::Success(serialized_data))) => {
        // Deserialize the JSON string into Vec<SessionData>
        let sessions: Vec<SessionData> = serde_json::from_str(&serialized_data).map_err(|e| {
          error!("Failed to deserialize session data: {}", e);
          map_to_napi_error("Failed to deserialize session data")
        })?;
        info!("Read all sessions successful.");
        Ok(sessions)
      }
      Ok(Ok(PtyResult::Failure(msg))) => {
        error!("Read all sessions failed: {}", msg);
        Err(Error::new(Status::Unknown, msg))
      }
      Ok(Err(_)) => {
        error!("Read all sessions operation: Receiver dropped");
        Err(Error::new(
          Status::Unknown,
          "Read all sessions receiver dropped",
        ))
      }
      Err(_) => {
        error!("Read all sessions timed out");
        Err(Error::new(Status::Unknown, "Read all sessions timed out"))
      }
      _ => {
        error!("Read all sessions operation returned unexpected result");
        Err(Error::new(
          Status::Unknown,
          "Read all sessions operation returned unexpected result",
        ))
      }
    }
  }

  /// Removes a specific session.
  #[napi]
  pub async fn remove_session(&self, session_id: u32) -> Result<()> {
    info!("Removing session {}.", session_id);

    // Create a oneshot channel to receive the remove result.
    let (sender, receiver) = oneshot::channel();

    // Send the RemoveSession command.
    self
      .command_sender
      .send(PtyCommand::RemoveSession(session_id, sender))
      .map_err(|e| {
        error!("Failed to send remove session command: {}", e);
        map_to_napi_error(format!("Failed to send remove session command: {}", e))
      })?;

    // Await the result with a timeout.
    match timeout(Duration::from_secs(5), receiver).await {
      Ok(Ok(PtyResult::Success(_))) => {
        info!("Remove session {} successful.", session_id);
        Ok(())
      }
      Ok(Ok(PtyResult::Failure(msg))) => {
        error!("Remove session {} failed: {}", session_id, msg);
        Err(Error::new(Status::Unknown, msg))
      }
      Ok(Err(_)) => {
        error!("Remove session {}: Receiver dropped", session_id);
        Err(Error::new(
          Status::Unknown,
          "Remove session operation receiver dropped",
        ))
      }
      Err(_) => {
        error!("Remove session {} timed out", session_id);
        Err(Error::new(Status::Unknown, "Remove session timed out"))
      }
      _ => {
        error!("Remove session {} returned unexpected result", session_id);
        Err(Error::new(
          Status::Unknown,
          "Remove session operation returned unexpected result",
        ))
      }
    }
  }

  /// Merges multiple sessions into one.
  #[napi]
  pub async fn merge_sessions(&self, session_ids: Vec<u32>) -> Result<()> {
    if session_ids.is_empty() {
      return Err(Error::new(
        Status::InvalidArg,
        "No session IDs provided for merging",
      ));
    }

    info!("Merging sessions: {:?}", session_ids);

    // Create a oneshot channel to receive the merge result.
    let (sender, receiver) = oneshot::channel();

    // Send the MergeSessions command.
    self
      .command_sender
      .send(PtyCommand::MergeSessions(session_ids, sender))
      .map_err(|e| {
        error!("Failed to send merge sessions command: {}", e);
        map_to_napi_error(format!("Failed to send merge sessions command: {}", e))
      })?;

    // Await the result with a timeout.
    match timeout(Duration::from_secs(5), receiver).await {
      Ok(Ok(PtyResult::Success(_))) => {
        info!("Merge sessions operation successful.");
        Ok(())
      }
      Ok(Ok(PtyResult::Failure(msg))) => {
        error!("Merge sessions operation failed: {}", msg);
        Err(Error::new(Status::Unknown, msg))
      }
      Ok(Err(_)) => {
        error!("Merge sessions operation: Receiver dropped");
        Err(Error::new(
          Status::Unknown,
          "Merge sessions operation receiver dropped",
        ))
      }
      Err(_) => {
        error!("Merge sessions operation timed out");
        Err(Error::new(
          Status::Unknown,
          "Merge sessions operation timed out",
        ))
      }
      _ => {
        error!("Merge sessions operation returned unexpected result");
        Err(Error::new(
          Status::Unknown,
          "Merge sessions operation returned unexpected result",
        ))
      }
    }
  }

  /// Splits a session into two separate sessions.
  #[napi]
  pub async fn split_session(&self, session_id: u32) -> Result<SplitSessionResult> {
    info!("Splitting session {}.", session_id);

    // Create a oneshot channel to receive the split result.
    let (sender, receiver) = oneshot::channel();

    // Send the SplitSession command.
    self
      .command_sender
      .send(PtyCommand::SplitSession(session_id, sender))
      .map_err(|e| {
        error!("Failed to send split session command: {}", e);
        map_to_napi_error(format!("Failed to send split session command: {}", e))
      })?;

    // Await the result with a timeout.
    match timeout(Duration::from_secs(5), receiver).await {
      Ok(Ok(PtyResult::Success(serialized_data))) => {
        // Deserialize the JSON string into [u32; 2]
        let session_ids: [u32; 2] = serde_json::from_str(&serialized_data).map_err(|e| {
          error!("Failed to deserialize split session data: {}", e);
          map_to_napi_error("Failed to deserialize split session data")
        })?;
        info!("Split session operation successful: {:?}", session_ids);
        Ok(SplitSessionResult {
          session1: session_ids[0],
          session2: session_ids[1],
        })
      }
      Ok(Ok(PtyResult::Failure(msg))) => {
        error!("Split session operation failed: {}", msg);
        Err(Error::new(Status::Unknown, msg))
      }
      Ok(Err(_)) => {
        error!("Split session operation: Receiver dropped");
        Err(Error::new(
          Status::Unknown,
          "Split session operation receiver dropped",
        ))
      }
      Err(_) => {
        error!("Split session operation timed out");
        Err(Error::new(
          Status::Unknown,
          "Split session operation timed out",
        ))
      }
      _ => {
        error!("Split session operation returned unexpected result");
        Err(Error::new(
          Status::Unknown,
          "Split session operation returned unexpected result",
        ))
      }
    }
  }

  /// Changes the shell of the PTY process.
  #[napi]
  pub async fn change_shell(&self, shell_path: String) -> Result<()> {
    info!("Changing shell to {}.", shell_path);

    // Create a oneshot channel to receive the change shell result.
    let (sender, receiver) = oneshot::channel();

    // Send the ChangeShell command.
    self
      .command_sender
      .send(PtyCommand::ChangeShell(shell_path.clone(), sender))
      .map_err(|e| {
        error!("Failed to send change shell command: {}", e);
        map_to_napi_error(format!("Failed to send change shell command: {}", e))
      })?;

    // Await the result with a timeout.
    match timeout(Duration::from_secs(5), receiver).await {
      Ok(Ok(PtyResult::Success(_))) => {
        info!("Change shell operation successful.");
        Ok(())
      }
      Ok(Ok(PtyResult::Failure(msg))) => {
        error!("Change shell operation failed: {}", msg);
        Err(Error::new(Status::Unknown, msg))
      }
      Ok(Err(_)) => {
        error!("Change shell operation: Receiver dropped");
        Err(Error::new(
          Status::Unknown,
          "Change shell operation receiver dropped",
        ))
      }
      Err(_) => {
        error!("Change shell operation timed out");
        Err(Error::new(
          Status::Unknown,
          "Change shell operation timed out",
        ))
      }
      _ => {
        error!("Change shell operation returned unexpected result");
        Err(Error::new(
          Status::Unknown,
          "Change shell operation returned unexpected result",
        ))
      }
    }
  }

  /// Retrieves the status of the PTY process.
  #[napi]
  pub async fn status(&self) -> Result<String> {
    info!("Retrieving PTY status.");

    // Create a oneshot channel to receive the status result.
    let (sender, receiver) = oneshot::channel();

    // Send the Status command.
    self
      .command_sender
      .send(PtyCommand::Status(sender))
      .map_err(|e| {
        error!("Failed to send status command: {}", e);
        map_to_napi_error(format!("Failed to send status command: {}", e))
      })?;

    // Await the result with a timeout.
    match timeout(Duration::from_secs(5), receiver).await {
      Ok(Ok(PtyResult::Success(status))) => {
        info!("PTY status retrieved successfully: {}", status);
        Ok(status)
      }
      Ok(Ok(PtyResult::Failure(msg))) => {
        error!("Status operation failed: {}", msg);
        Err(Error::new(Status::Unknown, msg))
      }
      Ok(Err(_)) => {
        error!("Status operation: Receiver dropped");
        Err(Error::new(
          Status::Unknown,
          "Status operation receiver dropped",
        ))
      }
      Err(_) => {
        error!("Status operation timed out");
        Err(Error::new(Status::Unknown, "Status operation timed out"))
      }
      _ => {
        error!("Status operation returned unexpected result");
        Err(Error::new(
          Status::Unknown,
          "Status operation returned unexpected result",
        ))
      }
    }
  }

  /// Adjusts the logging level.
  #[napi]
  pub async fn set_log_level(&self, level: String) -> Result<()> {
    info!("Setting log level to {}.", level);

    // Create a oneshot channel to receive the set log level result.
    let (sender, receiver) = oneshot::channel();

    // Send the SetLogLevel command.
    self
      .command_sender
      .send(PtyCommand::SetLogLevel(level.clone(), sender))
      .map_err(|e| {
        error!("Failed to send set log level command: {}", e);
        map_to_napi_error(format!("Failed to send set log level command: {}", e))
      })?;

    // Await the result with a timeout.
    match timeout(Duration::from_secs(5), receiver).await {
      Ok(Ok(PtyResult::Success(_))) => {
        info!("Set log level operation successful.");
        Ok(())
      }
      Ok(Ok(PtyResult::Failure(msg))) => {
        error!("Set log level operation failed: {}", msg);
        Err(Error::new(Status::Unknown, msg))
      }
      Ok(Err(_)) => {
        error!("Set log level operation: Receiver dropped");
        Err(Error::new(
          Status::Unknown,
          "Set log level operation receiver dropped",
        ))
      }
      Err(_) => {
        error!("Set log level operation timed out");
        Err(Error::new(
          Status::Unknown,
          "Set log level operation timed out",
        ))
      }
      _ => {
        error!("Set log level operation returned unexpected result");
        Err(Error::new(
          Status::Unknown,
          "Set log level operation returned unexpected result",
        ))
      }
    }
  }

  /// Starts a new worker.
  ///
  /// Initializes the worker with the provided `WorkerData` and JavaScript callback.
  #[napi]
  pub fn start_worker(&self, worker_data: WorkerData, callback: JsFunction) -> Result<()> {
    info!("Starting new worker with data: {:?}", worker_data);

    // Start the worker without moving JsFunction into another thread.
    self.worker.start_worker(worker_data, callback)
  }

  /// Shuts down the worker gracefully.
  #[napi]
  pub fn shutdown_worker(&self) -> Result<()> {
    info!("Shutting down worker.");

    self.worker.shutdown_worker()
  }
}

impl Drop for PtyHandle {
  /// Cleans up resources when a `PtyHandle` is dropped.
  ///
  /// Initiates a shutdown of the PTY process and sends a shutdown command through the channel.
  fn drop(&mut self) {
    initialize_logging();
    info!("Dropping PtyHandle, initiating cleanup.");

    // Send shutdown signal to the background read task.
    let _ = self.shutdown_sender.send(());

    // Create a oneshot channel to receive the shutdown result.
    let (sender, _receiver) = oneshot::channel();

    // Send the ShutdownPty command.
    let _ = self.command_sender.send(PtyCommand::ShutdownPty(sender));

    // Initiate worker shutdown
    if let Err(e) = self.worker.shutdown_worker() {
      error!("Failed to shutdown worker during handle drop: {}", e);
    }
  }
}

/// Helper function to convert generic errors into `napi::Error`.
fn map_to_napi_error<E: std::fmt::Display>(e: E) -> napi::Error {
  napi::Error::from_reason(e.to_string())
}

// #[cfg(test)]
// pub(crate) fn new_for_test() -> Result<PtyHandle> {
//   use crate::pty::multiplexer::PtyMultiplexer;
//   use crate::pty::platform::PtyProcess;
//   use crate::utils::logging::initialize_logging;
//   use crossbeam_channel::bounded;
//   use std::sync::Arc;
//   use tokio::sync::Mutex as TokioMutex;

//   initialize_logging();
//   info!("Creating new PtyHandle for test.");

//   // Create bounded channels for commands and results.
//   let (command_sender, command_receiver) = bounded::<PtyCommand>(100);
//   let (result_sender, _result_receiver) = bounded::<PtyResult>(100);

//   // Initialize the PTY process.
//   let pty_process = PtyProcess::new()
//     .map_err(|e| napi::Error::from_reason(format!("Failed to initialize PTY process: {}", e)))?;
//   let pty = Arc::new(TokioMutex::new(pty_process.clone()));

//   // Initialize the multiplexer without Env and JsObject for testing.
//   let multiplexer = PtyMultiplexer::new_for_test(pty_process.clone());
//   let multiplexer_arc = Arc::new(multiplexer);

//   // Instantiate PtyHandle.
//   let handle = PtyHandle {
//     multiplexer: Arc::clone(&multiplexer_arc),
//     shutdown_sender: broadcast::channel(1).0,
//     command_sender: command_sender.clone(),
//   };

//   // Start the command handler.
//   {
//     let multiplexer_clone = Arc::clone(&multiplexer_arc);
//     let command_receiver = command_receiver.clone();
//     let result_sender = result_sender.clone();
//     tokio::task::spawn_blocking(move || {
//       handle_commands(
//         Arc::new(parking_lot::Mutex::new(Some(pty_process))),
//         Arc::new(parking_lot::Mutex::new(Some(multiplexer_arc.clone()))),
//         command_receiver,
//         result_sender,
//       );
//     });
//   }

//   Ok(handle)
// }
