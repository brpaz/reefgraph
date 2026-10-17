use std::fmt::Write;

use super::model::{C4Diagram, Element};
use crate::shared::syntax::position_line;

const INDENT: &str = "    ";

/// Renders the diagram as Mermaid source: at each nesting level the elements
/// first, then the boundaries with their contents; then relations, untouched
/// statements and stored positions.
pub fn serialize(diagram: &C4Diagram) -> String {
    let mut out = String::new();
    for line in &diagram.preamble {
        out.push_str(line);
        out.push('\n');
    }
    out.push_str(diagram.level.keyword());
    out.push('\n');
    if let Some(title) = &diagram.title {
        let _ = writeln!(out, "{INDENT}title {title}");
    }
    write_level(&mut out, diagram, None, 1);
    for r in &diagram.relations {
        let mut args = vec![r.from.clone(), r.to.clone(), quote(&r.label)];
        if !r.technology.is_empty() || !r.extra.is_empty() {
            args.push(quote(&r.technology));
        }
        args.extend(r.extra.iter().cloned());
        let _ = writeln!(out, "{INDENT}{}({})", r.kind.keyword(), args.join(", "));
    }
    for line in &diagram.passthrough {
        let _ = writeln!(out, "{INDENT}{line}");
    }
    for element in &diagram.elements {
        if let Some(pos) = element.pos {
            let _ = writeln!(out, "{}", position_line(&element.id, pos));
        }
    }
    out
}

fn write_level(out: &mut String, diagram: &C4Diagram, parent: Option<&str>, depth: usize) {
    let indent = INDENT.repeat(depth);
    for element in diagram.child_elements(parent) {
        let _ = writeln!(out, "{indent}{}", element_statement(element));
    }
    for boundary in diagram.child_boundaries(parent) {
        let mut args = vec![boundary.id.clone(), quote(&boundary.label)];
        args.extend(boundary.extra.iter().cloned());
        let _ = writeln!(
            out,
            "{indent}{}({}) {{",
            boundary.kind.keyword(),
            args.join(", ")
        );
        write_level(out, diagram, Some(&boundary.id), depth + 1);
        let _ = writeln!(out, "{indent}}}");
    }
}

fn element_statement(e: &Element) -> String {
    let mut optional = Vec::new();
    if e.kind.has_technology() {
        optional.push(quote(&e.technology));
    }
    optional.push(quote(&e.description));
    // Trailing empty arguments are left out unless something follows them.
    if e.extra.is_empty() {
        while optional.last().is_some_and(|a| a == "\"\"") {
            optional.pop();
        }
    }
    let mut args = vec![e.id.clone(), quote(&e.label)];
    args.extend(optional);
    args.extend(e.extra.iter().cloned());
    format!("{}({})", e.keyword(), args.join(", "))
}

/// Quotes a text argument. Mermaid has no escape for `"` inside one, so
/// double quotes become single quotes.
fn quote(text: &str) -> String {
    format!("\"{}\"", text.replace('"', "'"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diagrams::c4::parser::parse;

    const SAMPLE: &str = "---
title: Bank
---
C4Container
    title Internet Banking
    Person(customer, \"Customer\", \"A customer of the bank\")
    System_Ext(mail, \"E-mail System\")
    System_Boundary(bank, \"Internet Banking\") {
        Container(web, \"Web App\", \"React\", \"Delivers the SPA\")
        ContainerDb(db, \"Database\", \"\", \"Stores accounts\")
        Container_Boundary(api, \"API\", \"service\") {
            Component(auth, \"Auth\", \"Go\")
        }
    }
    Rel(customer, web, \"Uses\", \"HTTPS\")
    BiRel(web, auth, \"Signs in\")
    Rel_D(auth, db, \"Reads\", \"\", $tags=\"x\")
    UpdateLayoutConfig($c4ShapeInRow=\"3\")
    %% kept
%% @reefgraph pos customer 100 80
%% @reefgraph pos web 300 80
";

    #[test]
    fn round_trips_sample_exactly() {
        let diagram = parse(SAMPLE).unwrap();
        assert_eq!(serialize(&diagram), SAMPLE);
    }

    #[test]
    fn normalizes_reverse_relations_and_long_hints() {
        let diagram =
            parse("C4Context\nSystem(a, A)\nSystem(b, B)\nRel_Back(a, b, \"x\")\nRel_Up(a, b, y)")
                .unwrap();
        assert_eq!(
            serialize(&diagram),
            "C4Context\n    System(a, \"A\")\n    System(b, \"B\")\n    Rel(b, a, \"x\")\n    Rel_U(a, b, \"y\")\n"
        );
    }
}
