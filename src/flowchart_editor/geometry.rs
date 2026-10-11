use crate::flowchart::NodeShape;
use crate::geometry::{Point, Rect};

/// Horizontal and vertical space between a label and its shape outline.
const PADDING: (f64, f64) = (32.0, 22.0);
const MIN_SIZE: (f64, f64) = (60.0, 40.0);
const CURVE_SAMPLES: usize = 20;
const SELF_LOOP_REACH: f64 = 40.0;

/// Outer size of a shape wrapping a label of the given size.
pub fn shape_size(shape: NodeShape, (tw, th): (f64, f64)) -> (f64, f64) {
    let w = (tw + PADDING.0).max(MIN_SIZE.0);
    let h = (th + PADDING.1).max(MIN_SIZE.1);
    match shape {
        NodeShape::Circle | NodeShape::DoubleCircle => {
            let d = (tw + PADDING.1).max(h);
            (d, d)
        }
        NodeShape::Rhombus => (w * 1.5, h * 1.5),
        NodeShape::Hexagon => (w + h / 2.0, h),
        NodeShape::Parallelogram
        | NodeShape::ParallelogramAlt
        | NodeShape::Trapezoid
        | NodeShape::TrapezoidAlt => (w + h * 2.0 / 3.0, h),
        NodeShape::Asymmetric => (w + h / 3.0, h),
        NodeShape::Cylinder => (w, h + 12.0),
        _ => (w, h),
    }
}

/// Where a ray from the shape's center towards `toward` leaves its outline.
pub fn boundary_point(shape: NodeShape, rect: &Rect, toward: Point) -> Point {
    let c = rect.center();
    let (dx, dy) = (toward.x - c.x, toward.y - c.y);
    if dx == 0.0 && dy == 0.0 {
        return c;
    }
    let (a, b) = (rect.w / 2.0, rect.h / 2.0);
    let t = match shape {
        NodeShape::Circle | NodeShape::DoubleCircle => {
            1.0 / ((dx / a).powi(2) + (dy / b).powi(2)).sqrt()
        }
        NodeShape::Rhombus => 1.0 / (dx.abs() / a + dy.abs() / b),
        _ => (a / dx.abs()).min(b / dy.abs()),
    };
    Point::new(c.x + dx * t, c.y + dy * t)
}

/// Path between two nodes. A non-zero `bend` curves the edge sideways so
/// parallel edges between the same nodes stay apart.
pub fn edge_path(
    from_shape: NodeShape,
    from: &Rect,
    to_shape: NodeShape,
    to: &Rect,
    bend: f64,
) -> Vec<Point> {
    let (ca, cb) = (from.center(), to.center());
    if bend == 0.0 {
        return vec![
            boundary_point(from_shape, from, cb),
            boundary_point(to_shape, to, ca),
        ];
    }
    let (dx, dy) = (cb.x - ca.x, cb.y - ca.y);
    let len = dx.hypot(dy).max(1e-6);
    let ctrl = Point::new(
        (ca.x + cb.x) / 2.0 - dy / len * bend * 2.0,
        (ca.y + cb.y) / 2.0 + dx / len * bend * 2.0,
    );
    let p0 = boundary_point(from_shape, from, ctrl);
    let p2 = boundary_point(to_shape, to, ctrl);
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
    from_shape: NodeShape,
    from: &Rect,
    to_shape: NodeShape,
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
    let p0 = boundary_point(from_shape, from, c1);
    let p3 = boundary_point(to_shape, to, c2);
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
pub fn self_loop_path(shape: NodeShape, rect: &Rect) -> Vec<Point> {
    let c = rect.center();
    let start = boundary_point(shape, rect, Point::new(c.x + rect.w / 4.0, rect.y - rect.h));
    let end = boundary_point(
        shape,
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

    fn close(a: Point, b: Point) -> bool {
        (a.x - b.x).abs() < 1e-9 && (a.y - b.y).abs() < 1e-9
    }

    #[test]
    fn boundary_of_rectangle() {
        let r = Rect::centered(Point::new(0.0, 0.0), (100.0, 40.0));
        let p = boundary_point(NodeShape::Rectangle, &r, Point::new(200.0, 0.0));
        assert!(close(p, Point::new(50.0, 0.0)));
        let p = boundary_point(NodeShape::Rectangle, &r, Point::new(0.0, -10.0));
        assert!(close(p, Point::new(0.0, -20.0)));
    }

    #[test]
    fn boundary_of_circle_and_rhombus() {
        let r = Rect::centered(Point::new(0.0, 0.0), (40.0, 40.0));
        let p = boundary_point(NodeShape::Circle, &r, Point::new(10.0, 10.0));
        assert!((p.x.hypot(p.y) - 20.0).abs() < 1e-9);
        let p = boundary_point(NodeShape::Rhombus, &r, Point::new(10.0, 10.0));
        assert!(close(p, Point::new(10.0, 10.0)));
    }

    #[test]
    fn straight_edge_touches_both_outlines() {
        let a = Rect::centered(Point::new(0.0, 0.0), (100.0, 40.0));
        let b = Rect::centered(Point::new(0.0, 200.0), (100.0, 40.0));
        let path = edge_path(NodeShape::Rectangle, &a, NodeShape::Rectangle, &b, 0.0);
        assert_eq!(path.len(), 2);
        assert!(close(path[0], Point::new(0.0, 20.0)));
        assert!(close(path[1], Point::new(0.0, 180.0)));
    }

    #[test]
    fn bent_edge_curves_to_one_side() {
        let a = Rect::centered(Point::new(0.0, 0.0), (100.0, 40.0));
        let b = Rect::centered(Point::new(0.0, 200.0), (100.0, 40.0));
        let path = edge_path(NodeShape::Rectangle, &a, NodeShape::Rectangle, &b, 20.0);
        assert!(path_midpoint(&path).x < 0.0);
        let path = edge_path(NodeShape::Rectangle, &a, NodeShape::Rectangle, &b, -20.0);
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
        let path = detour_path(NodeShape::Rectangle, &a, NodeShape::Rectangle, &b, 50.0);
        assert!(!path_crosses(&path, &[mid]));
    }

    #[test]
    fn distance_to_polyline() {
        let path = [Point::new(0.0, 0.0), Point::new(10.0, 0.0)];
        assert_eq!(distance_to_path(Point::new(5.0, 3.0), &path), 3.0);
        assert_eq!(distance_to_path(Point::new(13.0, 4.0), &path), 5.0);
    }

    #[test]
    fn circles_are_round() {
        let (w, h) = shape_size(NodeShape::Circle, (80.0, 18.0));
        assert_eq!(w, h);
    }
}
