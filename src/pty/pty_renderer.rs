// src/pty/pty_renderer.rs

use crate::pty::PtyOutputHandler;
use crate::virtual_dom::diff::diff;
use crate::virtual_dom::renderer::Renderer;
use crate::virtual_dom::VNode;
use napi::bindgen_prelude::*;
use napi_derive::napi;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use vte::Perform;

/// N-API wrapper for `PtyRenderer`.
///
/// This struct serves as an interface between the PTY output and the Virtual DOM renderer.
/// It processes raw PTY data, converts it into virtual DOM nodes, and updates the renderer accordingly.
#[napi]
pub struct PtyRenderer {
  /// The Virtual DOM renderer instance.
  renderer: Arc<Mutex<Renderer>>,
  /// The previous virtual DOM node for diffing purposes.
  prev_vdom: VNode,
}

#[napi]
impl PtyRenderer {
  /// Creates a new `PtyRenderer` instance.
  ///
  /// Initializes the renderer with a root virtual DOM node.
  ///
  /// # Example
  ///
  /// ```javascript
  /// const ptyRenderer = new PtyRenderer();
  /// ```
  #[napi(constructor)]
  pub fn new() -> Self {
    PtyRenderer {
      renderer: Arc::new(Mutex::new(Renderer::new(VNode::element(
        "root".to_string(),
        HashMap::new(),
        HashMap::new(),
        vec![],
      )))),
      prev_vdom: VNode::element("root".to_string(), HashMap::new(), HashMap::new(), vec![]),
    }
  }

  /// Processes PTY output data by updating the virtual DOM and rendering changes.
  ///
  /// This method converts raw PTY data into a styled virtual DOM node, diffs it against the previous
  /// state, and applies necessary patches to the renderer.
  ///
  /// # Parameters
  ///
  /// - `data`: A `Buffer` containing raw PTY output data.
  ///
  /// # Example
  ///
  /// ```javascript
  /// ptyRenderer.render(Buffer.from('Hello, PTY!'));
  /// ```
  #[napi]
  pub fn render(&mut self, data: Buffer) -> Result<()> {
    // Convert data to UTF-8 string
    let content = String::from_utf8_lossy(data.as_ref()).to_string();

    // Create the new virtual DOM node
    let mut props = HashMap::new();
    props.insert("content".to_string(), content.clone());

    let new_vdom = VNode::element("text".to_string(), props, HashMap::new(), vec![]);

    // Lock the renderer and apply changes
    let renderer = self.renderer.lock().unwrap();

    // Compute and apply patches
    let patches = diff(&self.prev_vdom, &new_vdom);
    renderer.apply_patches(&patches)?;

    // Update previous state
    self.prev_vdom = new_vdom;

    Ok(())
  }

  /// Processes and sends rendered output to a specific session.
  ///
  /// This method is intended to be used in conjunction with session management systems.
  /// Currently, it updates the virtual DOM based on PTY output.
  ///
  /// # Parameters
  ///
  /// - `session_id`: The ID of the session to send data to (currently unused).
  /// - `data`: A `Buffer` containing raw PTY output data.
  ///
  /// # Example
  ///
  /// ```javascript
  /// ptyRenderer.processAndSend(1, Buffer.from('Session-specific data'));
  /// ```
  #[napi]
  pub fn process_and_send(&mut self, _session_id: u32, data: Buffer) -> Result<()> {
    // Convert data to a UTF-8 string, handling potential invalid bytes gracefully.
    let content = String::from_utf8_lossy(data.as_ref()).to_string();

    // Create a new virtual DOM node representing the PTY output.
    let mut props = HashMap::new();
    props.insert("content".to_string(), content.clone());

    let new_vdom = VNode::element("text".to_string(), props, HashMap::new(), vec![]);

    // Lock the renderer to apply changes.
    let renderer = self.renderer.lock().unwrap();

    // Diff the new VDOM against the previous state to determine necessary patches.
    let patches = diff(&self.prev_vdom, &new_vdom);

    // Apply the computed patches to the renderer.
    renderer.apply_patches(&patches)?;

    // Update the previous VDOM state.
    self.prev_vdom = new_vdom;

    Ok(())
  }
}

impl Perform for PtyRenderer {
  /// Handles printable characters by passing them to the renderer.
  ///
  /// # Parameters
  ///
  /// - `c`: The character to print.
  fn print(&mut self, c: char) {
    let byte = c as u8;
    self.handle_output(&[byte]);
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
  /// - `intermediate`: Intermediate bytes.
  /// - `ignore`: Whether to ignore the hook.
  /// - `action`: The action character.
  fn hook(&mut self, _params: &vte::Params, _intermediate: &[u8], _ignore: bool, _action: char) {}

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

impl PtyOutputHandler for PtyRenderer {
  /// Handles PTY output by rendering it to the virtual DOM.
  ///
  /// # Parameters
  ///
  /// - `data`: A byte slice containing raw PTY output data.
  fn handle_output(&mut self, data: &[u8]) {
    // Convert data to a UTF-8 string, handling potential invalid bytes gracefully.
    let content = String::from_utf8_lossy(data).to_string();

    // Create a new virtual DOM node representing the PTY output.
    let mut props = HashMap::new();
    props.insert("content".to_string(), content.clone());

    let new_vdom = VNode::element("text".to_string(), props, HashMap::new(), vec![]);

    // Lock the renderer to apply changes.
    let renderer = self.renderer.lock().unwrap();

    // Diff the new VDOM against the previous state to determine necessary patches.
    let patches = diff(&self.prev_vdom, &new_vdom);

    // Apply the computed patches to the renderer.
    if let Err(e) = renderer.apply_patches(&patches) {
      eprintln!("Error applying patches: {:?}", e);
    }

    // Update the previous VDOM state.
    self.prev_vdom = new_vdom;
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::virtual_dom::{VElement as VirtualVElement, VNode as VirtualVNode};
  use std::sync::Arc;

  // Helper function to create a test VNode with the same structure as what the renderer creates
  fn create_test_node(content: &str) -> VirtualVNode {
    let mut props = HashMap::new();
    props.insert("content".to_string(), content.to_string());
    VirtualVNode::Element(VirtualVElement {
      tag: "text".to_string(),
      props,
      styles: HashMap::new(),
      children: vec![],
    })
  }

  // Helper function to create root node
  fn create_root_node() -> VirtualVNode {
    VirtualVNode::Element(VirtualVElement {
      tag: "root".to_string(),
      props: HashMap::new(),
      styles: HashMap::new(),
      children: vec![],
    })
  }

  // Helper function to compare VNodes with debug output
  fn nodes_are_equal(renderer: &Renderer, expected: &VirtualVNode) -> bool {
    let current_state = renderer
      .get_current_state()
      .expect("Failed to get current state");

    if current_state != *expected {
      println!("Node comparison failed!");
      println!("Expected: {:?}", expected);
      println!("Got: {:?}", current_state);
      false
    } else {
      true
    }
  }

  /// Test that `PtyRenderer` correctly processes and renders PTY output.
  #[test]
  fn test_pty_renderer_render() {
    // Initialize the renderer with proper root node
    let mut pty_renderer = PtyRenderer::new();

    // Simulate PTY output
    let data = Buffer::from("Hello, PTY!".as_bytes());
    pty_renderer.render(data).unwrap();

    // Get renderer lock
    let renderer = pty_renderer.renderer.lock().unwrap();

    // The expected node structure matches what the renderer creates
    let expected_node = create_test_node("Hello, PTY!");

    assert!(
      nodes_are_equal(&renderer, &expected_node),
      "Node structures don't match after render"
    );
  }

  /// Test that `PtyRenderer` applies patches correctly
  #[test]
  fn test_pty_renderer_apply_patches() {
    let mut pty_renderer = PtyRenderer::new();

    // First update
    let data1 = Buffer::from("Hello, ".as_bytes());
    pty_renderer.render(data1).unwrap();

    // Second update
    let data2 = Buffer::from("PTY!".as_bytes());
    pty_renderer.render(data2).unwrap();

    // Verify final state
    let renderer = pty_renderer.renderer.lock().unwrap();
    let expected_node = create_test_node("PTY!");

    assert!(
      nodes_are_equal(&renderer, &expected_node),
      "Node structures don't match after patches"
    );
  }

  /// Test that `PtyRenderer` handles empty data gracefully
  #[test]
  fn test_pty_renderer_handle_empty_data() {
    let mut pty_renderer = PtyRenderer::new();

    // Simulate empty PTY output
    let empty_data = Buffer::from("".as_bytes());
    pty_renderer.render(empty_data).unwrap();

    // Verify state
    let renderer = pty_renderer.renderer.lock().unwrap();
    let expected_node = create_test_node("");

    assert!(
      nodes_are_equal(&renderer, &expected_node),
      "Node structures don't match for empty input"
    );
  }

  /// Test that `PtyRenderer` processes multiple outputs correctly
  #[test]
  fn test_pty_renderer_multiple_outputs() {
    let mut pty_renderer = PtyRenderer::new();

    // Simulate multiple PTY outputs
    let data1 = Buffer::from("Hello, ".as_bytes());
    pty_renderer.render(data1).unwrap();

    let data2 = Buffer::from("World!".as_bytes());
    pty_renderer.render(data2).unwrap();

    // Verify final state
    let renderer = pty_renderer.renderer.lock().unwrap();
    let expected_node = create_test_node("World!");

    assert!(
      nodes_are_equal(&renderer, &expected_node),
      "Node structures don't match after multiple outputs"
    );
  }
}
