use crate::flowchart::NodeShape;
use crate::routing::Outline;

/// Horizontal and vertical space between a label and its shape outline.
const PADDING: (f64, f64) = (32.0, 22.0);
const MIN_SIZE: (f64, f64) = (60.0, 40.0);

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

/// Outline edges are clipped against for a shape.
pub fn outline(shape: NodeShape) -> Outline {
    match shape {
        NodeShape::Circle | NodeShape::DoubleCircle => Outline::Ellipse,
        NodeShape::Rhombus => Outline::Diamond,
        _ => Outline::Rect,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn circles_are_round() {
        let (w, h) = shape_size(NodeShape::Circle, (80.0, 18.0));
        assert_eq!(w, h);
    }
}
