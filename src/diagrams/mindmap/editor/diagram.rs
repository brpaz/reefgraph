//! Mindmaps on the box canvas: nodes are boxes and links run from each node
//! to its children. Mermaid stores no positions for mindmaps, so node
//! positions are derived from the tree on every use and moves are ignored.
//! Rows of a node are its children: the "add row" button adds a child and
//! edits its text in place.

use std::any::Any;
use std::collections::HashMap;

use gtk::cairo;

use super::{preset_id, preset_shape, render};
use crate::canvas::box_canvas::{self, BoxDiagram, Highlight, Measure};
use crate::canvas::drawing::{Colors, Text, log_draw_error};
use crate::diagrams::mindmap::layout;
use crate::diagrams::mindmap::model::Node;
use crate::diagrams::mindmap::{Mindmap, Shape};
use crate::shared::geometry::{Point, Rect};
use crate::shared::ids::RenameError;
use crate::shared::routing::Outline;

/// Text of a node added from the canvas.
fn new_node_text() -> String {
    gettextrs::gettext("New idea")
}

impl Mindmap {
    /// For every node below the root, the index of the root's child it
    /// descends from.
    fn branches(&self) -> HashMap<&str, usize> {
        let mut branches = HashMap::new();
        let Some(root) = self.root() else {
            return branches;
        };
        let children = self.children_index();
        let first = children.get(root.key.as_str()).into_iter().flatten();
        let mut pending: Vec<(&Node, usize)> = first.enumerate().map(|(i, n)| (*n, i)).collect();
        while let Some((node, branch)) = pending.pop() {
            branches.insert(node.key.as_str(), branch);
            let below = children.get(node.key.as_str()).into_iter().flatten();
            pending.extend(below.map(|n| (*n, branch)));
        }
        branches
    }

    fn child_key(&self, parent: &str, row: usize) -> Option<String> {
        self.children(parent).nth(row).map(|n| n.key.clone())
    }
}

impl BoxDiagram for Mindmap {
    fn clone_diagram(&self) -> Box<dyn BoxDiagram> {
        Box::new(self.clone())
    }

    fn same_as(&self, other: &dyn BoxDiagram) -> bool {
        box_canvas::same(self, other)
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }

    fn box_ids(&self) -> Vec<String> {
        self.nodes.iter().map(|n| n.key.clone()).collect()
    }

    fn boxes(&self, measure: Measure) -> Vec<(&str, Rect)> {
        let rects = layout::layout(self, measure);
        self.nodes
            .iter()
            .filter_map(|n| Some((n.key.as_str(), *rects.get(&n.key)?)))
            .collect()
    }

    /// Only the root has a position: it anchors the layout, so dragging it
    /// moves the whole mindmap.
    fn position(&self, id: &str) -> Option<Point> {
        self.root().filter(|r| r.key == id).and(self.origin)
    }

    fn set_position(&mut self, id: &str, pos: Point) {
        if self.root().is_some_and(|r| r.key == id) {
            self.origin = Some(pos);
        }
    }

    fn box_rect(&self, id: &str, measure: Measure) -> Option<Rect> {
        layout::layout(self, measure).remove(id)
    }

    fn outline(&self, id: &str) -> Outline {
        match self.node(id).and_then(Node::shape) {
            Some(Shape::Circle | Shape::Cloud | Shape::Bang) => Outline::Ellipse,
            Some(Shape::Square | Shape::Rounded | Shape::Hexagon) | None => Outline::Rect,
        }
    }

    /// The tree layout keeps branches clear of other nodes.
    fn routes_around_boxes(&self) -> bool {
        false
    }

    fn label(&self, id: &str) -> Option<String> {
        Some(self.node(id)?.display())
    }

    /// The canvas falls back to the id when the text is cleared; a node
    /// keeps its text then.
    fn set_label(&mut self, id: &str, label: String) {
        if label == id {
            return;
        }
        if let Some(node) = self.node_mut(id) {
            node.set_text(&label);
        }
    }

    fn header_center(&self, id: &str, measure: Measure) -> Option<Point> {
        Some(self.box_rect(id, measure)?.center())
    }

    fn layout(&mut self, _all: bool, _measure: Measure) {}

    fn links(&self) -> Vec<(String, String)> {
        self.nodes
            .iter()
            .filter_map(|n| Some((n.parent.clone()?, n.key.clone())))
            .collect()
    }

    fn default_preset(&self) -> &'static str {
        preset_id(None)
    }

    /// New nodes go under the root, or become the root of an empty mindmap,
    /// placed at `at`.
    fn add_box(&mut self, preset: &str, at: Point) -> Option<String> {
        let shape = preset_shape(preset)?;
        let root = self.root().map(|r| r.key.clone());
        if root.is_none() {
            self.origin = Some(at);
        }
        self.add_node(root.as_deref(), shape, &new_node_text())
    }

    fn remove_box(&mut self, id: &str) {
        self.remove_subtree(id);
    }

    fn paste_box(&mut self, from: &dyn BoxDiagram, id: &str, _at: Point) -> Option<String> {
        let copied = from.as_any().downcast_ref::<Mindmap>()?.node(id)?.clone();
        self.paste_node(&copied)
    }

    fn rename_box(&mut self, _old: &str, _new: &str) -> Result<(), RenameError> {
        Err(RenameError::Invalid)
    }

    /// Linking a node to another moves the target under it.
    fn add_link(&mut self, from: &str, to: &str) -> Option<usize> {
        if !self.reparent(to, from) {
            return None;
        }
        self.links().iter().position(|(_, child)| child == to)
    }

    /// Every node but the root has exactly one parent, so links are only
    /// changed by moving nodes.
    fn remove_link(&mut self, _index: usize) {}

    fn link_label(&self, _index: usize) -> Option<String> {
        None
    }

    fn set_link_label(&mut self, _index: usize, _label: String) {}

    fn row_at(&self, _id: &str, _p: Point, _measure: Measure) -> Option<usize> {
        None
    }

    fn add_row(&mut self, id: &str) -> Option<usize> {
        self.add_node(Some(id), None, &new_node_text())?;
        Some(self.children(id).count() - 1)
    }

    fn row_text(&self, id: &str, row: usize, measure: Measure) -> Option<(String, Point)> {
        let child = self.child_key(id, row)?;
        Some((
            self.node(&child)?.display(),
            self.box_rect(&child, measure)?.center(),
        ))
    }

    /// Clearing a child's text removes it.
    fn set_row_text(&mut self, id: &str, row: usize, text: &str) -> Result<(), String> {
        let Some(child) = self.child_key(id, row) else {
            return Ok(());
        };
        if text.is_empty() {
            self.remove_subtree(&child);
        } else if let Some(node) = self.node_mut(&child) {
            node.set_text(text);
        }
        Ok(())
    }

    fn draw(
        &self,
        cr: &cairo::Context,
        paths: &[Vec<Point>],
        highlight: &Highlight,
        colors: &Colors,
        measure: Measure,
    ) -> Vec<Text> {
        let rects = layout::layout(self, measure);
        let branches = self.branches();
        let branch = |key: &str| branches.get(key).copied();
        for (i, ((_, child), path)) in self.links().iter().zip(paths).enumerate() {
            let color = render::branch_color(branch(child), colors);
            log_draw_error(render::draw_branch(
                cr,
                path,
                &color,
                colors,
                highlight.link == Some(i),
            ));
        }
        let mut texts = Vec::new();
        for node in &self.nodes {
            let Some(rect) = rects.get(&node.key) else {
                continue;
            };
            let color = render::branch_color(branch(&node.key), colors);
            log_draw_error(render::draw_node(
                cr,
                rect,
                node.shape(),
                &color,
                colors,
                highlight.boxes.contains(&node.key),
            ));
            let text = Text::plain(node.display(), rect.center(), colors.fg);
            texts.push(Text {
                bold: node.parent.is_none(),
                ..text
            });
        }
        texts
    }
}
