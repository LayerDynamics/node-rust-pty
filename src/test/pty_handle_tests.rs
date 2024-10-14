// src/test/pty_handle_tests.rs

#[cfg(test)]
mod tests {
  use super::*;
  use crate::pty::handle::PtyHandle; // Import the PtyHandle
  use crate::pty::multiplexer::PtyMultiplexer;
  use log::{error, info};
  use serial_test::serial; // Import for serial testing
  use std::time::Duration;
  use tokio::time; // Import the `time` module

  /// Helper function to initialize logging for tests
  fn init_test_logging() {
    use crate::utils::logging::initialize_logging;
    initialize_logging();
  }

  #[tokio::test(flavor = "multi_thread", worker_threads = 1)]
  #[serial]
  async fn test_create_pty_handle() {
    init_test_logging();
    let pty_handle = PtyHandle::new().await;
    assert!(
      pty_handle.is_ok(),
      "Failed to create PtyHandle: {:?}",
      pty_handle.err()
    );
    let pty = pty_handle.unwrap();
    assert!(pty.pid().is_some(), "Invalid child process ID");

    info!("test_create_pty_handle: Completed successfully.");

    // Explicitly close the PTY handle
    pty.close().await.expect("Failed to close PtyHandle");
    info!("test_create_pty_handle: PtyHandle closed.");
  }

  #[tokio::test(flavor = "multi_thread", worker_threads = 1)]
  #[serial]
  async fn test_write_to_pty() {
    init_test_logging();
    let start_time = std::time::Instant::now();

    let pty_handle = PtyHandle::new().await.expect("Failed to create PtyHandle");
    info!(
      "test_write_to_pty: PtyHandle created in {:?}",
      start_time.elapsed()
    );

    // Write data to the PTY
    let write_start = std::time::Instant::now();
    let write_result = pty_handle.write(String::from("test data")).await;
    info!(
      "test_write_to_pty: Writing to PTY took {:?}",
      write_start.elapsed()
    );

    assert!(
      write_result.is_ok(),
      "Failed to write to PTY: {:?}",
      write_result.err()
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
        assert_eq!(data.trim(), "test data", "Incorrect data read from PTY");
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
    init_test_logging();
    let start_time = std::time::Instant::now();

    let pty_handle = PtyHandle::new().await.expect("Failed to create PtyHandle");
    info!(
      "test_resize_pty: PtyHandle created in {:?}",
      start_time.elapsed()
    );

    // Resize the PTY
    let resize_start = std::time::Instant::now();
    let resize_result = pty_handle.resize(80, 24).await;
    info!(
      "test_resize_pty: Resizing PTY took {:?}",
      resize_start.elapsed()
    );

    assert!(
      resize_result.is_ok(),
      "Failed to resize PTY: {:?}",
      resize_result.err()
    );

    info!("test_resize_pty: Resize operation successful.");

    // Explicitly close the PTY handle
    pty_handle.close().await.expect("Failed to close PtyHandle");
    info!("test_resize_pty: PtyHandle closed.");
  }

  #[tokio::test(flavor = "multi_thread", worker_threads = 1)]
  #[serial]
  async fn test_write_and_read_from_pty() {
    init_test_logging();
    let start_time = std::time::Instant::now();

    let pty_handle = PtyHandle::new().await.expect("Failed to create PtyHandle");
    info!(
      "test_write_and_read_from_pty: PtyHandle created in {:?}",
      start_time.elapsed()
    );

    // Write a command to the PTY
    let write_data = String::from("echo hello");
    let write_start = std::time::Instant::now();
    let write_result = pty_handle.write(write_data.clone()).await;
    info!(
      "test_write_and_read_from_pty: Writing to PTY took {:?}",
      write_start.elapsed()
    );

    assert!(
      write_result.is_ok(),
      "Failed to write to PTY: {:?}",
      write_result.err()
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
          "PTY did not output expected 'hello'"
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
    init_test_logging();
    let start_time = std::time::Instant::now();

    let pty_handle = PtyHandle::new().await.expect("Failed to create PtyHandle");
    info!(
      "test_kill_process: PtyHandle created in {:?}",
      start_time.elapsed()
    );

    // Kill the PTY process
    let kill_start = std::time::Instant::now();
    let kill_result = pty_handle.kill_process(libc::SIGTERM).await;
    info!(
      "test_kill_process: Sending SIGTERM took {:?}",
      kill_start.elapsed()
    );

    assert!(
      kill_result.is_ok(),
      "Failed to send SIGTERM to PTY process: {:?}",
      kill_result.err()
    );
    info!("test_kill_process: SIGTERM sent successfully.");

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
  async fn test_close_master_fd() {
    init_test_logging();
    let start_time = std::time::Instant::now();

    let pty_handle = PtyHandle::new().await.expect("Failed to create PtyHandle");
    info!(
      "test_close_master_fd: PtyHandle created in {:?}",
      start_time.elapsed()
    );

    // Attempt to close the master file descriptor
    let close_start = std::time::Instant::now();
    let close_result = pty_handle.close_master_fd().await;
    info!(
      "test_close_master_fd: Closing master_fd took {:?}",
      close_start.elapsed()
    );

    assert!(
      close_result.is_ok(),
      "Failed to close PTY master file descriptor: {:?}",
      close_result.err()
    );
    info!("test_close_master_fd: Master_fd closed successfully.");

    // Attempt to write to the PTY after closing the master FD
    let write_start = std::time::Instant::now();
    let write_result = pty_handle.write(String::from("test")).await;
    info!(
      "test_close_master_fd: Attempting to write after closing took {:?}",
      write_start.elapsed()
    );

    assert!(
      write_result.is_err(),
      "Write succeeded even after master FD was closed"
    );

    if let Err(err) = write_result {
      assert!(
        err.to_string().contains("Bad file descriptor") || err.to_string().contains("closed"),
        "Unexpected error after closing master FD: {}",
        err
      );
      info!(
        "test_close_master_fd: Received expected error after closing master_fd: {}",
        err
      );
    }

    info!(
      "test_close_master_fd: Completed in {:?}",
      start_time.elapsed()
    );

    // Explicitly close the PTY handle (even though master_fd is already closed)
    pty_handle.close().await.expect("Failed to close PtyHandle");
    info!("test_close_master_fd: PtyHandle closed.");
  }

  #[tokio::test(flavor = "multi_thread", worker_threads = 1)]
  #[serial]
  async fn test_multiplexer_basic() {
    init_test_logging();
    let start_time = std::time::Instant::now();

    let pty_handle = PtyHandle::new().await.expect("Failed to create PtyHandle");
    info!(
      "test_multiplexer_basic: PtyHandle created in {:?}",
      start_time.elapsed()
    );

    // Get the underlying PtyProcess from the PtyHandle
    let pty_process = pty_handle
      .get_pty_process()
      .expect("Failed to get PtyProcess from PtyHandle");
    info!("test_multiplexer_basic: Retrieved PtyProcess.");

    // Create the multiplexer using the PtyProcess
    let mut multiplexer = PtyMultiplexer::new(pty_process);
    info!("test_multiplexer_basic: PtyMultiplexer created.");

    // Create two sessions
    let session1 = multiplexer.create_session();
    let session2 = multiplexer.create_session();
    info!(
      "test_multiplexer_basic: Created sessions {} and {}.",
      session1, session2
    );

    // Send data to session 1
    let write_start1 = std::time::Instant::now();
    let write_result1 = multiplexer.send_to_session(session1, b"session1 data");
    info!(
      "test_multiplexer_basic: Sending to session1 took {:?}",
      write_start1.elapsed()
    );

    assert!(
      write_result1.is_ok(),
      "Failed to write to session 1: {:?}",
      write_result1.err()
    );

    // Send data to session 2
    let write_start2 = std::time::Instant::now();
    let write_result2 = multiplexer.send_to_session(session2, b"session2 data");
    info!(
      "test_multiplexer_basic: Sending to session2 took {:?}",
      write_start2.elapsed()
    );

    assert!(
      write_result2.is_ok(),
      "Failed to write to session 2: {:?}",
      write_result2.err()
    );

    // Distribute the output from the PTY to all sessions
    let distribute_start = std::time::Instant::now();
    let distribute_result = multiplexer.distribute_output();
    info!(
      "test_multiplexer_basic: Distributing output took {:?}",
      distribute_start.elapsed()
    );

    assert!(
      distribute_result.is_ok(),
      "Failed to distribute output to sessions: {:?}",
      distribute_result.err()
    );

    // Check that both sessions received their respective data
    let read_start1 = std::time::Instant::now();
    let session1_output = multiplexer
      .read_from_session(session1)
      .await
      .expect("Failed to read from session 1");
    info!(
      "test_multiplexer_basic: Reading from session1 took {:?}",
      read_start1.elapsed()
    );

    assert!(
      session1_output
        .windows(b"session1 data".len())
        .any(|window| window == b"session1 data"),
      "Session 1 did not receive the correct output"
    );
    info!("test_multiplexer_basic: Session1 received correct output.");

    let read_start2 = std::time::Instant::now();
    let session2_output = multiplexer
      .read_from_session(session2)
      .await
      .expect("Failed to read from session 2");
    info!(
      "test_multiplexer_basic: Reading from session2 took {:?}",
      read_start2.elapsed()
    );

    assert!(
      session2_output
        .windows(b"session2 data".len())
        .any(|window| window == b"session2 data"),
      "Session 2 did not receive the correct output"
    );
    info!("test_multiplexer_basic: Session2 received correct output.");

    info!(
      "test_multiplexer_basic: Completed in {:?}",
      start_time.elapsed()
    );

    // Explicitly close the PTY handle
    pty_handle.close().await.expect("Failed to close PtyHandle");
    info!("test_multiplexer_basic: PtyHandle closed.");
  }
}
