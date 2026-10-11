pub use crate::geometry::Point;
pub use crate::ids::RenameError;

use crate::ids;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Direction {
    #[default]
    TopBottom,
    BottomTop,
    LeftRight,
    RightLeft,
}

impl Direction {
    pub const ALL: [Direction; 4] = [
        Direction::TopBottom,
        Direction::BottomTop,
        Direction::LeftRight,
        Direction::RightLeft,
    ];

    pub fn keyword(self) -> &'static str {
        match self {
            Direction::TopBottom => "TB",
            Direction::BottomTop => "BT",
            Direction::LeftRight => "LR",
            Direction::RightLeft => "RL",
        }
    }

    pub fn from_keyword(s: &str) -> Option<Self> {
        match s {
            "TB" | "TD" => Some(Direction::TopBottom),
            "BT" => Some(Direction::BottomTop),
            "LR" => Some(Direction::LeftRight),
            "RL" => Some(Direction::RightLeft),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Hash)]
pub enum NodeShape {
    #[default]
    Rectangle,
    Rounded,
    Stadium,
    Subroutine,
    Cylinder,
    Circle,
    DoubleCircle,
    Rhombus,
    Hexagon,
    Parallelogram,
    ParallelogramAlt,
    Trapezoid,
    TrapezoidAlt,
    Asymmetric,
}

impl NodeShape {
    pub const ALL: [NodeShape; 14] = [
        NodeShape::Rectangle,
        NodeShape::Rounded,
        NodeShape::Stadium,
        NodeShape::Subroutine,
        NodeShape::Cylinder,
        NodeShape::Circle,
        NodeShape::DoubleCircle,
        NodeShape::Rhombus,
        NodeShape::Hexagon,
        NodeShape::Parallelogram,
        NodeShape::ParallelogramAlt,
        NodeShape::Trapezoid,
        NodeShape::TrapezoidAlt,
        NodeShape::Asymmetric,
    ];

    /// Mermaid opening and closing delimiters around the node label.
    pub fn delimiters(self) -> (&'static str, &'static str) {
        match self {
            NodeShape::Rectangle => ("[", "]"),
            NodeShape::Rounded => ("(", ")"),
            NodeShape::Stadium => ("([", "])"),
            NodeShape::Subroutine => ("[[", "]]"),
            NodeShape::Cylinder => ("[(", ")]"),
            NodeShape::Circle => ("((", "))"),
            NodeShape::DoubleCircle => ("(((", ")))"),
            NodeShape::Rhombus => ("{", "}"),
            NodeShape::Hexagon => ("{{", "}}"),
            NodeShape::Parallelogram => ("[/", "/]"),
            NodeShape::ParallelogramAlt => ("[\\", "\\]"),
            NodeShape::Trapezoid => ("[/", "\\]"),
            NodeShape::TrapezoidAlt => ("[\\", "/]"),
            NodeShape::Asymmetric => (">", "]"),
        }
    }

    /// Stable identifier, used for drag-and-drop payloads.
    pub fn name(self) -> &'static str {
        match self {
            NodeShape::Rectangle => "rectangle",
            NodeShape::Rounded => "rounded",
            NodeShape::Stadium => "stadium",
            NodeShape::Subroutine => "subroutine",
            NodeShape::Cylinder => "cylinder",
            NodeShape::Circle => "circle",
            NodeShape::DoubleCircle => "double-circle",
            NodeShape::Rhombus => "rhombus",
            NodeShape::Hexagon => "hexagon",
            NodeShape::Parallelogram => "parallelogram",
            NodeShape::ParallelogramAlt => "parallelogram-alt",
            NodeShape::Trapezoid => "trapezoid",
            NodeShape::TrapezoidAlt => "trapezoid-alt",
            NodeShape::Asymmetric => "asymmetric",
        }
    }

    pub fn from_name(s: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|shape| shape.name() == s)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Stroke {
    #[default]
    Normal,
    Dotted,
    Thick,
}

impl Stroke {
    pub const ALL: [Stroke; 3] = [Stroke::Normal, Stroke::Dotted, Stroke::Thick];
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Head {
    #[default]
    None,
    Arrow,
    Circle,
    Cross,
}

impl Head {
    pub const ALL: [Head; 4] = [Head::None, Head::Arrow, Head::Circle, Head::Cross];
}

#[derive(Debug, Clone, PartialEq)]
pub struct Node {
    pub id: String,
    pub label: String,
    pub shape: NodeShape,
    /// Class attached with the `:::class` shorthand.
    pub class: Option<String>,
    /// Center of the node in canvas coordinates; `None` until laid out.
    pub pos: Option<Point>,
}

impl Node {
    pub fn new(id: impl Into<String>) -> Self {
        let id = id.into();
        Self {
            label: id.clone(),
            id,
            shape: NodeShape::default(),
            class: None,
            pos: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Edge {
    pub from: String,
    pub to: String,
    pub label: String,
    pub stroke: Stroke,
    pub start: Head,
    pub end: Head,
}

/// Words Mermaid treats as keywords, which break the diagram when used as node ids.
const RESERVED_IDS: &[&str] = &["end", "graph", "flowchart", "subgraph", "style", "class"];

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Flowchart {
    pub direction: Direction,
    pub nodes: Vec<Node>,
    pub edges: Vec<Edge>,
    /// Lines before the `flowchart` declaration (front matter, directives, comments).
    pub preamble: Vec<String>,
    /// Statements the editor does not model (`classDef`, `style`, `click`, comments…),
    /// kept verbatim so they survive a round trip.
    pub passthrough: Vec<String>,
}

impl Flowchart {
    pub fn is_valid_id(id: &str) -> bool {
        ids::is_valid_id(id, RESERVED_IDS)
    }

    pub fn node(&self, id: &str) -> Option<&Node> {
        self.nodes.iter().find(|n| n.id == id)
    }

    pub fn node_mut(&mut self, id: &str) -> Option<&mut Node> {
        self.nodes.iter_mut().find(|n| n.id == id)
    }

    /// Returns the node with `id`, creating a default one when missing.
    pub fn ensure_node(&mut self, id: &str) -> &mut Node {
        let idx = match self.nodes.iter().position(|n| n.id == id) {
            Some(idx) => idx,
            None => {
                self.nodes.push(Node::new(id));
                self.nodes.len() - 1
            }
        };
        &mut self.nodes[idx]
    }

    /// First free id in the sequence `A`..`Z`, `N1`, `N2`…
    pub fn next_id(&self) -> String {
        ids::next_free_id(|id| self.node(id).is_some())
    }

    pub fn add_node(&mut self, shape: NodeShape, pos: Point) -> String {
        let id = self.next_id();
        self.nodes.push(Node {
            label: id.clone(),
            id: id.clone(),
            shape,
            class: None,
            pos: Some(pos),
        });
        id
    }

    pub fn remove_node(&mut self, id: &str) {
        self.nodes.retain(|n| n.id != id);
        self.edges.retain(|e| e.from != id && e.to != id);
    }

    pub fn rename_node(&mut self, old: &str, new: &str) -> Result<(), RenameError> {
        if old == new {
            return Ok(());
        }
        if !Self::is_valid_id(new) {
            return Err(RenameError::Invalid);
        }
        if self.node(new).is_some() {
            return Err(RenameError::Taken);
        }
        if let Some(node) = self.node_mut(old) {
            if node.label == old {
                node.label = new.to_string();
            }
            node.id = new.to_string();
        }
        for edge in &mut self.edges {
            if edge.from == old {
                edge.from = new.to_string();
            }
            if edge.to == old {
                edge.to = new.to_string();
            }
        }
        Ok(())
    }

    pub fn add_edge(&mut self, from: &str, to: &str) -> usize {
        self.edges.push(Edge {
            from: from.to_string(),
            to: to.to_string(),
            end: Head::Arrow,
            ..Edge::default()
        });
        self.edges.len() - 1
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn next_id_skips_used_letters() {
        let mut chart = Flowchart::default();
        chart.ensure_node("A");
        chart.ensure_node("C");
        assert_eq!(chart.next_id(), "B");
    }

    #[test]
    fn next_id_falls_back_to_numbered_ids() {
        let mut chart = Flowchart::default();
        for c in 'A'..='Z' {
            chart.ensure_node(&c.to_string());
        }
        assert_eq!(chart.next_id(), "N1");
    }

    #[test]
    fn remove_node_drops_connected_edges() {
        let mut chart = Flowchart::default();
        chart.add_edge("A", "B");
        chart.add_edge("B", "C");
        chart.add_edge("C", "D");
        chart.remove_node("B");
        assert_eq!(chart.edges.len(), 1);
        assert_eq!(chart.edges[0].from, "C");
    }

    #[test]
    fn rename_updates_edges_and_default_label() {
        let mut chart = Flowchart::default();
        chart.ensure_node("A");
        chart.ensure_node("B").label = "Bee".into();
        chart.add_edge("A", "B");
        chart.rename_node("A", "Start").unwrap();
        chart.rename_node("B", "Second").unwrap();
        assert_eq!(chart.nodes[0].label, "Start");
        assert_eq!(chart.nodes[1].label, "Bee");
        assert_eq!(chart.edges[0].from, "Start");
        assert_eq!(chart.edges[0].to, "Second");
    }

    #[test]
    fn rename_rejects_invalid_and_taken_ids() {
        let mut chart = Flowchart::default();
        chart.ensure_node("A");
        chart.ensure_node("B");
        assert_eq!(chart.rename_node("A", "B"), Err(RenameError::Taken));
        assert_eq!(
            chart.rename_node("A", "has space"),
            Err(RenameError::Invalid)
        );
        assert_eq!(chart.rename_node("A", "end"), Err(RenameError::Invalid));
    }

    #[test]
    fn shape_names_round_trip() {
        for shape in NodeShape::ALL {
            assert_eq!(NodeShape::from_name(shape.name()), Some(shape));
        }
    }
}
