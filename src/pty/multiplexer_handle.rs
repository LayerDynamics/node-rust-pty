// src/pty/multiplexer_handle.rs

use crate::pty::multiplexer::PtyMultiplexer;
use crate::pty::platform::PtyProcess;
use napi::bindgen_prelude::*;
use napi_derive::napi;
use serde::Serialize; // If needed
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::io;

/// Struct representing session data, exposed to JavaScript.
#[napi(object)]
#[derive(Debug, Clone)]
pub struct SessionData {
    pub session_id: u32,
    pub data: Vec<u8>,
}

/// Struct exposed to JavaScript for managing multiple PTY sessions.
#[napi]
pub struct MultiplexerHandle {
    pub(crate) multiplexer: Arc<Mutex<PtyMultiplexer>>,
}

#[napi]
impl MultiplexerHandle {
    /// Creates a new `MultiplexerHandle` with a new PTY process.
    #[napi(factory)]
    pub fn new() -> Result<Self> {
        let pty_process = PtyProcess::new().map_err(|e| {
            Error::new(
                Status::GenericFailure,
                format!("Failed to create PtyProcess: {}", e),
            )
        })?;

        let multiplexer = PtyMultiplexer::new(pty_process);
        Ok(MultiplexerHandle {
            multiplexer: Arc::new(Mutex::new(multiplexer)),
        })
    }

    /// Creates a new session within the multiplexer.
    #[napi]
    pub fn create_session(&self) -> Result<u32> {
        let mut multiplexer = self.multiplexer.lock().unwrap();
        let session_id = multiplexer.create_session();
        Ok(session_id)
    }

    /// Sends data to a specific session.
    #[napi]
    pub fn send_to_session(&self, session_id: u32, data: Vec<u8>) -> Result<()> {
        let mut multiplexer = self.multiplexer.lock().unwrap();
        multiplexer.send_to_session(session_id, &data).map_err(|e| {
            Error::new(
                Status::InvalidArg,
                format!("Failed to send data to session {}: {}", session_id, e),
            )
        })
    }

    /// Broadcasts data to all active sessions.
    #[napi]
    pub fn broadcast(&self, data: Vec<u8>) -> Result<()> {
        let mut multiplexer = self.multiplexer.lock().unwrap();
        multiplexer.broadcast(&data).map_err(|e| {
            Error::new(
                Status::InvalidArg,
                format!("Failed to broadcast data: {}", e),
            )
        })
    }

    /// Reads data from a specific session.
    #[napi]
    pub fn read_from_session(&self, session_id: u32) -> Result<Vec<u8>> {
        let multiplexer = self.multiplexer.lock().unwrap();
        let data = multiplexer
            .read_from_session(session_id)
            .map_err(|e| Error::new(Status::InvalidArg, format!("Failed to read from session {}: {}", session_id, e)))?;
        Ok(data.into_bytes())
    }

    /// Reads data from all sessions.
    #[napi]
    pub fn read_all_sessions(&self) -> Result<Vec<SessionData>> {
        let multiplexer = self.multiplexer.lock().unwrap();
        let sessions_map = multiplexer.read_all_sessions().map_err(|e| {
            Error::new(
                Status::InvalidArg,
                format!("Failed to read all sessions: {}", e),
            )
        })?;

        let sessions: Result<Vec<SessionData>> = sessions_map.into_iter().map(|(id, data)| {
            let session_id = id.parse::<u32>().map_err(|e| Error::new(Status::InvalidArg, format!("Failed to parse session id: {}", e)))?;
            Ok(SessionData {
                session_id,
                data,
            })
        }).collect();

        sessions
    }

    /// Removes a specific session.
    #[napi]
    pub fn remove_session(&self, session_id: u32) -> Result<()> {
        let mut multiplexer = self.multiplexer.lock().unwrap();
        multiplexer.remove_session(session_id).map_err(|e| {
            Error::new(
                Status::InvalidArg,
                format!("Failed to remove session {}: {}", session_id, e),
            )
        })
    }

    /// Merges multiple sessions into one.
    #[napi]
    pub fn merge_sessions(&self, session_ids: Vec<u32>) -> Result<()> {
        let mut multiplexer = self.multiplexer.lock().unwrap();
        multiplexer.merge_sessions(session_ids).map_err(|e| {
            Error::new(
                Status::InvalidArg,
                format!("Failed to merge sessions: {}", e),
            )
        })
    }

    /// Splits a session into multiple sub-sessions.
    #[napi]
    pub fn split_session(&self, session_id: u32) -> Result<()> {
        let mut multiplexer = self.multiplexer.lock().unwrap();
        multiplexer.split_session(session_id).map_err(|e| {
            Error::new(
                Status::InvalidArg,
                format!("Failed to split session {}: {}", session_id, e),
            )
        })
    }

    /// Sets an environment variable for the PTY process.
    #[napi]
    pub fn set_env(&self, key: String, value: String) -> Result<()> {
        let mut multiplexer = self.multiplexer.lock().unwrap();
        multiplexer.set_env(key, value).map_err(|e| {
            Error::new(
                Status::InvalidArg,
                format!("Failed to set environment variable: {}", e),
            )
        })
    }

    /// Changes the shell of the PTY process.
    #[napi]
    pub fn change_shell(&self, shell_path: String) -> Result<()> {
        let mut multiplexer = self.multiplexer.lock().unwrap();
        multiplexer.change_shell(shell_path).map_err(|e| {
            Error::new(
                Status::InvalidArg,
                format!("Failed to change shell: {}", e),
            )
        })
    }

    /// Retrieves the status of the PTY process.
    #[napi]
    pub fn status(&self) -> Result<String> {
        let multiplexer = self.multiplexer.lock().unwrap();
        multiplexer.status().map_err(|e| {
            Error::new(
                Status::InvalidArg,
                format!("Failed to get PTY status: {}", e),
            )
        })
    }

    /// Adjusts the logging level.
    #[napi]
    pub fn set_log_level(&self, level: String) -> Result<()> {
        let mut multiplexer = self.multiplexer.lock().unwrap();
        multiplexer.set_log_level(level).map_err(|e| {
            Error::new(
                Status::InvalidArg,
                format!("Failed to set log level: {}", e),
            )
        })
    }

    /// Closes all sessions within the multiplexer.
    #[napi]
    pub fn close_all_sessions(&self) -> Result<()> {
        let mut multiplexer = self.multiplexer.lock().unwrap();
        multiplexer.close_all_sessions().map_err(|e| {
            Error::new(
                Status::InvalidArg,
                format!("Failed to close all sessions: {}", e),
            )
        })
    }

    /// Gracefully shuts down the PTY process and closes all sessions.
    #[napi]
    pub fn shutdown_pty(&self) -> Result<()> {
        let mut multiplexer = self.multiplexer.lock().unwrap();
        multiplexer.shutdown_pty().map_err(|e| {
            Error::new(
                Status::InvalidArg,
                format!("Failed to shutdown PTY: {}", e),
            )
        })
    }
}
