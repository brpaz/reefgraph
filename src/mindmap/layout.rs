//! Node geometry and layout for mindmaps. Mermaid stores no positions for
//! mindmaps, so the layout is derived from the tree every time: the root's
//! children alternate to its right and left, and each branch grows outwards
//! with its children stacked around it.

use std::collections::HashMap;

use super::model::{Mindmap, Node, Shape};
use crate::geometry::{Point, Rect};

/// Gap between a node and its children, and between stacked subtrees.
const GAPS: (f64, f64) = (50.0, 14.0);

pub fn node_size(node: &Node, text_size: impl Fn(&str) -> (f64, f64)) -> (f64, f64) {
    let (w, h) = text_size(&node.display());
    match node.shape() {
        None => (w + 16.0, h + 12.0),
        Some(Shape::Square | Shape::Rounded) => (w + 24.0, h + 16.0),
        Some(Shape::Circle) => {
            let d = w.max(h) + 24.0;
            (d, d)
        }
        Some(Shape::Bang | Shape::Cloud) => (w + 44.0, h + 32.0),
        Some(Shape::Hexagon) => (w + 44.0, h + 16.0),
    }
}

/// Rectangle of every node, keyed by node key. Runs in time linear in the
/// number of nodes, since it is redone whenever the canvas measures boxes.
pub fn layout(mindmap: &Mindmap, text_size: impl Fn(&str) -> (f64, f64)) -> HashMap<String, Rect> {
    let mut rects = HashMap::new();
    let Some(root) = mindmap.root() else {
        return rects;
    };
    let sizes = mindmap
        .nodes
        .iter()
        .map(|n| (n.key.as_str(), node_size(n, &text_size)))
        .collect();
    let children = mindmap
        .children_index()
        .into_iter()
        .map(|(key, nodes)| (key, nodes.iter().map(|n| n.key.as_str()).collect()))
        .collect();
    let mut tree = Tree {
        sizes,
        children,
        extents: HashMap::new(),
    };
    tree.measure(&root.key);
    let center = mindmap.origin.unwrap_or_default();
    rects.insert(
        root.key.clone(),
        Rect::centered(center, tree.size(&root.key)),
    );
    let (right, left): (Vec<_>, Vec<_>) = tree
        .children_of(&root.key)
        .iter()
        .enumerate()
        .partition(|(i, _)| i % 2 == 0);
    for (side, branch) in [(1.0, right), (-1.0, left)] {
        let keys: Vec<&str> = branch.into_iter().map(|(_, k)| *k).collect();
        tree.place_children(&root.key, &keys, center, side, &mut rects);
    }
    rects
}

struct Tree<'a> {
    sizes: HashMap<&'a str, (f64, f64)>,
    children: HashMap<&'a str, Vec<&'a str>>,
    /// Height taken by each node and everything below it.
    extents: HashMap<&'a str, f64>,
}

impl<'a> Tree<'a> {
    fn size(&self, key: &str) -> (f64, f64) {
        self.sizes.get(key).copied().unwrap_or_default()
    }

    fn children_of(&self, key: &str) -> &[&'a str] {
        self.children.get(key).map_or(&[], Vec::as_slice)
    }

    fn extent(&self, key: &str) -> f64 {
        self.extents.get(key).copied().unwrap_or_default()
    }

    fn stack_height(&self, keys: &[&str]) -> f64 {
        let total: f64 = keys.iter().map(|k| self.extent(k)).sum();
        total + GAPS.1 * keys.len().saturating_sub(1) as f64
    }

    /// Fills in the extents of `key` and everything below it.
    fn measure(&mut self, key: &'a str) {
        let children = self.children_of(key).to_vec();
        for &child in &children {
            self.measure(child);
        }
        let extent = self.stack_height(&children).max(self.size(key).1);
        self.extents.insert(key, extent);
    }

    /// Places `keys`, children of `parent` centered at `center`, on `side`
    /// (1 for right, -1 for left), stacked around the parent's height.
    fn place_children(
        &self,
        parent: &str,
        keys: &[&str],
        center: Point,
        side: f64,
        rects: &mut HashMap<String, Rect>,
    ) {
        let mut y = center.y - self.stack_height(keys) / 2.0;
        for key in keys {
            let extent = self.extent(key);
            let size = self.size(key);
            let x = center.x + side * (self.size(parent).0 / 2.0 + GAPS.0 + size.0 / 2.0);
            let at = Point::new(x, y + extent / 2.0);
            rects.insert(key.to_string(), Rect::centered(at, size));
            self.place_children(key, self.children_of(key), at, side, rects);
            y += extent + GAPS.1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mindmap::parser::parse;

    fn size(s: &str) -> (f64, f64) {
        (s.chars().count() as f64 * 8.0, 16.0)
    }

    #[test]
    fn branches_alternate_sides_and_do_not_overlap() {
        let m = parse(
            "mindmap
  Root
    A
      A1
      A2
    B
    C
      C1",
        )
        .unwrap();
        let rects = layout(&m, size);
        let x = |k: &str| rects[k].center().x;
        assert!(x("n1") > 0.0 && x("n4") < 0.0 && x("n5") > 0.0);
        assert!(x("n2") > x("n1"));
        let right: Vec<&Rect> = ["n1", "n2", "n3", "n5", "n6"]
            .iter()
            .map(|k| &rects[*k])
            .collect();
        for (i, a) in right.iter().enumerate() {
            for b in &right[i + 1..] {
                let overlap =
                    a.x < b.x + b.w && b.x < a.x + a.w && a.y < b.y + b.h && b.y < a.y + a.h;
                assert!(!overlap, "{a:?} overlaps {b:?}");
            }
        }
    }

    #[test]
    fn grows_from_the_origin() {
        let mut m = parse("mindmap\n  Root\n    A").unwrap();
        m.origin = Some(Point::new(300.0, 200.0));
        let rects = layout(&m, size);
        assert_eq!(rects["n0"].center(), Point::new(300.0, 200.0));
        assert!(rects["n1"].center().x > 300.0);
    }

    #[test]
    fn lays_out_large_mindmaps_quickly() {
        let mut src = String::from("mindmap\n  Root\n");
        for i in 0..20_000 {
            src.push_str(&format!("    Node {i}\n      Leaf {i}\n"));
        }
        let m = parse(&src).unwrap();
        let started = std::time::Instant::now();
        assert_eq!(layout(&m, size).len(), 40_001);
        // Linear layout takes well under a second here even unoptimized; a
        // quadratic one takes minutes.
        assert!(started.elapsed().as_secs_f64() < 10.0);
    }

    #[test]
    fn circles_are_round() {
        let m = parse("mindmap\n  r((Root node))").unwrap();
        let (w, h) = node_size(&m.nodes[0], size);
        assert_eq!(w, h);
    }
}
