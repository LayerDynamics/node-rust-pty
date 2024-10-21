use criterion::{criterion_group, criterion_main, Criterion};
use node_rust_pty::PtyHandle; // Import your crate's PtyHandle
use std::time::Duration;
use tokio::runtime::Runtime;

// Function to create and interact with PtyHandle
async fn create_pty_handle() -> Result<(), Box<dyn std::error::Error>> {
  let pty_handle = PtyHandle::new("/path/to/pty").expect("Failed to create PtyHandle");
  let data = "benchmarking data";
  pty_handle
    .write(data.as_bytes())
    .await
    .expect("Failed to write data");
  let _output = pty_handle.read().await.expect("Failed to read data");
  pty_handle.close().await.expect("Failed to close PtyHandle");
  Ok(())
}

// Benchmark function
fn benchmark_pty_creation(c: &mut Criterion) {
  // Create a Tokio runtime to handle async execution
  let rt = Runtime::new().unwrap();

  c.bench_function("PtyHandle create and basic operations", |b| {
    b.iter(|| {
      // Run the async code inside the runtime's context
      rt.block_on(async {
        create_pty_handle().await.expect("Benchmark failed");
      });
    });
  });
}

// Criterion group configuration
criterion_group! {
    name = benches;
    config = Criterion::default().measurement_time(Duration::from_secs(10));
    targets = benchmark_pty_creation
}

criterion_main!(benches);
