//! Element box geometry, boundary frames and auto layout for C4 diagrams.

use std::collections::HashMap;

use super::model::{Boundary, BoundaryKind, C4Diagram, Element, ElementKind, ElementShape};
use crate::shared::geometry::{Point, Rect};
use crate::shared::layered::{self, Direction, Spacing};

/// Relations carry two-line labels, so ranks sit further apart.
const SPACING: Spacing = Spacing {
    rank: 110.0,
    node: 60.0,
};

const PADDING: f64 = 12.0;
const LINE_GAP: f64 = 4.0;
const MIN_WIDTH: f64 = 160.0;
pub const HEAD_RADIUS: f64 = 14.0;
/// Room above a person's box for the head, which slightly overlaps the box top.
pub const HEAD_ROOM: f64 = 2.0 * HEAD_RADIUS - 4.0;
/// Extra room under a database's top cap.
const DATABASE_CAP_ROOM: f64 = 10.0;
/// Space between a frame and what it contains.
const FRAME_PADDING: f64 = 20.0;

/// A line of text in an element box.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Line {
    Stereotype,
    Label,
    Technology,
    Description,
}

/// Text lines of an element, top to bottom; empty ones are left out.
pub fn element_lines(e: &Element) -> Vec<(Line, String)> {
    let mut lines = vec![
        (Line::Stereotype, e.stereotype()),
        (Line::Label, e.label.clone()),
    ];
    if e.kind.has_technology() && !e.technology.is_empty() {
        lines.push((Line::Technology, format!("[{}]", e.technology)));
    }
    if !e.description.is_empty() {
        lines.push((Line::Description, e.description.clone()));
    }
    lines
}

/// Layout of an element: its size and where each text line is centered,
/// as offsets from the top of the box.
#[derive(Debug, Clone, PartialEq)]
pub struct ElementBox {
    pub size: (f64, f64),
    /// Top of the drawn box below the head room, for people.
    pub body_top: f64,
    pub lines: Vec<(Line, String, f64)>,
    /// Bottom of the label line, where technology and description start.
    pub label_bottom: f64,
    /// Top and bottom of the technology line, if shown.
    pub technology_band: Option<(f64, f64)>,
}

impl ElementBox {
    pub fn rect(&self, center: Point) -> Rect {
        Rect::centered(center, self.size)
    }

    pub fn line_center(&self, center: Point, line: Line) -> Option<Point> {
        let rect = self.rect(center);
        self.lines
            .iter()
            .find(|l| l.0 == line)
            .map(|l| Point::new(rect.center().x, rect.y + l.2))
    }

    /// Row under `p`: 0 for the technology line, 1 for anything below the
    /// label. The stereotype and label are no row.
    pub fn row_at(&self, center: Point, p: Point) -> Option<usize> {
        let rect = self.rect(center);
        if !rect.contains(p) {
            return None;
        }
        let y = p.y - rect.y;
        match self.technology_band {
            Some((top, bottom)) if y >= top && y <= bottom => Some(0),
            _ if y > self.label_bottom => Some(1),
            _ => None,
        }
    }

    /// Where the in-place editor for a row points.
    pub fn row_anchor(&self, center: Point, row: usize) -> Point {
        let line = if row == 0 {
            Line::Technology
        } else {
            Line::Description
        };
        self.line_center(center, line).unwrap_or_else(|| {
            let rect = self.rect(center);
            Point::new(rect.center().x, rect.y + self.label_bottom + PADDING)
        })
    }
}

pub fn element_box(e: &Element, text_size: impl Fn(&str) -> (f64, f64)) -> ElementBox {
    let body_top = if e.kind == ElementKind::Person {
        HEAD_ROOM
    } else {
        0.0
    };
    let cap = if e.kind.has_shapes() && e.shape == ElementShape::Database {
        DATABASE_CAP_ROOM
    } else {
        0.0
    };
    let mut y = body_top + PADDING + cap;
    let mut width: f64 = 0.0;
    let mut lines = Vec::new();
    let mut label_bottom = y;
    let mut technology_band = None;
    for (line, text) in element_lines(e) {
        let (w, h) = text_size(&text);
        width = width.max(w);
        lines.push((line, text, y + h / 2.0));
        match line {
            Line::Label => label_bottom = y + h,
            Line::Technology => technology_band = Some((y, y + h)),
            _ => {}
        }
        y += h + LINE_GAP;
    }
    ElementBox {
        size: (
            (width + 2.0 * PADDING).max(MIN_WIDTH),
            y - LINE_GAP + PADDING,
        ),
        body_top,
        lines,
        label_bottom,
        technology_band,
    }
}

/// Text shown under a boundary's label.
pub fn boundary_type(b: &Boundary) -> String {
    let kind = match b.kind {
        BoundaryKind::Generic => b
            .extra
            .first()
            .filter(|a| !a.starts_with('$'))
            .map(|a| a.trim_matches('"').to_string())
            .unwrap_or_default(),
        BoundaryKind::Enterprise => "Enterprise".to_string(),
        BoundaryKind::System => "System".to_string(),
        BoundaryKind::Container => "Container".to_string(),
    };
    if kind.is_empty() {
        kind
    } else {
        format!("[{kind}]")
    }
}

/// A boundary drawn around its contents.
#[derive(Debug, Clone, PartialEq)]
pub struct Frame {
    pub id: String,
    pub rect: Rect,
    /// Text lines (label, type) and the left edge and center y of each.
    pub lines: Vec<(String, Point)>,
}

/// Frames of every boundary that contains something, outermost first.
pub fn frames(
    diagram: &C4Diagram,
    element_rect: impl Fn(&Element) -> Option<Rect>,
    text_size: impl Fn(&str) -> (f64, f64),
) -> Vec<Frame> {
    let mut done: HashMap<String, Option<Frame>> = HashMap::new();
    for b in &diagram.boundaries {
        frame_of(diagram, b, &element_rect, &text_size, &mut done, 0);
    }
    let mut frames: Vec<Frame> = diagram
        .boundaries
        .iter()
        .filter_map(|b| done.get(&b.id).cloned().flatten())
        .collect();
    frames.sort_by_key(|f| diagram.boundary_depth(&f.id));
    frames
}

fn frame_of(
    diagram: &C4Diagram,
    boundary: &Boundary,
    element_rect: &impl Fn(&Element) -> Option<Rect>,
    text_size: &impl Fn(&str) -> (f64, f64),
    done: &mut HashMap<String, Option<Frame>>,
    depth: usize,
) -> Option<Rect> {
    if let Some(frame) = done.get(&boundary.id) {
        return frame.as_ref().map(|f| f.rect);
    }
    // A boundary nested in itself through a broken parent chain draws nothing.
    if depth > diagram.boundaries.len() {
        return None;
    }
    let mut contents: Vec<Rect> = diagram
        .child_elements(Some(&boundary.id))
        .filter_map(element_rect)
        .collect();
    for child in diagram.child_boundaries(Some(&boundary.id)) {
        contents.extend(frame_of(
            diagram,
            child,
            element_rect,
            text_size,
            done,
            depth + 1,
        ));
    }
    let frame = contents
        .into_iter()
        .reduce(|a, b| a.union(&b))
        .map(|inner| {
            let texts: Vec<String> = [boundary.label.clone(), boundary_type(boundary)]
                .into_iter()
                .filter(|t| !t.is_empty())
                .collect();
            let heights: Vec<f64> = texts.iter().map(|t| text_size(t).1).collect();
            let header: f64 = heights.iter().map(|h| h + LINE_GAP).sum();
            // The label sits at the bottom left, as in C4-PlantUML, away from
            // relations arriving from above.
            let mut rect = inner.inflate(FRAME_PADDING);
            rect.h += header;
            let widest = texts.iter().map(|t| text_size(t).0).fold(0.0, f64::max);
            rect.w = rect.w.max(widest + 2.0 * PADDING);
            let mut y = rect.y + rect.h - header - PADDING / 2.0;
            let lines = texts
                .into_iter()
                .zip(heights)
                .map(|(text, h)| {
                    let line = (text, Point::new(rect.x + PADDING, y + h / 2.0));
                    y += h + LINE_GAP;
                    line
                })
                .collect();
            Frame {
                id: boundary.id.clone(),
                rect,
                lines,
            }
        });
    let rect = frame.as_ref().map(|f| f.rect);
    done.insert(boundary.id.clone(), frame);
    rect
}

/// Room reserved for a boundary's label lines while laying out; the frame
/// drawn later measures the actual text.
const FRAME_HEADER_ESTIMATE: f64 = 44.0;

/// Assigns a position to every element without one. Boundaries are laid out
/// as single nodes of their parent level, so members stay together and other
/// elements stay out of their frames. When some elements already have
/// positions, the new ones are offset to stay consistent with them.
pub fn layout_missing(diagram: &mut C4Diagram, size_of: impl Fn(&Element) -> (f64, f64)) {
    if diagram.elements.iter().all(|e| e.pos.is_some()) {
        return;
    }
    let computed: HashMap<String, Point> = place_group(diagram, None, &size_of, 0)
        .0
        .into_iter()
        .collect();
    let placed: Vec<(f64, f64)> = diagram
        .elements
        .iter()
        .filter_map(|e| {
            let (pos, auto) = (e.pos?, computed.get(&e.id)?);
            Some((pos.x - auto.x, pos.y - auto.y))
        })
        .collect();
    let offset = if placed.is_empty() {
        (0.0, 0.0)
    } else {
        let n = placed.len() as f64;
        (
            placed.iter().map(|p| p.0).sum::<f64>() / n,
            placed.iter().map(|p| p.1).sum::<f64>() / n,
        )
    };
    for element in &mut diagram.elements {
        if element.pos.is_none()
            && let Some(auto) = computed.get(&element.id)
        {
            element.pos = Some(Point::new(auto.x + offset.0, auto.y + offset.1));
        }
    }
}

/// The node of `group`'s level that an element belongs to: itself when it
/// sits directly in the group, or the child boundary it is nested in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum Node<'a> {
    Element(&'a str),
    Boundary(&'a str),
}

fn node_in<'a>(diagram: &'a C4Diagram, e: &'a Element, group: Option<&str>) -> Option<Node<'a>> {
    if e.boundary.as_deref() == group {
        return Some(Node::Element(&e.id));
    }
    let mut current = e.boundary.as_deref()?;
    for _ in 0..=diagram.boundaries.len() {
        let parent = diagram.boundary(current)?.parent.as_deref();
        if parent == group {
            return Some(Node::Boundary(current));
        }
        current = parent?;
    }
    None
}

/// Lays out the contents of `group` (the top level for `None`), returning
/// element centers and the rectangle covering them.
fn place_group(
    diagram: &C4Diagram,
    group: Option<&str>,
    size_of: &impl Fn(&Element) -> (f64, f64),
    depth: usize,
) -> (Vec<(String, Point)>, Option<Rect>) {
    if depth > diagram.boundaries.len() {
        return (Vec::new(), None);
    }
    let mut nodes: Vec<Node> = Vec::new();
    let mut sizes = Vec::new();
    // Child boundary contents, relative to the content center.
    let mut inner: HashMap<&str, Vec<(String, Point)>> = HashMap::new();
    for e in diagram.child_elements(group) {
        nodes.push(Node::Element(&e.id));
        sizes.push(size_of(e));
    }
    for b in diagram.child_boundaries(group) {
        let (positions, Some(bounds)) = place_group(diagram, Some(&b.id), size_of, depth + 1)
        else {
            continue;
        };
        let c = bounds.center();
        let relative = positions
            .into_iter()
            .map(|(id, p)| (id, Point::new(p.x - c.x, p.y - c.y)))
            .collect();
        inner.insert(&b.id, relative);
        nodes.push(Node::Boundary(&b.id));
        sizes.push((
            bounds.w + 2.0 * FRAME_PADDING,
            bounds.h + 2.0 * FRAME_PADDING + FRAME_HEADER_ESTIMATE,
        ));
    }

    let index: HashMap<Node, usize> = nodes.iter().enumerate().map(|(i, n)| (*n, i)).collect();
    let edges: Vec<(usize, usize)> = diagram
        .relations
        .iter()
        .filter_map(|r| {
            let at = |id: &str| {
                let node = node_in(diagram, diagram.element(id)?, group)?;
                index.get(&node).copied()
            };
            Some((at(&r.from)?, at(&r.to)?))
        })
        .collect();
    let centers = layered::layout(&sizes, &edges, Direction::TopBottom, SPACING);

    let mut positions = Vec::new();
    let mut bounds: Option<Rect> = None;
    for ((node, size), center) in nodes.iter().zip(&sizes).zip(centers) {
        let rect = Rect::centered(center, *size);
        bounds = Some(bounds.map_or(rect, |b| b.union(&rect)));
        match node {
            Node::Element(id) => positions.push((id.to_string(), center)),
            Node::Boundary(id) => {
                // The label sits below the contents, so they center higher up.
                let content = Point::new(center.x, center.y - FRAME_HEADER_ESTIMATE / 2.0);
                for (id, p) in inner.remove(id).unwrap_or_default() {
                    positions.push((id, Point::new(content.x + p.x, content.y + p.y)));
                }
            }
        }
    }
    (positions, bounds)
}

/// Discards every stored position and lays the whole diagram out again.
pub fn layout_all(diagram: &mut C4Diagram, size_of: impl Fn(&Element) -> (f64, f64)) {
    for element in &mut diagram.elements {
        element.pos = None;
    }
    layout_missing(diagram, size_of);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diagrams::c4::parser::parse;

    fn size(s: &str) -> (f64, f64) {
        (s.chars().count() as f64 * 8.0, 16.0)
    }

    #[test]
    fn finds_technology_and_description_rows() {
        let d = parse("C4Container\nContainer(w, \"Web\", \"React\", \"SPA\")").unwrap();
        let b = element_box(&d.elements[0], size);
        let c = Point::new(100.0, 100.0);
        let tech = b.line_center(c, Line::Technology).unwrap();
        let descr = b.line_center(c, Line::Description).unwrap();
        let label = b.line_center(c, Line::Label).unwrap();
        assert_eq!(b.row_at(c, tech), Some(0));
        assert_eq!(b.row_at(c, descr), Some(1));
        assert_eq!(b.row_at(c, label), None);
    }

    #[test]
    fn people_leave_room_for_the_head() {
        let d = parse("C4Context\nPerson(p, \"P\")\nSystem(s, \"S\")").unwrap();
        let person = element_box(&d.elements[0], size);
        let system = element_box(&d.elements[1], size);
        assert_eq!(person.size.1, system.size.1 + HEAD_ROOM);
    }

    #[test]
    fn keeps_outsiders_out_of_boundaries() {
        let mut d = parse(
            "C4Container
Person(p, \"P\")
System_Ext(mail, \"Mail\")
System_Boundary(b, \"B\") {
  Container(web, \"Web\")
  ContainerDb(db, \"Db\")
}
Rel(p, web, \"x\")
Rel(web, db, \"x\")
Rel(web, mail, \"x\")",
        )
        .unwrap();
        layout_missing(&mut d, |e| element_box(e, size).size);
        let rect = |e: &Element| Some(element_box(e, size).rect(e.pos?));
        let frame = frames(&d, rect, size)[0].rect;
        for id in ["p", "mail"] {
            let r = rect(d.element(id).unwrap()).unwrap();
            let overlaps = r.x < frame.x + frame.w
                && frame.x < r.x + r.w
                && r.y < frame.y + frame.h
                && frame.y < r.y + r.h;
            assert!(!overlaps, "{id} overlaps the frame");
        }
    }

    #[test]
    fn frames_enclose_nested_contents() {
        let mut d = parse(
            "C4Context
Enterprise_Boundary(e, \"Corp\") {
  System_Boundary(s, \"Sys\") {
    System(a, \"A\")
  }
  System(b, \"B\")
}
System_Boundary(empty, \"Empty\") {
}",
        )
        .unwrap();
        layout_missing(&mut d, |e| element_box(e, size).size);
        let rect = |e: &Element| Some(element_box(e, size).rect(e.pos?));
        let frames = frames(&d, rect, size);
        let ids: Vec<_> = frames.iter().map(|f| f.id.as_str()).collect();
        assert_eq!(ids, ["e", "s"]);
        let a = rect(d.element("a").unwrap()).unwrap();
        let (outer, inner) = (frames[0].rect, frames[1].rect);
        assert!(inner.contains(a.center()) && outer.contains(Point::new(inner.x, inner.y)));
        assert_eq!(frames[1].lines[1].0, "[System]");
    }
}
