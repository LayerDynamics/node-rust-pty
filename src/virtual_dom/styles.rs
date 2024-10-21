// src/virtual_dom/styles.rs

// cSpell:ignore vnode gray

use super::virtual_dom::{Styles, VNode};
#[allow(unused_imports)]
use napi::bindgen_prelude::*;
use napi_derive::napi;
use serde_json;
use std::collections::HashMap;

/// Helper function to create a style map from a list of key-value pairs.
///
/// # Parameters
///
/// - `attributes`: A vector of tuples where each tuple contains a style key and its corresponding value.
///
/// # Returns
///
/// A `Styles` `HashMap` containing the provided style attributes.
///
/// # Example
///
/// ```javascript
/// const styles = inputHandler.style([["color", "red"], ["bold", "true"]]);
/// ```
#[napi]
pub fn style(attributes: Vec<(String, String)>) -> Styles {
  let mut styles = HashMap::new();
  for (key, value) in attributes {
    styles.insert(key, value);
  }
  styles
}

/// Creates a styled text node with the given content and styles.
///
/// # Parameters
///
/// - `content`: The text content of the node.
/// - `styles`: A `Styles` `HashMap` defining the styles to apply.
///
/// # Returns
///
/// A JSON string representing the `VNode` for the styled text.
///
/// # Example
///
/// ```javascript
/// const textNode = inputHandler.styledText("Hello, World!", styles);
/// ```
#[napi]
pub fn styled_text(content: String, styles: Styles) -> Result<String> {
  let mut props = HashMap::new();
  props.insert("content".to_string(), content);
  let vnode = VNode::element("text".to_string(), props, styles, vec![]);
  serde_json::to_string(&vnode).map_err(|e| Error::from_reason(e.to_string()))
}

/// Creates a styled element node with the given tag, properties, styles, and children.
///
/// # Parameters
///
/// - `tag`: The tag name of the element (e.g., "div", "span").
/// - `props`: A `HashMap` containing properties or attributes for the element.
/// - `styles`: A `Styles` `HashMap` defining the styles to apply.
/// - `children`: A vector of JSON strings representing child `VNode`s.
///
/// # Returns
///
/// A JSON string representing the `VNode` for the styled element.
///
/// # Example
///
/// ```javascript
/// const elementNode = inputHandler.styledElement("div", props, styles, [childNodeJson]);
/// ```
#[napi]
pub fn styled_element(
  tag: String,
  props: HashMap<String, String>,
  styles: Styles,
  children: Vec<String>,
) -> Result<String> {
  // Deserialize each child JSON string into a VNode
  let mut vchildren = Vec::new();
  for child_json in children {
    let child_vnode: VNode = serde_json::from_str(&child_json)
      .map_err(|e| Error::from_reason(format!("Failed to parse child VNode: {}", e)))?;
    vchildren.push(child_vnode);
  }

  let vnode = VNode::element(tag, props, styles, vchildren);
  serde_json::to_string(&vnode).map_err(|e| Error::from_reason(e.to_string()))
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::virtual_dom::virtual_dom::VNode;
  use std::collections::HashMap;

  #[test]
  fn test_style_helper() {
    let attributes = vec![
      ("color".to_string(), "red".to_string()),
      ("bold".to_string(), "true".to_string()),
    ];
    let styles = style(attributes);
    let mut expected = HashMap::new();
    expected.insert("color".to_string(), "red".to_string());
    expected.insert("bold".to_string(), "true".to_string());
    assert_eq!(styles, expected);
  }

  #[test]
  fn test_styled_text() {
    let content = "Hello".to_string();
    let styles = style(vec![
      ("color".to_string(), "blue".to_string()),
      ("underline".to_string(), "true".to_string()),
    ]);
    let node_json = styled_text(content.clone(), styles.clone()).unwrap();
    let expected_vnode = VNode::element(
      "text".to_string(),
      HashMap::from([("content".to_string(), content)]),
      styles,
      vec![],
    );
    let expected_json = serde_json::to_string(&expected_vnode).unwrap();
    assert_eq!(node_json, expected_json);
  }

  #[test]
  fn test_styled_element() {
    let tag = "div".to_string();
    let props = HashMap::from([("id".to_string(), "container".to_string())]);
    let styles = style(vec![
      ("background".to_string(), "gray".to_string()),
      ("padding".to_string(), "10px".to_string()),
    ]);
    let child = VNode::text("Child Element".to_string());
    let child_json = serde_json::to_string(&child).unwrap();
    let node_json = styled_element(
      tag.clone(),
      props.clone(),
      styles.clone(),
      vec![child_json.clone()],
    )
    .unwrap();
    let expected_vnode = VNode::element(tag, props, styles, vec![child]);
    let expected_json = serde_json::to_string(&expected_vnode).unwrap();
    assert_eq!(node_json, expected_json);
  }
}
