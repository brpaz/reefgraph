//! Parser for Mermaid class diagrams.

use super::model::{ClassDiagram, Direction, Head, Member, Relation};
use crate::ids::is_id_char;
use crate::syntax::{ParseError, position_comment, split_source};

pub const KEYWORD: &str = "classDiagram";
pub const KEYWORD_V2: &str = "classDiagram-v2";

/// Statements kept verbatim without being modelled.
const PASSTHROUGH_KEYWORDS: &[&str] = &[
    "note", "click", "callback", "link", "cssClass", "classDef", "style",
];

pub fn parse(src: &str) -> Result<ClassDiagram, ParseError> {
    let source = split_source(src)?;
    let (header_line, header) = source.header;
    let v2 = match header {
        KEYWORD => false,
        KEYWORD_V2 => true,
        _ => {
            return Err(ParseError::new(
                header_line,
                format!("Expected `{KEYWORD}`, found `{header}`"),
            ));
        }
    };
    let mut diagram = ClassDiagram {
        v2,
        preamble: source.preamble,
        ..ClassDiagram::default()
    };
    let mut positions = Vec::new();
    // Class whose `{ … }` body is open, and the line it opened on.
    let mut open_body: Option<(String, usize)> = None;

    for (line_no, line) in source.body {
        if let Some(comment) = line.strip_prefix("%%") {
            match position_comment(comment) {
                Some(position) => positions.push(position),
                None => diagram.passthrough.push(line.to_string()),
            }
            continue;
        }
        if let Some((id, _)) = &open_body {
            if line == "}" {
                open_body = None;
            } else if let Some(annotation) = parse_annotation(line) {
                diagram
                    .ensure_class(id)
                    .annotations
                    .push(annotation.to_string());
            } else {
                diagram
                    .ensure_class(id)
                    .members
                    .push(Member(line.to_string()));
            }
            continue;
        }
        if let Some(id) =
            parse_statement(line, &mut diagram).map_err(|m| ParseError::new(line_no, m))?
        {
            open_body = Some((id, line_no));
        }
    }
    if let Some((id, line)) = open_body {
        return Err(ParseError::new(
            line,
            format!("Body of class `{id}` is missing its closing `}}`"),
        ));
    }

    for (id, pos) in positions {
        if let Some(class) = diagram.class_mut(&id) {
            class.pos = Some(pos);
        }
    }
    Ok(diagram)
}

/// `<<interface>>` → `interface`.
fn parse_annotation(text: &str) -> Option<&str> {
    text.strip_prefix("<<")?.strip_suffix(">>").map(str::trim)
}

/// Parses one statement. Returns the class id when it opens a class body.
fn parse_statement(line: &str, diagram: &mut ClassDiagram) -> Result<Option<String>, String> {
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
            return Ok(None);
        }
        "class" => return parse_class(rest, diagram),
        "namespace" => return Err("`namespace` is not supported yet".into()),
        _ if PASSTHROUGH_KEYWORDS.contains(&keyword) => {
            diagram.passthrough.push(line.to_string());
            return Ok(None);
        }
        _ => {}
    }
    if let Some(close) = line.find(">>").filter(|_| line.starts_with("<<")) {
        let annotation = line[2..close].trim().to_string();
        let (id, rest) = parse_name(line[close + 2..].trim_start())?;
        expect_end(rest)?;
        diagram.ensure_class(&id).annotations.push(annotation);
        return Ok(None);
    }

    let (from, rest) = parse_name(line)?;
    let rest = rest.trim_start();
    if let Some(member) = rest.strip_prefix(':') {
        diagram
            .ensure_class(&from)
            .members
            .push(Member(member.trim().to_string()));
        return Ok(None);
    }
    parse_relation(&from, rest, diagram)?;
    Ok(None)
}

/// Parses `Name~T~["Label"] {` after the `class` keyword.
fn parse_class(rest: &str, diagram: &mut ClassDiagram) -> Result<Option<String>, String> {
    let (id, mut rest) = parse_name(rest)?;
    let mut generic = None;
    if let Some(after) = rest.strip_prefix('~') {
        // Generics may nest (`List~List~int~~`), so the parameter list runs to
        // the last `~` before any label or body.
        let end_of_decl = after.find(['[', '{', ' ']).unwrap_or(after.len());
        let close = after[..end_of_decl]
            .rfind('~')
            .ok_or("Generic type is missing its closing `~`")?;
        generic = Some(after[..close].to_string());
        rest = &after[close + 1..];
    }
    let mut label = None;
    if let Some(after) = rest.strip_prefix('[') {
        let close = after
            .find(']')
            .ok_or("Class label is missing its closing `]`")?;
        let text = after[..close].trim();
        label = Some(
            text.strip_prefix('"')
                .and_then(|t| t.strip_suffix('"'))
                .unwrap_or(text)
                .to_string(),
        );
        rest = &after[close + 1..];
    }
    if rest.starts_with(":::") {
        return Err("Styling classes with `:::` is not supported yet".into());
    }

    let class = diagram.ensure_class(&id);
    if let Some(generic) = generic {
        class.generic = generic;
    }
    if let Some(label) = label {
        class.label = label;
    }
    let rest = rest.trim();
    match rest.strip_prefix('{').map(str::trim) {
        None if rest.is_empty() => Ok(None),
        None => Err(format!("Unexpected `{rest}` after the class name")),
        Some("") => Ok(Some(id)),
        Some("}") => Ok(None),
        Some(_) => Err("Put each member on its own line inside `{ }`".into()),
    }
}

/// Parses a class name, stopping before a relation's `--` or `..`.
fn parse_name(text: &str) -> Result<(String, &str), String> {
    let mut end = 0;
    for (i, c) in text.char_indices() {
        let line_starts = text[i..].starts_with("--") || text[i..].starts_with("..");
        if !(is_id_char(c) || c == '-') || line_starts {
            break;
        }
        end = i + c.len_utf8();
    }
    if end == 0 {
        let preview: String = text.chars().take(16).collect();
        return Err(format!("Expected a class name, found `{preview}`"));
    }
    Ok((text[..end].to_string(), &text[end..]))
}

fn expect_end(rest: &str) -> Result<(), String> {
    let rest = rest.trim();
    if rest.is_empty() {
        Ok(())
    } else {
        Err(format!("Unexpected `{rest}`"))
    }
}

/// Parses an optional `"cardinality"`.
fn parse_cardinality(text: &str) -> Result<(String, &str), String> {
    let text = text.trim_start();
    match text.strip_prefix('"') {
        Some(quoted) => {
            let end = quoted
                .find('"')
                .ok_or("Cardinality is missing its closing `\"`")?;
            Ok((quoted[..end].to_string(), &quoted[end + 1..]))
        }
        None => Ok((String::new(), text)),
    }
}

fn parse_relation(from: &str, rest: &str, diagram: &mut ClassDiagram) -> Result<(), String> {
    let expected = || {
        let preview: String = rest.chars().take(16).collect();
        format!("Expected `:` and a member, or a relation such as `<|--`, found `{preview}`")
    };
    let (from_cardinality, rest) = parse_cardinality(rest)?;
    let rest = rest.trim_start();

    let from_head = Head::ALL
        .into_iter()
        .filter(|h| *h != Head::None)
        .find(|h| {
            rest.strip_prefix(h.left_token())
                .is_some_and(|r| r.starts_with("--") || r.starts_with(".."))
        })
        .unwrap_or_default();
    let rest = &rest[from_head.left_token().len()..];
    let dashed = if rest.starts_with("..") {
        true
    } else if rest.starts_with("--") {
        false
    } else {
        return Err(expected());
    };
    let rest = &rest[2..];
    // A right-hand `o` is only a head when no class name follows directly.
    let to_head = Head::ALL
        .into_iter()
        .filter(|h| *h != Head::None)
        .find(|h| {
            rest.strip_prefix(h.right_token())
                .is_some_and(|r| !r.starts_with(|c: char| is_id_char(c)))
        })
        .unwrap_or_default();
    let rest = &rest[to_head.right_token().len()..];

    let (to_cardinality, rest) = parse_cardinality(rest)?;
    let (to, rest) = parse_name(rest.trim_start())?;
    let rest = rest.trim_start();
    let label = match rest.strip_prefix(':') {
        Some(label) => label.trim().to_string(),
        None if rest.is_empty() => String::new(),
        None => return Err(format!("Unexpected `{rest}` after the relation")),
    };

    diagram.ensure_class(from);
    diagram.ensure_class(&to);
    diagram.relations.push(Relation {
        from: from.to_string(),
        to,
        from_head,
        to_head,
        dashed,
        from_cardinality,
        to_cardinality,
        label,
    });
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::Point;

    fn heads(d: &ClassDiagram, i: usize) -> (Head, bool, Head) {
        let r = &d.relations[i];
        (r.from_head, r.dashed, r.to_head)
    }

    #[test]
    fn parses_every_relation_kind() {
        let d = parse(
            "classDiagram
                A <|-- B
                C *-- D
                E o-- F
                G --> H
                I -- J
                K ..> L
                M ..|> N
                O .. P
                Q <|--|> R
                S --() T
                U ()-- V
                W --o X",
        )
        .unwrap();
        use Head::*;
        let expected = [
            (Inheritance, false, None),
            (Composition, false, None),
            (Aggregation, false, None),
            (None, false, Arrow),
            (None, false, None),
            (None, true, Arrow),
            (None, true, Inheritance),
            (None, true, None),
            (Inheritance, false, Inheritance),
            (None, false, Lollipop),
            (Lollipop, false, None),
            (None, false, Aggregation),
        ];
        for (i, e) in expected.into_iter().enumerate() {
            assert_eq!(heads(&d, i), e, "relation {i}");
        }
    }

    #[test]
    fn parses_cardinalities_and_labels() {
        let d = parse(
            "classDiagram\n Customer \"1\" --> \"*\" Ticket : buys\n Galaxy --> \"many\" Star : Contains",
        )
        .unwrap();
        let r = &d.relations[0];
        assert_eq!(
            (r.from_cardinality.as_str(), r.to_cardinality.as_str()),
            ("1", "*")
        );
        assert_eq!(r.label, "buys");
        assert_eq!(d.relations[1].to_cardinality, "many");
        assert_eq!(d.relations[1].from_cardinality, "");
    }

    #[test]
    fn parses_class_bodies_members_and_annotations() {
        let d = parse(
            "classDiagram
                class Duck~T~[\"Rubber Duck\"] {
                    <<interface>>
                    +String beakColor
                    +swim()
                }
                class Empty
                class Inline {}
                Duck : +quack() bool
                <<abstract>> Empty
                Animal <|-- Duck",
        )
        .unwrap();
        let duck = d.class("Duck").unwrap();
        assert_eq!(duck.label, "Rubber Duck");
        assert_eq!(duck.generic, "T");
        assert_eq!(duck.annotations, ["interface"]);
        let members: Vec<_> = duck.members.iter().map(|m| m.0.as_str()).collect();
        assert_eq!(members, ["+String beakColor", "+swim()", "+quack() bool"]);
        assert_eq!(d.class("Empty").unwrap().annotations, ["abstract"]);
        assert!(d.class("Inline").is_some());
        assert_eq!(d.classes.len(), 4);
    }

    #[test]
    fn parses_nested_generics() {
        let d = parse("classDiagram\n class People~List~Person~~").unwrap();
        assert_eq!(d.classes[0].generic, "List~Person~");
    }

    #[test]
    fn keeps_header_direction_passthrough_and_positions() {
        let d = parse(
            "classDiagram-v2\n direction RL\n note for A \"hi\"\n A --> B\n click A href \"x\"\n%% @merman pos A 1 2",
        )
        .unwrap();
        assert!(d.v2);
        assert_eq!(d.direction, Some(Direction::RightLeft));
        assert_eq!(d.passthrough, ["note for A \"hi\"", "click A href \"x\""]);
        assert_eq!(d.class("A").unwrap().pos, Some(Point::new(1.0, 2.0)));
    }

    #[test]
    fn reports_errors() {
        assert_eq!(parse("classDiagram\n class A {\n  x").unwrap_err().line, 2);
        assert!(parse("classDiagram\n namespace N {").is_err());
        assert!(parse("classDiagram\n A -> B").is_err());
        assert!(parse("classDiagram\n class A:::hot").is_err());
        assert!(parse("classDiagram\n A --> B extra").is_err());
        assert!(parse("erDiagram").is_err());
    }
}
