// src/utils/ansi_parser.rs

use crate::pty::emulator::PtyOutputHandler;
use crate::virtual_dom::key_state::KeyState;
use crate::virtual_dom::state::State; // Ensure State is imported from the correct module
use crossterm::event::{self, Event as CrosstermEvent, KeyCode, KeyEvent, KeyModifiers};
use napi::{Env, JsObject, Result};
use napi_derive::napi;
use std::sync::Arc;
use vte::Perform;

/// Struct to parse ANSI escape sequences and handle PTY output.
pub struct ANSIParser {
  handler: Box<dyn PtyOutputHandler + Send>,
  key_state: Arc<KeyState>,
}

impl ANSIParser {
  /// Creates a new `ANSIParser` with the given PTY output handler and key state.
  ///
  /// # Parameters
  ///
  /// - `handler`: A boxed trait object implementing `PtyOutputHandler` and `Send`.
  /// - `key_state`: An `Arc`-wrapped `KeyState` instance.
  pub fn new(handler: Box<dyn PtyOutputHandler + Send>, key_state: Arc<KeyState>) -> Self {
    ANSIParser { handler, key_state }
  }
}

impl Perform for ANSIParser {
  fn print(&mut self, c: char) {
    let byte = c as u8;
    self.handler.handle_output(&[byte]);
  }

  fn execute(&mut self, byte: u8) {
    self.handler.handle_output(&[byte]);
  }

  fn csi_dispatch(
    &mut self,
    params: &vte::Params,
    intermediates: &[u8],
    _ignore: bool,
    command: char,
  ) {
    let mut sequence = vec![b'\x1b', b'['];
    for (i, param) in params.iter().enumerate() {
      if i > 0 {
        sequence.push(b';');
      }
      for p in param {
        // Convert each parameter from u16 to string bytes
        sequence.extend_from_slice(&p.to_string().as_bytes());
      }
    }
    if !intermediates.is_empty() {
      sequence.extend_from_slice(intermediates);
    }
    sequence.push(command as u8);
    self.handler.handle_output(&sequence);
  }

  fn osc_dispatch(&mut self, params: &[&[u8]], _bell_terminated: bool) {
    let mut sequence = vec![b'\x1b', b']'];
    for (i, param) in params.iter().enumerate() {
      if i > 0 {
        sequence.push(b';');
      }
      sequence.extend_from_slice(param);
    }
    sequence.push(b'\x07'); // BEL character
    self.handler.handle_output(&sequence);
  }

  fn hook(&mut self, params: &vte::Params, intermediates: &[u8], _ignore: bool, byte: char) {
    // Handle Device Control String (DCS) sequences if needed
    let mut sequence = vec![b'\x1b', b'P'];
    for (i, param) in params.iter().enumerate() {
      if i > 0 {
        sequence.push(b';');
      }
      for p in param {
        // Convert each parameter from u16 to string bytes
        sequence.extend_from_slice(&p.to_string().as_bytes());
      }
    }
    if !intermediates.is_empty() {
      sequence.extend_from_slice(intermediates);
    }
    sequence.push(byte as u8);
    self.handler.handle_output(&sequence);
  }

  fn put(&mut self, byte: u8) {
    self.handler.handle_output(&[byte]);
  }

  fn unhook(&mut self) {
    self.handler.handle_output(&[b'\x1b', b'\\']);
  }
}

/// Converts a `KeyEvent` into a sequence of bytes that can be sent to the PTY.
///
/// # Parameters
///
/// - `key_event`: The key event to convert.
///
/// # Returns
///
/// A `Vec<u8>` representing the byte sequence.
pub fn key_event_to_bytes(key_event: KeyEvent) -> Vec<u8> {
  let mut bytes = Vec::new();
  if key_event.modifiers.contains(KeyModifiers::CONTROL) {
    if let Some(b) = control_key_to_byte(key_event.code) {
      bytes.push(b);
    }
  } else if key_event.modifiers.contains(KeyModifiers::ALT) {
    bytes.push(0x1B); // ESC character for Alt modifier
    match key_event.code {
      KeyCode::Char(c) => bytes.push(c as u8),
      _ => {}
    }
  } else {
    match key_event.code {
      KeyCode::Char(c) => bytes.push(c as u8),
      KeyCode::Enter => bytes.push(b'\n'),
      KeyCode::Backspace => bytes.push(0x7F),
      KeyCode::Tab => bytes.push(b'\t'),
      KeyCode::Esc => bytes.push(b'\x1B'),
      KeyCode::Left => bytes.extend_from_slice(b"\x1B[D"),
      KeyCode::Right => bytes.extend_from_slice(b"\x1B[C"),
      KeyCode::Up => bytes.extend_from_slice(b"\x1B[A"),
      KeyCode::Down => bytes.extend_from_slice(b"\x1B[B"),
      KeyCode::Home => bytes.extend_from_slice(b"\x1B[H"),
      KeyCode::End => bytes.extend_from_slice(b"\x1B[F"),
      KeyCode::PageUp => bytes.extend_from_slice(b"\x1B[5~"),
      KeyCode::PageDown => bytes.extend_from_slice(b"\x1B[6~"),
      KeyCode::Delete => bytes.extend_from_slice(b"\x1B[3~"),
      KeyCode::Insert => bytes.extend_from_slice(b"\x1B[2~"),
      _ => {}
    }
  }
  bytes
}

/// Converts a `KeyCode` with CONTROL modifier to a single byte.
///
/// # Parameters
///
/// - `key_code`: The key code to convert.
///
/// # Returns
///
/// An `Option<u8>` containing the byte if applicable.
fn control_key_to_byte(key_code: KeyCode) -> Option<u8> {
  match key_code {
    KeyCode::Char(c) => Some((c as u8) & 0x1F), // Ctrl+A = 1, Ctrl+B = 2, etc.
    KeyCode::Enter => Some(0x0A),
    KeyCode::Backspace => Some(0x08),
    KeyCode::Tab => Some(0x09),
    KeyCode::Esc => Some(0x1B),
    _ => None,
  }
}

// #[cfg(test)]
// mod tests {
//     use super::*;
//     use crate::virtual_dom::key_state::KeyState;
//     use crate::pty::emulator::PtyOutputHandler;

//     struct MockHandler {
//         output: Vec<u8>,
//     }

//     impl MockHandler {
//         fn new() -> Self {
//             MockHandler { output: Vec::new() }
//         }
//     }

//     impl PtyOutputHandler for MockHandler {
//         fn handle_output(&mut self, data: &[u8]) {
//             self.output.extend_from_slice(data);
//         }
//     }

//     #[test]
//     fn test_ansi_parser_print() {
//         let mut handler = MockHandler::new();
//         let key_state = Arc::new(KeyState::new());
//         let mut parser = ANSIParser::new(handler, Arc::clone(&key_state));

//         parser.print('A');
//         assert_eq!(parser.handler.output, vec![b'A']);
//     }

//     #[test]
//     fn test_ansi_parser_csi_dispatch() {
//         let mut handler = MockHandler::new();
//         let key_state = Arc::new(KeyState::new());
//         let mut parser = ANSIParser::new(handler, Arc::clone(&key_state));

//         let params = vte::Params::default();
//         parser.csi_dispatch(&params, &[], false, 'C');
//         assert_eq!(parser.handler.output, vec![b'\x1b', b'[', b'C']);
//     }

//     #[test]
//     fn test_key_event_to_bytes_control_a() {
//         let key_event = KeyEvent::new(KeyCode::Char('a'), KeyModifiers::CONTROL);
//         let bytes = key_event_to_bytes(key_event);
//         assert_eq!(bytes, vec![1]); // Ctrl+A is 1
//     }

//     #[test]
//     fn test_key_event_to_bytes_alt_char() {
//         let key_event = KeyEvent::new(KeyCode::Char('b'), KeyModifiers::ALT);
//         let bytes = key_event_to_bytes(key_event);
//         assert_eq!(bytes, vec![0x1B, b'b']); // ESC + 'b'
//     }

//     #[test]
//     fn test_key_event_to_bytes_regular() {
//         let key_event = KeyEvent::new(KeyCode::Char('c'), KeyModifiers::NONE);
//         let bytes = key_event_to_bytes(key_event);
//         assert_eq!(bytes, vec![b'c']); // 'c' as u8
//     }

//     #[test]
//     fn test_key_event_to_bytes_enter() {
//         let key_event = KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE);
//         let bytes = key_event_to_bytes(key_event);
//         assert_eq!(bytes, vec![10]); // '\n' as u8
//     }
// }
