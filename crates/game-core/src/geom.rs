use hollowrite_protocol::{Dir8, CELL};

/// World position in milli-cells.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Pos {
    pub x: i32,
    pub y: i32,
}

/// 1/sqrt(2) in thousandths, used to normalise diagonal movement.
const DIAG: i64 = 707;

impl Pos {
    pub const fn new(x: i32, y: i32) -> Self {
        Pos { x, y }
    }

    pub fn cell_center(cx: i32, cy: i32) -> Self {
        Pos { x: cx * CELL + CELL / 2, y: cy * CELL + CELL / 2 }
    }

    pub fn cell(self) -> (i32, i32) {
        (self.x.div_euclid(CELL), self.y.div_euclid(CELL))
    }

    pub fn dist2(self, o: Pos) -> i64 {
        let dx = (self.x - o.x) as i64;
        let dy = (self.y - o.y) as i64;
        dx * dx + dy * dy
    }

    pub fn dist(self, o: Pos) -> i32 {
        (self.dist2(o) as f64).sqrt() as i32
    }

    pub fn within(self, o: Pos, r: i32) -> bool {
        self.dist2(o) <= (r as i64) * (r as i64)
    }

    pub fn chebyshev(self, o: Pos) -> i32 {
        (self.x - o.x).abs().max((self.y - o.y).abs())
    }

    pub fn offset(self, d: Dir8, amount: i32) -> Pos {
        let (dx, dy) = step(d, amount);
        Pos { x: self.x + dx, y: self.y + dy }
    }

    pub fn dir_to(self, o: Pos) -> Option<Dir8> {
        dir_towards(o.x - self.x, o.y - self.y)
    }
}

/// Displacement for moving `amount` along `d`; diagonals are scaled so the
/// travelled distance matches the orthogonal case.
pub fn step(d: Dir8, amount: i32) -> (i32, i32) {
    let (dx, dy) = d.offset();
    if d.is_diagonal() {
        let a = (amount as i64 * DIAG / 1000) as i32;
        (dx * a, dy * a)
    } else {
        (dx * amount, dy * amount)
    }
}

/// Nearest of the eight compass directions for a vector, or `None` for zero.
pub fn dir_towards(dx: i32, dy: i32) -> Option<Dir8> {
    if dx == 0 && dy == 0 {
        return None;
    }
    // tan(22.5°) ≈ 0.4142; compare with integer arithmetic.
    let (ax, ay) = ((dx as i64).abs(), (dy as i64).abs());
    let sx = dx.signum();
    let sy = dy.signum();
    if ay * 10000 <= ax * 4142 {
        Dir8::from_offset(sx, 0)
    } else if ax * 10000 <= ay * 4142 {
        Dir8::from_offset(0, sy)
    } else {
        Dir8::from_offset(sx, sy)
    }
}

pub fn unit(d: Dir8) -> (f64, f64) {
    let (dx, dy) = d.offset();
    let len = ((dx * dx + dy * dy) as f64).sqrt();
    (dx as f64 / len, dy as f64 / len)
}

/// Is `p` within a cone of `arc_deg` total width and radius `reach`,
/// centred on `origin` and opening along `facing`? `slack` widens the
/// radius to account for the target's body.
pub fn in_arc(origin: Pos, facing: Dir8, reach: i32, arc_deg: u32, p: Pos, slack: i32) -> bool {
    let dx = (p.x - origin.x) as f64;
    let dy = (p.y - origin.y) as f64;
    let d2 = dx * dx + dy * dy;
    let r = (reach + slack) as f64;
    if d2 > r * r {
        return false;
    }
    if d2 < (slack as f64).powi(2) {
        return true;
    }
    let (ux, uy) = unit(facing);
    let cos = (dx * ux + dy * uy) / d2.sqrt();
    cos >= (arc_deg as f64 / 2.0).to_radians().cos()
}

/// Is `p` within a thrust of length `reach` and half-width `half_width`?
pub fn in_line(origin: Pos, facing: Dir8, reach: i32, half_width: i32, p: Pos) -> bool {
    let (ux, uy) = unit(facing);
    let dx = (p.x - origin.x) as f64;
    let dy = (p.y - origin.y) as f64;
    let along = dx * ux + dy * uy;
    let across = (dx * -uy + dy * ux).abs();
    along >= -(half_width as f64) && along <= reach as f64 && across <= half_width as f64
}

/// Map cells whose centres fall inside a predicate, scanned over a square
/// of `radius` around `origin`.
pub fn cells_where(origin: Pos, radius: i32, mut f: impl FnMut(Pos) -> bool) -> Vec<[i16; 2]> {
    let (ox, oy) = origin.cell();
    let r = radius / CELL + 1;
    let mut out = Vec::new();
    for cy in oy - r..=oy + r {
        for cx in ox - r..=ox + r {
            if cx < 0 || cy < 0 {
                continue;
            }
            if f(Pos::cell_center(cx, cy)) {
                out.push([cx as i16, cy as i16]);
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn diagonal_speed_is_normalised() {
        let orth = step(Dir8::E, 1000);
        let diag = step(Dir8::Ne, 1000);
        let orth_len = ((orth.0 * orth.0 + orth.1 * orth.1) as f64).sqrt();
        let diag_len = ((diag.0 * diag.0 + diag.1 * diag.1) as f64).sqrt();
        assert!((orth_len - diag_len).abs() < 2.0, "{orth_len} vs {diag_len}");
    }

    #[test]
    fn dir_towards_buckets() {
        assert_eq!(dir_towards(10, 0), Some(Dir8::E));
        assert_eq!(dir_towards(10, 3), Some(Dir8::E));
        assert_eq!(dir_towards(10, 10), Some(Dir8::Se));
        assert_eq!(dir_towards(-1, -10), Some(Dir8::N));
        assert_eq!(dir_towards(0, 0), None);
    }

    #[test]
    fn arc_respects_angle_and_reach() {
        let o = Pos::new(0, 0);
        assert!(in_arc(o, Dir8::E, 2000, 90, Pos::new(1500, 0), 0));
        assert!(in_arc(o, Dir8::E, 2000, 90, Pos::new(1000, 900), 0));
        assert!(!in_arc(o, Dir8::E, 2000, 90, Pos::new(0, 1500), 0));
        assert!(!in_arc(o, Dir8::E, 2000, 90, Pos::new(-1500, 0), 0));
        assert!(!in_arc(o, Dir8::E, 2000, 90, Pos::new(2500, 0), 0));
        assert!(in_arc(o, Dir8::E, 2000, 90, Pos::new(2200, 0), 300));
    }

    #[test]
    fn line_respects_width() {
        let o = Pos::new(0, 0);
        assert!(in_line(o, Dir8::S, 3000, 500, Pos::new(200, 2500)));
        assert!(!in_line(o, Dir8::S, 3000, 500, Pos::new(800, 2500)));
        assert!(!in_line(o, Dir8::S, 3000, 500, Pos::new(0, 3500)));
    }
}
