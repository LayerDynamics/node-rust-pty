#[napi]
pub async fn read(&self) -> Result<String> {
    let pty_lock = Arc::clone(&self.pty);
    info!("Initiating read from PTY.");
    let read_timeout = Duration::from_secs(5);

    let read_future = timeout(read_timeout, task::spawn_blocking(move || -> Result<String> {
        // ... existing read logic ...
    }));

    // Await the timeout and handle potential errors
    let result = match read_future.await {
        Ok(inner_result) => inner_result, // Result<String, napi::Error>
        Err(_) => {
            error!("Read operation timed out after {:?}", read_timeout);
            return Err(Error::new(
                Status::GenericFailure,
                "Read operation timed out.".to_string(),
            ));
        }
    };

    result
}
