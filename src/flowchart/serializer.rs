use std::fmt::Write;

use super::model::{Edge, Flowchart, Head, Node, NodeShape, Stroke};
use crate::syntax::position_line;

const INDENT: &str = "    ";

/// Renders the chart as Mermaid source: node definitions first, then edges,
/// then untouched statements, then the stored node positions.
pub fn serialize(chart: &Flowchart) -> String {
    let mut out = String::new();
    for line in &chart.preamble {
        out.push_str(line);
        out.push('\n');
    }
    let _ = writeln!(out, "flowchart {}", chart.direction.keyword());
    for node in &chart.nodes {
        let _ = writeln!(out, "{INDENT}{}", node_definition(node));
    }
    for edge in &chart.edges {
        let _ = writeln!(out, "{INDENT}{} {} {}", edge.from, link(edge), edge.to);
    }
    for line in &chart.passthrough {
        let _ = writeln!(out, "{INDENT}{line}");
    }
    for node in &chart.nodes {
        if let Some(pos) = node.pos {
            let _ = writeln!(out, "{}", position_line(&node.id, pos));
        }
    }
    out
}

fn node_definition(node: &Node) -> String {
    let mut out = node.id.clone();
    if node.shape != NodeShape::Rectangle || node.label != node.id {
        let (open, close) = node.shape.delimiters();
        let _ = write!(out, "{open}{}{close}", quote(&node.label));
    }
    if let Some(class) = &node.class {
        let _ = write!(out, ":::{class}");
    }
    out
}

fn head_char(head: Head) -> Option<char> {
    match head {
        Head::None => None,
        Head::Arrow => Some('>'),
        Head::Circle => Some('o'),
        Head::Cross => Some('x'),
    }
}

fn link(edge: &Edge) -> String {
    let mut out = String::new();
    if let Some(c) = head_char(edge.start) {
        out.push(if c == '>' { '<' } else { c });
    }
    let end = head_char(edge.end);
    match edge.stroke {
        Stroke::Normal => {
            out.push_str("--");
            out.push(end.unwrap_or('-'));
        }
        Stroke::Thick => {
            out.push_str("==");
            out.push(end.unwrap_or('='));
        }
        Stroke::Dotted => {
            out.push_str("-.-");
            out.extend(end);
        }
    }
    if !edge.label.is_empty() {
        let _ = write!(out, "|{}|", quote(&edge.label));
    }
    out
}

/// Quotes labels containing characters that would otherwise be read as syntax.
fn quote(label: &str) -> String {
    let plain = !label.is_empty()
        && label.trim() == label
        && label
            .chars()
            .all(|c| c.is_alphanumeric() || " _-.,!?'".contains(c));
    if plain {
        label.to_string()
    } else {
        format!("\"{}\"", label.replace('"', "#quot;"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::flowchart::model::{Direction, Point};
    use crate::flowchart::parser::parse;

    #[test]
    fn serializes_nodes_edges_and_positions() {
        let mut chart = Flowchart {
            direction: Direction::LeftRight,
            ..Flowchart::default()
        };
        chart.ensure_node("A").label = "Start".into();
        chart.ensure_node("B").shape = NodeShape::Rhombus;
        chart.ensure_node("C");
        let i = chart.add_edge("A", "B");
        chart.edges[i].label = "go".into();
        chart.node_mut("A").unwrap().pos = Some(Point::new(10.4, 20.6));

        assert_eq!(
            serialize(&chart),
            "flowchart LR\n    A[Start]\n    B{B}\n    C\n    A -->|go| B\n%% @merman pos A 10 21\n"
        );
    }

    #[test]
    fn quotes_labels_with_syntax_characters() {
        assert_eq!(quote("Hello world"), "Hello world");
        assert_eq!(quote("a (b)"), "\"a (b)\"");
        assert_eq!(quote("say \"hi\""), "\"say #quot;hi#quot;\"");
        assert_eq!(quote(""), "\"\"");
    }

    #[test]
    fn round_trips_through_parser() {
        let src = "---
title: Round trip
---
flowchart RL
    A[\"Start (here)\"]:::hot
    B{Decide}
    C((Circle))
    D[/Input/]
    E>Flag]
    A --> B
    B -->|yes| C
    B -.->|\"no|maybe\"| D
    C ==> E
    D <--> E
    E o--o A
    E --x B
    A --- C
    A -.- D
    B === E
    classDef hot fill:#f96
    %% keep me
%% @merman pos A 1 2
%% @merman pos B -30 40
";
        let chart = parse(src).unwrap();
        let reparsed = parse(&serialize(&chart)).unwrap();
        assert_eq!(reparsed, chart);
        assert_eq!(serialize(&reparsed), serialize(&chart));
    }

    #[test]
    fn round_trips_every_link_style() {
        let mut chart = Flowchart::default();
        for stroke in Stroke::ALL {
            for start in Head::ALL {
                for end in Head::ALL {
                    let i = chart.add_edge("A", "B");
                    let edge = &mut chart.edges[i];
                    (edge.stroke, edge.start, edge.end) = (stroke, start, end);
                }
            }
        }
        chart.ensure_node("A");
        chart.ensure_node("B");
        let reparsed = parse(&serialize(&chart)).unwrap();
        assert_eq!(reparsed.edges, chart.edges);
    }
}
