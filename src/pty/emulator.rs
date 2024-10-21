// src/pty/emulator.rs
use crate::pty::pty_renderer::PtyRenderer;
use crossterm::event::{Event as CrosstermEvent, KeyCode, KeyEvent, KeyModifiers};
use napi::bindgen_prelude::*;
use napi::{JsUnknown, NapiValue};
use napi_derive::napi;
use std::sync::{Arc, Mutex};
use vte::{Parser, Perform};

/// Trait defining how PTY output should be handled.
pub trait PtyOutputHandler: Perform + Send {
  /// Handles the output data received from the PTY process.
  fn handle_output(&mut self, data: &[u8]);
}

/// Wrapper for `PtyRenderer` to implement the `Perform` trait required by VTE parser.
pub struct PtyOutputHandlerWrapper {
  renderer: Arc<Mutex<PtyRenderer>>,
}

impl PtyOutputHandlerWrapper {
  /// Creates a new `PtyOutputHandlerWrapper` with the given `PtyRenderer`.
  pub fn new(renderer: Arc<Mutex<PtyRenderer>>) -> Self {
    PtyOutputHandlerWrapper { renderer }
  }
}

impl Perform for PtyOutputHandlerWrapper {
  fn print(&mut self, c: char) {
    let byte = c as u8;
    let mut renderer = self.renderer.lock().unwrap();
    renderer.handle_output(&[byte]);
  }

  // Implement other required methods from the Perform trait as no-ops.
  fn execute(&mut self, _byte: u8) {}
  fn hook(
    &mut self,
    _params: &vte::Params,
    _intermediates: &[u8],
    _ignore: bool,
    _some_char: char,
  ) {
  }
  fn put(&mut self, _byte: u8) {}
  fn unhook(&mut self) {}
  fn osc_dispatch(&mut self, _params: &[&[u8]], _bell_terminated: bool) {}
  fn csi_dispatch(
    &mut self,
    _params: &vte::Params,
    _intermediates: &[u8],
    _ignore: bool,
    _action: char,
  ) {
  }
  fn esc_dispatch(&mut self, _intermediates: &[u8], _ignore: bool, _byte: u8) {}
}

#[napi]
pub struct Emulator {
  /// The VTE parser for handling terminal escape sequences.
  parser: Parser,
  /// The PTY output handler (`PtyOutputHandlerWrapper`) wrapped in a thread-safe mutex.
  output_handler: Arc<Mutex<PtyOutputHandlerWrapper>>,
}

#[napi]
impl Emulator {
  #[napi(constructor)]
  pub fn new() -> Self {
    let renderer = Arc::new(Mutex::new(PtyRenderer::new()));
    Emulator {
      parser: Parser::new(),
      output_handler: Arc::new(Mutex::new(PtyOutputHandlerWrapper::new(renderer))),
    }
  }

  #[napi]
  pub fn process_input(&mut self, input: Buffer) -> Result<()> {
    for byte in input.as_ref() {
      self
        .parser
        .advance(&mut *self.output_handler.lock().unwrap(), *byte);
    }
    Ok(())
  }

  #[napi]
  pub fn handle_event(
    &mut self,
    env: Env,
    key_code: String,
    modifiers: u32,
    command_sender: Function<JsUnknown, JsUnknown>,
  ) -> Result<()> {
    let key_event = self.construct_key_event(key_code, modifiers)?;

    match key_event {
      CrosstermEvent::Key(key_event) => {
        let input_bytes = self.key_event_to_bytes(key_event);
        let data = input_bytes.clone();

        // Create a N-API buffer from the data.
        let buffer = Buffer::from(data.as_slice());

        // Call the JavaScript function with buffer as the argument.
        unsafe {
          let raw_env = env.raw();
          let napi_value = napi::bindgen_prelude::Buffer::to_napi_value(raw_env, buffer)?;
          let js_value = JsUnknown::from_raw(env.raw(), napi_value)?;
          command_sender.call(js_value)?;
        }
      }
      _ => {}
    }
    Ok(())
  }

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

  fn control_key_to_byte(&self, key_code: KeyCode) -> Option<u8> {
    match key_code {
      KeyCode::Char(c) => Some((c as u8) & 0x1F), // Ctrl+A = 1, Ctrl+B = 2, etc.
      _ => None,
    }
  }

  fn construct_key_event(&self, key_code: String, modifiers: u32) -> Result<CrosstermEvent> {
    let key = match key_code.as_str() {
      "Char" => KeyCode::Char('a'),
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
