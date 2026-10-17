//! Plain 2D geometry in canvas coordinates.

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Point {
    pub x: f64,
    pub y: f64,
}

impl Point {
    pub fn new(x: f64, y: f64) -> Self {
        Self { x, y }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

impl Rect {
    pub fn centered(center: Point, (w, h): (f64, f64)) -> Self {
        Self {
            x: center.x - w / 2.0,
            y: center.y - h / 2.0,
            w,
            h,
        }
    }

    pub fn center(&self) -> Point {
        Point::new(self.x + self.w / 2.0, self.y + self.h / 2.0)
    }

    pub fn contains(&self, p: Point) -> bool {
        p.x >= self.x && p.x <= self.x + self.w && p.y >= self.y && p.y <= self.y + self.h
    }

    pub fn inflate(&self, d: f64) -> Self {
        Self {
            x: self.x - d,
            y: self.y - d,
            w: self.w + 2.0 * d,
            h: self.h + 2.0 * d,
        }
    }

    pub fn union(&self, other: &Rect) -> Self {
        let x = self.x.min(other.x);
        let y = self.y.min(other.y);
        Self {
            x,
            y,
            w: (self.x + self.w).max(other.x + other.w) - x,
            h: (self.y + self.h).max(other.y + other.h) - y,
        }
    }

    /// Smallest rectangle containing `self` and `p`.
    pub fn include(&self, p: Point) -> Self {
        self.union(&Rect {
            x: p.x,
            y: p.y,
            w: 0.0,
            h: 0.0,
        })
    }

    /// Connection handles at the middle of each side: top, right, bottom, left.
    pub fn ports(&self) -> [Point; 4] {
        let c = self.center();
        [
            Point::new(c.x, self.y),
            Point::new(self.x + self.w, c.y),
            Point::new(c.x, self.y + self.h),
            Point::new(self.x, c.y),
        ]
    }
}
