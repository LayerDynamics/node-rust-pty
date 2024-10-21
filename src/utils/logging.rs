// // src/utils/logging.rs

// use env_logger::{Builder, Env};
// use log::LevelFilter;
// use std::sync::Once;

// /// Ensures that logging is initialized only once
// static INIT: Once = Once::new();

// /// Initializes logging using `env_logger`.
// /// Ensures that logging is initialized only once per application run.
// pub fn initialize_logging() {
//   INIT.call_once(|| {
//     let env = Env::default().filter_or("MY_LOG_LEVEL", "info");

//     Builder::from_env(env)
//       .filter_level(LevelFilter::Info) // Default log level is info, can be changed by env variable
//       .init();

//     log::debug!("Logging initialized");
//   });
// }
// src/utils/logging.rs

// src/utils/logging.rs

use env_logger::{Builder, Env};
use log::LevelFilter;

/// Initializes logging using `env_logger`.
/// Ensures that logging is initialized only once per application run.
/// Uses `try_init` to avoid panics if the logger is already initialized.
pub fn initialize_logging() {
  let env = Env::default().filter_or("MY_LOG_LEVEL", "info");
  if let Err(e) = Builder::from_env(env)
    .filter_level(LevelFilter::Info) // Default log level is info; can be overridden by env variable
    .try_init()
  {
    // Logger is already initialized; log the information and proceed.
    eprintln!("Logger already initialized: {}", e);
  }
  log::debug!("Logging initialized");
}
