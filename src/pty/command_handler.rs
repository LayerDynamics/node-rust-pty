// src/pty/command_handler.rs

use crate::pty::commands::{PtyCommand, PtyResult};
use crate::pty::platform::PtyProcess;
use bytes::Bytes;
use crossbeam_channel::Receiver;
use log::{debug, error, info};
use parking_lot::Mutex;
use std::io;
use std::sync::Arc;
use tokio::sync::oneshot;

/// Handles incoming PTY commands by processing them and sending back results.
pub fn handle_commands(
    pty: Arc<Mutex<Option<PtyProcess>>>,
    receiver: Receiver<PtyCommand>,
    result_sender: crossbeam_channel::Sender<PtyResult>, // Unused in current implementation
) {
    tokio::spawn(async move {
        info!("Command handler started");

        loop {
            match receiver.recv() {
                Ok(command) => {
                    debug!("Received command: {:?}", command);
                    match command {
                        PtyCommand::Write(data, responder) => {
                            handle_write(&pty, &data, responder).await;
                        }
                        PtyCommand::Resize { cols, rows, sender } => {
                            handle_resize(&pty, cols, rows, sender).await;
                        }
                        PtyCommand::Execute(cmd, responder) => {
                            handle_execute(&pty, cmd, responder).await;
                        }
                        PtyCommand::Close(sender) => {
                            handle_close(&pty, sender).await;
                        }
                        PtyCommand::ForceKill(sender) => {
                            handle_force_kill(&pty, sender).await;
                        }
                        PtyCommand::Broadcast(data, responder) => {
                            handle_broadcast(&pty, &data, responder).await;
                        }
                        PtyCommand::ReadAll(sender) => {
                            handle_read_all(&pty, sender).await;
                        }
                        PtyCommand::ReadFromSession(session_id, sender) => {
                            handle_read_from_session(&pty, session_id, sender).await;
                        }
                        PtyCommand::ReadAllSessions(sender) => {
                            handle_read_all_sessions(&pty, sender).await;
                        }
                        PtyCommand::MergeSessions(session_ids, sender) => {
                            handle_merge_sessions(&pty, session_ids, sender).await;
                        }
                        PtyCommand::SplitSession(session_id, sender) => {
                            handle_split_session(&pty, session_id, sender).await;
                        }
                        PtyCommand::SetEnv(key, value, sender) => {
                            handle_set_env(&pty, key, value, sender).await;
                        }
                        PtyCommand::ChangeShell(shell_path, sender) => {
                            handle_change_shell(&pty, shell_path, sender).await;
                        }
                        PtyCommand::Status(sender) => {
                            handle_status(&pty, sender).await;
                        }
                        PtyCommand::SetLogLevel(level, sender) => {
                            handle_set_log_level(&pty, level, sender).await;
                        }
                        PtyCommand::ShutdownPty(sender) => {
                            handle_shutdown_pty(&pty, sender).await;
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

/// Handles the Write command.
async fn handle_write(
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

/// Handles the Resize command.
async fn handle_resize(
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

/// Handles the Execute command.
async fn handle_execute(
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
            info!("Executed command '{}' by writing {} bytes", command, bytes_written);
            let _ = responder.send(PtyResult::Success(format!("Executed command '{}'", command)));
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

/// Handles the Close command.
async fn handle_close(
    pty: &Arc<Mutex<Option<PtyProcess>>>,
    sender: oneshot::Sender<PtyResult>,
) {
    debug!("Handling Close command");
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
            let _ = sender.send(PtyResult::Success("Closed master_fd successfully".to_string()));
        }
        Err(e) => {
            error!("Failed to close master_fd: {}", e);
            let _ = sender.send(PtyResult::Failure(format!("Close error: {}", e)));
        }
    }
}

/// Handles the ForceKill command.
async fn handle_force_kill(
    pty: &Arc<Mutex<Option<PtyProcess>>>,
    sender: oneshot::Sender<PtyResult>,
) {
    debug!("Handling ForceKill command");
    let kill_result = {
        let pty_guard = pty.lock();
        if let Some(ref pty_process) = *pty_guard {
            pty_process.force_kill()
        } else {
            Err(io::Error::new(
                io::ErrorKind::BrokenPipe,
                "PTY process not initialized",
            ))
        }
    };

    match kill_result {
        Ok(_) => {
            info!("Force killed PTY process successfully");
            let _ = sender.send(PtyResult::Success("Force killed PTY process successfully".to_string()));
        }
        Err(e) => {
            error!("Failed to force kill PTY process: {}", e);
            let _ = sender.send(PtyResult::Failure(format!("Force kill error: {}", e)));
        }
    }
}

/// Handles the Broadcast command.
async fn handle_broadcast(
    pty: &Arc<Mutex<Option<PtyProcess>>>,
    data: &Bytes,
    responder: oneshot::Sender<PtyResult>,
) {
    debug!("Handling Broadcast command with {} bytes", data.len());
    let broadcast_result = {
        let pty_guard = pty.lock();
        if let Some(ref pty_process) = *pty_guard {
            pty_process.write_data(&data)
        } else {
            Err(io::Error::new(
                io::ErrorKind::BrokenPipe,
                "PTY process not initialized",
            ))
        }
    };

    match broadcast_result {
        Ok(bytes_written) => {
            info!("Broadcasted {} bytes to PTY", bytes_written);
            let _ = responder.send(PtyResult::Success(format!("Broadcasted {} bytes", bytes_written)));
        }
        Err(e) => {
            error!("Failed to broadcast to PTY: {}", e);
            let _ = responder.send(PtyResult::Failure(format!("Broadcast error: {}", e)));
        }
    }
}

/// Handles the ReadAll command.
async fn handle_read_all(
    pty: &Arc<Mutex<Option<PtyProcess>>>,
    sender: oneshot::Sender<PtyResult>,
) {
    debug!("Handling ReadAll command");
    let read_all_result = {
        let pty_guard = pty.lock();
        if let Some(ref pty_process) = *pty_guard {
            // Placeholder for actual read logic
            // For demonstration, we'll read up to 4096 bytes
            let mut buffer = [0u8; 4096];
            pty_process.read_data(&mut buffer)
        } else {
            Err(io::Error::new(
                io::ErrorKind::BrokenPipe,
                "PTY process not initialized",
            ))
        }
    };

    match read_all_result {
        Ok(bytes_read) => {
            info!("Read {} bytes from PTY", bytes_read);
            let _ = sender.send(PtyResult::Success(format!("Read {} bytes", bytes_read)));
        }
        Err(e) => {
            error!("Failed to read from PTY: {}", e);
            let _ = sender.send(PtyResult::Failure(format!("ReadAll error: {}", e)));
        }
    }
}

/// Handles the ReadFromSession command.
async fn handle_read_from_session(
    pty: &Arc<Mutex<Option<PtyProcess>>>,
    session_id: u32,
    sender: oneshot::Sender<PtyResult>,
) {
    debug!("Handling ReadFromSession command for session {}", session_id);
    // Placeholder: Implement actual session-specific read logic
    let _ = sender.send(PtyResult::Success(format!(
        "Read from session {} not implemented",
        session_id
    )));
}

/// Handles the ReadAllSessions command.
async fn handle_read_all_sessions(
    pty: &Arc<Mutex<Option<PtyProcess>>>,
    sender: oneshot::Sender<PtyResult>,
) {
    debug!("Handling ReadAllSessions command");
    // Placeholder: Implement actual logic to read from all sessions
    let _ = sender.send(PtyResult::Success("Read all sessions not implemented".to_string()));
}

/// Handles the MergeSessions command.
async fn handle_merge_sessions(
    pty: &Arc<Mutex<Option<PtyProcess>>>,
    session_ids: Vec<u32>,
    sender: oneshot::Sender<PtyResult>,
) {
    debug!("Handling MergeSessions command for sessions {:?}", session_ids);
    // Placeholder: Implement actual merge logic
    let _ = sender.send(PtyResult::Success(format!(
        "Merged sessions {:?}",
        session_ids
    )));
}

/// Handles the SplitSession command.
async fn handle_split_session(
    pty: &Arc<Mutex<Option<PtyProcess>>>,
    session_id: u32,
    sender: oneshot::Sender<PtyResult>,
) {
    debug!("Handling SplitSession command for session {}", session_id);
    // Placeholder: Implement actual split logic
    let _ = sender.send(PtyResult::Success(format!(
        "Split session {} into sub-sessions",
        session_id
    )));
}

/// Handles the SetEnv command.
async fn handle_set_env(
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
            let _ = sender.send(PtyResult::Success("Environment variable set successfully".to_string()));
        }
        Err(e) => {
            error!("Failed to set environment variable: {}", e);
            let _ = sender.send(PtyResult::Failure(format!("SetEnv error: {}", e)));
        }
    }
}

/// Handles the ChangeShell command.
async fn handle_change_shell(
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

/// Handles the Status command.
async fn handle_status(
    pty: &Arc<Mutex<Option<PtyProcess>>>,
    sender: oneshot::Sender<PtyResult>,
) {
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

/// Handles the SetLogLevel command.
async fn handle_set_log_level(
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

/// Handles the ShutdownPty command.
async fn handle_shutdown_pty(
    pty: &Arc<Mutex<Option<PtyProcess>>>,
    sender: oneshot::Sender<PtyResult>,
) {
    debug!("Handling ShutdownPty command");
    let shutdown_result = {
        let mut pty_guard = pty.lock();
        if let Some(ref mut pty_process) = *pty_guard {
            pty_process.shutdown_pty()
        } else {
            Err(io::Error::new(
                io::ErrorKind::BrokenPipe,
                "PTY process not initialized",
            ))
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
