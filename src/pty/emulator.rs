// // src/pty/emulator.rs
// use crate::pty::pty_renderer::PtyRenderer;
// use crossterm::event::{Event as CrosstermEvent, KeyCode, KeyEvent, KeyModifiers};
// use napi::bindgen_prelude::*;
// use napi::{JsUnknown, NapiValue};
// use napi_derive::napi;
// use std::sync::{Arc, Mutex};
// use vte::{Parser, Perform};

// /// Trait defining how PTY output should be handled.
// pub trait PtyOutputHandler: Perform + Send {
//   /// Handles the output data received from the PTY process.
//   fn handle_output(&mut self, data: &[u8]);
// }

// /// Wrapper for `PtyRenderer` to implement the `Perform` trait required by VTE parser.
// pub struct PtyOutputHandlerWrapper {
//   renderer: Arc<Mutex<PtyRenderer>>,
// }

// impl PtyOutputHandlerWrapper {
//   /// Creates a new `PtyOutputHandlerWrapper` with the given `PtyRenderer`.
//   pub fn new(renderer: Arc<Mutex<PtyRenderer>>) -> Self {
//     PtyOutputHandlerWrapper { renderer }
//   }
// }

// impl Perform for PtyOutputHandlerWrapper {
//   fn print(&mut self, c: char) {
//     let byte = c as u8;
//     let mut renderer = self.renderer.lock().unwrap();
//     renderer.handle_output(&[byte]);
//   }

//   // Implement other required methods from the Perform trait as no-ops.
//   fn execute(&mut self, _byte: u8) {}
//   fn hook(
//     &mut self,
//     _params: &vte::Params,
//     _intermediates: &[u8],
//     _ignore: bool,
//     _some_char: char,
//   ) {
//   }
//   fn put(&mut self, _byte: u8) {}
//   fn unhook(&mut self) {}
//   fn osc_dispatch(&mut self, _params: &[&[u8]], _bell_terminated: bool) {}
//   fn csi_dispatch(
//     &mut self,
//     _params: &vte::Params,
//     _intermediates: &[u8],
//     _ignore: bool,
//     _action: char,
//   ) {
//   }
//   fn esc_dispatch(&mut self, _intermediates: &[u8], _ignore: bool, _byte: u8) {}
// }

// #[napi]
// pub struct Emulator {
//   /// The VTE parser for handling terminal escape sequences.
//   parser: Parser,
//   /// The PTY output handler (`PtyOutputHandlerWrapper`) wrapped in a thread-safe mutex.
//   output_handler: Arc<Mutex<PtyOutputHandlerWrapper>>,
// }

// #[napi]
// impl Emulator {
//   #[napi(constructor)]
//   pub fn new() -> Self {
//     let renderer = Arc::new(Mutex::new(PtyRenderer::new()));
//     Emulator {
//       parser: Parser::new(),
//       output_handler: Arc::new(Mutex::new(PtyOutputHandlerWrapper::new(renderer))),
//     }
//   }

//   #[napi]
//   pub fn process_input(&mut self, input: Buffer) -> Result<()> {
//     for byte in input.as_ref() {
//       self
//         .parser
//         .advance(&mut *self.output_handler.lock().unwrap(), *byte);
//     }
//     Ok(())
//   }

//   #[napi]
//   pub fn handle_event(
//     &mut self,
//     env: Env,
//     key_code: String,
//     modifiers: u32,
//     command_sender: Function<JsUnknown, JsUnknown>,
//   ) -> Result<()> {
//     let key_event = self.construct_key_event(key_code, modifiers)?;

//     match key_event {
//       CrosstermEvent::Key(key_event) => {
//         let input_bytes = self.key_event_to_bytes(key_event);
//         let data = input_bytes.clone();

//         // Create a N-API buffer from the data.
//         let buffer = Buffer::from(data.as_slice());

//         // Call the JavaScript function with buffer as the argument.
//         unsafe {
//           let raw_env = env.raw();
//           let napi_value = napi::bindgen_prelude::Buffer::to_napi_value(raw_env, buffer)?;
//           let js_value = JsUnknown::from_raw(env.raw(), napi_value)?;
//           command_sender.call(js_value)?;
//         }
//       }
//       _ => {}
//     }
//     Ok(())
//   }

//   fn key_event_to_bytes(&self, key_event: KeyEvent) -> Vec<u8> {
//     let mut bytes = Vec::new();
//     if key_event.modifiers.contains(KeyModifiers::CONTROL) {
//       if let Some(b) = self.control_key_to_byte(key_event.code) {
//         bytes.push(b);
//       }
//     } else {
//       match key_event.code {
//         KeyCode::Char(c) => bytes.push(c as u8),
//         KeyCode::Enter => bytes.push(b'\n'),
//         KeyCode::Backspace => bytes.push(b'\x7F'),
//         KeyCode::Tab => bytes.push(b'\t'),
//         KeyCode::Esc => bytes.push(b'\x1B'),
//         KeyCode::Left => bytes.extend_from_slice(b"\x1B[D"),
//         KeyCode::Right => bytes.extend_from_slice(b"\x1B[C"),
//         KeyCode::Up => bytes.extend_from_slice(b"\x1B[A"),
//         KeyCode::Down => bytes.extend_from_slice(b"\x1B[B"),
//         _ => {}
//       }
//     }
//     bytes
//   }

//   fn control_key_to_byte(&self, key_code: KeyCode) -> Option<u8> {
//     match key_code {
//       KeyCode::Char(c) => Some((c as u8) & 0x1F), // Ctrl+A = 1, Ctrl+B = 2, etc.
//       _ => None,
//     }
//   }

//   fn construct_key_event(&self, key_code: String, modifiers: u32) -> Result<CrosstermEvent> {
//     let key = match key_code.as_str() {
//       "Char" => KeyCode::Char('a'),
//       "Enter" => KeyCode::Enter,
//       "Backspace" => KeyCode::Backspace,
//       "Tab" => KeyCode::Tab,
//       "Esc" => KeyCode::Esc,
//       "Left" => KeyCode::Left,
//       "Right" => KeyCode::Right,
//       "Up" => KeyCode::Up,
//       "Down" => KeyCode::Down,
//       _ => return Err(Error::new(Status::InvalidArg, "Invalid key code")),
//     };

//     let modifiers = KeyModifiers::from_bits(modifiers as u8).unwrap_or(KeyModifiers::NONE);
//     let key_event = KeyEvent::new(key, modifiers);
//     Ok(CrosstermEvent::Key(key_event))
//   }
// }
// src/pty/emulator.rs

use crate::pty::pty_renderer::PtyRenderer;
use crate::pty::output_handler::PtyOutputHandler;
use crossterm::event::{Event as CrosstermEvent, KeyCode, KeyEvent, KeyModifiers};
use napi::bindgen_prelude::*;
use napi::{JsUnknown, Status};
use napi_derive::napi;
use napi::NapiValue;
use std::sync::{Arc, Mutex};
use vte::{Parser, Perform};

/// Wrapper for `PtyRenderer` to implement the `Perform` trait required by VTE parser.
///
/// This struct ensures that the PTY output is correctly rendered using the `PtyRenderer`.
pub struct PtyOutputHandlerWrapper {
  renderer: Arc<Mutex<PtyRenderer>>,
}

impl PtyOutputHandlerWrapper {
  /// Creates a new `PtyOutputHandlerWrapper` with the given `PtyRenderer`.
  ///
  /// # Parameters
  ///
  /// - `renderer`: An `Arc<Mutex<PtyRenderer>>` instance for rendering PTY output.
  pub fn new(renderer: Arc<Mutex<PtyRenderer>>) -> Self {
    PtyOutputHandlerWrapper { renderer }
  }
}

impl Perform for PtyOutputHandlerWrapper {
  /// Handles printable characters by passing them to the renderer.
  ///
  /// # Parameters
  ///
  /// - `c`: The character to print.
  fn print(&mut self, c: char) {
    let byte = c as u8;
    let mut renderer = self.renderer.lock().unwrap();
    renderer.handle_output(&[byte]);
  }

  /// Handles executed bytes (non-printable control characters).
  ///
  /// This implementation ignores execute bytes.
  fn execute(&mut self, _byte: u8) {}

  /// Handles hooks (extended sequences).
  ///
  /// # Parameters
  ///
  /// - `params`: Parameters associated with the hook.
  /// - `intermediates`: Intermediate bytes.
  /// - `ignore`: Whether to ignore the hook.
  /// - `action`: The action character.
  fn hook(&mut self, _params: &vte::Params, _intermediates: &[u8], _ignore: bool, _action: char) {}

  /// Handles puts (extended sequences).
  ///
  /// This implementation ignores put sequences.
  fn put(&mut self, _byte: u8) {}

  /// Handles unhooks (ending of extended sequences).
  ///
  /// This implementation ignores unhook sequences.
  fn unhook(&mut self) {}

  /// Handles OSC (Operating System Command) dispatches.
  ///
  /// This implementation ignores OSC sequences.
  fn osc_dispatch(&mut self, _params: &[&[u8]], _bell_terminated: bool) {}

  /// Handles CSI (Control Sequence Introducer) dispatches.
  ///
  /// # Parameters
  ///
  /// - `params`: Parameters for the CSI sequence.
  /// - `intermediates`: Intermediate bytes.
  /// - `ignore`: Whether to ignore the sequence.
  /// - `action`: The action character.
  fn csi_dispatch(
    &mut self,
    _params: &vte::Params,
    _intermediates: &[u8],
    _ignore: bool,
    _action: char,
  ) {
    // Implementation can be added if needed.
  }

  /// Handles ESC (Escape) dispatches.
  ///
  /// This implementation ignores ESC sequences.
  fn esc_dispatch(&mut self, _intermediates: &[u8], _ignore: bool, _byte: u8) {}
}

/// N-API exposed `Emulator` struct for handling PTY emulation.
///
/// The `Emulator` processes input data, parses terminal escape sequences, and manages PTY output.
#[napi]
pub struct Emulator {
  /// The VTE parser for handling terminal escape sequences.
  parser: Parser,
  /// The PTY output handler (`PtyOutputHandlerWrapper`) wrapped in a thread-safe mutex.
  output_handler: Arc<Mutex<PtyOutputHandlerWrapper>>,
}

#[napi]
impl Emulator {
  /// Creates a new `Emulator` instance.
  ///
  /// Initializes the VTE parser and the PTY output handler with a new `PtyRenderer`.
  ///
  /// # Example
  ///
  /// ```javascript
  /// const emulator = new Emulator();
  /// ```
  #[napi(constructor)]
  pub fn new() -> Self {
    let renderer = Arc::new(Mutex::new(PtyRenderer::new()));
    Emulator {
      parser: Parser::new(),
      output_handler: Arc::new(Mutex::new(PtyOutputHandlerWrapper::new(renderer))),
    }
  }

  /// Processes input data by feeding it into the VTE parser.
  ///
  /// This method takes a `Buffer` of input bytes, parses each byte, and handles the output accordingly.
  ///
  /// # Parameters
  ///
  /// - `input`: A `Buffer` containing raw input bytes from the PTY.
  ///
  /// # Example
  ///
  /// ```javascript
  /// emulator.processInput(Buffer.from('Hello, PTY!'));
  /// ```
  #[napi]
  pub fn process_input(&mut self, input: Buffer) -> Result<()> {
    for byte in input.as_ref() {
      self
        .parser
        .advance(&mut *self.output_handler.lock().unwrap(), *byte);
    }
    Ok(())
  }

  /// Handles keyboard events by converting them into PTY input bytes and sending them to the PTY process.
  ///
  /// # Parameters
  ///
  /// - `env`: The N-API environment.
  /// - `key_code`: A string representing the key code (e.g., "Char", "Enter").
  /// - `modifiers`: An unsigned integer representing key modifiers (e.g., Control).
  /// - `command_sender`: A JavaScript function to send the converted key event bytes.
  ///
  /// # Example
  ///
  /// ```javascript
  /// emulator.handleEvent(env, "Enter", 0, commandSenderFunction);
  /// ```
  #[napi]
  pub fn handle_event(
    &mut self,
    env: Env, // Env is now used in this implementation
    key_code: String,
    modifiers: u32,
    command_sender: JsFunction, // Corrected type without generics
  ) -> Result<()> {
    let key_event = self.construct_key_event(key_code, modifiers)?;

    match key_event {
      CrosstermEvent::Key(key_event) => {
        let input_bytes = self.key_event_to_bytes(key_event);
        let data = input_bytes.clone();

        // Create a N-API buffer from the data.
        let buffer = Buffer::from(data.as_slice());

        // Call the JavaScript function with buffer as the argument.
        // Properly calling JsFunction requires a valid N-API environment.
        // Here, we assume that `command_sender.call` is safe to use.
        unsafe {
            let napi_value = napi::bindgen_prelude::Buffer::to_napi_value(env.raw(), buffer)?;
            command_sender.call(None, &[JsUnknown::from_raw(env.raw(), napi_value)?])?;
        }
      }
      _ => {}
    }
    Ok(())
  }

  /// Converts a `KeyEvent` into a sequence of bytes suitable for PTY input.
  ///
  /// # Parameters
  ///
  /// - `key_event`: The `KeyEvent` to convert.
  ///
  /// # Returns
  ///
  /// A `Vec<u8>` containing the bytes representing the key event.
  fn key_event_to_bytes(&self, key_event: KeyEvent) -> Vec<u8> {
    let mut bytes = Vec::new();
    if key_event.modifiers.contains(KeyModifiers::CONTROL) {
      if let Some(b) = self.control_key_to_byte(key_event.code) {
        bytes.push(b);
      }
    } else {
      match key_event.code {
        KeyCode::Char(c) => bytes.push(c as u8),
        KeyCode::Enter => bytes.push(b'\n'),
        KeyCode::Backspace => bytes.push(b'\x7F'),
        KeyCode::Tab => bytes.push(b'\t'),
        KeyCode::Esc => bytes.push(b'\x1B'),
        KeyCode::Left => bytes.extend_from_slice(b"\x1B[D"),
        KeyCode::Right => bytes.extend_from_slice(b"\x1B[C"),
        KeyCode::Up => bytes.extend_from_slice(b"\x1B[A"),
        KeyCode::Down => bytes.extend_from_slice(b"\x1B[B"),
        _ => {}
      }
    }
    bytes
  }

  /// Converts a control key `KeyCode` into its corresponding byte value.
  ///
  /// # Parameters
  ///
  /// - `key_code`: The `KeyCode` to convert.
  ///
  /// # Returns
  ///
  /// An `Option<u8>` containing the byte value if applicable.
  fn control_key_to_byte(&self, key_code: KeyCode) -> Option<u8> {
    match key_code {
      KeyCode::Char(c) => Some((c as u8) & 0x1F), // Ctrl+A = 1, Ctrl+B = 2, etc.
      _ => None,
    }
  }

  /// Constructs a `CrosstermEvent` from a key code string and modifiers.
  ///
  /// # Parameters
  ///
  /// - `key_code`: A string representing the key code (e.g., "Char", "Enter").
  /// - `modifiers`: An unsigned integer representing key modifiers.
  ///
  /// # Returns
  ///
  /// A `Result<CrosstermEvent, napi::Error>` containing the constructed event or an error.
  fn construct_key_event(&self, key_code: String, modifiers: u32) -> Result<CrosstermEvent> {
    let key = match key_code.as_str() {
      "Char" => KeyCode::Char('a'), // Default to 'a' for simplicity
      "Enter" => KeyCode::Enter,
      "Backspace" => KeyCode::Backspace,
      "Tab" => KeyCode::Tab,
      "Esc" => KeyCode::Esc,
      "Left" => KeyCode::Left,
      "Right" => KeyCode::Right,
      "Up" => KeyCode::Up,
      "Down" => KeyCode::Down,
      _ => return Err(Error::new(Status::InvalidArg, "Invalid key code")),
    };

    let modifiers = KeyModifiers::from_bits(modifiers as u8).unwrap_or(KeyModifiers::NONE);
    let key_event = KeyEvent::new(key, modifiers);
    Ok(CrosstermEvent::Key(key_event))
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use napi::JsFunction;
  use std::sync::mpsc::{self, Receiver, Sender};

  /// Helper struct to mock the JavaScript `JsFunction`.
  ///
  /// Note: Properly mocking `JsFunction` requires a valid N-API environment, which isn't straightforward.
  /// Therefore, these tests are placeholders and demonstrate the intended structure.
  struct MockJsFunction {
    sender: Sender<Buffer>,
  }

  impl MockJsFunction {
    fn new() -> (Self, Receiver<Buffer>) {
      let (sender, receiver) = mpsc::channel();
      (MockJsFunction { sender }, receiver)
    }

    /// Simulates calling the JavaScript function by sending the buffer through the channel.
    fn call(&self, buffer: Buffer) -> Result<()> {
      self.sender.send(buffer).map_err(|_| {
        Error::new(
          Status::GenericFailure,
          "Failed to send buffer to mock function",
        )
      })
    }
  }

  // Since mocking `JsFunction` is complex and environment-specific, the following tests are commented out.
  // They serve as examples of how you might structure tests with a proper N-API environment.

  /*
  #[test]
  fn test_emulator_constructor() {
      let emulator = Emulator::new();
      // Ensure that the parser and output_handler are initialized.
      // Since they are private, we can test their presence indirectly by processing input.
      // For this test, we'll assume no panics occur during initialization.
  }

  #[test]
  fn test_process_input() {
      let mut emulator = Emulator::new();

      // Create a buffer with known input bytes.
      let input_str = "Hello, PTY!";
      let buffer = Buffer::from(input_str.as_bytes());

      // Process the input.
      emulator.process_input(buffer).expect("Failed to process input");

      // Since `PtyRenderer` updates internal state, you would typically verify the renderer's state.
      // However, without access to `PtyRenderer`'s internals, this test ensures no panic occurs.
  }

  #[test]
  fn test_handle_event() {
      let mut emulator = Emulator::new();

      // Create a mock JsFunction and receiver.
      let (mock_function, receiver) = MockJsFunction::new();

      // Attempting to create a `JsFunction` without a valid Env is unsafe and thus skipped.
      // let env = unsafe { Env::from_raw(std::ptr::null_mut()) };
      // let result = emulator.handle_event(env, "Enter".to_string(), 0, mock_function.as_js_function());
      // assert!(result.is_ok());

      // Verify that the mock function received the correct buffer.
      // let received_buffer = receiver.recv_timeout(Duration::from_secs(1)).expect("Did not receive buffer");
      // assert_eq!(received_buffer.as_ref(), b"\n"); // Enter key corresponds to newline.
  }

  #[test]
  fn test_handle_event_invalid_key_code() {
      let mut emulator = Emulator::new();

      // Create a mock JsFunction and receiver.
      let (_mock_function, _receiver) = MockJsFunction::new();

      // Attempt to handle an invalid key event.
      // This would require a valid Env and JsFunction, which isn't feasible here.
      // Therefore, this test is also skipped.
      // let env = unsafe { Env::from_raw(std::ptr::null_mut()) };
      // let result = emulator.handle_event(env, "InvalidKey".to_string(), 0, mock_function.as_js_function());
      // assert!(result.is_err());
  }

  #[test]
  fn test_handle_event_control_key() {
      let mut emulator = Emulator::new();

      // Create a mock JsFunction and receiver.
      let (mock_function, receiver) = MockJsFunction::new();

      // Attempt to handle a control key event like Ctrl+A.
      // This requires a valid Env and JsFunction, which isn't feasible here.
      // Therefore, this test is also skipped.
      // let env = unsafe { Env::from_raw(std::ptr::null_mut()) };
      // let result = emulator.handle_event(env, "Char".to_string(), KeyModifiers::CONTROL.bits() as u32, mock_function.as_js_function());
      // assert!(result.is_ok());

      // Verify that the mock function received the correct buffer (Ctrl+A = 0x01).
      // let received_buffer = receiver.recv_timeout(Duration::from_secs(1)).expect("Did not receive buffer");
      // assert_eq!(received_buffer.as_ref(), &[0x01]); // Ctrl+A
  }

  #[test]
  fn test_handle_event_non_key_event() {
      let mut emulator = Emulator::new();

      // Create a mock JsFunction and receiver.
      let (_mock_function, _receiver) = MockJsFunction::new();

      // Attempt to handle a non-key event like a mouse event.
      // Since `handle_event` expects a key_code and modifiers, handling non-key events isn't directly possible.
      // Therefore, this test ensures that invalid key codes do not send any data.
      // This would require a valid Env and JsFunction, which isn't feasible here.
      // let env = unsafe { Env::from_raw(std::ptr::null_mut()) };
      // let result = emulator.handle_event(env, "Mouse".to_string(), 0, mock_function.as_js_function());
      // assert!(result.is_err());
      // assert!(receiver.try_recv().is_err());
  }
  */

  /// Tests that `Emulator::new` initializes correctly without panicking.
  #[test]
  fn test_emulator_new() {
    let emulator = Emulator::new();
    // If no panic occurs, the test passes.
  }

  /// Tests the `process_input` method with a simple input.
  ///
  /// Note: Without access to `PtyRenderer`'s internal state, this test ensures no panic occurs.
  #[test]
  fn test_process_input_no_panic() {
    let mut emulator = Emulator::new();
    let input_str = "Test input";
    let buffer = Buffer::from(input_str.as_bytes());
    let result = emulator.process_input(buffer);
    assert!(result.is_ok(), "process_input should return Ok");
  }
}
