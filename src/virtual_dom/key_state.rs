// src/virtual_dom/key_state.rs
use crate::virtual_dom::state::State;
use crossterm::event::KeyEvent;
use napi::{Env, JsUnknown, Result};
use std::sync::Arc;

/// Manages key events and updates the application state accordingly.
pub struct KeyState {
  /// Reference to the application state.
  state: Arc<State>,
}

impl KeyState {
  /// Creates a new `KeyState` with a reference to the application `State`.
  pub fn new(state: Arc<State>) -> Self {
    KeyState { state }
  }

  /// Handles a key event by delegating to the `State`'s `handle_key_event` method.
  pub fn handle_key_event(&self, env: Env, key_event: KeyEvent) -> Result<()> {
    let js_key_event = self.convert_key_event(env, key_event)?;
    self.state.handle_key_event(env, js_key_event)
  }

  fn convert_key_event(&self, env: Env, key_event: KeyEvent) -> Result<JsUnknown> {
    let mut js_object = env.create_object()?;

    let key_code = match key_event.code {
      crossterm::event::KeyCode::Char(c) => env.create_string_from_std(c.to_string())?,
      crossterm::event::KeyCode::Enter => env.create_string_from_std("Enter".to_string())?,
      crossterm::event::KeyCode::Esc => env.create_string_from_std("Esc".to_string())?,
      crossterm::event::KeyCode::Backspace => {
        env.create_string_from_std("Backspace".to_string())?
      }
      crossterm::event::KeyCode::Left => env.create_string_from_std("Left".to_string())?,
      crossterm::event::KeyCode::Right => env.create_string_from_std("Right".to_string())?,
      crossterm::event::KeyCode::Up => env.create_string_from_std("Up".to_string())?,
      crossterm::event::KeyCode::Down => env.create_string_from_std("Down".to_string())?,
      _ => env.create_string_from_std("Unknown".to_string())?,
    };

    js_object.set_named_property("code", key_code)?;

    let modifiers = env.create_int32(key_event.modifiers.bits() as i32)?;
    js_object.set_named_property("modifiers", modifiers)?;

    Ok(js_object.into_unknown())
  }
}
