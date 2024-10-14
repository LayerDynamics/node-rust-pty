use crate::pty::platform::PtyProcess;
use bytes::Bytes;
use libc::{SIGKILL, SIGTERM, WNOHANG};
use log::info;
use napi::bindgen_prelude::*;
use napi_derive::napi;
use std::collections::HashMap;
use std::io;
use std::sync::{Arc, Mutex};

/// Helper function to convert io::Error to napi::Error
fn convert_io_error(err: io::Error) -> napi::Error {
  napi::Error::new(napi::Status::GenericFailure, format!("{}", err))
}

/// Represents a session within the multiplexer
#[derive(Debug, Clone)]
pub struct PtySession {
  pub stream_id: u32,
  pub input_buffer: Vec<u8>,
  pub output_buffer: Arc<Mutex<Vec<u8>>>, // Changed to Arc<Mutex<>> for async access
}

/// Multiplexer to handle multiple virtual streams on the same PTY
#[derive(Debug, Clone)] // Add Clone here
pub struct PtyMultiplexer {
  pub pty_process: PtyProcess,
  pub sessions: Arc<Mutex<HashMap<u32, PtySession>>>, // Holds multiple sessions
}

// Implement FromNapiValue for PtyProcess
impl FromNapiValue for PtyProcess {
  unsafe fn from_napi_value(
    env: *mut napi::sys::napi_env__,
    napi_val: napi::sys::napi_value,
  ) -> napi::Result<Self> {
    // Convert from NAPI value to a valid PtyProcess
    let id: String = FromNapiValue::from_napi_value(env, napi_val)?;
    Ok(PtyProcess::new()?)
  }
}

// Implement ToNapiValue for PtyProcess
impl std::fmt::Display for PtyProcess {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    write!(f, "PtyProcess")
  }
}

impl ToNapiValue for PtyProcess {
  unsafe fn to_napi_value(
    env: *mut napi::sys::napi_env__,
    value: Self,
  ) -> napi::Result<napi::sys::napi_value> {
    let mut obj = std::ptr::null_mut();
    napi::sys::napi_create_object(env, &mut obj);
    Ok(obj)
  }
}

// Implement ToNapiValue for PtyMultiplexer
impl ToNapiValue for PtyMultiplexer {
  unsafe fn to_napi_value(
    env: *mut napi::sys::napi_env__,
    value: Self,
  ) -> napi::Result<napi::sys::napi_value> {
    let mut obj = std::ptr::null_mut();
    napi::sys::napi_create_object(env, &mut obj);
    let pty_process_value = PtyProcess::to_napi_value(env, value.pty_process)?;
    napi::sys::napi_set_named_property(
      env,
      obj,
      "pty_process\0".as_ptr() as *const i8,
      pty_process_value,
    );
    Ok(obj)
  }
}

// Implement TypeName for PtyMultiplexer
impl TypeName for PtyMultiplexer {
  fn type_name() -> &'static str {
    "PtyMultiplexer"
  }

  fn value_type() -> napi::ValueType {
    napi::ValueType::Object
  }
}

#[napi]
impl PtyMultiplexer {
  #[napi]
  pub fn new(pty_process: PtyProcess) -> Self {
    Self {
      pty_process,
      sessions: Arc::new(Mutex::new(HashMap::new())),
    }
  }

  #[napi]
  pub fn create_session(&mut self) -> u32 {
    let mut sessions = self.sessions.lock().unwrap();
    let stream_id = sessions.len() as u32 + 1;
    sessions.insert(
      stream_id,
      PtySession {
        stream_id,
        input_buffer: Vec::new(),
        output_buffer: Arc::new(Mutex::new(Vec::new())),
      },
    );
    stream_id
  }

  #[napi]
  pub fn send_to_session(&mut self, session_id: u32, data: &[u8]) -> napi::Result<()> {
    let mut sessions = self.sessions.lock().unwrap();
    if let Some(session) = sessions.get_mut(&session_id) {
      session.input_buffer.extend_from_slice(data);
      self
        .pty_process
        .write_data(&Bytes::copy_from_slice(data))
        .map_err(convert_io_error)?; // Convert io::Error to napi::Error
    } else {
      return Err(Error::new(
        napi::Status::InvalidArg,
        format!("Session ID {} not found", session_id),
      ));
    }
    Ok(())
  }

  #[napi]
  pub fn broadcast(&mut self, data: &[u8]) -> napi::Result<()> {
    let mut sessions = self.sessions.lock().unwrap();
    for session in sessions.values_mut() {
      session.input_buffer.extend_from_slice(data);
    }
    self
      .pty_process
      .write_data(&Bytes::copy_from_slice(data))
      .map_err(convert_io_error)?;
    Ok(())
  }

  #[napi]
  pub fn read_from_session(&self, session_id: u32) -> napi::Result<String> {
    let sessions = self.sessions.lock().unwrap();
    if let Some(session) = sessions.get(&session_id) {
      let output = session.output_buffer.lock().unwrap();
      let data = String::from_utf8_lossy(&output).to_string();
      Ok(data)
    } else {
      Err(Error::new(
        napi::Status::InvalidArg,
        format!("Session ID {} not found", session_id),
      ))
    }
  }

  #[napi]
  pub fn read_all(&self) -> napi::Result<()> {
    let mut buffer = [0u8; 4096];
    loop {
      match self.pty_process.read_data(&mut buffer) {
        Ok(0) => break,
        Ok(n) => {
          let sessions = self.sessions.lock().unwrap();
          for session in sessions.values() {
            let mut output = session.output_buffer.lock().unwrap();
            output.extend_from_slice(&buffer[..n]);
          }
        }
        Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => break,
        Err(e) => return Err(convert_io_error(e)),
      }
    }
    Ok(())
  }

  #[napi]
  pub fn read_all_sessions(&self) -> napi::Result<HashMap<String, Vec<u8>>> {
    let sessions = self.sessions.lock().unwrap();
    let mut result = HashMap::new();
    for (id, session) in sessions.iter() {
      let output = session.output_buffer.lock().unwrap().clone();
      result.insert(id.to_string(), output); // Convert u32 to String for compatibility
    }
    Ok(result)
  }

  #[napi]
  pub fn set_env(&mut self, _key: String, _value: String) -> napi::Result<()> {
    Err(Error::new(
      napi::Status::GenericFailure,
      "Setting environment variables at runtime is not supported",
    ))
  }

  #[napi]
  pub fn change_shell(&mut self, shell_path: String) -> napi::Result<()> {
    let command = format!("exec {}\n", shell_path);
    self
      .pty_process
      .write_data(&Bytes::from(command.into_bytes()))
      .map_err(convert_io_error)?;
    Ok(())
  }

  #[napi]
  pub fn status(&self) -> napi::Result<String> {
    match self.pty_process.waitpid(WNOHANG) {
      Ok(0) => Ok("Running".to_string()),
      Ok(pid) => Ok(format!("Terminated with PID {}", pid)),
      Err(e) => Err(convert_io_error(e)),
    }
  }

  #[napi]
  pub fn set_log_level(&self, level: String) -> napi::Result<()> {
    crate::utils::logging::initialize_logging();
    match level.to_lowercase().as_str() {
      "error" => log::set_max_level(log::LevelFilter::Error),
      "warn" => log::set_max_level(log::LevelFilter::Warn),
      "info" => log::set_max_level(log::LevelFilter::Info),
      "debug" => log::set_max_level(log::LevelFilter::Debug),
      "trace" => log::set_max_level(log::LevelFilter::Trace),
      _ => return Err(Error::new(napi::Status::InvalidArg, "Invalid log level")),
    };
    Ok(())
  }

  #[napi]
  pub fn merge_sessions(&self, session_ids: Vec<u32>) -> napi::Result<()> {
    if session_ids.is_empty() {
      return Err(Error::new(
        napi::Status::InvalidArg,
        "No session IDs provided for merging",
      ));
    }
    info!("Merging sessions: {:?}", session_ids);
    Ok(())
  }

  #[napi]
  pub fn split_session(&self, session_id: u32) -> napi::Result<()> {
    info!("Splitting session: {}", session_id);
    Ok(())
  }

  #[napi]
  pub fn remove_session(&self, session_id: u32) -> napi::Result<()> {
    let mut sessions = self.sessions.lock().unwrap();
    if sessions.remove(&session_id).is_some() {
      Ok(())
    } else {
      Err(Error::new(
        napi::Status::InvalidArg,
        format!("Session ID {} not found", session_id),
      ))
    }
  }

  #[napi]
  pub fn close_all_sessions(&self) -> napi::Result<()> {
    let mut sessions = self.sessions.lock().unwrap();
    sessions.clear();
    Ok(())
  }

  #[napi]
  pub fn shutdown_pty(&mut self) -> napi::Result<()> {
    self
      .pty_process
      .kill_process(SIGTERM)
      .map_err(convert_io_error)?;
    self
      .pty_process
      .waitpid(WNOHANG)
      .map_err(convert_io_error)?;
    self.close_all_sessions()?;
    Ok(())
  }

  #[napi]
  pub fn force_shutdown_pty(&mut self) -> napi::Result<()> {
    self
      .pty_process
      .kill_process(SIGKILL)
      .map_err(convert_io_error)?;
    self
      .pty_process
      .waitpid(WNOHANG)
      .map_err(convert_io_error)?;
    self.close_all_sessions()?;
    Ok(())
  }
}
