// src/utils/input_events.rs

use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct InputEvent {
  pub key: String,
  pub modifiers: Vec<String>,
}

impl InputEvent {
  pub fn new(key: String, modifiers: Vec<String>) -> Self {
    InputEvent { key, modifiers }
  }
}
