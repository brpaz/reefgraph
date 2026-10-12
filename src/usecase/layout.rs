//! Element geometry, boundary frames and auto layout for use case diagrams.

use std::collections::HashMap;

use super::model::{Element, ElementKind, UseCaseDiagram};
use crate::geometry::{Point, Rect};
use crate::layered::{self, Spacing};

const SPACING: Spacing = Spacing {
    rank: 90.0,
    node: 40.0,
};
/// Stick figure size above an actor's name.
pub const FIGURE_SIZE: (f64, f64) = (28.0, 46.0);
const FIGURE_GAP: f64 = 6.0;
const ELLIPSE_MIN: (f64, f64) = (120.0, 50.0);
const RECT_MIN: (f64, f64) = (120.0, 44.0);
/// Space between a frame and its members, and around its title.
pub const FRAME_PADDING: f64 = 20.0;

/// Text lines of an element: its stereotype as `«name»`, then its name.
pub fn text_lines(e: &Element) -> Vec<String> {
    e.stereotype()
        .map(|s| format!("«{s}»"))
        .into_iter()
        .chain(std::iter::once(e.name()))
        .collect()
}

fn text_block(e: &Element, text_size: &impl Fn(&str) -> (f64, f64)) -> (f64, f64) {
    text_lines(e)
        .iter()
        .map(|t| text_size(t))
        .fold((0.0, 0.0), |(w, h), (tw, th)| (w.max(tw), h + th))
}

pub fn element_size(e: &Element, text_size: impl Fn(&str) -> (f64, f64)) -> (f64, f64) {
    let (w, h) = text_block(e, &text_size);
    match e.kind {
        ElementKind::Actor => (w.max(FIGURE_SIZE.0 + 12.0), FIGURE_SIZE.1 + FIGURE_GAP + h),
        // Text fits inside the ellipse's inscribed rectangle.
        ElementKind::UseCase => (
            (w * 1.3 + 24.0).max(ELLIPSE_MIN.0),
            (h * 1.4 + 16.0).max(ELLIPSE_MIN.1),
        ),
        ElementKind::UseCaseRect => ((w + 28.0).max(RECT_MIN.0), (h + 20.0).max(RECT_MIN.1)),
    }
}

/// Center of the element's text block, for a box at `rect`.
pub fn text_center(e: &Element, rect: &Rect, text_size: impl Fn(&str) -> (f64, f64)) -> Point {
    match e.kind {
        ElementKind::Actor => {
            let (_, h) = text_block(e, &text_size);
            Point::new(rect.center().x, rect.y + rect.h - h / 2.0)
        }
        _ => rect.center(),
    }
}

/// A boundary drawn around its members.
#[derive(Debug, Clone, PartialEq)]
pub struct Frame {
    pub id: String,
    pub rect: Rect,
    pub title: String,
    /// Center of the title text.
    pub title_at: Point,
}

/// Frames of every boundary with members.
pub fn frames(
    diagram: &UseCaseDiagram,
    element_rect: impl Fn(&Element) -> Option<Rect>,
    text_size: impl Fn(&str) -> (f64, f64),
) -> Vec<Frame> {
    diagram
        .boundaries
        .iter()
        .filter_map(|b| {
            let inner = diagram
                .members(Some(&b.id))
                .filter_map(&element_rect)
                .reduce(|a, b| a.union(&b))?;
            let title = b.name();
            let (tw, th) = text_size(&title);
            let mut rect = inner.inflate(FRAME_PADDING);
            rect.y -= th;
            rect.h += th;
            rect.w = rect.w.max(tw + 2.0 * FRAME_PADDING);
            let title_at = Point::new(
                rect.x + FRAME_PADDING + tw / 2.0,
                rect.y + FRAME_PADDING / 2.0 + th / 2.0,
            );
            Some(Frame {
                id: b.id.clone(),
                rect,
                title,
                title_at,
            })
        })
        .collect()
}

/// Room reserved for a boundary's title while laying out.
const FRAME_TITLE_ESTIMATE: f64 = 20.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum Node<'a> {
    Element(&'a str),
    Boundary(&'a str),
}

/// Assigns a position to every element without one. Each boundary's
/// members are laid out first and the boundary then takes part in the top
/// level as one node, so members stay together and others stay out of the
/// frame. New positions are offset to stay consistent with existing ones.
pub fn layout_missing(diagram: &mut UseCaseDiagram, size_of: impl Fn(&Element) -> (f64, f64)) {
    if diagram.elements.iter().all(|e| e.pos.is_some()) {
        return;
    }
    let computed = compound_layout(diagram, &size_of);
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

/// Boundary an element is laid out in; one that does not exist is ignored.
fn group_of<'a>(diagram: &UseCaseDiagram, e: &'a Element) -> Option<&'a str> {
    e.boundary
        .as_deref()
        .filter(|b| diagram.boundary(b).is_some())
}

fn compound_layout(
    diagram: &UseCaseDiagram,
    size_of: &impl Fn(&Element) -> (f64, f64),
) -> HashMap<String, Point> {
    let direction = diagram.direction.unwrap_or_default();
    let node_of = |id: &str| {
        let e = diagram.element(id)?;
        Some(match group_of(diagram, e) {
            Some(b) => Node::Boundary(b),
            None => Node::Element(&e.id),
        })
    };

    let mut nodes = Vec::new();
    let mut sizes = Vec::new();
    // Member centers relative to their group's content center.
    let mut inner: HashMap<&str, Vec<(&str, Point)>> = HashMap::new();
    for e in diagram
        .elements
        .iter()
        .filter(|e| group_of(diagram, e).is_none())
    {
        nodes.push(Node::Element(&e.id));
        sizes.push(size_of(e));
    }
    for b in &diagram.boundaries {
        let members: Vec<&Element> = diagram.members(Some(&b.id)).collect();
        if members.is_empty() {
            continue;
        }
        let index: HashMap<&str, usize> = members
            .iter()
            .enumerate()
            .map(|(i, e)| (e.id.as_str(), i))
            .collect();
        let edges: Vec<(usize, usize)> = diagram
            .relations
            .iter()
            .filter_map(|r| Some((*index.get(r.from.as_str())?, *index.get(r.to.as_str())?)))
            .collect();
        let member_sizes: Vec<_> = members.iter().map(|e| size_of(e)).collect();
        let centers = layered::layout(&member_sizes, &edges, direction, SPACING);
        let bounds = centers
            .iter()
            .zip(&member_sizes)
            .map(|(c, s)| Rect::centered(*c, *s))
            .reduce(|a, b| a.union(&b))
            .unwrap_or_default();
        let c = bounds.center();
        inner.insert(
            &b.id,
            members
                .iter()
                .zip(centers)
                .map(|(e, p)| (e.id.as_str(), Point::new(p.x - c.x, p.y - c.y)))
                .collect(),
        );
        nodes.push(Node::Boundary(&b.id));
        sizes.push((
            bounds.w + 2.0 * FRAME_PADDING,
            bounds.h + 2.0 * FRAME_PADDING + FRAME_TITLE_ESTIMATE,
        ));
    }

    let index: HashMap<Node, usize> = nodes.iter().enumerate().map(|(i, n)| (*n, i)).collect();
    let edges: Vec<(usize, usize)> = diagram
        .relations
        .iter()
        .filter_map(|r| {
            let (from, to) = (node_of(&r.from)?, node_of(&r.to)?);
            if from == to {
                return None;
            }
            Some((*index.get(&from)?, *index.get(&to)?))
        })
        .collect();
    let centers = layered::layout(&sizes, &edges, direction, SPACING);

    let mut positions = HashMap::new();
    for (node, center) in nodes.iter().zip(centers) {
        match node {
            Node::Element(id) => {
                positions.insert(id.to_string(), center);
            }
            Node::Boundary(id) => {
                // The title sits above the members, so they center lower down.
                let content = Point::new(center.x, center.y + FRAME_TITLE_ESTIMATE / 2.0);
                for (id, p) in inner.remove(id).unwrap_or_default() {
                    positions.insert(id.to_string(), Point::new(content.x + p.x, content.y + p.y));
                }
            }
        }
    }
    positions
}

/// Discards every stored position and lays the whole diagram out again.
pub fn layout_all(diagram: &mut UseCaseDiagram, size_of: impl Fn(&Element) -> (f64, f64)) {
    for element in &mut diagram.elements {
        element.pos = None;
    }
    layout_missing(diagram, size_of);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::usecase::parser::parse;

    fn size(s: &str) -> (f64, f64) {
        (s.chars().count() as f64 * 8.0, 16.0)
    }

    #[test]
    fn actors_leave_room_for_the_figure() {
        let d = parse("usecase-beta\n actor A\n B(\"B\")").unwrap();
        let actor = element_size(&d.elements[0], size);
        assert_eq!(actor.1, FIGURE_SIZE.1 + FIGURE_GAP + 16.0);
        assert_eq!(element_size(&d.elements[1], size), ELLIPSE_MIN);
    }

    #[test]
    fn keeps_outsiders_out_of_boundaries() {
        let mut d = parse(
            "usecase-beta
            actor Customer
            actor Bank
            systemBoundary Shop
              Browse
              Checkout
            end
            Customer --> Browse
            Customer --> Checkout
            Checkout --> Bank",
        )
        .unwrap();
        layout_missing(&mut d, |e| element_size(e, size));
        let rect = |e: &Element| Some(Rect::centered(e.pos?, element_size(e, size)));
        let frame = frames(&d, rect, size)[0].rect;
        for id in ["Customer", "Bank"] {
            let r = rect(d.element(id).unwrap()).unwrap();
            let overlaps = r.x < frame.x + frame.w
                && frame.x < r.x + r.w
                && r.y < frame.y + frame.h
                && frame.y < r.y + r.h;
            assert!(!overlaps, "{id} overlaps the frame");
        }
        for id in ["Browse", "Checkout"] {
            assert!(frame.contains(d.element(id).unwrap().pos.unwrap()));
        }
    }
}
