//! The scene of the written file: the model and the support parts as triangles in the plate
//! frame (mm, z up from the plate), and how to draw them in a panel.

use crate::canvas::{Cam, Image, Raster, Rect, Rgb, View, shade};

/// One triangle, counter-clockwise seen from outside.
pub type Tri = [[f32; 3]; 3];

/// What a support part is, for colour and for the pictures.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    /// The ball at the model contact.
    TipBall,
    /// The tip cone.
    TipCone,
    /// A joint ball.
    Joint,
    /// A column.
    Column,
    /// A brace.
    Brace,
    /// A base flare.
    Flare,
    /// A raft pad or bridge.
    Raft,
}

/// One support part with its mesh.
#[derive(Debug, Clone, PartialEq)]
pub struct Part {
    /// The mesh.
    pub tris: Vec<Tri>,
    /// What it is.
    pub role: Role,
    /// The contact it belongs to (index into the layout's contacts), if known.
    pub contact: Option<u32>,
}

/// The model and its supports, in the plate frame.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Scene {
    /// Model triangles.
    pub model: Vec<Tri>,
    /// Support parts.
    pub parts: Vec<Part>,
}

type V3 = [f64; 3];

fn f64v(p: [f32; 3]) -> V3 {
    [f64::from(p[0]), f64::from(p[1]), f64::from(p[2])]
}

/// The unit normal of a triangle (zero vector for a degenerate one).
#[must_use]
pub fn normal(t: &Tri) -> V3 {
    let (a, b, c) = (f64v(t[0]), f64v(t[1]), f64v(t[2]));
    let (u, v) = (
        [b[0] - a[0], b[1] - a[1], b[2] - a[2]],
        [c[0] - a[0], c[1] - a[1], c[2] - a[2]],
    );
    let n = [
        u[1] * v[2] - u[2] * v[1],
        u[2] * v[0] - u[0] * v[2],
        u[0] * v[1] - u[1] * v[0],
    ];
    let l = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
    if l > 0.0 {
        [n[0] / l, n[1] / l, n[2] / l]
    } else {
        [0.0; 3]
    }
}

/// The centre of a triangle.
#[must_use]
pub fn centroid(t: &Tri) -> V3 {
    let mut c = [0.0; 3];
    for p in t {
        for k in 0..3 {
            c[k] += f64::from(p[k]) / 3.0;
        }
    }
    c
}

/// The area of a triangle, mm².
#[must_use]
pub fn area(t: &Tri) -> f64 {
    let (a, b, c) = (f64v(t[0]), f64v(t[1]), f64v(t[2]));
    let (u, v) = (
        [b[0] - a[0], b[1] - a[1], b[2] - a[2]],
        [c[0] - a[0], c[1] - a[1], c[2] - a[2]],
    );
    let n = [
        u[1] * v[2] - u[2] * v[1],
        u[2] * v[0] - u[0] * v[2],
        u[0] * v[1] - u[1] * v[0],
    ];
    0.5 * (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt()
}

impl Scene {
    /// Corners of the box around the model and every part.
    #[must_use]
    pub fn bounds(&self) -> (V3, V3) {
        let (mut lo, mut hi) = ([f64::INFINITY; 3], [f64::NEG_INFINITY; 3]);
        for t in self
            .model
            .iter()
            .chain(self.parts.iter().flat_map(|p| &p.tris))
        {
            for v in t {
                for k in 0..3 {
                    lo[k] = lo[k].min(f64::from(v[k]));
                    hi[k] = hi[k].max(f64::from(v[k]));
                }
            }
        }
        if lo[0] > hi[0] {
            return ([0.0; 3], [1.0; 3]);
        }
        (lo, hi)
    }

    /// The mean outward normal of the down-facing model faces, by area: the direction the
    /// underside looks.
    #[must_use]
    pub fn underside_normal(&self) -> V3 {
        // The most common down-facing direction: normals binned to 0.1 on each axis, the bin
        // with the most area wins, and its area-weighted mean is the answer.
        let mut bins: Vec<([i32; 3], f64, V3)> = Vec::new();
        for t in &self.model {
            let n = normal(t);
            if n[2] >= 0.0 {
                continue;
            }
            let a = area(t);
            let key = [0, 1, 2].map(|k| (n[k] * 10.0).round() as i32);
            let at = bins.iter().position(|b| b.0 == key).unwrap_or_else(|| {
                bins.push((key, 0.0, [0.0; 3]));
                bins.len() - 1
            });
            bins[at].1 += a;
            for (sum, v) in bins[at].2.iter_mut().zip(n) {
                *sum += v * a;
            }
        }
        match bins.iter().max_by(|a, b| a.1.total_cmp(&b.1)) {
            Some(b) => {
                let s = b.2;
                let l = (s[0] * s[0] + s[1] * s[1] + s[2] * s[2]).sqrt();
                [s[0] / l, s[1] / l, s[2] / l]
            }
            None => [0.0, 0.0, -1.0],
        }
    }

    /// The lowest model vertex.
    #[must_use]
    pub fn lowest_model_point(&self) -> V3 {
        let mut best = [0.0, 0.0, f64::INFINITY];
        for v in self.model.iter().flatten() {
            if f64::from(v[2]) < best[2] {
                best = f64v(*v);
            }
        }
        if best[2].is_finite() { best } else { [0.0; 3] }
    }
}

/// How a panel draws the scene.
#[allow(missing_debug_implementations, reason = "holds closures")]
pub struct Look<'a> {
    /// Draw the model.
    pub model: bool,
    /// Draw the support parts.
    pub parts: bool,
    /// The colour of model triangle `i` with unit normal `n`.
    pub model_colour: &'a dyn Fn(usize, &Tri) -> Rgb,
    /// The colour of a part.
    pub part_colour: &'a dyn Fn(&Part) -> Rgb,
    /// Draw only parts of these roles (all when empty).
    pub roles: &'a [Role],
}

/// The model grey-blue.
pub const MODEL: Rgb = [128, 150, 180];
/// Supports.
pub const SUPPORT: Rgb = [214, 168, 52];
/// Raft.
pub const RAFT: Rgb = [170, 120, 60];
/// Contact patches.
pub const PATCH: Rgb = [226, 48, 48];
/// Warnings.
pub const WARN: Rgb = [255, 140, 20];
/// Good.
pub const GREEN: Rgb = [70, 190, 90];

/// The default model colour.
#[must_use]
pub fn plain_model(_i: usize, _t: &Tri) -> Rgb {
    MODEL
}

/// The default part colour.
#[must_use]
pub fn plain_part(p: &Part) -> Rgb {
    if p.role == Role::Raft { RAFT } else { SUPPORT }
}

/// Draw `scene` into `rect` of `img` as seen by `view`. Returns the camera so the caller can
/// project overlays; coordinates it returns are relative to the panel, so add `rect.x/y`.
pub fn draw_scene(img: &mut Image, rect: Rect, view: &View, scene: &Scene, look: &Look<'_>) -> Cam {
    const SS: usize = 2;
    let (rw, rh) = (rect.w as usize * SS, rect.h as usize * SS);
    let cam = Cam::new(view, rw as f64, rh as f64);
    let mut r = Raster::new(rw, rh);
    let dir = cam.dir();
    let draw = |r: &mut Raster, t: &Tri, base: Rgb, id: u32| {
        let p = [0, 1, 2].map(|k| cam.project(f64v(t[k])));
        r.triangle(p, shade(base, normal(t), dir), id);
    };
    if look.model {
        for (i, t) in scene.model.iter().enumerate() {
            draw(&mut r, t, (look.model_colour)(i, t), 1);
        }
    }
    if look.parts {
        for (pi, part) in scene.parts.iter().enumerate() {
            if !look.roles.is_empty() && !look.roles.contains(&part.role) {
                continue;
            }
            let c = (look.part_colour)(part);
            for t in &part.tris {
                draw(&mut r, t, c, 2 + pi as u32);
            }
        }
    }
    r.outline(0.6);
    r.resolve(img, rect.x, rect.y, SS);
    Cam::new(view, f64::from(rect.w), f64::from(rect.h))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::canvas::BACKGROUND;

    fn square_up() -> Vec<Tri> {
        vec![
            [[0.0, 0.0, 0.0], [4.0, 0.0, 0.0], [4.0, 4.0, 0.0]],
            [[0.0, 0.0, 0.0], [4.0, 4.0, 0.0], [0.0, 4.0, 0.0]],
        ]
    }

    #[test]
    fn normals_areas_and_the_underside_come_from_the_winding() {
        let up = square_up();
        assert!((normal(&up[0])[2] - 1.0).abs() < 1e-12);
        assert!((area(&up[0]) - 8.0).abs() < 1e-12);
        let down: Vec<Tri> = up.iter().map(|t| [t[0], t[2], t[1]]).collect();
        let s = Scene {
            model: down,
            parts: Vec::new(),
        };
        assert!((s.underside_normal()[2] + 1.0).abs() < 1e-12);
        let (lo, hi) = s.bounds();
        assert_eq!((lo[0], hi[0]), (0.0, 4.0));
    }

    #[test]
    fn a_plate_seen_from_above_fills_its_footprint() {
        let s = Scene {
            model: square_up(),
            parts: Vec::new(),
        };
        let mut img = Image::new(100, 100, BACKGROUND);
        let view = View {
            dir: [0.0, 0.0, -1.0],
            up: [0.0, 1.0, 0.0],
            centre: [2.0, 2.0, 0.0],
            half_height: 3.0,
        };
        let look = Look {
            model: true,
            parts: true,
            model_colour: &plain_model,
            part_colour: &plain_part,
            roles: &[],
        };
        let cam = draw_scene(
            &mut img,
            Rect {
                x: 0,
                y: 0,
                w: 100,
                h: 100,
            },
            &view,
            &s,
            &look,
        );
        let (cx, cy, _) = cam.project([2.0, 2.0, 0.0]);
        assert_ne!(img.get(cx as i32, cy as i32), Some(BACKGROUND));
        let (ox, oy, _) = cam.project([4.5, 2.0, 0.0]);
        assert_eq!(img.get(ox as i32, oy as i32), Some(BACKGROUND));
    }
}
