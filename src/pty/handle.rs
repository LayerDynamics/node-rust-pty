// src/pty/handle.rs

use crate::pty::command_handler::handle_commands;
use crate::pty::commands::{PtyCommand, PtyResult};
use crate::pty::multiplexer::{PtyMultiplexer, PtySession};
use crate::pty::multiplexer_handle::MultiplexerHandle;
use crate::pty::platform::PtyProcess;
use crate::utils::logging::initialize_logging;
use bytes::Bytes;
use crossbeam_channel::{bounded, Receiver, Sender};
use log::{error, info, warn};
use napi::bindgen_prelude::*;
use napi_derive::napi;
use parking_lot::Mutex;
use std::collections::HashMap;
use std::io;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::oneshot;
use tokio::task::spawn_blocking;
use tokio::time::timeout;

/// Helper function to convert any error to `napi::Error`
fn map_to_napi_error<E: std::fmt::Display>(e: E) -> napi::Error {
  napi::Error::from_reason(e.to_string())
}

#[napi]
#[derive(Clone)]
pub struct PtyHandle {
  pty: Arc<Mutex<Option<PtyProcess>>>,
  multiplexer: Arc<Mutex<Option<PtyMultiplexer>>>,
  command_sender: Sender<PtyCommand>,
}

#[napi]
impl PtyHandle {
  /// Creates a new `PtyHandle` with logging enabled.
  #[napi(factory)]
  pub async fn new() -> Result<Self> {
    initialize_logging();
    info!("Creating new PtyHandle");

    // Create bounded channels for commands and results
    let (command_sender, command_receiver) = bounded::<PtyCommand>(100);
    let (result_sender, _result_receiver) = bounded::<PtyResult>(100);

    // Initialize the PTY process
    let pty_process = PtyProcess::new()
      .map_err(|e| map_to_napi_error(format!("Failed to initialize PTY process: {}", e)))?;
    let pty = Arc::new(Mutex::new(Some(pty_process)));

    // Initialize the multiplexer
    let pty_process = pty
      .lock()
      .take()
      .ok_or_else(|| map_to_napi_error("Failed to initialize multiplexer: PTY process is None"))?;
    let mut multiplexer = PtyMultiplexer::new(pty_process);
    let multiplexer_arc = Arc::new(Mutex::new(Some(multiplexer)));

    // Instantiate PtyHandle
    let handle = PtyHandle {
      pty: pty.clone(),
      multiplexer: multiplexer_arc.clone(),
      command_sender,
    };

    // Start the command handler
    handle.handle_commands(command_receiver, result_sender);

    Ok(handle)
  }

  /// Returns the underlying `PtyProcess` if available.
  pub fn get_pty_process(&self) -> Option<PtyProcess> {
    let pty_guard = self.pty.lock();
    if let Some(ref pty_process) = *pty_guard {
      Some(pty_process.clone())
    } else {
      None
    }
  }

  /// Returns the `MultiplexerHandle` if available.
  pub fn get_multiplexer_handle(&self) -> Option<MultiplexerHandle> {
    let multiplexer_guard = self.multiplexer.lock();
    if let Some(ref multiplexer) = *multiplexer_guard {
      Some(MultiplexerHandle {
        multiplexer: Arc::new(std::sync::Mutex::new((*multiplexer).clone())),
      })
    } else {
      None
    }
  }

  /// Handles incoming commands by delegating to the command handler.
  fn handle_commands(
    &self,
    command_receiver: Receiver<PtyCommand>,
    result_sender: Sender<PtyResult>,
  ) {
    handle_commands(self.pty.clone(), command_receiver, result_sender);
  }

  /// Asynchronously reads data from the PTY.
  #[napi]
  pub async fn read(&self) -> Result<String> {
    let pty_clone = self.pty.clone();
    info!("Initiating read from PTY.");
    let read_timeout = Duration::from_secs(5);

    // Create a oneshot channel to receive the read result
    let (sender, receiver) = oneshot::channel();

    // Send a read command (assuming PtyCommand::Read exists)
    self
      .command_sender
      .send(PtyCommand::Execute("read".to_string(), sender))
      .map_err(|e| {
        error!("Failed to send read command: {}", e);
        map_to_napi_error(format!("Failed to send read command: {}", e))
      })?;

    // Await the result with a timeout
    match timeout(read_timeout, receiver).await {
      Ok(Ok(PtyResult::Success(data))) => {
        info!("Read operation successful: {}", data);
        Ok(data)
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

  /// Asynchronously writes data to the PTY.
  #[napi]
  pub async fn write(&self, data: String) -> Result<()> {
    let data_bytes = Bytes::from(data.into_bytes());

    info!("Initiating write to PTY.");

    // Create a oneshot channel to receive the write result
    let (sender, receiver) = oneshot::channel();

    // Send the write command along with the responder
    self
      .command_sender
      .send(PtyCommand::Write(data_bytes, sender))
      .map_err(|e| {
        error!("Failed to send write command: {}", e);
        map_to_napi_error(format!("Failed to send write command: {}", e))
      })?;

    // Await the result with a timeout
    match timeout(Duration::from_secs(5), receiver).await {
      Ok(Ok(PtyResult::Success(msg))) => {
        info!("Write operation successful: {}", msg);
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
    }
  }

  /// Asynchronously resizes the PTY window.
  #[napi]
  pub async fn resize(&self, cols: u16, rows: u16) -> Result<()> {
    info!("Initiating resize of PTY to cols: {}, rows: {}", cols, rows);

    // Create a oneshot channel to receive the resize result
    let (sender, receiver) = oneshot::channel();

    // Send the resize command along with the responder
    self
      .command_sender
      .send(PtyCommand::Resize { cols, rows, sender })
      .map_err(|e| {
        error!("Failed to send resize command: {}", e);
        map_to_napi_error(format!("Failed to send resize command: {}", e))
      })?;

    // Await the result with a timeout
    match timeout(Duration::from_secs(5), receiver).await {
      Ok(Ok(PtyResult::Success(msg))) => {
        info!("Resize operation successful: {}", msg);
        Ok(())
      }
      Ok(Ok(PtyResult::Failure(msg))) => {
        error!("Resize operation failed: {}", msg);
        Err(Error::new(Status::Unknown, msg))
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
        Err(Error::new(Status::Unknown, "Resize operation timed out"))
      }
    }
  }

  /// Asynchronously executes a command in the PTY.
  #[napi]
  pub async fn execute(&self, command: String) -> Result<String> {
    info!("Executing command in PTY: {}", command);

    // Create a oneshot channel to receive the execute result
    let (sender, receiver) = oneshot::channel();

    // Send the execute command along with the responder
    self
      .command_sender
      .send(PtyCommand::Execute(command.clone(), sender))
      .map_err(|e| {
        error!("Failed to send execute command: {}", e);
        map_to_napi_error(format!("Failed to send execute command: {}", e))
      })?;

    // Await the result with a timeout
    match timeout(Duration::from_secs(5), receiver).await {
      Ok(Ok(PtyResult::Success(msg))) => {
        info!("Execute operation successful: {}", msg);
        Ok(msg)
      }
      Ok(Ok(PtyResult::Failure(msg))) => {
        error!("Execute operation failed: {}", msg);
        Err(Error::new(Status::Unknown, msg))
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
  #[napi]
  pub async fn close(&self) -> Result<()> {
    let pty_clone = self.pty.clone();
    let force_timeout = Duration::from_secs(5);
    info!("Initiating graceful shutdown of PTY.");
    let graceful_timeout = Duration::from_secs(10);

    let close_result = match timeout(
      graceful_timeout,
      spawn_blocking(move || -> std::result::Result<(), napi::Error> {
        let mut pty_option = pty_clone.lock();

        if let Some(pty) = pty_option.take() {
          info!("Sending SIGTERM to child process {}.", pty.pid);
          if let Err(e) = pty.kill_process(libc::SIGTERM) {
            let err = format!("Failed to send SIGTERM to process {}: {}", pty.pid, e);
            error!("{}", err);
            // Continue to close master_fd even if SIGTERM fails
          } else {
            info!("SIGTERM sent successfully.");
            match pty.waitpid(libc::WNOHANG) {
              Ok(pid) => {
                if pid == 0 {
                  error!(
                    "Process {} did not terminate immediately after SIGTERM.",
                    pty.pid
                  );
                } else {
                  info!("Process {} terminated gracefully.", pid);
                }
              }
              Err(e) => {
                error!("Error waiting for process {}: {}", pty.pid, e);
              }
            }
          }

          if let Err(e) = pty.close_master_fd() {
            let err = format!("Failed to close master_fd {}: {}", pty.master_fd, e);
            error!("{}", err);
          } else {
            info!("Closed master_fd {}.", pty.master_fd);
          }

          Ok(())
        } else {
          warn!("PTY not initialized or already closed.");
          Ok(())
        }
      }),
    )
    .await
    {
      Ok(task_result) => match task_result {
        Ok(inner_result) => inner_result,
        Err(join_error) => Err(map_to_napi_error(format!("Join error: {}", join_error))),
      },
      Err(_) => Err(map_to_napi_error("Graceful shutdown operation timed out")),
    };

    if let Err(e) = close_result {
      error!("Error during graceful shutdown: {}", e);
      // Attempt to force kill
      return self.force_kill(force_timeout).await;
    }

    info!("PTY closed successfully.");
    Ok(())
  }

  /// Forcefully kills the PTY process if graceful shutdown fails.
  async fn force_kill(&self, force_timeout: Duration) -> Result<()> {
    let pty_clone = self.pty.clone();

    let force_kill_result = match timeout(
      force_timeout,
      spawn_blocking(move || -> std::result::Result<(), napi::Error> {
        let mut pty_option = pty_clone.lock();

        if let Some(pty) = pty_option.take() {
          info!("Sending SIGKILL to child process {}.", pty.pid);
          if let Err(e) = pty.kill_process(libc::SIGKILL) {
            let err = format!("Failed to send SIGKILL to process {}: {}", pty.pid, e);
            error!("{}", err);
            return Err(map_to_napi_error(err));
          } else {
            info!("SIGKILL sent successfully to process {}.", pty.pid);
            pty.waitpid(0).map(|_| ()).map_err(|e| {
              let err = format!("Failed to wait for process {}: {}", pty.pid, e);
              map_to_napi_error(err)
            })?;
            pty.close_master_fd().map_err(|e| {
              let err = format!("Failed to close master_fd {}: {}", pty.master_fd, e);
              map_to_napi_error(err)
            })?;
            info!("Closed master_fd {}.", pty.master_fd);
            Ok(())
          }
        } else {
          warn!("PTY not initialized or already closed.");
          Ok(())
        }
      }),
    )
    .await
    {
      Ok(task_result) => match task_result {
        Ok(inner_result) => inner_result,
        Err(join_error) => Err(map_to_napi_error(format!("Join error: {}", join_error))),
      },
      Err(_) => Err(map_to_napi_error("Force kill operation timed out")),
    };

    force_kill_result?;

    info!("PTY forcefully terminated.");
    Ok(())
  }

  /// Returns the process ID (PID) of the PTY process.
  #[napi]
  pub fn pid(&self) -> Option<i32> {
    let pty_guard = self.pty.lock();
    if let Some(ref pty_process) = *pty_guard {
      Some(pty_process.pid)
    } else {
      None
    }
  }

  /// Sends a signal to the PTY process.
  #[napi]
  pub async fn kill_process(&self, signal: i32) -> Result<()> {
    let pty_clone = self.pty.clone();
    info!("Initiating kill process with signal {}.", signal);

    let kill_result = match timeout(
      Duration::from_secs(5),
      spawn_blocking(move || -> std::result::Result<(), napi::Error> {
        let mut pty_option = pty_clone.lock();

        if let Some(pty) = pty_option.as_mut() {
          info!("Sending signal {} to process {}.", signal, pty.pid);
          pty.kill_process(signal).map_err(|e| {
            let err = format!(
              "Failed to send signal {} to process {}: {}",
              signal, pty.pid, e
            );
            map_to_napi_error(err)
          })?;
          Ok(())
        } else {
          Err(map_to_napi_error("PTY process not initialized"))
        }
      }),
    )
    .await
    {
      Ok(task_result) => match task_result {
        Ok(inner_result) => inner_result,
        Err(join_error) => Err(map_to_napi_error(format!("Join error: {}", join_error))),
      },
      Err(_) => Err(map_to_napi_error("Kill process operation timed out")),
    };

    kill_result?;

    Ok(())
  }

  /// Waits for the PTY process to change state.
  #[napi]
  pub async fn waitpid(&self, options: i32) -> Result<i32> {
    let pty_clone = self.pty.clone();
    info!("Initiating waitpid with options {}.", options);

    let wait_result = match timeout(
      Duration::from_secs(5),
      spawn_blocking(move || -> std::result::Result<i32, napi::Error> {
        let pty_guard = pty_clone.lock();
        if let Some(ref pty_process) = *pty_guard {
          pty_process.waitpid(options).map_err(|e| {
            let err = format!("Failed to waitpid: {}", e);
            map_to_napi_error(err)
          })
        } else {
          Err(map_to_napi_error("PTY process not initialized"))
        }
      }),
    )
    .await
    {
      Ok(task_result) => match task_result {
        Ok(inner_result) => inner_result,
        Err(join_error) => Err(map_to_napi_error(format!("Join error: {}", join_error))),
      },
      Err(_) => Err(map_to_napi_error("Waitpid operation timed out")),
    };

    let pid = wait_result?;

    Ok(pid)
  }

  /// Closes the master file descriptor of the PTY process.
  #[napi]
  pub async fn close_master_fd(&self) -> Result<()> {
    let pty_clone = self.pty.clone();
    info!("Initiating close_master_fd.");

    let close_result = match timeout(
      Duration::from_secs(5),
      spawn_blocking(move || -> std::result::Result<(), napi::Error> {
        let pty_guard = pty_clone.lock();
        if let Some(ref pty_process) = *pty_guard {
          match pty_process.close_master_fd() {
            Ok(_) => Ok(()),
            Err(e)
              if e.kind() == io::ErrorKind::InvalidInput || e.kind() == io::ErrorKind::NotFound =>
            {
              log::warn!("Master FD already closed or invalid.");
              Ok(())
            }
            Err(e) => Err(map_to_napi_error(format!(
              "Failed to close master_fd: {}",
              e
            ))),
          }
        } else {
          Err(map_to_napi_error("PTY process not initialized"))
        }
      }),
    )
    .await
    {
      Ok(task_result) => match task_result {
        Ok(inner_result) => inner_result,
        Err(join_error) => Err(map_to_napi_error(format!("Join error: {}", join_error))),
      },
      Err(_) => Err(map_to_napi_error("Close master FD operation timed out")),
    };

    close_result?;

    Ok(())
  }

  /// Retrieves the underlying `MultiplexerHandle` for managing sessions.
  #[napi(getter)]
  pub fn multiplexer_handle(&self) -> Option<MultiplexerHandle> {
    self.get_multiplexer_handle()
  }
}

impl Drop for PtyHandle {
  fn drop(&mut self) {
    initialize_logging();
    let pty_clone = self.pty.clone();
    let multiplexer_clone = self.multiplexer.clone();
    info!("Dropping PtyHandle, initiating cleanup.");

    tokio::task::block_in_place(|| {
      let mut pty_option = pty_clone.lock();

      if let Some(pty) = pty_option.take() {
        info!("Dropping: Sending SIGTERM to child process {}.", pty.pid);
        if let Err(e) = pty.kill_process(libc::SIGTERM) {
          let err = format!(
            "Dropping: Failed to send SIGTERM to process {}: {}",
            pty.pid, e
          );
          error!("{}", err);
        } else {
          std::thread::sleep(Duration::from_secs(2));
          match pty.waitpid(0) {
            Ok(pid) => {
              if pid == 0 {
                error!(
                  "Dropping: Process {} did not terminate gracefully.",
                  pty.pid
                );
              } else {
                info!("Dropping: Process {} terminated gracefully.", pid);
              }
            }
            Err(e) => {
              let err = format!(
                "Dropping: Process {} did not terminate gracefully: {}",
                pty.pid, e
              );
              error!("{}", err);
            }
          }
        }

        info!(
          "Dropping: Sending SIGKILL to child process {} as a fallback.",
          pty.pid
        );
        if let Err(e) = pty.kill_process(libc::SIGKILL) {
          let err = format!(
            "Dropping: Failed to send SIGKILL to process {}: {}",
            pty.pid, e
          );
          error!("{}", err);
        } else {
          info!("Dropping: SIGKILL sent to process {}.", pty.pid);
          let _ = pty.waitpid(0);
        }

        if pty.master_fd >= 0 {
          if let Err(e) = pty.close_master_fd() {
            let err = format!(
              "Dropping: Failed to close master_fd {}: {}",
              pty.master_fd, e
            );
            error!("{}", err);
          } else {
            info!("Dropping: Master_fd {} closed successfully.", pty.master_fd);
          }
        }

        info!("Dropping: PTY session closed via Drop.");
      } else {
        warn!("Dropping: PTY not initialized or already closed.");
      }

      // Additionally, close all sessions in the multiplexer
      let mut multiplexer_option = multiplexer_clone.lock();
      if let Some(ref multiplexer) = *multiplexer_option {
        if let Err(e) = multiplexer.close_all_sessions() {
          error!("Dropping: Failed to close all sessions: {}", e);
        } else {
          info!("Dropping: All sessions closed successfully.");
        }
      }
    });
  }
}
