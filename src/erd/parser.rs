//! Parser for Mermaid entity relationship diagrams.

use super::model::{Attribute, Cardinality, Direction, Erd, Relationship, is_name_char};
use crate::syntax::{ParseError, position_comment, split_source};

pub const KEYWORD: &str = "erDiagram";

/// Statements kept verbatim without being modelled.
const PASSTHROUGH_KEYWORDS: &[&str] = &["classDef", "class", "style"];

pub fn parse(src: &str) -> Result<Erd, ParseError> {
    let source = split_source(src)?;
    let (header_line, header) = source.header;
    if header != KEYWORD {
        return Err(ParseError::new(
            header_line,
            format!("Expected `{KEYWORD}`, found `{header}`"),
        ));
    }
    let mut erd = Erd {
        preamble: source.preamble,
        ..Erd::default()
    };
    let mut positions = Vec::new();
    // Entity whose `{ … }` attribute block is open, and the line it opened on.
    let mut open_block: Option<(String, usize)> = None;

    for (line_no, line) in source.body {
        if let Some(comment) = line.strip_prefix("%%") {
            match position_comment(comment) {
                Some(position) => positions.push(position),
                None => erd.passthrough.push(line.to_string()),
            }
            continue;
        }
        if let Some((id, _)) = &open_block {
            if line == "}" {
                open_block = None;
                continue;
            }
            let attribute = Attribute::parse(line).map_err(|m| ParseError::new(line_no, m))?;
            erd.ensure_entity(id).attributes.push(attribute);
            continue;
        }
        if let Some(id) =
            parse_statement(line, &mut erd).map_err(|m| ParseError::new(line_no, m))?
        {
            open_block = Some((id, line_no));
        }
    }
    if let Some((id, line)) = open_block {
        return Err(ParseError::new(
            line,
            format!("Attribute block of `{id}` is missing its closing `}}`"),
        ));
    }

    for (id, pos) in positions {
        if let Some(entity) = erd.entity_mut(&id) {
            entity.pos = Some(pos);
        }
    }
    Ok(erd)
}

/// Parses one statement. Returns the entity id when it opens an attribute block.
fn parse_statement(line: &str, erd: &mut Erd) -> Result<Option<String>, String> {
    let (keyword, rest) = match line.split_once(char::is_whitespace) {
        Some((k, r)) => (k, r.trim()),
        None => (line, ""),
    };
    if keyword == "direction" {
        erd.direction = Some(
            Direction::from_keyword(rest).ok_or_else(|| format!("Unknown direction `{rest}`"))?,
        );
        return Ok(None);
    }
    if PASSTHROUGH_KEYWORDS.contains(&keyword) {
        erd.passthrough.push(line.to_string());
        return Ok(None);
    }

    let (id, rest) = parse_name(line)?;
    let (alias, rest) = parse_alias(rest)?;
    let rest = rest.trim_start();
    let define = |erd: &mut Erd| {
        let entity = erd.ensure_entity(&id);
        if let Some(alias) = &alias {
            entity.label = alias.clone();
        }
    };

    if rest.is_empty() {
        define(erd);
        return Ok(None);
    }
    if let Some(block) = rest.strip_prefix('{') {
        define(erd);
        let block = block.trim();
        if block.is_empty() {
            return Ok(Some(id));
        }
        return match block.strip_suffix('}').map(str::trim) {
            Some("") => Ok(None),
            Some(attribute) => {
                let attribute = Attribute::parse(attribute)?;
                erd.ensure_entity(&id).attributes.push(attribute);
                Ok(None)
            }
            None => Err("Put each attribute on its own line inside `{ }`".into()),
        };
    }

    let (from_cardinality, identifying, to_cardinality, rest) = parse_relation(rest)?;
    let (to, rest) = parse_name(rest.trim_start())?;
    let label = rest
        .trim_start()
        .strip_prefix(':')
        .ok_or("Expected `:` followed by the relationship label")?
        .trim();
    let label = label
        .strip_prefix('"')
        .and_then(|l| l.strip_suffix('"'))
        .unwrap_or(label);

    define(erd);
    erd.ensure_entity(&to);
    erd.relationships.push(Relationship {
        from: id,
        to,
        from_cardinality,
        to_cardinality,
        identifying,
        label: label.to_string(),
    });
    Ok(None)
}

/// Parses a plain or quoted entity name, returning it and the remaining text.
fn parse_name(text: &str) -> Result<(String, &str), String> {
    if let Some(quoted) = text.strip_prefix('"') {
        let end = quoted
            .find('"')
            .ok_or("Entity name is missing its closing `\"`")?;
        if end == 0 {
            return Err("Entity names cannot be empty".into());
        }
        return Ok((quoted[..end].to_string(), &quoted[end + 1..]));
    }
    let end = text.find(|c| !is_name_char(c)).unwrap_or(text.len());
    if end == 0 {
        let preview: String = text.chars().take(16).collect();
        return Err(format!("Expected an entity name, found `{preview}`"));
    }
    Ok((text[..end].to_string(), &text[end..]))
}

/// Parses an optional `[Alias]` or `["Alias"]` directly after a name.
fn parse_alias(text: &str) -> Result<(Option<String>, &str), String> {
    let Some(inner) = text.strip_prefix('[') else {
        return Ok((None, text));
    };
    let end = inner.find(']').ok_or("Alias is missing its closing `]`")?;
    let alias = inner[..end].trim();
    let alias = alias
        .strip_prefix('"')
        .and_then(|a| a.strip_suffix('"'))
        .unwrap_or(alias);
    Ok((Some(alias.to_string()), &inner[end + 1..]))
}

/// Parses `||--o{` style markers or their word forms (`only one to zero or more`).
fn parse_relation(text: &str) -> Result<(Cardinality, bool, Cardinality, &str), String> {
    if let Some(symbols) = parse_relation_symbols(text) {
        return Ok(symbols);
    }
    let mut aliases: Vec<(&str, Cardinality)> = Cardinality::ALL
        .into_iter()
        .flat_map(|c| c.aliases().iter().map(move |a| (*a, c)))
        .collect();
    aliases.sort_by_key(|(a, _)| std::cmp::Reverse(a.len()));
    let take_alias = |text: &'static str, s: &str| -> Option<usize> {
        s.strip_prefix(text)
            .filter(|rest| rest.starts_with(char::is_whitespace))
            .map(|_| text.len())
    };
    let words = || -> Option<(Cardinality, bool, Cardinality, &str)> {
        let (len, from) = aliases
            .iter()
            .find_map(|(a, c)| Some((take_alias(a, text)?, *c)))?;
        let rest = text[len..].trim_start();
        let (identifying, rest) = if let Some(r) = rest.strip_prefix("optionally to") {
            (false, r)
        } else {
            (true, rest.strip_prefix("to")?)
        };
        let rest = rest.trim_start();
        let (len, to) = aliases
            .iter()
            .find_map(|(a, c)| Some((take_alias(a, rest)?, *c)))?;
        Some((from, identifying, to, &rest[len..]))
    };
    words().ok_or_else(|| {
        let preview: String = text.chars().take(16).collect();
        format!("Expected a relationship such as `||--o{{`, found `{preview}`")
    })
}

fn parse_relation_symbols(text: &str) -> Option<(Cardinality, bool, Cardinality, &str)> {
    let from = Cardinality::ALL
        .into_iter()
        .find(|c| text.starts_with(c.left_token()))?;
    let rest = &text[2..];
    let identifying = if rest.starts_with("--") {
        true
    } else if rest.starts_with("..") {
        false
    } else {
        return None;
    };
    let rest = &rest[2..];
    let to = Cardinality::ALL
        .into_iter()
        .find(|c| rest.starts_with(c.right_token()))?;
    Some((from, identifying, to, &rest[2..]))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::erd::model::Key;
    use crate::geometry::Point;

    #[test]
    fn parses_relationships() {
        let erd = parse(
            "erDiagram
                CUSTOMER ||--o{ ORDER : places
                CUSTOMER }|..|{ DELIVERY-ADDRESS : has
                INVOICE |o--o| ORDER : \"liable for\"",
        )
        .unwrap();
        let ids: Vec<_> = erd.entities.iter().map(|e| e.id.as_str()).collect();
        assert_eq!(ids, ["CUSTOMER", "ORDER", "DELIVERY-ADDRESS", "INVOICE"]);
        let r = &erd.relationships;
        assert_eq!(
            (r[0].from_cardinality, r[0].to_cardinality, r[0].identifying),
            (Cardinality::ExactlyOne, Cardinality::ZeroOrMore, true)
        );
        assert_eq!(
            (r[1].from_cardinality, r[1].to_cardinality, r[1].identifying),
            (Cardinality::OneOrMore, Cardinality::OneOrMore, false)
        );
        assert_eq!(r[2].label, "liable for");
        assert_eq!(r[2].from_cardinality, Cardinality::ZeroOrOne);
    }

    #[test]
    fn parses_word_relationships() {
        let erd = parse(
            "erDiagram
                MANUFACTURER only one to zero or more CAR : makes
                PERSON many(0) optionally to 1+ CAR : drives",
        )
        .unwrap();
        let r = &erd.relationships;
        assert_eq!(
            (r[0].from_cardinality, r[0].identifying, r[0].to_cardinality),
            (Cardinality::ExactlyOne, true, Cardinality::ZeroOrMore)
        );
        assert_eq!(
            (r[1].from_cardinality, r[1].identifying, r[1].to_cardinality),
            (Cardinality::ZeroOrMore, false, Cardinality::OneOrMore)
        );
    }

    #[test]
    fn parses_attribute_blocks_and_aliases() {
        let erd = parse(
            "erDiagram
                p[Person] {
                    string firstName PK
                    int age \"years\"
                }
                a[\"Customer Account\"] {
                }
                \"HOSPITAL\" { int id PK }
                STANDALONE
                p ||--o| a : has",
        )
        .unwrap();
        let p = erd.entity("p").unwrap();
        assert_eq!(p.label, "Person");
        assert_eq!(p.attributes.len(), 2);
        assert_eq!(p.attributes[0].keys, [Key::Primary]);
        assert_eq!(erd.entity("a").unwrap().label, "Customer Account");
        assert_eq!(erd.entity("HOSPITAL").unwrap().attributes[0].name, "id");
        assert!(erd.entity("STANDALONE").is_some());
        assert_eq!(erd.relationships.len(), 1);
    }

    #[test]
    fn parses_direction_comments_and_positions() {
        let erd = parse(
            "erDiagram\n direction LR\n A ||--|| B : x\n %% hello\n classDef big font-size:20px\n%% @reefgraph pos A 5 6",
        )
        .unwrap();
        assert_eq!(erd.direction, Some(Direction::LeftRight));
        assert_eq!(erd.entity("A").unwrap().pos, Some(Point::new(5.0, 6.0)));
        assert_eq!(erd.passthrough, ["%% hello", "classDef big font-size:20px"]);
    }

    #[test]
    fn reports_errors_with_lines() {
        let err = parse("erDiagram\n A {\n  int\n }").unwrap_err();
        assert_eq!(err.line, 3);
        let err = parse("erDiagram\n A {\n  int id").unwrap_err();
        assert_eq!(err.line, 2);
        assert!(parse("erDiagram\n A ||--o{ B").is_err());
        assert!(parse("erDiagram\n A -> B : x").is_err());
        assert!(parse("erDiagram\n direction XY").is_err());
        assert!(parse("flowchart TD").is_err());
    }
}
