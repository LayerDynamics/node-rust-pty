// src/pty/output_handler.rs

use crate::virtual_dom::diff::diff;
use crate::virtual_dom::renderer::Renderer as VirtualDomRenderer;
use crate::virtual_dom::{VElement, VNode};
use bytes::Bytes;
use napi::bindgen_prelude::*;
use napi_derive::napi;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use vte::{Parser, Perform};

/// Trait defining how PTY output should be handled.
///
/// Implementations of this trait are responsible for processing raw data from the PTY process.
/// This could involve updating the virtual DOM, dispatching data to multiple sessions, or other
/// forms of output processing.
///
/// # Example
///
/// ```rust
/// struct MyOutputHandler {
///     // Fields for your handler
/// }
///
/// impl Perform for MyOutputHandler {
///     fn print(&mut self, c: char) {
///         // Handle printable characters
///     }
///
///     // Implement other required methods...
/// }
///
/// impl PtyOutputHandler for MyOutputHandler {
///     fn handle_output(&mut self, data: &[u8]) {
///         // Process the output data
///     }
/// }
/// ```
pub trait PtyOutputHandler: Perform + Send {
  /// Handles the output data received from the PTY process.
  ///
  /// # Parameters
  ///
  /// - `data`: A slice of bytes containing the raw output from the PTY.
  fn handle_output(&mut self, data: &[u8]);
}

/// Wrapper for handling PTY output and integrating with the virtual DOM.
///
/// This struct implements both the `Perform` trait required by the VTE parser and the `PtyOutputHandler`
/// trait to process PTY output data.
pub struct PtyOutputHandlerWrapper {
  /// The renderer for updating the virtual DOM.
  renderer: Arc<Mutex<VirtualDomRenderer>>,
  /// The previous virtual DOM state for diffing.
  prev_vdom: VNode,
}

impl PtyOutputHandlerWrapper {
  /// Creates a new `PtyOutputHandlerWrapper`.
  ///
  /// # Parameters
  ///
  /// - `renderer`: An `Arc<Mutex<VirtualDomRenderer>>` instance for rendering updates.
  pub fn new(renderer: Arc<Mutex<VirtualDomRenderer>>) -> Self {
    // Initialize the previous VDOM as the root element.
    PtyOutputHandlerWrapper {
      renderer,
      prev_vdom: VNode::element("root".to_string(), HashMap::new(), HashMap::new(), vec![]),
    }
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

impl PtyOutputHandler for PtyOutputHandlerWrapper {
  /// Processes PTY output by updating the virtual DOM renderer.
  ///
  /// # Parameters
  ///
  /// - `data`: A byte slice containing raw PTY output data.
  fn handle_output(&mut self, data: &[u8]) {
    // Convert data to a UTF-8 string, handling potential invalid bytes gracefully.
    let content = String::from_utf8_lossy(data).to_string();

    // Define styles for the text. This can be expanded to handle dynamic styling based on content.
    let styles = HashMap::new();

    // Create a new virtual DOM node representing the PTY output.
    let mut props = HashMap::new();
    props.insert("content".to_string(), content.clone());

    let new_vdom = VNode::element("text".to_string(), props, styles, vec![]);

    // Lock the renderer to apply changes.
    let renderer = self.renderer.lock().unwrap();

    // Diff the new VDOM against the previous state to determine necessary patches.
    let patches = diff(&self.prev_vdom, &new_vdom);

    // Apply the computed patches to the renderer.
    renderer.apply_patches(&patches).unwrap();

    // Update the previous VDOM state.
    self.prev_vdom = new_vdom;
  }
}

// #[cfg(test)]
// mod tests {
//   use super::*;
//   use crate::virtual_dom::virtual_dom::{VElement, VNode};
//   use bytes::Bytes;
//   use std::sync::Arc;
//   use std::sync::Mutex;

//   /// Test that `PtyOutputHandlerWrapper` correctly processes and renders PTY output.
//   #[tokio::test]
//   async fn test_pty_output_handler_wrapper() {
//     // Initialize the renderer with a root node.
//     let renderer = Arc::new(Mutex::new(VirtualDomRenderer::new(VNode::element(
//       "root".to_string(),
//       HashMap::new(),
//       HashMap::new(),
//       vec![],
//     ))));

//     // Create the output handler wrapper.
//     let mut handler = PtyOutputHandlerWrapper::new(renderer.clone());

//     // Simulate PTY output.
//     let output_data = b"Hello, PTY!";
//     handler.handle_output(output_data);

//     // Check that the renderer has been updated.
//     let renderer_guard = renderer.lock().unwrap();
//     assert_eq!(
//       renderer_guard.root,
//       VNode::element(
//         "text".to_string(),
//         {
//           let mut props = HashMap::new();
//           props.insert("content".to_string(), "Hello, PTY!".to_string());
//           props
//         },
//         HashMap::new(),
//         vec![]
//       )
//     );
//   }

//   /// Test that `Renderer` applies patches correctly.
//   #[test]
//   fn test_renderer_apply_patches() {
//     // Initialize renderer with root node.
//     let initial_vdom = VNode::element("root".to_string(), HashMap::new(), HashMap::new(), vec![]);
//     let mut renderer = VirtualDomRenderer::new(initial_vdom.clone());

//     // Create a new VDOM node.
//     let new_vdom = VNode::element(
//       "text".to_string(),
//       {
//         let mut props = HashMap::new();
//         props.insert("content".to_string(), "Test data".to_string());
//         props
//       },
//       HashMap::new(),
//       vec![],
//     );

//     // Compute patches.
//     let patches = diff(&initial_vdom, &new_vdom);

//     // Apply patches.
//     renderer.apply_patches(&patches);

//     // Verify that the current VDOM matches the new VDOM.
//     assert_eq!(renderer.root, new_vdom);
//   }

//   /// Test that `Renderer` handles empty data gracefully.
//   #[test]
//   fn test_renderer_handle_empty_data() {
//     // Initialize renderer with root node.
//     let initial_vdom = VNode::element("root".to_string(), HashMap::new(), HashMap::new(), vec![]);
//     let mut renderer = VirtualDomRenderer::new(initial_vdom.clone());

//     // Simulate empty PTY output.
//     // Assuming Renderer has a method to handle empty data; if not, this test should be adjusted accordingly.
//     // For example:
//     // renderer.handle_output(&[]);
//     // Since handle_output is not defined, we'll assume no changes occur.

//     // Verify that the VDOM remains unchanged.
//     assert_eq!(renderer.root, initial_vdom);
//   }

//   /// Test that multiple outputs are handled correctly.
//   #[tokio::test]
//   async fn test_multiple_outputs() {
//     // Initialize the renderer with a root node.
//     let renderer = Arc::new(Mutex::new(VirtualDomRenderer::new(VNode::element(
//       "root".to_string(),
//       HashMap::new(),
//       HashMap::new(),
//       vec![],
//     ))));

//     // Create the output handler wrapper.
//     let mut handler = PtyOutputHandlerWrapper::new(renderer.clone());

//     // Simulate multiple PTY outputs.
//     let output_data1 = b"Hello, ";
//     let output_data2 = b"PTY!";
//     handler.handle_output(output_data1);
//     handler.handle_output(output_data2);

//     // Check that the renderer has been updated correctly.
//     let renderer_guard = renderer.lock().unwrap();
//     assert_eq!(
//       renderer_guard.root,
//       VNode::element(
//         "text".to_string(),
//         {
//           let mut props = HashMap::new();
//           props.insert("content".to_string(), "PTY!".to_string());
//           props
//         },
//         HashMap::new(),
//         vec![]
//       )
//     );
//   }
// }
