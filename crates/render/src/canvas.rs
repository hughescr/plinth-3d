//! Drawing: an RGB image with lines, discs, text and boxes; an orthographic camera; and a
//! z-buffered triangle rasteriser with flat shading and outlines.

use crate::font::{CELL_H, CELL_W, glyph};
use crate::png::encode_rgb;

/// A colour.
pub type Rgb = [u8; 3];

/// The background of every picture (ChituBox's dark grey-blue).
pub const BACKGROUND: Rgb = [38, 42, 52];
/// Text on the background.
pub const TEXT: Rgb = [235, 238, 245];

/// A raster image, 3 bytes per pixel, row by row from the top.
#[derive(Debug, Clone, PartialEq)]
pub struct Image {
    /// Width, pixels.
    pub w: u32,
    /// Height, pixels.
    pub h: u32,
    /// Pixel data, `w * h * 3` bytes.
    pub rgb: Vec<u8>,
}

/// A rectangle in pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rect {
    /// Left.
    pub x: i32,
    /// Top.
    pub y: i32,
    /// Width.
    pub w: i32,
    /// Height.
    pub h: i32,
}

impl Image {
    /// A new image filled with `bg`.
    #[must_use]
    pub fn new(w: u32, h: u32, bg: Rgb) -> Self {
        let mut rgb = Vec::with_capacity((w * h * 3) as usize);
        for _ in 0..w * h {
            rgb.extend_from_slice(&bg);
        }
        Self { w, h, rgb }
    }

    /// The PNG file bytes.
    #[must_use]
    pub fn to_png(&self) -> Vec<u8> {
        encode_rgb(self.w, self.h, &self.rgb)
    }

    /// The pixel at `(x, y)`, if inside.
    #[must_use]
    pub fn get(&self, x: i32, y: i32) -> Option<Rgb> {
        let i = self.index(x, y)?;
        Some([self.rgb[i], self.rgb[i + 1], self.rgb[i + 2]])
    }

    fn index(&self, x: i32, y: i32) -> Option<usize> {
        (x >= 0 && y >= 0 && (x as u32) < self.w && (y as u32) < self.h)
            .then(|| ((y as u32 * self.w + x as u32) * 3) as usize)
    }

    /// Set a pixel (ignored outside the image).
    pub fn set(&mut self, x: i32, y: i32, c: Rgb) {
        if let Some(i) = self.index(x, y) {
            self.rgb[i..i + 3].copy_from_slice(&c);
        }
    }

    /// Blend colour `c` over a pixel with weight `a` in 0..=1.
    pub fn blend(&mut self, x: i32, y: i32, c: Rgb, a: f32) {
        if let Some(i) = self.index(x, y) {
            for (dst, &src) in self.rgb[i..i + 3].iter_mut().zip(&c) {
                let v = f32::from(*dst) * (1.0 - a) + f32::from(src) * a;
                *dst = v.round().clamp(0.0, 255.0) as u8;
            }
        }
    }

    /// Fill a rectangle.
    pub fn fill_rect(&mut self, r: Rect, c: Rgb) {
        for y in r.y..r.y + r.h {
            for x in r.x..r.x + r.w {
                self.set(x, y, c);
            }
        }
    }

    /// Fill a rectangle with `c` at weight `a`.
    pub fn shade_rect(&mut self, r: Rect, c: Rgb, a: f32) {
        for y in r.y..r.y + r.h {
            for x in r.x..r.x + r.w {
                self.blend(x, y, c, a);
            }
        }
    }

    /// A rectangle outline `t` pixels thick.
    pub fn frame(&mut self, r: Rect, c: Rgb, t: i32) {
        self.fill_rect(Rect { h: t, ..r }, c);
        self.fill_rect(
            Rect {
                y: r.y + r.h - t,
                h: t,
                ..r
            },
            c,
        );
        self.fill_rect(Rect { w: t, ..r }, c);
        self.fill_rect(
            Rect {
                x: r.x + r.w - t,
                w: t,
                ..r
            },
            c,
        );
    }

    /// A line of thickness `t` pixels (round pen).
    pub fn line(&mut self, a: (f64, f64), b: (f64, f64), c: Rgb, t: f64) {
        let len = (b.0 - a.0).hypot(b.1 - a.1);
        if !len.is_finite() {
            return;
        }
        let steps = (len * 2.0).ceil().clamp(1.0, 20_000.0) as i32;
        for s in 0..=steps {
            let f = f64::from(s) / f64::from(steps);
            self.disc((a.0 + (b.0 - a.0) * f, a.1 + (b.1 - a.1) * f), 0.5 * t, c);
        }
    }

    /// A filled disc.
    pub fn disc(&mut self, p: (f64, f64), r: f64, c: Rgb) {
        if !(p.0.is_finite() && p.1.is_finite() && r.is_finite()) {
            return;
        }
        let (x0, x1) = (
            ((p.0 - r - 1.0).floor() as i32).max(-1),
            ((p.0 + r + 1.0).ceil() as i32).min(self.w as i32),
        );
        let (y0, y1) = (
            ((p.1 - r - 1.0).floor() as i32).max(-1),
            ((p.1 + r + 1.0).ceil() as i32).min(self.h as i32),
        );
        for y in y0..=y1 {
            for x in x0..=x1 {
                let d = (f64::from(x) + 0.5 - p.0).hypot(f64::from(y) + 0.5 - p.1);
                if d <= r {
                    self.set(x, y, c);
                }
            }
        }
    }

    /// A circle outline of thickness `t`.
    pub fn ring(&mut self, p: (f64, f64), r: f64, c: Rgb, t: f64) {
        let n = ((r * 6.0).ceil() as i32).max(16);
        for i in 0..n {
            let (a0, a1) = (
                f64::from(i) / f64::from(n) * std::f64::consts::TAU,
                f64::from(i + 1) / f64::from(n) * std::f64::consts::TAU,
            );
            self.line(
                (p.0 + r * a0.cos(), p.1 + r * a0.sin()),
                (p.0 + r * a1.cos(), p.1 + r * a1.sin()),
                c,
                t,
            );
        }
    }

    /// An arrow from `a` to `b` with a head at `b`.
    pub fn arrow(&mut self, a: (f64, f64), b: (f64, f64), c: Rgb, t: f64) {
        self.line(a, b, c, t);
        let ang = (b.1 - a.1).atan2(b.0 - a.0);
        for s in [-0.45_f64, 0.45] {
            let (hx, hy) = (b.0 - 14.0 * (ang + s).cos(), b.1 - 14.0 * (ang + s).sin());
            self.line(b, (hx, hy), c, t);
        }
    }

    /// Draw `text` with its top-left at `(x, y)`; `scale` multiplies the 5 x 7 pixel font.
    pub fn text(&mut self, x: i32, y: i32, text: &str, scale: u32, c: Rgb) {
        let s = scale as i32;
        for (i, ch) in text.chars().enumerate() {
            let g = glyph(ch);
            let ox = x + (i as i32) * (CELL_W as i32) * s;
            for (col, bits) in g.iter().enumerate() {
                for row in 0..7 {
                    if bits >> row & 1 == 1 {
                        self.fill_rect(
                            Rect {
                                x: ox + (col as i32) * s,
                                y: y + row * s,
                                w: s,
                                h: s,
                            },
                            c,
                        );
                    }
                }
            }
        }
    }

    /// Text on a dark box so it reads over a picture.
    pub fn label(&mut self, x: i32, y: i32, text: &str, scale: u32, c: Rgb) {
        let w = text.chars().count() as i32 * CELL_W as i32 * scale as i32 + 6;
        let h = CELL_H as i32 * scale as i32 + 2;
        self.shade_rect(
            Rect {
                x: x - 3,
                y: y - 2,
                w,
                h,
            },
            [10, 12, 18],
            0.78,
        );
        self.text(x, y, text, scale, c);
    }

    /// Copy `src` into this image at `(x, y)`.
    pub fn paste(&mut self, src: &Image, x: i32, y: i32) {
        for sy in 0..src.h as i32 {
            for sx in 0..src.w as i32 {
                if let Some(c) = src.get(sx, sy) {
                    self.set(x + sx, y + sy, c);
                }
            }
        }
    }

    /// Wrap `text` to `width_chars` characters per line.
    #[must_use]
    pub fn wrap(text: &str, width_chars: usize) -> Vec<String> {
        let mut lines = Vec::new();
        let mut cur = String::new();
        for word in text.split_whitespace() {
            if !cur.is_empty() && cur.len() + 1 + word.len() > width_chars {
                lines.push(std::mem::take(&mut cur));
            }
            if !cur.is_empty() {
                cur.push(' ');
            }
            cur.push_str(word);
        }
        if !cur.is_empty() {
            lines.push(cur);
        }
        lines
    }
}

type V3 = [f64; 3];

fn sub(a: V3, b: V3) -> V3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn dot(a: V3, b: V3) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn cross(a: V3, b: V3) -> V3 {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn unit(a: V3) -> V3 {
    let l = dot(a, a).sqrt();
    if l > 0.0 {
        [a[0] / l, a[1] / l, a[2] / l]
    } else {
        [0.0, 0.0, 1.0]
    }
}

/// Where the camera looks.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct View {
    /// Direction the camera looks along (from the camera into the scene).
    pub dir: V3,
    /// A direction that points up on the screen, roughly.
    pub up: V3,
    /// The world point at the middle of the panel.
    pub centre: V3,
    /// World mm covered by half the panel's height.
    pub half_height: f64,
}

/// An orthographic camera for one panel.
#[derive(Debug, Clone, Copy)]
pub struct Cam {
    right: V3,
    up: V3,
    dir: V3,
    centre: V3,
    scale: f64,
    ox: f64,
    oy: f64,
}

impl Cam {
    /// A camera for `view` on a panel of `w` x `h` pixels.
    #[must_use]
    pub fn new(view: &View, w: f64, h: f64) -> Self {
        let dir = unit(view.dir);
        let right = unit(cross(dir, view.up));
        let up = cross(right, dir);
        Self {
            right,
            up,
            dir,
            centre: view.centre,
            scale: 0.5 * h / view.half_height,
            ox: 0.5 * w,
            oy: 0.5 * h,
        }
    }

    /// Screen position (x right, y down) and depth (bigger is farther) of a world point.
    #[must_use]
    pub fn project(&self, p: V3) -> (f64, f64, f64) {
        let d = sub(p, self.centre);
        (
            self.ox + dot(d, self.right) * self.scale,
            self.oy - dot(d, self.up) * self.scale,
            dot(d, self.dir),
        )
    }

    /// The pixel length of `mm` millimetres.
    #[must_use]
    pub fn px(&self, mm: f64) -> f64 {
        mm * self.scale
    }

    /// The view direction.
    #[must_use]
    pub fn dir(&self) -> V3 {
        self.dir
    }
}

/// A z-buffered colour layer for one panel.
#[derive(Debug)]
pub struct Raster {
    w: usize,
    h: usize,
    depth: Vec<f32>,
    colour: Vec<Rgb>,
    id: Vec<u32>,
}

impl Raster {
    /// An empty raster (background shows through where nothing is drawn).
    #[must_use]
    pub fn new(w: usize, h: usize) -> Self {
        Self {
            w,
            h,
            depth: vec![f32::INFINITY; w * h],
            colour: vec![[0; 3]; w * h],
            id: vec![0; w * h],
        }
    }

    /// Draw one triangle given in screen space with depth; `id` tells objects apart for the
    /// outline pass (0 is reserved for empty).
    pub fn triangle(&mut self, p: [(f64, f64, f64); 3], colour: Rgb, id: u32) {
        let min_x = p
            .iter()
            .map(|q| q.0)
            .fold(f64::INFINITY, f64::min)
            .floor()
            .max(0.0) as usize;
        let max_x = (p
            .iter()
            .map(|q| q.0)
            .fold(f64::NEG_INFINITY, f64::max)
            .ceil()
            .max(0.0) as usize)
            .min(self.w.saturating_sub(1));
        let min_y = p
            .iter()
            .map(|q| q.1)
            .fold(f64::INFINITY, f64::min)
            .floor()
            .max(0.0) as usize;
        let max_y = (p
            .iter()
            .map(|q| q.1)
            .fold(f64::NEG_INFINITY, f64::max)
            .ceil()
            .max(0.0) as usize)
            .min(self.h.saturating_sub(1));
        let area = (p[1].0 - p[0].0) * (p[2].1 - p[0].1) - (p[1].1 - p[0].1) * (p[2].0 - p[0].0);
        if area.abs() < 1e-12 || self.w == 0 || self.h == 0 {
            return;
        }
        for y in min_y..=max_y {
            for x in min_x..=max_x {
                let (fx, fy) = (x as f64 + 0.5, y as f64 + 0.5);
                let w0 = ((p[1].0 - fx) * (p[2].1 - fy) - (p[1].1 - fy) * (p[2].0 - fx)) / area;
                let w1 = ((p[2].0 - fx) * (p[0].1 - fy) - (p[2].1 - fy) * (p[0].0 - fx)) / area;
                let w2 = 1.0 - w0 - w1;
                if w0 < -1e-9 || w1 < -1e-9 || w2 < -1e-9 {
                    continue;
                }
                let z = (w0 * p[0].2 + w1 * p[1].2 + w2 * p[2].2) as f32;
                let i = y * self.w + x;
                if z < self.depth[i] {
                    self.depth[i] = z;
                    self.colour[i] = colour;
                    self.id[i] = id;
                }
            }
        }
    }

    /// Darken pixels at object borders and at depth jumps larger than `jump`.
    pub fn outline(&mut self, jump: f32) {
        let edge = |i: usize, j: usize, s: &Self| -> bool {
            s.id[i] != s.id[j]
                || (s.depth[i].is_finite()
                    && s.depth[j].is_finite()
                    && (s.depth[i] - s.depth[j]).abs() > jump)
        };
        let mut dark = vec![false; self.w * self.h];
        for y in 0..self.h {
            for x in 0..self.w {
                let i = y * self.w + x;
                if self.id[i] == 0 {
                    continue;
                }
                let right = x + 1 < self.w && edge(i, i + 1, self);
                let down = y + 1 < self.h && edge(i, i + self.w, self);
                dark[i] = right || down;
            }
        }
        for (c, d) in self.colour.iter_mut().zip(dark) {
            if d {
                *c = [(c[0] / 4), (c[1] / 4), (c[2] / 4)];
            }
        }
    }

    /// Composite onto `img` at `(x, y)`, shrinking by `ss` in each direction (box filter).
    pub fn resolve(&self, img: &mut Image, x: i32, y: i32, ss: usize) {
        let (ow, oh) = (self.w / ss, self.h / ss);
        for py in 0..oh {
            for px in 0..ow {
                let (mut acc, mut n) = ([0_u32; 3], 0_u32);
                for sy in 0..ss {
                    for sx in 0..ss {
                        let i = (py * ss + sy) * self.w + px * ss + sx;
                        if self.id[i] != 0 {
                            for (a, &v) in acc.iter_mut().zip(&self.colour[i]) {
                                *a += u32::from(v);
                            }
                            n += 1;
                        }
                    }
                }
                if n > 0 {
                    let cover = n as f32 / (ss * ss) as f32;
                    let c = [(acc[0] / n) as u8, (acc[1] / n) as u8, (acc[2] / n) as u8];
                    img.blend(x + px as i32, y + py as i32, c, cover);
                }
            }
        }
    }
}

/// Flat shading of a triangle with unit normal `n` seen along `dir`: ambient plus a light
/// from the viewer, two-sided.
#[must_use]
pub fn shade(base: Rgb, n: V3, dir: V3) -> Rgb {
    let k = 0.30 + 0.70 * dot(unit(n), unit(dir)).abs().powf(0.8);
    [
        (f64::from(base[0]) * k).round().min(255.0) as u8,
        (f64::from(base[1]) * k).round().min(255.0) as u8,
        (f64::from(base[2]) * k).round().min(255.0) as u8,
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shapes_land_where_asked() {
        let mut im = Image::new(40, 30, [0, 0, 0]);
        im.fill_rect(
            Rect {
                x: 2,
                y: 3,
                w: 4,
                h: 5,
            },
            [9, 9, 9],
        );
        assert_eq!(im.get(2, 3), Some([9, 9, 9]));
        assert_eq!(im.get(6, 3), Some([0, 0, 0]));
        im.disc((20.0, 15.0), 3.0, [1, 2, 3]);
        assert_eq!(im.get(20, 15), Some([1, 2, 3]));
        assert_eq!(im.get(30, 15), Some([0, 0, 0]));
        im.text(0, 20, "A", 1, [255, 255, 255]);
        // The left column of an A is the 0x7C pattern: rows 2..=6 lit.
        assert_eq!(im.get(0, 22), Some([255, 255, 255]));
        assert_eq!(im.get(0, 20), Some([0, 0, 0]));
        assert!(im.get(-1, 0).is_none());
    }

    #[test]
    fn a_camera_looking_along_y_sees_x_to_the_right_and_z_up() {
        let v = View {
            dir: [0.0, 1.0, 0.0],
            up: [0.0, 0.0, 1.0],
            centre: [0.0, 0.0, 0.0],
            half_height: 10.0,
        };
        let c = Cam::new(&v, 200.0, 100.0);
        let (x, y, d) = c.project([5.0, 3.0, 2.0]);
        assert!(x > 100.0 && y < 50.0 && d > 0.0);
        assert!((c.px(1.0) - 5.0).abs() < 1e-9);
    }

    #[test]
    fn the_nearer_triangle_wins_and_outlines_mark_the_border() {
        let mut r = Raster::new(40, 40);
        let tri = |z: f64| [(2.0, 2.0, z), (38.0, 2.0, z), (2.0, 38.0, z)];
        r.triangle(tri(5.0), [200, 0, 0], 1);
        r.triangle(tri(2.0), [0, 200, 0], 2);
        r.triangle(tri(9.0), [0, 0, 200], 3);
        assert_eq!(r.colour[10 * 40 + 10], [0, 200, 0]);
        r.outline(0.5);
        let mut im = Image::new(40, 40, [0, 0, 0]);
        r.resolve(&mut im, 0, 0, 1);
        assert_eq!(im.get(5, 5), Some([0, 200, 0]));
        // Pixels on the hypotenuse have a different neighbour and are darker.
        assert!(
            im.get(20, 20).is_some_and(|c| c[1] < 200)
                || im.get(19, 20).is_some_and(|c| c[1] < 200)
        );
    }

    #[test]
    fn wrap_breaks_on_words() {
        let l = Image::wrap("one two three four", 9);
        assert_eq!(l, vec!["one two", "three", "four"]);
    }
}
