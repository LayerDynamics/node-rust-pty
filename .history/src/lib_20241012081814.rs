info!("Dropping: Process {} terminated gracefully.", pty.pid);


#[napi]
pub async fn test_pty_handle_creation() -> Result<bool> {
    info!("Starting test_pty_handle_creation");
    let pty_handle = PtyHandle::new().await?;
    
    // Use the pty_handle to perform a simple operation
    pty_handle.resize(80, 24).await?;
    
    // Read from the PTY (this might block if there's no data, so we'll add a timeout)
    let read_result = tokio::time::timeout(Duration::from_secs(1), pty_handle.read()).await;
    match read_result {
        Ok(Ok(data)) => info!("Read from PTY: {}", data),
        Ok(Err(e)) => warn!("Error reading from PTY: {}", e),
        Err(_) => info!("No data read from PTY within timeout"),
    }

    // Write to the PTY
    pty_handle.write("echo Hello, PTY!\n".to_string()).await?;

    // Close the PTY
    pty_handle.close().await?;

    info!("PtyHandle created, used, and closed successfully");
    Ok(true)
}