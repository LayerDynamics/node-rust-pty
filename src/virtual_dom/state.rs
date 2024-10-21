// src/virtual_dom/state.rs

use napi::bindgen_prelude::{Env, Function, Result as NapiResult, Unknown};
use napi::threadsafe_function::ErrorStrategy::CalleeHandled;
use napi::threadsafe_function::{ThreadsafeFunction, ThreadsafeFunctionCallMode};
use napi::NapiRaw;
use napi::NapiValue;
use napi_derive::napi;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

// Import serde for serialization
use serde::Serialize;

// Helper function to convert Unknown to String
fn unknown_to_string(env: Env, unknown: &Unknown) -> Result<String, napi::Error> {
  // Create a JsUnknown from the original Unknown reference
  let js_unknown = unsafe { napi::JsUnknown::from_raw(env.raw(), unknown.raw())? };
  let js_string = js_unknown.coerce_to_string()?;
  let utf8_value = js_string.into_utf8()?;
  utf8_value.as_str().map(|s| s.to_owned()).map_err(|_| {
    napi::Error::new(
      napi::Status::GenericFailure,
      "Failed to convert JsString to Rust String.".to_string(),
    )
  })
}

#[napi]
pub struct State {
  value: Arc<Mutex<Option<String>>>, // Changed to String for Send
  listeners: Arc<Mutex<HashMap<u32, Arc<ThreadsafeFunction<String, CalleeHandled>>>>>,
  next_listener_id: Arc<Mutex<u32>>,
}

#[napi]
impl State {
  pub fn new(env: Env, initial: Unknown) -> NapiResult<Self> {
    // Convert Unknown to String using the helper function
    let initial_str = unknown_to_string(env, &initial).map_err(|e| {
      napi::Error::new(
        napi::Status::GenericFailure,
        format!("Serialization Error: {}", e),
      )
    })?;
    Ok(Self {
      value: Arc::new(Mutex::new(Some(initial_str))),
      listeners: Arc::new(Mutex::new(HashMap::new())),
      next_listener_id: Arc::new(Mutex::new(0)),
    })
  }

  pub fn set(&self, env: Env, new_value: Unknown) -> NapiResult<()> {
    // Convert new_value to String using the helper function
    let new_value_str = unknown_to_string(env, &new_value).map_err(|e| {
      napi::Error::new(
        napi::Status::GenericFailure,
        format!("Serialization Error: {}", e),
      )
    })?;
    {
      let mut value_guard = self.value.lock().unwrap();
      *value_guard = Some(new_value_str.clone());
    }
    self.notify_listeners(env)?;
    Ok(())
  }

  #[napi(getter)]
  pub fn get(&self, _env: Env) -> NapiResult<String> {
    let value_guard = self.value.lock().unwrap();
    if let Some(ref value) = *value_guard {
      Ok(value.clone())
    } else {
      Err(napi::Error::new(
        napi::Status::GenericFailure,
        "State value is not set.".to_string(),
      ))
    }
  }

  #[napi]
  pub fn subscribe(&self, env: Env, callback: Function<Unknown, ()>) -> NapiResult<u32> {
    // Convert the callback to a JsFunction using from_raw
    let js_function = unsafe { napi::JsFunction::from_raw(env.raw(), callback.raw())? };

    // Create the threadsafe function using the environment instance
    let tsfn = env.create_threadsafe_function(
      &js_function,
      0,
      |ctx: napi::threadsafe_function::ThreadSafeCallContext<String>| {
        // Pass the serialized string to the JavaScript callback
        Ok(vec![ctx.value.clone()])
      },
    )?;

    // Generate a unique listener ID
    let id = {
      let mut id_guard = self.next_listener_id.lock().unwrap();
      let current_id = *id_guard;
      *id_guard += 1;
      current_id
    };

    // Insert the threadsafe function into the listeners map
    let mut listeners_guard = self.listeners.lock().unwrap();
    listeners_guard.insert(id, Arc::new(tsfn));
    Ok(id)
  }

  #[napi]
  pub fn unsubscribe(&self, _env: Env, listener_id: u32) -> NapiResult<()> {
    let mut listeners_guard = self.listeners.lock().unwrap();
    listeners_guard.remove(&listener_id);
    Ok(())
  }

  fn notify_listeners(&self, _env: Env) -> NapiResult<()> {
    // Clone all listeners to avoid holding the lock during callback invocation
    let listeners: Vec<_> = {
      let listeners_guard = self.listeners.lock().unwrap();
      listeners_guard.values().cloned().collect()
    };

    // Get the current value
    let value_str = {
      let value_guard = self.value.lock().unwrap();
      if let Some(ref value) = *value_guard {
        value.clone()
      } else {
        return Err(napi::Error::new(
          napi::Status::GenericFailure,
          "State value is not set.".to_string(),
        ));
      }
    };

    // Notify each listener
    for tsfn in listeners {
      // Call the threadsafe function with the serialized string value
      let status = tsfn
        .as_ref()
        .call(Ok(value_str.clone()), ThreadsafeFunctionCallMode::Blocking);
      if status != napi::Status::Ok {
        return Err(napi::Error::new(
          napi::Status::GenericFailure,
          format!("Failed to call threadsafe function: {:?}", status),
        ));
      }
    }

    Ok(())
  }

  pub fn handle_key_event(&self, env: Env, js_key_event: Unknown) -> NapiResult<()> {
    // Convert the key event to a JsObject using an unsafe block
    let js_key_event = unsafe { napi::JsObject::from_raw(env.raw(), js_key_event.raw())? };

    // Extract the "code" property from the event object
    let js_code = js_key_event.get_named_property::<napi::JsString>("code")?;
    let code_str = js_code.into_utf8()?.into_owned()?;

    // Extract the "modifiers" property from the event object
    let js_modifiers = js_key_event.get_named_property::<napi::JsNumber>("modifiers")?;
    let modifiers = js_modifiers.get_int32()?;

    // Based on the key code and modifiers, update the internal state or notify listeners
    {
      // Lock the state value for modification
      let mut value_guard = self.value.lock().unwrap();
      // Update the state value with a message based on the key event
      *value_guard = Some(format!(
        "Key pressed: {}, Modifiers: {}",
        code_str, modifiers
      ));
    }

    // Notify listeners about the change in state
    self.notify_listeners(env)?;

    Ok(())
  }
}
