// src/test/pty_handle_tests.rs

#[cfg(test)]
mod tests {
  use crate::pty::handle::{MultiplexerHandle, PtyHandle};
  use crate::utils::logging::initialize_logging;
  use log::error;
  use log::info;
  use serial_test::serial;
  use std::time::Duration;
  use tokio::time;

  use nix::libc::SIGTERM;

  #[tokio::test(flavor = "multi_thread", worker_threads = 1)]
  #[serial]
  async fn test_create_pty_handle() {
    initialize_logging();
    let pty_handle = PtyHandle::new_for_test().expect("Failed to create PtyHandle");

    // Example: You might have a method to get the PID or other attributes
    let pid = pty_handle.pid().await.expect("Failed to get PID");
    assert!(pid > 0, "Invalid PID retrieved from PtyHandle");

    info!("test_create_pty_handle: Completed successfully.");

    // Explicitly close the PTY handle
    pty_handle.close().await.expect("Failed to close PtyHandle");
    info!("test_create_pty_handle: PtyHandle closed.");
  }

  #[tokio::test(flavor = "multi_thread", worker_threads = 1)]
  #[serial]
  async fn test_write_to_pty() {
    initialize_logging();
    let start_time = std::time::Instant::now();

    let pty_handle = PtyHandle::new_for_test().expect("Failed to create PtyHandle");
    info!(
      "test_write_to_pty: PtyHandle created in {:?}",
      start_time.elapsed()
    );

    // Write data to the PTY
    let write_start = std::time::Instant::now();
    pty_handle
      .write(String::from("echo test data\n"))
      .await
      .expect("Failed to write to PTY");
    info!(
      "test_write_to_pty: Writing to PTY took {:?}",
      write_start.elapsed()
    );

    // Read data from the PTY with a timeout
    let read_start = std::time::Instant::now();
    let read_result = time::timeout(Duration::from_secs(5), pty_handle.read()).await;
    info!(
      "test_write_to_pty: Reading from PTY took {:?}",
      read_start.elapsed()
    );

    match read_result {
      Ok(Ok(data)) => {
        info!("test_write_to_pty: Data read from PTY: {}", data);
        assert!(
          data.contains("test data"),
          "Incorrect data read from PTY. Output: {}",
          data
        );
      }
      Ok(Err(e)) => {
        panic!("test_write_to_pty: Failed to read from PTY: {}", e);
      }
      Err(_) => {
        panic!("test_write_to_pty: Read operation timed out");
      }
    }
    info!("test_write_to_pty: Completed in {:?}", start_time.elapsed());

    // Explicitly close the PTY handle
    pty_handle.close().await.expect("Failed to close PtyHandle");
    info!("test_write_to_pty: PtyHandle closed.");
  }

  #[tokio::test(flavor = "multi_thread", worker_threads = 1)]
  #[serial]
  async fn test_resize_pty() {
    initialize_logging();
    let start_time = std::time::Instant::now();
    let pty_handle = PtyHandle::new_for_test().expect("Failed to create PtyHandle");
    info!(
      "test_resize_pty: PtyHandle created in {:?}",
      start_time.elapsed()
    );

    // Resize the PTY
    let resize_start = std::time::Instant::now();
    pty_handle
      .resize(40, 100)
      .await
      .expect("Failed to resize PTY");
    info!(
      "test_resize_pty: Resizing PTY took {:?}",
      resize_start.elapsed()
    );

    // Execute 'stty size' to verify the terminal size
    let verify_start = std::time::Instant::now();
    let verify_command = "stty size\n";
    pty_handle
      .write(verify_command.to_string())
      .await
      .expect("Failed to write verify command to PTY");
    info!(
      "test_resize_pty: Writing verify command took {:?}",
      verify_start.elapsed()
    );

    // Read the output with a timeout
    let read_start = std::time::Instant::now();
    let read_result = time::timeout(Duration::from_secs(5), pty_handle.read()).await;
    info!(
      "test_resize_pty: Reading verification output took {:?}",
      read_start.elapsed()
    );

    match read_result {
      Ok(Ok(output)) => {
        info!("test_resize_pty: Output after resize: '{}'", output.trim());
        // Split the output into lines
        let lines: Vec<&str> = output.trim().split('\n').collect();
        // The first line is the echoed command, the second line should be the size
        if lines.len() >= 2 {
          let size_output = lines[1];
          assert!(
            size_output.contains("40 100"),
            "Incorrect terminal size after resize. Expected '40 100', got '{}'",
            size_output
          );
        } else {
          panic!(
            "Terminal size output incomplete. Expected at least 2 lines, got {}",
            lines.len()
          );
        }
      }
      Ok(Err(e)) => {
        panic!("test_resize_pty: Failed to read verification output: {}", e);
      }
      Err(_) => {
        panic!("test_resize_pty: Read operation timed out");
      }
    }
    info!("test_resize_pty: Completed in {:?}", start_time.elapsed());

    // Explicitly close the PTY handle
    pty_handle.close().await.expect("Failed to close PtyHandle");
    info!("test_resize_pty: PtyHandle closed.");
  }

  #[tokio::test(flavor = "multi_thread", worker_threads = 1)]
  #[serial]
  async fn test_write_and_read_from_pty() {
    initialize_logging();
    let start_time = std::time::Instant::now();

    let pty_handle = PtyHandle::new_for_test().expect("Failed to create PtyHandle");
    info!(
      "test_write_and_read_from_pty: PtyHandle created in {:?}",
      start_time.elapsed()
    );

    // Write a command to the PTY
    let write_data = String::from("echo hello\n");
    let write_start = std::time::Instant::now();
    pty_handle
      .write(write_data.clone())
      .await
      .expect("Failed to write to PTY");
    info!(
      "test_write_and_read_from_pty: Writing to PTY took {:?}",
      write_start.elapsed()
    );

    // Wait for the command to execute and read the output with a timeout
    let sleep_start = std::time::Instant::now();
    time::sleep(Duration::from_millis(100)).await;
    info!(
      "test_write_and_read_from_pty: Sleeping for {:?} before reading.",
      sleep_start.elapsed()
    );

    let read_start = std::time::Instant::now();
    let read_result = time::timeout(Duration::from_secs(5), pty_handle.read()).await;
    info!(
      "test_write_and_read_from_pty: Reading from PTY took {:?}",
      read_start.elapsed()
    );

    match read_result {
      Ok(Ok(output)) => {
        info!(
          "test_write_and_read_from_pty: Data read from PTY: {}",
          output
        );
        assert!(
          output.contains("hello"),
          "PTY did not output expected 'hello'. Output: {}",
          output
        );
      }
      Ok(Err(e)) => {
        panic!(
          "test_write_and_read_from_pty: Failed to read from PTY: {}",
          e
        );
      }
      Err(_) => {
        panic!("test_write_and_read_from_pty: Read operation timed out");
      }
    }
    info!(
      "test_write_and_read_from_pty: Completed in {:?}",
      start_time.elapsed()
    );

    // Explicitly close the PTY handle
    pty_handle.close().await.expect("Failed to close PtyHandle");
    info!("test_write_and_read_from_pty: PtyHandle closed.");
  }

  #[tokio::test(flavor = "multi_thread", worker_threads = 1)]
  #[serial]
  async fn test_kill_process() {
    initialize_logging();
    let start_time = std::time::Instant::now();

    let pty_handle = PtyHandle::new_for_test().expect("Failed to create PtyHandle");
    info!(
      "test_kill_process: PtyHandle created in {:?}",
      start_time.elapsed()
    );

    // Kill the PTY process
    let kill_start = std::time::Instant::now();
    pty_handle
      .kill_process(SIGTERM)
      .await
      .expect("Failed to send SIGTERM to PTY process");
    info!(
      "test_kill_process: Sending SIGTERM took {:?}",
      kill_start.elapsed()
    );

    // Wait for the PTY process to terminate
    let wait_start = std::time::Instant::now();
    let wait_result = time::timeout(Duration::from_secs(5), pty_handle.waitpid(0)).await;
    info!(
      "test_kill_process: Waiting for process termination took {:?}",
      wait_start.elapsed()
    );

    match wait_result {
      Ok(Ok(pid)) => {
        info!("test_kill_process: Process {} terminated.", pid);
        assert!(pid > 0, "Failed to wait for PTY process to terminate");
      }
      Ok(Err(e)) => {
        panic!(
          "test_kill_process: Failed to wait for PTY process to terminate: {}",
          e
        );
      }
      Err(_) => {
        panic!("test_kill_process: Waitpid operation timed out");
      }
    }
    info!("test_kill_process: Completed in {:?}", start_time.elapsed());

    // Explicitly close the PTY handle
    pty_handle.close().await.expect("Failed to close PtyHandle");
    info!("test_kill_process: PtyHandle closed.");
  }

  #[tokio::test(flavor = "multi_thread", worker_threads = 1)]
  #[serial]
  async fn test_set_env() {
    initialize_logging();
    let pty_handle = PtyHandle::new_for_test().expect("Failed to create PtyHandle");

    // Set an environment variable
    pty_handle
      .set_env("TEST_ENV_VAR".to_string(), "test_value".to_string())
      .await
      .expect("Failed to set environment variable");

    // Verify that the environment variable is set
    let value = std::env::var("TEST_ENV_VAR").expect("Environment variable not found");
    assert_eq!(
      value, "test_value",
      "Environment variable has incorrect value"
    );

    // Explicitly close the PTY handle
    pty_handle.close().await.expect("Failed to close PtyHandle");
    info!("test_set_env: PtyHandle closed.");
  }

  #[tokio::test(flavor = "multi_thread", worker_threads = 1)]
  #[serial]
  async fn test_execute_command() {
    initialize_logging();
    let pty_handle = PtyHandle::new_for_test().expect("Failed to create PtyHandle");

    // Execute a command and get the output
    let output = pty_handle
      .execute("echo execute test\n".to_string())
      .await
      .expect("Failed to execute command");

    // Verify the output
    assert!(
      output.contains("execute test"),
      "Execute command did not return expected output. Output: {}",
      output
    );

    // Explicitly close the PTY handle
    pty_handle.close().await.expect("Failed to close PtyHandle");
    info!("test_execute_command: PtyHandle closed.");
  }

  #[cfg(target_os = "macos")] // Ensure this test is run on macOS
  #[tokio::test(flavor = "multi_thread", worker_threads = 1)]
  #[serial]
  async fn test_multiplexer_basic() {
    initialize_logging();
    let start_time = std::time::Instant::now();

    let pty_handle = PtyHandle::new_for_test().expect("Failed to create PtyHandle");
    info!(
      "test_multiplexer_basic: PtyHandle created in {:?}",
      start_time.elapsed()
    );

    // Get the MultiplexerHandle from the PtyHandle using the getter
    let multiplexer_handle = pty_handle.multiplexer_handle();
    info!("test_multiplexer_basic: Retrieved MultiplexerHandle.");

    // Create two sessions
    let session1 = multiplexer_handle
      .create_session()
      .await
      .expect("Failed to create session 1");
    let session2 = multiplexer_handle
      .create_session()
      .await
      .expect("Failed to create session 2");
    info!(
      "test_multiplexer_basic: Created sessions {:?} and {:?}.",
      session1, session2
    );

    // Add a small delay to ensure sessions are ready
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Send executable commands to each session
    let cmd1 = "echo session1 data\n";
    let cmd2 = "echo session2 data\n";

    // Send data to session 1
    let write_start1 = std::time::Instant::now();
    multiplexer_handle
      .send_to_session(session1, cmd1.as_bytes().to_vec())
      .await
      .expect("Failed to send to session 1");
    info!(
      "test_multiplexer_basic: Sending to session1 took {:?}",
      write_start1.elapsed()
    );

    // Send data to session 2
    let write_start2 = std::time::Instant::now();
    multiplexer_handle
      .send_to_session(session2, cmd2.as_bytes().to_vec())
      .await
      .expect("Failed to send to session 2");
    info!(
      "test_multiplexer_basic: Sending to session2 took {:?}",
      write_start2.elapsed()
    );

    // Add a longer delay to ensure both sessions have time to process the commands
    tokio::time::sleep(Duration::from_millis(700)).await;

    // Function to retry reading up to 15 times with delays and error logging
    async fn read_with_retries(
      multiplexer_handle: &MultiplexerHandle,
      session_id: u32,
    ) -> Option<String> {
      for attempt in 1..=15 {
        match multiplexer_handle.read_from_session(session_id).await {
          Ok(output) if !output.is_empty() => {
            return Some(output);
          }
          Ok(_) => {
            error!(
              "Attempt {}: Session {} read returned empty output, retrying...",
              attempt, session_id
            );
          }
          Err(e) => {
            error!(
              "Attempt {}: Failed to read from session {}: {:?}, retrying...",
              attempt, session_id, e
            );
          }
        }
        // Wait before retrying
        tokio::time::sleep(Duration::from_millis(300)).await;
      }
      None
    }

    // Read from session 1
    let read_start1 = std::time::Instant::now();
    let session1_output = read_with_retries(&multiplexer_handle, session1)
      .await
      .expect("Failed to read from session 1 after multiple retries");
    info!(
      "test_multiplexer_basic: Reading from session1 took {:?}",
      read_start1.elapsed()
    );

    // Assert the output for session1
    assert!(
      session1_output.contains("session1"),
      "Session 1 did not receive the correct output. Actual: {}",
      session1_output
    );

    // Read from session 2
    let read_start2 = std::time::Instant::now();
    let session2_output = read_with_retries(&multiplexer_handle, session2)
      .await
      .expect("Failed to read from session 2 after multiple retries");
    info!(
      "test_multiplexer_basic: Reading from session2 took {:?}",
      read_start2.elapsed()
    );

    // Assert the output for session2
    assert!(
      session2_output.contains("session2"),
      "Session 2 did not receive the correct output. Actual: {}",
      session2_output
    );

    info!(
      "test_multiplexer_basic: Completed in {:?}",
      start_time.elapsed()
    );

    // Explicitly close the PTY handle
    pty_handle.close().await.expect("Failed to close PtyHandle");
    info!("test_multiplexer_basic: PtyHandle closed.");
  }
}
