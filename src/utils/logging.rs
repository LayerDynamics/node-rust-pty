// src/utils/logging.rs

use env_logger::{Builder, Env};
use log::LevelFilter;
use std::sync::Once;

/// Ensures that logging is initialized only once
static INIT: Once = Once::new();

/// Initializes logging using `env_logger`.
/// Ensures that logging is initialized only once per application run.
pub fn initialize_logging() {
  INIT.call_once(|| {
    let env = Env::default().filter_or("MY_LOG_LEVEL", "info");

    Builder::from_env(env)
      .filter_level(LevelFilter::Info) // Default log level is info, can be changed by env variable
      .init();

    log::debug!("Logging initialized");
  });
}
