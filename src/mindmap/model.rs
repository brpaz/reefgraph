use std::collections::HashMap;

use crate::geometry::Point;
use crate::syntax;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Shape {
    /// Plain text without an id.
    #[default]
    Default,
    /// `id[text]`
    Square,
    /// `id(text)`
    Rounded,
    /// `id((text))`
    Circle,
    /// `id))text((`
    Bang,
    /// `id)text(`
    Cloud,
    /// `id{{text}}`
    Hexagon,
}

impl Shape {
    pub const ALL: [Shape; 7] = [
        Shape::Default,
        Shape::Square,
        Shape::Rounded,
        Shape::Circle,
        Shape::Bang,
        Shape::Cloud,
        Shape::Hexagon,
    ];

    /// Opening and closing delimiters around the text.
    pub fn delimiters(self) -> (&'static str, &'static str) {
        match self {
            Shape::Default => ("", ""),
            Shape::Square => ("[", "]"),
            Shape::Rounded => ("(", ")"),
            Shape::Circle => ("((", "))"),
            Shape::Bang => ("))", "(("),
            Shape::Cloud => (")", "("),
            Shape::Hexagon => ("{{", "}}"),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Node {
    /// Handle the editor refers to the node by; not written to the source,
    /// where plain-text nodes have no id.
    pub key: String,
    /// Id written before a shaped node's text.
    pub id: String,
    /// Text as written: plain, a `"string"` or a "`markdown string`".
    pub text: String,
    pub shape: Shape,
    /// Key of the parent; `None` for the root.
    pub parent: Option<String>,
    /// `::icon(…)`, `:::class` and comment lines written after the node.
    pub extras: Vec<String>,
}

impl Node {
    /// Text shown in the node.
    pub fn display(&self) -> String {
        syntax::label_text(&self.text)
            .replace("<br/>", "\n")
            .replace("<br>", "\n")
    }

    /// Sets text typed by the user, quoting it when it holds characters
    /// that would end the shape or be read as one, or would make the line
    /// an icon, class or comment.
    pub fn set_text(&mut self, text: &str) {
        let text = text.replace('\n', "<br/>");
        let reserved = text.contains(['(', ')', '[', ']', '{', '}', '"'])
            || text.starts_with(':')
            || text.starts_with("%%");
        self.text = if reserved {
            syntax::quote_label(&text)
        } else {
            text
        };
    }
}

/// Nodes in source order; siblings keep their relative order.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Mindmap {
    pub nodes: Vec<Node>,
    /// Lines before the declaration.
    pub preamble: Vec<String>,
    /// Comments between the declaration and the root.
    pub leading: Vec<String>,
    /// Canvas position of the root, which the layout grows from. Mermaid
    /// does not store it; `None` places the root at the canvas origin.
    pub origin: Option<Point>,
}

impl Mindmap {
    pub fn node(&self, key: &str) -> Option<&Node> {
        self.nodes.iter().find(|n| n.key == key)
    }

    pub fn node_mut(&mut self, key: &str) -> Option<&mut Node> {
        self.nodes.iter_mut().find(|n| n.key == key)
    }

    pub fn root(&self) -> Option<&Node> {
        self.nodes.iter().find(|n| n.parent.is_none())
    }

    pub fn children<'a>(&'a self, key: &str) -> impl Iterator<Item = &'a Node> + use<'a> {
        let key = key.to_string();
        self.nodes
            .iter()
            .filter(move |n| n.parent.as_deref() == Some(key.as_str()))
    }

    /// Children of every node, keyed by parent key, in sibling order. Build
    /// it once where a pass visits the whole tree; `children` scans all
    /// nodes on every call.
    pub fn children_index(&self) -> HashMap<&str, Vec<&Node>> {
        let mut index: HashMap<&str, Vec<&Node>> = HashMap::new();
        for node in &self.nodes {
            if let Some(parent) = &node.parent {
                index.entry(parent.as_str()).or_default().push(node);
            }
        }
        index
    }

    /// Whether `key` is `ancestor` or lies below it.
    pub fn is_within(&self, key: &str, ancestor: &str) -> bool {
        let mut current = Some(key.to_string());
        for _ in 0..=self.nodes.len() {
            match current {
                Some(k) if k == ancestor => return true,
                Some(k) => current = self.node(&k).and_then(|n| n.parent.clone()),
                None => return false,
            }
        }
        false
    }

    pub fn next_key(&self) -> String {
        (0..)
            .map(|i| format!("n{i}"))
            .find(|k| self.node(k).is_none())
            .expect("key sequence is infinite")
    }

    /// Id for a new shaped node: `id`, `id2`… unused by other nodes.
    fn free_id(&self) -> String {
        std::iter::once("id".to_string())
            .chain((2..).map(|i| format!("id{i}")))
            .find(|id| !self.nodes.iter().any(|n| n.id == *id))
            .expect("id sequence is infinite")
    }

    /// Adds a node of `shape` under `parent`, or as the root when `parent`
    /// is `None` and there is none yet, and returns its key.
    pub fn add_node(&mut self, parent: Option<&str>, shape: Shape, text: &str) -> Option<String> {
        if parent.is_none() && self.root().is_some() {
            return None;
        }
        if let Some(parent) = parent
            && self.node(parent).is_none()
        {
            return None;
        }
        let key = self.next_key();
        let id = if shape == Shape::Default {
            String::new()
        } else {
            self.free_id()
        };
        let mut node = Node {
            key: key.clone(),
            id,
            text: String::new(),
            shape,
            parent: parent.map(String::from),
            extras: Vec::new(),
        };
        node.set_text(text);
        self.nodes.push(node);
        Some(key)
    }

    /// Changes a node's shape. Shaped nodes need an id; plain ones are
    /// written as their text alone, so they drop it.
    pub fn set_shape(&mut self, key: &str, shape: Shape) {
        let id = self.free_id();
        if let Some(node) = self.node_mut(key) {
            node.shape = shape;
            if shape == Shape::Default {
                node.id.clear();
            } else if node.id.is_empty() {
                node.id = id;
            }
        }
    }

    /// Removes a node and everything below it. The root goes only when it is
    /// the last node.
    pub fn remove_subtree(&mut self, key: &str) {
        let is_root = self.node(key).is_some_and(|n| n.parent.is_none());
        if is_root && self.nodes.len() > 1 {
            return;
        }
        let doomed: Vec<String> = self
            .nodes
            .iter()
            .filter(|n| self.is_within(&n.key, key))
            .map(|n| n.key.clone())
            .collect();
        self.nodes.retain(|n| !doomed.contains(&n.key));
    }

    /// Moves `key` under `parent`, as its last child. The root, moves into
    /// the node's own subtree and moves under its current parent are refused.
    pub fn reparent(&mut self, key: &str, parent: &str) -> bool {
        let movable = self
            .node(key)
            .is_some_and(|n| n.parent.as_deref().is_some_and(|p| p != parent));
        if !movable || self.node(parent).is_none() || self.is_within(parent, key) {
            return false;
        }
        let Some(idx) = self.nodes.iter().position(|n| n.key == key) else {
            return false;
        };
        let mut node = self.nodes.remove(idx);
        node.parent = Some(parent.to_string());
        self.nodes.push(node);
        true
    }

    /// Adds a copy of `node` (without its children) next to the original,
    /// or under the root when it was the root, and returns its key.
    pub fn paste_node(&mut self, node: &Node) -> Option<String> {
        let parent = node
            .parent
            .clone()
            .filter(|p| self.node(p).is_some())
            .or_else(|| self.root().map(|r| r.key.clone()))?;
        let key = self.next_key();
        let id = if node.id.is_empty() {
            String::new()
        } else {
            self.free_id()
        };
        self.nodes.push(Node {
            key: key.clone(),
            id,
            parent: Some(parent),
            ..node.clone()
        });
        Some(key)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tree() -> Mindmap {
        let mut m = Mindmap::default();
        let root = m.add_node(None, Shape::Circle, "Root").unwrap();
        let a = m.add_node(Some(&root), Shape::Default, "A").unwrap();
        m.add_node(Some(&a), Shape::Default, "B").unwrap();
        m.add_node(Some(&root), Shape::Square, "C").unwrap();
        m
    }

    #[test]
    fn adds_keys_ids_and_a_single_root() {
        let m = tree();
        let keys: Vec<_> = m.nodes.iter().map(|n| n.key.as_str()).collect();
        assert_eq!(keys, ["n0", "n1", "n2", "n3"]);
        assert_eq!(
            (m.nodes[0].id.as_str(), m.nodes[3].id.as_str()),
            ("id", "id2")
        );
        assert_eq!(m.nodes[1].id, "");
        let mut m = m;
        assert_eq!(m.add_node(None, Shape::Default, "Second root"), None);
    }

    #[test]
    fn reparents_without_cycles() {
        let mut m = tree();
        assert!(!m.reparent("n1", "n2"));
        assert!(!m.reparent("n1", "n0"), "already a child of the root");
        assert!(!m.reparent("n0", "n3"));
        assert!(m.reparent("n2", "n3"));
        assert_eq!(m.node("n2").unwrap().parent.as_deref(), Some("n3"));
    }

    #[test]
    fn removes_subtrees_but_keeps_a_root_with_children() {
        let mut m = tree();
        m.remove_subtree("n0");
        assert_eq!(m.nodes.len(), 4);
        m.remove_subtree("n1");
        let keys: Vec<_> = m.nodes.iter().map(|n| n.key.as_str()).collect();
        assert_eq!(keys, ["n0", "n3"]);
    }

    #[test]
    fn plain_nodes_drop_their_id() {
        let mut m = tree();
        m.set_shape("n3", Shape::Default);
        assert_eq!(m.node("n3").unwrap().id, "");
        m.set_shape("n3", Shape::Hexagon);
        assert_eq!(m.node("n3").unwrap().id, "id2");
    }

    #[test]
    fn quotes_text_that_would_break_the_shape() {
        let mut m = tree();
        let node = m.node_mut("n3").unwrap();
        node.set_text("Costs (net)");
        assert_eq!(node.text, "\"Costs (net)\"");
        assert_eq!(node.display(), "Costs (net)");
        node.set_text("%% not a comment");
        assert_eq!(node.text, "\"%% not a comment\"");
        node.set_text("two\nlines");
        assert_eq!(node.text, "two<br/>lines");
        assert_eq!(node.display(), "two\nlines");
    }
}
