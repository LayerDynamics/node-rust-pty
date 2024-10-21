// src/worker/mod.rs

/// The `worker` module contains the `PtyWorker` struct responsible for handling socket connections
/// and managing PTY interactions.
///
/// It facilitates communication between the PTY process and worker sockets, enabling bi-directional
/// data flow and session management.
pub mod pty_worker;

pub use pty_worker::PtyWorker;
