// src/pty/commands.rs
use bytes::Bytes;
use tokio::sync::oneshot;

/// Enumeration of possible commands to send to the PTY handler.
#[derive(Debug)]
pub enum PtyCommand {
  /// Write data to the PTY.
  Write(Bytes, oneshot::Sender<PtyResult>),
  /// Read data from the PTY.
  Read(oneshot::Sender<PtyResult>),
  /// Resize the PTY window.
  Resize {
    cols: u16,
    rows: u16,
    sender: oneshot::Sender<PtyResult>,
  },
  /// Execute a command in the PTY.
  Execute(String, oneshot::Sender<PtyResult>),
  /// Close the PTY gracefully.
  Close(oneshot::Sender<PtyResult>),
  /// Forcefully kill the PTY process.
  ForceKill(oneshot::Sender<PtyResult>),
  /// Send a signal to the PTY process.
  KillProcess(i32, oneshot::Sender<PtyResult>),
  /// Wait for the PTY process to change state.
  WaitPid(i32, oneshot::Sender<PtyResult>),
  /// Close the master file descriptor of the PTY.
  CloseMasterFd(oneshot::Sender<PtyResult>),
  /// Broadcast data to all sessions in the multiplexer.
  Broadcast(Bytes, oneshot::Sender<PtyResult>),
  /// Read data from all sessions.
  ReadAll(oneshot::Sender<PtyResult>),
  /// Read data from a specific session.
  ReadFromSession(u32, oneshot::Sender<PtyResult>),
  /// Read data from all sessions.
  ReadAllSessions(oneshot::Sender<PtyResult>),
  /// Merge multiple sessions into one.
  MergeSessions(Vec<u32>, oneshot::Sender<PtyResult>),
  /// Split a session into multiple sub-sessions.
  SplitSession(u32, oneshot::Sender<PtyResult>),
  /// Set an environment variable.
  SetEnv(String, String, oneshot::Sender<PtyResult>),
  /// Change the shell for the PTY process.
  ChangeShell(String, oneshot::Sender<PtyResult>),
  /// Retrieve the status of the PTY process.
  Status(oneshot::Sender<PtyResult>),
  /// Set the log level for the PTY process.
  SetLogLevel(String, oneshot::Sender<PtyResult>),
  /// Shut down the PTY process gracefully.
  ShutdownPty(oneshot::Sender<PtyResult>),
}

/// Enumeration of possible results from PTY operations.
#[derive(Debug)]
pub enum PtyResult {
  /// Operation was successful, possibly with a message.
  Success(String),
  /// Data returned from a read operation.
  Data(Bytes),
  /// Operation failed, with an error message.
  Failure(String),
}
