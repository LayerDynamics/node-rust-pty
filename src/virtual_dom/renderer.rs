// src/virtual_dom/renderer.rs

use crate::virtual_dom::diff::{diff, Patch as DiffPatch, Patch};
use crate::virtual_dom::{VElement as VirtualVElement, VNode as VirtualVNode};
use napi::bindgen_prelude::*;
use napi::NapiValue;
use napi::{JsObject, JsString, JsUnknown, NapiRaw};
use napi_derive::napi;
use std::collections::HashMap;
use std::io::{self, Write};
use std::sync::{Arc, Mutex};

/// A wrapped HashMap to facilitate N-API conversions.
#[derive(Clone, Debug)]
pub struct WrappedHashMap(pub HashMap<String, String>);

impl FromNapiRef for WrappedHashMap {
  unsafe fn from_napi_ref(
    env: napi::sys::napi_env,
    napi_val: napi::sys::napi_value,
  ) -> napi::Result<&'static Self> {
    let obj = JsObject::from_raw_unchecked(env, napi_val);
    let keys = obj.get_property_names()?.into_vec(env)?;
    let keys: Vec<JsString> = keys
      .into_iter()
      .map(|raw_key| {
        let unknown = JsUnknown::from_raw(env, raw_key)?;
        unknown.coerce_to_string() // Removed `env` argument
      })
      .collect::<napi::Result<Vec<_>>>()?;
    let mut map = HashMap::new();
    for key in keys.iter() {
      let key_str: String = key.into_utf8()?.into_owned()?;
      let value: String = obj.get_named_property(&key_str)?;
      map.insert(key_str, value);
    }
    Ok(Box::leak(Box::new(WrappedHashMap(map))))
  }
}

impl FromNapiValue for WrappedHashMap {
  unsafe fn from_napi_value(
    env: napi::sys::napi_env,
    napi_val: napi::sys::napi_value,
  ) -> napi::Result<Self> {
    let obj = JsObject::from_raw_unchecked(env, napi_val);
    let keys = obj.get_property_names()?.into_vec(env)?;
    let keys: Vec<JsString> = keys
      .into_iter()
      .map(|raw_key| {
        let unknown = JsUnknown::from_raw(env, raw_key)?;
        unknown.coerce_to_string() // Removed `env` argument
      })
      .collect::<napi::Result<Vec<_>>>()?;
    let mut map = HashMap::new();
    for key in keys.iter() {
      let key_str: String = key.into_utf8()?.into_owned()?;
      let value: String = obj.get_named_property(&key_str)?;
      map.insert(key_str, value);
    }
    Ok(WrappedHashMap(map))
  }
}

impl ToNapiValue for WrappedHashMap {
  unsafe fn to_napi_value(
    env: napi::sys::napi_env,
    val: Self,
  ) -> napi::Result<napi::sys::napi_value> {
    let mut obj: napi::sys::napi_value = std::mem::zeroed();
    let status = napi::sys::napi_create_object(env, &mut obj);
    if status != napi::sys::Status::napi_ok {
      return Err(napi::Error::from_reason("Failed to create JS object"));
    }

    for (k, v) in val.0 {
      // Create JS string for the key
      let mut key: napi::sys::napi_value = std::mem::zeroed();
      let status =
        napi::sys::napi_create_string_utf8(env, k.as_ptr() as *const _, k.len(), &mut key);
      if status != napi::sys::Status::napi_ok {
        return Err(napi::Error::from_reason("Failed to create string for key"));
      }

      // Create JS string for the value
      let mut value: napi::sys::napi_value = std::mem::zeroed();
      let status =
        napi::sys::napi_create_string_utf8(env, v.as_ptr() as *const _, v.len(), &mut value);
      if status != napi::sys::Status::napi_ok {
        return Err(napi::Error::from_reason(
          "Failed to create string for value",
        ));
      }

      // Set the property on the object
      let status = napi::sys::napi_set_property(env, obj, key, value);
      if status != napi::sys::Status::napi_ok {
        return Err(napi::Error::from_reason(
          "Failed to set property on JS object",
        ));
      }
    }

    Ok(obj)
  }
}

/// A wrapped VNode to facilitate thread-safe operations and N-API conversions.
#[derive(Clone, Debug)]
pub struct WrappedVNode(Arc<Mutex<VirtualVNode>>);

impl ToNapiValue for WrappedVNode {
  unsafe fn to_napi_value(
    env: napi::sys::napi_env,
    val: Self,
  ) -> napi::Result<napi::sys::napi_value> {
    let vnode: VirtualVNode = val.0.lock().unwrap().clone();
    VirtualVNode::to_napi_value(env, vnode)
  }
}

impl FromNapiValue for WrappedVNode {
  unsafe fn from_napi_value(
    env: napi::sys::napi_env,
    napi_val: napi::sys::napi_value,
  ) -> napi::Result<Self> {
    let vnode: VirtualVNode = VirtualVNode::from_napi_value(env, napi_val)?;
    Ok(WrappedVNode(Arc::new(Mutex::new(vnode))))
  }
}

/// Internal representation of a Patch. Not exposed to N-API directly.
#[derive(Clone, Debug)]
enum PatchInternal {
  Replace(VirtualVNode),
  UpdateProps(WrappedHashMap),
  AppendChild(VirtualVNode),
  RemoveChild(u32),
  UpdateText(String),
  None,
}

/// The Renderer applies changes from the Virtual DOM to the terminal.
///
/// It maintains the current state of the Virtual DOM and applies necessary updates
/// to reflect changes efficiently.
#[napi]
pub struct Renderer {
  /// The current root of the Virtual DOM.
  pub root: WrappedVNode,
  /// Tracks rendered nodes for targeted updates.
  rendered_nodes: Arc<Mutex<HashMap<u32, VirtualVNode>>>,
}

#[napi]
impl Renderer {
  /// Creates a new Renderer with the given root `VNode`.
  #[napi(constructor)]
  pub fn new(root: VirtualVNode) -> Self {
    Renderer {
      root: WrappedVNode(Arc::new(Mutex::new(root))),
      rendered_nodes: Arc::new(Mutex::new(HashMap::new())),
    }
  }

  /// Renders a new Virtual DOM by diffing it against the current one and applying patches.
  #[napi]
  pub fn render(&self, new_root: VirtualVNode) -> Result<()> {
    let root_guard = self.root.0.lock().unwrap();
    let patches = diff(&root_guard, &new_root);
    drop(root_guard); // Release the lock before applying patches

    self.apply_patches(&patches)?;
    let mut root_guard = self.root.0.lock().unwrap();
    *root_guard = new_root;
    Ok(())
  }

  /// Applies a list of patches to update the terminal.
  ///
  /// This method is internal and not exposed to N-API.
  pub fn apply_patches(&self, patches: &[Patch]) -> Result<()> {
    let internal_patches = patches
      .iter()
      .map(|p| match p {
        DiffPatch::Replace(node) => PatchInternal::Replace(node.clone()),
        DiffPatch::UpdateProps(props) => PatchInternal::UpdateProps(WrappedHashMap(props.clone())),
        DiffPatch::AppendChild(node) => PatchInternal::AppendChild(node.clone()),
        DiffPatch::RemoveChild(index) => PatchInternal::RemoveChild(*index as u32),
        DiffPatch::UpdateText(new_text) => PatchInternal::UpdateText(new_text.clone()),
        DiffPatch::None => PatchInternal::None,
      })
      .collect::<Vec<_>>();

    for patch in internal_patches {
      match patch {
        PatchInternal::Replace(node) => {
          self.render_node(&node)?;
        }
        PatchInternal::UpdateProps(props) => {
          self.update_props(&props)?;
        }
        PatchInternal::AppendChild(node) => {
          self.append_child(&node)?;
        }
        PatchInternal::RemoveChild(index) => {
          self.remove_child(index)?;
        }
        PatchInternal::UpdateText(new_text) => {
          self.update_text(new_text)?;
        }
        PatchInternal::None => {}
      }
    }
    Ok(())
  }

  /// Renders a single `VNode` to the terminal.
  pub fn render_node(&self, node: &VirtualVNode) -> Result<()> {
    match node {
      VirtualVNode::Text(text) => {
        self.render_text(text)?;
      }
      VirtualVNode::Element(elem) => {
        self.render_element(elem)?;
      }
    }
    Ok(())
  }

  /// Renders a text node with styles to the terminal.
  fn render_text(&self, text: &str) -> Result<()> {
    print!("{}", text);
    io::stdout()
      .flush()
      .map_err(|e| napi::Error::from_reason(format!("IO Error: {}", e)))?;
    Ok(())
  }

  /// Renders a `VElement` based on its tag, properties, and styles.
  pub fn render_element(&self, elem: &VirtualVElement) -> Result<()> {
    match elem.tag.as_str() {
      "text" => {
        let styled_content = self.apply_styles(elem)?;
        if let Some(content) = elem.props.get("content") {
          // Removed `.0`
          print!("{}{}\x1B[0m", styled_content, content);
          io::stdout()
            .flush()
            .map_err(|e| napi::Error::from_reason(format!("IO Error: {}", e)))?;
        }
      }
      _ => {
        for child in &elem.children {
          self.render_node(child)?;
        }
      }
    }
    Ok(())
  }

  /// Applies styles to the content using ANSI escape codes.
  fn apply_styles(&self, elem: &VirtualVElement) -> Result<String> {
    let mut style_codes = String::new();

    // Apply styles using ANSI escape codes
    for (key, value) in &elem.styles {
      // Removed `.0`
      match key.as_str() {
        "color" => {
          let color_code = match value.as_str() {
            "black" => "30",
            "red" => "31",
            "green" => "32",
            "yellow" => "33",
            "blue" => "34",
            "magenta" => "35",
            "cyan" => "36",
            "white" => "37",
            _ => "0",
          };
          style_codes.push_str(&format!("\x1B[{}m", color_code));
        }
        "background" => {
          let bg_color_code = match value.as_str() {
            "black" => "40",
            "red" => "41",
            "green" => "42",
            "yellow" => "43",
            "blue" => "44",
            "magenta" => "45",
            "cyan" => "46",
            "white" => "47",
            _ => "0",
          };
          style_codes.push_str(&format!("\x1B[{}m", bg_color_code));
        }
        "bold" if value == "true" => {
          style_codes.push_str("\x1B[1m");
        }
        "underline" if value == "true" => {
          style_codes.push_str("\x1B[4m");
        }
        "italic" if value == "true" => {
          style_codes.push_str("\x1B[3m");
        }
        "strikethrough" if value == "true" => {
          style_codes.push_str("\x1B[9m");
        }
        _ => {}
      }
    }

    Ok(style_codes)
  }

  /// Clears the terminal screen.
  #[napi]
  pub fn clear_screen(&self) -> Result<()> {
    print!("\x1B[2J\x1B[1;1H");
    io::stdout()
      .flush()
      .map_err(|e| napi::Error::from_reason(format!("IO Error: {}", e)))?;
    Ok(())
  }

  /// Appends a child `VNode` to the current node.
  pub fn append_child(&self, node: &VirtualVNode) -> Result<()> {
    self.render_node(node)?;
    Ok(())
  }

  /// Removes a child node at the specified index.
  ///
  /// # Parameters
  ///
  /// - `index`: The index of the child to remove.
  #[napi]
  pub fn remove_child(&self, index: u32) -> Result<()> {
    let mut rendered = self.rendered_nodes.lock().unwrap();
    if rendered.remove(&index).is_some() {
      self.clear_screen()?;
      // Re-render remaining nodes if necessary
      for node in rendered.values() {
        self.render_node(node)?;
      }
    }
    Ok(())
  }

  /// Updates the text content of a text node.
  ///
  /// # Parameters
  ///
  /// - `new_text`: The new text content.
  #[napi]
  pub fn update_text(&self, new_text: String) -> Result<()> {
    self.clear_screen();
    self.render_text(&new_text)?;
    Ok(())
  }

  /// Updates the properties of an element.
  ///
  /// # Parameters
  ///
  /// - `props`: A reference to a `WrappedHashMap` containing the properties to update.
  #[napi]
  pub fn update_props(&self, props: &WrappedHashMap) -> Result<()> {
    for (key, value) in &props.0 {
      print!("{}: {}\n", key, value);
    }
    io::stdout()
      .flush()
      .map_err(|e| napi::Error::from_reason(format!("IO Error: {}", e)))?;
    Ok(())
  }

  /// Renders the entire Virtual DOM to the terminal.
  #[napi(js_name = "renderAll")]
  pub fn render_all(&self) -> Result<()> {
    self.clear_screen();
    let root = self.root.0.lock().unwrap();
    self.render_node(&root)?;
    Ok(())
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::virtual_dom::{VElement as VirtualVElement, VNode as VirtualVNode};
  use std::collections::HashMap;

  #[test]
  fn test_renderer_new() {
    let props = HashMap::new();
    let styles = HashMap::new();
    let root = VirtualVNode::Element(VirtualVElement {
      tag: "root".to_string(),
      props,
      styles,
      children: vec![],
    });
    let renderer = Renderer::new(root.clone());
    assert_eq!(*renderer.root.0.lock().unwrap(), root);
  }

  #[test]
  fn test_renderer_render() {
    let renderer = Renderer::new(VirtualVNode::Text("Old Text".to_string()));
    let new_vdom = VirtualVNode::Text("New Text".to_string());
    assert!(renderer.render(new_vdom.clone()).is_ok());
    assert_eq!(*renderer.root.0.lock().unwrap(), new_vdom);
  }

  #[test]
  fn test_renderer_apply_patches() {
    let old = VirtualVNode::Text("Hello".to_string());
    let new = VirtualVNode::Text("Hello, World!".to_string());
    let patches = diff(&old, &new);
    let renderer = Renderer::new(old.clone());
    assert!(renderer.apply_patches(&patches).is_ok());
    assert_eq!(*renderer.root.0.lock().unwrap(), new);
  }

  #[test]
  fn test_renderer_clear_screen() {
    let renderer = Renderer::new(VirtualVNode::Text("Hello".to_string()));
    assert!(renderer.clear_screen().is_ok());
  }

  #[test]
  fn test_renderer_update_text() {
    let renderer = Renderer::new(VirtualVNode::Text("Hello".to_string()));
    assert!(renderer.update_text("Updated Text".to_string()).is_ok());
  }

  #[test]
  fn test_renderer_update_props() {
    let renderer = Renderer::new(VirtualVNode::Text("Hello".to_string()));
    let mut props_map = HashMap::new();
    props_map.insert("color".to_string(), "red".to_string());
    props_map.insert("background".to_string(), "blue".to_string());
    let props = WrappedHashMap(props_map);
    assert!(renderer.update_props(&props).is_ok());
  }

  #[test]
  fn test_renderer_render_all() {
    let props = HashMap::new();
    let styles = HashMap::new();
    let root = VirtualVNode::Element(VirtualVElement {
      tag: "div".to_string(),
      props: props.clone(),
      styles: styles.clone(),
      children: vec![
        VirtualVNode::Text("Hello".to_string()),
        VirtualVNode::Element(VirtualVElement {
          tag: "span".to_string(),
          props: props.clone(),
          styles: styles.clone(),
          children: vec![VirtualVNode::Text("World".to_string())],
        }),
      ],
    });
    let renderer = Renderer::new(root.clone());
    assert!(renderer.render_all().is_ok());
  }
}

// cSpell:ignore vnode vdom
