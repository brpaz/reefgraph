//! Parser for Mermaid state diagrams.

use std::collections::HashSet;

use super::model::{Direction, END_ID, START_ID, StateDiagram, StateKind};
use crate::shared::ids::is_id_char;
use crate::shared::syntax::{ParseError, position_comment, split_source};

pub const KEYWORD: &str = "stateDiagram";
pub const KEYWORD_V2: &str = "stateDiagram-v2";

/// Statements kept verbatim without being modelled.
const PASSTHROUGH_KEYWORDS: &[&str] = &[
    "classDef", "class", "style", "hide", "scale", "accTitle", "accDescr",
];

/// Written for both the start and the end state, told apart by which side
/// of a transition it is on.
pub const TERMINAL: &str = "[*]";

pub fn parse(src: &str) -> Result<StateDiagram, ParseError> {
    let source = split_source(src)?;
    let (header_line, header) = source.header;
    let v2 = match header {
        KEYWORD => false,
        KEYWORD_V2 => true,
        _ => {
            return Err(ParseError::new(
                header_line,
                format!("Expected `{KEYWORD_V2}`, found `{header}`"),
            ));
        }
    };
    let mut diagram = StateDiagram {
        v2,
        preamble: source.preamble,
        ..StateDiagram::default()
    };
    let mut positions = Vec::new();
    // Line a multi-line note opened on, while its `end note` is pending.
    let mut open_note: Option<usize> = None;
    // States given a description so far; later descriptions add lines.
    let mut described = HashSet::new();

    for (line_no, line) in source.body {
        if open_note.is_some() {
            diagram.passthrough.push(line.to_string());
            if line == "end note" {
                open_note = None;
            }
            continue;
        }
        if let Some(comment) = line.strip_prefix("%%") {
            match position_comment(comment) {
                Some(position) => positions.push(position),
                None => diagram.passthrough.push(line.to_string()),
            }
            continue;
        }
        if line.starts_with("note ") {
            diagram.passthrough.push(line.to_string());
            if !line.contains(':') {
                open_note = Some(line_no);
            }
            continue;
        }
        parse_statement(line, &mut diagram, &mut described)
            .map_err(|m| ParseError::new(line_no, m))?;
    }
    if let Some(line) = open_note {
        return Err(ParseError::new(line, "Note is missing its `end note`"));
    }

    for (id, pos) in positions {
        if id == START_ID || id == END_ID {
            diagram.ensure_state(&id);
        }
        if let Some(state) = diagram.state_mut(&id) {
            state.pos = Some(pos);
        }
    }
    Ok(diagram)
}

fn parse_statement(
    line: &str,
    diagram: &mut StateDiagram,
    described: &mut HashSet<String>,
) -> Result<(), String> {
    let (keyword, rest) = match line.split_once(char::is_whitespace) {
        Some((k, r)) => (k, r.trim()),
        None => (line, ""),
    };
    match keyword {
        "direction" => {
            diagram.direction = Some(
                Direction::from_keyword(rest)
                    .ok_or_else(|| format!("Unknown direction `{rest}`"))?,
            );
            return Ok(());
        }
        "state" => return parse_state(rest, diagram, described),
        "--" => return Err("Concurrent regions (`--`) are not supported yet".into()),
        _ if PASSTHROUGH_KEYWORDS.contains(&keyword) => {
            diagram.passthrough.push(line.to_string());
            return Ok(());
        }
        _ => {}
    }

    if let Some((from, rest)) = line.split_once("-->") {
        let from = parse_end(from.trim(), START_ID)?;
        let (to, label) = match rest.split_once(':') {
            Some((to, label)) => (to, label.trim()),
            None => (rest, ""),
        };
        let to = parse_end(to.trim(), END_ID)?;
        diagram.ensure_state(&from);
        diagram.ensure_state(&to);
        diagram.transitions.push(super::Transition {
            from,
            to,
            label: label.to_string(),
        });
        return Ok(());
    }

    let (id, description) = match line.split_once(':') {
        Some((id, text)) => (id.trim(), Some(text.trim())),
        None => (line, None),
    };
    let id = parse_id(id)?;
    if let Some(text) = description {
        add_description(diagram, described, &id, text);
    } else {
        diagram.ensure_state(&id);
    }
    Ok(())
}

/// Parses what follows `state`: `"Description" as id`, `id <<choice>>`,
/// `id : description` or a plain `id`.
fn parse_state(
    rest: &str,
    diagram: &mut StateDiagram,
    described: &mut HashSet<String>,
) -> Result<(), String> {
    if rest.ends_with('{') {
        return Err("Composite states are not supported yet".into());
    }
    if let Some(quoted) = rest.strip_prefix('"') {
        let (label, after) = quoted
            .split_once('"')
            .ok_or("State description is missing its closing `\"`")?;
        let id = after
            .trim()
            .strip_prefix("as ")
            .ok_or("Expected `as` and an id after the state description")?;
        let id = parse_id(id.trim())?;
        diagram.ensure_state(&id).label = label.to_string();
        described.insert(id);
        return Ok(());
    }
    if let Some((id, stereotype)) = rest.split_once("<<") {
        let name = stereotype
            .strip_suffix(">>")
            .ok_or("Stereotype is missing its closing `>>`")?
            .trim();
        let kind = StateKind::from_stereotype(name)
            .ok_or_else(|| format!("Unknown state type `<<{name}>>`"))?;
        let id = parse_id(id.trim())?;
        diagram.ensure_state(&id).kind = kind;
        return Ok(());
    }
    let (id, description) = match rest.split_once(':') {
        Some((id, text)) => (id.trim(), Some(text.trim())),
        None => (rest, None),
    };
    let id = parse_id(id)?;
    if let Some(text) = description {
        add_description(diagram, described, &id, text);
    } else {
        diagram.ensure_state(&id);
    }
    Ok(())
}

/// The first description replaces the id as the label; later ones add lines.
fn add_description(
    diagram: &mut StateDiagram,
    described: &mut HashSet<String>,
    id: &str,
    text: &str,
) {
    let state = diagram.ensure_state(id);
    if described.insert(id.to_string()) {
        state.label = text.to_string();
    } else {
        state.label = format!("{}\n{text}", state.label);
    }
}

/// A transition end: `[*]` is `terminal`, anything else a state id.
fn parse_end(text: &str, terminal: &str) -> Result<String, String> {
    if text == TERMINAL {
        Ok(terminal.to_string())
    } else {
        parse_id(text)
    }
}

fn parse_id(text: &str) -> Result<String, String> {
    if text.contains(":::") {
        return Err("Styling states with `:::` is not supported yet".into());
    }
    if text.is_empty() || !text.chars().all(is_id_char) {
        let preview: String = text.chars().take(16).collect();
        return Err(format!("Expected a state name, found `{preview}`"));
    }
    Ok(text.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shared::geometry::Point;

    #[test]
    fn parses_states_transitions_and_terminals() {
        let d = parse(
            "stateDiagram-v2
                [*] --> Still
                Still --> Moving : push
                Moving --> Still
                Moving --> [*]",
        )
        .unwrap();
        assert!(d.v2);
        let ids: Vec<_> = d.states.iter().map(|s| s.id.as_str()).collect();
        assert_eq!(ids, [START_ID, "Still", "Moving", END_ID]);
        assert_eq!(d.state(START_ID).unwrap().kind, StateKind::Start);
        assert_eq!(d.state(END_ID).unwrap().kind, StateKind::End);
        assert_eq!(d.transitions[1].label, "push");
        assert_eq!(d.transitions[3].to, END_ID);
    }

    #[test]
    fn parses_descriptions_and_pseudo_states() {
        let d = parse(
            "stateDiagram
                state \"Waiting for input\" as Wait
                Busy : Working
                Busy : on it
                state check <<choice>>
                state split <<fork>>
                state merge <<join>>
                Plain",
        )
        .unwrap();
        assert!(!d.v2);
        assert_eq!(d.state("Wait").unwrap().label, "Waiting for input");
        assert_eq!(d.state("Busy").unwrap().label, "Working\non it");
        assert_eq!(d.state("check").unwrap().kind, StateKind::Choice);
        assert_eq!(d.state("split").unwrap().kind, StateKind::Fork);
        assert_eq!(d.state("merge").unwrap().kind, StateKind::Join);
        assert_eq!(d.state("Plain").unwrap().label, "Plain");
    }

    #[test]
    fn keeps_notes_passthrough_direction_and_positions() {
        let d = parse(
            "stateDiagram-v2
                direction LR
                A --> B
                note right of A : single
                note left of B
                    two
                    lines
                end note
                classDef hot fill:#f00
                %% comment
                %% @reefgraph pos A 10 20
                %% @reefgraph pos [*]end 5 6",
        )
        .unwrap();
        assert_eq!(d.direction, Some(Direction::LeftRight));
        assert_eq!(
            d.passthrough,
            [
                "note right of A : single",
                "note left of B",
                "two",
                "lines",
                "end note",
                "classDef hot fill:#f00",
                "%% comment",
            ]
        );
        assert_eq!(d.state("A").unwrap().pos, Some(Point::new(10.0, 20.0)));
        // A positioned end state exists even before a transition reaches it.
        assert_eq!(d.state(END_ID).unwrap().pos, Some(Point::new(5.0, 6.0)));
    }

    #[test]
    fn reports_errors() {
        assert!(parse("stateDiagram-v2\n state Composite {").is_err());
        assert!(parse("stateDiagram-v2\n A --> B\n --").is_err());
        assert!(parse("stateDiagram-v2\n A:::hot --> B").is_err());
        assert!(parse("stateDiagram-v2\n state x <<history>>").is_err());
        assert!(parse("stateDiagram-v2\n bad name").is_err());
        assert_eq!(
            parse("stateDiagram-v2\n note left of A\n text")
                .unwrap_err()
                .line,
            2
        );
        assert!(parse("classDiagram").is_err());
    }
}
