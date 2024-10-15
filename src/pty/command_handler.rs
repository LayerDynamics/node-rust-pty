// src/pty/command_handler.rs

use crate::pty::commands::{PtyCommand, PtyResult};
use crate::pty::multiplexer::PtyMultiplexer;
use crate::pty::platform::PtyProcess;
use bytes::Bytes;
use crossbeam_channel::Receiver;
use log::{debug, error, info};
use parking_lot::Mutex;
use std::io;
use std::sync::Arc;
use tokio::runtime::Builder;
use tokio::sync::oneshot;

/// Handles incoming PTY (Pseudo Terminal) commands by processing them and sending back results.
///
/// This function initializes a Tokio runtime within the thread to execute asynchronous operations.
/// It continuously listens for incoming `PtyCommand` messages from the provided receiver, processes them,
/// and sends back `PtyResult` responses through the responder channels.
///
/// # Parameters
///
/// - `pty`: An `Arc<Mutex<Option<PtyProcess>>>` representing the shared PTY process.
/// - `multiplexer`: An `Arc<Mutex<Option<PtyMultiplexer>>>` representing the shared PTY multiplexer.
/// - `receiver`: A `Receiver<PtyCommand>` channel from which incoming PTY commands are received.
/// - `_result_sender`: A `crossbeam_channel::Sender<PtyResult>` channel intended for sending back results
///   (currently unused).
///
/// # Examples
///
/// ```rust
/// use std::sync::Arc;
/// use parking_lot::Mutex;
/// use crossbeam_channel::unbounded;
///
/// let pty = Arc::new(Mutex::new(Some(PtyProcess::new())));
/// let multiplexer = Arc::new(Mutex::new(Some(PtyMultiplexer::new())));
/// let (sender, receiver) = unbounded();
///
/// handle_commands(pty, multiplexer, receiver, sender);
/// ```
pub fn handle_commands(
  pty: Arc<Mutex<Option<PtyProcess>>>,
  multiplexer: Arc<Mutex<Option<PtyMultiplexer>>>,
  receiver: Receiver<PtyCommand>,
  _result_sender: crossbeam_channel::Sender<PtyResult>, // Currently unused
) {
  // Create a new Tokio runtime for this thread
  let runtime = Builder::new_multi_thread()
    .enable_all()
    .worker_threads(4)
    .build()
    .expect("Failed to create Tokio runtime");

  // Use the runtime to run the async code
  runtime.block_on(async move {
    info!("Command handler started");

    loop {
      match receiver.recv() {
        Ok(command) => {
          debug!("Received command: {:?}", command);
          match command {
            PtyCommand::Write(data, responder) => {
              handle_write(&pty, &data, responder);
            }
            PtyCommand::Read(responder) => {
              handle_read(&pty, responder);
            }
            PtyCommand::Resize { cols, rows, sender } => {
              handle_resize(&pty, cols, rows, sender);
            }
            PtyCommand::Execute(cmd, responder) => {
              handle_execute(&pty, cmd, responder);
            }
            PtyCommand::Close(sender) => {
              handle_close(&pty, sender);
            }
            PtyCommand::ForceKill(sender) => {
              handle_force_kill(&pty, sender);
            }
            PtyCommand::KillProcess(signal, responder) => {
              handle_kill_process(&pty, signal, responder);
            }
            PtyCommand::WaitPid(options, responder) => {
              handle_waitpid(&pty, options, responder);
            }
            PtyCommand::CloseMasterFd(responder) => {
              handle_close_master_fd(&pty, responder);
            }
            PtyCommand::Broadcast(data, responder) => {
              handle_broadcast(&multiplexer, &data, responder);
            }
            PtyCommand::ReadAll(sender) => {
              handle_read_all(&pty, sender);
            }
            PtyCommand::ReadFromSession(session_id, sender) => {
              handle_read_from_session(&multiplexer, session_id, sender);
            }
            PtyCommand::ReadAllSessions(sender) => {
              handle_read_all_sessions(&multiplexer, sender);
            }
            PtyCommand::MergeSessions(session_ids, sender) => {
              handle_merge_sessions(&multiplexer, session_ids, sender);
            }
            PtyCommand::SplitSession(session_id, sender) => {
              handle_split_session(&multiplexer, session_id, sender);
            }
            PtyCommand::SetEnv(key, value, sender) => {
              handle_set_env(&pty, key, value, sender);
            }
            PtyCommand::ChangeShell(shell_path, sender) => {
              handle_change_shell(&pty, shell_path, sender);
            }
            PtyCommand::Status(sender) => {
              handle_status(&pty, sender);
            }
            PtyCommand::SetLogLevel(level, sender) => {
              handle_set_log_level(&pty, level, sender);
            }
            PtyCommand::ShutdownPty(sender) => {
              handle_shutdown_pty(&pty, sender);
            }
          }
        }
        Err(err) => {
          error!("Command receiver encountered an error: {}", err);
          break;
        }
      }
    }

    info!("Command handler terminated");
  });
}

/// Handles the `Write` command by writing data to the PTY process.
///
/// This function attempts to write the provided data to the PTY process. It locks the PTY,
/// performs the write operation, and sends back a `PtyResult` indicating success or failure.
///
/// # Parameters
///
/// - `pty`: A reference to an `Arc<Mutex<Option<PtyProcess>>>` representing the shared PTY process.
/// - `data`: A reference to a `Bytes` object containing the data to be written.
/// - `responder`: A `oneshot::Sender<PtyResult>` channel used to send back the result of the operation.
///
/// # Examples
///
/// ```rust
/// use bytes::Bytes;
/// use tokio::sync::oneshot;
///
/// let pty = Arc::new(Mutex::new(Some(PtyProcess::new())));
/// let data = Bytes::from("Hello, PTY!");
/// let (tx, rx) = oneshot::channel();
///
/// handle_write(&pty, &data, tx);
/// ```
fn handle_write(
  pty: &Arc<Mutex<Option<PtyProcess>>>,
  data: &Bytes,
  responder: oneshot::Sender<PtyResult>,
) {
  debug!("Handling Write command with {} bytes", data.len());
  let write_result = {
    let pty_guard = pty.lock();
    if let Some(ref pty_process) = *pty_guard {
      pty_process.write_data(data)
    } else {
      Err(io::Error::new(
        io::ErrorKind::BrokenPipe,
        "PTY process not initialized",
      ))
    }
  };

  match write_result {
    Ok(bytes_written) => {
      info!("Wrote {} bytes to PTY", bytes_written);
      let _ = responder.send(PtyResult::Success(format!("Wrote {} bytes", bytes_written)));
    }
    Err(e) => {
      error!("Failed to write to PTY: {}", e);
      let _ = responder.send(PtyResult::Failure(format!("Write error: {}", e)));
    }
  }
}

/// Handles the `Read` command by reading data from the PTY process.
///
/// This function attempts to read data from the PTY process into a buffer. It locks the PTY,
/// performs the read operation, and sends back a `PtyResult` containing the read data or an error.
///
/// # Parameters
///
/// - `pty`: A reference to an `Arc<Mutex<Option<PtyProcess>>>` representing the shared PTY process.
/// - `responder`: A `oneshot::Sender<PtyResult>` channel used to send back the result of the operation.
///
/// # Examples
///
/// ```rust
/// use tokio::sync::oneshot;
///
/// let pty = Arc::new(Mutex::new(Some(PtyProcess::new())));
/// let (tx, rx) = oneshot::channel();
///
/// handle_read(&pty, tx);
/// ```
fn handle_read(pty: &Arc<Mutex<Option<PtyProcess>>>, responder: oneshot::Sender<PtyResult>) {
  debug!("Handling Read command");
  let read_result = {
    let pty_guard = pty.lock();
    if let Some(ref pty_process) = *pty_guard {
      let mut buffer = [0u8; 4096];
      match pty_process.read_data(&mut buffer) {
        Ok(bytes_read) => Ok(Bytes::copy_from_slice(&buffer[..bytes_read])),
        Err(e) => Err(e),
      }
    } else {
      Err(io::Error::new(
        io::ErrorKind::BrokenPipe,
        "PTY process not initialized",
      ))
    }
  };

  match read_result {
    Ok(data) => {
      info!("Read {} bytes from PTY", data.len());
      let _ = responder.send(PtyResult::Data(data));
    }
    Err(e) => {
      error!("Failed to read from PTY: {}", e);
      let _ = responder.send(PtyResult::Failure(format!("Read error: {}", e)));
    }
  }
}

/// Handles the `Resize` command by resizing the PTY dimensions.
///
/// This function attempts to resize the PTY to the specified number of columns and rows.
/// It locks the PTY, performs the resize operation, and sends back a `PtyResult` indicating
/// success or failure.
///
/// # Parameters
///
/// - `pty`: A reference to an `Arc<Mutex<Option<PtyProcess>>>` representing the shared PTY process.
/// - `cols`: The desired number of columns for the PTY.
/// - `rows`: The desired number of rows for the PTY.
/// - `sender`: A `oneshot::Sender<PtyResult>` channel used to send back the result of the operation.
///
/// # Examples
///
/// ```rust
/// use tokio::sync::oneshot;
///
/// let pty = Arc::new(Mutex::new(Some(PtyProcess::new())));
/// let (tx, rx) = oneshot::channel();
///
/// handle_resize(&pty, 80, 24, tx);
/// ```
fn handle_resize(
  pty: &Arc<Mutex<Option<PtyProcess>>>,
  cols: u16,
  rows: u16,
  sender: oneshot::Sender<PtyResult>,
) {
  debug!("Handling Resize command to cols: {}, rows: {}", cols, rows);
  let resize_result = {
    let pty_guard = pty.lock();
    if let Some(ref pty_process) = *pty_guard {
      pty_process.resize(cols, rows)
    } else {
      Err(io::Error::new(
        io::ErrorKind::BrokenPipe,
        "PTY process not initialized",
      ))
    }
  };

  match resize_result {
    Ok(_) => {
      info!("Resized PTY to cols: {}, rows: {}", cols, rows);
      let _ = sender.send(PtyResult::Success(format!(
        "Resized to cols: {}, rows: {}",
        cols, rows
      )));
    }
    Err(e) => {
      error!("Failed to resize PTY: {}", e);
      let _ = sender.send(PtyResult::Failure(format!("Resize error: {}", e)));
    }
  }
}

/// Handles the `Execute` command by executing a command in the PTY process.
///
/// This function writes the provided command followed by a newline character to the PTY process.
/// It locks the PTY, performs the write operation, and sends back a `PtyResult` indicating
/// success or failure.
///
/// # Parameters
///
/// - `pty`: A reference to an `Arc<Mutex<Option<PtyProcess>>>` representing the shared PTY process.
/// - `command`: A `String` containing the command to be executed.
/// - `responder`: A `oneshot::Sender<PtyResult>` channel used to send back the result of the operation.
///
/// # Examples
///
/// ```rust
/// use tokio::sync::oneshot;
///
/// let pty = Arc::new(Mutex::new(Some(PtyProcess::new())));
/// let command = "ls -la".to_string();
/// let (tx, rx) = oneshot::channel();
///
/// handle_execute(&pty, command, tx);
/// ```
fn handle_execute(
  pty: &Arc<Mutex<Option<PtyProcess>>>,
  command: String,
  responder: oneshot::Sender<PtyResult>,
) {
  debug!("Handling Execute command: {}", command);
  let execute_result = {
    let pty_guard = pty.lock();
    if let Some(ref pty_process) = *pty_guard {
      pty_process.write_data(&Bytes::from(format!("{}\n", command).into_bytes()))
    } else {
      Err(io::Error::new(
        io::ErrorKind::BrokenPipe,
        "PTY process not initialized",
      ))
    }
  };

  match execute_result {
    Ok(bytes_written) => {
      info!(
        "Executed command '{}' by writing {} bytes",
        command, bytes_written
      );
      let _ = responder.send(PtyResult::Success(format!(
        "Executed command '{}'",
        command
      )));
    }
    Err(e) => {
      error!("Failed to execute command '{}': {}", command, e);
      let _ = responder.send(PtyResult::Failure(format!(
        "Execution error for command '{}': {}",
        command, e
      )));
    }
  }
}

/// Handles the `Close` command by gracefully closing the PTY process.
///
/// This function attempts to gracefully terminate the PTY process by sending a `SIGTERM` signal,
/// waiting for the process to terminate, and closing the master file descriptor.
/// It locks the PTY, performs the operations, and sends back a `PtyResult` indicating
/// success or failure.
///
/// # Parameters
///
/// - `pty`: A reference to an `Arc<Mutex<Option<PtyProcess>>>` representing the shared PTY process.
/// - `sender`: A `oneshot::Sender<PtyResult>` channel used to send back the result of the operation.
///
/// # Examples
///
/// ```rust
/// use tokio::sync::oneshot;
///
/// let pty = Arc::new(Mutex::new(Some(PtyProcess::new())));
/// let (tx, rx) = oneshot::channel();
///
/// handle_close(&pty, tx);
/// ```
fn handle_close(pty: &Arc<Mutex<Option<PtyProcess>>>, sender: oneshot::Sender<PtyResult>) {
  debug!("Handling Close command");
  let close_result = {
    let mut pty_option = pty.lock();
    if let Some(pty_process) = pty_option.take() {
      // Attempt to gracefully terminate the PTY process
      if let Err(e) = pty_process.kill_process(libc::SIGTERM) {
        error!("Failed to send SIGTERM to PTY process: {}", e);
      }
      // Wait for the process to terminate
      if let Err(e) = pty_process.waitpid(libc::WNOHANG) {
        error!("Failed to waitpid on PTY process: {}", e);
      }
      // Close the master FD
      pty_process.close_master_fd()
    } else {
      Ok(())
    }
  };

  match close_result {
    Ok(_) => {
      info!("Closed PTY process successfully");
      let _ = sender.send(PtyResult::Success("PTY closed successfully".to_string()));
    }
    Err(e) => {
      error!("Failed to close PTY process: {}", e);
      let _ = sender.send(PtyResult::Failure(format!("Close error: {}", e)));
    }
  }
}

/// Handles the `ForceKill` command by forcefully terminating the PTY process.
///
/// This function attempts to forcefully kill the PTY process by sending a `SIGKILL` signal,
/// waiting for the process to terminate, and closing the master file descriptor.
/// It locks the PTY, performs the operations, and sends back a `PtyResult` indicating
/// success or failure.
///
/// # Parameters
///
/// - `pty`: A reference to an `Arc<Mutex<Option<PtyProcess>>>` representing the shared PTY process.
/// - `sender`: A `oneshot::Sender<PtyResult>` channel used to send back the result of the operation.
///
/// # Examples
///
/// ```rust
/// use tokio::sync::oneshot;
///
/// let pty = Arc::new(Mutex::new(Some(PtyProcess::new())));
/// let (tx, rx) = oneshot::channel();
///
/// handle_force_kill(&pty, tx);
/// ```
fn handle_force_kill(pty: &Arc<Mutex<Option<PtyProcess>>>, sender: oneshot::Sender<PtyResult>) {
  debug!("Handling ForceKill command");
  let force_kill_result = {
    let mut pty_option = pty.lock();
    if let Some(pty_process) = pty_option.take() {
      // Forcefully kill the PTY process
      if let Err(e) = pty_process.kill_process(libc::SIGKILL) {
        error!("Failed to send SIGKILL to PTY process: {}", e);
      }
      // Wait for the process to terminate
      if let Err(e) = pty_process.waitpid(0) {
        error!("Failed to waitpid on PTY process: {}", e);
      }
      // Close the master FD
      pty_process.close_master_fd()
    } else {
      Err(io::Error::new(
        io::ErrorKind::BrokenPipe,
        "PTY process not initialized",
      ))
    }
  };

  match force_kill_result {
    Ok(_) => {
      info!("Force killed PTY process successfully");
      let _ = sender.send(PtyResult::Success(
        "Force killed PTY process successfully".to_string(),
      ));
    }
    Err(e) => {
      error!("Failed to force kill PTY process: {}", e);
      let _ = sender.send(PtyResult::Failure(format!("Force kill error: {}", e)));
    }
  }
}

/// Handles the `KillProcess` command by sending a specified signal to the PTY process.
///
/// This function attempts to send the provided signal to the PTY process.
/// It locks the PTY, performs the kill operation, and sends back a `PtyResult` indicating
/// success or failure.
///
/// # Parameters
///
/// - `pty`: A reference to an `Arc<Mutex<Option<PtyProcess>>>` representing the shared PTY process.
/// - `signal`: An `i32` representing the signal number to be sent to the PTY process.
/// - `responder`: A `oneshot::Sender<PtyResult>` channel used to send back the result of the operation.
///
/// # Examples
///
/// ```rust
/// use tokio::sync::oneshot;
///
/// let pty = Arc::new(Mutex::new(Some(PtyProcess::new())));
/// let signal = libc::SIGTERM;
/// let (tx, rx) = oneshot::channel();
///
/// handle_kill_process(&pty, signal, tx);
/// ```
fn handle_kill_process(
  pty: &Arc<Mutex<Option<PtyProcess>>>,
  signal: i32,
  responder: oneshot::Sender<PtyResult>,
) {
  debug!("Handling KillProcess command with signal {}", signal);
  let kill_result = {
    let pty_guard = pty.lock();
    if let Some(ref pty_process) = *pty_guard {
      pty_process.kill_process(signal)
    } else {
      Err(io::Error::new(
        io::ErrorKind::BrokenPipe,
        "PTY process not initialized",
      ))
    }
  };

  match kill_result {
    Ok(_) => {
      info!("Sent signal {} to PTY process successfully", signal);
      let _ = responder.send(PtyResult::Success(format!(
        "Sent signal {} successfully",
        signal
      )));
    }
    Err(e) => {
      error!("Failed to send signal {}: {}", signal, e);
      let _ = responder.send(PtyResult::Failure(format!("KillProcess error: {}", e)));
    }
  }
}

/// Handles the `WaitPid` command by waiting for the PTY process to change state.
///
/// This function attempts to wait for the PTY process using the provided options.
/// It locks the PTY, performs the waitpid operation, and sends back a `PtyResult` containing
/// the PID or an error.
///
/// # Parameters
///
/// - `pty`: A reference to an `Arc<Mutex<Option<PtyProcess>>>` representing the shared PTY process.
/// - `options`: An `i32` representing options for the waitpid system call.
/// - `responder`: A `oneshot::Sender<PtyResult>` channel used to send back the result of the operation.
///
/// # Examples
///
/// ```rust
/// use tokio::sync::oneshot;
///
/// let pty = Arc::new(Mutex::new(Some(PtyProcess::new())));
/// let options = 0; // Example option
/// let (tx, rx) = oneshot::channel();
///
/// handle_waitpid(&pty, options, tx);
/// ```
fn handle_waitpid(
  pty: &Arc<Mutex<Option<PtyProcess>>>,
  options: i32,
  responder: oneshot::Sender<PtyResult>,
) {
  debug!("Handling WaitPid command with options {}", options);
  let wait_result = {
    let pty_guard = pty.lock();
    if let Some(ref pty_process) = *pty_guard {
      pty_process.waitpid(options)
    } else {
      Err(io::Error::new(
        io::ErrorKind::BrokenPipe,
        "PTY process not initialized",
      ))
    }
  };

  match wait_result {
    Ok(pid) => {
      info!("Waitpid returned with PID {}", pid);
      let _ = responder.send(PtyResult::Success(pid.to_string()));
    }
    Err(e) => {
      error!("Failed to waitpid: {}", e);
      let _ = responder.send(PtyResult::Failure(format!("WaitPid error: {}", e)));
    }
  }
}

/// Handles the `CloseMasterFd` command by closing the master file descriptor of the PTY.
///
/// This function attempts to close the master file descriptor associated with the PTY process.
/// It locks the PTY, performs the close operation, and sends back a `PtyResult` indicating
/// success or failure.
///
/// # Parameters
///
/// - `pty`: A reference to an `Arc<Mutex<Option<PtyProcess>>>` representing the shared PTY process.
/// - `responder`: A `oneshot::Sender<PtyResult>` channel used to send back the result of the operation.
///
/// # Examples
///
/// ```rust
/// use tokio::sync::oneshot;
///
/// let pty = Arc::new(Mutex::new(Some(PtyProcess::new())));
/// let (tx, rx) = oneshot::channel();
///
/// handle_close_master_fd(&pty, tx);
/// ```
fn handle_close_master_fd(
  pty: &Arc<Mutex<Option<PtyProcess>>>,
  responder: oneshot::Sender<PtyResult>,
) {
  debug!("Handling CloseMasterFd command");
  let close_result = {
    let pty_guard = pty.lock();
    if let Some(ref pty_process) = *pty_guard {
      pty_process.close_master_fd()
    } else {
      Err(io::Error::new(
        io::ErrorKind::BrokenPipe,
        "PTY process not initialized",
      ))
    }
  };

  match close_result {
    Ok(_) => {
      info!("Closed master_fd successfully");
      let _ = responder.send(PtyResult::Success(
        "Closed master_fd successfully".to_string(),
      ));
    }
    Err(e) => {
      error!("Failed to close master_fd: {}", e);
      let _ = responder.send(PtyResult::Failure(format!("Close error: {}", e)));
    }
  }
}

/// Handles the `Broadcast` command by broadcasting data to all sessions in the multiplexer.
///
/// This function attempts to broadcast the provided data to all active sessions managed by the multiplexer.
/// It locks the multiplexer, performs the broadcast operation, and sends back a `PtyResult` indicating
/// success or failure.
///
/// # Parameters
///
/// - `multiplexer`: A reference to an `Arc<Mutex<Option<PtyMultiplexer>>>` representing the shared multiplexer.
/// - `data`: A reference to a `Bytes` object containing the data to be broadcasted.
/// - `responder`: A `oneshot::Sender<PtyResult>` channel used to send back the result of the operation.
///
/// # Examples
///
/// ```rust
/// use bytes::Bytes;
/// use tokio::sync::oneshot;
///
/// let multiplexer = Arc::new(Mutex::new(Some(PtyMultiplexer::new())));
/// let data = Bytes::from("Broadcast message");
/// let (tx, rx) = oneshot::channel();
///
/// handle_broadcast(&multiplexer, &data, tx);
/// ```
fn handle_broadcast(
  multiplexer: &Arc<Mutex<Option<PtyMultiplexer>>>,
  data: &Bytes,
  responder: oneshot::Sender<PtyResult>,
) {
  debug!("Handling Broadcast command with {} bytes", data.len());
  let broadcast_result = {
    let mut multiplexer_guard = multiplexer.lock();
    if let Some(ref mut multiplexer) = *multiplexer_guard {
      multiplexer.broadcast((&data[..]).into()).map_err(|e| {
        io::Error::new(
          io::ErrorKind::Other,
          format!("Failed to broadcast data: {}", e),
        )
      })
    } else {
      Err(io::Error::new(
        io::ErrorKind::BrokenPipe,
        "Multiplexer not initialized",
      ))
    }
  };

  match broadcast_result {
    Ok(_) => {
      info!("Broadcasted data to all sessions");
      let _ = responder.send(PtyResult::Success("Broadcast successful".to_string()));
    }
    Err(e) => {
      error!("Failed to broadcast data: {}", e);
      let _ = responder.send(PtyResult::Failure(format!("Broadcast error: {}", e)));
    }
  }
}

/// Handles the `ReadAll` command by reading all available data from the PTY process.
///
/// This function attempts to read all available data from the PTY process into a buffer.
/// It locks the PTY, performs the read operation, and sends back a `PtyResult` containing
/// the read data or an error.
///
/// # Parameters
///
/// - `pty`: A reference to an `Arc<Mutex<Option<PtyProcess>>>` representing the shared PTY process.
/// - `sender`: A `oneshot::Sender<PtyResult>` channel used to send back the result of the operation.
///
/// # Examples
///
/// ```rust
/// use tokio::sync::oneshot;
///
/// let pty = Arc::new(Mutex::new(Some(PtyProcess::new())));
/// let (tx, rx) = oneshot::channel();
///
/// handle_read_all(&pty, tx);
/// ```
fn handle_read_all(pty: &Arc<Mutex<Option<PtyProcess>>>, sender: oneshot::Sender<PtyResult>) {
  debug!("Handling ReadAll command");
  let read_all_result = {
    let pty_guard = pty.lock();
    if let Some(ref pty_process) = *pty_guard {
      let mut buffer = [0u8; 4096];
      match pty_process.read_data(&mut buffer) {
        Ok(bytes_read) => Ok(Bytes::copy_from_slice(&buffer[..bytes_read])),
        Err(e) => Err(e),
      }
    } else {
      Err(io::Error::new(
        io::ErrorKind::BrokenPipe,
        "PTY process not initialized",
      ))
    }
  };

  match read_all_result {
    Ok(data) => {
      info!("Read {} bytes from PTY", data.len());
      let _ = sender.send(PtyResult::Data(data));
    }
    Err(e) => {
      error!("Failed to read from PTY: {}", e);
      let _ = sender.send(PtyResult::Failure(format!("ReadAll error: {}", e)));
    }
  }
}

/// Handles the `ReadFromSession` command by reading data from a specific session in the multiplexer.
///
/// This function attempts to read data from the specified session within the multiplexer.
/// It locks the multiplexer, performs the read operation, and sends back a `PtyResult` containing
/// the read data or an error.
///
/// # Parameters
///
/// - `multiplexer`: A reference to an `Arc<Mutex<Option<PtyMultiplexer>>>` representing the shared multiplexer.
/// - `session_id`: A `u32` identifying the session from which to read data.
/// - `sender`: A `oneshot::Sender<PtyResult>` channel used to send back the result of the operation.
///
/// # Examples
///
/// ```rust
/// use tokio::sync::oneshot;
///
/// let multiplexer = Arc::new(Mutex::new(Some(PtyMultiplexer::new())));
/// let session_id = 1;
/// let (tx, rx) = oneshot::channel();
///
/// handle_read_from_session(&multiplexer, session_id, tx);
/// ```
fn handle_read_from_session(
  multiplexer: &Arc<Mutex<Option<PtyMultiplexer>>>,
  session_id: u32,
  sender: oneshot::Sender<PtyResult>,
) {
  debug!(
    "Handling ReadFromSession command for session {}",
    session_id
  );

  let multiplexer_guard = multiplexer.lock();
  if let Some(ref multiplexer) = *multiplexer_guard {
    let data_result = multiplexer.read_from_session(session_id).map_err(|e| {
      io::Error::new(
        io::ErrorKind::Other,
        format!("Failed to read from session {}: {}", session_id, e),
      )
    });

    match data_result {
      Ok(data) => {
        let _ = sender.send(PtyResult::Success(
          String::from_utf8_lossy(&data).to_string(),
        ));
      }
      Err(e) => {
        error!("Failed to read from session {}: {}", session_id, e);
        let _ = sender.send(PtyResult::Failure(e.to_string()));
      }
    }
  } else {
    let _ = sender.send(PtyResult::Failure(
      "Multiplexer not initialized".to_string(),
    ));
  }
}

/// Handles the `ReadAllSessions` command by reading data from all sessions in the multiplexer.
///
/// This function attempts to read data from all active sessions managed by the multiplexer.
/// It locks the multiplexer, performs the read operations, aggregates the data, and sends back
/// a `PtyResult` containing the aggregated data or an error.
///
/// # Parameters
///
/// - `multiplexer`: A reference to an `Arc<Mutex<Option<PtyMultiplexer>>>` representing the shared multiplexer.
/// - `sender`: A `oneshot::Sender<PtyResult>` channel used to send back the result of the operation.
///
/// # Examples
///
/// ```rust
/// use tokio::sync::oneshot;
///
/// let multiplexer = Arc::new(Mutex::new(Some(PtyMultiplexer::new())));
/// let (tx, rx) = oneshot::channel();
///
/// handle_read_all_sessions(&multiplexer, tx);
/// ```
fn handle_read_all_sessions(
  multiplexer: &Arc<Mutex<Option<PtyMultiplexer>>>,
  sender: oneshot::Sender<PtyResult>,
) {
  debug!("Handling ReadAllSessions command");

  let multiplexer_guard = multiplexer.lock();
  if let Some(ref multiplexer) = *multiplexer_guard {
    let sessions_map = multiplexer
      .read_all_sessions()
      .map_err(|e| {
        io::Error::new(
          io::ErrorKind::Other,
          format!("Failed to read sessions: {}", e),
        )
      })
      .unwrap();

    let mut all_session_data = Vec::new();
    for session in sessions_map {
      let session_id = session.session_id;
      all_session_data.push(format!(
        "Session {}: {}",
        session_id,
        String::from_utf8_lossy(&session.data)
      ));
    }
    let result = all_session_data.join("\n");
    let _ = sender.send(PtyResult::Success(result));
  } else {
    let _ = sender.send(PtyResult::Failure(
      "Multiplexer not initialized".to_string(),
    ));
  }
}

/// Handles the `MergeSessions` command by merging multiple sessions in the multiplexer.
///
/// This function attempts to merge the specified sessions within the multiplexer.
/// It locks the multiplexer, performs the merge operation, and sends back a `PtyResult` indicating
/// success or failure.
///
/// # Parameters
///
/// - `multiplexer`: A reference to an `Arc<Mutex<Option<PtyMultiplexer>>>` representing the shared multiplexer.
/// - `session_ids`: A `Vec<u32>` containing the IDs of the sessions to be merged.
/// - `sender`: A `oneshot::Sender<PtyResult>` channel used to send back the result of the operation.
///
/// # Examples
///
/// ```rust
/// use tokio::sync::oneshot;
///
/// let multiplexer = Arc::new(Mutex::new(Some(PtyMultiplexer::new())));
/// let session_ids = vec![1, 2, 3];
/// let (tx, rx) = oneshot::channel();
///
/// handle_merge_sessions(&multiplexer, session_ids, tx);
/// ```
fn handle_merge_sessions(
  multiplexer: &Arc<Mutex<Option<PtyMultiplexer>>>,
  session_ids: Vec<u32>,
  sender: oneshot::Sender<PtyResult>,
) {
  debug!(
    "Handling MergeSessions command for sessions {:?}",
    session_ids
  );

  let mut multiplexer_guard = multiplexer.lock();
  if let Some(ref mut multiplexer) = *multiplexer_guard {
    let merge_result = multiplexer.merge_sessions(session_ids.clone());

    match merge_result {
      Ok(_) => {
        let _ = sender.send(PtyResult::Success(format!(
          "Merged sessions {:?}",
          session_ids
        )));
      }
      Err(e) => {
        error!("Failed to merge sessions: {}", e);
        let _ = sender.send(PtyResult::Failure(format!("MergeSessions error: {}", e)));
      }
    }
  } else {
    let _ = sender.send(PtyResult::Failure(
      "Multiplexer not initialized".to_string(),
    ));
  }
}

/// Handles the `SplitSession` command by splitting a session into sub-sessions in the multiplexer.
///
/// This function attempts to split the specified session into sub-sessions within the multiplexer.
/// It locks the multiplexer, performs the split operation, and sends back a `PtyResult` indicating
/// success or failure.
///
/// # Parameters
///
/// - `multiplexer`: A reference to an `Arc<Mutex<Option<PtyMultiplexer>>>` representing the shared multiplexer.
/// - `session_id`: A `u32` identifying the session to be split.
/// - `sender`: A `oneshot::Sender<PtyResult>` channel used to send back the result of the operation.
///
/// # Examples
///
/// ```rust
/// use tokio::sync::oneshot;
///
/// let multiplexer = Arc::new(Mutex::new(Some(PtyMultiplexer::new())));
/// let session_id = 1;
/// let (tx, rx) = oneshot::channel();
///
/// handle_split_session(&multiplexer, session_id, tx);
/// ```
fn handle_split_session(
  multiplexer: &Arc<Mutex<Option<PtyMultiplexer>>>,
  session_id: u32,
  sender: oneshot::Sender<PtyResult>,
) {
  debug!("Handling SplitSession command for session {}", session_id);

  let mut multiplexer_guard = multiplexer.lock();
  if let Some(ref mut multiplexer) = *multiplexer_guard {
    let split_result = multiplexer.split_session(session_id);

    match split_result {
      Ok(_) => {
        let _ = sender.send(PtyResult::Success(format!(
          "Split session {} into sub-sessions",
          session_id
        )));
      }
      Err(e) => {
        error!("Failed to split session: {}", e);
        let _ = sender.send(PtyResult::Failure(format!("SplitSession error: {}", e)));
      }
    }
  } else {
    let _ = sender.send(PtyResult::Failure(
      "Multiplexer not initialized".to_string(),
    ));
  }
}

/// Handles the `SetEnv` command by setting an environment variable in the PTY process.
///
/// This function attempts to set the specified environment variable in the PTY process.
/// It locks the PTY, performs the set operation, and sends back a `PtyResult` indicating
/// success or failure.
///
/// # Parameters
///
/// - `pty`: A reference to an `Arc<Mutex<Option<PtyProcess>>>` representing the shared PTY process.
/// - `key`: A `String` representing the name of the environment variable to set.
/// - `value`: A `String` representing the value to assign to the environment variable.
/// - `sender`: A `oneshot::Sender<PtyResult>` channel used to send back the result of the operation.
///
/// # Examples
///
/// ```rust
/// use tokio::sync::oneshot;
///
/// let pty = Arc::new(Mutex::new(Some(PtyProcess::new())));
/// let key = "PATH".to_string();
/// let value = "/usr/bin".to_string();
/// let (tx, rx) = oneshot::channel();
///
/// handle_set_env(&pty, key, value, tx);
/// ```
fn handle_set_env(
  pty: &Arc<Mutex<Option<PtyProcess>>>,
  key: String,
  value: String,
  sender: oneshot::Sender<PtyResult>,
) {
  debug!("Handling SetEnv command: {}={}", key, value);
  let set_env_result = {
    let mut pty_guard = pty.lock();
    if let Some(ref mut pty_process) = *pty_guard {
      pty_process.set_env(key, value)
    } else {
      Err(io::Error::new(
        io::ErrorKind::BrokenPipe,
        "PTY process not initialized",
      ))
    }
  };

  match set_env_result {
    Ok(_) => {
      info!("Set environment variable successfully");
      let _ = sender.send(PtyResult::Success(
        "Environment variable set successfully".to_string(),
      ));
    }
    Err(e) => {
      error!("Failed to set environment variable: {}", e);
      let _ = sender.send(PtyResult::Failure(format!("SetEnv error: {}", e)));
    }
  }
}

/// Handles the `ChangeShell` command by changing the shell of the PTY process.
///
/// This function attempts to change the shell executable path of the PTY process.
/// It locks the PTY, performs the change operation, and sends back a `PtyResult` indicating
/// success or failure.
///
/// # Parameters
///
/// - `pty`: A reference to an `Arc<Mutex<Option<PtyProcess>>>` representing the shared PTY process.
/// - `shell_path`: A `String` representing the file system path to the new shell executable.
/// - `sender`: A `oneshot::Sender<PtyResult>` channel used to send back the result of the operation.
///
/// # Examples
///
/// ```rust
/// use tokio::sync::oneshot;
///
/// let pty = Arc::new(Mutex::new(Some(PtyProcess::new())));
/// let shell_path = "/bin/zsh".to_string();
/// let (tx, rx) = oneshot::channel();
///
/// handle_change_shell(&pty, shell_path, tx);
/// ```
fn handle_change_shell(
  pty: &Arc<Mutex<Option<PtyProcess>>>,
  shell_path: String,
  sender: oneshot::Sender<PtyResult>,
) {
  debug!("Handling ChangeShell command to {}", shell_path);
  let change_shell_result = {
    let mut pty_guard = pty.lock();
    if let Some(ref mut pty_process) = *pty_guard {
      pty_process.change_shell(shell_path)
    } else {
      Err(io::Error::new(
        io::ErrorKind::BrokenPipe,
        "PTY process not initialized",
      ))
    }
  };

  match change_shell_result {
    Ok(_) => {
      info!("Changed shell successfully");
      let _ = sender.send(PtyResult::Success("Shell changed successfully".to_string()));
    }
    Err(e) => {
      error!("Failed to change shell: {}", e);
      let _ = sender.send(PtyResult::Failure(format!("ChangeShell error: {}", e)));
    }
  }
}

/// Handles the `Status` command by retrieving the current status of the PTY process.
///
/// This function attempts to retrieve the status of the PTY process.
/// It locks the PTY, performs the status retrieval, and sends back a `PtyResult` containing
/// the status or an error.
///
/// # Parameters
///
/// - `pty`: A reference to an `Arc<Mutex<Option<PtyProcess>>>` representing the shared PTY process.
/// - `sender`: A `oneshot::Sender<PtyResult>` channel used to send back the result of the operation.
///
/// # Examples
///
/// ```rust
/// use tokio::sync::oneshot;
///
/// let pty = Arc::new(Mutex::new(Some(PtyProcess::new())));
/// let (tx, rx) = oneshot::channel();
///
/// handle_status(&pty, tx);
/// ```
fn handle_status(pty: &Arc<Mutex<Option<PtyProcess>>>, sender: oneshot::Sender<PtyResult>) {
  debug!("Handling Status command");
  let status_result = {
    let pty_guard = pty.lock();
    if let Some(ref pty_process) = *pty_guard {
      pty_process.status()
    } else {
      Err(io::Error::new(
        io::ErrorKind::BrokenPipe,
        "PTY process not initialized",
      ))
    }
  };

  match status_result {
    Ok(status) => {
      info!("PTY Status: {}", status);
      let _ = sender.send(PtyResult::Success(status));
    }
    Err(e) => {
      error!("Failed to get PTY status: {}", e);
      let _ = sender.send(PtyResult::Failure(format!("Status error: {}", e)));
    }
  }
}

/// Handles the `SetLogLevel` command by setting the logging level of the PTY process.
///
/// This function attempts to set the logging level of the PTY process to the specified level.
/// It locks the PTY, performs the set operation, and sends back a `PtyResult` indicating
/// success or failure.
///
/// # Parameters
///
/// - `pty`: A reference to an `Arc<Mutex<Option<PtyProcess>>>` representing the shared PTY process.
/// - `level`: A `String` representing the desired logging level (e.g., "debug", "info").
/// - `sender`: A `oneshot::Sender<PtyResult>` channel used to send back the result of the operation.
///
/// # Examples
///
/// ```rust
/// use tokio::sync::oneshot;
///
/// let pty = Arc::new(Mutex::new(Some(PtyProcess::new())));
/// let level = "debug".to_string();
/// let (tx, rx) = oneshot::channel();
///
/// handle_set_log_level(&pty, level, tx);
/// ```
fn handle_set_log_level(
  pty: &Arc<Mutex<Option<PtyProcess>>>,
  level: String,
  sender: oneshot::Sender<PtyResult>,
) {
  debug!("Handling SetLogLevel command to {}", level);
  let set_log_level_result = {
    let pty_guard = pty.lock();
    if let Some(ref pty_process) = *pty_guard {
      pty_process.set_log_level(level.clone())
    } else {
      Err(io::Error::new(
        io::ErrorKind::BrokenPipe,
        "PTY process not initialized",
      ))
    }
  };

  match set_log_level_result {
    Ok(_) => {
      info!("Set log level successfully");
      let _ = sender.send(PtyResult::Success("Log level set successfully".to_string()));
    }
    Err(e) => {
      error!("Failed to set log level: {}", e);
      let _ = sender.send(PtyResult::Failure(format!("SetLogLevel error: {}", e)));
    }
  }
}

/// Handles the `ShutdownPty` command by shutting down the PTY process gracefully.
///
/// This function attempts to gracefully shut down the PTY process by sending a `SIGTERM` signal,
/// waiting for the process to terminate, and closing the master file descriptor.
/// It locks the PTY, performs the shutdown operations, and sends back a `PtyResult` indicating
/// success or failure.
///
/// # Parameters
///
/// - `pty`: A reference to an `Arc<Mutex<Option<PtyProcess>>>` representing the shared PTY process.
/// - `sender`: A `oneshot::Sender<PtyResult>` channel used to send back the result of the operation.
///
/// # Examples
///
/// ```rust
/// use tokio::sync::oneshot;
///
/// let pty = Arc::new(Mutex::new(Some(PtyProcess::new())));
/// let (tx, rx) = oneshot::channel();
///
/// handle_shutdown_pty(&pty, tx);
/// ```
fn handle_shutdown_pty(pty: &Arc<Mutex<Option<PtyProcess>>>, sender: oneshot::Sender<PtyResult>) {
  debug!("Handling ShutdownPty command");
  let shutdown_result = {
    let mut pty_option = pty.lock();
    if let Some(pty_process) = pty_option.take() {
      // Attempt to gracefully terminate the PTY process
      if let Err(e) = pty_process.kill_process(libc::SIGTERM) {
        error!("Failed to send SIGTERM to PTY process: {}", e);
      }
      // Wait for the process to terminate
      if let Err(e) = pty_process.waitpid(libc::WNOHANG) {
        error!("Failed to waitpid on PTY process: {}", e);
      }
      // Close the master FD
      pty_process.close_master_fd()
    } else {
      Ok(())
    }
  };

  match shutdown_result {
    Ok(_) => {
      info!("Shutdown PTY successfully");
      let _ = sender.send(PtyResult::Success("PTY shutdown successfully".to_string()));
    }
    Err(e) => {
      error!("Failed to shutdown PTY: {}", e);
      let _ = sender.send(PtyResult::Failure(format!("ShutdownPty error: {}", e)));
    }
  }
}
