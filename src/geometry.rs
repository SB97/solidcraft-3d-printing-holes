//! Pure 2D geometry of a teardrop hole, in sketch coordinates. No host calls.
//!
//! A teardrop is a circle (centre `c`, radius `r`) whose top is replaced by two
//! tangent lines meeting in a tip on the "up" axis, at `d` from the centre. The plugin's
//! default is a low tip, `d = r * (1 + p/100)` with a few percent `p`; with an overhang
//! angle `alpha` (from the horizontal) instead, `d = r / cos(alpha)`: `r * sqrt(2)` for 45°.

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Vec2 {
    pub x: f64,
    pub y: f64,
}

impl Vec2 {
    pub const fn new(x: f64, y: f64) -> Self {
        Self { x, y }
    }
    pub fn add(self, o: Vec2) -> Vec2 {
        Vec2::new(self.x + o.x, self.y + o.y)
    }
    pub fn scale(self, k: f64) -> Vec2 {
        Vec2::new(self.x * k, self.y * k)
    }
    pub fn len(self) -> f64 {
        self.x.hypot(self.y)
    }
    pub fn normalized(self) -> Option<Vec2> {
        let l = self.len();
        (l > 1e-12).then(|| self.scale(1.0 / l))
    }
    /// Rotated 90° counter-clockwise.
    pub fn left(self) -> Vec2 {
        Vec2::new(-self.y, self.x)
    }
    /// Angle from the sketch X axis in degrees, in [0, 360).
    pub fn angle_deg(self) -> f64 {
        let a = self.y.atan2(self.x).to_degrees();
        let a = if a < 0.0 { a + 360.0 } else { a };
        if a >= 360.0 - 1e-9 {
            0.0
        } else {
            a
        }
    }
}

/// Minimum tilt between the sketch plane and the horizontal (in sin) for `up` = world +Z.
/// Below that world +Z gives no usable direction in the sketch.
pub const MIN_AUTO_UP: f64 = 0.17; // ~10°

/// World +Z projected into a sketch plane, as a sketch-space unit vector; `None` when the
/// plane is (nearly) horizontal and the projection gives no direction.
pub fn auto_up(x_dir: [f64; 3], y_dir: [f64; 3]) -> Option<Vec2> {
    let v = Vec2::new(x_dir[2], y_dir[2]);
    if v.len() < MIN_AUTO_UP {
        return None;
    }
    v.normalized()
}

/// Parse an explicit `up` value: `+x`, `-x`, `+y`, `-y`, or an angle in degrees from sketch X.
pub fn parse_up(text: &str) -> Result<Vec2, String> {
    let t = text.trim().to_ascii_lowercase();
    let v = match t.as_str() {
        "+x" | "x" => Vec2::new(1.0, 0.0),
        "-x" => Vec2::new(-1.0, 0.0),
        "+y" | "y" => Vec2::new(0.0, 1.0),
        "-y" => Vec2::new(0.0, -1.0),
        _ => {
            let num = t.trim_end_matches("deg").trim_end_matches('°').trim();
            let a: f64 = num.parse().map_err(|_| {
                format!("`{text}` is not world +Z, a sketch axis (+X, -X, +Y, -Y) or an angle in degrees")
            })?;
            let r = a.to_radians();
            Vec2::new(r.cos(), r.sin())
        }
    };
    // Snap tiny components so axis directions stay exactly axis aligned.
    let snap = |c: f64| if c.abs() < 1e-12 { 0.0 } else { c };
    Ok(Vec2::new(snap(v.x), snap(v.y)))
}

/// How the tip is sized.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Tip {
    /// Overhang angle from the horizontal, degrees (0 < a < 90).
    Angle(f64),
    /// Distance from the centre to the tip, mm (> r).
    Height(f64),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Teardrop {
    /// Distance centre → (virtual) tip.
    pub tip_dist: f64,
    /// Half the angle the arc leaves open at the top, measured from `up` (= overhang angle).
    pub phi_deg: f64,
    /// Apex angle between the two flanks, degrees (= 180 - 2*overhang).
    pub apex_deg: f64,
    pub tip: Vec2,
    /// Tangent point on the left of `up` (counter-clockwise side): the arc's start.
    pub t_left: Vec2,
    /// Tangent point on the right of `up`: the arc's end (the arc runs CCW around the bottom).
    pub t_right: Vec2,
}

pub fn teardrop(center: Vec2, r: f64, up: Vec2, tip: Tip) -> Result<Teardrop, String> {
    if !(r.is_finite() && r > 0.0) {
        return Err(format!("the circle's radius must be positive (is {r})"));
    }
    let u = up.normalized().ok_or("`up` must not be a zero vector")?;
    let (tip_dist, phi) = match tip {
        Tip::Angle(a) => {
            if !(a.is_finite() && a > 0.0 && a < 90.0) {
                return Err(format!("overhang_angle must be between 0 and 90 degrees (is {a})"));
            }
            let a = a.to_radians();
            (r / a.cos(), a)
        }
        Tip::Height(h) => {
            if !(h.is_finite() && h > r * (1.0 + 1e-6)) {
                return Err(format!(
                    "the tip must lie above the circle: the override is measured from the centre, so {h:.4} mm must exceed the radius {r:.4} mm (e.g. d/2 + 0.3 mm)"
                ));
            }
            (h, (r / h).acos())
        }
    };
    let w = u.left();
    let at = |along: f64, side: f64| center.add(u.scale(along)).add(w.scale(side));
    let (s, c) = phi.sin_cos();
    let t_left = at(r * c, r * s);
    let t_right = at(r * c, -r * s);
    Ok(Teardrop {
        tip_dist,
        phi_deg: phi.to_degrees(),
        apex_deg: 180.0 - 2.0 * phi.to_degrees(),
        tip: at(tip_dist, 0.0),
        t_left,
        t_right,
    })
}

/// Area of the teardrop profile (for checks): bottom sector plus the kite.
pub fn area(r: f64, t: &Teardrop) -> f64 {
    let phi = t.phi_deg.to_radians();
    let sector = r * r * (std::f64::consts::PI - phi);
    // Two right triangles centre-tangent point-tip, legs r and L = d*sin(phi).
    let kite = r * t.tip_dist * phi.sin();
    sector + kite
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-9
    }

    #[test]
    fn classic_45_degrees() {
        let t = teardrop(Vec2::new(0.0, 0.0), 4.0, Vec2::new(0.0, 1.0), Tip::Angle(45.0)).unwrap();
        assert!(close(t.tip_dist, 4.0 * 2f64.sqrt()));
        assert!(close(t.apex_deg, 90.0));
        assert!(close(t.tip.y, 4.0 * 2f64.sqrt()) && close(t.tip.x, 0.0));
        let s = 4.0 / 2f64.sqrt();
        assert!(close(t.t_left.x, -s) && close(t.t_left.y, s));
        assert!(close(t.t_right.x, s) && close(t.t_right.y, s));
        // r²(3π/4 + 1)
        assert!(close(area(4.0, &t), 16.0 * (0.75 * std::f64::consts::PI + 1.0)));
    }

    #[test]
    fn up_down_in_sketch_matches_solidcraft_prototype() {
        // XZ plane: sketch y = world -Z, so up is -y. Centre (0,5), r 4: tip at y = 5 - 4√2.
        let up = auto_up([1.0, 0.0, 0.0], [0.0, 0.0, -1.0]).unwrap();
        assert_eq!(up, Vec2::new(0.0, -1.0));
        let t = teardrop(Vec2::new(0.0, 5.0), 4.0, up, Tip::Angle(45.0)).unwrap();
        assert!(close(t.tip.y, 5.0 - 4.0 * 2f64.sqrt()));
        // arc start (left of up = CCW of (0,-1) = +x side)
        assert!(t.t_left.x > 0.0 && t.t_right.x < 0.0);
        assert!((area(4.0, &t) - 53.69911184).abs() < 1e-6);
    }

    #[test]
    fn horizontal_plane_gives_no_direction() {
        assert!(auto_up([1.0, 0.0, 0.0], [0.0, 1.0, 0.0]).is_none());
    }

    #[test]
    fn tilted_plane_projects_z() {
        let k = 0.5f64.sqrt();
        let up = auto_up([1.0, 0.0, 0.0], [0.0, k, k]).unwrap();
        assert!(close(up.x, 0.0) && close(up.y, 1.0));
    }

    #[test]
    fn explicit_up() {
        assert_eq!(parse_up("+x").unwrap(), Vec2::new(1.0, 0.0));
        assert_eq!(parse_up("-Y").unwrap(), Vec2::new(0.0, -1.0));
        assert_eq!(parse_up("90").unwrap(), Vec2::new(0.0, 1.0));
        assert_eq!(parse_up("180 deg").unwrap(), Vec2::new(-1.0, 0.0));
        assert!(parse_up("oben").is_err());
        assert!(close(Vec2::new(0.0, -1.0).angle_deg(), 270.0));
        assert!(close(Vec2::new(1.0, 0.0).angle_deg(), 0.0));
    }

    #[test]
    fn height_override() {
        let t = teardrop(Vec2::new(0.0, 0.0), 4.0, Vec2::new(0.0, 1.0), Tip::Height(8.0)).unwrap();
        assert!(close(t.phi_deg, 60.0)); // cos φ = r/h = 0.5
        assert!(close(t.apex_deg, 60.0));
        assert!(teardrop(Vec2::new(0.0, 0.0), 4.0, Vec2::new(0.0, 1.0), Tip::Height(4.0)).is_err());
    }


    #[test]
    fn bad_angles() {
        for a in [0.0, 90.0, -5.0, f64::NAN] {
            assert!(teardrop(Vec2::new(0.0, 0.0), 1.0, Vec2::new(0.0, 1.0), Tip::Angle(a)).is_err());
        }
    }
}
