// src/pty/multiplexer.rs

use crate::pty::platform::PtyProcess;
use async_trait::async_trait;
use bytes::Bytes;
use dashmap::DashMap;
use futures::future::join_all;
use libc::{SIGKILL, SIGTERM, WNOHANG};
use log::{error, info};
use napi::bindgen_prelude::*;
use napi::JsObject;
use napi_derive::napi;
use std::collections::HashMap;
use std::io;
use std::sync::Arc;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};
use tokio::sync::{broadcast, Mutex as TokioMutex};
use tokio::time::{sleep, Duration};

#[async_trait]
pub trait PtyProcessInterface: Send + Sync {
  async fn write_data(&self, data: &Bytes) -> io::Result<usize>;
  async fn read_data(&self, buffer: &mut [u8]) -> io::Result<usize>;
  async fn is_running(&self) -> io::Result<bool>;
  async fn kill_process(&self, signal: i32) -> io::Result<()>;
  async fn waitpid(&self, options: i32) -> io::Result<i32>;
  async fn resize(&self, cols: u16, rows: u16) -> io::Result<()>;
  async fn get_pid(&self) -> io::Result<i32>;
  async fn set_env(&self, key: String, value: String) -> io::Result<()>;
}

#[async_trait]
impl PtyProcessInterface for PtyProcess {
  async fn write_data(&self, data: &Bytes) -> io::Result<usize> {
    self.write_data(data)
  }

  async fn read_data(&self, buffer: &mut [u8]) -> io::Result<usize> {
    Ok(self.read_data(buffer)?)
  }

  async fn is_running(&self) -> io::Result<bool> {
    Ok(true)
  }

  async fn kill_process(&self, _signal: i32) -> io::Result<()> {
    Ok(())
  }

  async fn waitpid(&self, _options: i32) -> io::Result<i32> {
    Ok(0)
  }

  async fn resize(&self, _cols: u16, _rows: u16) -> io::Result<()> {
    Ok(())
  }

  async fn get_pid(&self) -> io::Result<i32> {
    Ok(self.pid)
  }

  async fn set_env(&self, key: String, value: String) -> io::Result<()> {
    // Construct the shell command to set the environment variable
    let command = format!("export {}=\"{}\"\n", key, value);

    // Convert String to Bytes
    let bytes_command = Bytes::copy_from_slice(command.as_bytes());

    // Write the command to the PTY process
    self.write_data(&bytes_command).map(|_| ())
  }
}

fn convert_io_error(err: io::Error) -> napi::Error {
  napi::Error::from_reason(err.to_string())
}

#[derive(Clone)]
pub struct PtySession {
  pub stream_id: u32,
  pub input_buffer: Vec<u8>,
  pub output_buffer: Arc<TokioMutex<Vec<u8>>>,
}

#[napi(object)]
pub struct SessionData {
  pub session_id: u32,
  pub data: Buffer,
}

#[napi]
#[derive(Clone)]
pub struct PtyMultiplexer {
  #[napi(skip)]
  pub pty_process: Arc<dyn PtyProcessInterface>,
  #[napi(skip)]
  pub sessions: Arc<DashMap<u32, PtySession>>,
  #[napi(skip)]
  pub shutdown_sender: broadcast::Sender<()>,
}

#[napi]
impl PtyMultiplexer {
  #[napi(constructor)]
  pub fn new(env: Env, pty_process: JsObject) -> Result<Self> {
    let pty_process: PtyProcess = PtyProcess::from_js_object(&pty_process)?;
    let (shutdown_sender, _) = broadcast::channel(1);
    let multiplexer = Self {
      pty_process: Arc::new(pty_process),
      sessions: Arc::new(DashMap::new()),
      shutdown_sender: shutdown_sender.clone(),
    };
    let multiplexer_clone = multiplexer.clone();
    tokio::spawn(async move {
      let mut shutdown_receiver = multiplexer_clone.shutdown_sender.subscribe();
      let mut retry_delay = Duration::from_secs(1);
      let max_retry_delay = Duration::from_secs(32);
      loop {
        tokio::select! {
            res = multiplexer_clone.read_and_dispatch() => {
                match res {
                    Ok(_) => {
                        retry_delay = Duration::from_secs(1);
                    },
                    Err(e) => {
                        error!("Error in background PTY read task: {}", e);
                        sleep(retry_delay).await;
                        retry_delay = std::cmp::min(retry_delay * 2, max_retry_delay);
                        continue;
                    },
                }
            },
            _ = shutdown_receiver.recv() => {
                info!("Shutdown signal received. Terminating background PTY read task.");
                break;
            },
        }
      }
      info!("Background PTY read task terminated.");
    });
    Ok(multiplexer)
  }

  #[napi]
  pub async fn create_session(&self) -> Result<u32> {
    let stream_id = if let Some(max_id) = self.sessions.iter().map(|entry| *entry.key()).max() {
      max_id + 1
    } else {
      1
    };
    self.sessions.insert(
      stream_id,
      PtySession {
        stream_id,
        input_buffer: Vec::new(),
        output_buffer: Arc::new(TokioMutex::new(Vec::new())),
      },
    );
    info!("Created new session with ID: {}", stream_id);
    Ok(stream_id)
  }

  #[napi]
  pub async fn send_to_session(&self, session_id: u32, data: Buffer) -> Result<()> {
    let data_slice = data.as_ref().to_vec();
    if let Some(mut session) = self.sessions.get_mut(&session_id) {
      session.input_buffer.extend_from_slice(&data_slice);
      drop(session);
      let bytes_data = Bytes::copy_from_slice(&data_slice);
      self
        .pty_process
        .write_data(&bytes_data)
        .await
        .map_err(convert_io_error)?;
      info!("Sent data to session {}: {:?}", session_id, data_slice);
      Ok(())
    } else {
      Err(napi::Error::from_reason(format!(
        "Session ID {} not found",
        session_id
      )))
    }
  }

  #[napi]
  pub async fn broadcast(&self, data: Buffer) -> Result<()> {
    let data_slice = data.as_ref().to_vec();
    for mut session in self.sessions.iter_mut() {
      session.input_buffer.extend_from_slice(&data_slice);
    }
    let bytes_data = Bytes::copy_from_slice(&data_slice);
    self
      .pty_process
      .write_data(&bytes_data)
      .await
      .map_err(convert_io_error)?;
    info!("Broadcasted data to all sessions: {:?}", data_slice);
    Ok(())
  }

  #[napi]
  pub async fn read_from_session(&self, session_id: u32) -> Result<Buffer> {
    if let Some(session) = self.sessions.get(&session_id) {
      let mut output = session.output_buffer.lock().await;
      let data = output.clone();
      output.clear();
      info!("Read data from session {}: {:?}", session_id, data);
      Ok(Buffer::from(data))
    } else {
      Err(napi::Error::from_reason(format!(
        "Session ID {} not found",
        session_id
      )))
    }
  }

  #[napi]
  pub async fn read_all_sessions(&self) -> Result<Vec<SessionData>> {
    let mut result = Vec::new();
    for entry in self.sessions.iter() {
      let output = entry.value().output_buffer.lock().await.clone();
      result.push(SessionData {
        session_id: *entry.key(),
        data: Buffer::from(output),
      });
    }
    info!("Read data from all sessions.");
    Ok(result)
  }

  #[napi]
  pub async fn remove_session(&self, session_id: u32) -> Result<()> {
    if self.sessions.remove(&session_id).is_some() {
      info!("Removed session with ID: {}", session_id);
      Ok(())
    } else {
      Err(napi::Error::from_reason(format!(
        "Session ID {} not found",
        session_id
      )))
    }
  }

  #[napi]
  pub async fn merge_sessions(&self, session_ids: Vec<u32>) -> Result<()> {
    if session_ids.is_empty() {
      return Err(napi::Error::from_reason(
        "No session IDs provided for merging".to_string(),
      ));
    }
    info!("Merging sessions: {:?}", session_ids);
    let primary_session_id = session_ids[0];
    let primary_session = if let Some(session) = self.sessions.get(&primary_session_id) {
      session.clone()
    } else {
      return Err(napi::Error::from_reason(format!(
        "Primary session ID {} not found",
        primary_session_id
      )));
    };
    let mut sessions_to_merge = Vec::new();
    for &session_id in session_ids.iter().skip(1) {
      if let Some(session) = self.sessions.remove(&session_id) {
        sessions_to_merge.push((session_id, session));
      }
    }
    for (session_id, session) in sessions_to_merge {
      let mut primary_input = primary_session.input_buffer.clone();
      primary_input.extend_from_slice(&session.1.input_buffer);
      let mut primary_output = primary_session.output_buffer.lock().await;
      let session_output = &*session.1.output_buffer.lock().await;
      primary_output.extend_from_slice(&session_output);
      info!("Merged and removed session with ID: {}", session_id);
    }
    if let Some(mut updated_primary) = self.sessions.get_mut(&primary_session_id) {
      updated_primary.input_buffer = primary_session.input_buffer.clone();
      let mut primary_output_buffer = updated_primary.output_buffer.lock().await;
      *primary_output_buffer = primary_session.output_buffer.lock().await.clone();
    }
    info!(
      "Successfully merged sessions {:?} into session {}",
      session_ids, primary_session_id
    );
    Ok(())
  }

  #[napi]
  pub async fn split_session(&self, session_id: u32) -> Result<[u32; 2]> {
    info!("Splitting session: {}", session_id);
    let session_clone = if let Some(session) = self.sessions.get(&session_id) {
      session.clone()
    } else {
      return Err(napi::Error::from_reason(format!(
        "Session ID {} not found",
        session_id
      )));
    };
    let input_half_size = session_clone.input_buffer.len() / 2;
    let output_half_size = {
      let output = session_clone.output_buffer.lock().await;
      output.len() / 2
    };
    let input1 = session_clone.input_buffer[..input_half_size].to_vec();
    let input2 = session_clone.input_buffer[input_half_size..].to_vec();
    let output1 = {
      let output = session_clone.output_buffer.lock().await;
      output[..output_half_size].to_vec()
    };
    let output2 = {
      let output = session_clone.output_buffer.lock().await;
      output[output_half_size..].to_vec()
    };
    let new_session_1_id = self
      .sessions
      .iter()
      .map(|entry| *entry.key())
      .max()
      .unwrap_or(0)
      + 1;
    self.sessions.insert(
      new_session_1_id,
      PtySession {
        stream_id: new_session_1_id,
        input_buffer: input1,
        output_buffer: Arc::new(TokioMutex::new(output1)),
      },
    );
    let new_session_2_id = new_session_1_id + 1;
    self.sessions.insert(
      new_session_2_id,
      PtySession {
        stream_id: new_session_2_id,
        input_buffer: input2,
        output_buffer: Arc::new(TokioMutex::new(output2)),
      },
    );
    self.sessions.remove(&session_id);
    info!(
      "Successfully split session {} into sessions {} and {}",
      session_id, new_session_1_id, new_session_2_id
    );
    Ok([new_session_1_id, new_session_2_id])
  }

  #[napi]
  pub async fn set_env(&self, key: String, value: String) -> Result<()> {
    // Set the environment variable in the PTY process
    self
      .pty_process
      .set_env(key.clone(), value.clone())
      .await
      .map_err(convert_io_error)?;
    info!("Set environment variable: {} = {}", key, value);
    Ok(())
  }

  #[napi]
  pub async fn change_shell(&self, shell_path: String) -> Result<()> {
    let command = format!("exec {}\n", shell_path);
    let bytes_command = Bytes::copy_from_slice(command.as_bytes());
    self
      .pty_process
      .write_data(&bytes_command)
      .await
      .map_err(convert_io_error)?;
    info!("Changed shell to: {}", shell_path);
    Ok(())
  }

  #[napi]
  pub async fn status(&self) -> Result<String> {
    match self.pty_process.is_running().await {
      Ok(true) => Ok("Running".to_string()),
      Ok(false) => Ok("Terminated".to_string()),
      Err(e) => Err(convert_io_error(e)),
    }
  }

  #[napi]
  pub async fn set_log_level(&self, level: String) -> Result<()> {
    match level.to_lowercase().as_str() {
      "error" => log::set_max_level(log::LevelFilter::Error),
      "warn" => log::set_max_level(log::LevelFilter::Warn),
      "info" => log::set_max_level(log::LevelFilter::Info),
      "debug" => log::set_max_level(log::LevelFilter::Debug),
      "trace" => log::set_max_level(log::LevelFilter::Trace),
      _ => {
        error!("Invalid log level: {}", level);
        return Err(napi::Error::from_reason("Invalid log level".to_string()));
      }
    };
    info!("Log level set to: {}", level);
    Ok(())
  }

  #[napi]
  pub async fn close_all_sessions(&self) -> Result<()> {
    self.sessions.clear();
    info!("All sessions have been closed.");
    Ok(())
  }

  #[napi]
  pub async fn shutdown_pty(&self) -> Result<()> {
    self
      .pty_process
      .kill_process(SIGTERM)
      .await
      .map_err(convert_io_error)?;
    self
      .pty_process
      .waitpid(WNOHANG)
      .await
      .map_err(convert_io_error)?;
    self.close_all_sessions().await?;
    let _ = self.shutdown_sender.send(());
    info!("PTY has been gracefully shut down.");
    Ok(())
  }

  #[napi]
  pub async fn force_shutdown_pty(&self) -> Result<()> {
    self
      .pty_process
      .kill_process(SIGKILL)
      .await
      .map_err(convert_io_error)?;
    self
      .pty_process
      .waitpid(WNOHANG)
      .await
      .map_err(convert_io_error)?;
    self.close_all_sessions().await?;
    let _ = self.shutdown_sender.send(());
    info!("PTY has been forcefully shut down.");
    Ok(())
  }

  #[napi]
  pub async fn resize(&self, cols: u16, rows: u16) -> Result<()> {
    self
      .pty_process
      .resize(cols, rows)
      .await
      .map_err(convert_io_error)?;
    info!("Resized PTY to cols: {}, rows: {}", cols, rows);
    Ok(())
  }

  pub async fn dispatch_output(&self, data: &[u8]) -> Result<()> {
    let data_clone = data.to_vec();
    let session_entries: Vec<_> = self.sessions.iter().collect();
    let update_futures = session_entries.into_iter().map(|entry| {
      let session = entry.value().clone();
      let data = data_clone.clone();
      async move {
        let mut buffer = session.output_buffer.lock().await;
        buffer.extend_from_slice(&data);
        Ok::<(), io::Error>(())
      }
    });
    let results = join_all(update_futures).await;
    for res in results {
      res.map_err(|e| convert_io_error(io::Error::new(io::ErrorKind::Other, e)))?;
    }
    info!("Dispatched {} bytes to all sessions.", data.len());
    Ok(())
  }

  pub async fn read_and_dispatch(&self) -> Result<()> {
    let mut buffer = [0u8; 4096];
    match self.pty_process.read_data(&mut buffer).await {
      Ok(n) if n > 0 => {
        let data = buffer[..n].to_vec();
        info!("Read {} bytes from PTY.", n);
        self.dispatch_output(&data).await?;
      }
      Ok(_) => {}
      Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {}
      Err(e) => {
        error!("Error reading from PTY: {}", e);
        return Err(convert_io_error(e));
      }
    }
    Ok(())
  }

  pub async fn list_sessions(&self) -> Result<Vec<u32>> {
    let session_ids: Vec<u32> = self.sessions.iter().map(|entry| *entry.key()).collect();
    info!("Listing all active sessions: {:?}", session_ids);
    Ok(session_ids)
  }

  #[napi]
  pub async fn read_from_pty(&self) -> Result<Buffer> {
    let mut buffer = [0u8; 4096];
    match self.pty_process.read_data(&mut buffer).await {
      Ok(n) if n > 0 => Ok(Buffer::from(buffer[..n].to_vec())),
      Ok(_) => Ok(Buffer::from(Vec::new())),
      Err(e) => Err(convert_io_error(e)),
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use bytes::Bytes;
  use log::info;
  use once_cell::sync::OnceCell;
  use std::sync::Arc;
  use std::time::Duration;
  use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};
  use tokio::sync::Mutex as TokioMutex;
  use tokio::time::{self, timeout};

  static INIT: OnceCell<()> = OnceCell::new();
  const TEST_TIMEOUT: Duration = Duration::from_secs(10);
  const POLL_INTERVAL: Duration = Duration::from_millis(100);
  const MAX_RETRIES: u32 = 100;

  fn init_logger() {
    INIT.get_or_init(|| {
      env_logger::builder()
        .filter_level(log::LevelFilter::Debug)
        .is_test(true)
        .try_init()
        .ok();
    });
  }

  struct TestGuard {
    multiplexer: PtyMultiplexer,
  }

  impl Drop for TestGuard {
    fn drop(&mut self) {
      let _ = self.multiplexer.shutdown_sender.send(());
    }
  }

  async fn wait_for_output(
    multiplexer: &PtyMultiplexer,
    session_id: u32,
    expected_content: Option<&str>,
  ) -> Option<String> {
    for _ in 0..MAX_RETRIES {
      if let Ok(output) = multiplexer.read_from_session(session_id).await {
        let output_str = String::from_utf8_lossy(&output).to_string();
        if !output_str.is_empty() {
          if let Some(expected) = expected_content {
            if output_str.contains(expected) {
              return Some(output_str);
            }
          } else {
            return Some(output_str);
          }
        }
      }
      time::sleep(POLL_INTERVAL).await;
    }
    None
  }

  struct MockPtyProcess {
    write_sender: UnboundedSender<Vec<u8>>,
    read_receiver: TokioMutex<UnboundedReceiver<Vec<u8>>>,
    is_running: TokioMutex<bool>,
    env_vars: Arc<TokioMutex<HashMap<String, String>>>,
  }

  impl MockPtyProcess {
    fn new() -> Self {
      let (write_sender, mut write_receiver) = tokio::sync::mpsc::unbounded_channel::<Vec<u8>>();
      let (read_sender, read_receiver) = tokio::sync::mpsc::unbounded_channel::<Vec<u8>>();
      let env_vars = Arc::new(TokioMutex::new(HashMap::new()));
      let env_vars_clone = Arc::clone(&env_vars);

      tokio::spawn(async move {
        while let Some(data) = write_receiver.recv().await {
          let command = String::from_utf8_lossy(&data);
          if command.starts_with("echo ") {
            let rest = command[5..].trim_end_matches('\n');
            if rest.starts_with('$') {
              let key = rest[1..].to_string();
              let env = env_vars_clone.lock().await;
              let value = env.get(&key).cloned().unwrap_or_default();
              let response = format!("{}\n", value);
              let _ = read_sender.send(response.into_bytes());
            } else {
              let response = rest.to_string() + "\n";
              let _ = read_sender.send(response.into_bytes());
            }
          } else if command.starts_with("export ") {
            // parse export KEY="VALUE"
            let rest = command[7..].trim_end_matches('\n');
            if let Some(eq_pos) = rest.find('=') {
              let key = rest[..eq_pos].to_string();
              let value_part = rest[eq_pos + 1..].trim();
              let value = if value_part.starts_with('"')
                && value_part.ends_with('"')
                && value_part.len() >= 2
              {
                value_part[1..value_part.len() - 1].to_string()
              } else {
                value_part.to_string()
              };
              let mut env = env_vars_clone.lock().await;
              env.insert(key, value);
            }
          } else if command.starts_with("stty size") {
            let response = "40 100\n".to_string();
            let _ = read_sender.send(response.into_bytes());
          } else if command.starts_with("exec ") {
            // For simplicity, do nothing on exec commands
          } else {
            // Handle other commands if necessary
          }
        }
      });

      Self {
        write_sender,
        read_receiver: TokioMutex::new(read_receiver),
        is_running: TokioMutex::new(true),
        env_vars,
      }
    }
  }

  #[async_trait]
  impl PtyProcessInterface for MockPtyProcess {
    async fn set_env(&self, key: String, value: String) -> io::Result<()> {
      let mut env = self.env_vars.lock().await;
      env.insert(key, value);
      Ok(())
    }

    async fn write_data(&self, data: &Bytes) -> io::Result<usize> {
      self.write_sender.send(data.to_vec()).map_err(|_| {
        io::Error::new(io::ErrorKind::BrokenPipe, "Failed to send data to mock PTY")
      })?;
      Ok(data.len())
    }

    async fn read_data(&self, buffer: &mut [u8]) -> io::Result<usize> {
      let mut receiver = self.read_receiver.lock().await;
      match receiver.recv().await {
        Some(data) => {
          let len = data.len().min(buffer.len());
          buffer[..len].copy_from_slice(&data[..len]);
          Ok(len)
        }
        None => Ok(0),
      }
    }

    async fn is_running(&self) -> io::Result<bool> {
      let running = self.is_running.lock().await;
      Ok(*running)
    }

    async fn kill_process(&self, _signal: i32) -> io::Result<()> {
      let mut running = self.is_running.lock().await;
      *running = false;
      Ok(())
    }

    async fn waitpid(&self, _options: i32) -> io::Result<i32> {
      Ok(0)
    }

    async fn resize(&self, _cols: u16, _rows: u16) -> io::Result<()> {
      Ok(())
    }

    async fn get_pid(&self) -> io::Result<i32> {
      Ok(1234)
    }
  }

  async fn create_test_multiplexer() -> (PtyMultiplexer, TestGuard) {
    let mock_pty = Arc::new(MockPtyProcess::new());
    let sessions = Arc::new(DashMap::new());
    let (shutdown_sender, _) = broadcast::channel(1);
    let multiplexer = PtyMultiplexer::new_for_test(
      mock_pty as Arc<dyn PtyProcessInterface>,
      sessions.clone(),
      shutdown_sender,
    );
    let guard = TestGuard {
      multiplexer: multiplexer.clone(),
    };
    (multiplexer, guard)
  }

  impl PtyMultiplexer {
    pub fn new_for_test(
      pty_process: Arc<dyn PtyProcessInterface>,
      sessions: Arc<DashMap<u32, PtySession>>,
      shutdown_sender: broadcast::Sender<()>,
    ) -> Self {
      let multiplexer = Self {
        pty_process,
        sessions,
        shutdown_sender: shutdown_sender.clone(),
      };
      let multiplexer_clone = multiplexer.clone();
      tokio::spawn(async move {
        let mut shutdown_receiver = multiplexer_clone.shutdown_sender.subscribe();
        let mut retry_delay = Duration::from_secs(1);
        let max_retry_delay = Duration::from_secs(32);
        loop {
          tokio::select! {
              res = multiplexer_clone.read_and_dispatch() => {
                  match res {
                      Ok(_) => {
                          retry_delay = Duration::from_secs(1);
                      },
                      Err(e) => {
                          error!("Error in background PTY read task: {}", e);
                          sleep(retry_delay).await;
                          retry_delay = std::cmp::min(retry_delay * 2, max_retry_delay);
                          continue;
                      },
                  }
              },
              _ = shutdown_receiver.recv() => {
                  info!("Shutdown signal received. Terminating background PTY read task.");
                  break;
              },
          }
        }
        info!("Background PTY read task terminated.");
      });
      multiplexer
    }
  }

  #[tokio::test]
  async fn test_create_pty_multiplexer() {
    init_logger();
    let result = timeout(TEST_TIMEOUT, async {
      let (multiplexer, _guard) = create_test_multiplexer().await;
      let session_id = multiplexer
        .create_session()
        .await
        .expect("Failed to create session");
      assert_eq!(session_id, 1);
      multiplexer
        .send_to_session(session_id, Buffer::from("echo test\n".as_bytes()))
        .await
        .expect("Failed to send data to session");
      let output = wait_for_output(&multiplexer, session_id, Some("test"))
        .await
        .expect("Failed to get output in time");
      assert!(output.contains("test"));
    })
    .await;
    assert!(result.is_ok(), "Test timed out");
  }

  #[tokio::test]
  async fn test_resize_pty() {
    init_logger();
    let result = timeout(TEST_TIMEOUT, async {
      let (multiplexer, _guard) = create_test_multiplexer().await;
      let session_id = multiplexer
        .create_session()
        .await
        .expect("Failed to create session");
      multiplexer
        .resize(40, 100)
        .await
        .expect("Failed to resize PTY");
      time::sleep(Duration::from_millis(500)).await;
      multiplexer
        .send_to_session(session_id, Buffer::from("stty size\n".as_bytes()))
        .await
        .expect("Failed to send 'stty size' command");
      let output = wait_for_output(&multiplexer, session_id, Some("40 100"))
        .await
        .expect("Failed to get terminal size output");
      let lines: Vec<&str> = output.trim().split('\n').collect();
      let size_line = lines
        .iter()
        .find(|line| line.contains("40") && line.contains("100"))
        .expect("Could not find size information in output");
      assert!(size_line.contains("40 100"));
    })
    .await;
    assert!(result.is_ok(), "Test timed out");
  }

  #[tokio::test]
  async fn test_send_and_read_sessions() {
    init_logger();
    let result = timeout(TEST_TIMEOUT, async {
      let (multiplexer, _guard) = create_test_multiplexer().await;
      let session1 = multiplexer
        .create_session()
        .await
        .expect("Failed to create session 1");
      let session2 = multiplexer
        .create_session()
        .await
        .expect("Failed to create session 2");
      multiplexer
        .send_to_session(session1, Buffer::from("echo session1 data\n".as_bytes()))
        .await
        .expect("Failed to send to session 1");
      multiplexer
        .send_to_session(session2, Buffer::from("echo session2 data\n".as_bytes()))
        .await
        .expect("Failed to send to session 2");
      let output1 = wait_for_output(&multiplexer, session1, Some("session1 data"))
        .await
        .expect("Failed to get output from session 1");
      let output2 = wait_for_output(&multiplexer, session2, Some("session2 data"))
        .await
        .expect("Failed to get output from session 2");
      assert!(output1.contains("session1 data"));
      assert!(output2.contains("session2 data"));
    })
    .await;
    assert!(result.is_ok(), "Test timed out");
  }

  #[tokio::test]
  async fn test_set_env() {
    init_logger();
    let result = timeout(TEST_TIMEOUT, async {
      let (multiplexer, _guard) = create_test_multiplexer().await;
      let session_id = multiplexer
        .create_session()
        .await
        .expect("Failed to create session");
      multiplexer
        .set_env("TEST_ENV_VAR".to_string(), "test_value".to_string())
        .await
        .expect("Failed to set environment variable");
      time::sleep(Duration::from_millis(500)).await;
      multiplexer
        .send_to_session(session_id, Buffer::from("echo $TEST_ENV_VAR\n".as_bytes()))
        .await
        .expect("Failed to send echo command");
      let output = wait_for_output(&multiplexer, session_id, Some("test_value"))
        .await
        .expect("Failed to get environment variable output");
      assert!(output.contains("test_value"));
    })
    .await;
    assert!(result.is_ok(), "Test timed out");
  }

  #[tokio::test]
  async fn test_change_shell() {
    init_logger();
    let result = timeout(TEST_TIMEOUT, async {
      let (multiplexer, _guard) = create_test_multiplexer().await;
      let session_id = multiplexer
        .create_session()
        .await
        .expect("Failed to create session");
      multiplexer
        .change_shell("/bin/sh".to_string())
        .await
        .expect("Failed to change shell");
      time::sleep(Duration::from_millis(500)).await;
      multiplexer
        .send_to_session(session_id, Buffer::from("echo Shell Changed\n".as_bytes()))
        .await
        .expect("Failed to send echo command");
      let output = wait_for_output(&multiplexer, session_id, Some("Shell Changed"))
        .await
        .expect("Failed to get shell change confirmation");
      assert!(output.contains("Shell Changed"));
    })
    .await;
    assert!(result.is_ok(), "Test timed out");
  }

  #[tokio::test]
  async fn test_shutdown_pty() {
    init_logger();
    let result = timeout(TEST_TIMEOUT, async {
      let (multiplexer, _guard) = create_test_multiplexer().await;
      let session_id = multiplexer
        .create_session()
        .await
        .expect("Failed to create session");
      multiplexer
        .send_to_session(
          session_id,
          Buffer::from("echo Before Shutdown\n".as_bytes()),
        )
        .await
        .expect("Failed to send command");
      let output = wait_for_output(&multiplexer, session_id, Some("Before Shutdown"))
        .await
        .expect("Failed to get pre-shutdown output");
      assert!(output.contains("Before Shutdown"));
      multiplexer
        .shutdown_pty()
        .await
        .expect("Failed to shutdown PTY");
      let send_result = multiplexer
        .send_to_session(session_id, Buffer::from("echo After Shutdown\n".as_bytes()))
        .await;
      assert!(send_result.is_err(), "Expected send after shutdown to fail");
      if let Ok(final_output) = multiplexer.read_from_session(session_id).await {
        let final_str = String::from_utf8_lossy(&final_output).to_string();
        assert!(!final_str.contains("After Shutdown"));
      }
    })
    .await;
    assert!(result.is_ok(), "Test timed out");
  }
}
