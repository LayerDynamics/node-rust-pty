// src/pty/pty_renderer.rs

use crate::pty::emulator::PtyOutputHandler;
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
    // Convert data to a UTF-8 string, handling potential invalid bytes gracefully.
    let content = String::from_utf8_lossy(data.as_ref()).to_string();

    // Create a new virtual DOM node representing the PTY output.
    let mut props = HashMap::new();
    props.insert("content".to_string(), content.clone());
    let new_vdom = VNode::element("text".to_string(), props, HashMap::new(), vec![]);

    let renderer = self.renderer.lock().unwrap();

    // Diff the new VDOM against the previous state to determine necessary patches.
    let patches = diff(&self.prev_vdom, &new_vdom);

    // Apply the computed patches to the renderer.
    renderer.apply_patches(&patches)?;

    // Update the previous VDOM state.
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
    let mut renderer = self.renderer.lock().unwrap();

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

// #[cfg(test)]
// mod tests {
//     use super::*;
//     use crate::virtual_dom::VNode;
//     use std::sync::Arc;
//     use std::sync::Mutex;

//     /// Test that `PtyRenderer` correctly processes and renders PTY output.
//     #[test]
//     fn test_pty_renderer_render() {
//         // Initialize the renderer with a root node.
//         let renderer = Arc::new(Mutex::new(Renderer::new(VNode::element(
//             "root".to_string(),
//             HashMap::new(),
//             HashMap::new(),
//             vec![],
//         ))));

//         // Create the output handler wrapper.
//         let mut pty_renderer = PtyRenderer {
//             renderer: Arc::clone(&renderer),
//             prev_vdom: VNode::element("root".to_string(), HashMap::new(), HashMap::new(), vec![]),
//         };

//         // Simulate PTY output.
//         let data = Buffer::from("Hello, PTY!".as_bytes());
//         pty_renderer.render(data).unwrap();

//         // Verify the final state.
//         let renderer_guard = renderer.lock().unwrap();
//         assert_eq!(
//             *renderer_guard.root.lock().unwrap(),
//             VNode::element(
//                 "text".to_string(),
//                 {
//                     let mut props = HashMap::new();
//                     props.insert("content".to_string(), "Hello, PTY!".to_string());
//                     props
//                 },
//                 HashMap::new(),
//                 vec![],
//             )
//         );
//     }

//     /// Test that `PtyRenderer` applies patches correctly via the `render` method.
//     #[test]
//     fn test_pty_renderer_apply_patches() {
//         let renderer = Arc::new(Mutex::new(Renderer::new(VNode::element(
//             "root".to_string(),
//             HashMap::new(),
//             HashMap::new(),
//             vec![],
//         ))));

//         let mut pty_renderer = PtyRenderer {
//             renderer: Arc::clone(&renderer),
//             prev_vdom: VNode::element("root".to_string(), HashMap::new(), HashMap::new(), vec![]),
//         };

//         // Simulate PTY output.
//         let data1 = Buffer::from("Hello, ".as_bytes());
//         pty_renderer.render(data1).unwrap();

//         // Simulate more PTY output.
//         let data2 = Buffer::from("PTY!".as_bytes());
//         pty_renderer.render(data2).unwrap();

//         // Verify the final state.
//         let renderer_guard = renderer.lock().unwrap();
//         assert_eq!(
//             *renderer_guard.root.lock().unwrap(),
//             VNode::element(
//                 "text".to_string(),
//                 {
//                     let mut props = HashMap::new();
//                     props.insert("content".to_string(), "PTY!".to_string());
//                     props
//                 },
//                 HashMap::new(),
//                 vec![],
//             )
//         );
//     }

//     /// Test that `PtyRenderer` handles empty data gracefully.
//     #[test]
//     fn test_pty_renderer_handle_empty_data() {
//         let renderer = Arc::new(Mutex::new(Renderer::new(VNode::element(
//             "root".to_string(),
//             HashMap::new(),
//             HashMap::new(),
//             vec![],
//         ))));

//         let mut pty_renderer = PtyRenderer {
//             renderer: Arc::clone(&renderer),
//             prev_vdom: VNode::element("root".to_string(), HashMap::new(), HashMap::new(), vec![]),
//         };

//         // Simulate empty PTY output.
//         let empty_data = Buffer::from("".as_bytes());
//         pty_renderer.render(empty_data).unwrap();

//         // Verify that the VDOM remains unchanged.
//         let renderer_guard = renderer.lock().unwrap();
//         assert_eq!(*renderer_guard.root.lock().unwrap(), VNode::element(
//             "root".to_string(),
//             HashMap::new(),
//             HashMap::new(),
//             vec![],
//         ));
//     }

//     /// Test that `PtyRenderer` processes multiple outputs correctly.
//     #[test]
//     fn test_pty_renderer_multiple_outputs() {
//         let renderer = Arc::new(Mutex::new(Renderer::new(VNode::element(
//             "root".to_string(),
//             HashMap::new(),
//             HashMap::new(),
//             vec![],
//         ))));

//         let mut pty_renderer = PtyRenderer {
//             renderer: Arc::clone(&renderer),
//             prev_vdom: VNode::element("root".to_string(), HashMap::new(), HashMap::new(), vec![]),
//         };

//         // Simulate multiple PTY outputs.
//         let data1 = Buffer::from("Hello, ".as_bytes());
//         pty_renderer.render(data1).unwrap();

//         let data2 = Buffer::from("World!".as_bytes());
//         pty_renderer.render(data2).unwrap();

//         // Verify the final state.
//         let renderer_guard = renderer.lock().unwrap();
//         assert_eq!(
//             *renderer_guard.root.lock().unwrap(),
//             VNode::element(
//                 "text".to_string(),
//                 {
//                     let mut props = HashMap::new();
//                     props.insert("content".to_string(), "World!".to_string());
//                     props
//                 },
//                 HashMap::new(),
//                 vec![],
//             )
//         );
//     }
// }
