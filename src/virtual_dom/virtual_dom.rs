// src/virtual_dom/mod.rs

use napi::bindgen_prelude::*;
use napi_derive::napi;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Represents the styles applied to a `VElement`.
pub type Styles = HashMap<String, String>;

/// Represents a Virtual DOM Node.
///
/// A `VNode` can be either a text node containing a string or an element node with a tag,
/// properties, styles, and child nodes.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum VNode {
  /// A text node containing a string.
  Text(String),
  /// An element node with a tag, properties, styles, and children.
  Element(VElement),
}

/// Represents an element in the Virtual DOM.
///
/// A `VElement` contains a tag name, a set of properties (attributes), styles,
/// and a list of child `VNode`s.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VElement {
  /// The tag name, e.g., "div", "span", "text", etc.
  pub tag: String,
  /// Properties associated with the element, e.g., attributes.
  pub props: HashMap<String, String>,
  /// Styles associated with the element.
  pub styles: Styles,
  /// Child nodes of this element.
  pub children: Vec<VNode>,
}

impl VElement {
  /// Creates a new `VElement` instance.
  ///
  /// # Parameters
  ///
  /// - `tag`: The tag name of the element.
  /// - `props`: A `HashMap` containing properties or attributes for the element.
  /// - `styles`: A `HashMap` defining the styles to apply.
  /// - `children`: A `Vec` of child `VNode`s.
  ///
  /// # Returns
  ///
  /// A new instance of `VElement`.
  pub fn new(
    tag: String,
    props: HashMap<String, String>,
    styles: Styles,
    children: Vec<VNode>,
  ) -> Self {
    VElement {
      tag,
      props,
      styles,
      children,
    }
  }
}

impl VNode {
  /// Creates a new text node.
  ///
  /// - `content`: The text content of the node.
  ///
  /// # Returns
  ///
  /// A `VNode` representing a text node.
  pub fn text(content: String) -> Self {
    VNode::Text(content)
  }

  /// Creates a new element node.
  ///
  /// # Parameters
  ///
  /// - `tag`: The tag name of the element.
  /// - `props`: A `HashMap` containing properties or attributes for the element.
  /// - `styles`: A `HashMap` defining the styles to apply.
  /// - `children`: A `Vec` of child `VNode`s.
  ///
  /// # Returns
  ///
  /// A `VNode` representing an element node.
  pub fn element(
    tag: String,
    props: HashMap<String, String>,
    styles: Styles,
    children: Vec<VNode>,
  ) -> Self {
    VNode::Element(VElement::new(tag, props, styles, children))
  }
}

/// Initializes the Virtual DOM module for N-API.
///
/// This function is automatically called by Node.js when the module is loaded.
#[napi]
pub fn init() -> Result<()> {
  Ok(())
}

/// Helper function to create `Styles` from a list of key-value pairs.
fn style(pairs: &[(&str, &str)]) -> Styles {
  pairs
    .iter()
    .map(|(k, v)| (k.to_string(), v.to_string()))
    .collect()
}

#[cfg(test)]
mod tests {
  use super::*;
  use std::collections::HashMap;

  #[test]
  fn test_vnode_text_creation() {
    let text = "Sample Text";
    let node = VNode::text(text.to_string());
    assert_eq!(node, VNode::Text(text.to_string()));
  }

  #[test]
  fn test_vnode_element_creation() {
    let tag = "span".to_string();
    let props = HashMap::from([("class".to_string(), "highlight".to_string())]);
    let styles = style(&[("color", "yellow")]);
    let children = vec![VNode::text("Highlighted Text".to_string())];
    let node = VNode::element(tag.clone(), props.clone(), styles.clone(), children.clone());

    let expected = VNode::Element(VElement::new(tag, props, styles, children));
    assert_eq!(node, expected);
  }

  #[test]
  fn test_vnode_equality() {
    let node1 = VNode::text("Hello".to_string());
    let node2 = VNode::text("Hello".to_string());
    let node3 = VNode::text("World".to_string());
    assert_eq!(node1, node2);
    assert_ne!(node1, node3);
  }

  #[test]
  fn test_velement_equality() {
    let tag = "div".to_string();
    let props = HashMap::from([("id".to_string(), "container".to_string())]);
    let styles = style(&[("background", "grey")]);
    let children = vec![VNode::text("Child".to_string())];

    let elem1 = VElement::new(tag.clone(), props.clone(), styles.clone(), children.clone());
    let elem2 = VElement::new(tag.clone(), props.clone(), styles.clone(), children.clone());
    let elem3 = VElement::new(tag, props, styles, vec![]);

    assert_eq!(elem1, elem2);
    assert_ne!(elem1, elem3);
  }
}
