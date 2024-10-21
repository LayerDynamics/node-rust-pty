// src/virtual_dom/virtual_dom_test.rs

#[cfg(test)]
mod tests {
  use super::super::diff::{diff, Patch};
  use super::super::virtual_dom::{VElement, VNode};
  use std::collections::HashMap;

  #[test]
  fn test_vnode_equality() {
    let v1 = VNode::text("Hello");
    let v2 = VNode::text("Hello");
    assert_eq!(v1, v2);
  }

  #[test]
  fn test_diff_replace_text() {
    let old = VNode::text("Hello");
    let new = VNode::text("World");
    let patches = diff(&old, &new);
    assert_eq!(patches.len(), 1);
    match &patches[0] {
      Patch::Replace(node) => {
        assert_eq!(node, &new);
      }
      _ => panic!("Expected Replace patch"),
    }
  }

  #[test]
  fn test_diff_update_props() {
    let mut old_props = HashMap::new();
    old_props.insert("color".to_string(), "red".to_string());

    let mut new_props = HashMap::new();
    new_props.insert("color".to_string(), "blue".to_string());

    let old_vdom = VNode::element("text", old_props.clone(), vec![]);
    let new_vdom = VNode::element("text", new_props.clone(), vec![]);

    let patches = diff(&old_vdom, &new_vdom);
    assert_eq!(patches.len(), 1);
    match &patches[0] {
      Patch::UpdateProps(props) => {
        assert_eq!(props.get("color"), Some(&"blue".to_string()));
      }
      _ => panic!("Expected UpdateProps patch"),
    }
  }

  #[test]
  fn test_diff_append_child() {
    let old_vdom = VNode::element("box", HashMap::new(), vec![]);
    let new_child = VNode::text("Child");
    let mut new_children = vec![];
    new_children.push(new_child.clone());

    let new_vdom = VNode::element("box", HashMap::new(), new_children.clone());

    let patches = diff(&old_vdom, &new_vdom);
    assert_eq!(patches.len(), 1);
    match &patches[0] {
      Patch::AppendChild(node) => {
        assert_eq!(node, &new_child);
      }
      _ => panic!("Expected AppendChild patch"),
    }
  }

  #[test]
  fn test_renderer_replace() {
    use super::super::renderer::Renderer;

    let initial_vdom = VNode::text("Hello");
    let mut renderer = Renderer::new(initial_vdom.clone());

    let new_vdom = VNode::text("World");
    renderer.render(new_vdom.clone());

    // Since the Renderer doesn't expose internal state, manual verification is needed.
    // Alternatively, extend the Renderer to allow inspection for testing purposes.
  }

  // Additional tests can be added to cover more scenarios
}
