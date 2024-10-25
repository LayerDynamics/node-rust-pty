// src/pty/commands.rs

use bytes::Bytes;
use serde::{Deserialize, Serialize};
use serde_bytes;
use tokio::sync::oneshot;

/// Wrapper for `Bytes` to implement `Serialize` and `Deserialize`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SerializableBytes(#[serde(with = "serde_bytes")] Vec<u8>);

impl SerializableBytes {
  /// Public method to access the inner byte slice.
  pub fn as_slice(&self) -> &[u8] {
    &self.0
  }
}

impl From<Bytes> for SerializableBytes {
  fn from(bytes: Bytes) -> Self {
    SerializableBytes(bytes.to_vec())
  }
}

impl From<SerializableBytes> for Bytes {
  fn from(serializable_bytes: SerializableBytes) -> Self {
    Bytes::from(serializable_bytes.0)
  }
}

/// Implement `AsRef<[u8]>` for `SerializableBytes` to allow conversion to `&[u8]`.
impl AsRef<[u8]> for SerializableBytes {
  fn as_ref(&self) -> &[u8] {
    self.as_slice()
  }
}

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
  /// List all sessions.
  ListSessions(oneshot::Sender<PtyResult>),
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
  /// Create a new session.
  CreateSession(oneshot::Sender<PtyResult>),
  /// Remove a session by ID.
  RemoveSession(u32, oneshot::Sender<PtyResult>),
  /// Close all active sessions.
  CloseAllSessions(oneshot::Sender<PtyResult>),
  /// Send data to a specific session.
  SendToSession(u32, Bytes, oneshot::Sender<PtyResult>),
}

/// Enumeration of possible results from PTY operations.
#[derive(Debug, Serialize, Deserialize)]
pub enum PtyResult {
  /// Operation was successful, possibly with a message.
  Success(String),
  /// Data returned from a read operation.
  Data(SerializableBytes),
  /// Operation failed, with an error message.
  Failure(String),
}

impl PtyResult {
  /// Convert `PtyResult` to JSON string representation.
  pub fn to_json(&self) -> Result<String, serde_json::Error> {
    serde_json::to_string(self)
  }

  /// Create a `PtyResult` from a JSON string representation.
  pub fn from_json(json_str: &str) -> Result<Self, serde_json::Error> {
    serde_json::from_str(json_str)
  }
}
