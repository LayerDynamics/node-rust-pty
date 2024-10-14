// src/pty/commands.rs

use bytes::Bytes;
use tokio::sync::oneshot; // Ensure that the oneshot module is imported

/// Enumeration of possible commands to send to the PTY handler.
#[derive(Debug)]
pub enum PtyCommand {
    Write(Bytes, oneshot::Sender<PtyResult>),
    Resize {
        cols: u16,
        rows: u16,
        sender: oneshot::Sender<PtyResult>,
    },
    Execute(String, oneshot::Sender<PtyResult>),
    Close(oneshot::Sender<PtyResult>),
    ForceKill(oneshot::Sender<PtyResult>),
    Broadcast(Bytes, oneshot::Sender<PtyResult>),
    ReadAll(oneshot::Sender<PtyResult>),
    ReadFromSession(u32, oneshot::Sender<PtyResult>),
    ReadAllSessions(oneshot::Sender<PtyResult>),
    MergeSessions(Vec<u32>, oneshot::Sender<PtyResult>),
    SplitSession(u32, oneshot::Sender<PtyResult>),
    SetEnv(String, String, oneshot::Sender<PtyResult>),
    ChangeShell(String, oneshot::Sender<PtyResult>),
    Status(oneshot::Sender<PtyResult>),
    SetLogLevel(String, oneshot::Sender<PtyResult>),
    ShutdownPty(oneshot::Sender<PtyResult>),
}

/// Enumeration of possible results from PTY operations.
#[derive(Debug)]
pub enum PtyResult {
    Success(String),
    Failure(String),
}
