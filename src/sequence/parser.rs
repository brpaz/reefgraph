//! Parser for the subset of Mermaid sequence diagram syntax the editor models.

use super::model::{
    ARROWS, BlockKind, Item, Message, Note, NotePlacement, ParticipantKind, Sequence,
};
use crate::ids::is_id_char;
use crate::syntax::{ParseError, split_source, split_statements};

pub const KEYWORD: &str = "sequenceDiagram";

/// Statements kept verbatim without being modelled.
const PASSTHROUGH_KEYWORDS: &[&str] = &["links", "link", "properties", "details"];
const UNSUPPORTED_KEYWORDS: &[&str] = &["box", "create", "destroy"];

pub fn parse(src: &str) -> Result<Sequence, ParseError> {
    let source = split_source(src)?;
    let (header_line, header) = source.header;
    if header != KEYWORD {
        return Err(ParseError::new(
            header_line,
            format!("Expected `{KEYWORD}`, found `{header}`"),
        ));
    }
    let mut seq = Sequence {
        preamble: source.preamble,
        ..Sequence::default()
    };
    let mut open_blocks = Vec::new();
    let mut last_line = header_line;
    for (line_no, line) in source.body {
        last_line = line_no;
        if line.starts_with("%%") {
            seq.passthrough.push(line.to_string());
            continue;
        }
        for statement in split_statements(line) {
            parse_statement(statement, &mut seq, &mut open_blocks)
                .map_err(|m| ParseError::new(line_no, m))?;
        }
    }
    if let Some(kind) = open_blocks.last() {
        return Err(ParseError::new(
            last_line,
            format!("`{}` block is missing its `end`", kind.keyword()),
        ));
    }
    Ok(seq)
}

/// Splits `keyword rest` into the first word and the trimmed remainder.
fn split_keyword(statement: &str) -> (&str, &str) {
    match statement.split_once(char::is_whitespace) {
        Some((keyword, rest)) => (keyword, rest.trim()),
        None => (statement, ""),
    }
}

fn parse_statement(
    statement: &str,
    seq: &mut Sequence,
    open_blocks: &mut Vec<BlockKind>,
) -> Result<(), String> {
    if let Some(rest) = statement.strip_prefix("title")
        && rest.starts_with([':', ' ', '\t'])
    {
        seq.title = Some(rest.trim_start_matches(':').trim().to_string());
        return Ok(());
    }
    let (keyword, rest) = split_keyword(statement);
    match keyword {
        "participant" | "actor" => {
            let kind = if keyword == "actor" {
                ParticipantKind::Actor
            } else {
                ParticipantKind::Participant
            };
            parse_participant(rest, kind, seq)
        }
        "autonumber" => {
            seq.autonumber = true;
            Ok(())
        }
        "activate" | "deactivate" => {
            let id = parse_id(rest)?;
            seq.ensure_participant(&id);
            seq.items.push(if keyword == "activate" {
                Item::Activate(id)
            } else {
                Item::Deactivate(id)
            });
            Ok(())
        }
        "end" => {
            open_blocks.pop().ok_or("`end` without a matching block")?;
            seq.items.push(Item::BlockEnd);
            Ok(())
        }
        _ if keyword.eq_ignore_ascii_case("note") => parse_note(rest, seq),
        _ if PASSTHROUGH_KEYWORDS.contains(&keyword) => {
            seq.passthrough.push(statement.to_string());
            Ok(())
        }
        _ if UNSUPPORTED_KEYWORDS.contains(&keyword) => {
            Err(format!("`{keyword}` is not supported yet"))
        }
        _ => {
            if let Some(kind) = BlockKind::from_keyword(keyword) {
                open_blocks.push(kind);
                seq.items.push(Item::BlockStart {
                    kind,
                    label: rest.to_string(),
                });
                return Ok(());
            }
            if let Some(open) = branch_owner(keyword) {
                return match open_blocks.last() {
                    Some(kind) if kind.branch_keyword() == Some(keyword) => {
                        seq.items.push(Item::BlockBranch {
                            label: rest.to_string(),
                        });
                        Ok(())
                    }
                    _ => Err(format!("`{keyword}` is only allowed inside `{open}`")),
                };
            }
            parse_message(statement, seq)
        }
    }
}

/// Block keyword that allows the given branch keyword.
fn branch_owner(keyword: &str) -> Option<&'static str> {
    BlockKind::ALL
        .into_iter()
        .find(|k| k.branch_keyword() == Some(keyword))
        .map(BlockKind::keyword)
}

fn parse_id(text: &str) -> Result<String, String> {
    if !text.is_empty() && text.chars().all(is_id_char) {
        Ok(text.to_string())
    } else {
        Err(format!(
            "`{text}` is not a valid participant id; use letters, digits and `_`"
        ))
    }
}

fn parse_participant(rest: &str, kind: ParticipantKind, seq: &mut Sequence) -> Result<(), String> {
    let (id, label) = match rest.split_once(" as ") {
        Some((id, label)) => (id.trim(), Some(label.trim())),
        None => (rest, None),
    };
    let id = parse_id(id)?;
    let participant = seq.ensure_participant(&id);
    participant.kind = kind;
    if let Some(label) = label {
        participant.label = label.to_string();
    }
    Ok(())
}

fn parse_note(rest: &str, seq: &mut Sequence) -> Result<(), String> {
    let (target, text) = rest
        .split_once(':')
        .ok_or("Expected `:` followed by the note text")?;
    let target = target.trim();
    let (placement, ids) = NotePlacement::ALL
        .into_iter()
        .find_map(|p| Some((p, target.strip_prefix(p.keyword())?)))
        .ok_or("Expected `left of`, `right of` or `over` after `Note`")?;
    let participants = ids
        .split(',')
        .map(|id| parse_id(id.trim()))
        .collect::<Result<Vec<_>, _>>()?;
    if participants.len() > 1 && placement != NotePlacement::Over {
        return Err("Only `Note over` can span two participants".into());
    }
    if participants.len() > 2 {
        return Err("A note can span at most two participants".into());
    }
    for id in &participants {
        seq.ensure_participant(id);
    }
    seq.items.push(Item::Note(Note {
        placement,
        participants,
        text: text.trim().to_string(),
    }));
    Ok(())
}

fn parse_message(statement: &str, seq: &mut Sequence) -> Result<(), String> {
    let from_end = statement
        .find(|c| !is_id_char(c))
        .unwrap_or(statement.len());
    let from = &statement[..from_end];
    if from.is_empty() {
        return Err(format!("Unexpected `{statement}`"));
    }
    let rest = statement[from_end..].trim_start();
    let Some((token, line, head)) = ARROWS.iter().find(|(t, _, _)| rest.starts_with(*t)) else {
        return Err(format!("Unknown statement `{statement}`"));
    };
    let mut rest = rest[token.len()..].trim_start();
    let mut activate = false;
    let mut deactivate = false;
    if let Some(r) = rest.strip_prefix('+') {
        activate = true;
        rest = r.trim_start();
    } else if let Some(r) = rest.strip_prefix('-') {
        deactivate = true;
        rest = r.trim_start();
    }
    let to_end = rest.find(|c| !is_id_char(c)).unwrap_or(rest.len());
    let to = &rest[..to_end];
    if to.is_empty() {
        return Err("Expected the receiving participant after the arrow".into());
    }
    let text = rest[to_end..]
        .trim_start()
        .strip_prefix(':')
        .ok_or("Expected `:` followed by the message text")?;

    seq.ensure_participant(from);
    seq.ensure_participant(to);
    seq.items.push(Item::Message(Message {
        from: from.to_string(),
        to: to.to_string(),
        text: text.trim().to_string(),
        line: *line,
        head: *head,
        activate,
        deactivate,
    }));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sequence::model::{ArrowHead, LineStyle};

    fn messages(seq: &Sequence) -> Vec<&Message> {
        seq.items
            .iter()
            .filter_map(|i| match i {
                Item::Message(m) => Some(m),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn parses_participants_in_order() {
        let seq = parse(
            "sequenceDiagram\n  actor U as User\n  participant S\n  U->>S: hi\n  S->>DB: query",
        )
        .unwrap();
        let ids: Vec<_> = seq.participants.iter().map(|p| p.id.as_str()).collect();
        assert_eq!(ids, ["U", "S", "DB"]);
        assert_eq!(seq.participants[0].label, "User");
        assert_eq!(seq.participants[0].kind, ParticipantKind::Actor);
        assert_eq!(seq.participants[2].label, "DB");
    }

    #[test]
    fn parses_every_arrow() {
        let mut src = String::from("sequenceDiagram\n");
        for (token, _, _) in ARROWS {
            src.push_str(&format!("A{token}B: m\n"));
        }
        let seq = parse(&src).unwrap();
        let parsed: Vec<_> = messages(&seq).iter().map(|m| (m.line, m.head)).collect();
        let expected: Vec<_> = ARROWS.iter().map(|(_, l, h)| (*l, *h)).collect();
        assert_eq!(parsed, expected);
    }

    #[test]
    fn parses_activation_shorthand_and_text() {
        let seq = parse("sequenceDiagram\n A->>+B: Hello there\n B-->>-A: Hi: again").unwrap();
        let m = messages(&seq);
        assert!(m[0].activate && !m[0].deactivate);
        assert_eq!(m[0].text, "Hello there");
        assert!(m[1].deactivate);
        assert_eq!(m[1].text, "Hi: again");
        assert_eq!(
            (m[1].line, m[1].head),
            (LineStyle::Dotted, ArrowHead::Filled)
        );
    }

    #[test]
    fn parses_notes() {
        let seq =
            parse("sequenceDiagram\n Note right of A: r\n note left of B: l\n Note over A,B: both")
                .unwrap();
        let notes: Vec<_> = seq
            .items
            .iter()
            .filter_map(|i| match i {
                Item::Note(n) => Some((n.placement, n.participants.len(), n.text.as_str())),
                _ => None,
            })
            .collect();
        assert_eq!(
            notes,
            [
                (NotePlacement::RightOf, 1, "r"),
                (NotePlacement::LeftOf, 1, "l"),
                (NotePlacement::Over, 2, "both"),
            ]
        );
    }

    #[test]
    fn parses_blocks_and_branches() {
        let src = "sequenceDiagram
            alt is sick
                A->>B: not so good
            else is well
                loop every minute
                    A->>B: fine
                end
            end
            par
                A->>B: x
            and
                A->>B: y
            end";
        let seq = parse(src).unwrap();
        assert_eq!(
            seq.items[0],
            Item::BlockStart {
                kind: BlockKind::Alt,
                label: "is sick".into()
            }
        );
        assert_eq!(
            seq.items[2],
            Item::BlockBranch {
                label: "is well".into()
            }
        );
        assert!(seq.validate().is_ok());
    }

    #[test]
    fn rejects_misplaced_branches_and_unbalanced_blocks() {
        assert!(parse("sequenceDiagram\n loop x\n else y\n end").is_err());
        let err = parse("sequenceDiagram\n opt x\n A->>B: m").unwrap_err();
        assert_eq!(err.line, 3);
        assert!(parse("sequenceDiagram\n end").is_err());
    }

    #[test]
    fn parses_title_autonumber_activations_and_passthrough() {
        let seq = parse(
            "sequenceDiagram\n title: Login\n autonumber\n activate A\n deactivate A\n %% note to self\n links A: {\"x\": \"y\"}",
        )
        .unwrap();
        assert_eq!(seq.title.as_deref(), Some("Login"));
        assert!(seq.autonumber);
        assert_eq!(
            seq.items,
            [Item::Activate("A".into()), Item::Deactivate("A".into())]
        );
        assert_eq!(seq.passthrough.len(), 2);
    }

    #[test]
    fn reports_bad_messages() {
        assert!(parse("sequenceDiagram\n A->>B hi").is_err());
        assert!(parse("sequenceDiagram\n A=>B: hi").is_err());
        assert!(parse("sequenceDiagram\n box Aqua").is_err());
        assert!(parse("flowchart TD").is_err());
    }
}
