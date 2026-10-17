//! Edge paths between node outlines: clipping at the outline, bending
//! parallel edges apart and detouring around nodes in the way.

use std::collections::HashMap;

use crate::shared::geometry::{Point, Rect};

const CURVE_SAMPLES: usize = 20;
const SELF_LOOP_REACH: f64 = 40.0;
const PARALLEL_EDGE_SPACING: f64 = 24.0;
/// Detour offsets tried, in order, to route an edge around nodes in its way.
const DETOUR_OFFSETS: [f64; 6] = [50.0, -50.0, 90.0, -90.0, 150.0, -150.0];

/// Outline used to clip edge ends at a node.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outline {
    Rect,
    Ellipse,
    Diamond,
}

/// Paths for `edges` (pairs of node ids) between the given nodes. Edges whose
/// ends are missing get an empty path.
pub fn route_edges(
    nodes: &HashMap<&str, (Outline, Rect)>,
    edges: &[(&str, &str)],
) -> Vec<Vec<Point>> {
    let pair_key = |from: &str, to: &str| {
        if from <= to {
            (from.to_string(), to.to_string())
        } else {
            (to.to_string(), from.to_string())
        }
    };
    let mut groups: HashMap<(String, String), Vec<usize>> = HashMap::new();
    for (i, (from, to)) in edges.iter().enumerate() {
        groups.entry(pair_key(from, to)).or_default().push(i);
    }
    edges
        .iter()
        .enumerate()
        .map(|(i, &(from, to))| {
            let (Some((fo, fr)), Some((to_outline, tr))) = (nodes.get(from), nodes.get(to)) else {
                return Vec::new();
            };
            if from == to {
                return self_loop_path(*fo, fr);
            }
            let key = pair_key(from, to);
            let group = &groups[&key];
            let k = group.iter().position(|&j| j == i).unwrap_or(0) as f64;
            let mut bend = (k - (group.len() - 1) as f64 / 2.0) * PARALLEL_EDGE_SPACING;
            // Bend direction is relative to the edge direction; keep it
            // consistent within a group of edges going both ways.
            if from != key.0 {
                bend = -bend;
            }
            let path = edge_path(*fo, fr, *to_outline, tr, bend);
            let obstacles: Vec<Rect> = nodes
                .iter()
                .filter(|(id, _)| **id != from && **id != to)
                .map(|(_, (_, r))| r.inflate(4.0))
                .collect();
            if !path_crosses(&path, &obstacles) {
                return path;
            }
            DETOUR_OFFSETS
                .iter()
                .map(|offset| detour_path(*fo, fr, *to_outline, tr, bend + offset))
                .find(|p| !path_crosses(p, &obstacles))
                .unwrap_or(path)
        })
        .collect()
}

/// Where a ray from the shape's center towards `toward` leaves its outline.
pub fn boundary_point(outline: Outline, rect: &Rect, toward: Point) -> Point {
    let c = rect.center();
    let (dx, dy) = (toward.x - c.x, toward.y - c.y);
    if dx == 0.0 && dy == 0.0 {
        return c;
    }
    let (a, b) = (rect.w / 2.0, rect.h / 2.0);
    let t = match outline {
        Outline::Ellipse => 1.0 / ((dx / a).powi(2) + (dy / b).powi(2)).sqrt(),
        Outline::Diamond => 1.0 / (dx.abs() / a + dy.abs() / b),
        Outline::Rect => (a / dx.abs()).min(b / dy.abs()),
    };
    Point::new(c.x + dx * t, c.y + dy * t)
}

/// Path between two nodes. A non-zero `bend` curves the edge sideways so
/// parallel edges between the same nodes stay apart.
pub fn edge_path(
    from_outline: Outline,
    from: &Rect,
    to_outline: Outline,
    to: &Rect,
    bend: f64,
) -> Vec<Point> {
    let (ca, cb) = (from.center(), to.center());
    if bend == 0.0 {
        return vec![
            boundary_point(from_outline, from, cb),
            boundary_point(to_outline, to, ca),
        ];
    }
    let (dx, dy) = (cb.x - ca.x, cb.y - ca.y);
    let len = dx.hypot(dy).max(1e-6);
    let ctrl = Point::new(
        (ca.x + cb.x) / 2.0 - dy / len * bend * 2.0,
        (ca.y + cb.y) / 2.0 + dx / len * bend * 2.0,
    );
    let p0 = boundary_point(from_outline, from, ctrl);
    let p2 = boundary_point(to_outline, to, ctrl);
    (0..=CURVE_SAMPLES)
        .map(|i| {
            let t = i as f64 / CURVE_SAMPLES as f64;
            let u = 1.0 - t;
            Point::new(
                u * u * p0.x + 2.0 * u * t * ctrl.x + t * t * p2.x,
                u * u * p0.y + 2.0 * u * t * ctrl.y + t * t * p2.y,
            )
        })
        .collect()
}

/// Path that leaves both nodes sideways and arcs around whatever lies between
/// them, peaking about `offset` away from the straight line.
pub fn detour_path(
    from_outline: Outline,
    from: &Rect,
    to_outline: Outline,
    to: &Rect,
    offset: f64,
) -> Vec<Point> {
    let (ca, cb) = (from.center(), to.center());
    let (dx, dy) = (cb.x - ca.x, cb.y - ca.y);
    let len = dx.hypot(dy).max(1e-6);
    // A cubic Bézier reaches 3/4 of its control point offset at its midpoint.
    let k = offset * 4.0 / 3.0;
    let (nx, ny) = (-dy / len * k, dx / len * k);
    let c1 = Point::new(ca.x + nx, ca.y + ny);
    let c2 = Point::new(cb.x + nx, cb.y + ny);
    let p0 = boundary_point(from_outline, from, c1);
    let p3 = boundary_point(to_outline, to, c2);
    cubic(p0, c1, c2, p3)
}

fn cubic(p0: Point, c1: Point, c2: Point, p3: Point) -> Vec<Point> {
    (0..=CURVE_SAMPLES)
        .map(|i| {
            let t = i as f64 / CURVE_SAMPLES as f64;
            let u = 1.0 - t;
            let (a, b, c, d) = (u * u * u, 3.0 * u * u * t, 3.0 * u * t * t, t * t * t);
            Point::new(
                a * p0.x + b * c1.x + c * c2.x + d * p3.x,
                a * p0.y + b * c1.y + c * c2.y + d * p3.y,
            )
        })
        .collect()
}

/// Loop leaving the top of the node and coming back on its right side.
pub fn self_loop_path(outline: Outline, rect: &Rect) -> Vec<Point> {
    let c = rect.center();
    let start = boundary_point(
        outline,
        rect,
        Point::new(c.x + rect.w / 4.0, rect.y - rect.h),
    );
    let end = boundary_point(
        outline,
        rect,
        Point::new(rect.x + rect.w * 2.0, c.y - rect.h / 4.0),
    );
    let c1 = Point::new(start.x, start.y - SELF_LOOP_REACH);
    let c2 = Point::new(end.x + SELF_LOOP_REACH, end.y);
    cubic(start, c1, c2, end)
}

/// Whether a path passes through any of the given rectangles.
pub fn path_crosses(path: &[Point], obstacles: &[Rect]) -> bool {
    path.windows(2)
        .any(|w| obstacles.iter().any(|r| segment_intersects(w[0], w[1], r)))
}

/// Liang–Barsky clipping of the segment `a`–`b` against `r`.
fn segment_intersects(a: Point, b: Point, r: &Rect) -> bool {
    let (dx, dy) = (b.x - a.x, b.y - a.y);
    let (mut t0, mut t1) = (0.0_f64, 1.0_f64);
    for (p, q) in [
        (-dx, a.x - r.x),
        (dx, r.x + r.w - a.x),
        (-dy, a.y - r.y),
        (dy, r.y + r.h - a.y),
    ] {
        if p == 0.0 {
            if q < 0.0 {
                return false;
            }
        } else {
            let t = q / p;
            if p < 0.0 {
                t0 = t0.max(t);
            } else {
                t1 = t1.min(t);
            }
        }
    }
    t0 <= t1
}

/// Point where an edge label sits.
pub fn path_midpoint(path: &[Point]) -> Point {
    match path {
        [] => Point::default(),
        [a, b] => Point::new((a.x + b.x) / 2.0, (a.y + b.y) / 2.0),
        _ => path[path.len() / 2],
    }
}

/// Point at `t` (0 to 1) of the way along the path, by length.
fn point_along(path: &[Point], t: f64) -> Option<Point> {
    let lengths: Vec<f64> = path
        .windows(2)
        .map(|w| (w[1].x - w[0].x).hypot(w[1].y - w[0].y))
        .collect();
    let mut remaining = lengths.iter().sum::<f64>() * t;
    for (w, len) in path.windows(2).zip(lengths) {
        if remaining <= len && len > 0.0 {
            let f = remaining / len;
            return Some(Point::new(
                w[0].x + (w[1].x - w[0].x) * f,
                w[0].y + (w[1].y - w[0].y) * f,
            ));
        }
        remaining -= len;
    }
    path.last().copied()
}

/// Where to center a label of `size` on `path`: its middle, or the point
/// nearest to it along the path where the label covers none of `avoid`.
/// Falls back to the middle when no such point exists.
pub fn label_position(path: &[Point], size: (f64, f64), avoid: &[Rect]) -> Point {
    let clear = |p: Point| {
        let r = Rect::centered(p, size);
        !avoid
            .iter()
            .any(|o| r.x < o.x + o.w && o.x < r.x + r.w && r.y < o.y + o.h && o.y < r.y + r.h)
    };
    let middle = path_midpoint(path);
    if clear(middle) {
        return middle;
    }
    (1..=9)
        .flat_map(|i| [0.5 - 0.05 * i as f64, 0.5 + 0.05 * i as f64])
        .filter_map(|t| point_along(path, t))
        .find(|p| clear(*p))
        .unwrap_or(middle)
}

pub fn distance_to_path(p: Point, path: &[Point]) -> f64 {
    path.windows(2)
        .map(|w| distance_to_segment(p, w[0], w[1]))
        .fold(f64::INFINITY, f64::min)
}

fn distance_to_segment(p: Point, a: Point, b: Point) -> f64 {
    let (dx, dy) = (b.x - a.x, b.y - a.y);
    let len2 = dx * dx + dy * dy;
    let t = if len2 == 0.0 {
        0.0
    } else {
        (((p.x - a.x) * dx + (p.y - a.y) * dy) / len2).clamp(0.0, 1.0)
    };
    (p.x - (a.x + t * dx)).hypot(p.y - (a.y + t * dy))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn labels_slide_off_boxes_in_their_way() {
        let path = [Point::new(0.0, 0.0), Point::new(200.0, 0.0)];
        let size = (40.0, 16.0);
        assert_eq!(label_position(&path, size, &[]), Point::new(100.0, 0.0));
        let blocker = Rect::centered(Point::new(100.0, 0.0), (60.0, 30.0));
        let at = label_position(&path, size, &[blocker]);
        let label = Rect::centered(at, size);
        assert!(label.x + label.w <= blocker.x || label.x >= blocker.x + blocker.w);
        assert_eq!(at.y, 0.0);
        // Nowhere clear: keep the middle.
        let wall = Rect::centered(Point::new(100.0, 0.0), (400.0, 30.0));
        assert_eq!(label_position(&path, size, &[wall]), Point::new(100.0, 0.0));
    }

    fn close(a: Point, b: Point) -> bool {
        (a.x - b.x).abs() < 1e-9 && (a.y - b.y).abs() < 1e-9
    }

    #[test]
    fn boundary_of_rectangle() {
        let r = Rect::centered(Point::new(0.0, 0.0), (100.0, 40.0));
        let p = boundary_point(Outline::Rect, &r, Point::new(200.0, 0.0));
        assert!(close(p, Point::new(50.0, 0.0)));
        let p = boundary_point(Outline::Rect, &r, Point::new(0.0, -10.0));
        assert!(close(p, Point::new(0.0, -20.0)));
    }

    #[test]
    fn boundary_of_circle_and_rhombus() {
        let r = Rect::centered(Point::new(0.0, 0.0), (40.0, 40.0));
        let p = boundary_point(Outline::Ellipse, &r, Point::new(10.0, 10.0));
        assert!((p.x.hypot(p.y) - 20.0).abs() < 1e-9);
        let p = boundary_point(Outline::Diamond, &r, Point::new(10.0, 10.0));
        assert!(close(p, Point::new(10.0, 10.0)));
    }

    #[test]
    fn straight_edge_touches_both_outlines() {
        let a = Rect::centered(Point::new(0.0, 0.0), (100.0, 40.0));
        let b = Rect::centered(Point::new(0.0, 200.0), (100.0, 40.0));
        let path = edge_path(Outline::Rect, &a, Outline::Rect, &b, 0.0);
        assert_eq!(path.len(), 2);
        assert!(close(path[0], Point::new(0.0, 20.0)));
        assert!(close(path[1], Point::new(0.0, 180.0)));
    }

    #[test]
    fn bent_edge_curves_to_one_side() {
        let a = Rect::centered(Point::new(0.0, 0.0), (100.0, 40.0));
        let b = Rect::centered(Point::new(0.0, 200.0), (100.0, 40.0));
        let path = edge_path(Outline::Rect, &a, Outline::Rect, &b, 20.0);
        assert!(path_midpoint(&path).x < 0.0);
        let path = edge_path(Outline::Rect, &a, Outline::Rect, &b, -20.0);
        assert!(path_midpoint(&path).x > 0.0);
    }

    #[test]
    fn detects_paths_crossing_obstacles() {
        let path = [Point::new(0.0, 0.0), Point::new(100.0, 0.0)];
        let blocker = Rect::centered(Point::new(50.0, 0.0), (10.0, 10.0));
        let aside = Rect::centered(Point::new(50.0, 30.0), (10.0, 10.0));
        assert!(path_crosses(&path, &[blocker]));
        assert!(!path_crosses(&path, &[aside]));
        let long = [Point::new(0.0, 0.0), Point::new(10_000.0, 0.0)];
        assert!(path_crosses(&long, &[blocker]));
    }

    #[test]
    fn detour_clears_nodes_in_between() {
        let a = Rect::centered(Point::new(0.0, 0.0), (80.0, 40.0));
        let mid = Rect::centered(Point::new(150.0, 0.0), (80.0, 40.0));
        let b = Rect::centered(Point::new(300.0, 0.0), (80.0, 40.0));
        let path = detour_path(Outline::Rect, &a, Outline::Rect, &b, 50.0);
        assert!(!path_crosses(&path, &[mid]));
    }

    #[test]
    fn distance_to_polyline() {
        let path = [Point::new(0.0, 0.0), Point::new(10.0, 0.0)];
        assert_eq!(distance_to_path(Point::new(5.0, 3.0), &path), 3.0);
        assert_eq!(distance_to_path(Point::new(13.0, 4.0), &path), 5.0);
    }

    #[test]
    fn parallel_edges_bend_apart_and_blocked_edges_detour() {
        let a = Rect::centered(Point::new(0.0, 0.0), (80.0, 40.0));
        let mid = Rect::centered(Point::new(150.0, 0.0), (80.0, 40.0));
        let b = Rect::centered(Point::new(300.0, 0.0), (80.0, 40.0));
        let nodes = HashMap::from([
            ("A", (Outline::Rect, a)),
            ("M", (Outline::Rect, mid)),
            ("B", (Outline::Rect, b)),
        ]);
        let paths = route_edges(
            &nodes,
            &[("A", "M"), ("M", "A"), ("A", "B"), ("A", "A"), ("A", "X")],
        );
        assert_eq!(paths.len(), 5);
        // A->M and M->A bend to opposite sides of the straight line.
        let (m1, m2) = (path_midpoint(&paths[0]), path_midpoint(&paths[1]));
        assert!(m1.y * m2.y < 0.0);
        // A->B would cross M, so it detours.
        assert!(!path_crosses(&paths[2], &[mid.inflate(4.0)]));
        assert!(paths[3].len() > 2);
        assert!(paths[4].is_empty());
    }
}
