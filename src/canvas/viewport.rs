//! Pan and zoom state mapping canvas coordinates to widget pixels.

use crate::shared::geometry::{Point, Rect};

const MIN_ZOOM: f64 = 0.2;
const MAX_ZOOM: f64 = 4.0;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Viewport {
    pub zoom: f64,
    /// Widget position of the canvas origin.
    pub offset: (f64, f64),
}

impl Default for Viewport {
    fn default() -> Self {
        Self {
            zoom: 1.0,
            offset: (0.0, 0.0),
        }
    }
}

impl Viewport {
    pub fn to_world(self, x: f64, y: f64) -> Point {
        Point::new(
            (x - self.offset.0) / self.zoom,
            (y - self.offset.1) / self.zoom,
        )
    }

    pub fn to_screen(self, p: Point) -> (f64, f64) {
        (
            p.x * self.zoom + self.offset.0,
            p.y * self.zoom + self.offset.1,
        )
    }

    /// Canvas area visible in a widget of the given size.
    pub fn visible(self, (w, h): (f64, f64)) -> Rect {
        let top_left = self.to_world(0.0, 0.0);
        Rect {
            x: top_left.x,
            y: top_left.y,
            w: w / self.zoom,
            h: h / self.zoom,
        }
    }

    pub fn pan(&mut self, dx: f64, dy: f64) {
        self.offset = (self.offset.0 + dx, self.offset.1 + dy);
    }

    /// Scales by `factor`, keeping the canvas point under widget position `at` fixed.
    pub fn zoom_around(&mut self, at: (f64, f64), factor: f64) {
        let anchor = self.to_world(at.0, at.1);
        self.zoom = (self.zoom * factor).clamp(MIN_ZOOM, MAX_ZOOM);
        self.offset = (at.0 - anchor.x * self.zoom, at.1 - anchor.y * self.zoom);
    }

    /// Centers `bounds` in a widget of the given size, zooming out if needed
    /// but never in beyond 100%.
    pub fn fit(&mut self, bounds: Rect, (w, h): (f64, f64)) {
        self.zoom = (w / bounds.w).min(h / bounds.h).clamp(MIN_ZOOM, 1.0);
        let c = bounds.center();
        self.offset = (w / 2.0 - c.x * self.zoom, h / 2.0 - c.y * self.zoom);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn screen_and_world_round_trip() {
        let v = Viewport {
            zoom: 2.0,
            offset: (10.0, -5.0),
        };
        let p = v.to_world(50.0, 40.0);
        assert_eq!(v.to_screen(p), (50.0, 40.0));
    }

    #[test]
    fn zoom_keeps_anchor_fixed() {
        let mut v = Viewport::default();
        let before = v.to_world(100.0, 80.0);
        v.zoom_around((100.0, 80.0), 1.5);
        assert_eq!(v.to_world(100.0, 80.0), before);
        assert_eq!(v.zoom, 1.5);
    }

    #[test]
    fn zoom_is_clamped() {
        let mut v = Viewport::default();
        v.zoom_around((0.0, 0.0), 100.0);
        assert_eq!(v.zoom, MAX_ZOOM);
    }

    #[test]
    fn fit_centers_bounds() {
        let mut v = Viewport::default();
        let bounds = Rect::centered(Point::new(500.0, 500.0), (2000.0, 100.0));
        v.fit(bounds, (1000.0, 500.0));
        assert_eq!(v.zoom, 0.5);
        assert_eq!(v.to_screen(Point::new(500.0, 500.0)), (500.0, 250.0));
    }
}
