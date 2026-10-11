use std::fmt::Write;

use super::model::{Item, Sequence, arrow_token};
use super::parser::KEYWORD;

const INDENT: &str = "    ";

/// Renders the diagram as Mermaid source with participants declared up
/// front and block contents indented.
pub fn serialize(seq: &Sequence) -> String {
    let mut out = String::new();
    for line in &seq.preamble {
        out.push_str(line);
        out.push('\n');
    }
    out.push_str(KEYWORD);
    out.push('\n');
    if let Some(title) = &seq.title {
        let _ = writeln!(out, "{INDENT}title {title}");
    }
    if seq.autonumber {
        let _ = writeln!(out, "{INDENT}autonumber");
    }
    for p in &seq.participants {
        let _ = write!(out, "{INDENT}{} {}", p.kind.keyword(), p.id);
        if p.label != p.id {
            let _ = write!(out, " as {}", p.label);
        }
        out.push('\n');
    }

    let mut depth = 1;
    for (index, item) in seq.items.iter().enumerate() {
        if matches!(item, Item::BlockEnd | Item::BlockBranch { .. }) {
            depth = (depth - 1).max(1);
        }
        out.push_str(&INDENT.repeat(depth));
        match item {
            Item::Message(m) => {
                let modifier = if m.activate {
                    "+"
                } else if m.deactivate {
                    "-"
                } else {
                    ""
                };
                let _ = write!(
                    out,
                    "{}{}{modifier}{}: {}",
                    m.from,
                    arrow_token(m.line, m.head),
                    m.to,
                    m.text
                );
            }
            Item::Note(n) => {
                let _ = write!(
                    out,
                    "Note {} {}: {}",
                    n.placement.keyword(),
                    n.participants.join(","),
                    n.text
                );
            }
            Item::Activate(id) => {
                let _ = write!(out, "activate {id}");
            }
            Item::Deactivate(id) => {
                let _ = write!(out, "deactivate {id}");
            }
            Item::BlockStart { kind, label } => {
                out.push_str(kind.keyword());
                push_label(&mut out, label);
                depth += 1;
            }
            Item::BlockBranch { label } => {
                let keyword = seq
                    .block_kind(index)
                    .and_then(|k| k.branch_keyword())
                    .unwrap_or("else");
                out.push_str(keyword);
                push_label(&mut out, label);
                depth += 1;
            }
            Item::BlockEnd => out.push_str("end"),
        }
        out.push('\n');
    }
    for line in &seq.passthrough {
        let _ = writeln!(out, "{INDENT}{line}");
    }
    out
}

fn push_label(out: &mut String, label: &str) {
    if !label.is_empty() {
        out.push(' ');
        out.push_str(label);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sequence::parser::parse;

    const SAMPLE: &str = "---
title: Demo
---
sequenceDiagram
    title Checkout
    autonumber
    actor U as Shopper
    participant API
    participant DB as Database
    U->>+API: POST /order
    alt in stock
        API->>DB: reserve
        DB-->>API: ok
    else out of stock
        API--xU: rejected
    end
    par
        API-)DB: log
    and
        API-)U: email
    end
    critical commit
        API->>DB: commit
    option timeout
        API->>API: retry
    end
    Note over API,DB: transactional
    Note left of U: waits
    activate DB
    deactivate DB
    API-->>-U: 201 Created
    rect rgb(200, 220, 255)
        U->U: think
    end
    %% trailing comment
";

    #[test]
    fn round_trips_sample_exactly() {
        let seq = parse(SAMPLE).unwrap();
        assert_eq!(serialize(&seq), SAMPLE);
    }

    #[test]
    fn declares_implicit_participants() {
        let seq = parse("sequenceDiagram\nA->>B: hi").unwrap();
        assert_eq!(
            serialize(&seq),
            "sequenceDiagram\n    participant A\n    participant B\n    A->>B: hi\n"
        );
    }
}
