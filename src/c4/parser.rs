//! Parser for Mermaid C4 context, container and component diagrams.

use super::model::{Boundary, BoundaryKind, C4Diagram, Element, Level, Relation, RelationKind};
use crate::syntax::{ParseError, position_comment, split_source};

/// Declarations of the C4 views this editor does not support yet.
pub const UNSUPPORTED_KEYWORDS: &[&str] = &["C4Dynamic", "C4Deployment"];

pub fn is_keyword(keyword: &str) -> bool {
    Level::from_keyword(keyword).is_some() || UNSUPPORTED_KEYWORDS.contains(&keyword)
}

pub fn parse(src: &str) -> Result<C4Diagram, ParseError> {
    let source = split_source(src)?;
    let (header_line, header) = source.header;
    let keyword = source.keyword();
    let Some(level) = Level::from_keyword(keyword) else {
        let message = if UNSUPPORTED_KEYWORDS.contains(&keyword) {
            format!("`{keyword}` diagrams are not supported yet")
        } else {
            format!("Expected `C4Context`, `C4Container` or `C4Component`, found `{header}`")
        };
        return Err(ParseError::new(header_line, message));
    };
    let mut diagram = C4Diagram {
        level,
        preamble: source.preamble,
        ..C4Diagram::default()
    };
    let mut positions = Vec::new();
    // Open boundaries, innermost last, with the line each opened on.
    let mut open: Vec<(String, usize)> = Vec::new();
    // Relations with the line they were declared on, checked once every
    // element is known.
    let mut relation_lines = Vec::new();

    for (line_no, line) in source.body {
        if let Some(comment) = line.strip_prefix("%%") {
            match position_comment(comment) {
                Some(position) => positions.push(position),
                None => diagram.passthrough.push(line.to_string()),
            }
            continue;
        }
        if line == "}" {
            if open.pop().is_none() {
                return Err(ParseError::new(line_no, "Unexpected `}`"));
            }
            continue;
        }
        if let Some(title) = line.strip_prefix("title")
            && (title.is_empty() || title.starts_with(char::is_whitespace))
        {
            diagram.title = Some(title.trim().to_string());
            continue;
        }
        let parent = open.last().map(|(id, _)| id.clone());
        let statement =
            parse_statement(line, parent, &mut diagram).map_err(|m| ParseError::new(line_no, m))?;
        match statement {
            Statement::OpenBoundary(id) => open.push((id, line_no)),
            Statement::Relation => relation_lines.push(line_no),
            Statement::Other => {}
        }
    }
    if let Some((id, line)) = open.pop() {
        return Err(ParseError::new(
            line,
            format!("Boundary `{id}` is missing its closing `}}`"),
        ));
    }
    for (relation, line) in diagram.relations.iter().zip(relation_lines) {
        for end in [&relation.from, &relation.to] {
            if diagram.element(end).is_none() {
                return Err(ParseError::new(line, format!("Unknown element `{end}`")));
            }
        }
    }

    for (id, pos) in positions {
        if let Some(element) = diagram.element_mut(&id) {
            element.pos = Some(pos);
        }
    }
    Ok(diagram)
}

enum Statement {
    OpenBoundary(String),
    Relation,
    Other,
}

/// Statement names kept verbatim: styling and layout configuration.
fn is_passthrough(name: &str) -> bool {
    name.starts_with("Update") || name.starts_with("Add") || name == "RoundedBoxShape"
}

fn parse_statement(
    line: &str,
    parent: Option<String>,
    diagram: &mut C4Diagram,
) -> Result<Statement, String> {
    let (name, args, rest) = split_call(line)?;
    if is_passthrough(name) {
        diagram.passthrough.push(line.to_string());
        return Ok(Statement::Other);
    }
    let opens_block = match rest {
        "" => false,
        "{" => true,
        "{}" | "{ }" => false,
        _ => return Err(format!("Unexpected `{rest}` after `{name}(…)`")),
    };

    if let Some(kind) = BoundaryKind::from_keyword(name) {
        let mut args = Args::new(&args);
        let id = args.alias()?;
        let label = args.next().unwrap_or_else(|| id.clone());
        diagram.boundaries.push(Boundary {
            id: id.clone(),
            label,
            kind,
            extra: args.rest(),
            parent,
        });
        return Ok(if opens_block {
            Statement::OpenBoundary(id)
        } else {
            Statement::Other
        });
    }
    if opens_block {
        return Err(format!("`{name}` cannot contain other elements"));
    }

    if let Some((kind, shape, external)) = Element::parse_keyword(name) {
        let mut args = Args::new(&args);
        let id = args.alias()?;
        if diagram.is_taken(&id) {
            return Err(format!("`{id}` is declared twice"));
        }
        let label = args.next().unwrap_or_else(|| id.clone());
        let technology = if kind.has_technology() {
            args.next().unwrap_or_default()
        } else {
            String::new()
        };
        let description = args.next().unwrap_or_default();
        diagram.elements.push(Element {
            id,
            label,
            kind,
            shape,
            external,
            technology,
            description,
            extra: args.rest(),
            boundary: parent,
            pos: None,
        });
        return Ok(Statement::Other);
    }

    let (kind, reversed) = match name {
        "Rel_Back" => (Some(RelationKind::Rel), true),
        _ => (RelationKind::from_keyword(name), false),
    };
    if let Some(kind) = kind {
        let mut args = Args::new(&args);
        let mut from = args.alias()?;
        let mut to = args.alias()?;
        if reversed {
            std::mem::swap(&mut from, &mut to);
        }
        diagram.relations.push(Relation {
            from,
            to,
            label: args.next().unwrap_or_default(),
            technology: args.next().unwrap_or_default(),
            kind,
            extra: args.rest(),
        });
        return Ok(Statement::Relation);
    }
    Err(format!("Unknown C4 statement `{name}`"))
}

/// Splits `Name(args) rest` into its name, raw arguments and trailing text.
fn split_call(line: &str) -> Result<(&str, Vec<&str>, &str), String> {
    let open = line
        .find('(')
        .ok_or_else(|| format!("Expected `Name(…)`, found `{line}`"))?;
    let name = line[..open].trim();
    let mut args = Vec::new();
    let mut in_quote = false;
    let mut start = open + 1;
    for (i, c) in line.char_indices().skip_while(|(i, _)| *i <= open) {
        match c {
            '"' => in_quote = !in_quote,
            ',' if !in_quote => {
                args.push(line[start..i].trim());
                start = i + 1;
            }
            ')' if !in_quote => {
                let last = line[start..i].trim();
                if !(args.is_empty() && last.is_empty()) {
                    args.push(last);
                }
                return Ok((name, args, line[i + 1..].trim()));
            }
            _ => {}
        }
    }
    Err(format!("`{name}(` is missing its closing `)`"))
}

/// Positional arguments of a statement, read in order.
struct Args<'a> {
    raw: &'a [&'a str],
    next: usize,
}

impl<'a> Args<'a> {
    fn new(raw: &'a [&'a str]) -> Self {
        Self { raw, next: 0 }
    }

    /// Next positional argument, unquoted. Named `$key=value` arguments end
    /// the positional ones.
    fn next(&mut self) -> Option<String> {
        let arg = self.raw.get(self.next)?;
        if arg.starts_with('$') {
            return None;
        }
        self.next += 1;
        Some(unquote(arg).to_string())
    }

    fn alias(&mut self) -> Result<String, String> {
        let id = self.next().ok_or("Expected an alias")?;
        if !C4Diagram::is_valid_id(&id) {
            return Err(format!(
                "`{id}` is not a valid alias; use letters, digits and `_`"
            ));
        }
        Ok(id)
    }

    /// Arguments not read yet, as written.
    fn rest(&self) -> Vec<String> {
        self.raw[self.next..]
            .iter()
            .map(|a| a.to_string())
            .collect()
    }
}

fn unquote(arg: &str) -> &str {
    arg.strip_prefix('"')
        .and_then(|a| a.strip_suffix('"'))
        .unwrap_or(arg)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::c4::model::{ElementKind, ElementShape};

    #[test]
    fn parses_elements_boundaries_and_relations() {
        let d = parse(
            "C4Container
title Banking
Person(customer, \"Customer\", \"A bank customer\")
System_Boundary(bank, \"Internet Banking\") {
    Container(web, \"Web App\", \"React\", \"Delivers the SPA\")
    ContainerDb_Ext(db, \"Database\", \"Postgres\", \"\", $tags=\"legacy\")
}
Rel(customer, web, \"Uses\", \"HTTPS\")
Rel_Back(db, web, \"Reads from\")
UpdateRelStyle(customer, web, $offsetY=\"-40\")
",
        )
        .unwrap();
        assert_eq!(d.level, Level::Container);
        assert_eq!(d.title.as_deref(), Some("Banking"));
        let web = d.element("web").unwrap();
        assert_eq!(web.kind, ElementKind::Container);
        assert_eq!(web.technology, "React");
        assert_eq!(web.boundary.as_deref(), Some("bank"));
        let db = d.element("db").unwrap();
        assert_eq!((db.shape, db.external), (ElementShape::Database, true));
        assert_eq!(db.extra, ["$tags=\"legacy\""]);
        assert_eq!(d.relations[0].technology, "HTTPS");
        assert_eq!(
            (d.relations[1].from.as_str(), d.relations[1].to.as_str()),
            ("web", "db")
        );
        assert_eq!(d.passthrough.len(), 1);
    }

    #[test]
    fn nests_boundaries() {
        let d = parse(
            "C4Context
Enterprise_Boundary(e, \"Corp\") {
  System_Boundary(s, \"Sys\") {
    System(a, \"A\")
  }
}",
        )
        .unwrap();
        assert_eq!(d.boundary("s").unwrap().parent.as_deref(), Some("e"));
        assert_eq!(d.element("a").unwrap().boundary.as_deref(), Some("s"));
    }

    #[test]
    fn reports_errors_with_line_numbers() {
        let err = parse("C4Context\nSystem(a, \"A\")\nRel(a, b, \"x\")").unwrap_err();
        assert_eq!((err.line, err.message.as_str()), (3, "Unknown element `b`"));
        let err = parse("C4Context\nSystem_Boundary(b, \"B\") {\n").unwrap_err();
        assert_eq!(err.line, 2);
        let err = parse("C4Deployment\n").unwrap_err();
        assert!(err.message.contains("not supported yet"));
        let err = parse("C4Context\nSystem(a b)").unwrap_err();
        assert!(err.message.contains("not a valid alias"));
    }
}
