//! The twelve review pictures of design `step11-construction.md` section 9, drawn from the
//! written file's scene and the report data. Every picture has a title, labels on the image,
//! a legend and a caption on the image, so it explains itself.

use plinth_support::load::needs::SiteTag;
use plinth_support::load::{LoadWarning, SiteClass, WarningClass};
use plinth_support::report_data::{LoadData, ReportData};

use crate::canvas::{BACKGROUND, Image, Rect, Rgb, TEXT, View};
use crate::scene::{
    GREEN, Look, MODEL, PATCH, Part, RAFT, Role, SUPPORT, Scene, Tri, WARN, area, centroid,
    draw_scene, normal, plain_model, plain_part,
};

type V3 = [f64; 3];

/// One finished picture.
#[derive(Debug, Clone, PartialEq)]
pub struct Picture {
    /// File name stem, for example `01-before-after`.
    pub name: String,
    /// The title written on the image.
    pub title: String,
    /// The caption written on the image.
    pub caption: String,
    /// The pixels.
    pub image: Image,
}

const W: u32 = 1500;
const H: u32 = 980;
const CLASS1: Rgb = [60, 200, 200];
const CLASS2: Rgb = [220, 90, 220];
const GREY: Rgb = [150, 155, 165];

struct Ctx<'a> {
    scene: &'a Scene,
    data: &'a ReportData,
    ld: Option<&'a LoadData>,
    under: V3,
}

fn sub(a: V3, b: V3) -> V3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn len(a: V3) -> f64 {
    (a[0] * a[0] + a[1] * a[1] + a[2] * a[2]).sqrt()
}

fn dist(a: V3, b: V3) -> f64 {
    len(sub(a, b))
}

/// A page with a title bar, a caption area and a legend; returns it and the content box.
fn page(title: &str, caption: &str, legend: &[(Rgb, &str)]) -> (Image, Rect) {
    let mut img = Image::new(W, H, BACKGROUND);
    img.fill_rect(
        Rect {
            x: 0,
            y: 0,
            w: W as i32,
            h: 44,
        },
        [24, 27, 34],
    );
    img.text(16, 10, title, 3, TEXT);
    let lines = Image::wrap(caption, 118);
    let cap_h = 14 + 22 * lines.len() as i32;
    let top = H as i32 - cap_h - 34;
    img.fill_rect(
        Rect {
            x: 0,
            y: top,
            w: W as i32,
            h: H as i32 - top,
        },
        [24, 27, 34],
    );
    for (i, l) in lines.iter().enumerate() {
        img.text(16, top + 8 + 22 * i as i32, l, 2, TEXT);
    }
    let mut x = 16;
    for (c, name) in legend {
        img.fill_rect(
            Rect {
                x,
                y: H as i32 - 26,
                w: 16,
                h: 16,
            },
            *c,
        );
        img.text(x + 22, H as i32 - 25, name, 2, TEXT);
        x += 22 + 12 * name.len() as i32 + 26;
    }
    (
        img,
        Rect {
            x: 8,
            y: 52,
            w: W as i32 - 16,
            h: top - 60,
        },
    )
}

fn split(r: Rect, n: i32) -> Vec<Rect> {
    let w = (r.w - 8 * (n - 1)) / n;
    (0..n)
        .map(|i| Rect {
            x: r.x + i * (w + 8),
            w,
            ..r
        })
        .collect()
}

fn frame_it(img: &mut Image, r: Rect) {
    img.frame(r, [70, 76, 90], 2);
}

fn finish(name: &str, title: &str, caption: &str, image: Image) -> Picture {
    Picture {
        name: name.to_owned(),
        title: title.to_owned(),
        caption: caption.to_owned(),
        image,
    }
}

/// A view along `dir` that fits the whole scene into a panel of the given shape.
fn fit(scene: &Scene, dir: V3, up: V3, rect: Rect) -> View {
    let (lo, hi) = scene.bounds();
    let centre = [
        0.5 * (lo[0] + hi[0]),
        0.5 * (lo[1] + hi[1]),
        0.5 * (lo[2] + hi[2]),
    ];
    let d = {
        let l = len(dir);
        [dir[0] / l, dir[1] / l, dir[2] / l]
    };
    let r = {
        let c = [
            d[1] * up[2] - d[2] * up[1],
            d[2] * up[0] - d[0] * up[2],
            d[0] * up[1] - d[1] * up[0],
        ];
        let l = len(c).max(1e-12);
        [c[0] / l, c[1] / l, c[2] / l]
    };
    let u = [
        r[1] * d[2] - r[2] * d[1],
        r[2] * d[0] - r[0] * d[2],
        r[0] * d[1] - r[1] * d[0],
    ];
    let (mut er, mut eu) = (0.0_f64, 0.0_f64);
    for i in 0..8 {
        let p = [
            if i & 1 == 0 { lo[0] } else { hi[0] },
            if i & 2 == 0 { lo[1] } else { hi[1] },
            if i & 4 == 0 { lo[2] } else { hi[2] },
        ];
        let q = sub(p, centre);
        er = er.max((q[0] * r[0] + q[1] * r[1] + q[2] * r[2]).abs());
        eu = eu.max((q[0] * u[0] + q[1] * u[1] + q[2] * u[2]).abs());
    }
    let aspect = f64::from(rect.w) / f64::from(rect.h);
    View {
        dir,
        up,
        centre,
        half_height: (eu * 1.12).max(er * 1.12 / aspect).max(1.0),
    }
}

fn closeup(c: &Ctx<'_>, dir: V3, centre: V3, half: f64) -> View {
    let _ = c;
    View {
        dir,
        up: [0.0, 0.0, 1.0],
        centre,
        half_height: half,
    }
}

fn under_dir(c: &Ctx<'_>) -> V3 {
    [-c.under[0], -c.under[1], -c.under[2]]
}

fn side_dir(c: &Ctx<'_>) -> V3 {
    // Along the axis most perpendicular to the underside normal's tilt: the plane of the tilt
    // is the one that contains the normal and z, so look across it.
    let (nx, ny) = (c.under[0], c.under[1]);
    let l = nx.hypot(ny);
    if l < 1e-9 {
        [-1.0, 0.0, 0.0]
    } else {
        [-ny / l, nx / l, 0.0]
    }
}

fn contacts_of<'a>(c: &Ctx<'a>) -> &'a [plinth_support::load::ContactView] {
    c.ld.map_or(&[][..], |l| l.layout.contacts.as_slice())
}

fn class_colour(k: SiteClass) -> Rgb {
    match k {
        SiteClass::HiddenUnderside => CLASS1,
        SiteClass::PartlyVisibleRim => CLASS2,
    }
}

fn base_look<'a>() -> Look<'a> {
    Look {
        model: true,
        parts: true,
        model_colour: &plain_model,
        part_colour: &plain_part,
        roles: &[],
    }
}

/// Only the tips (ball, cone, joint): the columns would hide the underside in a view along it.
fn tips_look<'a>() -> Look<'a> {
    Look {
        roles: &[Role::TipBall, Role::TipCone, Role::Joint],
        ..base_look()
    }
}

/// The look of panel `i` of a two-panel row: tips only in the underside view (the first),
/// everything in the side view.
fn look_for<'a>(i: usize) -> Look<'a> {
    if i == 0 { tips_look() } else { base_look() }
}

/// Number the contacts of `idx` on a panel; `label` gives extra text.
fn mark(
    img: &mut Image,
    rect: Rect,
    cam: &crate::canvas::Cam,
    c: &Ctx<'_>,
    idx: &[usize],
    extra: &dyn Fn(usize) -> String,
) {
    let cs = contacts_of(c);
    for &i in idx {
        let (x, y, _) = cam.project(cs[i].centre);
        let (px, py) = (f64::from(rect.x) + x, f64::from(rect.y) + y);
        if px < f64::from(rect.x)
            || py < f64::from(rect.y)
            || px > f64::from(rect.x + rect.w)
            || py > f64::from(rect.y + rect.h)
        {
            continue;
        }
        img.ring((px, py), 9.0, class_colour(cs[i].site_class), 2.0);
        let text = format!("{i}{}", extra(i));
        img.label(px as i32 + 11, py as i32 - 16, &text, 1, TEXT);
    }
}

fn unverified_page(name: &str, title: &str, why: &str) -> Picture {
    let (mut img, r) = page(
        title,
        &format!("Unverified: {why}. Nothing is drawn rather than a guess."),
        &[],
    );
    img.label(r.x + 20, r.y + 20, "UNVERIFIED", 3, WARN);
    finish(name, title, why, img)
}

/// All twelve pictures. Pictures that need load data say so when `data` has none.
#[must_use]
pub fn render_pictures(scene: &Scene, data: &ReportData) -> Vec<Picture> {
    let c = Ctx {
        scene,
        data,
        ld: data.load_data.as_ref(),
        under: scene.underside_normal(),
    };
    vec![
        p01_before_after(&c),
        p02_underside(&c),
        p03_bottom_cluster(&c),
        p04_rib_island(&c),
        p05_lip_band(&c),
        p06_worst_contact(&c),
        p07_worst_load(&c),
        p08_islands(&c),
        p09_raft(&c),
        p10_need(&c),
        p11_warnings(&c),
        p12_rim(&c),
    ]
}

fn p01_before_after(c: &Ctx<'_>) -> Picture {
    let tilt = (-c.under[2]).clamp(-1.0, 1.0).acos().to_degrees();
    let lift = c.scene.lowest_model_point()[2];
    let cap = format!(
        "Same camera, from the side. Left: the approved base, tilted {tilt:.0} degrees from flat, lifted {lift:.1} mm above the plate. Right: the written file with its supports and raft. The underside (lip and rings) faces the plate side and stays hidden when the base stands on a table."
    );
    let (mut img, r) = page(
        "1  Before and after, same camera",
        &cap,
        &[
            (MODEL, "model"),
            (SUPPORT, "supports"),
            (RAFT, "raft"),
            (PATCH, "contact patch"),
        ],
    );
    let panels = split(r, 2);
    let view = fit(c.scene, side_dir(c), [0.0, 0.0, 1.0], panels[0]);
    let none = Look {
        parts: false,
        ..base_look()
    };
    for (i, p) in panels.iter().enumerate() {
        frame_it(&mut img, *p);
        let both = base_look();
        let cam = draw_scene(
            &mut img,
            *p,
            &view,
            c.scene,
            if i == 0 { &none } else { &both },
        );
        let (lo, hi) = c.scene.bounds();
        let a = cam.project([0.5 * (lo[0] + hi[0]), lo[1] - 4.0, 0.0]);
        let b = cam.project([0.5 * (lo[0] + hi[0]), hi[1] + 4.0, 0.0]);
        img.line(
            (f64::from(p.x) + a.0, f64::from(p.y) + a.1),
            (f64::from(p.x) + b.0, f64::from(p.y) + b.1),
            [200, 200, 210],
            2.0,
        );
        img.label(
            p.x + 8,
            p.y + 6,
            if i == 0 {
                "BEFORE: model on its lift"
            } else {
                "AFTER: supports and raft"
            },
            2,
            TEXT,
        );
    }
    // Labels on the left panel.
    let cam0 = crate::canvas::Cam::new(&view, f64::from(panels[0].w), f64::from(panels[0].h));
    let low = c.scene.lowest_model_point();
    let (lx, ly, _) = cam0.project(low);
    let p0 = panels[0];
    img.label(
        p0.x + lx as i32 - 40,
        p0.y + ly as i32 + 30,
        "low edge",
        2,
        WARN,
    );
    img.arrow(
        (f64::from(p0.x) + lx - 10.0, f64::from(p0.y) + ly + 28.0),
        (f64::from(p0.x) + lx, f64::from(p0.y) + ly + 3.0),
        WARN,
        2.0,
    );
    let (ox, oy, _) = cam0.project([0.0, low[1], 0.0]);
    img.arrow(
        (f64::from(p0.x) + ox + 60.0, f64::from(p0.y) + oy),
        (f64::from(p0.x) + ox + 60.0, f64::from(p0.y) + ly),
        [255, 255, 255],
        2.0,
    );
    img.label(
        p0.x + ox as i32 + 70,
        p0.y + ((oy + ly) * 0.5) as i32,
        &format!("lift {lift:.1} mm"),
        2,
        TEXT,
    );
    let (lo, hi) = c.scene.bounds();
    let mid = [
        0.5 * (lo[0] + hi[0]),
        0.5 * (lo[1] + hi[1]),
        0.5 * (lo[2] + hi[2]),
    ];
    let under_pt = [mid[0], mid[1] + 6.0 * c.under[1], mid[2] + 6.0 * c.under[2]];
    let top_pt = [mid[0], mid[1] - 6.0 * c.under[1], mid[2] - 6.0 * c.under[2]];
    for (pt, text) in [(under_pt, "hidden underside"), (top_pt, "top")] {
        let (x, y, _) = cam0.project(pt);
        img.label(p0.x + x as i32, p0.y + y as i32, text, 2, TEXT);
    }
    let (ax, ay, _) = cam0.project([mid[0], mid[1], mid[2]]);
    img.label(
        p0.x + ax as i32 - 30,
        p0.y + ay as i32 - 60,
        &format!("{tilt:.0} deg from flat"),
        2,
        GREY,
    );
    finish("01-before-after", "Before and after", &cap, img)
}

fn p02_underside(c: &Ctx<'_>) -> Picture {
    let cs = contacts_of(c);
    let mut order: Vec<usize> = (0..cs.len()).collect();
    order.sort_by(|&a, &b| cs[b].patch_lower_mm2.total_cmp(&cs[a].patch_lower_mm2));
    let top5: Vec<usize> = order.iter().copied().take(5).collect();
    let (mut best, mut pair) = (f64::INFINITY, None);
    for i in 0..cs.len() {
        for j in i + 1..cs.len() {
            let d = dist(cs[i].centre, cs[j].centre);
            if d < best {
                best = d;
                pair = Some((i, j));
            }
        }
    }
    let n1 = cs
        .iter()
        .filter(|k| k.site_class == SiteClass::HiddenUnderside)
        .count();
    let cap = format!(
        "The underside seen from outside, along the underside normal. {} contacts: {n1} on the hidden underside (class 1), {} on the partly visible rim (class 2). The 5 largest patch lower bounds are labelled in mm2; the closest pair of contacts is joined by a line with its distance.",
        cs.len(),
        cs.len() - n1
    );
    let (mut img, r) = page(
        "2  Underside with contacts",
        &cap,
        &[
            (CLASS1, "class 1 hidden"),
            (CLASS2, "class 2 rim"),
            (WARN, "closest pair"),
        ],
    );
    frame_it(&mut img, r);
    let view = fit(c.scene, under_dir(c), [0.0, 0.0, 1.0], r);
    let cam = draw_scene(&mut img, r, &view, c.scene, &look_for(0));
    let all: Vec<usize> = (0..cs.len()).collect();
    mark(&mut img, r, &cam, c, &all, &|i| {
        if top5.contains(&i) {
            format!(" {:.2} mm2", cs[i].patch_lower_mm2)
        } else {
            String::new()
        }
    });
    if let Some((i, j)) = pair {
        let (a, b) = (cam.project(cs[i].centre), cam.project(cs[j].centre));
        let (ra, rb) = (
            (f64::from(r.x) + a.0, f64::from(r.y) + a.1),
            (f64::from(r.x) + b.0, f64::from(r.y) + b.1),
        );
        img.line(ra, rb, WARN, 2.0);
        img.label(
            ((ra.0 + rb.0) * 0.5) as i32,
            ((ra.1 + rb.1) * 0.5) as i32 + 12,
            &format!("closest {best:.2} mm"),
            1,
            WARN,
        );
    }
    if cs.is_empty() {
        img.label(r.x + 20, r.y + 20, "no contacts in the layout", 2, WARN);
    }
    finish(
        "02-underside-contacts",
        "Underside with contacts",
        &cap,
        img,
    )
}

fn closeup_page(
    c: &Ctx<'_>,
    name: &str,
    title: &str,
    centre: V3,
    half: f64,
    cap_lead: &str,
) -> Picture {
    let cs = contacts_of(c);
    let near: Vec<usize> = (0..cs.len())
        .filter(|&i| dist(cs[i].centre, centre) <= half * 1.4)
        .collect();
    let largest = near
        .iter()
        .map(|&i| cs[i].patch_lower_mm2)
        .fold(0.0, f64::max);
    let cap = format!(
        "{cap_lead} {} contacts in view; the largest patch lower bound in view is {largest:.2} mm2. Head angles are not part of the layout data, so they are not stated here (unverified).",
        near.len()
    );
    let (mut img, r) = page(
        title,
        &cap,
        &[(CLASS1, "class 1 hidden"), (CLASS2, "class 2 rim")],
    );
    let two = split(r, 2);
    for (i, p) in two.iter().enumerate() {
        frame_it(&mut img, *p);
        let dir = if i == 0 { under_dir(c) } else { side_dir(c) };
        let cam = draw_scene(
            &mut img,
            *p,
            &closeup(c, dir, centre, half),
            c.scene,
            &look_for(i),
        );
        mark(&mut img, *p, &cam, c, &near, &|k| {
            format!(" {:.2}", cs[k].patch_lower_mm2)
        });
        img.label(
            p.x + 8,
            p.y + 6,
            if i == 0 {
                "underside view"
            } else {
                "side view"
            },
            2,
            TEXT,
        );
    }
    finish(name, title, &cap, img)
}

fn p03_bottom_cluster(c: &Ctx<'_>) -> Picture {
    let low = c.scene.lowest_model_point();
    closeup_page(
        c,
        "03-bottom-cluster",
        "Bottom cluster at the lip edge",
        low,
        4.0,
        "The lowest point of the lip edge, where the first island starts, and the contacts around it.",
    )
}

fn island_centre(c: &Ctx<'_>, k: usize) -> Option<V3> {
    let ld = c.ld?;
    let isl = ld.structure.islands.get(k)?;
    let z = c.data.process.lift_mm
        + (f64::from(isl.first_layer) + 0.5) * c.data.process.layer_height_mm;
    Some([isl.first_xy[0], isl.first_xy[1], z])
}

fn p04_rib_island(c: &Ctx<'_>) -> Picture {
    match island_centre(c, 1) {
        Some(p) => closeup_page(
            c,
            "04-rib-island",
            "Lowest point of the rib (island 2)",
            p,
            3.0,
            "The lowest point of the central rib, where the second island starts, and the contacts around it.",
        ),
        None => unverified_page(
            "04-rib-island",
            "Lowest point of the rib (island 2)",
            "the layout has no second island or no load data",
        ),
    }
}

/// The centre of the largest cluster of sub-45 degree samples that are below 30 degrees.
fn lip_band_centre(ld: &LoadData) -> Option<V3> {
    let mut bins: Vec<((i64, i64, i64), f64, V3)> = Vec::new();
    for p in ld.needs.points.iter().filter(|p| p.angle_deg < 30.0) {
        let k = (
            (p.xyz[0] / 3.0).floor() as i64,
            (p.xyz[1] / 3.0).floor() as i64,
            (p.xyz[2] / 3.0).floor() as i64,
        );
        match bins.iter_mut().find(|b| b.0 == k) {
            Some(b) => {
                b.1 += p.area_mm2;
                for a in 0..3 {
                    b.2[a] += p.xyz[a] * p.area_mm2;
                }
            }
            None => bins.push((
                k,
                p.area_mm2,
                [
                    p.xyz[0] * p.area_mm2,
                    p.xyz[1] * p.area_mm2,
                    p.xyz[2] * p.area_mm2,
                ],
            )),
        }
    }
    let b = bins.into_iter().max_by(|a, b| a.1.total_cmp(&b.1))?;
    Some([b.2[0] / b.1, b.2[1] / b.1, b.2[2] / b.1])
}

fn p05_lip_band(c: &Ctx<'_>) -> Picture {
    match c.ld.and_then(lip_band_centre) {
        Some(p) => closeup_page(
            c,
            "05-lip-band",
            "Largest flat band and its contacts",
            p,
            5.0,
            "The largest region of down-facing faces flatter than 30 degrees (the inner lip top band on Round 25) and the contacts that cover it.",
        ),
        None => unverified_page(
            "05-lip-band",
            "Largest flat band and its contacts",
            "no overhang samples were given",
        ),
    }
}

fn p06_worst_contact(c: &Ctx<'_>) -> Picture {
    let Some(ld) = c.ld else {
        return unverified_page(
            "06-worst-contact",
            "Worst contact",
            "no load data was given",
        );
    };
    let Some(w) = ld
        .detail
        .contacts
        .iter()
        .max_by(|a, b| a.ratio.total_cmp(&b.ratio))
    else {
        return unverified_page(
            "06-worst-contact",
            "Worst contact",
            "the layout has no contact",
        );
    };
    let cv = &ld.layout.contacts[w.contact as usize];
    let patch_r = (cv.patch_lower_mm2 / std::f64::consts::PI).sqrt();
    let cap = format!(
        "Contact {} has the largest tributary ratio, {:.1}, in layer {}. Patch lower bound {:.3} mm2 (drawn as a disc of the same area; the checker's area interval is not part of this data, unverified). The ring is the 3.0 mm contact-surrounding region. The smallest gap outside the regions is {} mm.",
        w.contact,
        w.ratio,
        w.worst_layer,
        cv.patch_lower_mm2,
        if c.data.supports.smallest_gap_mm > 0.0 {
            format!("{:.3}", c.data.supports.smallest_gap_mm)
        } else {
            "not measured here (unverified)".to_owned()
        }
    );
    let (mut img, r) = page(
        "6  Worst contact",
        &cap,
        &[(PATCH, "patch (same area)"), (WARN, "3.0 mm region")],
    );
    let two = split(r, 2);
    for (i, p) in two.iter().enumerate() {
        frame_it(&mut img, *p);
        let dir = if i == 0 { under_dir(c) } else { side_dir(c) };
        let cam = draw_scene(
            &mut img,
            *p,
            &closeup(c, dir, cv.centre, 3.6),
            c.scene,
            &look_for(i),
        );
        let (x, y, _) = cam.project(cv.centre);
        let (cx, cy) = (f64::from(p.x) + x, f64::from(p.y) + y);
        img.ring((cx, cy), cam.px(3.0), WARN, 2.0);
        img.ring((cx, cy), cam.px(patch_r).max(3.0), PATCH, 3.0);
        img.label(
            p.x + 8,
            p.y + 6,
            if i == 0 {
                "underside view"
            } else {
                "side view"
            },
            2,
            TEXT,
        );
    }
    finish("06-worst-contact", "Worst contact", &cap, img)
}

/// A distinct colour for contact `i`.
fn palette(i: u32) -> Rgb {
    const P: [Rgb; 8] = [
        [230, 100, 90],
        [90, 170, 230],
        [120, 210, 110],
        [235, 200, 70],
        [200, 120, 220],
        [90, 220, 200],
        [240, 150, 80],
        [170, 170, 240],
    ];
    P[(i % 8) as usize]
}

fn ratio_colour(r: f64) -> Rgb {
    if r > 12.0 {
        PATCH
    } else if r >= 8.0 {
        [235, 205, 60]
    } else {
        GREEN
    }
}

fn p07_worst_load(c: &Ctx<'_>) -> Picture {
    let Some(ld) = c.ld else {
        return unverified_page("07-worst-load", "Worst load", "no load data was given");
    };
    let peak = ld.detail.peak;
    let cap = format!(
        "(a) Peel area of each layer against plate height, with the design ratio times the active section capacity; the worst layer is marked. (b) The peeling layer {}: each square is a source area element, coloured by the contact it goes to; the label is assigned area / section = ratio (sizing heuristic, not a load bound). (c) Side view, supports coloured by their largest tributary ratio.",
        peak.map_or_else(
            || "(none)".to_owned(),
            |p| format!(
                "{} at {:.2} mm, {:.2} mm2",
                p.layer,
                c.data.process.lift_mm + f64::from(p.layer) * c.data.process.layer_height_mm,
                p.area_mm2
            )
        )
    );
    let (mut img, r) = page(
        "7  Worst load",
        &cap,
        &[
            (GREEN, "ratio < 8"),
            ([235, 205, 60], "8 to 12"),
            (PATCH, "over 12"),
        ],
    );
    let a = Rect {
        w: r.w / 2 - 4,
        h: r.h / 2 - 4,
        ..r
    };
    let b = Rect {
        x: r.x + r.w / 2 + 4,
        w: r.w / 2 - 4,
        ..r
    };
    let cc = Rect {
        y: r.y + r.h / 2 + 4,
        w: r.w / 2 - 4,
        h: r.h / 2 - 4,
        ..r
    };
    for p in [a, b, cc] {
        frame_it(&mut img, p);
    }
    chart(&mut img, a, c, ld);
    peak_plan(&mut img, b, c, ld);
    let by_contact = |p: &Part| {
        p.contact
            .and_then(|i| ld.detail.contacts.get(i as usize))
            .map_or(GREY, |k| ratio_colour(k.ratio))
    };
    let look = Look {
        part_colour: &by_contact,
        model_colour: &|_, _| [90, 100, 118],
        ..base_look()
    };
    draw_scene(
        &mut img,
        cc,
        &fit(c.scene, side_dir(c), [0.0, 0.0, 1.0], cc),
        c.scene,
        &look,
    );
    img.label(cc.x + 8, cc.y + 6, "(c) side view by ratio", 2, TEXT);
    finish("07-worst-load", "Worst load", &cap, img)
}

fn chart(img: &mut Image, r: Rect, c: &Ctx<'_>, ld: &LoadData) {
    let layers = &ld.detail.report.layers;
    img.label(
        r.x + 8,
        r.y + 6,
        "(a) peel area and capacity by plate height",
        2,
        TEXT,
    );
    if layers.is_empty() {
        return;
    }
    let ymax = layers
        .iter()
        .map(|l| l.area_mm2.max(l.capacity_mm2))
        .fold(1.0, f64::max)
        * 1.08;
    let (x0, x1, y0, y1) = (r.x + 60, r.x + r.w - 20, r.y + r.h - 40, r.y + 44);
    let px = |k: usize| {
        f64::from(x0) + (f64::from(x1 - x0)) * k as f64 / (layers.len().max(2) - 1) as f64
    };
    let py = |v: f64| f64::from(y0) - f64::from(y0 - y1) * (v / ymax).min(1.0);
    img.line(
        (f64::from(x0), f64::from(y0)),
        (f64::from(x1), f64::from(y0)),
        GREY,
        1.0,
    );
    img.line(
        (f64::from(x0), f64::from(y0)),
        (f64::from(x0), f64::from(y1)),
        GREY,
        1.0,
    );
    for k in 1..layers.len() {
        img.line(
            (px(k - 1), py(layers[k - 1].capacity_mm2)),
            (px(k), py(layers[k].capacity_mm2)),
            GREEN,
            1.5,
        );
        img.line(
            (px(k - 1), py(layers[k - 1].area_mm2)),
            (px(k), py(layers[k].area_mm2)),
            PATCH,
            2.0,
        );
    }
    if let Some(p) = ld.detail.peak {
        let x = px(p.layer as usize);
        img.line((x, f64::from(y0)), (x, f64::from(y1)), WARN, 1.0);
        img.label(
            x as i32 - 90,
            r.y + 24,
            &format!("worst layer {}", p.layer),
            1,
            WARN,
        );
    }
    let lift = c.data.process.lift_mm;
    let h = c.data.process.layer_height_mm;
    img.text(x0, y0 + 8, &format!("z {lift:.1}"), 1, TEXT);
    img.text(
        x1 - 60,
        y0 + 8,
        &format!("z {:.1} mm", lift + h * layers.len() as f64),
        1,
        TEXT,
    );
    img.text(r.x + 6, y1, &format!("{ymax:.0} mm2"), 1, TEXT);
    img.label(
        x1 - 330,
        y1 + 4,
        "red: peel area   green: 12 x active capacity",
        1,
        TEXT,
    );
}

fn peak_plan(img: &mut Image, r: Rect, c: &Ctx<'_>, ld: &LoadData) {
    img.label(
        r.x + 8,
        r.y + 6,
        "(b) the peeling layer, seen from the plate",
        2,
        TEXT,
    );
    let Some(pk) = ld.detail.peak else { return };
    let src = &ld.peak_sources;
    if src.is_empty() {
        return;
    }
    let mut lo = [f64::INFINITY; 2];
    let mut hi = [f64::NEG_INFINITY; 2];
    for e in src {
        for k in 0..2 {
            lo[k] = lo[k].min(e.xyz[k]);
            hi[k] = hi[k].max(e.xyz[k]);
        }
    }
    let inner = Rect {
        x: r.x + 30,
        y: r.y + 50,
        w: r.w - 60,
        h: r.h - 120,
    };
    let s = (f64::from(inner.w) / (hi[0] - lo[0]).max(1e-6))
        .min(f64::from(inner.h) / (hi[1] - lo[1]).max(1e-6));
    let (used_w, used_h) = ((hi[0] - lo[0]) * s, (hi[1] - lo[1]) * s);
    let to = |x: f64, y: f64| {
        (
            f64::from(inner.x) + 0.5 * (f64::from(inner.w) - used_w) + (x - lo[0]) * s,
            f64::from(inner.y) + 0.5 * (f64::from(inner.h) + used_h) - (y - lo[1]) * s,
        )
    };
    for e in src {
        let (x, y) = to(e.xyz[0], e.xyz[1]);
        let col = e.contact.map_or([90, 90, 90], palette);
        img.fill_rect(
            Rect {
                x: x as i32 - 3,
                y: y as i32 - 3,
                w: 7,
                h: 7,
            },
            col,
        );
    }
    for (i, cv) in ld.layout.contacts.iter().enumerate() {
        let load: f64 = src
            .iter()
            .filter(|e| e.contact == Some(i as u32))
            .map(|e| e.area)
            .sum();
        if load <= 0.0 {
            continue;
        }
        let (x, y) = to(cv.centre[0], cv.centre[1]);
        let b = ld
            .detail
            .groups
            .iter()
            .find(|g| g.layer == pk.layer && g.active.contains(&(i as u32)));
        let sec = b.map_or(0.0, |g| g.section_sum_mm2 / g.active.len() as f64);
        img.ring((x, y), 7.0, TEXT, 2.0);
        img.label(
            x as i32 + 9,
            y as i32 - 14,
            &format!("{i}: {load:.2}/{:.2}={:.1}", sec, load / sec.max(1e-9)),
            1,
            TEXT,
        );
    }
    img.text(
        r.x + 8,
        r.y + r.h - 30,
        &format!(
            "layer {} z {:.2} mm  area {:.2} mm2  capacity {:.2} mm2",
            pk.layer,
            c.data.process.lift_mm + f64::from(pk.layer) * c.data.process.layer_height_mm,
            pk.area_mm2,
            pk.capacity_mm2
        ),
        1,
        TEXT,
    );
}

fn p08_islands(c: &Ctx<'_>) -> Picture {
    let Some(ld) = c.ld else {
        return unverified_page("08-islands", "The two islands", "no load data was given");
    };
    let n = ld.structure.islands.len().min(2);
    let cap = "Each row: the slice of the island's first layer with the island in red (the shapes ChituBox's island report shows), and the underside view with an arrow to where it starts. Sizes and growth are in the report.".to_owned();
    let (mut img, r) = page(
        "8  Islands",
        &cap,
        &[(PATCH, "the island"), (GREY, "other material")],
    );
    let rows = n.max(1) as i32;
    for (k, isl) in ld.structure.islands.iter().take(2).enumerate() {
        let row = Rect {
            y: r.y + k as i32 * (r.h / rows),
            h: r.h / rows - 6,
            ..r
        };
        let two = split(row, 2);
        frame_it(&mut img, two[0]);
        frame_it(&mut img, two[1]);
        slice_panel(&mut img, two[0], isl);
        img.label(
            two[0].x + 8,
            two[0].y + 6,
            &format!(
                "island {} first layer {}  {:.3} mm2  {} x {} px",
                isl.component, isl.first_layer, isl.first_area_mm2, isl.width_px, isl.height_px
            ),
            1,
            TEXT,
        );
        if let Some(p) = island_centre(c, k) {
            let cam = draw_scene(
                &mut img,
                two[1],
                &closeup(c, under_dir(c), p, 9.0),
                c.scene,
                &look_for(0),
            );
            let (x, y, _) = cam.project(p);
            let (px, py) = (f64::from(two[1].x) + x, f64::from(two[1].y) + y);
            img.arrow((px + 70.0, py - 70.0), (px + 4.0, py - 4.0), PATCH, 3.0);
            img.label(two[1].x + 8, two[1].y + 6, "underside view", 2, TEXT);
        }
    }
    finish("08-islands", "Islands", &cap, img)
}

fn slice_panel(img: &mut Image, r: Rect, isl: &plinth_support::load::IslandInfo) {
    let mut lo = [f64::INFINITY; 2];
    let mut hi = [f64::NEG_INFINITY; 2];
    for p in isl.first_layer_rings.iter().flatten() {
        for k in 0..2 {
            lo[k] = lo[k].min(p[k]);
            hi[k] = hi[k].max(p[k]);
        }
    }
    if lo[0] > hi[0] {
        return;
    }
    let pad = 1.2;
    let (cx, cy) = (0.5 * (lo[0] + hi[0]), 0.5 * (lo[1] + hi[1]));
    let ext = (hi[0] - lo[0]).max(hi[1] - lo[1]).max(2.0) + pad;
    let s = f64::from(r.w.min(r.h) - 60) / ext;
    let to = |p: [f64; 2]| {
        (
            f64::from(r.x) + 0.5 * f64::from(r.w) + (p[0] - cx) * s,
            f64::from(r.y) + 0.5 * f64::from(r.h) - (p[1] - cy) * s,
        )
    };
    for (i, ring) in isl.first_layer_rings.iter().enumerate() {
        let col = if Some(i) == isl.ring { PATCH } else { GREY };
        for j in 0..ring.len() {
            let (a, b) = (to(ring[j]), to(ring[(j + 1) % ring.len()]));
            img.line(a, b, col, if Some(i) == isl.ring { 3.0 } else { 1.0 });
        }
    }
    // The first layer of a late island is a speck: ring it so it can be found.
    let (x, y) = to(isl.first_xy);
    img.ring((x, y), 16.0, PATCH, 2.0);
    img.label(x as i32 + 20, y as i32 - 8, "island", 1, PATCH);
}

fn p09_raft(c: &Ctx<'_>) -> Picture {
    let rafts: Vec<&Part> = c
        .scene
        .parts
        .iter()
        .filter(|p| p.role == Role::Raft)
        .collect();
    let boxes: Vec<(V3, V3)> = rafts
        .iter()
        .map(|p| {
            let (mut lo, mut hi) = ([f64::INFINITY; 3], [f64::NEG_INFINITY; 3]);
            for v in p.tris.iter().flatten() {
                for k in 0..3 {
                    lo[k] = lo[k].min(f64::from(v[k]));
                    hi[k] = hi[k].max(f64::from(v[k]));
                }
            }
            (lo, hi)
        })
        .collect();
    let pieces = pieces_of(&boxes);
    let cap = format!(
        "The raft seen from below: {} parts (pads and bridges), {} connected piece(s) by overlap of their boxes in plan view. That is a picture check; the solid union is proved by the checker, which is not part of this data (unverified). Gold dots are column bases.",
        rafts.len(),
        pieces
    );
    let (mut img, r) = page(
        "9  Raft from below",
        &cap,
        &[(RAFT, "raft"), (SUPPORT, "column bases")],
    );
    frame_it(&mut img, r);
    let sub_scene = Scene {
        model: Vec::new(),
        parts: c
            .scene
            .parts
            .iter()
            .filter(|p| matches!(p.role, Role::Raft | Role::Flare))
            .cloned()
            .collect(),
    };
    if sub_scene.parts.is_empty() {
        img.label(r.x + 20, r.y + 20, "no raft in the scene", 2, WARN);
    } else {
        let view = fit(&sub_scene, [0.0, 0.0, 1.0], [0.0, 1.0, 0.0], r);
        draw_scene(
            &mut img,
            r,
            &view,
            &sub_scene,
            &Look {
                model: false,
                ..base_look()
            },
        );
    }
    finish("09-raft", "Raft from below", &cap, img)
}

fn pieces_of(boxes: &[(V3, V3)]) -> usize {
    let n = boxes.len();
    let mut parent: Vec<usize> = (0..n).collect();
    fn find(p: &mut [usize], mut a: usize) -> usize {
        while p[a] != a {
            p[a] = p[p[a]];
            a = p[a];
        }
        a
    }
    for i in 0..n {
        for j in i + 1..n {
            let (a, b) = (boxes[i], boxes[j]);
            if a.0[0] <= b.1[0] && b.0[0] <= a.1[0] && a.0[1] <= b.1[1] && b.0[1] <= a.1[1] {
                let (x, y) = (find(&mut parent, i), find(&mut parent, j));
                parent[x] = y;
            }
        }
    }
    (0..n).filter(|&i| find(&mut parent, i) == i).count()
}

fn p10_need(c: &Ctx<'_>) -> Picture {
    let cs = contacts_of(c);
    let state = |t: &Tri| -> u8 {
        let n = normal(t);
        if n[2] >= 0.0 || -n[2] <= 45.0_f64.to_radians().cos() {
            0
        } else if cs.iter().any(|k| dist(k.centre, centroid(t)) <= 3.0) {
            1
        } else {
            2
        }
    };
    let (mut covered, mut open, mut flat) = (0.0, 0.0, 0.0);
    for t in &c.scene.model {
        match state(t) {
            1 => covered += area(t),
            2 => open += area(t),
            _ => flat += area(t),
        }
    }
    let cap = format!(
        "The model coloured by need, seen from the underside. Grey-blue: self-supporting (45 degrees or steeper, or facing up), {flat:.1} mm2. Green: flatter than 45 degrees and a contact within 3.0 mm, {covered:.1} mm2. Orange: flatter than 45 degrees with no contact within 3.0 mm, {open:.1} mm2 (surface areas; the print-order timing of the cover is checked in the report)."
    );
    let (mut img, r) = page(
        "10  Model coloured by need",
        &cap,
        &[
            (MODEL, "self-supporting"),
            (GREEN, "needs support, covered"),
            (WARN, "needs support, not covered"),
        ],
    );
    frame_it(&mut img, r);
    let colour = |i: usize, t: &Tri| -> Rgb {
        let _ = i;
        match state(t) {
            1 => GREEN,
            2 => WARN,
            _ => MODEL,
        }
    };
    let cam = draw_scene(
        &mut img,
        r,
        &fit(c.scene, under_dir(c), [0.0, 0.0, 1.0], r),
        c.scene,
        &Look {
            model_colour: &colour,
            ..tips_look()
        },
    );
    mark(
        &mut img,
        r,
        &cam,
        c,
        &(0..cs.len()).collect::<Vec<_>>(),
        &|_| String::new(),
    );
    finish("10-need", "Model coloured by need", &cap, img)
}

fn warning_centre(w: &LoadWarning, cs: &[plinth_support::load::ContactView]) -> Option<V3> {
    let pts: Vec<V3> = w
        .contacts
        .iter()
        .filter_map(|&i| cs.get(i as usize))
        .map(|c| c.centre)
        .collect();
    if pts.is_empty() {
        return None;
    }
    let n = pts.len() as f64;
    Some([
        pts.iter().map(|p| p[0]).sum::<f64>() / n,
        pts.iter().map(|p| p[1]).sum::<f64>() / n,
        pts.iter().map(|p| p[2]).sum::<f64>() / n,
    ])
}

fn p11_warnings(c: &Ctx<'_>) -> Picture {
    let cs = contacts_of(c);
    let located: Vec<&LoadWarning> = c
        .data
        .warnings
        .iter()
        .filter(|w| w.class != WarningClass::Information)
        .filter(|w| warning_centre(w, cs).is_some())
        .collect();
    let shown = located.len().min(3);
    let cap = format!(
        "Each located warning in two views (underside and side), contacts of the warning ringed. {} located warning(s) of {} in total; the first {shown} are drawn. The report lists all of them with their text.",
        located.len(),
        c.data.warnings.len()
    );
    let (mut img, r) = page("11  Warning regions", &cap, &[(WARN, "warning contacts")]);
    if shown == 0 {
        img.label(r.x + 20, r.y + 20, "no located warning to draw", 2, TEXT);
    }
    for (k, w) in located.iter().take(3).enumerate() {
        let row = Rect {
            y: r.y + k as i32 * (r.h / 3),
            h: r.h / 3 - 6,
            ..r
        };
        let centre = warning_centre(w, cs).unwrap_or([0.0; 3]);
        for (i, p) in split(row, 2).iter().enumerate() {
            frame_it(&mut img, *p);
            let dir = if i == 0 { under_dir(c) } else { side_dir(c) };
            let cam = draw_scene(
                &mut img,
                *p,
                &closeup(c, dir, centre, 4.5),
                c.scene,
                &look_for(i),
            );
            for &ci in &w.contacts {
                if let Some(cv) = cs.get(ci as usize) {
                    let (x, y, _) = cam.project(cv.centre);
                    img.ring((f64::from(p.x) + x, f64::from(p.y) + y), 10.0, WARN, 3.0);
                }
            }
            img.label(
                p.x + 8,
                p.y + 6,
                &format!("{}: {}", if i == 0 { "underside" } else { "side" }, w.kind),
                1,
                WARN,
            );
        }
    }
    finish("11-warnings", "Warning regions", &cap, img)
}

fn p12_rim(c: &Ctx<'_>) -> Picture {
    let cs = contacts_of(c);
    let rim: Vec<usize> = (0..cs.len())
        .filter(|&i| cs[i].site_class == SiteClass::PartlyVisibleRim)
        .collect();
    let needing: Vec<&plinth_support::load::needs::NeedPoint> =
        c.ld.map(|l| {
            l.needs
                .points
                .iter()
                .filter(|p| p.class == SiteTag::PartlyVisible)
                .collect()
        })
        .unwrap_or_default();
    let miss: Vec<_> = needing
        .iter()
        .filter(|p| !cs.iter().any(|k| dist(k.centre, p.xyz) <= 3.0))
        .collect();
    let cap = format!(
        "The partly visible rim band below 45 degrees. {} rim contacts (class 2), numbered with their patch lower bound in mm2. {} of {} rim-band samples have no contact within 3.0 mm (orange dots). Site classes come from the ray test; samples whose class could not be decided are not counted here.",
        rim.len(),
        miss.len(),
        needing.len()
    );
    let (mut img, r) = page(
        "12  Rim band",
        &cap,
        &[
            (CLASS2, "class 2 contact"),
            (WARN, "not covered within 3.0 mm"),
        ],
    );
    let centre = if rim.is_empty() {
        let (lo, hi) = c.scene.bounds();
        [
            0.5 * (lo[0] + hi[0]),
            0.5 * (lo[1] + hi[1]),
            0.5 * (lo[2] + hi[2]),
        ]
    } else {
        let n = rim.len() as f64;
        [
            rim.iter().map(|&i| cs[i].centre[0]).sum::<f64>() / n,
            rim.iter().map(|&i| cs[i].centre[1]).sum::<f64>() / n,
            rim.iter().map(|&i| cs[i].centre[2]).sum::<f64>() / n,
        ]
    };
    for (i, p) in split(r, 2).iter().enumerate() {
        frame_it(&mut img, *p);
        let dir = if i == 0 { under_dir(c) } else { side_dir(c) };
        let cam = draw_scene(
            &mut img,
            *p,
            &closeup(c, dir, centre, 6.5),
            c.scene,
            &look_for(i),
        );
        mark(&mut img, *p, &cam, c, &rim, &|k| {
            format!(" {:.2}", cs[k].patch_lower_mm2)
        });
        for m in &miss {
            let (x, y, _) = cam.project(m.xyz);
            img.disc((f64::from(p.x) + x, f64::from(p.y) + y), 3.0, WARN);
        }
        img.label(
            p.x + 8,
            p.y + 6,
            if i == 0 {
                "underside view"
            } else {
                "side view"
            },
            2,
            TEXT,
        );
    }
    finish("12-rim-band", "Rim band", &cap, img)
}

#[cfg(test)]
mod tests;
