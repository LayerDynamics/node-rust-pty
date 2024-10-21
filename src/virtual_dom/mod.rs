// src/virtual_dom/mod.rs

// Declare submodules within the virtual_dom module
pub mod diff;
pub mod input_handler;
pub mod key_state;
pub mod renderer;
pub mod state;
pub mod styles;
pub mod virtual_dom;

use napi::bindgen_prelude::*;
use napi::JsObject;
use napi_derive::napi;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// Re-export relevant types and functions from the submodules so they can be accessed directly when importing virtual_dom
pub use diff::{diff, Patch};
pub use input_handler::{handle_input, PtyOutputHandler};
pub use renderer::Renderer;
pub use state::State;
pub use styles::{styled_element, styled_text};
pub use virtual_dom::{Styles as VDomStyles, VElement as VDomElement, VNode as VDomNode};

/// Represents the styles applied to a `VElement`.
pub type Styles = HashMap<String, String>;

/// Represents the properties or attributes applied to a `VElement`.
pub type Props = HashMap<String, String>;

/// Represents a Virtual DOM Node.
///
/// Note: Removed duplicate `VNode` definition to avoid type conflicts.
/// Use `VDomNode` for consistency across modules.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum VNode {
  /// A text node containing a string.
  Text(String),
  /// An element node with a tag, properties, styles, and children.
  Element(VElement),
}

impl VNode {
  /// Constructs a `Text` variant of `VNode`.
  pub fn text(s: String) -> Self {
    VNode::Text(s)
  }

  /// Constructs an `Element` variant of `VNode`.
  pub fn element(tag: String, props: Props, styles: Styles, children: Vec<VNode>) -> Self {
    VNode::Element(VElement::new(tag, props, styles, children))
  }

  /// Finds a `VNode` in the tree that satisfies the given predicate.
  pub fn find<F>(&self, predicate: F) -> Option<VNode>
  where
    F: Fn(&VNode) -> bool,
  {
    if predicate(self) {
      return Some(self.clone());
    }

    if let VNode::Element(ref elem) = self {
      for child in &elem.children {
        if let Some(found) = child.find(&predicate) {
          return Some(found);
        }
      }
    }

    None
  }

  /// Clones the subtree rooted at this `VNode`.
  pub fn clone_subtree(&self) -> VNode {
    self.clone()
  }

  /// Merges another `VNode` into this one.
  pub fn merge(&mut self, other: VNode) {
    *self = other;
  }
}

impl ToNapiValue for VNode {
  unsafe fn to_napi_value(
    env: napi::sys::napi_env,
    val: Self,
  ) -> napi::Result<napi::sys::napi_value> {
    let env = napi::Env::from_raw(env);
    let obj = match val {
      VNode::Text(text) => {
        let mut obj = env.create_object()?;
        obj.set_named_property("type", "Text")?;
        obj.set_named_property("content", text)?;
        obj
      }
      VNode::Element(elem) => {
        let mut obj = env.create_object()?;
        obj.set_named_property("type", "Element")?;
        obj.set_named_property("content", elem)?;
        obj
      }
    };
    napi::JsObject::to_napi_value(env.raw(), obj)
  }
}

impl FromNapiValue for VNode {
  unsafe fn from_napi_value(
    env: napi::sys::napi_env,
    napi_val: napi::sys::napi_value,
  ) -> napi::Result<Self> {
    let env = napi::Env::from_raw(env);
    let obj: napi::JsObject = napi::JsObject::from_napi_value(env.raw(), napi_val)?;
    let node_type: String = obj.get_named_property("type")?;
    match node_type.as_str() {
      "Text" => {
        let content: String = obj.get_named_property("content")?;
        Ok(VNode::Text(content))
      }
      "Element" => {
        let content: VElement = obj.get_named_property("content")?;
        Ok(VNode::Element(content))
      }
      _ => Err(napi::Error::from_reason(format!(
        "Unknown VNode type: {}",
        node_type
      ))),
    }
  }
}

/// Represents an element in the Virtual DOM.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VElement {
  /// The tag name, e.g., "div", "span", "text", etc.
  pub tag: String,
  /// Properties associated with the element, e.g., attributes.
  pub props: Props,
  /// Styles associated with the element.
  pub styles: Styles,
  /// Child nodes of this element.
  pub children: Vec<VNode>,
}

impl VElement {
  /// Creates a new `VElement` instance.
  pub fn new(tag: String, props: Props, styles: Styles, children: Vec<VNode>) -> Self {
    VElement {
      tag,
      props,
      styles,
      children,
    }
  }
}

impl ToNapiValue for VElement {
  unsafe fn to_napi_value(
    env: napi::sys::napi_env,
    val: Self,
  ) -> napi::Result<napi::sys::napi_value> {
    let env = napi::Env::from_raw(env);
    let mut obj = env.create_object()?;
    obj.set_named_property("tag", val.tag)?;
    obj.set_named_property("props", val.props)?;
    obj.set_named_property("styles", val.styles)?;
    obj.set_named_property("children", val.children)?;
    napi::JsObject::to_napi_value(env.raw(), obj)
  }
}

impl FromNapiValue for VElement {
  unsafe fn from_napi_value(
    env: napi::sys::napi_env,
    napi_val: napi::sys::napi_value,
  ) -> napi::Result<Self> {
    let env = napi::Env::from_raw(env);
    let obj: napi::JsObject = napi::JsObject::from_napi_value(env.raw(), napi_val)?;
    let tag: String = obj.get_named_property("tag")?;
    let props: Props = obj.get_named_property("props")?;
    let styles: Styles = obj.get_named_property("styles")?;
    let children: Vec<VNode> = obj.get_named_property("children")?;
    Ok(VElement {
      tag,
      props,
      styles,
      children,
    })
  }
}

/// Initializes the Virtual DOM module for N-API.
///
/// This function is automatically called by Node.js when the module is loaded.
#[napi]
pub fn init_virtual_dom() -> Result<()> {
  Ok(())
}

/// Helper function to create `Styles` from a list of key-value pairs.
pub fn style(pairs: &[(&str, &str)]) -> Styles {
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

// Implement ValidateNapiValue for VElement
impl TypeName for VElement {
  fn type_name() -> &'static str {
    "VElement"
  }

  fn value_type() -> napi::ValueType {
    napi::ValueType::Object
  }
}

impl ValidateNapiValue for VElement {
  unsafe fn validate(
    env: napi::sys::napi_env,
    value: napi::sys::napi_value,
  ) -> napi::Result<napi::sys::napi_value> {
    let env = napi::Env::from_raw(env);
    let value = napi::JsUnknown::from_napi_value(env.raw(), value)?;
    // Attempt to extract each field to ensure the structure is correct
    let obj: JsObject = value.coerce_to_object()?.coerce_to_object()?;
    obj.get_named_property::<String>("tag")?;
    obj.get_named_property::<Props>("props")?;
    obj.get_named_property::<Styles>("styles")?;
    obj.get_named_property::<Vec<VNode>>("children")?;
    napi::JsObject::to_napi_value(env.raw(), obj)
  }
}

impl TypeName for VNode {
  fn type_name() -> &'static str {
    "VNode"
  }

  fn value_type() -> napi::ValueType {
    napi::ValueType::Object
  }
}

impl ValidateNapiValue for VNode {
  unsafe fn validate(
    env: napi::sys::napi_env,
    value: napi::sys::napi_value,
  ) -> napi::Result<napi::sys::napi_value> {
    let env = napi::Env::from_raw(env);
    let value = napi::JsUnknown::from_napi_value(env.raw(), value)?;
    let obj: JsObject = value.coerce_to_object()?.coerce_to_object()?;
    let node_type: String = obj.get_named_property("type")?;
    match node_type.as_str() {
      "Text" => {
        obj.get_named_property::<String>("content")?;
      }
      "Element" => {
        obj.get_named_property::<VElement>("content")?;
      }
      _ => {
        return Err(napi::Error::from_reason("Unknown VNode type".to_string()));
      }
    }
    napi::JsObject::to_napi_value(env.raw(), obj)
  }
}
