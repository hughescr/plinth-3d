//! The load audit (design 7.2, last row). It is a report, not a rule: it takes no part in any
//! verdict. From the written file alone it gives the additive area ratio the generator's
//! load model must agree with.
//!
//! * **Layer area.** The area of the model's section at the middle of each layer (0.05 mm),
//!   from the oriented section segments of the closed model mesh (holes subtract). Other
//!   slicing heights differ by the new down-facing area per layer (design section 0: 1 to 4 %).
//! * **Section of a contact.** `B_i` = the smaller of the patch's certified lower bound (over
//!   both representations, so it holds whichever solid is sliced) and the exposed neck of its
//!   tip cone (the written cone's section where the cone leaves the contact ball). A contact
//!   counts from the layer of a point of its patch.
//! * **Ratio.** `X(k) = A_k / sum B_i` over the contacts counted at layer `k`, and the
//!   utilisation `X / 12` (design ratio P5 = 12). `X > 25` (beyond the measured breaking
//!   ratio) or a positive area over no counted section is the unprintable trigger.
//!
//! What it does not do: it treats the whole model as one load group (the per-component split
//! needs the model's pieces, which this checker does not label), so it understates the ratio of
//! a component that is poorly held; it counts the whole patch from the layer of one patch
//! point, so it overstates early capacity. The generator's largest tributary ratio must be at
//! least this group ratio; a generator that drops a layer or a component would be below it.

use crate::json::Json;
use crate::patch::PatchOut;
use crate::scene::{LAYER_MM, Scene};

/// The design ratio (PRD P5).
pub const DESIGN_RATIO: f64 = 12.0;
/// Beyond this ratio supports snap (PRD P4 measured breaking ratio).
pub const BREAKING_RATIO: f64 = 25.0;

/// The ratio of `area` to `section`: `(X, utilisation, unprintable trigger)`.
#[must_use]
pub fn ratio(area: f64, section: f64) -> (f64, f64, bool) {
    if area > 0.0 && section <= 0.0 {
        return (f64::INFINITY, f64::INFINITY, true);
    }
    if area <= 0.0 {
        return (0.0, 0.0, false);
    }
    let x = area / section;
    (x, x / DESIGN_RATIO, x > BREAKING_RATIO)
}

/// Area of the model's section at height `z` of every layer, mm2, for layers `0..layers`.
fn layer_areas(scene: &Scene, z0: f64, layers: usize) -> Vec<f64> {
    let mut area = vec![0.0; layers];
    for t in &scene.model {
        let (zlo, zhi) = t
            .iter()
            .fold((f64::INFINITY, f64::NEG_INFINITY), |(l, h), p| {
                (l.min(p[2]), h.max(p[2]))
            });
        let n = crate::v3::cross(crate::v3::sub(t[1], t[0]), crate::v3::sub(t[2], t[0]));
        #[allow(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "a layer index"
        )]
        let first = (((zlo - z0) / LAYER_MM - 0.5).ceil().max(0.0)) as usize;
        #[allow(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "a layer index"
        )]
        let last = ((((zhi - z0) / LAYER_MM - 0.5).floor()).max(-1.0) + 1.0) as usize;
        for (k, slot) in area.iter_mut().enumerate().take(last).skip(first) {
            let c = z0 + (k as f64 + 0.5) * LAYER_MM;
            let mut pts: Vec<[f64; 2]> = Vec::with_capacity(2);
            for e in 0..3 {
                let (p, q) = (t[e], t[(e + 1) % 3]);
                let (dp, dq) = (p[2] - c, q[2] - c);
                if (dp < 0.0 && dq >= 0.0) || (dp >= 0.0 && dq < 0.0) {
                    let s = dp / (dp - dq);
                    pts.push([p[0] + (q[0] - p[0]) * s, p[1] + (q[1] - p[1]) * s]);
                }
            }
            if let [a, b] = pts[..] {
                let d = [b[0] - a[0], b[1] - a[1]];
                let sign = if d[0] * -n[1] + d[1] * n[0] >= 0.0 {
                    1.0
                } else {
                    -1.0
                };
                *slot += sign * 0.5 * (a[0] * b[1] - b[0] * a[1]);
            }
        }
    }
    area
}

/// One contact: the layer it counts from and its section.
struct Contact {
    from: usize,
    section: f64,
}

fn contacts(scene: &Scene, patches: &PatchOut, z0: f64) -> Vec<Contact> {
    let mut out = Vec::new();
    for (ci, chain) in scene.chains.chains.iter().enumerate() {
        let Some(ball) = chain.contact else { continue };
        let mut lows = Vec::new();
        let mut at_z = f64::INFINITY;
        for rep in &patches.reps {
            let comps: Vec<_> = rep.comps.iter().filter(|c| c.chain == ci).collect();
            lows.push(comps.iter().map(|c| c.certain_lo).sum::<f64>());
            at_z = comps.iter().map(|c| c.at[2]).fold(at_z, f64::min);
        }
        if !at_z.is_finite() {
            continue;
        }
        let patch_lb = lows.iter().copied().fold(f64::INFINITY, f64::min);
        let neck = chain
            .cone
            .and_then(|c| crate::h6::neck_area(scene, &scene.parts[ball], &scene.parts[c]))
            .unwrap_or(f64::INFINITY);
        #[allow(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "a layer index"
        )]
        let from = (((at_z - z0) / LAYER_MM) + 1e-9).floor().max(0.0) as usize;
        out.push(Contact {
            from,
            section: patch_lb.min(neck),
        });
    }
    out
}

/// The audit as report JSON.
#[must_use]
pub fn run(scene: &Scene, patches: &PatchOut) -> Json {
    let scope = "whole model as one load group; per-component grouping is not built, so a poorly held component's ratio is understated; only model-contact sections are counted: redundant incidental support contacts carry no strength credit";
    if scene.parts.is_empty() {
        return Json::obj([
            (
                "state",
                Json::str("no support records: nothing carries the model"),
            ),
            ("scope", Json::str(scope)),
        ]);
    }
    if !scene.model_closed {
        return Json::obj([
            (
                "state",
                Json::str("not computed: the model is not a closed, consistently oriented mesh"),
            ),
            ("scope", Json::str(scope)),
        ]);
    }
    let z0 = scene.plate_z();
    let top = scene
        .model
        .iter()
        .flatten()
        .map(|p| p[2])
        .fold(f64::NEG_INFINITY, f64::max);
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "a layer index"
    )]
    let layers = (((top - z0) / LAYER_MM).ceil().max(0.0) as usize).min(40_000);
    let areas = layer_areas(scene, z0, layers);
    let cs = contacts(scene, patches, z0);
    let (mut worst, mut worst_k, mut trigger_layers) = ((0.0, 0.0), 0usize, 0usize);
    let mut largest_area = (0.0f64, 0usize);
    for (k, a) in areas.iter().enumerate() {
        if *a > largest_area.0 {
            largest_area = (*a, k);
        }
        let section: f64 = cs.iter().filter(|c| c.from <= k).map(|c| c.section).sum();
        let (x, u, trig) = ratio(*a, section);
        trigger_layers += usize::from(trig);
        if x > worst.0 {
            worst = (x, u);
            worst_k = k;
        }
    }
    let num = |v: f64| {
        if v.is_finite() {
            Json::Num(v)
        } else {
            Json::str("infinite")
        }
    };
    Json::obj([
        ("state", Json::str("computed")),
        ("scope", Json::str(scope)),
        ("layer_height_mm", Json::Num(LAYER_MM)),
        ("layers", Json::Uint(layers as u64)),
        ("contacts_counted", Json::Uint(cs.len() as u64)),
        ("design_ratio", Json::Num(DESIGN_RATIO)),
        ("breaking_ratio", Json::Num(BREAKING_RATIO)),
        ("largest_layer_area_mm2", Json::Num(largest_area.0)),
        (
            "largest_layer_area_layer",
            Json::Uint(largest_area.1 as u64),
        ),
        ("largest_additive_area_ratio", num(worst.0)),
        ("utilisation_at_design_ratio", num(worst.1)),
        ("worst_layer", Json::Uint(worst_k as u64)),
        (
            "unprintable_trigger_layers",
            Json::Uint(trigger_layers as u64),
        ),
        (
            "method",
            Json::str(
                "mid-plane model section area per layer over the sum of contact sections (smaller of the certified patch lower bound over both representations and the exposed neck), contacts counted from the layer of a patch point",
            ),
        ),
    ])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testkit::{self as kit, test_limits};

    #[test]
    fn sixty_square_millimetres_over_two_of_section_is_ratio_30_utilisation_2_5_and_unprintable() {
        let (x, u, trigger) = ratio(60.0, 2.0);
        assert!((x - 30.0).abs() < 1e-12 && (u - 2.5).abs() < 1e-12 && trigger);
        assert!(!ratio(60.0, 6.0).2);
        // Positive area over no section is unprintable, never zero.
        let (x, _, trigger) = ratio(1.0, 0.0);
        assert!(x.is_infinite() && trigger);
        assert!(!ratio(0.0, 0.0).2);
    }

    #[test]
    fn a_box_over_one_contact_reports_the_layer_area_over_that_contact() {
        // A 10 x 6 mm box (60 mm2 a layer) lifted 5 mm, one tip ball under it.
        let model = kit::closed_box([-5.0, -3.0, -5.0], [5.0, 3.0, -4.0]);
        let ball = kit::part(0, 7, [0.0, 0.0, -5.35], [0.0, 0.0, -4.65], 0.35, 0.35);
        let scene = Scene::from_parts(&test_limits(), model, [0.0, 0.0, 5.0], vec![ball]);
        let patches = crate::patch::measure(&scene);
        let j = run(&scene, &patches);
        let text = j.render();
        assert!(text.contains("\"state\": \"computed\""), "{text}");
        let Some(Json::Num(a)) = j.get("largest_layer_area_mm2") else {
            panic!("{text}");
        };
        assert!((a - 60.0).abs() < 1e-9, "{a}");
        let Some(Json::Num(x)) = j.get("largest_additive_area_ratio") else {
            panic!("{text}");
        };
        // Patch of the tip ball: pi (0.35^2 - 0.0) = 0.385 mm2 at best; the ratio is about 150+.
        assert!(*x > 100.0, "{x}");
    }
}
