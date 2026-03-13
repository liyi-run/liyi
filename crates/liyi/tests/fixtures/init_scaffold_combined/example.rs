/// A point in 2D space.
pub struct Point {
    pub x: f64,
    pub y: f64,
}

struct Internal(u8);

impl Point {
    /// Create a new point at the origin.
    pub fn origin() -> Self {
        Self { x: 0.0, y: 0.0 }
    }

    /// Calculate the Euclidean distance to another point.
    ///
    /// Uses the standard distance formula: sqrt((x2-x1)^2 + (y2-y1)^2).
    pub fn distance_to(&self, other: &Point) -> f64 {
        let dx = self.x - other.x;
        let dy = self.y - other.y;
        (dx * dx + dy * dy).sqrt()
    }

    pub fn translate(&mut self, dx: f64, dy: f64) {
        self.x += dx;
        self.y += dy;
    }
}

/// Scale all points by a given factor.
pub fn scale_all(points: &mut [Point], factor: f64) {
    for p in points.iter_mut() {
        p.x *= factor;
        p.y *= factor;
    }
}

fn identity(x: f64) -> f64 {
    x
}
