//! Parser for Mermaid use case diagrams (`usecase-beta`).

use super::model::{
    Boundary, Direction, ElementKind, Label, Relation, RelationKind, UseCaseDiagram,
};
use crate::ids::is_id_char;
use crate::syntax::{ParseError, position_comment, split_source};

pub const KEYWORD: &str = "usecase-beta";

/// Statements kept verbatim without being modelled.
const PASSTHROUGH_KEYWORDS: &[&str] = &["classDef", "class", "style", "note", "accTitle:"];

pub fn parse(src: &str) -> Result<UseCaseDiagram, ParseError> {
    let source = split_source(src)?;
    let (header_line, header) = source.header;
    if header != KEYWORD {
        return Err(ParseError::new(
            header_line,
            format!("Expected `{KEYWORD}`, found `{header}`"),
        ));
    }
    let mut diagram = UseCaseDiagram {
        preamble: source.preamble,
        ..UseCaseDiagram::default()
    };
    let mut positions = Vec::new();
    let mut block = Block::None;

    for (line_no, line) in source.body {
        let error = |m: String| ParseError::new(line_no, m);
        if let Some(comment) = line.strip_prefix("%%") {
            match position_comment(comment) {
                Some(position) => positions.push(position),
                None => diagram.passthrough.push(line.to_string()),
            }
            continue;
        }
        match &block {
            Block::Description(_) => {
                diagram.passthrough.push(line.to_string());
                if line == "}" {
                    block = Block::None;
                }
                continue;
            }
            Block::Boundary(id, _) => {
                if line == "end" {
                    block = Block::None;
                } else {
                    let id = id.clone();
                    parse_member(line, &id, &mut diagram).map_err(error)?;
                }
                continue;
            }
            Block::None => {}
        }
        block = parse_statement(line, line_no, &mut diagram).map_err(error)?;
    }
    match block {
        Block::Boundary(id, line) => {
            return Err(ParseError::new(
                line,
                format!("System boundary `{id}` is missing its `end`"),
            ));
        }
        Block::Description(line) => {
            return Err(ParseError::new(line, "`accDescr` is missing its `}`"));
        }
        Block::None => {}
    }

    for (id, pos) in positions {
        if let Some(element) = diagram.element_mut(&id) {
            element.pos = Some(pos);
        }
    }
    Ok(diagram)
}

/// Multi-line construct being read, with the line it opened on.
enum Block {
    None,
    Boundary(String, usize),
    Description(usize),
}

fn parse_statement(
    line: &str,
    line_no: usize,
    diagram: &mut UseCaseDiagram,
) -> Result<Block, String> {
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
            return Ok(Block::None);
        }
        "systemBoundary" => {
            let id = parse_boundary(rest, diagram)?;
            return Ok(Block::Boundary(id, line_no));
        }
        "json" => return Err("JSON tables are not supported yet".into()),
        "actor" => {
            parse_declaration(rest, ElementKind::Actor, None, diagram)?;
            return Ok(Block::None);
        }
        _ if PASSTHROUGH_KEYWORDS.contains(&keyword) => {
            diagram.passthrough.push(line.to_string());
            return Ok(Block::None);
        }
        _ => {}
    }
    if line.starts_with("accDescr") {
        diagram.passthrough.push(line.to_string());
        let block = line.ends_with('{');
        return Ok(if block {
            Block::Description(line_no)
        } else {
            Block::None
        });
    }
    if let Some(relation) = parse_relation(line) {
        let relation = relation?;
        diagram.ensure_element(&relation.from);
        diagram.ensure_element(&relation.to);
        diagram.relations.push(relation);
        return Ok(Block::None);
    }
    // `id@{ … }` on its own sets metadata of an element, a boundary or an
    // edge, which may be declared later.
    if take_id(line).is_some_and(|(_, rest)| rest.starts_with("@{")) {
        if !line.ends_with('}') {
            return Err("Metadata spanning several lines is not supported yet".into());
        }
        diagram.passthrough.push(line.to_string());
        return Ok(Block::None);
    }
    parse_declaration(line, ElementKind::UseCase, None, diagram)?;
    Ok(Block::None)
}

/// A line inside a system boundary: an actor or use case declaration.
fn parse_member(line: &str, boundary: &str, diagram: &mut UseCaseDiagram) -> Result<(), String> {
    let (kind, rest) = match line.strip_prefix("actor ") {
        Some(rest) => (ElementKind::Actor, rest.trim()),
        None => (ElementKind::UseCase, line),
    };
    if kind == ElementKind::UseCase && parse_relation(line).is_some() {
        return Err("Relationships go outside system boundaries".into());
    }
    parse_declaration(rest, kind, Some(boundary), diagram)
}

/// Parses `systemBoundary <id><title><extra>` and returns the id.
fn parse_boundary(rest: &str, diagram: &mut UseCaseDiagram) -> Result<String, String> {
    let (id, title, extra) = if rest.starts_with('"') {
        let (label, after) = parse_quoted(rest)?;
        let id = id_from_text(&Label(label.clone()).display(""));
        (id, Label(label), after)
    } else {
        let (id, after) = take_id(rest).ok_or_else(|| unexpected(rest))?;
        let (title, after) = match after.chars().next() {
            Some(open @ ('[' | '(')) => {
                let (label, after) = parse_delimited(after, open)?;
                (Label(label), after)
            }
            _ => (Label::default(), after),
        };
        (id.to_string(), title, after)
    };
    check_extra(extra)?;
    diagram.boundaries.push(Boundary {
        id: id.clone(),
        title,
        extra: extra.trim_end().to_string(),
    });
    Ok(id)
}

/// Parses an actor (after `actor`) or use case declaration: an id or a
/// quoted label, then an optional label in `( )` or `[ ]` and extras.
fn parse_declaration(
    text: &str,
    kind: ElementKind,
    boundary: Option<&str>,
    diagram: &mut UseCaseDiagram,
) -> Result<(), String> {
    if text.starts_with('"') {
        let (label, after) = parse_quoted(text)?;
        check_extra(after)?;
        let label = Label(label);
        let element = diagram.ensure_element(&id_from_text(&label.display("")));
        element.label = label;
        element.kind = kind;
        element.extra = after.trim_end().to_string();
        element.boundary = boundary.map(String::from);
        return Ok(());
    }
    let (id, after) = take_id(text).ok_or_else(|| unexpected(text))?;
    let (shape, label, after) = match after.chars().next() {
        Some(open @ ('(' | '[')) => {
            let (label, after) = parse_delimited(after, open)?;
            let shape = match open {
                '[' => ElementKind::UseCaseRect,
                _ => ElementKind::UseCase,
            };
            (Some(shape), Some(Label(label)), after)
        }
        _ => (None, None, after),
    };
    check_extra(after)?;
    let element = diagram.ensure_element(id);
    element.kind = match kind {
        ElementKind::Actor => ElementKind::Actor,
        _ => shape.unwrap_or(element.kind),
    };
    // `Id[Id]` only spells out the shape.
    if let Some(label) = label {
        element.label = if label.0 == id {
            Label::default()
        } else {
            label
        };
    }
    element.extra = after.trim_end().to_string();
    if boundary.is_some() {
        element.boundary = boundary.map(String::from);
    }
    Ok(())
}

/// Metadata, stereotype and classes are kept as written; anything else
/// after a declaration is an error.
fn check_extra(extra: &str) -> Result<(), String> {
    let rest = extra.trim_start();
    if rest.is_empty()
        || rest.starts_with("@{")
        || rest.starts_with("<<")
        || rest.starts_with(":::")
    {
        if rest.contains("@{") && !rest.contains('}') {
            return Err("Metadata spanning several lines is not supported yet".into());
        }
        Ok(())
    } else {
        Err(unexpected(rest))
    }
}

fn unexpected(text: &str) -> String {
    let preview: String = text.chars().take(16).collect();
    format!("Unexpected `{preview}`")
}

/// Identifier made from a quoted declaration: every non-word character
/// becomes `_`.
fn id_from_text(text: &str) -> String {
    text.chars()
        .map(|c| if is_id_char(c) { c } else { '_' })
        .collect()
}

fn take_id(text: &str) -> Option<(&str, &str)> {
    let end = text.find(|c: char| !is_id_char(c)).unwrap_or(text.len());
    (end > 0).then(|| text.split_at(end))
}

/// `"text"` at the start of `text`, returned with its quotes, and the rest.
fn parse_quoted(text: &str) -> Result<(String, &str), String> {
    let close = text[1..].find('"').ok_or(
        "Text is missing its closing `\"` (labels spanning several lines are not supported yet)",
    )?;
    Ok((text[..close + 2].to_string(), &text[close + 2..]))
}

/// Label between `open` and its closing delimiter, as written.
fn parse_delimited(text: &str, open: char) -> Result<(String, &str), String> {
    let close = if open == '[' { ']' } else { ')' };
    let inner = &text[1..];
    let trimmed = inner.trim_start();
    let (label, after) = if trimmed.starts_with('"') {
        let (label, after) = parse_quoted(trimmed)?;
        (label, after.trim_start())
    } else {
        let end = inner
            .find(close)
            .ok_or_else(|| format!("Label is missing its closing `{close}`"))?;
        (inner[..end].trim().to_string(), &inner[end..])
    };
    let after = after
        .strip_prefix(close)
        .ok_or_else(|| format!("Label is missing its closing `{close}`"))?;
    Ok((label, after))
}

/// Parses `from [edge@]operator to`. Returns `None` when the line is not a
/// relationship, and an error when it is one but malformed.
fn parse_relation(line: &str) -> Option<Result<Relation, String>> {
    let (from, rest) = take_id(line)?;
    let ops = rest.trim_start();
    if ops.len() == rest.len() || ops.starts_with("<<") {
        return None;
    }
    let (edge_id, ops) = match take_id(ops) {
        Some((id, after)) if after.starts_with('@') && !after.starts_with("@{") => {
            (Some(id.to_string()), &after[1..])
        }
        _ => (None, ops),
    };
    let (kind, label, extra_dashes, rest) = match parse_operator(ops)? {
        Ok(parsed) => parsed,
        Err(err) => return Some(Err(err)),
    };
    let rest = rest.trim_start();
    let Some((to, after)) = take_id(rest) else {
        return Some(Err(format!(
            "Expected a target after the relationship, found `{rest}`"
        )));
    };
    if !after.trim().is_empty() {
        return Some(Err(unexpected(after.trim())));
    }
    Some(Ok(Relation {
        from: from.to_string(),
        to: to.to_string(),
        kind,
        label,
        extra_dashes,
        edge_id,
    }))
}

type Operator<'a> = (RelationKind, Label, usize, &'a str);

/// Reads a relationship operator. `None` when `text` does not start with one.
fn parse_operator(text: &str) -> Option<Result<Operator<'_>, String>> {
    let fixed = [
        ("--|>", RelationKind::Generalization),
        ("o--", RelationKind::CircleFrom),
        ("x--", RelationKind::CrossFrom),
    ];
    for (token, kind) in fixed {
        if let Some(rest) = text.strip_prefix(token)
            && rest.starts_with(char::is_whitespace)
        {
            return Some(Ok((kind, Label::default(), 0, rest)));
        }
    }
    if let Some(rest) = text.strip_prefix("..>") {
        let rest = rest.trim_start();
        let Some(rest) = rest.strip_prefix(':') else {
            return Some(Err("Expected `: include` or `: extend` after `..>`".into()));
        };
        let rest = rest.trim_start();
        let kind = match take_id(rest) {
            Some(("include", _)) => RelationKind::Include,
            Some(("extend", _)) => RelationKind::Extend,
            _ => return Some(Err("Expected `include` or `extend` after `..> :`".into())),
        };
        let rest = &rest[if kind == RelationKind::Include { 7 } else { 6 }..];
        return Some(Ok((kind, Label::default(), 0, rest)));
    }
    if let Some(rest) = text.strip_prefix('<') {
        let dashes = rest.chars().take_while(|&c| c == '-').count();
        if dashes < 2 {
            return None;
        }
        return Some(Ok((
            RelationKind::ArrowBack,
            Label::default(),
            dashes - 2,
            &rest[dashes..],
        )));
    }
    let dashes = text.chars().take_while(|&c| c == '-').count();
    if dashes < 2 {
        return None;
    }
    let rest = &text[dashes..];
    let extra = dashes - 2;
    if let Some(after) = rest.strip_prefix('>') {
        return Some(Ok((RelationKind::Arrow, Label::default(), extra, after)));
    }
    for (head, kind) in [('o', RelationKind::CircleTo), ('x', RelationKind::CrossTo)] {
        if let Some(after) = rest.strip_prefix(head)
            && after.starts_with(char::is_whitespace)
            && dashes == 2
        {
            return Some(Ok((kind, Label::default(), 0, after)));
        }
    }
    let after = rest.trim_start();
    // `A -- B` is a plain line; anything else after `--` is a label followed
    // by the closing operator.
    let plain = take_id(after).is_some_and(|(_, tail)| tail.trim().is_empty());
    if plain || dashes > 2 {
        return Some(Ok((RelationKind::Line, Label::default(), extra, rest)));
    }
    Some(parse_labelled(after))
}

/// Parses `label -->` or `label --` after the opening `--`.
fn parse_labelled(text: &str) -> Result<Operator<'_>, String> {
    let (label, rest) = if text.starts_with('"') {
        let (label, rest) = parse_quoted(text)?;
        (label, rest.trim_start())
    } else {
        let end = text
            .find("--")
            .ok_or("Expected `-->` or `--` after the relationship label")?;
        (text[..end].trim().to_string(), &text[end..])
    };
    let dashes = rest.chars().take_while(|&c| c == '-').count();
    if dashes < 2 {
        return Err("Expected `-->` or `--` after the relationship label".into());
    }
    let after = &rest[dashes..];
    let (kind, after) = match after.strip_prefix('>') {
        Some(after) => (RelationKind::Arrow, after),
        None => (RelationKind::Line, after),
    };
    Ok((kind, Label(label), dashes - 2, after))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::Point;

    fn kinds(d: &UseCaseDiagram) -> Vec<RelationKind> {
        d.relations.iter().map(|r| r.kind).collect()
    }

    #[test]
    fn parses_actors_use_cases_and_inferred_endpoints() {
        let d = parse(
            "usecase-beta
            Customer --> Login
            actor Customer
            actor Admin(\"Main administrator\")
            Login(\"Sign in\")
            Report[Generate report]
            \"Reset password\"
            Admin --> Report",
        )
        .unwrap();
        let c = d.element("Customer").unwrap();
        assert_eq!(c.kind, ElementKind::Actor);
        assert_eq!(d.element("Admin").unwrap().name(), "Main administrator");
        let login = d.element("Login").unwrap();
        assert_eq!(
            (login.kind, login.name()),
            (ElementKind::UseCase, "Sign in".into())
        );
        let report = d.element("Report").unwrap();
        assert_eq!(report.kind, ElementKind::UseCaseRect);
        assert_eq!(report.label.0, "Generate report");
        assert_eq!(
            d.element("Reset_password").unwrap().name(),
            "Reset password"
        );
    }

    #[test]
    fn parses_every_operator() {
        let d = parse(
            "usecase-beta
            User --> Start
            Start <-- Support
            Start -- Finish
            User --o Start
            Start o-- Support
            User --x Finish
            Finish x-- Support
            Admin --|> Person
            Checkout ..> : include Payment
            ApplyCoupon ..> : extend Checkout
            A ----> B
            C <--- D",
        )
        .unwrap();
        use RelationKind::*;
        assert_eq!(
            kinds(&d),
            [
                Arrow,
                ArrowBack,
                Line,
                CircleTo,
                CircleFrom,
                CrossTo,
                CrossFrom,
                Generalization,
                Include,
                Extend,
                Arrow,
                ArrowBack
            ]
        );
        assert_eq!(d.relations[10].extra_dashes, 2);
        assert_eq!(d.relations[11].extra_dashes, 1);
        assert_eq!(
            (d.relations[8].from.as_str(), d.relations[8].to.as_str()),
            ("Checkout", "Payment")
        );
    }

    #[test]
    fn parses_labels_and_edge_ids() {
        let d = parse(
            "usecase-beta
            User -- \"include account details\" --> Login
            Analyst -- files R&D report --> Literal
            Customer opens@-- \"starts checkout\" ---> Checkout
            Checkout payment@..> : include Payment
            A -- note -- B",
        )
        .unwrap();
        let r = &d.relations;
        assert_eq!(r[0].label.display(""), "include account details");
        assert_eq!(r[0].kind, RelationKind::Arrow);
        assert_eq!(r[1].label.0, "files R&D report");
        assert_eq!(r[2].edge_id.as_deref(), Some("opens"));
        assert_eq!(r[2].extra_dashes, 1);
        assert_eq!(r[3].edge_id.as_deref(), Some("payment"));
        assert_eq!(r[3].kind, RelationKind::Include);
        assert_eq!(
            (r[4].kind, r[4].label.0.as_str()),
            (RelationKind::Line, "note")
        );
    }

    #[test]
    fn parses_boundaries_extras_and_passthrough() {
        let d = parse(
            "usecase-beta
            direction LR
            systemBoundary sb1[\"Payment service\"]@{ type: package }:::system
              actor Clerk(\"Payment clerk\")
              %% inside
              Authorize(\"Authorize payment\") <<Core>>:::critical
            end
            systemBoundary \"Order system\"
              Checkout
            end
            Order_system@{ type: package }
            actor SalesAgent(\"Sales agent\")@{ business: true } <<Employee>>
            Clerk --> Authorize
            opens@{ animation: fast }
            note for Clerk \"Starts\"
            classDef system stroke:#4b4b7a
            accDescr {
              Long text
            }
            %% @merman pos Clerk 10 20",
        )
        .unwrap();
        assert_eq!(d.direction, Some(Direction::LeftRight));
        let sb = d.boundary("sb1").unwrap();
        assert_eq!(sb.name(), "Payment service");
        assert_eq!(sb.extra, "@{ type: package }:::system");
        assert_eq!(d.boundary("Order_system").unwrap().name(), "Order system");
        let clerk = d.element("Clerk").unwrap();
        assert_eq!(clerk.boundary.as_deref(), Some("sb1"));
        assert_eq!(clerk.pos, Some(Point::new(10.0, 20.0)));
        let authorize = d.element("Authorize").unwrap();
        assert_eq!(authorize.extra, " <<Core>>:::critical");
        assert_eq!(authorize.stereotype(), Some("Core"));
        assert_eq!(
            d.element("Checkout").unwrap().boundary.as_deref(),
            Some("Order_system")
        );
        assert_eq!(
            d.element("SalesAgent").unwrap().extra,
            "@{ business: true } <<Employee>>"
        );
        assert_eq!(
            d.passthrough,
            [
                "%% inside",
                "Order_system@{ type: package }",
                "opens@{ animation: fast }",
                "note for Clerk \"Starts\"",
                "classDef system stroke:#4b4b7a",
                "accDescr {",
                "Long text",
                "}",
            ]
        );
    }

    #[test]
    fn reports_errors() {
        assert!(parse("usecase-beta\n systemBoundary S\n A").is_err());
        assert!(parse("usecase-beta\n systemBoundary S\n A --> B\n end").is_err());
        assert!(parse("usecase-beta\n json P@{\n }").is_err());
        assert!(parse("usecase-beta\n A(\"open").is_err());
        assert!(parse("usecase-beta\n A --> B extra").is_err());
        assert!(parse("usecase-beta\n A ..> B").is_err());
        assert!(parse("usecase-beta\n A(x) junk").is_err());
        assert!(parse("usecase").is_err());
    }
}
