// src/virtual_dom/renderer.rs

use crate::virtual_dom::diff::{diff, Patch as DiffPatch, Patch};
use crate::virtual_dom::{VElement as VirtualElement, VNode as VirtualVNode};
use napi::bindgen_prelude::*;
use napi::{JsObject, JsString, JsUnknown, NapiValue};
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
        let obj = JsObject::from_raw(env, napi_val)?;
        let keys = obj.get_property_names()?.into_vec(env)?;
        let keys: Vec<JsString> = keys
            .into_iter()
            .map(|raw_key| {
                let unknown = JsUnknown::from_raw(env, raw_key)?;
                unknown.coerce_to_string()
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
        let obj = JsObject::from_napi_value(env, napi_val)?;
        let keys = obj.get_property_names()?.into_vec(env)?;
        let keys: Vec<JsString> = keys
            .into_iter()
            .map(|raw_key| {
                let unknown = JsUnknown::from_napi_value(env, raw_key)?;
                unknown.coerce_to_string()
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
            let mut key: napi::sys::napi_value = std::mem::zeroed();
            let status =
                napi::sys::napi_create_string_utf8(env, k.as_ptr() as *const _, k.len(), &mut key);
            if status != napi::sys::Status::napi_ok {
                return Err(napi::Error::from_reason("Failed to create string for key"));
            }

            let mut value: napi::sys::napi_value = std::mem::zeroed();
            let status =
                napi::sys::napi_create_string_utf8(env, v.as_ptr() as *const _, v.len(), &mut value);
            if status != napi::sys::Status::napi_ok {
                return Err(napi::Error::from_reason(
                    "Failed to create string for value",
                ));
            }

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
        let vnode = val.0.lock().unwrap().clone();
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

/// Internal representation of a Patch.
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
#[napi]
pub struct Renderer {
    pub root: WrappedVNode,
    rendered_nodes: Arc<Mutex<HashMap<u32, VirtualVNode>>>,
}

#[napi]
impl Renderer {
    /// Creates a new `Renderer` instance with the given root `VirtualVNode`.
    ///
    /// # Parameters
    ///
    /// - `root`: The root node of the Virtual DOM.
    ///
    /// # Example
    ///
    /// ```javascript
    /// const renderer = new Renderer(rootVNode);
    /// ```
    #[napi(constructor)]
    pub fn new(root: VirtualVNode) -> Self {
        Renderer {
            root: WrappedVNode(Arc::new(Mutex::new(root))),
            rendered_nodes: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Renders a new Virtual DOM root node by computing and applying patches.
    ///
    /// # Parameters
    ///
    /// - `new_root`: The new root `VirtualVNode` to render.
    ///
    /// # Returns
    ///
    /// A `Result` indicating success or failure.
    ///
    /// # Example
    ///
    /// ```javascript
    /// renderer.render(newRootVNode);
    /// ```
    #[napi]
    pub fn render(&self, new_root: VirtualVNode) -> Result<()> {
        let current_root = {
            let root_guard = self.root.0.lock().unwrap();
            root_guard.clone()
        };

        let patches = diff(&current_root, &new_root);

        self.apply_patches(&patches)?;

        // Update the root after applying patches
        let mut root_guard = self.root.0.lock().unwrap();
        *root_guard = new_root;

        Ok(())
    }

    /// Applies a list of patches to the Virtual DOM and updates the renderer accordingly.
    ///
    /// # Parameters
    ///
    /// - `patches`: A slice of `Patch` enums representing the changes to apply.
    ///
    /// # Returns
    ///
    /// A `Result` indicating success or failure.
    ///
    /// # Example
    ///
    /// ```rust
    /// renderer.apply_patches(&patches)?;
    /// ```
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

        // Lock the root once for the duration of applying all patches
        let mut root_guard = self.root.0.lock().unwrap();

        for patch in internal_patches {
            match patch {
                PatchInternal::Replace(node) => {
                    // Replace the current node with the new node
                    *root_guard = node.clone();
                    self.render_node(&node)?;
                }
                PatchInternal::UpdateProps(props) => {
                    self.update_props(&mut root_guard, &props)?;
                }
                PatchInternal::AppendChild(node) => {
                    self.append_child(&mut root_guard, node)?;
                }
                PatchInternal::RemoveChild(index) => {
                    self.remove_child(&mut root_guard, index)?;
                }
                PatchInternal::UpdateText(new_text) => {
                    self.update_text(&mut root_guard, new_text)?;
                }
                PatchInternal::None => {}
            }
        }

        Ok(())
    }

    /// Renders a single `VirtualVNode` by delegating to appropriate rendering methods.
    ///
    /// # Parameters
    ///
    /// - `node`: The `VirtualVNode` to render.
    ///
    /// # Returns
    ///
    /// A `Result` indicating success or failure.
    ///
    /// # Example
    ///
    /// ```rust
    /// renderer.render_node(&node)?;
    /// ```
    pub fn render_node(&self, node: &VirtualVNode) -> Result<()> {
        match node {
            VirtualVNode::Text(text) => self.render_text(text),
            VirtualVNode::Element(elem) => {
                self.render_element(elem)
            }
        }
    }

    /// Renders a text node by printing its content to the terminal.
    ///
    /// # Parameters
    ///
    /// - `text`: The text content to render.
    ///
    /// # Returns
    ///
    /// A `Result` indicating success or failure.
    ///
    /// # Example
    ///
    /// ```rust
    /// renderer.render_text("Hello, World!")?;
    /// ```
    fn render_text(&self, text: &str) -> Result<()> {
        #[cfg(not(test))]
        {
            print!("{}", text);
            io::stdout()
                .flush()
                .map_err(|e| napi::Error::from_reason(format!("IO Error: {}", e)))
        }

        #[cfg(test)]
        {
            // No-op during tests to prevent I/O
            Ok(())
        }
    }

    /// Renders an element node by applying styles and rendering its children.
    ///
    /// # Parameters
    ///
    /// - `elem`: The `VirtualVElement` to render.
    ///
    /// # Returns
    ///
    /// A `Result` indicating success or failure.
    ///
    /// # Example
    ///
    /// ```rust
    /// renderer.render_element(&element)?;
    /// ```
    pub fn render_element(&self, elem: &VirtualElement) -> Result<()> {
        match elem.tag.as_str() {
            "text" => {
                let styled_content = self.apply_styles(elem)?;
                if let Some(content) = elem.props.get("content") {
                    #[cfg(not(test))]
                    {
                        print!("{}{}\x1B[0m", styled_content, content);
                        io::stdout()
                            .flush()
                            .map_err(|e| napi::Error::from_reason(format!("IO Error: {}", e)))?;
                    }
                }
                Ok(())
            }
            _ => {
                for child in &elem.children {
                    self.render_node(child)?;
                }
                Ok(())
            }
        }
    }

    /// Applies ANSI styles to the given `VirtualVElement`.
    ///
    /// # Parameters
    ///
    /// - `elem`: The `VirtualVElement` whose styles are to be applied.
    ///
    /// # Returns
    ///
    /// A `Result` containing the ANSI escape codes as a `String` or an error.
    ///
    /// # Example
    ///
    /// ```rust
    /// let styles = renderer.apply_styles(&element)?;
    /// ```
    fn apply_styles(&self, elem: &VirtualElement) -> Result<String> {
        let mut style_codes = String::new();

        for (key, value) in &elem.styles {
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
                "bold" if value == "true" => style_codes.push_str("\x1B[1m"),
                "underline" if value == "true" => style_codes.push_str("\x1B[4m"),
                "italic" if value == "true" => style_codes.push_str("\x1B[3m"),
                "strikethrough" if value == "true" => style_codes.push_str("\x1B[9m"),
                _ => {}
            }
        }

        Ok(style_codes)
    }

    /// Retrieves the current state of the Virtual DOM.
    ///
    /// # Returns
    ///
    /// A `Result` containing the current `VirtualVNode` or an error.
    ///
    /// # Example
    ///
    /// ```rust
    /// let current_state = renderer.get_current_state()?;
    /// ```
    pub fn get_current_state(&self) -> Result<VirtualVNode> {
        let root = self.root.0.lock().map_err(|e| {
            Error::new(
                Status::GenericFailure,
                format!("Failed to lock root node: {}", e),
            )
        })?;
        Ok((*root).clone())
    }

    /// Clears the terminal screen.
    ///
    /// # Returns
    ///
    /// A `Result` indicating success or failure.
    ///
    /// # Example
    ///
    /// ```rust
    /// renderer.clear_screen()?;
    /// ```
    #[napi]
    pub fn clear_screen(&self) -> Result<()> {
        #[cfg(not(test))]
        {
            print!("\x1B[2J\x1B[1;1H");
            io::stdout()
                .flush()
                .map_err(|e| napi::Error::from_reason(format!("IO Error: {}", e)))
        }

        #[cfg(test)]
        {
            // No-op during tests to prevent I/O
            Ok(())
        }
    }

    /// Updates the text content of the given node and re-renders it.
    ///
    /// # Parameters
    ///
    /// - `node`: A mutable reference to the `VirtualVNode` to update.
    /// - `new_text`: The new text content to set.
    ///
    /// # Returns
    ///
    /// A `Result` indicating success or failure.
    ///
    /// # Example
    ///
    /// ```rust
    /// renderer.update_text(&mut vnode, "New Text".to_string())?;
    /// ```
    pub fn update_text(&self, node: &mut VirtualVNode, new_text: String) -> Result<()> {
        self.clear_screen()?;
        self.render_text(&new_text)?;

        // Update the node's 'content' property
        if let VirtualVNode::Element(ref mut elem) = node {
            elem.props.insert("content".to_string(), new_text);
        } else {
            return Err(napi::Error::from_reason(
                "Cannot update text on a text node".to_string(),
            ));
        }

        Ok(())
    }

    /// Updates the properties of the given node and re-renders the changes.
    ///
    /// # Parameters
    ///
    /// - `node`: A mutable reference to the `VirtualVNode` to update.
    /// - `props`: A reference to a `WrappedHashMap` containing the properties to update.
    ///
    /// # Returns
    ///
    /// A `Result` indicating success or failure.
    ///
    /// # Example
    ///
    /// ```rust
    /// let props = WrappedHashMap(HashMap::from([("class".to_string(), "active".to_string())]));
    /// renderer.update_props(&mut vnode, &props)?;
    /// ```
    pub fn update_props(&self, node: &mut VirtualVNode, props: &WrappedHashMap) -> Result<()> {
        // Update the node's props
        if let VirtualVNode::Element(ref mut elem) = node {
            for (key, value) in &props.0 {
                elem.props.insert(key.clone(), value.clone());
            }
        } else {
            return Err(napi::Error::from_reason(
                "Cannot update props on a text node".to_string(),
            ));
        }

        // Render the updated props (optional: remove if not needed)
        for (key, value) in &props.0 {
            print!("{}: {}\n", key, value);
        }
        io::stdout()
            .flush()
            .map_err(|e| napi::Error::from_reason(format!("IO Error: {}", e)))
    }

    /// Appends a child `VirtualVNode` to the given node.
    ///
    /// # Parameters
    ///
    /// - `node`: A mutable reference to the `VirtualVNode` to append the child to.
    /// - `child`: The `VirtualVNode` to append as a child.
    ///
    /// # Returns
    ///
    /// A `Result` indicating success or failure.
    ///
    /// # Example
    ///
    /// ```rust
    /// let child = VirtualVNode::text("Child Node".to_string());
    /// renderer.append_child(&mut vnode, child.clone())?;
    /// ```
    pub fn append_child(&self, node: &mut VirtualVNode, child: VirtualVNode) -> Result<()> {
        self.render_node(&child)?;

        // Append to the node's children
        if let VirtualVNode::Element(ref mut elem) = node {
            elem.children.push(child);
            Ok(())
        } else {
            Err(napi::Error::from_reason(
                "Cannot append child to a text node".to_string(),
            ))
        }
    }

    /// Removes a child node at the specified index from the given node.
    ///
    /// # Parameters
    ///
    /// - `node`: A mutable reference to the `VirtualVNode` to remove the child from.
    /// - `index`: The index of the child to remove.
    ///
    /// # Returns
    ///
    /// A `Result` indicating success or failure.
    ///
    /// # Example
    ///
    /// ```rust
    /// renderer.remove_child(&mut vnode, 0)?;
    /// ```
    pub fn remove_child(&self, node: &mut VirtualVNode, index: u32) -> Result<()> {
        // Remove from the node's children
        if let VirtualVNode::Element(ref mut elem) = node {
            if (index as usize) < elem.children.len() {
                elem.children.remove(index as usize);
            } else {
                return Err(napi::Error::from_reason(format!(
                    "Child index {} out of bounds",
                    index
                )));
            }
        } else {
            return Err(napi::Error::from_reason(
                "Cannot remove child from a text node".to_string(),
            ));
        }

        // Optionally, clear the screen and re-render if necessary
        #[cfg(not(test))]
        {
            self.clear_screen()?;
            self.render_node(node)?;
        }

        Ok(())
    }

    /// Renders all nodes by clearing the screen and rendering the root node.
    ///
    /// # Returns
    ///
    /// A `Result` indicating success or failure.
    ///
    /// # Example
    ///
    /// ```rust
    /// renderer.render_all()?;
    /// ```
    #[napi(js_name = "renderAll")]
    pub fn render_all(&self) -> Result<()> {
        self.clear_screen()?;
        let root = self.root.0.lock().unwrap().clone();
        self.render_node(&root)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::virtual_dom::diff::{diff, Patch};
    use crate::virtual_dom::{VElement as VirtualElement, VNode as VirtualVNode};
    use std::collections::HashMap;
    use std::sync::Arc;

    fn create_test_vnode(content: &str) -> VirtualVNode {
        VirtualVNode::Element(VirtualElement {
            tag: "text".to_string(),
            props: {
                let mut props = HashMap::new();
                props.insert("content".to_string(), content.to_string());
                props
            },
            styles: HashMap::new(),
            children: vec![],
        })
    }

    fn create_element_vnode(tag: &str, children: Vec<VirtualVNode>) -> VirtualVNode {
        VirtualVNode::Element(VirtualElement {
            tag: tag.to_string(),
            props: HashMap::new(),
            styles: HashMap::new(),
            children,
        })
    }

    // Helper function to verify renderer state
    fn verify_renderer_state(renderer: &Renderer, expected: &VirtualVNode) -> bool {
        match renderer.get_current_state() {
            Ok(state) => {
                if &state == expected {
                    true
                } else {
                    println!("Node comparison failed!");
                    println!("Expected: {:?}", expected);
                    println!("Got: {:?}", state);
                    false
                }
            }
            Err(e) => {
                println!("Failed to get current state: {}", e);
                false
            }
        }
    }

    /// Test that `Renderer` initializes correctly with the given root node.
    #[test]
    fn test_renderer_new() {
        let root = create_test_vnode("Initial");
        let renderer = Renderer::new(root.clone());
        assert_eq!(*renderer.root.0.lock().unwrap(), root);
    }

    /// Test that `Renderer` correctly renders a new root node.
    #[test]
    fn test_renderer_render() {
        let renderer = Renderer::new(create_test_vnode("Initial"));
        let new_vdom = create_test_vnode("Updated");
        assert!(renderer.render(new_vdom.clone()).is_ok());
        assert_eq!(*renderer.root.0.lock().unwrap(), new_vdom);
    }

    /// Test that `Renderer` correctly applies patches to update the root node.
    #[test]
    fn test_renderer_apply_patches() {
        // Initial state
        let initial_vdom = create_test_vnode("Initial content");
        let renderer = Renderer::new(initial_vdom.clone());

        // Updated state
        let updated_vdom = create_test_vnode("Updated content");
        let patches = diff(&initial_vdom, &updated_vdom);

        // Apply patches and update state
        assert!(
            renderer.apply_patches(&patches).is_ok(),
            "Failed to apply patches"
        );
        assert!(
            verify_renderer_state(&renderer, &updated_vdom),
            "Renderer state does not match expected state after applying patches"
        );

        // Test Replace patch
        let replacement_vdom = create_test_vnode("Replacement content");
        let replace_patches = vec![Patch::Replace(replacement_vdom.clone())];
        assert!(renderer.apply_patches(&replace_patches).is_ok());
        assert!(verify_renderer_state(&renderer, &replacement_vdom));
    }

    /// Test that `Renderer` can clear the screen without errors.
    #[test]
    fn test_renderer_clear_screen() {
        let renderer = Renderer::new(create_test_vnode("Test"));
        assert!(renderer.clear_screen().is_ok());
    }

    /// Test that `Renderer` correctly updates the text content of the root node.
    #[test]
    fn test_renderer_update_text() {
        let renderer = Renderer::new(create_test_vnode("Initial"));
        let new_text = "Updated text".to_string();

        // Update text and state
        assert!(renderer.render(create_test_vnode(&new_text)).is_ok());
        let expected_vdom = create_test_vnode(&new_text);
        assert!(verify_renderer_state(&renderer, &expected_vdom));

        // Verify update
        let root_guard = renderer.root.0.lock().unwrap();
        if let VirtualVNode::Element(elem) = &*root_guard {
            assert_eq!(
                elem.props.get("content").unwrap(),
                &new_text,
                "Content was not updated correctly"
            );
        } else {
            panic!("Unexpected node type");
        }
    }

    /// Test that `Renderer` correctly updates the properties of the root node.
    #[test]
    fn test_renderer_update_props() {
        let renderer = Renderer::new(create_test_vnode("Test"));
        let mut props = HashMap::new();
        props.insert("class".to_string(), "new-class".to_string());
        props.insert("id".to_string(), "test-id".to_string());
        let wrapped_props = WrappedHashMap(props);
        assert!(renderer.update_props(&mut renderer.root.0.lock().unwrap(), &wrapped_props).is_ok());

        // Verify that props are updated
        let root_guard = renderer.root.0.lock().unwrap();
        if let VirtualVNode::Element(ref elem) = *root_guard {
            assert_eq!(elem.props.get("class").unwrap(), "new-class");
            assert_eq!(elem.props.get("id").unwrap(), "test-id");
        } else {
            panic!("Unexpected node type");
        }
    }

    /// Test that `Renderer` correctly applies specific patches like UpdateText, UpdateProps, AppendChild, and RemoveChild.
    #[test]
    fn test_renderer_specific_patches() {
        let initial_vdom = create_test_vnode("Initial");
        let renderer = Renderer::new(initial_vdom.clone());

        // Test UpdateText patch
        let updated_vdom = create_test_vnode("New text");
        let patches = diff(&initial_vdom, &updated_vdom);
        assert!(renderer.apply_patches(&patches).is_ok());
        assert!(verify_renderer_state(&renderer, &updated_vdom));

        // Test UpdateProps patch
        let mut props = HashMap::new();
        props.insert("class".to_string(), "new-class".to_string());
        let props_patch = vec![Patch::UpdateProps(props)];
        assert!(renderer.apply_patches(&props_patch).is_ok());

        // Expected VDOM after UpdateProps
        let expected_vdom = {
            let mut vnode = create_test_vnode("New text");
            if let VirtualVNode::Element(ref mut elem) = vnode {
                elem.props.insert("class".to_string(), "new-class".to_string());
            }
            vnode
        };
        assert!(verify_renderer_state(&renderer, &expected_vdom));

        // Test AppendChild patch
        let child = create_test_vnode("Child");
        let append_patch = vec![Patch::AppendChild(child.clone())];
        assert!(renderer.apply_patches(&append_patch).is_ok());

        // Expected VDOM after AppendChild
        let expected_vdom = {
            let mut vnode = create_test_vnode("New text");
            if let VirtualVNode::Element(ref mut elem) = vnode {
                elem.props.insert("class".to_string(), "new-class".to_string());
                elem.children.push(child.clone());
            }
            vnode
        };
        assert!(verify_renderer_state(&renderer, &expected_vdom));

        // Test RemoveChild patch
        let remove_patch = vec![Patch::RemoveChild(0)];
        assert!(renderer.apply_patches(&remove_patch).is_ok());

        // Expected VDOM after RemoveChild
        let expected_vdom = create_test_vnode("New text");
        {
            let mut vnode = create_test_vnode("New text");
            if let VirtualVNode::Element(ref mut elem) = vnode {
                elem.props.insert("class".to_string(), "new-class".to_string());
                // No children after removal
            }
            assert!(verify_renderer_state(&renderer, &vnode));
        }
    }

    /// Test that `Renderer` correctly handles a complex tree structure.
    #[test]
    fn test_renderer_complex_tree() {
        let root = create_element_vnode(
            "div",
            vec![
                create_test_vnode("Child 1"),
                create_element_vnode("span", vec![create_test_vnode("Nested Child")]),
                create_test_vnode("Child 2"),
            ],
        );

        let renderer = Renderer::new(root.clone());
        assert!(renderer.render_all().is_ok());
        assert!(verify_renderer_state(&renderer, &root));
    }

    /// Test that `Renderer` correctly applies styles to elements.
    #[test]
    fn test_renderer_style_application() {
        let mut props = HashMap::new();
        props.insert("content".to_string(), "Styled Text".to_string());

        let mut styles = HashMap::new();
        styles.insert("color".to_string(), "red".to_string());
        styles.insert("bold".to_string(), "true".to_string());

        let styled_node = VirtualVNode::Element(VirtualElement {
            tag: "text".to_string(),
            props,
            styles,
            children: vec![],
        });

        let renderer = Renderer::new(styled_node.clone());
        assert!(renderer.render_all().is_ok());
        assert!(verify_renderer_state(&renderer, &styled_node));
    }

    /// Test that `Renderer` handles invalid styles gracefully without crashing.
    #[test]
    fn test_renderer_error_handling() {
        // Create initial renderer with empty styles
        let renderer = Renderer::new(create_test_vnode("Test"));

        // Create test styles
        let mut styles = HashMap::new();
        styles.insert("invalid_style".to_string(), "value".to_string());

        // Create a new node with our test styles
        let styled_node = VirtualVNode::Element(VirtualElement {
            tag: "text".to_string(),
            props: HashMap::new(),
            styles: styles.clone(),
            children: vec![],
        });

        // Render the styled node using the render method to update the root node
        assert!(renderer.render(styled_node.clone()).is_ok());

        // Verify styles were preserved
        let root_guard = renderer.root.0.lock().unwrap();
        if let VirtualVNode::Element(elem) = &*root_guard {
            // Verify style count
            assert_eq!(
                elem.styles.len(),
                1,
                "Expected 1 style entry (invalid_style), got {}",
                elem.styles.len()
            );

            // Verify style content
            assert_eq!(
                elem.styles.get("invalid_style"),
                Some(&"value".to_string()),
                "Invalid style should be preserved"
            );
        } else {
            panic!("Root node should be an Element");
        }
    }

    /// Test that `Renderer` can handle concurrent access without deadlocks or state inconsistencies.
    #[test]
    fn test_renderer_concurrent_access() {
        let renderer = Arc::new(Renderer::new(create_test_vnode("Initial")));
        let renderer_clone = Arc::clone(&renderer);

        let handle = std::thread::spawn(move || {
            let new_vdom = create_test_vnode("Updated from thread");
            renderer_clone.render(new_vdom).is_ok()
        });

        let main_vdom = create_test_vnode("Updated from main");
        assert!(renderer.render(main_vdom.clone()).is_ok());

        assert!(handle.join().unwrap());

        // Depending on thread scheduling, the final state could be either
        // "Updated from main" or "Updated from thread". We'll allow both.
        let final_state = renderer.get_current_state().unwrap();

        let expected_main = create_test_vnode("Updated from main");
        let expected_thread = create_test_vnode("Updated from thread");

        assert!(
            final_state == expected_main || final_state == expected_thread,
            "Final state should be either 'Updated from main' or 'Updated from thread', got: {:?}",
            final_state
        );
    }

    /// Test that `Renderer` correctly performs batch updates.
    #[test]
    fn test_renderer_batch_updates() {
        let renderer = Renderer::new(create_test_vnode("Initial"));

        let updates = vec![
            create_test_vnode("Update 1"),
            create_test_vnode("Update 2"),
            create_test_vnode("Update 3"),
        ];

        for update in updates {
            assert!(renderer.render(update.clone()).is_ok());
            assert!(verify_renderer_state(&renderer, &update));
        }
    }

    /// Test that `Renderer` correctly handles empty and no-op patches without errors.
    #[test]
    fn test_renderer_empty_updates() {
        let renderer = Renderer::new(create_test_vnode("Initial"));

        // Empty patches should be handled gracefully
        let empty_patches: Vec<Patch> = vec![];
        assert!(renderer.apply_patches(&empty_patches).is_ok());

        // None patch should be handled gracefully
        let none_patches = vec![Patch::None];
        assert!(renderer.apply_patches(&none_patches).is_ok());
    }

    /// Test that `Renderer` correctly handles sequential updates without state inconsistencies.
    #[test]
    fn test_renderer_sequential_updates() {
        let renderer = Renderer::new(create_test_vnode("Initial"));

        // Test multiple sequential updates
        let updates = [
            ("First update", "class-1"),
            ("Second update", "class-2"),
            ("Third update", "class-3"),
        ];

        for (content, class) in updates.iter() {
            let mut props = HashMap::new();
            props.insert("class".to_string(), class.to_string());
            let mut vdom = create_test_vnode(content);
            if let VirtualVNode::Element(ref mut elem) = vdom {
                elem.props.insert("class".to_string(), class.to_string());
            }
            assert!(renderer.render(vdom.clone()).is_ok());
            assert!(verify_renderer_state(&renderer, &vdom));
        }
    }
}
