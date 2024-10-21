// src/virtual_dom/diff.rs
use crate::virtual_dom::styles::style; // Import style correctly
use crate::virtual_dom::VNode;
use napi_derive::napi;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Represents the types of changes (patches) that can be applied to the Virtual DOM.
#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub enum Patch {
  /// Replace an entire node with a new node.
  Replace(VNode),
  /// Update the properties of a node.
  UpdateProps(HashMap<String, String>),
  /// Append a new child node.
  AppendChild(VNode),
  /// Remove a child node at a specific index.
  RemoveChild(usize),
  /// Update the content of a text node.
  UpdateText(String),
  /// No changes needed.
  None,
}

#[napi]
impl Patch {
  /// Returns a description of the patch type.
  ///
  /// # Returns
  ///
  /// A `String` describing the type of patch.
  pub fn get_description(&self) -> String {
    match self {
      Patch::Replace(_) => "Replace".to_string(),
      Patch::UpdateProps(_) => "UpdateProps".to_string(),
      Patch::AppendChild(_) => "AppendChild".to_string(),
      Patch::RemoveChild(_) => "RemoveChild".to_string(),
      Patch::UpdateText(_) => "UpdateText".to_string(),
      Patch::None => "None".to_string(),
    }
  }
}

/// Compares two `VNode`s and returns a list of patches required to transform `old` into `new`.
pub fn diff(old: &VNode, new: &VNode) -> Vec<Patch> {
  let mut patches = Vec::new();

  if old == new {
    patches.push(Patch::None);
    return patches;
  }

  match (old, new) {
    (VNode::Text(old_text), VNode::Text(new_text)) => {
      if old_text != new_text {
        patches.push(Patch::UpdateText(new_text.clone()));
      }
    }
    (VNode::Element(old_elem), VNode::Element(new_elem)) => {
      if old_elem.tag != new_elem.tag {
        patches.push(Patch::Replace(new.clone()));
      } else {
        // Diff properties
        let prop_patches = diff_props(&old_elem.props, &new_elem.props);
        if !prop_patches.is_empty() {
          patches.push(Patch::UpdateProps(prop_patches));
        }

        // Diff styles
        let style_patches = diff_props(&old_elem.styles, &new_elem.styles);
        if !style_patches.is_empty() {
          patches.push(Patch::UpdateProps(style_patches));
        }

        // Diff children
        let child_patches = diff_children(&old_elem.children, &new_elem.children);
        patches.extend(child_patches);
      }
    }
    _ => {
      patches.push(Patch::Replace(new.clone()));
    }
  }

  patches
}

/// Compares two sets of properties and returns the properties that need to be updated.
fn diff_props(
  old_props: &HashMap<String, String>,
  new_props: &HashMap<String, String>,
) -> HashMap<String, String> {
  let mut patches = HashMap::new();

  for (key, new_value) in new_props {
    if old_props.get(key) != Some(new_value) {
      patches.insert(key.clone(), new_value.clone());
    }
  }

  // Detect removed properties
  for key in old_props.keys() {
    if !new_props.contains_key(key) {
      patches.insert(key.clone(), "".to_string()); // Empty string signifies removal
    }
  }

  patches
}

/// Compares two lists of children and returns the necessary patches.
fn diff_children(old_children: &[VNode], new_children: &[VNode]) -> Vec<Patch> {
  let mut patches = Vec::new();
  let max_len = old_children.len().max(new_children.len());

  for i in 0..max_len {
    let old = old_children.get(i);
    let new = new_children.get(i);

    match (old, new) {
      (Some(old_node), Some(new_node)) => {
        patches.extend(diff(old_node, new_node));
      }
      (None, Some(new_node)) => {
        patches.push(Patch::AppendChild(new_node.clone()));
      }
      (Some(_), None) => {
        patches.push(Patch::RemoveChild(i));
      }
      (None, None) => {}
    }
  }

  patches
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::virtual_dom::{VElement, VNode};
  use std::collections::HashMap;

  #[test]
  fn test_diff_no_changes() {
    let old = VNode::text("Hello".to_string());
    let new = VNode::text("Hello".to_string());
    let patches = diff(&old, &new);
    assert_eq!(patches, vec![Patch::None]);
  }

  #[test]
  fn test_diff_update_text() {
    let old = VNode::text("Hello".to_string());
    let new = VNode::text("Hello, World!".to_string());
    let patches = diff(&old, &new);
    let expected = vec![Patch::UpdateText("Hello, World!".to_string())];
    assert_eq!(patches, expected);
  }

  #[test]
  fn test_diff_replace_node() {
    let old = VNode::text("Hello".to_string());
    let new = VNode::element("div".to_string(), HashMap::new(), HashMap::new(), vec![]);
    let patches = diff(&old, &new);
    assert_eq!(patches, vec![Patch::Replace(new.clone())]);
  }

  #[test]
  fn test_diff_update_props() {
    let old = VNode::element(
      "div".to_string(),
      HashMap::from([("id".to_string(), "container".to_string())]),
      HashMap::new(),
      vec![],
    );
    let new = VNode::element(
      "div".to_string(),
      HashMap::from([("id".to_string(), "main".to_string())]),
      HashMap::new(),
      vec![],
    );
    let patches = diff(&old, &new);
    let expected_patch = vec![Patch::UpdateProps(HashMap::from([(
      "id".to_string(),
      "main".to_string(),
    )]))];
    assert_eq!(patches, expected_patch);
  }

  #[test]
  fn test_diff_append_child() {
    let old = VNode::element(
      "ul".to_string(),
      HashMap::new(),
      HashMap::new(),
      vec![VNode::element(
        "li".to_string(),
        HashMap::new(),
        HashMap::new(),
        vec![VNode::text("Item 1".to_string())],
      )],
    );
    let new = VNode::element(
      "ul".to_string(),
      HashMap::new(),
      HashMap::new(),
      vec![
        VNode::element(
          "li".to_string(),
          HashMap::new(),
          HashMap::new(),
          vec![VNode::text("Item 1".to_string())],
        ),
        VNode::element(
          "li".to_string(),
          HashMap::new(),
          HashMap::new(),
          vec![VNode::text("Item 2".to_string())],
        ),
      ],
    );
    let patches = diff(&old, &new);
    let expected_patch = vec![Patch::AppendChild(VNode::element(
      "li".to_string(),
      HashMap::new(),
      HashMap::new(),
      vec![VNode::text("Item 2".to_string())],
    ))];
    assert_eq!(patches, expected_patch);
  }

  #[test]
  fn test_diff_remove_child() {
    let old = VNode::element(
      "ul".to_string(),
      HashMap::new(),
      HashMap::new(),
      vec![
        VNode::element(
          "li".to_string(),
          HashMap::new(),
          HashMap::new(),
          vec![VNode::text("Item 1".to_string())],
        ),
        VNode::element(
          "li".to_string(),
          HashMap::new(),
          HashMap::new(),
          vec![VNode::text("Item 2".to_string())],
        ),
      ],
    );
    let new = VNode::element(
      "ul".to_string(),
      HashMap::new(),
      HashMap::new(),
      vec![VNode::element(
        "li".to_string(),
        HashMap::new(),
        HashMap::new(),
        vec![VNode::text("Item 1".to_string())],
      )],
    );
    let patches = diff(&old, &new);
    let expected_patch = vec![Patch::RemoveChild(1)];
    assert_eq!(patches, expected_patch);
  }
}
