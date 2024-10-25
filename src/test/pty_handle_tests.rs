use crate::pty::command_handler::handle_commands;
use crate::pty::commands::{PtyCommand, PtyResult};
use crate::pty::multiplexer::PtyMultiplexer;
use crate::pty::platform::PtyProcess;
use crate::utils::logging::initialize_logging;
use crate::worker::pty_worker::PtyWorker;
use crate::PtyHandle; // Import PtyHandle
use bytes::Bytes;
use crossbeam_channel::{bounded, Sender};
use dashmap::DashMap;
use lock_api::Mutex as LockApiMutex;
use log::{error, info};
use parking_lot::RawMutex;
use std::ptr::NonNull;
use std::sync::Arc;
use std::thread;
use std::time::Duration;
use tokio::sync::{broadcast, oneshot, Mutex};
use tokio::time::{sleep, timeout};

// Import napi types
use napi::{Env, JsObject, NapiValue};

const OPERATION_TIMEOUT: Duration = Duration::from_secs(5);
const SLEEP_DURATION: Duration = Duration::from_millis(100);
const SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(10);

/// Context for managing PtyHandle during tests.
pub struct TestContext {
    handle: Arc<PtyHandle>,
    multiplexer: Arc<Mutex<PtyMultiplexer>>,
    command_handler_handle: thread::JoinHandle<()>,
    read_task_handle: tokio::task::JoinHandle<()>,
    _shutdown_sender: broadcast::Sender<()>,
}

impl Drop for TestContext {
    fn drop(&mut self) {
        // Ensure shutdown signal is sent
        let _ = self._shutdown_sender.send(());

        // Give a short time for cleanup
        thread::sleep(Duration::from_millis(100));

        // Force shutdown if still running
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let _ = self.handle.force_kill(1000).await;
        });
    }
}

impl TestContext {
    /// Sets up the PtyHandle and associated background tasks for testing.
    async fn setup_pty_handle() -> TestContext {
        // Initialize logging for the test
        initialize_logging();

        // Create a mock PtyProcess with timeout
        let pty_process = timeout(OPERATION_TIMEOUT, async {
            crate::pty::platform::PtyProcess::new()
        })
        .await
        .expect("Setup timeout")
        .expect("Failed to create PtyProcess");

        // Initialize channels and multiplexer
        let (_command_sender, command_receiver) = bounded::<PtyCommand>(100);
        let (_result_sender, _result_receiver) = bounded::<PtyResult>(100);
        let (shutdown_sender, _) = broadcast::channel(1);
        let _sessions = Arc::new(DashMap::<String, PtyProcess>::new());

        // Create dummy Env and JsObject for testing using NonNull to avoid null pointers
        let env_ptr = NonNull::<napi::sys::napi_env__>::dangling().as_ptr();
        let env = unsafe { Env::from_raw(env_ptr) };
        let js_object_ptr = NonNull::<napi::sys::napi_value__>::dangling().as_ptr();
        let js_object = unsafe { JsObject::from_raw(env.raw(), js_object_ptr).unwrap() };

        // Set up multiplexer
        let multiplexer = Arc::new(Mutex::new(
            PtyMultiplexer::new(env, js_object).expect("Failed to create PtyMultiplexer"),
        ));

        // Initialize worker and handle
        let _worker = PtyWorker::new();

        let multiplexer_guard = multiplexer.lock().await;
        let _multiplexer_instance = multiplexer_guard.clone();
        drop(multiplexer_guard); // Drop the guard to release the lock

        // Create dummy Env for PtyHandle
        let env_handle_ptr = NonNull::<napi::sys::napi_env__>::dangling().as_ptr();
        let env_handle = unsafe { Env::from_raw(env_handle_ptr) };

        let handle = Arc::new(PtyHandle::new(env_handle).expect("Failed to create PtyHandle"));

        // Start command handler thread
        let command_receiver_clone = command_receiver.clone();
        let result_sender_clone = _result_sender.clone();
        let pty_process_clone = pty_process.clone();
        let multiplexer_clone = Arc::clone(&multiplexer);

        let handle_clone = Arc::clone(&handle);

        let command_handler_handle = thread::spawn(move || {
            let pty_process_locked =
                Arc::new(LockApiMutex::<RawMutex, Option<PtyProcess>>::new(Some(pty_process_clone)));
            let multiplexer_guard = multiplexer_clone.blocking_lock().clone();
            let multiplexer_locked = Arc::new(LockApiMutex::<RawMutex, Option<PtyMultiplexer>>::new(
                Some(multiplexer_guard),
            ));
            handle_commands(
                pty_process_locked,
                multiplexer_locked,
                command_receiver_clone,
                result_sender_clone,
            );
        });

        // Start background read task
        let multiplexer_clone = Arc::clone(&multiplexer);
        let mut shutdown_receiver = shutdown_sender.subscribe();

        let read_task_handle = tokio::spawn(async move {
            let mut shutdown_received = false;
            while !shutdown_received {
                tokio::select! {
                    // Check for shutdown signal
                    Ok(_) = shutdown_receiver.recv() => {
                        info!("Shutdown signal received in read task");
                        shutdown_received = true;
                    }
                    // Perform read with timeout
                    _ = async {
                        if let Ok(multiplexer_guard) = timeout(
                            Duration::from_millis(100),
                            multiplexer_clone.lock(),
                        ).await {
                            let multiplexer = multiplexer_guard.clone();
                            if let Err(e) = multiplexer.read_and_dispatch().await {
                                error!("Error in background PTY read task: {}", e);
                            }
                        }
                        sleep(SLEEP_DURATION).await;
                    } => {}
                }
            }
            info!("Background read task terminated");
        });

        TestContext {
            handle,
            multiplexer,
            command_handler_handle,
            read_task_handle,
            _shutdown_sender: shutdown_sender,
        }
    }

    /// Shuts down the PtyHandle and waits for background tasks to terminate.
    async fn shutdown(mut self) {
        // Send shutdown signal
        let _ = self._shutdown_sender.send(());

        // Close PTY handle with timeout
        if let Err(e) = timeout(SHUTDOWN_TIMEOUT, self.handle.close()).await {
            error!("Failed to close PTY handle: {:?}", e);
            // Force kill as fallback
            let _ = self.handle.force_kill(1000).await;
        }

        // Wait for command handler with timeout
        let command_handler_handle =
            std::mem::replace(&mut self.command_handler_handle, thread::spawn(|| {}));
        let command_handler_timeout = timeout(
            SHUTDOWN_TIMEOUT,
            tokio::task::spawn_blocking(move || {
                if let Err(e) = command_handler_handle.join() {
                    error!("Command handler panicked: {:?}", e);
                }
            }),
        )
        .await;

        // Wait for read task with timeout
        let read_task_handle = std::mem::replace(&mut self.read_task_handle, tokio::spawn(async {}));
        let read_task_timeout = timeout(SHUTDOWN_TIMEOUT, read_task_handle).await;

        // Handle timeouts
        tokio::select! {
            _ = async { command_handler_timeout } => {
                info!("Command handler terminated");
            }
            _ = async { read_task_timeout } => {
                info!("Read task terminated");
            }
            _ = sleep(SHUTDOWN_TIMEOUT) => {
                error!("Shutdown timed out");
            }
        }
    }
}

/// Helper function to wait for specific output with timeout
async fn wait_for_output(handle: &Arc<PtyHandle>, expected: &str) -> Option<String> {
    let start = std::time::Instant::now();
    while start.elapsed() < OPERATION_TIMEOUT {
        if let Ok(output) = handle.read().await {
            if output.contains(expected) {
                return Some(output);
            }
        }
        sleep(SLEEP_DURATION).await;
    }
    None
}

// Example test functions

#[tokio::test]
async fn test_create_pty_handle() {
    let context = TestContext::setup_pty_handle().await;

    // Use timeout for status check
    match timeout(OPERATION_TIMEOUT, context.handle.status()).await {
        Ok(Ok(status)) => {
            assert!(
                status.contains("Running") || status.contains("active"),
                "PtyHandle is not properly initialized"
            );
        }
        Ok(Err(e)) => panic!("Failed to get PTY status: {}", e),
        Err(_) => panic!("Status check timed out"),
    }

    // Explicit cleanup
    context.shutdown().await;
}

#[tokio::test]
async fn test_write_to_pty() {
    let context = TestContext::setup_pty_handle().await;

    // Write with timeout
    match timeout(
        OPERATION_TIMEOUT,
        context.handle.write("echo test data\n".to_string()),
    )
    .await
    {
        Ok(Ok(_)) => {
            // Wait for output with timeout
            let output = wait_for_output(&context.handle, "test data")
                .await
                .expect("Failed to get output");
            assert!(output.contains("test data"));
        }
        Ok(Err(e)) => panic!("Write failed: {}", e),
        Err(_) => panic!("Write operation timed out"),
    }

    context.shutdown().await;
}

#[tokio::test]
async fn test_resize_pty() {
    let context = TestContext::setup_pty_handle().await;

    // Resize with timeout
    match timeout(OPERATION_TIMEOUT, context.handle.resize(40, 100)).await {
        Ok(Ok(_)) => {
            // Verify size with stty
            if let Ok(_) = context.handle.write("stty size\n".to_string()).await {
                let output = wait_for_output(&context.handle, "40 100")
                    .await
                    .expect("Failed to get size output");
                assert!(output.contains("40 100"));
            }
        }
        Ok(Err(e)) => panic!("Resize failed: {}", e),
        Err(_) => panic!("Resize operation timed out"),
    }

    context.shutdown().await;
}

#[tokio::test]
async fn test_write_and_read_from_pty() {
    let context = TestContext::setup_pty_handle().await;

    // Write with timeout
    match timeout(
        OPERATION_TIMEOUT,
        context.handle.write("echo hello\n".to_string()),
    )
    .await
    {
        Ok(Ok(_)) => {
            let output = wait_for_output(&context.handle, "hello")
                .await
                .expect("Failed to get output");
            assert!(output.contains("hello"));
        }
        Ok(Err(e)) => panic!("Write failed: {}", e),
        Err(_) => panic!("Write operation timed out"),
    }

    context.shutdown().await;
}

#[tokio::test]
async fn test_kill_process() {
    let context = TestContext::setup_pty_handle().await;

    // Kill with timeout
    match timeout(OPERATION_TIMEOUT, context.handle.force_kill(5000)).await {
        Ok(Ok(_)) => {
            // Wait for process termination
            match timeout(OPERATION_TIMEOUT, context.handle.waitpid(0)).await {
                Ok(Ok(pid)) => assert!(pid > 0),
                Ok(Err(e)) => panic!("Waitpid failed: {}", e),
                Err(_) => panic!("Waitpid timed out"),
            }
        }
        Ok(Err(e)) => panic!("Force kill failed: {}", e),
        Err(_) => panic!("Force kill operation timed out"),
    }

    context.shutdown().await;
}

#[tokio::test]
async fn test_set_env() {
    let context = TestContext::setup_pty_handle().await;

    // Set env var with timeout
    match timeout(
        OPERATION_TIMEOUT,
        context
            .handle
            .set_env("TEST_ENV_VAR".to_string(), "test_value".to_string()),
    )
    .await
    {
        Ok(Ok(_)) => {
            // Verify env var
            if let Ok(_) = context
                .handle
                .write("echo $TEST_ENV_VAR\n".to_string())
                .await
            {
                let output = wait_for_output(&context.handle, "test_value")
                    .await
                    .expect("Failed to get env var output");
                assert!(output.contains("test_value"));
            }
        }
        Ok(Err(e)) => panic!("Set env failed: {}", e),
        Err(_) => panic!("Set env operation timed out"),
    }

    context.shutdown().await;
}

#[tokio::test]
async fn test_execute_command() {
    let context = TestContext::setup_pty_handle().await;

    // Execute command with timeout
    match timeout(
        OPERATION_TIMEOUT,
        context.handle.execute("echo execute test\n".to_string()),
    )
    .await
    {
        Ok(Ok(_)) => {
            let output = wait_for_output(&context.handle, "execute test")
                .await
                .expect("Failed to get command output");
            assert!(output.contains("execute test"));
        }
        Ok(Err(e)) => panic!("Execute failed: {}", e),
        Err(_) => panic!("Execute operation timed out"),
    }

    context.shutdown().await;
}

#[tokio::test]
async fn test_multiplexer_basic() {
    let context = TestContext::setup_pty_handle().await;

    // Create sessions with timeout
    let session1 = match timeout(OPERATION_TIMEOUT, context.handle.create_session()).await {
        Ok(Ok(id)) => id,
        Ok(Err(e)) => panic!("Failed to create session 1: {}", e),
        Err(_) => panic!("Create session 1 timed out"),
    };

    let session2 = match timeout(OPERATION_TIMEOUT, context.handle.create_session()).await {
        Ok(Ok(id)) => id,
        Ok(Err(e)) => panic!("Failed to create session 2: {}", e),
        Err(_) => panic!("Create session 2 timed out"),
    };

    // Send data to sessions with timeout
    for (session_id, data) in &[
        (session1, "echo session1 data\n"),
        (session2, "echo session2 data\n"),
    ] {
        match timeout(
            OPERATION_TIMEOUT,
            context
                .handle
                .send_to_session(*session_id, Vec::from(data.as_bytes())),
        )
        .await
        {
            Ok(Ok(_)) => {
                let expected = if *session_id == session1 {
                    "session1 data"
                } else {
                    "session2 data"
                };
                let output = wait_for_output(&context.handle, expected)
                    .await
                    .expect("Failed to get session output");
                assert!(output.contains(expected));
            }
            Ok(Err(e)) => panic!("Send to session {} failed: {}", session_id, e),
            Err(_) => panic!("Send to session {} timed out", session_id),
        }
    }

    context.shutdown().await;
}

#[tokio::test]
async fn test_session_management() {
    let context = TestContext::setup_pty_handle().await;

    // Test creating multiple sessions
    let mut sessions = Vec::new();
    for i in 0..3 {
        match timeout(OPERATION_TIMEOUT, context.handle.create_session()).await {
            Ok(Ok(session_id)) => {
                sessions.push(session_id);
                info!("Created session {}: {}", i, session_id);
            }
            Ok(Err(e)) => panic!("Failed to create session {}: {}", i, e),
            Err(_) => panic!("Create session {} timed out", i),
        }
    }

    // Test listing sessions
    let active_sessions = timeout(OPERATION_TIMEOUT, async {
        // Wait a bit for sessions to be fully registered
        sleep(Duration::from_millis(100)).await;
        context.handle.list_sessions().await
    })
    .await
    .expect("List sessions timeout")
    .expect("Failed to list sessions");

    assert_eq!(active_sessions.len(), sessions.len());

    // Test removing sessions
    for session_id in sessions {
        match timeout(OPERATION_TIMEOUT, context.handle.remove_session(session_id)).await {
            Ok(Ok(_)) => info!("Removed session {}", session_id),
            Ok(Err(e)) => panic!("Failed to remove session {}: {}", session_id, e),
            Err(_) => panic!("Remove session {} timed out", session_id),
        }
    }

    context.shutdown().await;
}

#[tokio::test]
async fn test_session_data_handling() {
    let context = TestContext::setup_pty_handle().await;

    // Create a test session
    let session_id = timeout(OPERATION_TIMEOUT, context.handle.create_session())
        .await
        .expect("Create session timeout")
        .expect("Failed to create session");

    // Test data echo
    let test_data = "Hello, Session!";
    match timeout(
        OPERATION_TIMEOUT,
        context.handle.send_to_session(
            session_id,
            Vec::from(format!("echo {}\n", test_data).as_bytes()),
        ),
    )
    .await
    {
        Ok(Ok(_)) => {
            // Wait for the echo response
            let output = timeout(OPERATION_TIMEOUT, async {
                let mut attempts = 0;
                while attempts < 10 {
                    if let Ok(data) = context.handle.read_from_session(session_id).await {
                        if data.contains(test_data) {
                            return Some(data);
                        }
                    }
                    sleep(SLEEP_DURATION).await;
                    attempts += 1;
                }
                None
            })
            .await
            .expect("Read timeout")
            .expect("Failed to read session data");

            assert!(
                output.contains(test_data),
                "Expected output containing '{}', got: {}",
                test_data,
                output
            );
        }
        Ok(Err(e)) => panic!("Failed to send data to session: {}", e),
        Err(_) => panic!("Send data timeout"),
    }

    context.shutdown().await;
}

#[tokio::test]
async fn test_broadcast() {
    let context = TestContext::setup_pty_handle().await;

    // Create multiple sessions
    let session_count = 3;
    let mut sessions = Vec::new();
    for _ in 0..session_count {
        let session_id = timeout(OPERATION_TIMEOUT, context.handle.create_session())
            .await
            .expect("Create session timeout")
            .expect("Failed to create session");
        sessions.push(session_id);
    }

    // Broadcast message
    let broadcast_msg = "Broadcast Test Message";
    match timeout(
        OPERATION_TIMEOUT,
        context
            .handle
            .broadcast(Vec::from(format!("echo {}\n", broadcast_msg).as_bytes())),
    )
    .await
    {
        Ok(Ok(_)) => {
            // Verify broadcast received by all sessions
            for session_id in &sessions {
                let output = timeout(OPERATION_TIMEOUT, async {
                    let mut attempts = 0;
                    while attempts < 10 {
                        if let Ok(data) = context.handle.read_from_session(*session_id).await {
                            if data.contains(broadcast_msg) {
                                return Some(data);
                            }
                        }
                        sleep(SLEEP_DURATION).await;
                        attempts += 1;
                    }
                    None
                })
                .await
                .expect("Read timeout")
                .expect("Failed to read session data");

                assert!(
                    output.contains(broadcast_msg),
                    "Session {} did not receive broadcast",
                    session_id
                );
            }
        }
        Ok(Err(e)) => panic!("Broadcast failed: {}", e),
        Err(_) => panic!("Broadcast timeout"),
    }

    context.shutdown().await;
}

#[tokio::test]
async fn test_session_merge_split() {
    let context = TestContext::setup_pty_handle().await;

    // Create two sessions
    let session1 = timeout(OPERATION_TIMEOUT, context.handle.create_session())
        .await
        .expect("Create session timeout")
        .expect("Failed to create session 1");

    let session2 = timeout(OPERATION_TIMEOUT, context.handle.create_session())
        .await
        .expect("Create session timeout")
        .expect("Failed to create session 2");

    // Send unique data to each session
    for (session_id, msg) in &[(session1, "Session 1 Data"), (session2, "Session 2 Data")] {
        match timeout(
            OPERATION_TIMEOUT,
            context
                .handle
                .send_to_session(*session_id, Vec::from(format!("echo {}\n", msg).as_bytes())),
        )
        .await
        {
            Ok(Ok(_)) => {
                let output = wait_for_output(&context.handle, msg)
                    .await
                    .expect("Failed to get session output");
                assert!(output.contains(msg));
            }
            Ok(Err(e)) => panic!("Failed to send to session {}: {}", session_id, e),
            Err(_) => panic!("Send to session {} timed out", session_id),
        }
    }

    // Merge sessions
    match timeout(
        OPERATION_TIMEOUT,
        context.handle.merge_sessions(vec![session1, session2]),
    )
    .await
    {
        Ok(Ok(_)) => info!("Sessions merged successfully"),
        Ok(Err(e)) => panic!("Failed to merge sessions: {}", e),
        Err(_) => panic!("Merge sessions timeout"),
    }

    // Verify merged session
    let merged_output = timeout(
        OPERATION_TIMEOUT,
        context.handle.read_from_session(session1),
    )
    .await
    .expect("Read timeout")
    .expect("Failed to read merged session");

    assert!(
        merged_output.contains("Session 1 Data") && merged_output.contains("Session 2 Data"),
        "Merged session does not contain all data"
    );

    context.shutdown().await;
}

#[tokio::test]
async fn test_shell_operations() {
    let context = TestContext::setup_pty_handle().await;

    // Test shell environment
    let test_cmd = "echo $SHELL";
    match timeout(
        OPERATION_TIMEOUT,
        context.handle.write(format!("{}\n", test_cmd)),
    )
    .await
    {
        Ok(Ok(_)) => {
            let output = wait_for_output(&context.handle, "/bin")
                .await
                .expect("Failed to get shell output");
            assert!(
                output.contains("/bin/bash") || output.contains("/bin/sh"),
                "Unexpected shell: {}",
                output
            );
        }
        Ok(Err(e)) => panic!("Shell command failed: {}", e),
        Err(_) => panic!("Shell command timeout"),
    }

    // Test shell change
    match timeout(
        OPERATION_TIMEOUT,
        context.handle.change_shell("/bin/sh".to_string()),
    )
    .await
    {
        Ok(Ok(_)) => {
            // Verify shell change
            match timeout(
                OPERATION_TIMEOUT,
                context.handle.write(format!("{}\n", test_cmd)),
            )
            .await
            {
                Ok(Ok(_)) => {
                    let output = wait_for_output(&context.handle, "/bin/sh")
                        .await
                        .expect("Failed to get new shell output");
                    assert!(output.contains("/bin/sh"), "Shell not changed: {}", output);
                }
                Ok(Err(e)) => panic!("Shell verification failed: {}", e),
                Err(_) => panic!("Shell verification timeout"),
            }
        }
        Ok(Err(e)) => panic!("Change shell failed: {}", e),
        Err(_) => panic!("Change shell timeout"),
    }

    context.shutdown().await;
}

#[tokio::test]
async fn test_pty_cleanup() {
    let context = TestContext::setup_pty_handle().await;

    // Create some sessions and perform operations
    let session = timeout(OPERATION_TIMEOUT, context.handle.create_session())
        .await
        .expect("Create session timeout")
        .expect("Failed to create session");

    // Send some data
    match timeout(
        OPERATION_TIMEOUT,
        context
            .handle
            .send_to_session(session, Vec::from("echo cleanup test\n".as_bytes())),
    )
    .await
    {
        Ok(Ok(_)) => {
            let output = wait_for_output(&context.handle, "cleanup test")
                .await
                .expect("Failed to get output");
            assert!(output.contains("cleanup test"));
        }
        Ok(Err(e)) => panic!("Send data failed: {}", e),
        Err(_) => panic!("Send data timeout"),
    }

    // Test graceful shutdown
    match timeout(OPERATION_TIMEOUT, context.handle.close()).await {
        Ok(Ok(_)) => {
            // Verify cleanup
            match timeout(OPERATION_TIMEOUT, context.handle.status()).await {
                Ok(Ok(status)) => assert!(!status.contains("Running")),
                Ok(Err(_)) => {} // Expected error after close
                Err(_) => panic!("Status check timeout"),
            }
        }
        Ok(Err(e)) => panic!("Graceful shutdown failed: {}", e),
        Err(_) => panic!("Shutdown timeout"),
    }

    // Ensure proper cleanup
    context.shutdown().await;
}

#[tokio::test]
async fn test_concurrent_operations() {
    let context = TestContext::setup_pty_handle().await;

    // Create multiple sessions
    let mut sessions = Vec::new();
    for i in 0..3 {
        let session_id = timeout(OPERATION_TIMEOUT, context.handle.create_session())
            .await
            .expect("Create session timeout")
            .expect("Failed to create session");
        sessions.push((i, session_id));
    }

    // Perform concurrent operations
    let mut handles = Vec::new();
    for (i, session_id) in sessions {
        let handle_clone = Arc::clone(&context.handle);
        let handle = tokio::spawn(async move {
            let msg = format!("Concurrent message {}", i);
            match timeout(
                OPERATION_TIMEOUT,
                handle_clone.send_to_session(session_id, Vec::from(format!("echo {}\n", msg).as_bytes())),
            )
            .await
            {
                Ok(Ok(_)) => {
                    // Wait for output
                    let mut attempts = 0;
                    while attempts < 10 {
                        if let Ok(output) = handle_clone.read_from_session(session_id).await {
                            if output.contains(&msg) {
                                return Ok(());
                            }
                        }
                        sleep(SLEEP_DURATION).await;
                        attempts += 1;
                    }
                    Err(format!("Failed to get output for session {}", session_id))
                }
                Ok(Err(e)) => Err(format!("Failed to send to session {}: {}", session_id, e)),
                Err(_) => Err(format!("Operation timeout for session {}", session_id)),
            }
        });
        handles.push(handle);
    }

    // Wait for all operations to complete
    for handle in handles {
        match timeout(OPERATION_TIMEOUT, handle).await {
            Ok(Ok(result)) => {
                if let Err(e) = result {
                    panic!("Concurrent operation failed: {}", e);
                }
            }
            Ok(Err(e)) => panic!("Task panicked: {:?}", e),
            Err(_) => panic!("Task timeout"),
        }
    }

    context.shutdown().await;
}
