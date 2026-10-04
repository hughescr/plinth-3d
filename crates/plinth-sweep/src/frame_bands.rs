//! **R6 + D4: brace bands counted through a frame in which every band can
//! sway** (Craig ruling 2026-10-03, interim pending the slenderness coupon;
//! `field-2026-09-28/bracing-mechanics.md`, revision 2, §3 "How many bands?
//! Every band compliant").
//!
//! Until this rule the engine credited every `|X|` band as a FIXED POINT: a
//! run needed `⌈L / limit⌉ − 1` bands, one per whole limit it overshoots,
//! because each piece between two bands was measured as a cantilever clamped
//! at the band below. A band is not a clamp. It is two diagonals between two
//! posts that bend too, so the two posts and their bands sway together as one
//! frame, and the frame needs MORE bands: about 2 at twice the limit, 3 at
//! 2.5 times and 5-6 at 3 times, where the fixed-point count gives 1, 2 and 2.
//! Past about 3-3.5 times no number of bands between two posts holds (the
//! two-post ceiling; D9).
//!
//! The model is the study's `stage/bands2` panel (`bracing-mechanics/tools/
//! bands2.ts`), ported: two posts clamped at the run's base, each band two
//! diagonals that meet at mid-gap, every node free. It is solved with the
//! frame solver of [`crate::frame`], loaded by the S-law's uniform drag on
//! both posts (and, for comparison, by a sideways force at each top), and the
//! run's top sway is read in pixels: `E`, `q` and the pixel cancel into `S`,
//! exactly as they do in the one-pixel law itself, so the answer `1.0` is the
//! same limit the run was measured against.
//!
//! Its numbers are held to the study's own on the study's own fixtures, every
//! cell (`tests::the_panel_reproduces_the_study_band_table`).
//!
//! **What a count costs** (band-speed lane, 2026-10-04). The ladder asks for a
//! count for every partner in reach of every run it cannot hold, so counts
//! are the hot path of the brace rung (about six million of them in the toed
//! slab's search at round 13). [`count_bands`] therefore solves each panel
//! ONCE, in its own plane, for the drag alone ([`panel_drag`]): the panel is
//! planar and the tip column is reported, never decided on, so the full
//! three-dimensional solve for both loads that [`panel_sway`] still does (the
//! tests' reference) is not needed. The in-plane solve is the full solve's
//! arithmetic on the in-plane freedoms in the same order, so its drag is the
//! same number to the last bit
//! (`tests::the_drag_alone_is_the_drags_in_the_full_sway_to_the_last_bit`),
//! about six times cheaper (315 µs against 48 µs on 2,000 random panels,
//! 4.5 bands a panel).

use crate::frame::Frame;

/// The study's mesh along each post: 24 even divisions of the post height,
/// plus every band end (`bands2.ts`: `for (let i = 1; i < 24; i++)`). A
/// discretisation choice, not physics; it only sets where the uniform drag is
/// lumped (the beam elements are exact under nodal loads).
const POST_DIVISIONS: u32 = 24;

/// Heights are rounded to a micrometre (`1 / LEVELS_PER_MM`) before the mesh
/// is built (`bands2.ts`: `toFixed(3)`), so that a band end and a post level
/// that agree to a micrometre are one node. Multiplied and divided by the
/// exact integer, as `Math.round(z * 1000) / 1000` does: dividing by `1e-3`
/// instead moves the last bit, and two levels exactly the merge distance
/// apart then merge on one side and not the other.
const LEVELS_PER_MM: f64 = 1000.0;

/// One two-post panel: the run's post (`post`), its partner, the gap between
/// their axes, the run's length above its base, and its bands, each as
/// `(bottom, top)` heights above the base with the diagonal radius.
#[derive(Clone, Debug, PartialEq)]
pub struct Panel {
    pub post_radius_mm: f64,
    pub partner_radius_mm: f64,
    pub gap_mm: f64,
    pub height_mm: f64,
    pub bands: Vec<PanelBand>,
}

/// One `|X|` band of a [`Panel`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PanelBand {
    pub bottom_mm: f64,
    pub top_mm: f64,
    pub brace_radius_mm: f64,
}

/// How far the run's top sways, against its own one-pixel limit.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PanelSway {
    /// Under the S-law's uniform drag on both posts: the top sway in pixels.
    /// `≤ 1` passes.
    pub drag_over_pixel: f64,
    /// Under a unit sideways force at both tops, as a multiple of what the
    /// same force does to a free post exactly at its limit (`bands2.ts`'s tip
    /// column; reported, not used to decide).
    pub tip_over_limit: f64,
}

fn level(z: f64) -> f64 {
    (z * LEVELS_PER_MM).round() / LEVELS_PER_MM
}

/// A panel's frame, built, with what the two solves read off it.
struct Built {
    frame: Frame,
    /// The node numbers up each post, base first.
    columns: Vec<Vec<usize>>,
    top_a: usize,
    top_b: usize,
    /// `π r⁴ / 4` of the run's post.
    second: f64,
    /// The run's length in its own one-pixel lengths.
    lambda: f64,
    height: f64,
}

/// The study's uniform drag, lumped: each node takes half of each
/// neighbouring division, on both posts.
fn drag_loads(built: &Built) -> Vec<(usize, [f64; 3])> {
    let frame = &built.frame;
    let mut drag: Vec<(usize, [f64; 3])> = Vec::new();
    for column in &built.columns {
        for (i, &node) in column.iter().enumerate() {
            let z = |at: usize| frame.point(column[at])[2];
            let below = if i == 0 { z(i) } else { z(i - 1) };
            let above = if i + 1 < column.len() { z(i + 1) } else { z(i) };
            drag.push((node, [0.5 * (above - below), 0.0, 0.0]));
        }
    }
    drag
}

/// Solve one panel. `None` where a band does not fit inside the run, or the
/// input is not a panel (a non-positive radius, gap or height).
#[must_use]
pub fn panel_sway(panel: &Panel, ratio: f64) -> Option<PanelSway> {
    let built = build(panel, ratio)?;
    let tip = built.frame.solve(&[
        (built.top_a, [1.0, 0.0, 0.0]),
        (built.top_b, [1.0, 0.0, 0.0]),
    ]);
    if !tip.solved[built.top_a] {
        return None;
    }
    let swept = built.frame.solve(&drag_loads(&built));
    if !swept.solved[built.top_a] {
        return None;
    }
    let Built {
        top_a,
        second,
        lambda,
        height,
        ..
    } = built;
    Some(PanelSway {
        drag_over_pixel: swept.moved[top_a][0] / (height.powi(4) / (8.0 * second))
            * lambda.powi(4),
        tip_over_limit: tip.moved[top_a][0] / (height.powi(3) / (3.0 * second)) * lambda.powi(3),
    })
}

/// [`panel_sway`]'s `drag_over_pixel` and nothing else: the one solve
/// [`count_bands`] and the engine's checks read. The same number to the last
/// bit (`tests::the_drag_alone_is_the_drags_in_the_full_sway_to_the_last_bit`),
/// from one in-plane solve instead of two full ones: a panel is planar, and
/// the tip column is reported, never decided on.
#[must_use]
pub fn panel_drag(panel: &Panel, ratio: f64) -> Option<f64> {
    let built = build(panel, ratio)?;
    let swept = built.frame.solve_in_plane(&drag_loads(&built));
    if !swept.solved[built.top_a] {
        return None;
    }
    Some(
        swept.moved[built.top_a][0] / (built.height.powi(4) / (8.0 * built.second))
            * built.lambda.powi(4),
    )
}

/// The frame of one panel, or `None` where a band does not fit inside the
/// run or the input is not a panel.
fn build(panel: &Panel, ratio: f64) -> Option<Built> {
    let Panel {
        post_radius_mm: rp,
        partner_radius_mm: rq,
        gap_mm: gap,
        height_mm: height,
        ref bands,
    } = *panel;
    let sane = |v: f64| v.is_finite() && v > 0.0;
    if !(sane(rp) && sane(rq) && sane(gap) && sane(height) && sane(ratio)) {
        return None;
    }
    let mut levels: Vec<f64> = vec![0.0, height];
    for i in 1..POST_DIVISIONS {
        levels.push(level(height * f64::from(i) / f64::from(POST_DIVISIONS)));
    }
    let mut ends: Vec<(f64, f64, f64, f64)> = Vec::with_capacity(bands.len());
    for band in bands {
        let (bottom, top) = (level(band.bottom_mm), level(band.top_mm));
        if !(bottom.is_finite() && top.is_finite())
            || bottom < 0.0
            || top > height
            || top <= bottom
            || !sane(band.brace_radius_mm)
        {
            return None;
        }
        levels.push(bottom);
        levels.push(top);
        ends.push((
            bottom,
            top,
            0.5 * (band.bottom_mm + band.top_mm),
            band.brace_radius_mm,
        ));
    }
    levels.retain(|z| (0.0..=height).contains(z));
    levels.sort_by(f64::total_cmp);
    levels.dedup();
    let mut frame = Frame::new();
    let mut columns: Vec<Vec<usize>> = Vec::with_capacity(2);
    for (x, radius) in [(0.0, rp), (gap, rq)] {
        let mut column = Vec::with_capacity(levels.len());
        let mut previous = frame.node([x, 0.0, levels[0]], true);
        column.push(previous);
        for &z in &levels[1..] {
            let node = frame.node([x, 0.0, z], false);
            if node == previous {
                continue;
            }
            frame.member(previous, node, radius);
            column.push(node);
            previous = node;
        }
        columns.push(column);
    }
    for &(bottom, top, centre, brace) in &ends {
        let middle = frame.node([0.5 * gap, 0.0, centre], false);
        let a0 = frame.node([0.0, 0.0, bottom], false);
        let a1 = frame.node([0.0, 0.0, top], false);
        let b0 = frame.node([gap, 0.0, bottom], false);
        let b1 = frame.node([gap, 0.0, top], false);
        frame.member(a0, middle, brace);
        frame.member(middle, b1, brace);
        frame.member(b0, middle, brace);
        frame.member(middle, a1, brace);
    }
    let top_a = *columns[0].last()?;
    let top_b = *columns[1].last()?;
    let second = core::f64::consts::PI * rp.powi(4) / 4.0;
    let lambda = height / (ratio * rp);
    Some(Built {
        frame,
        columns,
        top_a,
        top_b,
        second,
        lambda,
        height,
    })
}

/// What [`count_bands`] found for one pair.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum BandCount {
    /// The fewest bands, laid out as the caller lays them, whose panel holds
    /// the run's top within one pixel under drag, and that sway.
    Holds { bands: usize, drag_over_pixel: f64 },
    /// No band count that fits the run holds it: the pair is past its
    /// two-post ceiling with the bands it can carry (D9). `most_bands` is the
    /// largest count that fit (`0`: not even one), and its sway.
    PastCeiling {
        most_bands: usize,
        drag_over_pixel: f64,
    },
}

/// Relative float noise on the one-pixel comparison: a panel that sways
/// exactly one pixel holds. Numerical, not physical.
const PIXEL_NOISE: f64 = 1.0e-9;

/// **R6: the fewest bands that hold a run**, counted through the panel.
///
/// `panel` carries the pair's geometry with no bands; `layout(n)` lays `n`
/// bands as the engine will build them (`(bottom, top)` heights above the
/// run's base and the diagonal radius), or `None` where `n` bands do not fit
/// the run. The count starts at `from` (the engine passes the fixed-point
/// count, `⌈L / limit⌉ − 1`, which a frame never needs fewer than) and rises
/// until the run's top sways within one pixel. It stops, past the ceiling,
/// when `n` bands no longer fit, or when one more band no longer reduces the
/// sway (the pair bends as one beam: more bands buy nothing).
///
/// One sparse solve per count tried.
#[must_use]
pub fn count_bands(
    panel: &Panel,
    ratio: f64,
    from: usize,
    layout: &dyn Fn(usize) -> Option<Vec<PanelBand>>,
) -> BandCount {
    count_bands_with(panel, ratio, from, layout, &panel_drag)
}

/// [`count_bands`] by the FULL two-load solve ([`panel_sway`]): the loop as
/// it stood before the band-speed lane (2026-10-04), kept as the reference the
/// fast count is held to (`tests::the_fast_count_is_the_full_sways_count`,
/// and the `brace_frame_bands` integration test). Not for the engine: it
/// solves every panel twice, in six freedoms a node.
#[doc(hidden)]
#[must_use]
pub fn count_bands_by_full_sway(
    panel: &Panel,
    ratio: f64,
    from: usize,
    layout: &dyn Fn(usize) -> Option<Vec<PanelBand>>,
) -> BandCount {
    count_bands_with(panel, ratio, from, layout, &|trial, ratio| {
        panel_sway(trial, ratio).map(|sway| sway.drag_over_pixel)
    })
}

fn count_bands_with(
    panel: &Panel,
    ratio: f64,
    from: usize,
    layout: &dyn Fn(usize) -> Option<Vec<PanelBand>>,
    drag_of: &dyn Fn(&Panel, f64) -> Option<f64>,
) -> BandCount {
    let mut best = BandCount::PastCeiling {
        most_bands: 0,
        drag_over_pixel: f64::INFINITY,
    };
    let mut previous = f64::INFINITY;
    let mut n = from.max(1);
    loop {
        let Some(bands) = layout(n) else {
            return best;
        };
        let trial = Panel {
            bands,
            ..panel.clone()
        };
        let Some(drag) = drag_of(&trial, ratio) else {
            return best;
        };
        if drag <= 1.0 + PIXEL_NOISE {
            return BandCount::Holds {
                bands: n,
                drag_over_pixel: drag,
            };
        }
        if drag >= previous {
            return best;
        }
        best = BandCount::PastCeiling {
            most_bands: n,
            drag_over_pixel: drag,
        };
        previous = drag;
        n += 1;
    }
}

/// The study's own band layout: `n` bands centred at `H i / (n + 1)`, each
/// `rise` deep, every diagonal `max(r_min, ℓ / S)` with `ℓ = √(gap² + rise²)`
/// (`bands2.ts`). Test fixtures and the study's tables only; the engine lays
/// its bands on its own layers.
#[cfg(test)]
fn study_bands(height: f64, n: u32, rise: f64, gap: f64, ratio: f64, r_min: f64) -> Vec<PanelBand> {
    let brace = r_min.max(gap.hypot(rise) / ratio);
    (1..=n)
        .map(|i| {
            let centre = height * f64::from(i) / f64::from(n + 1);
            PanelBand {
                bottom_mm: centre - 0.5 * rise,
                top_mm: centre + 0.5 * rise,
                brace_radius_mm: brace,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{
        count_bands, count_bands_by_full_sway, panel_drag, panel_sway, study_bands, BandCount,
        Panel, PanelBand,
    };

    /// `bracing-mechanics/data/bands2.out` at full precision, every cell:
    /// `(post radius, gap, λ, bands, drag, tip)`, from `tools/bands2.ts`
    /// run unchanged with its print loop replaced by a JSON dump of `run()`
    /// over the same cells (2026-10-03, bun 1.4.2; the dump is
    /// `field-2026-09-28/lanes/brace-frame-bands/bands2full.json`). Its two
    /// fixtures are the Cylinder and Sphere median pair (`r 0.15, gap 2.2`)
    /// and the study's mid pair (`r 0.4, gap 5`); `S = 40`, the diagonal at
    /// 57.3° from vertical, `r_min = 0.05`.
    const STUDY: &[(f64, f64, f64, u32, f64, f64)] = &[
        (0.15, 2.2, 1.25, 1, 0.2266208441728485, 0.24085538386060787),
        (0.15, 2.2, 1.25, 2, 0.0755115918264712, 0.07838417567351799),
        (0.15, 2.2, 1.25, 3, 0.039991756931322835, 0.038483994860862895),
        (0.15, 2.2, 1.25, 4, 0.028588208382076086, 0.02515138542178436),
        (0.15, 2.2, 1.25, 5, 0.024316492121734902, 0.0198575125539315),
        (0.15, 2.2, 1.25, 6, 0.022101582003676873, 0.01721910133662238),
        (0.15, 2.2, 1.5, 1, 0.49599638632862325, 0.4343736939306676),
        (0.15, 2.2, 1.5, 2, 0.1664260788670312, 0.1432040143115189),
        (0.15, 2.2, 1.5, 3, 0.08625022149808906, 0.06975791393313784),
        (0.15, 2.2, 1.5, 4, 0.058584917680109094, 0.044077161240348606),
        (0.15, 2.2, 1.5, 5, 0.04737769309790508, 0.03341087076965059),
        (0.15, 2.2, 1.5, 6, 0.04246086817910305, 0.028504084159190753),
        (0.15, 2.2, 1.75, 1, 0.9554887821219782, 0.7113990005951074),
        (0.15, 2.2, 1.75, 2, 0.3241011119638926, 0.23752615733284568),
        (0.15, 2.2, 1.75, 3, 0.16712379003300581, 0.11585592069265577),
        (0.15, 2.2, 1.75, 4, 0.11072093945633726, 0.07212527300899955),
        (0.15, 2.2, 1.75, 5, 0.0864630191539017, 0.05327816392375724),
        (0.15, 2.2, 1.75, 6, 0.07513312092846292, 0.04427460692279465),
        (0.15, 2.2, 2.0, 1, 1.6788668481571187, 1.086945841738688),
        (0.15, 2.2, 2.0, 2, 0.5756920255218609, 0.36696916462442597),
        (0.15, 2.2, 2.0, 3, 0.2972667470470467, 0.17976475897375313),
        (0.15, 2.2, 2.0, 4, 0.19458400573661647, 0.11122397082677304),
        (0.15, 2.2, 2.0, 5, 0.14884229921533004, 0.08097200592249577),
        (0.15, 2.2, 2.0, 6, 0.1263309737748879, 0.06603584303289245),
        (0.15, 2.2, 2.5, 1, 4.272807302147102, 2.193662119045805),
        (0.15, 2.2, 2.5, 2, 1.4925041397669263, 0.7536909033865755),
        (0.15, 2.2, 2.5, 3, 0.7770575265433176, 0.37295893291907045),
        (0.15, 2.2, 2.5, 4, 0.5047708808292228, 0.2302830082731717),
        (0.15, 2.2, 2.5, 5, 0.37857204766398883, 0.16546674975745906),
        (0.15, 2.2, 2.5, 6, 0.31289312976327144, 0.13229746359189934),
        (0.15, 2.2, 3.0, 1, 9.109735186074897, 3.874638381310446),
        (0.15, 2.2, 3.0, 2, 3.227101728234229, 1.348314637264155),
        (0.15, 2.2, 3.0, 3, 1.6951323473991093, 0.6732263218804336),
        (0.15, 2.2, 3.0, 4, 1.1015134156238293, 0.41667182678817616),
        (0.15, 2.2, 3.0, 5, 0.8201287657281241, 0.2981460071569205),
        (0.15, 2.2, 3.0, 6, 0.669455033377321, 0.23621745317595502),
        (0.15, 2.2, 4.0, 1, 29.807765210022886, 9.439833831057516),
        (0.15, 2.2, 4.0, 2, 10.762789071038036, 3.341046852046133),
        (0.15, 2.2, 4.0, 3, 5.7333856169412645, 1.6905175391891365),
        (0.15, 2.2, 4.0, 4, 3.746119093065891, 1.0531019874101106),
        (0.15, 2.2, 4.0, 5, 2.7802120746126304, 0.7530502235186965),
        (0.15, 2.2, 4.0, 6, 2.249294489592244, 0.5928283254065628),
        (0.4, 5.0, 1.25, 1, 0.2664585544916323, 0.27290551577774874),
        (0.4, 5.0, 1.25, 2, 0.09924413746455805, 0.09782354798459796),
        (0.4, 5.0, 1.25, 3, 0.057359190845324384, 0.05240116674251358),
        (0.4, 5.0, 1.25, 4, 0.04206391450083097, 0.035823862445518107),
        (0.4, 5.0, 1.25, 5, 0.03533989964527746, 0.028532510433381),
        (0.4, 5.0, 1.25, 6, 0.03193950932194266, 0.024868884350898172),
        (0.4, 5.0, 1.5, 1, 0.5669772608595817, 0.48182699934705575),
        (0.4, 5.0, 1.5, 2, 0.210366979921707, 0.17298157217067262),
        (0.4, 5.0, 1.5, 3, 0.11934731832324608, 0.09167433967151666),
        (0.4, 5.0, 1.5, 4, 0.08510540770860803, 0.06137696616872391),
        (0.4, 5.0, 1.5, 5, 0.06943278244100187, 0.047707545183227604),
        (0.4, 5.0, 1.5, 6, 0.061438988099808994, 0.040774922345247315),
        (0.4, 5.0, 1.75, 1, 1.0712697984503934, 0.7775621843623681),
        (0.4, 5.0, 1.75, 2, 0.397858528729263, 0.2801393352486241),
        (0.4, 5.0, 1.75, 3, 0.22385917968659863, 0.14788288713802636),
        (0.4, 5.0, 1.75, 4, 0.15715665112071142, 0.09792937795073908),
        (0.4, 5.0, 1.75, 5, 0.125922692104349, 0.07502705438547329),
        (0.4, 5.0, 1.75, 6, 0.10950398167709906, 0.0631677717012756),
        (0.4, 5.0, 2.0, 1, 1.855871625863707, 1.1752626614836454),
        (0.4, 5.0, 2.0, 2, 0.6911184478461257, 0.4251305649552),
        (0.4, 5.0, 2.0, 3, 0.38747775462875533, 0.2241706058139304),
        (0.4, 5.0, 2.0, 4, 0.2695986775506526, 0.1475684018691632),
        (0.4, 5.0, 2.0, 5, 0.2135606279998986, 0.11204226237280718),
        (0.4, 5.0, 2.0, 6, 0.1835094875557138, 0.09341394495673011),
        (0.4, 5.0, 2.5, 1, 4.6333545853092, 2.3371656087053685),
        (0.4, 5.0, 2.5, 2, 1.7360912689488, 0.8514076532527262),
        (0.4, 5.0, 2.5, 3, 0.9722509319088801, 0.4495578393210287),
        (0.4, 5.0, 2.5, 4, 0.6708605154296385, 0.2944532389196601),
        (0.4, 5.0, 2.5, 5, 0.5249063048254092, 0.2215026653636608),
        (0.4, 5.0, 2.5, 6, 0.4447180859697378, 0.18258611200197258),
        (0.4, 5.0, 3.0, 1, 9.756117616052713, 4.088747853898456),
        (0.4, 5.0, 3.0, 2, 3.6761278782292246, 1.4981481846011122),
        (0.4, 5.0, 3.0, 3, 2.061632225585989, 0.7929838828188195),
        (0.4, 5.0, 3.0, 4, 1.4187646064182133, 0.5187273475077331),
        (0.4, 5.0, 3.0, 5, 1.1034205852197, 0.3885803579326033),
        (0.4, 5.0, 3.0, 6, 0.9282909456439391, 0.31848727882276084),
        (0.4, 5.0, 4.0, 1, 31.44710738148407, 9.845797653821514),
        (0.4, 5.0, 4.0, 2, 11.950770359661737, 3.637291852198087),
        (0.4, 5.0, 4.0, 3, 6.730069437824154, 1.9345412934368773),
        (0.4, 5.0, 4.0, 4, 4.627978227581068, 1.2662185493890186),
        (0.4, 5.0, 4.0, 5, 3.583732957137401, 0.9458256644683868),
        (0.4, 5.0, 4.0, 6, 2.99602653200176, 0.7712908530413545),
    ];

    /// **The equivalence test.** Every cell of both study tables, drag and
    /// tip. The solvers differ only in how they eliminate (dense Gaussian
    /// there, skyline Cholesky here). Measured 2026-10-03: 77 of the 84 cells
    /// agree to 1.4e-8 or better and are held to 1e-7. The other seven are the
    /// mid pair with 4 bands (up to 8.9e-6), held to 2e-5: their mesh
    /// carries a post element a few micrometres long (at λ 3, a band end at
    /// 7.995 mm beside the mesh level at 8.000 mm), and its conditioning
    /// leaves the answer uncertain at parts per million in EITHER solver. On
    /// `r 0.4, gap 5, λ 3, 4 bands` the study's dense elimination gives
    /// 1.4187646, the same elimination with three steps of iterative
    /// refinement 1.4187601, and this solver 1.4187622, between the two.
    #[test]
    fn the_panel_reproduces_the_study_band_table() {
        let tilt = 57.3_f64.to_radians().tan();
        let mut checked = 0_usize;
        let mut worst = 0.0_f64;
        for &(radius, gap, lambda, n, drag, tip) in STUDY {
            let rise = gap / tilt;
            let height = lambda * 40.0 * radius;
            let panel = Panel {
                post_radius_mm: radius,
                partner_radius_mm: radius,
                gap_mm: gap,
                height_mm: height,
                bands: study_bands(height, n, rise, gap, 40.0, 0.05),
            };
            let sway = panel_sway(&panel, 40.0).expect("every study cell fits");
            for (got, want, what) in [
                (sway.drag_over_pixel, drag, "drag"),
                (sway.tip_over_limit, tip, "tip"),
            ] {
                worst = worst.max((got - want).abs() / want);
                let ill_conditioned = n == 4 && (radius - 0.4).abs() < 1.0e-12;
                let tolerance = if ill_conditioned { 2.0e-5 } else { 1.0e-7 };
                assert!(
                    (got - want).abs() <= tolerance * want,
                    "r {radius} gap {gap} λ {lambda} n {n}: {what} {got} against the study's {want}"
                );
            }
            checked += 1;
        }
        assert_eq!(checked, 84, "every cell of both study tables is checked");
        assert!(worst > 0.0, "the comparison read real numbers");
    }

    /// A small deterministic generator, so the random panels are the same on
    /// every machine.
    struct Lcg(u64);

    impl Lcg {
        #[expect(
            clippy::cast_precision_loss,
            reason = "the top 53 bits of a u64 are exact in f64"
        )]
        fn unit(&mut self) -> f64 {
            self.0 = self
                .0
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            (self.0 >> 11) as f64 / (1_u64 << 53) as f64
        }

        fn between(&mut self, low: f64, high: f64) -> f64 {
            low + (high - low) * self.unit()
        }

        /// A whole number below `count`.
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "a small positive count"
        )]
        fn below(&mut self, count: u32) -> usize {
            (self.unit() * f64::from(count)) as usize
        }
    }

    /// A random panel: unequal posts, 1 to 8 bands at random places (the
    /// ends may land a few micrometres from a mesh level or from each other,
    /// so the node merge is exercised), diagonals of random radius.
    fn random_panel(rng: &mut Lcg) -> Panel {
        let post = rng.between(0.08, 0.8);
        let height = rng.between(2.0, 25.0);
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "a small positive count"
        )]
        let count = 1 + (rng.unit() * 8.0) as usize;
        let mut cuts: Vec<f64> = (0..2 * count).map(|_| rng.between(0.0, height)).collect();
        cuts.sort_by(f64::total_cmp);
        let brace = rng.between(0.05, post);
        Panel {
            post_radius_mm: post,
            partner_radius_mm: rng.between(0.08, 0.8),
            gap_mm: rng.between(0.4, 6.0),
            height_mm: height,
            bands: cuts
                .chunks(2)
                .map(|pair| PanelBand {
                    bottom_mm: pair[0],
                    top_mm: pair[1],
                    brace_radius_mm: brace,
                })
                .collect(),
        }
    }

    /// **The fast drag is the full sway's drag, to the last bit.** The
    /// planar solve does the in-plane arithmetic of the full solve in the
    /// same order, so every cell of both study tables and 1,500 random panels
    /// (the same two outcomes: a number with the same bits, or none) agree
    /// exactly, not to a tolerance. Sabotage (2026-10-04): form the planar
    /// stiffness's terms as `(from * to) * k` instead of `(from * k) * to`, a
    /// reordering that is the same number to 1e-16, and this test, its
    /// stiffness twin in `frame` and `the_fast_count_is_the_full_sways_count`
    /// all fail while the study table (tolerance 1e-7) still passes.
    #[test]
    fn the_drag_alone_is_the_drags_in_the_full_sway_to_the_last_bit() {
        let tilt = 57.3_f64.to_radians().tan();
        let mut panels: Vec<(Panel, f64)> = STUDY
            .iter()
            .map(|&(radius, gap, lambda, n, _, _)| {
                let height = lambda * 40.0 * radius;
                (
                    Panel {
                        post_radius_mm: radius,
                        partner_radius_mm: radius,
                        gap_mm: gap,
                        height_mm: height,
                        bands: study_bands(height, n, gap / tilt, gap, 40.0, 0.05),
                    },
                    40.0,
                )
            })
            .collect();
        let mut rng = Lcg(0x5eed_ba4d_0000_0001);
        for _ in 0..1_500 {
            let ratio = rng.between(20.0, 60.0);
            panels.push((random_panel(&mut rng), ratio));
        }
        let (mut some, mut none) = (0_usize, 0_usize);
        for (panel, ratio) in &panels {
            let full = panel_sway(panel, *ratio).map(|sway| sway.drag_over_pixel.to_bits());
            let fast = panel_drag(panel, *ratio).map(f64::to_bits);
            assert_eq!(fast, full, "{panel:?} at S {ratio}");
            if full.is_some() {
                some += 1;
            } else {
                none += 1;
            }
        }
        assert!(some > 1_000, "the comparison read real numbers: {some} solved");
        assert!(none > 0, "and some panels the two both refuse: {none}");
    }

    /// **The fast count is the full sway's count.** `count_bands` against
    /// the loop as it stood before the lane ([`count_bands_by_full_sway`]),
    /// on the study's own layout for 1 to `fit` bands over random pairs,
    /// starting from 1 to 4: the same bands, or the same ceiling with the
    /// same most bands, and the same sway to the last bit.
    #[test]
    fn the_fast_count_is_the_full_sways_count() {
        let tilt = 57.3_f64.to_radians().tan();
        let mut rng = Lcg(0x5eed_ba4d_0000_0002);
        let (mut holds, mut past) = (0_usize, 0_usize);
        for _ in 0..400 {
            let post = rng.between(0.1, 0.6);
            let gap = rng.between(0.5, 5.0);
            let ratio = rng.between(25.0, 45.0);
            let height = ratio * post * rng.between(1.3, 4.0);
            let fit = 1 + u32::try_from(rng.below(10)).expect("below ten");
            let from = 1 + rng.below(4);
            let panel = Panel {
                post_radius_mm: post,
                partner_radius_mm: rng.between(0.1, 0.6),
                gap_mm: gap,
                height_mm: height,
                bands: Vec::new(),
            };
            let layout = |n: usize| {
                u32::try_from(n)
                    .ok()
                    .filter(|&n| n <= fit)
                    .map(|n| study_bands(height, n, gap / tilt, gap, ratio, 0.05))
            };
            let fast = count_bands(&panel, ratio, from, &layout);
            let full = count_bands_by_full_sway(&panel, ratio, from, &layout);
            assert_eq!(fast, full, "{panel:?} from {from} fit {fit}");
            match fast {
                BandCount::Holds { .. } => holds += 1,
                BandCount::PastCeiling { .. } => past += 1,
            }
        }
        assert!(holds > 20 && past > 20, "both outcomes read: {holds} hold, {past} past");
    }

    /// The study's headline, read off the same table by the same rule the
    /// engine uses (the fewest bands whose drag sway is within one pixel):
    /// 2 bands at twice the limit, 3 at 2.5 times, 5 (thin pair) and 6 (mid
    /// pair) at 3 times; none of 1-6 at 4 times.
    #[test]
    fn the_fewest_bands_are_the_studys() {
        let fewest = |radius: f64, lambda: f64| {
            STUDY
                .iter()
                .filter(|cell| {
                    (cell.0 - radius).abs() < 1.0e-12
                        && (cell.2 - lambda).abs() < 1.0e-12
                        && cell.4 <= 1.0
                })
                .map(|cell| cell.3)
                .min()
        };
        assert_eq!(fewest(0.15, 2.0), Some(2));
        assert_eq!(fewest(0.15, 2.5), Some(3));
        assert_eq!(fewest(0.15, 3.0), Some(5));
        assert_eq!(fewest(0.4, 3.0), Some(6));
        assert_eq!(fewest(0.15, 4.0), None);
        assert_eq!(fewest(0.4, 1.75), Some(2));
    }
}
