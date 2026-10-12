//! Auto layout for flowcharts, built on the shared layered layout.

use std::collections::HashMap;

use super::model::{Flowchart, Node};
use crate::layered;

/// Assigns a position to every node without one.
pub fn layout_missing(chart: &mut Flowchart, size_of: impl Fn(&Node) -> (f64, f64)) {
    let index: HashMap<&str, usize> = chart
        .nodes
        .iter()
        .enumerate()
        .map(|(i, node)| (node.id.as_str(), i))
        .collect();
    let edges: Vec<(usize, usize)> = chart
        .edges
        .iter()
        .filter_map(|e| Some((*index.get(e.from.as_str())?, *index.get(e.to.as_str())?)))
        .collect();
    let sizes: Vec<_> = chart.nodes.iter().map(&size_of).collect();
    let mut positions: Vec<_> = chart.nodes.iter().map(|n| n.pos).collect();
    layered::fill_missing(
        &mut positions,
        &sizes,
        &edges,
        chart.direction,
        layered::Spacing::default(),
    );
    for (node, pos) in chart.nodes.iter_mut().zip(positions) {
        node.pos = pos;
    }
}

/// Discards every stored position and lays the whole chart out again.
pub fn layout_all(chart: &mut Flowchart, size_of: impl Fn(&Node) -> (f64, f64)) {
    for node in &mut chart.nodes {
        node.pos = None;
    }
    layout_missing(chart, size_of);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::flowchart::model::Point;
    use crate::flowchart::parser::parse;

    fn size(_: &Node) -> (f64, f64) {
        (80.0, 40.0)
    }

    fn pos(chart: &Flowchart, id: &str) -> Point {
        chart.node(id).unwrap().pos.unwrap()
    }

    #[test]
    fn top_bottom_places_successors_below() {
        let mut chart = parse("flowchart TD\n A --> B --> C\n A --> D").unwrap();
        layout_all(&mut chart, size);
        assert!(pos(&chart, "B").y > pos(&chart, "A").y);
        assert!(pos(&chart, "C").y > pos(&chart, "B").y);
        assert_eq!(pos(&chart, "B").y, pos(&chart, "D").y);
        assert_ne!(pos(&chart, "B").x, pos(&chart, "D").x);
    }

    #[test]
    fn left_right_places_successors_to_the_right() {
        let mut chart = parse("flowchart LR\n A --> B").unwrap();
        layout_all(&mut chart, size);
        assert!(pos(&chart, "B").x > pos(&chart, "A").x);
        assert_eq!(pos(&chart, "B").y, pos(&chart, "A").y);
    }

    #[test]
    fn bottom_top_places_successors_above() {
        let mut chart = parse("flowchart BT\n A --> B").unwrap();
        layout_all(&mut chart, size);
        assert!(pos(&chart, "B").y < pos(&chart, "A").y);
    }

    #[test]
    fn handles_cycles_and_self_loops() {
        let mut chart = parse("flowchart TD\n A --> B --> C --> A\n C --> C").unwrap();
        layout_all(&mut chart, size);
        assert!(pos(&chart, "A").y < pos(&chart, "B").y);
        assert!(pos(&chart, "B").y < pos(&chart, "C").y);
    }

    #[test]
    fn missing_layout_keeps_existing_positions() {
        let mut chart = parse("flowchart TD\n A --> B\n%% @reefgraph pos A 500 500").unwrap();
        layout_missing(&mut chart, size);
        assert_eq!(pos(&chart, "A"), Point::new(500.0, 500.0));
        let b = pos(&chart, "B");
        assert_eq!(b.x, 500.0);
        assert!(b.y > 500.0);
    }
}
