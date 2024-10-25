// src/virtual_dom/diff.rs
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
        patches.extend(diff_children(&old_elem.children, &new_elem.children));
      }
    }
    _ => {
      patches.push(Patch::Replace(new.clone()));
    }
  }

  // Remove any None patches unless it's the only patch
  if patches.len() > 1 {
    patches.retain(|p| !matches!(p, Patch::None));
  }

  patches
}

/// Compares two sets of properties and returns the properties that need to be updated.
fn diff_props(
  old_props: &HashMap<String, String>,
  new_props: &HashMap<String, String>,
) -> HashMap<String, String> {
  let mut patches = HashMap::new();

  // Find changed or new properties
  for (key, new_value) in new_props {
    match old_props.get(key) {
      Some(old_value) if old_value != new_value => {
        patches.insert(key.clone(), new_value.clone());
      }
      None => {
        patches.insert(key.clone(), new_value.clone());
      }
      _ => {}
    }
  }

  // Find removed properties
  for key in old_props.keys() {
    if !new_props.contains_key(key) {
      patches.insert(key.clone(), "".to_string());
    }
  }

  patches
}

/// Compares two lists of children and returns the necessary patches.
fn diff_children(old_children: &[VNode], new_children: &[VNode]) -> Vec<Patch> {
  let mut patches = Vec::new();
  let mut i = 0;

  // First handle the overlapping children
  while i < old_children.len() && i < new_children.len() {
    if old_children[i] != new_children[i] {
      // If the nodes are different, extend with their diff patches
      patches.extend(diff(&old_children[i], &new_children[i]));
    }
    i += 1;
  }

  // Handle any remaining new children that need to be appended
  while i < new_children.len() {
    patches.push(Patch::AppendChild(new_children[i].clone()));
    i += 1;
  }

  // Handle any remaining old children that need to be removed
  // Note: We remove from right to left to maintain correct indices
  for j in (i..old_children.len()).rev() {
    patches.push(Patch::RemoveChild(j));
  }

  patches
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::virtual_dom::VElement;
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

  #[test]
  fn test_diff_complex_update() {
    let old = VNode::element(
      "div".to_string(),
      HashMap::from([("class".to_string(), "container".to_string())]),
      HashMap::new(),
      vec![
        VNode::text("Hello".to_string()),
        VNode::element("span".to_string(), HashMap::new(), HashMap::new(), vec![]),
      ],
    );

    let new = VNode::element(
      "div".to_string(),
      HashMap::from([("class".to_string(), "wrapper".to_string())]),
      HashMap::new(),
      vec![
        VNode::text("Hello World".to_string()),
        VNode::element("p".to_string(), HashMap::new(), HashMap::new(), vec![]),
      ],
    );

    let patches = diff(&old, &new);
    assert!(!patches.is_empty());
    assert!(patches.iter().any(|p| matches!(p, Patch::UpdateProps(_))));
  }
}
