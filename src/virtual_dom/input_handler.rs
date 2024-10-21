// src/virtual_dom/input_handler.rs
use crate::virtual_dom::diff::Patch;
use crate::virtual_dom::renderer::Renderer;
use crate::virtual_dom::state::State;
use crate::virtual_dom::{VElement, VNode};
use crossterm::event::{self, Event as CrosstermEvent, KeyCode, KeyEvent, KeyModifiers};
use napi::{Env, JsObject, Result};
use napi_derive::napi;
use std::sync::{Arc, Mutex};
use vte::Perform;

/// Trait to handle PTY output.
pub trait PtyOutputHandler {
  fn handle_output(&mut self, data: &[u8]);
}

/// Struct to parse ANSI escape sequences and handle PTY output.
pub struct AnsiParser<'a> {
  handler: &'a mut dyn PtyOutputHandler,
}

impl<'a> AnsiParser<'a> {
  pub fn new(handler: &'a mut dyn PtyOutputHandler) -> Self {
    AnsiParser { handler }
  }
}

impl<'a> Perform for AnsiParser<'a> {
  fn print(&mut self, c: char) {
    let byte = c as u8;
    self.handler.handle_output(&[byte]);
  }

  fn execute(&mut self, _byte: u8) {}

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
        sequence.extend_from_slice(p.to_string().as_bytes());
      }
    }
    if !intermediates.is_empty() {
      sequence.extend_from_slice(intermediates);
    }
    sequence.push(command as u8);
    self.handler.handle_output(&sequence);
  }

  fn osc_dispatch(&mut self, _params: &[&[u8]], _bell_terminated: bool) {}

  fn hook(&mut self, _params: &vte::Params, _intermediates: &[u8], _ignore: bool, _byte: char) {}

  fn put(&mut self, _byte: u8) {}

  fn unhook(&mut self) {}
}

#[napi]
pub fn handle_input(env: Env) -> Result<Option<JsObject>> {
  match event::poll(std::time::Duration::from_millis(100)) {
    Ok(true) => match event::read() {
      Ok(CrosstermEvent::Key(key_event)) => {
        let js_key_event = key_event_to_js_object(env, key_event)?;
        Ok(Some(js_key_event))
      }
      Ok(_) => Ok(None),
      Err(e) => Err(napi::Error::from_reason(format!(
        "Error reading event: {}",
        e
      ))),
    },
    Ok(false) => Ok(None),
    Err(e) => Err(napi::Error::from_reason(format!(
      "Error polling events: {}",
      e
    ))),
  }
}

fn key_event_to_js_object(env: Env, key_event: KeyEvent) -> Result<JsObject> {
  let mut js_object = env.create_object()?;
  js_object.set("code", key_event.code.to_string())?;
  js_object.set("modifiers", format!("{:?}", key_event.modifiers))?;
  Ok(js_object)
}

pub fn key_event_to_bytes(key_event: KeyEvent) -> Vec<u8> {
  let mut bytes = Vec::new();
  if key_event.modifiers.contains(KeyModifiers::CONTROL) {
    if let Some(b) = control_key_to_byte(key_event.code) {
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

fn control_key_to_byte(key_code: KeyCode) -> Option<u8> {
  match key_code {
    KeyCode::Char(c) => Some((c as u8) & 0x1F),
    _ => None,
  }
}

/// Processes a key event and updates the state accordingly.
///
/// # Parameters
///
/// - `_key_event`: The key event to process.
/// - `_state`: The current application state.
///
/// # Note
///
/// This is a placeholder for implementing specific key event processing logic.
/// You can modify the state or trigger actions based on the key event.
pub fn process_key_event(_key_event: KeyEvent, _state: &mut State) {
  // Placeholder for process_key_event logic
}

#[cfg(test)]
mod tests {
  use super::*;

  struct MockOutputHandler {
    pub outputs: Vec<Vec<u8>>,
  }

  impl MockOutputHandler {
    fn new() -> Self {
      MockOutputHandler {
        outputs: Vec::new(),
      }
    }
  }

  // Implement the PtyOutputHandler trait for MockOutputHandler
  impl PtyOutputHandler for MockOutputHandler {
    fn handle_output(&mut self, data: &[u8]) {
      self.outputs.push(data.to_vec());
    }
  }

  #[test]
  fn test_ansi_parser_print() {
    let mut handler = MockOutputHandler::new();
    let mut parser = AnsiParser::new(&mut handler);
    parser.print('A');
    assert_eq!(handler.outputs.len(), 1);
    assert_eq!(handler.outputs[0], vec![b'A']);
  }

  #[test]
  fn test_ansi_parser_csi_dispatch() {
    let mut handler = MockOutputHandler::new();
    let mut parser = AnsiParser::new(&mut handler);
    let params = vte::Params::default();
    parser.csi_dispatch(&params, &[], false, 'C');
    assert_eq!(handler.outputs.len(), 1);
    assert_eq!(handler.outputs[0], vec![b'\x1b', b'[', b'C']);
  }

  #[test]
  fn test_key_event_to_bytes_control() {
    let key_event = KeyEvent::new(KeyCode::Char('A'), KeyModifiers::CONTROL);
    let bytes = key_event_to_bytes(key_event);
    assert_eq!(bytes, vec![1]);
  }

  #[test]
  fn test_key_event_to_bytes_regular() {
    let key_event = KeyEvent::new(KeyCode::Char('a'), KeyModifiers::NONE);
    let bytes = key_event_to_bytes(key_event);
    assert_eq!(bytes, vec![b'a']);
  }
}
