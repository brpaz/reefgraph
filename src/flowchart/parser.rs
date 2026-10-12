//! Parser for the subset of Mermaid flowchart syntax the editor models.

use super::model::{Direction, Edge, Flowchart, Head, NodeShape, Stroke};
use crate::ids::is_id_char;
pub use crate::syntax::ParseError;
use crate::syntax::{position_comment, split_source, split_statements};

/// Statements kept verbatim without being modelled.
const PASSTHROUGH_KEYWORDS: &[&str] = &["classDef", "class", "style", "linkStyle", "click"];
const UNSUPPORTED_KEYWORDS: &[&str] = &["subgraph", "end", "direction"];

/// Node shapes ordered so that longer openers are tried first. Shapes sharing an
/// opener are adjacent and disambiguated by their closer.
const SHAPES: &[(&str, &str, NodeShape)] = &[
    ("(((", ")))", NodeShape::DoubleCircle),
    ("((", "))", NodeShape::Circle),
    ("([", "])", NodeShape::Stadium),
    ("[[", "]]", NodeShape::Subroutine),
    ("[(", ")]", NodeShape::Cylinder),
    ("{{", "}}", NodeShape::Hexagon),
    ("[/", "/]", NodeShape::Parallelogram),
    ("[/", "\\]", NodeShape::Trapezoid),
    ("[\\", "\\]", NodeShape::ParallelogramAlt),
    ("[\\", "/]", NodeShape::TrapezoidAlt),
    ("(", ")", NodeShape::Rounded),
    ("[", "]", NodeShape::Rectangle),
    ("{", "}", NodeShape::Rhombus),
    (">", "]", NodeShape::Asymmetric),
];

pub fn parse(src: &str) -> Result<Flowchart, ParseError> {
    let source = split_source(src)?;
    let mut chart = Flowchart {
        preamble: source.preamble,
        ..Flowchart::default()
    };
    let (header_line, header) = source.header;
    let mut header_statements = split_statements(header).into_iter();
    chart.direction = parse_header(header_statements.next().unwrap_or_default())
        .map_err(|m| ParseError::new(header_line, m))?;
    for statement in header_statements {
        parse_statement(statement, &mut chart).map_err(|m| ParseError::new(header_line, m))?;
    }

    let mut positions = Vec::new();
    for (line_no, line) in source.body {
        if let Some(comment) = line.strip_prefix("%%") {
            match position_comment(comment) {
                Some(position) => positions.push(position),
                None => chart.passthrough.push(line.to_string()),
            }
            continue;
        }
        for statement in split_statements(line) {
            parse_statement(statement, &mut chart).map_err(|m| ParseError::new(line_no, m))?;
        }
    }

    for (id, pos) in positions {
        if let Some(node) = chart.node_mut(&id) {
            node.pos = Some(pos);
        }
    }
    Ok(chart)
}

fn parse_header(header: &str) -> Result<Direction, String> {
    let mut words = header.split_whitespace();
    match words.next() {
        Some("flowchart" | "graph") => {}
        Some(other) => {
            return Err(format!(
                "Only flowcharts are supported, expected `flowchart` or `graph` but found `{other}`"
            ));
        }
        None => return Err("Expected a `flowchart` or `graph` declaration".into()),
    }
    let direction = match words.next() {
        Some(word) => {
            Direction::from_keyword(word).ok_or_else(|| format!("Unknown direction `{word}`"))?
        }
        None => Direction::default(),
    };
    if let Some(extra) = words.next() {
        return Err(format!(
            "Unexpected `{extra}` after the diagram declaration"
        ));
    }
    Ok(direction)
}

fn parse_statement(statement: &str, chart: &mut Flowchart) -> Result<(), String> {
    if let Some(keyword) = statement.split_whitespace().next() {
        if PASSTHROUGH_KEYWORDS.contains(&keyword) {
            chart.passthrough.push(statement.to_string());
            return Ok(());
        }
        if UNSUPPORTED_KEYWORDS.contains(&keyword) {
            return Err(format!("`{keyword}` is not supported yet"));
        }
    }

    let mut cursor = Cursor::new(statement);
    let mut previous = parse_node_group(&mut cursor, chart)?;
    loop {
        cursor.skip_ws();
        if cursor.is_eof() {
            return Ok(());
        }
        let link = parse_link(&mut cursor)?;
        let next = parse_node_group(&mut cursor, chart)?;
        for from in &previous {
            for to in &next {
                chart.edges.push(Edge {
                    from: from.clone(),
                    to: to.clone(),
                    label: link.label.clone(),
                    stroke: link.stroke,
                    start: link.start,
                    end: link.end,
                });
            }
        }
        previous = next;
    }
}

/// Parses `A & B & C`.
fn parse_node_group(cursor: &mut Cursor, chart: &mut Flowchart) -> Result<Vec<String>, String> {
    let mut ids = Vec::new();
    loop {
        cursor.skip_ws();
        ids.push(parse_node(cursor, chart)?);
        cursor.skip_ws();
        if !cursor.eat("&") {
            return Ok(ids);
        }
    }
}

fn parse_node(cursor: &mut Cursor, chart: &mut Flowchart) -> Result<String, String> {
    let id = cursor.take_while(is_id_char).to_string();
    if id.is_empty() {
        return Err(if cursor.is_eof() {
            "Expected a node after the link".into()
        } else {
            format!("Expected a node id, found `{}`", cursor.preview())
        });
    }
    let shape = parse_shape(cursor)?;
    let class = cursor
        .eat(":::")
        .then(|| cursor.take_while(|c| is_id_char(c) || c == '-').to_string());

    let node = chart.ensure_node(&id);
    if let Some((shape, label)) = shape {
        node.shape = shape;
        node.label = label;
    }
    if class.is_some() {
        node.class = class;
    }
    Ok(id)
}

fn parse_shape(cursor: &mut Cursor) -> Result<Option<(NodeShape, String)>, String> {
    let rest = cursor.rest();
    let mut unclosed = None;
    let mut i = 0;
    while i < SHAPES.len() {
        let open = SHAPES[i].0;
        let group: Vec<_> = SHAPES[i..].iter().take_while(|s| s.0 == open).collect();
        i += group.len();
        let Some(body) = rest.strip_prefix(open) else {
            continue;
        };
        if let Some((label, consumed, shape)) = match_shape_body(body, &group) {
            cursor.pos += open.len() + consumed;
            return Ok(Some((shape, label)));
        }
        unclosed.get_or_insert(open);
    }
    match unclosed {
        Some(open) => Err(format!("Missing closing delimiter for `{open}`")),
        None => Ok(None),
    }
}

/// Matches the label and closing delimiter after a shape opener, returning the
/// label, the number of bytes consumed and the resolved shape.
fn match_shape_body(
    body: &str,
    group: &[&(&str, &str, NodeShape)],
) -> Option<(String, usize, NodeShape)> {
    if let Some(inner) = body.strip_prefix('"') {
        let end = inner.find('"')?;
        let after = &inner[end + 1..];
        let trimmed = after.trim_start();
        let ws = after.len() - trimmed.len();
        return group.iter().find_map(|(_, close, shape)| {
            trimmed.starts_with(close).then(|| {
                (
                    decode(&inner[..end]),
                    1 + end + 1 + ws + close.len(),
                    *shape,
                )
            })
        });
    }
    group
        .iter()
        .filter_map(|(_, close, shape)| body.find(close).map(|idx| (idx, close.len(), *shape)))
        .min_by_key(|(idx, _, _)| *idx)
        .map(|(idx, close_len, shape)| (decode(body[..idx].trim()), idx + close_len, shape))
}

fn decode(text: &str) -> String {
    text.replace("#quot;", "\"")
}

struct Link {
    stroke: Stroke,
    start: Head,
    end: Head,
    label: String,
}

fn parse_link(cursor: &mut Cursor) -> Result<Link, String> {
    let start_pos = cursor.pos;
    let start = match (cursor.peek(), cursor.peek_nth(1)) {
        (Some('<'), _) => Head::Arrow,
        (Some('o'), Some('-' | '=' | '.')) => Head::Circle,
        (Some('x'), Some('-' | '=' | '.')) => Head::Cross,
        _ => Head::None,
    };
    if start != Head::None {
        cursor.pos += 1;
    }

    let rest = cursor.rest();
    let body = if rest.starts_with("-.") {
        parse_dotted_link(cursor)
    } else if rest.starts_with("--") {
        parse_solid_link(cursor, '-', Stroke::Normal)
    } else if rest.starts_with("==") {
        parse_solid_link(cursor, '=', Stroke::Thick)
    } else {
        None
    };
    let Some((stroke, end, mut label)) = body else {
        cursor.pos = start_pos;
        return Err(format!(
            "Expected a link such as `-->`, found `{}`",
            cursor.preview()
        ));
    };

    cursor.skip_ws();
    if cursor.eat("|") {
        label = parse_pipe_label(cursor).ok_or("Missing closing `|` for the link label")?;
    }
    Ok(Link {
        stroke,
        start,
        end,
        label,
    })
}

fn head_from_char(c: char) -> Option<Head> {
    match c {
        '>' => Some(Head::Arrow),
        'o' => Some(Head::Circle),
        'x' => Some(Head::Cross),
        _ => None,
    }
}

/// Consumes an arrow head. `o` and `x` only count when not followed by an id
/// character, so `--x B` is a cross head while `-- xyz -->` is label text.
fn eat_end_head(cursor: &mut Cursor) -> Option<Head> {
    let c = cursor.peek()?;
    let head = head_from_char(c)?;
    if c != '>' && cursor.peek_nth(1).is_some_and(is_id_char) {
        return None;
    }
    cursor.pos += 1;
    Some(head)
}

/// Parses `---`, `-->`, `--o`, `===`, `==>`… and the labelled `-- text -->` form.
fn parse_solid_link(
    cursor: &mut Cursor,
    ch: char,
    stroke: Stroke,
) -> Option<(Stroke, Head, String)> {
    let run = cursor.count(ch);
    if let Some(head) = eat_end_head(cursor) {
        return Some((stroke, head, String::new()));
    }
    if run >= 3 {
        return Some((stroke, Head::None, String::new()));
    }
    let rest = cursor.rest();
    let (text_end, consumed, head) = find_solid_closing(rest, ch)?;
    cursor.pos += consumed;
    Some((stroke, head, decode(rest[..text_end].trim())))
}

/// Finds the closing half of a `-- text -->` link, returning where the text ends,
/// the bytes consumed including the closing link, and its head.
fn find_solid_closing(rest: &str, ch: char) -> Option<(usize, usize, Head)> {
    for (i, _) in rest.char_indices() {
        if i > 0 && rest[..i].ends_with(ch) {
            continue;
        }
        let tail = &rest[i..];
        let run = tail.chars().take_while(|&c| c == ch).count();
        if run < 2 {
            continue;
        }
        let mut after = tail[run..].chars();
        match after.next() {
            Some('>') => return Some((i, i + run + 1, Head::Arrow)),
            Some(c @ ('o' | 'x')) if !after.next().is_some_and(is_id_char) => {
                return Some((i, i + run + 1, head_from_char(c)?));
            }
            _ if run >= 3 => return Some((i, i + run, Head::None)),
            _ => {}
        }
    }
    None
}

/// Parses `-.-`, `-.->`, `-..->` and the labelled `-. text .->` form.
fn parse_dotted_link(cursor: &mut Cursor) -> Option<(Stroke, Head, String)> {
    cursor.eat("-");
    let dots = cursor.count('.');
    if cursor.eat("-") {
        let head = eat_end_head(cursor).unwrap_or_default();
        return Some((Stroke::Dotted, head, String::new()));
    }
    if dots != 1 || !cursor.peek().is_some_and(char::is_whitespace) {
        return None;
    }
    let rest = cursor.rest();
    let idx = rest.find(".-")?;
    let label = decode(rest[..idx].trim_end_matches('.').trim());
    cursor.pos += idx + 2;
    let head = eat_end_head(cursor).unwrap_or_default();
    Some((Stroke::Dotted, head, label))
}

fn parse_pipe_label(cursor: &mut Cursor) -> Option<String> {
    cursor.skip_ws();
    let rest = cursor.rest();
    if let Some(inner) = rest.strip_prefix('"') {
        let end = inner.find('"')?;
        let after = inner[end + 1..].trim_start();
        let close = after.strip_prefix('|')?;
        cursor.pos += rest.len() - close.len();
        return Some(decode(&inner[..end]));
    }
    let end = rest.find('|')?;
    cursor.pos += end + 1;
    Some(decode(rest[..end].trim()))
}

struct Cursor<'a> {
    src: &'a str,
    pos: usize,
}

impl<'a> Cursor<'a> {
    fn new(src: &'a str) -> Self {
        Self { src, pos: 0 }
    }

    fn rest(&self) -> &'a str {
        &self.src[self.pos..]
    }

    fn is_eof(&self) -> bool {
        self.pos >= self.src.len()
    }

    fn peek(&self) -> Option<char> {
        self.rest().chars().next()
    }

    fn peek_nth(&self, n: usize) -> Option<char> {
        self.rest().chars().nth(n)
    }

    fn eat(&mut self, s: &str) -> bool {
        let matched = self.rest().starts_with(s);
        if matched {
            self.pos += s.len();
        }
        matched
    }

    fn skip_ws(&mut self) {
        self.pos = self.src.len() - self.rest().trim_start().len();
    }

    fn take_while(&mut self, f: impl Fn(char) -> bool) -> &'a str {
        let rest = self.rest();
        let end = rest.find(|c| !f(c)).unwrap_or(rest.len());
        self.pos += end;
        &rest[..end]
    }

    fn count(&mut self, ch: char) -> usize {
        self.take_while(|c| c == ch).len()
    }

    fn preview(&self) -> String {
        self.rest().chars().take(16).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::Point;

    fn edge(chart: &Flowchart, i: usize) -> (&str, &str, &str, Stroke, Head, Head) {
        let e = &chart.edges[i];
        (&e.from, &e.to, &e.label, e.stroke, e.start, e.end)
    }

    #[test]
    fn parses_header_directions() {
        assert_eq!(parse("graph LR").unwrap().direction, Direction::LeftRight);
        assert_eq!(
            parse("flowchart TD").unwrap().direction,
            Direction::TopBottom
        );
        assert_eq!(parse("flowchart").unwrap().direction, Direction::TopBottom);
    }

    #[test]
    fn rejects_other_diagram_types() {
        let err = parse("sequenceDiagram\n").unwrap_err();
        assert_eq!(err.line, 1);
        assert!(err.message.contains("sequenceDiagram"));
    }

    #[test]
    fn rejects_empty_input() {
        assert!(parse("  \n").is_err());
    }

    #[test]
    fn parses_every_shape() {
        for shape in NodeShape::ALL {
            let (open, close) = shape.delimiters();
            let chart = parse(&format!("flowchart TD\n  A{open}Hello world{close}")).unwrap();
            assert_eq!(chart.nodes[0].shape, shape, "shape {shape:?}");
            assert_eq!(chart.nodes[0].label, "Hello world", "shape {shape:?}");
        }
    }

    #[test]
    fn parses_quoted_labels() {
        let chart = parse("flowchart TD\n  A[\"Say #quot;hi#quot; (now) [ok]\"]").unwrap();
        assert_eq!(chart.nodes[0].label, "Say \"hi\" (now) [ok]");
    }

    #[test]
    fn reports_unclosed_shape() {
        let err = parse("flowchart TD\n\n  A[oops").unwrap_err();
        assert_eq!(err.line, 3);
    }

    #[test]
    fn parses_link_variants() {
        let src = "flowchart LR
            A --> B
            A --- B
            A -.-> B
            A -.- B
            A ==> B
            A === B
            A --o B
            A --x B
            A <--> B
            A o--o B
            A ----> B";
        let chart = parse(src).unwrap();
        use Head::*;
        use Stroke::*;
        let expected = [
            (Normal, None, Arrow),
            (Normal, None, None),
            (Dotted, None, Arrow),
            (Dotted, None, None),
            (Thick, None, Arrow),
            (Thick, None, None),
            (Normal, None, Circle),
            (Normal, None, Cross),
            (Normal, Arrow, Arrow),
            (Normal, Circle, Circle),
            (Normal, None, Arrow),
        ];
        assert_eq!(chart.nodes.len(), 2);
        for (i, (stroke, start, end)) in expected.into_iter().enumerate() {
            assert_eq!(
                edge(&chart, i),
                ("A", "B", "", stroke, start, end),
                "edge {i}"
            );
        }
    }

    #[test]
    fn parses_link_labels() {
        let src = "flowchart TD
            A -->|yes| B
            A -- no --> C
            A -. maybe .-> D
            A == sure ==> E
            A -->|\"a|b\"| F
            A -- open --- G";
        let chart = parse(src).unwrap();
        let labels: Vec<_> = chart.edges.iter().map(|e| e.label.as_str()).collect();
        assert_eq!(labels, ["yes", "no", "maybe", "sure", "a|b", "open"]);
        assert_eq!(chart.edges[5].end, Head::None);
        assert_eq!(chart.edges[2].stroke, Stroke::Dotted);
        assert_eq!(chart.edges[3].stroke, Stroke::Thick);
    }

    #[test]
    fn parses_chains_and_groups() {
        let chart = parse("flowchart TD\n  A --> B --> C\n  D & E --> F & G").unwrap();
        let pairs: Vec<_> = chart
            .edges
            .iter()
            .map(|e| format!("{}{}", e.from, e.to))
            .collect();
        assert_eq!(pairs, ["AB", "BC", "DF", "DG", "EF", "EG"]);
    }

    #[test]
    fn parses_inline_definitions_without_spaces() {
        let chart = parse("graph TD;A[Start]-->B{Ok?};B-->|Yes|C((Done))").unwrap();
        assert_eq!(chart.nodes.len(), 3);
        assert_eq!(chart.nodes[1].shape, NodeShape::Rhombus);
        assert_eq!(chart.nodes[2].shape, NodeShape::Circle);
        assert_eq!(chart.edges[1].label, "Yes");
    }

    #[test]
    fn later_definitions_override_shape() {
        let chart = parse("flowchart TD\n  A --> B\n  B(Rounded)").unwrap();
        assert_eq!(chart.nodes[1].shape, NodeShape::Rounded);
        assert_eq!(chart.nodes[1].label, "Rounded");
    }

    #[test]
    fn keeps_passthrough_statements_and_preamble() {
        let src = "---\ntitle: Demo\n---\n%%{init: {}}%%\nflowchart TD\n  A:::hot --> B\n  classDef hot fill:#f00\n  %% a comment\n  click A callback";
        let chart = parse(src).unwrap();
        assert_eq!(
            chart.preamble,
            ["---", "title: Demo", "---", "%%{init: {}}%%"]
        );
        assert_eq!(chart.nodes[0].class.as_deref(), Some("hot"));
        assert_eq!(
            chart.passthrough,
            ["classDef hot fill:#f00", "%% a comment", "click A callback"]
        );
    }

    #[test]
    fn reads_positions_from_comments() {
        let chart =
            parse("flowchart TD\n  A --> B\n%% @reefgraph pos A 10 20.5\n%% @reefgraph pos Z 1 1")
                .unwrap();
        assert_eq!(chart.nodes[0].pos, Some(Point::new(10.0, 20.5)));
        assert_eq!(chart.nodes[1].pos, None);
        assert!(chart.passthrough.is_empty());
    }

    #[test]
    fn keeps_unreadable_position_comments_as_comments() {
        let chart =
            parse("flowchart TD\n  A\n%% @reefgraph pos A x y\n%% @reefgraph possible").unwrap();
        assert_eq!(chart.nodes[0].pos, None);
        assert_eq!(
            chart.passthrough,
            ["%% @reefgraph pos A x y", "%% @reefgraph possible"]
        );
    }

    #[test]
    fn rejects_subgraphs() {
        let err = parse("flowchart TD\n  subgraph one\n  A\n  end").unwrap_err();
        assert_eq!(err.line, 2);
    }

    #[test]
    fn reports_bad_links() {
        let err = parse("flowchart TD\n  A -> B").unwrap_err();
        assert!(err.message.contains("link"), "{}", err.message);
    }
}
