use std::collections::HashMap;

use super::model::{Mindmap, Node};
use super::parser::KEYWORD;

const INDENT: &str = "  ";

/// Renders the mindmap as Mermaid source, one node per line indented by its
/// depth, each followed by its icon, class and comment lines.
pub fn serialize(mindmap: &Mindmap) -> String {
    let mut out = String::new();
    for line in &mindmap.preamble {
        out.push_str(line);
        out.push('\n');
    }
    out.push_str(KEYWORD);
    out.push('\n');
    for line in &mindmap.leading {
        out.push_str(line);
        out.push('\n');
    }
    if let Some(root) = mindmap.root() {
        write_node(&mut out, &mindmap.children_index(), root, 1);
    }
    out
}

fn write_node(out: &mut String, children: &HashMap<&str, Vec<&Node>>, node: &Node, depth: usize) {
    let indent = INDENT.repeat(depth);
    let (open, close) = node.shape.delimiters();
    out.push_str(&format!("{indent}{}{open}{}{close}\n", node.id, node.text));
    for extra in &node.extras {
        out.push_str(&format!("{indent}{extra}\n"));
    }
    for child in children.get(node.key.as_str()).into_iter().flatten() {
        write_node(out, children, child, depth + 1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mindmap::model::Shape;
    use crate::mindmap::parser::parse;

    const SAMPLE: &str = "---
title: Plans
---
mindmap
%% ideas
  root((mindmap))
    Origins
      Long history
      ::icon(fa fa-book)
      Popularisation
        British popular psychology author Tony Buzan
    research{{Research}}
      On effectiveness<br/>and features
      uses[Uses]
      :::urgent large
        b))Bang((
        c)Cloud(
        d(\"`**Bold**`\")
";

    #[test]
    fn round_trips_sample_exactly() {
        let m = parse(SAMPLE).unwrap();
        assert_eq!(serialize(&m), SAMPLE);
    }

    #[test]
    fn edited_mindmaps_parse_back_unchanged() {
        let mut m = parse("mindmap\n  Root").unwrap();
        let a = m
            .add_node(Some("n0"), Shape::Hexagon, "Costs (net)")
            .unwrap();
        let b = m.add_node(Some(&a), Shape::Default, "two\nlines").unwrap();
        m.add_node(Some("n0"), Shape::Cloud, "Later").unwrap();
        m.add_node(Some("n0"), Shape::Default, "f(x)").unwrap();
        m.reparent(&b, "n0");
        let back = parse(&serialize(&m)).unwrap();
        // Keys follow source order, which puts children after their parent.
        assert_eq!(serialize(&back), serialize(&m));
        let texts: Vec<_> = back.nodes.iter().map(|n| n.display()).collect();
        assert_eq!(
            texts,
            ["Root", "Costs (net)", "Later", "f(x)", "two\nlines"]
        );
    }
}
