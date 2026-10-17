use std::fmt::Write;

use super::model::{Erd, is_name_char};
use super::parser::KEYWORD;
use crate::shared::syntax::position_line;

const INDENT: &str = "    ";

/// Renders the diagram as Mermaid source: entities with their attributes
/// first, then relationships, untouched statements and stored positions.
pub fn serialize(erd: &Erd) -> String {
    let mut out = String::new();
    for line in &erd.preamble {
        out.push_str(line);
        out.push('\n');
    }
    out.push_str(KEYWORD);
    out.push('\n');
    if let Some(direction) = erd.direction {
        let _ = writeln!(out, "{INDENT}direction {}", direction.keyword());
    }
    for entity in &erd.entities {
        let _ = write!(out, "{INDENT}{}", quote(&entity.id));
        if entity.label != entity.id {
            let _ = write!(out, "[{}]", quote(&entity.label));
        }
        if entity.attributes.is_empty() {
            out.push('\n');
            continue;
        }
        out.push_str(" {\n");
        for attribute in &entity.attributes {
            let _ = writeln!(out, "{INDENT}{INDENT}{}", attribute.to_line());
        }
        let _ = writeln!(out, "{INDENT}}}");
    }
    for r in &erd.relationships {
        let _ = writeln!(
            out,
            "{INDENT}{} {}{}{} {} : {}",
            quote(&r.from),
            r.from_cardinality.left_token(),
            if r.identifying { "--" } else { ".." },
            r.to_cardinality.right_token(),
            quote(&r.to),
            quote(&r.label)
        );
    }
    for line in &erd.passthrough {
        let _ = writeln!(out, "{INDENT}{line}");
    }
    for entity in &erd.entities {
        if let Some(pos) = entity.pos {
            let _ = writeln!(out, "{}", position_line(&entity.id, pos));
        }
    }
    out
}

fn quote(text: &str) -> String {
    if !text.is_empty() && text.chars().all(is_name_char) {
        text.to_string()
    } else {
        format!("\"{text}\"")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diagrams::erd::parser::parse;

    const SAMPLE: &str = "---
title: Shop
---
erDiagram
    direction LR
    CUSTOMER {
        string name PK \"Full name\"
        string email UK
    }
    ORDER {
        int id PK
        int customerId FK
    }
    p[Person]
    a[\"Customer Account\"] {
        string login
    }
    \"LINE ITEM\"
    CUSTOMER ||--o{ ORDER : places
    ORDER ||--|{ \"LINE ITEM\" : contains
    p }o..o| a : \"logs in as\"
    %% kept
%% @reefgraph pos CUSTOMER 100 80
%% @reefgraph pos ORDER 300 80
";

    #[test]
    fn round_trips_sample_exactly() {
        let erd = parse(SAMPLE).unwrap();
        assert_eq!(serialize(&erd), SAMPLE);
    }

    #[test]
    fn normalizes_word_relationships_to_symbols() {
        let erd = parse("erDiagram\nA only one to zero or more B : has").unwrap();
        assert_eq!(
            serialize(&erd),
            "erDiagram\n    A\n    B\n    A ||--o{ B : has\n"
        );
    }
}
