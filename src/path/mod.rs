// src/path/mod.rs

use dirs_next::home_dir;
use log::{debug, error};
use napi::bindgen_prelude::*;
use napi::{Error, Result, Status};
use napi_derive::napi;
use regex::Regex;
use std::env;
use std::env::temp_dir;
use std::path::{Path, PathBuf};

/// Retrieves the current user's home directory.
///
/// # Returns
///
/// - `String`: The path to the home directory.
///
/// # Example
///
/// ```javascript
/// const path = require('your-module');
///
/// path.getHomeDir()
///   .then(home => {
///     console.log('Home Directory:', home);
///   })
///   .catch(err => {
///     console.error(err);
///   });
/// ```
#[napi]
pub async fn get_home_dir() -> Result<String> {
  match home_dir() {
    Some(dir) => {
      debug!("Home directory found: {:?}", dir);
      Ok(dir.to_string_lossy().into_owned())
    }
    None => {
      error!("Failed to retrieve the home directory.");
      Err(Error::new(
        Status::GenericFailure,
        "Home directory not found.".to_string(),
      ))
    }
  }
}

/// Retrieves the system's temporary directory.
///
/// # Returns
///
/// - `String`: The path to the temporary directory.
///
/// # Example
///
/// ```javascript
/// const path = require('your-module');
///
/// path.getTempDir()
///   .then(temp => {
///     console.log('Temporary Directory:', temp);
///   })
///   .catch(err => {
///     console.error(err);
///   });
/// ```
#[napi]
pub async fn get_temp_dir() -> Result<String> {
  let temp = temp_dir();
  if temp.exists() {
    debug!("Temporary directory found: {:?}", temp);
    Ok(temp.to_string_lossy().into_owned())
  } else {
    error!("Temporary directory does not exist: {:?}", temp);
    Err(Error::new(
      Status::GenericFailure,
      "Temporary directory not found.".to_string(),
    ))
  }
}

/// Joins multiple path segments into a single path string.
///
/// # Arguments
///
/// * `segments` - An array of path segments.
///
/// # Returns
///
/// A string representing the joined path.
///
/// # Example
///
/// ```javascript
/// const path = require('your-module');
///
/// path.joinPaths(['/usr', 'local', 'bin'])
///   .then(fullPath => {
///     console.log(fullPath); // Outputs: "/usr/local/bin"
///   })
///   .catch(err => {
///     console.error(err);
///   });
/// ```
#[napi]
pub async fn join_paths(segments: Vec<String>) -> Result<String> {
  let mut path = PathBuf::new();
  for segment in segments {
    path.push(segment);
  }
  debug!("Joined path: {:?}", path);
  Ok(path.to_string_lossy().into_owned())
}

/// Converts a relative path to an absolute path.
///
/// # Arguments
///
/// * `path_str` - A string representing the path to be converted.
///
/// # Returns
///
/// The absolute path as a string.
///
/// # Example
///
/// ```javascript
/// const path = require('your-module');
/// const { Path } = require('path'); // Assuming you have a Path class or similar
///
/// path.absolutePath('./some/relative/path')
///   .then(absolute => {
///     console.log('Absolute Path:', absolute);
///   })
///   .catch(err => {
///     console.error(err);
///   });
/// ```
#[napi]
pub async fn absolute_path(path_str: String) -> Result<String> {
  let path = Path::new(&path_str);
  match path.canonicalize() {
    Ok(abs_path) => {
      debug!("Absolute path: {:?}", abs_path);
      Ok(abs_path.to_string_lossy().into_owned())
    }
    Err(e) => {
      error!("Failed to convert to absolute path: {}", e);
      Err(Error::from_reason(format!(
        "Failed to convert to absolute path: {}",
        e
      )))
    }
  }
}

/// Checks if a given path exists.
///
/// # Arguments
///
/// * `path_str` - A string representing the path to be checked.
///
/// # Returns
///
/// - `Boolean`: `true` if the path exists, `false` otherwise.
///
/// # Example
///
/// ```javascript
/// const path = require('your-module');
///
/// path.pathExists('/usr/bin')
///   .then(exists => {
///     console.log('Path exists:', exists);
///   })
///   .catch(err => {
///     console.error(err);
///   });
/// ```
#[napi]
pub async fn path_exists(path_str: String) -> Result<bool> {
  let path = Path::new(&path_str);
  let exists = path.exists();
  debug!("Path exists check for {:?}: {}", path, exists);
  Ok(exists)
}

/// Checks if a given path is a file.
///
/// # Arguments
///
/// * `path_str` - A string representing the path to be checked.
///
/// # Returns
///
/// - `Boolean`: `true` if the path is a file, `false` otherwise.
///
/// # Example
///
/// ```javascript
/// const path = require('your-module');
///
/// path.isFile('/usr/bin/bash')
///   .then(isFile => {
///     console.log('Is file:', isFile);
///   })
///   .catch(err => {
///     console.error(err);
///   });
/// ```
#[napi]
pub async fn is_file(path_str: String) -> Result<bool> {
  let path = Path::new(&path_str);
  let is_file = path.is_file();
  debug!("Is file check for {:?}: {}", path, is_file);
  Ok(is_file)
}

/// Checks if a given path is a directory.
///
/// # Arguments
///
/// * `path_str` - A string representing the path to be checked.
///
/// # Returns
///
/// - `Boolean`: `true` if the path is a directory, `false` otherwise.
///
/// # Example
///
/// ```javascript
/// const path = require('your-module');
///
/// path.isDir('/usr/local')
///   .then(isDir => {
///     console.log('Is directory:', isDir);
///   })
///   .catch(err => {
///     console.error(err);
///   });
/// ```
#[napi]
pub async fn is_dir(path_str: String) -> Result<bool> {
  let path = Path::new(&path_str);
  let is_dir = path.is_dir();
  debug!("Is directory check for {:?}: {}", path, is_dir);
  Ok(is_dir)
}

/// Expands the given path by replacing environment variables and tilde with their actual values.
///
/// Supports tilde expansion (e.g., `~` to home directory) and environment variable expansion
/// using `${VAR}` syntax.
///
/// # Parameters
///
/// - `path`: The path string to expand.
///
/// # Returns
///
/// - `Ok(String)`: The expanded path as a `String`.
/// - `Err(napi::Error)`: If an environment variable is not set.
///
/// # Example
///
/// ```javascript
/// let expanded = path.expandPath("~/${USER}/documents").then(expandedPath => {
///   console.log(expandedPath);
/// }).catch(err => {
///   console.error(err);
/// });
/// ```
#[napi]
pub async fn expand_path_napi(path: String) -> Result<String> {
  let expanded = expand_path(path)?;
  Ok(expanded)
}

/// Internal function to expand paths.
///
/// Replaces tilde (~) with home directory and replaces `${VAR}` with environment variables.
///
/// # Arguments
///
/// * `path_str` - The path string to expand.
///
/// # Returns
///
/// * `Result<String>` - The expanded path or an error.
///
/// # Errors
///
/// Returns an error if an environment variable is referenced but not set.
pub fn expand_path(mut path_str: String) -> Result<String> {
  // Tilde expansion
  if path_str.starts_with('~') {
    if let Some(home) = home_dir() {
      path_str = path_str.replacen("~", &home.to_string_lossy(), 1);
      debug!("Tilde expanded path: {}", path_str);
    } else {
      error!("Home directory not found for tilde expansion.");
      return Err(Error::new(
        Status::GenericFailure,
        "Home directory not found for tilde expansion.".to_string(),
      ));
    }
  }

  // Environment variable expansion using ${VAR}
  let re = Regex::new(r"\$\{(\w+)\}").map_err(|e| {
    Error::from_reason(format!(
      "Failed to compile regex for environment variable expansion: {}",
      e
    ))
  })?;

  // Collect all matches first to avoid multiple mutable borrows
  let captures: Vec<(String, String)> = re
    .captures_iter(&path_str)
    .filter_map(|caps| {
      if caps.len() == 2 {
        let var = caps.get(1)?.as_str().to_string();
        match env::var(&var) {
          Ok(val) => Some((caps.get(0).unwrap().as_str().to_string(), val)),
          Err(_) => None,
        }
      } else {
        None
      }
    })
    .collect();

  for (full_match, var_value) in captures {
    if let Some(pos) = path_str.find(&full_match) {
      path_str.replace_range(pos..pos + full_match.len(), &var_value);
      debug!("Replaced {} with {}", full_match, var_value);
    }
  }

  // After replacements, check if any ${VAR} remain
  if re.is_match(&path_str) {
    error!("Some environment variables were not set.");
    return Err(Error::new(
      Status::GenericFailure,
      "Some environment variables were not set.".to_string(),
    ));
  }

  debug!("Expanded path: {}", path_str);
  Ok(path_str)
}

/// Retrieves the default shell path based on the operating system.
///
/// # Returns
///
/// The path to the default shell as a string.
///
/// # Example
///
/// ```javascript
/// const path = require('your-module');
///
/// path.getDefaultShell()
///   .then(shell => {
///     console.log('Default Shell:', shell);
///   })
///   .catch(err => {
///     console.error(err);
///   });
/// ```
#[napi]
pub async fn get_default_shell() -> Result<String> {
  #[cfg(target_os = "linux")]
  {
    get_default_shell_linux().await
  }
  #[cfg(target_os = "macos")]
  {
    get_default_shell_macos().await
  }
  #[cfg(target_os = "windows")]
  {
    get_default_shell_windows().await
  }
  #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
  {
    Err(Error::new(
      Status::InvalidArg,
      "Unsupported operating system for getting default shell.".to_string(),
    ))
  }
}

#[cfg(target_os = "linux")]
async fn get_default_shell_linux() -> Result<String> {
  match env::var("SHELL") {
    Ok(shell) => {
      let path = PathBuf::from(&shell);
      if is_file(shell.clone()).await? {
        debug!("Default shell on Linux: {:?}", path);
        Ok(shell)
      } else {
        error!(
          "SHELL environment variable points to a non-file: {:?}",
          path
        );
        Err(Error::from_reason(
          "SHELL environment variable points to a non-file.".to_string(),
        ))
      }
    }
    Err(_) => {
      // Fallback to /bin/bash
      let fallback = "/bin/bash".to_string();
      if is_file(fallback.clone()).await? {
        debug!("Fallback default shell on Linux: {:?}", fallback);
        Ok(fallback)
      } else {
        error!("Fallback shell /bin/bash does not exist.");
        Err(Error::from_reason(
          "Fallback shell /bin/bash does not exist.".to_string(),
        ))
      }
    }
  }
}

#[cfg(target_os = "macos")]
async fn get_default_shell_macos() -> Result<String> {
  match env::var("SHELL") {
    Ok(shell) => {
      let path = PathBuf::from(&shell);
      if is_file(shell.clone()).await? {
        debug!("Default shell on macOS: {:?}", path);
        Ok(shell)
      } else {
        error!(
          "SHELL environment variable points to a non-file: {:?}",
          path
        );
        Err(Error::from_reason(
          "SHELL environment variable points to a non-file.".to_string(),
        ))
      }
    }
    Err(_) => {
      // Fallback to /bin/bash
      let fallback = "/bin/bash".to_string();
      if is_file(fallback.clone()).await? {
        debug!("Fallback default shell on macOS: {:?}", fallback);
        Ok(fallback)
      } else {
        error!("Fallback shell /bin/bash does not exist.");
        Err(Error::from_reason(
          "Fallback shell /bin/bash does not exist.".to_string(),
        ))
      }
    }
  }
}

#[cfg(target_os = "windows")]
async fn get_default_shell_windows() -> Result<String> {
  // Attempt to retrieve the COMSPEC environment variable (typically CMD)
  match env::var("COMSPEC") {
    Ok(cmd) => {
      let path = cmd.clone();
      if is_file(path.clone()).await? {
        debug!("Default shell on Windows (COMSPEC): {:?}", path);
        Ok(cmd)
      } else {
        error!(
          "COMSPEC environment variable points to a non-file: {:?}",
          path
        );
        Err(Error::from_reason(
          "COMSPEC environment variable points to a non-file.".to_string(),
        ))
      }
    }
    Err(_) => {
      // Fallback to PowerShell
      let fallback = r"C:\Windows\System32\WindowsPowerShell\v1.0\powershell.exe".to_string();
      if is_file(fallback.clone()).await? {
        debug!("Fallback default shell on Windows: {:?}", fallback);
        Ok(fallback)
      } else {
        error!("Fallback shell PowerShell does not exist.");
        Err(Error::from_reason(
          "Fallback shell PowerShell does not exist.".to_string(),
        ))
      }
    }
  }
}

/// Constructs the PTY device path based on the operating system.
///
/// # Returns
///
/// The PTY device path as a string.
///
/// # Example
///
/// ```javascript
/// const path = require('your-module');
///
/// path.getPtyDevicePath()
///   .then(ptyPath => {
///     console.log('PTY Device Path:', ptyPath);
///   })
///   .catch(err => {
///     console.error(err);
///   });
/// ```
#[napi]
pub async fn get_pty_device_path() -> Result<String> {
  #[cfg(target_os = "linux")]
  {
    get_pty_device_path_linux().await
  }
  #[cfg(target_os = "macos")]
  {
    get_pty_device_path_macos().await
  }
  #[cfg(target_os = "windows")]
  {
    get_pty_device_path_windows().await
  }
  #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
  {
    Err(Error::new(
      Status::InvalidArg,
      "Unsupported operating system for PTY device path.".to_string(),
    ))
  }
}

#[cfg(target_os = "linux")]
async fn get_pty_device_path_linux() -> Result<String> {
  // PTY devices are usually accessed via /dev/pts/<number>
  // Since the PTY number is dynamic, returning the base PTY path.
  let base = "/dev/pts".to_string();
  if path_exists(base.clone()).await? {
    debug!("PTY base directory on Linux: {:?}", base);
    Ok(base)
  } else {
    error!("PTY base directory does not exist: {:?}", base);
    Err(Error::from_reason(
      "PTY base directory does not exist.".to_string(),
    ))
  }
}

#[cfg(target_os = "macos")]
async fn get_pty_device_path_macos() -> Result<String> {
  // PTY devices are located in /dev
  let base = "/dev".to_string();
  if path_exists(base.clone()).await? {
    debug!("PTY base directory on macOS: {:?}", base);
    Ok(base)
  } else {
    error!("PTY base directory does not exist: {:?}", base);
    Err(Error::from_reason(
      "PTY base directory does not exist.".to_string(),
    ))
  }
}

#[cfg(target_os = "windows")]
async fn get_pty_device_path_windows() -> Result<String> {
  // Placeholder: Implement ConPTY or other pseudoconsole mechanisms as needed.
  // For demonstration, returning the COMSPEC path.
  match env::var("COMSPEC") {
    Ok(cmd) => {
      let path = cmd.clone();
      if is_file(path.clone()).await? {
        debug!("PTY equivalent on Windows (COMSPEC): {:?}", path);
        Ok(path)
      } else {
        error!(
          "COMSPEC environment variable points to a non-file: {:?}",
          path
        );
        Err(Error::from_reason(
          "COMSPEC environment variable points to a non-file.".to_string(),
        ))
      }
    }
    Err(_) => {
      error!("COMSPEC environment variable not set.");
      Err(Error::from_reason(
        "COMSPEC environment variable not set.".to_string(),
      ))
    }
  }
}

/// Retrieves the expanded PTY device path based on the operating system.
///
/// This function is useful for testing or advanced configurations.
///
/// # Returns
///
/// The expanded PTY device path as a string.
///
/// # Example
///
/// ```javascript
/// const path = require('your-module');
///
/// path.getExpandedPtyDevicePath()
///   .then(expandedPtyPath => {
///     console.log('Expanded PTY Device Path:', expandedPtyPath);
///   })
///   .catch(err => {
///     console.error(err);
///   });
/// ```
#[napi]
pub async fn get_expanded_pty_device_path() -> Result<String> {
  // Example function to demonstrate additional exposed functionality.
  // You can implement specific logic as needed.
  get_pty_device_path().await
}

#[cfg(test)]
mod tests {
  use super::*;
  use std::env;

  #[tokio::test]
  async fn test_get_home_dir() {
    let home = get_home_dir().await.expect("Failed to get home directory");
    assert!(!home.is_empty());
  }

  #[tokio::test]
  async fn test_get_temp_dir() {
    let temp = get_temp_dir().await.expect("Failed to get temp directory");
    assert!(!temp.is_empty());
  }

  #[tokio::test]
  async fn test_join_paths() {
    let segments = vec!["/usr".to_string(), "local".to_string(), "bin".to_string()];
    let joined = join_paths(segments).await.expect("Failed to join paths");
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    {
      assert_eq!(joined, "/usr/local/bin");
    }
    #[cfg(target_os = "windows")]
    {
      assert_eq!(joined, r"\usr\local\bin"); // Windows path separator
    }
  }

  #[tokio::test]
  async fn test_absolute_path() {
    let relative = "./";
    let absolute = absolute_path(relative.to_string())
      .await
      .expect("Failed to get absolute path");
    assert!(Path::new(&absolute).is_absolute());
  }

  #[tokio::test]
  async fn test_path_exists() {
    #[cfg(unix)]
    let path = "/";

    #[cfg(windows)]
    let path = "C:\\";

    assert!(path_exists(path.to_string()).await.unwrap_or(false));
  }

  #[tokio::test]
  async fn test_is_file() {
    let shell = get_default_shell()
      .await
      .expect("Failed to get default shell");
    assert!(is_file(shell).await.unwrap_or(false));
  }

  #[tokio::test]
  async fn test_is_dir() {
    #[cfg(unix)]
    let path = "/";

    #[cfg(windows)]
    let path = "C:\\";

    assert!(is_dir(path.to_string()).await.unwrap_or(false));
  }

  #[tokio::test]
  async fn test_expand_path() {
    // Set the USER or USERNAME environment variable depending on OS
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    {
      env::set_var("USER", "testuser");
      env::set_var("HOME", "/home/testuser"); // Ensure HOME is set for tilde expansion
    }

    #[cfg(target_os = "windows")]
    {
      env::set_var("USERNAME", "testuser");
      env::set_var("USERPROFILE", "C:\\Users\\testuser"); // Ensure USERPROFILE is set for tilde expansion
    }

    #[cfg(any(target_os = "linux", target_os = "macos"))]
    let input = "~/documents/${USER}";

    #[cfg(target_os = "windows")]
    let input = "~\\documents\\${USERNAME}";

    let expected = {
      #[cfg(any(target_os = "linux", target_os = "macos"))]
      {
        "/home/testuser/documents/testuser".to_string()
      }

      #[cfg(target_os = "windows")]
      {
        "C:\\Users\\testuser\\documents\\testuser".to_string()
      }
    };

    let result = expand_path(input.to_string()).expect("Failed to expand path");
    assert_eq!(result, expected);
  }

  #[tokio::test]
  async fn test_get_default_shell() {
    let shell = get_default_shell()
      .await
      .expect("Failed to get default shell");
    assert!(is_file(shell.clone()).await.unwrap_or(false));
  }

  #[tokio::test]
  async fn test_get_pty_device_path() {
    let pty_path = get_pty_device_path()
      .await
      .expect("Failed to get PTY device path");
    assert!(!pty_path.is_empty());
  }
}
