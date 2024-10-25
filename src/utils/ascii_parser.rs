// src/utils/ascii_parser.rs

use crate::pty::PtyOutputHandler;
use crate::virtual_dom::key_state::KeyState;
use crate::virtual_dom::state::State;
use crossterm::event::{self, Event as CrosstermEvent, KeyCode, KeyEvent, KeyModifiers};
use napi::{Env, JsObject, Result};
use napi_derive::napi;
use std::sync::Arc;

/// ASCII control characters
const ASCII_BEL: u8 = 0x07; // Bell
const ASCII_BS: u8 = 0x08; // Backspace
const ASCII_HT: u8 = 0x09; // Horizontal Tab
const ASCII_LF: u8 = 0x0A; // Line Feed
const ASCII_VT: u8 = 0x0B; // Vertical Tab
const ASCII_FF: u8 = 0x0C; // Form Feed
const ASCII_CR: u8 = 0x0D; // Carriage Return
const ASCII_SUB: u8 = 0x1A; // Substitute
const ASCII_ESC: u8 = 0x1B; // Escape
const ASCII_DEL: u8 = 0x7F; // Delete

/// Represents different types of ASCII text
///
/// # Examples
///
/// ```
/// use ascii_parser::ASCIIText;
///
/// let printable = ASCIIText::Print(vec![b'H', b'e', b'l', b'l', b'o']);
/// let control = ASCIIText::Control(0x07);
/// let whitespace = ASCIIText::Whitespace(' ');
/// let newline = ASCIIText::Newline(vec![b'\n']);
/// ```
#[derive(Debug, Clone, PartialEq)]
pub enum ASCIIText {
  /// Regular printable text
  Print(Vec<u8>),
  /// Control character
  Control(u8),
  /// Whitespace (space, tab)
  Whitespace(char),
  /// Newline characters (LF, CR, CRLF)
  Newline(Vec<u8>),
}

/// Struct to parse ASCII text and handle PTY output
pub struct ASCIIParser {
  handler: Box<dyn PtyOutputHandler + Send>,
  state: Option<Arc<State>>,
  current_line: String,
  line_buffer: Vec<String>,
  word_buffer: String,
  column_position: usize,
  tab_width: usize,
  key_state: Option<Arc<KeyState>>,
  bell_callback: Option<Box<dyn Fn() + Send>>,
}

impl ASCIIParser {
  /// Creates a new ASCIIParser with a PTY output handler
  pub fn new(handler: Box<dyn PtyOutputHandler + Send>) -> Self {
    ASCIIParser {
      handler,
      state: None,
      current_line: String::new(),
      line_buffer: Vec::new(),
      word_buffer: String::new(),
      column_position: 0,
      tab_width: 8, // Default tab width
      key_state: None,
      bell_callback: None,
    }
  }

  /// Creates a new ASCIIParser with state management
  pub fn with_state(
    handler: Box<dyn PtyOutputHandler + Send>,
    state: Arc<State>,
    key_state: Arc<KeyState>,
  ) -> Self {
    ASCIIParser {
      handler,
      state: Some(state),
      current_line: String::new(),
      line_buffer: Vec::new(),
      word_buffer: String::new(),
      column_position: 0,
      tab_width: 8,
      key_state: Some(key_state),
      bell_callback: None,
    }
  }

  /// Set a callback for bell characters
  pub fn set_bell_callback<F>(&mut self, callback: F)
  where
    F: Fn() + Send + 'static,
  {
    self.bell_callback = Some(Box::new(callback));
  }

  /// Set the tab width (number of spaces per tab)
  pub fn set_tab_width(&mut self, width: usize) {
    self.tab_width = width;
  }

  /// Processes a sequence of ASCII bytes
  pub fn process_bytes(&mut self, bytes: &[u8]) {
    for &byte in bytes {
      self.process_byte(byte);
    }
  }

  /// Process a single ASCII byte
  pub fn process_byte(&mut self, byte: u8) {
    match byte {
      // Control characters
      ASCII_BEL => self.handle_bell(),
      ASCII_BS => self.handle_backspace(),
      ASCII_HT => self.handle_tab(),
      ASCII_LF => self.handle_linefeed(),
      ASCII_VT => self.handle_vertical_tab(),
      ASCII_FF => self.handle_form_feed(),
      ASCII_CR => self.handle_carriage_return(),
      ASCII_SUB => self.handle_substitute(),
      ASCII_ESC => self.handle_escape(),
      ASCII_DEL => self.handle_delete(),

      // Printable ASCII characters
      0x20..=0x7E => self.handle_printable(byte),

      // Invalid or non-ASCII bytes
      _ => self.handle_invalid_byte(byte),
    }
  }

  // Handler methods for different ASCII categories

  fn handle_bell(&mut self) {
    if let Some(ref callback) = self.bell_callback {
      callback();
    }
    self.handler.handle_output(&[ASCII_BEL]);
  }

  fn handle_backspace(&mut self) {
    if self.column_position > 0 {
      self.column_position -= 1;
      if !self.current_line.is_empty() {
        self.current_line.pop();
      }
    }
    self.handler.handle_output(&[ASCII_BS]);
  }

  fn handle_tab(&mut self) {
    let spaces_to_add = self.tab_width - (self.column_position % self.tab_width);
    self.current_line.push_str(&" ".repeat(spaces_to_add));
    self.column_position += spaces_to_add;
    self.handler.handle_output(&[ASCII_HT]);
  }

  fn handle_linefeed(&mut self) {
    self.flush_current_line();
    // Removed the line that pushes an empty string to avoid extra empty lines
    self.column_position = 0;
    self.handler.handle_output(&[ASCII_LF]);
  }

  fn handle_vertical_tab(&mut self) {
    self.handle_linefeed(); // Treat VT as LF for simple terminals
  }

  fn handle_form_feed(&mut self) {
    self.flush_current_line();
    self.line_buffer.clear();
    self.column_position = 0;
    self.handler.handle_output(&[ASCII_FF]);
  }

  fn handle_carriage_return(&mut self) {
    self.column_position = 0;
    self.handler.handle_output(&[ASCII_CR]);
  }

  fn handle_substitute(&mut self) {
    // Replace with SUB character (^Z)
    self.current_line.push('^');
    self.current_line.push('Z');
    self.column_position += 2;
    self.handler.handle_output(&[ASCII_SUB]);
  }

  fn handle_escape(&mut self) {
    // Simple escape handling - just output the ESC character
    self.handler.handle_output(&[ASCII_ESC]);
  }

  fn handle_delete(&mut self) {
    if !self.current_line.is_empty() {
      self.current_line.pop();
      self.column_position = self.column_position.saturating_sub(1);
    }
    self.handler.handle_output(&[ASCII_DEL]);
  }

  fn handle_printable(&mut self, byte: u8) {
    if let Some(c) = char::from_u32(byte as u32) {
      self.current_line.push(c);
      self.column_position += 1;
      if c.is_whitespace() {
        self.flush_word_buffer();
      } else {
        self.word_buffer.push(c);
      }
    }
    self.handler.handle_output(&[byte]);
  }

  fn handle_invalid_byte(&mut self, byte: u8) {
    // Replace invalid bytes with a placeholder
    self.current_line.push('?');
    self.column_position += 1;
    // Still output the original byte to maintain raw data
    self.handler.handle_output(&[byte]);
  }

  fn flush_word_buffer(&mut self) {
    if !self.word_buffer.is_empty() {
      self.word_buffer.clear();
    }
  }

  fn flush_current_line(&mut self) {
    if !self.current_line.is_empty() {
      self.line_buffer.push(self.current_line.clone());
      self.current_line.clear();
      self.word_buffer.clear();
    }
  }

  /// Gets the current column position
  pub fn get_column(&self) -> usize {
    self.column_position
  }

  /// Gets the current line contents
  pub fn get_current_line(&self) -> &str {
    &self.current_line
  }

  /// Gets the entire buffer contents
  pub fn get_buffer(&self) -> Vec<String> {
    self.line_buffer.clone()
  }

  /// Gets the current word being built
  pub fn get_current_word(&self) -> &str {
    &self.word_buffer
  }

  /// Clears all buffers and resets position
  pub fn clear(&mut self) {
    self.line_buffer.clear();
    self.current_line.clear();
    self.word_buffer.clear();
    self.column_position = 0;
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use std::sync::Mutex;

  struct MockHandler {
    output: Arc<Mutex<Vec<u8>>>,
  }

  impl MockHandler {
    fn new() -> Self {
      MockHandler {
        output: Arc::new(Mutex::new(Vec::new())),
      }
    }
  }

  impl vte::Perform for MockHandler {
    fn print(&mut self, c: char) {
      self.handle_output(&[c as u8]);
    }

    fn execute(&mut self, byte: u8) {
      self.handle_output(&[byte]);
    }

    fn hook(&mut self, _params: &vte::Params, _intermediates: &[u8], _ignore: bool, _action: char) {
      let _ = _params;
    }

    fn put(&mut self, byte: u8) {
      self.handle_output(&[byte]);
    }

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

    fn esc_dispatch(&mut self, _intermediates: &[u8], _ignore: bool, _action: u8) {}
  }

  impl PtyOutputHandler for MockHandler {
    fn handle_output(&mut self, data: &[u8]) {
      self.output.lock().unwrap().extend_from_slice(data);
    }
  }

  #[test]
  fn test_ascii_parser_printable() {
    let handler = Box::new(MockHandler::new());
    let output = handler.output.clone();
    let mut parser = ASCIIParser::new(handler);

    parser.process_bytes(b"Hello World");

    let result = output.lock().unwrap();
    assert_eq!(&*result, b"Hello World");
    assert_eq!(parser.get_column(), 11);
  }

  #[test]
  fn test_ascii_parser_control_chars() {
    let handler = Box::new(MockHandler::new());
    let output = handler.output.clone();
    let mut parser = ASCIIParser::new(handler);

    parser.process_bytes(b"Line1\r\nLine2\n");

    let result = output.lock().unwrap();
    assert_eq!(&*result, b"Line1\r\nLine2\n");
    assert_eq!(
      parser.get_buffer(),
      vec!["Line1".to_string(), "Line2".to_string()]
    );
  }

  #[test]
  fn test_ascii_parser_tab_handling() {
    let handler = Box::new(MockHandler::new());
    let output = handler.output.clone();
    let mut parser = ASCIIParser::new(handler);
    parser.set_tab_width(4);

    parser.process_bytes(b"a\tb");

    let result = output.lock().unwrap();
    assert_eq!(&*result, b"a\tb");
    assert_eq!(parser.get_column(), 5); // 'a' + 3 spaces + 'b'
  }

  #[test]
  fn test_ascii_parser_word_buffer() {
    let handler = Box::new(MockHandler::new());
    let mut parser = ASCIIParser::new(handler);

    parser.process_bytes(b"Hello ");
    assert_eq!(parser.get_current_word(), "");

    parser.process_bytes(b"World");
    assert_eq!(parser.get_current_word(), "World");
  }

  #[test]
  fn test_ascii_parser_bell() {
    let handler = Box::new(MockHandler::new());
    let mut parser = ASCIIParser::new(handler);
    let bell_called = Arc::new(Mutex::new(false));
    let bell_called_clone = bell_called.clone();

    parser.set_bell_callback(move || {
      *bell_called_clone.lock().unwrap() = true;
    });

    parser.process_bytes(&[ASCII_BEL]);
    assert!(*bell_called.lock().unwrap());
  }

  #[test]
  fn test_ascii_parser_invalid_bytes() {
    let handler = Box::new(MockHandler::new());
    let mut parser = ASCIIParser::new(handler);

    parser.process_bytes(&[0x80]); // Invalid ASCII byte
    assert_eq!(parser.get_current_line(), "?");
  }

  #[test]
  fn test_ascii_parser_clear() {
    let handler = Box::new(MockHandler::new());
    let mut parser = ASCIIParser::new(handler);

    parser.process_bytes(b"Hello\nWorld");
    assert!(!parser.get_buffer().is_empty());

    parser.clear();
    assert!(parser.get_buffer().is_empty());
    assert_eq!(parser.get_column(), 0);
    assert!(parser.get_current_word().is_empty());
  }
}
