//! CAL-2 plate map: a ChituBox-style top view (front edge at the bottom), drawn from the
//! written file's own body footprints and head positions, with every coupon labelled.
use crate::{
    Result,
    cal2_spec::{Kind, Variant},
    plate,
};
use std::fmt::Write as _;

const SCALE: f64 = 10.3;
const CX: f64 = 1200.0;
const CY: f64 = 770.0;

#[derive(Debug, Clone)]
pub struct MapCoupon {
    pub number: u32,
    pub row: String,
    pub kind: Kind,
    pub variant: Variant,
    pub neck_mm: f64,
    pub heads: usize,
    pub ratio: f64,
    /// Footprint (body and supports) as lo and hi corners on the plate, mm.
    pub foot: [[f64; 2]; 2],
    /// Body footprint, mm.
    pub body: [[f64; 2]; 2],
    /// Inner radius of a ring body, mm, read from the written body.
    pub ring_inner: Option<f64>,
    /// Footprint of the stem above the full layers (short hold), mm.
    pub stem: Option<[[f64; 2]; 2]>,
    pub sites: Vec<[f64; 2]>,
    pub control: bool,
}

fn px(p: [f64; 2]) -> [f64; 2] {
    [CX + p[0] * SCALE, CY - p[1] * SCALE]
}

fn colour(c: &MapCoupon) -> &'static str {
    match (c.kind, (c.neck_mm * 100.0).round() as i64) {
        (Kind::Witness, _) => "#c53030",
        (_, 40) => "#2b6cb0",
        (_, 55) => "#2f855a",
        (_, 80) => "#dd6b20",
        _ => "#805ad5",
    }
}

fn coupon(s: &mut String, c: &MapCoupon) -> Result<()> {
    let col = colour(c);
    let [x0, y1] = px(c.body[0]);
    let [x1, y0] = px(c.body[1]);
    let dash = if c.control {
        " stroke-dasharray=\"7 4\""
    } else {
        ""
    };
    if let Some(inner) = c.ring_inner {
        let (cx, cy) = ((x0 + x1) / 2.0, (y0 + y1) / 2.0);
        let outer = (x1 - x0) / 2.0;
        let inner = inner * SCALE;
        writeln!(
            s,
            "<path d=\"M{:.1} {cy:.1} a{outer:.1} {outer:.1} 0 1 0 {:.1} 0 a{outer:.1} {outer:.1} 0 1 0 {:.1} 0 M{:.1} {cy:.1} a{inner:.1} {inner:.1} 0 1 0 {:.1} 0 a{inner:.1} {inner:.1} 0 1 0 {:.1} 0\" fill=\"{col}\" fill-opacity=\"0.22\" fill-rule=\"evenodd\" stroke=\"{col}\" stroke-width=\"3\"{dash}/>",
            cx - outer,
            2.0 * outer,
            -2.0 * outer,
            cx - inner,
            2.0 * inner,
            -2.0 * inner
        )?;
    } else {
        writeln!(
            s,
            "<rect x=\"{x0:.1}\" y=\"{y0:.1}\" width=\"{:.1}\" height=\"{:.1}\" fill=\"{col}\" fill-opacity=\"0.22\" stroke=\"{col}\" stroke-width=\"3\"{dash}/>",
            x1 - x0,
            y1 - y0
        )?;
    }
    if let Some(stem) = c.stem {
        let [sx0, sy1] = px(stem[0]);
        let [sx1, sy0] = px(stem[1]);
        writeln!(
            s,
            "<rect x=\"{sx0:.1}\" y=\"{sy0:.1}\" width=\"{:.1}\" height=\"{:.1}\" fill=\"none\" stroke=\"{col}\" stroke-width=\"2\" stroke-dasharray=\"3 3\"/>",
            sx1 - sx0,
            sy1 - sy0
        )?;
    }
    for &site in &c.sites {
        let [x, y] = px(site);
        writeln!(
            s,
            "<circle cx=\"{x:.1}\" cy=\"{y:.1}\" r=\"{:.1}\" fill=\"none\" stroke=\"#888\" stroke-width=\"1\"/><circle cx=\"{x:.1}\" cy=\"{y:.1}\" r=\"3.2\" fill=\"#171717\"/>",
            1.1 * SCALE
        )?;
    }
    // The tab is at the front-left corner of the top face: the stem's, when there is one.
    let top = c.stem.unwrap_or(c.body);
    if c.ring_inner.is_none()
        && crate::cal2_body::Features::fit(top[1][0] - top[0][0], top[1][1] - top[0][1])
    {
        let t = crate::cal2_body::TAB * SCALE;
        let [tx, ty] = px(top[0]);
        writeln!(
            s,
            "<rect x=\"{tx:.1}\" y=\"{:.1}\" width=\"{t:.1}\" height=\"{t:.1}\" fill=\"#171717\"/>",
            ty - t
        )?;
    }
    let mid = px([(c.foot[0][0] + c.foot[1][0]) / 2.0, c.foot[0][1]])[0];
    let base = px([0.0, c.foot[0][1]])[1];
    let kind = match c.kind {
        Kind::Column => " col",
        Kind::Witness => " witness",
        Kind::Head => "",
    };
    let variant = if c.kind == Kind::Witness {
        "formation witness"
    } else if c.control {
        "4-head control"
    } else {
        c.variant.name()
    };
    writeln!(
        s,
        "<text x=\"{mid:.1}\" y=\"{:.1}\" text-anchor=\"middle\" font-size=\"15\" font-weight=\"bold\" fill=\"{col}\">C{:02} {:.2}x{}{kind} R{:.0}</text><text x=\"{mid:.1}\" y=\"{:.1}\" text-anchor=\"middle\" font-size=\"15\" fill=\"#171717\">{variant} ({})</text>",
        base + 17.0,
        c.number,
        c.neck_mm,
        c.heads,
        c.ratio,
        base + 34.0,
        c.row,
    )?;
    Ok(())
}

/// The map as SVG text. `cut` is the human-readable list of dropped coupons.
///
/// # Errors
/// Only a formatting failure.
pub fn svg(coupons: &[MapCoupon], cut: &str, seed: u64) -> Result<String> {
    let mut s = String::from(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"2400\" height=\"1720\" viewBox=\"0 0 2400 1720\"><rect width=\"2400\" height=\"1720\" fill=\"white\"/><g font-family=\"Arial, sans-serif\" fill=\"#171717\">",
    );
    writeln!(
        s,
        "<text x=\"100\" y=\"60\" font-size=\"44\" font-weight=\"bold\">CAL-2 calibration plate: NOT a print candidate</text><text x=\"100\" y=\"104\" font-size=\"26\">Top view as ChituBox shows it. {} coupons, each labelled number, neck mm x heads, R (peak layer ratio against the minimum section), variant (row). Shuffle seed {seed}.</text>",
        coupons.len()
    )?;
    let [x0, y0] = px([-plate::BUILD[0] / 2.0, plate::BUILD[1] / 2.0]);
    writeln!(
        s,
        "<rect x=\"{x0:.1}\" y=\"{y0:.1}\" width=\"{:.1}\" height=\"{:.1}\" fill=\"#fafafa\" stroke=\"#777\" stroke-width=\"2\"/>",
        plate::BUILD[0] * SCALE,
        plate::BUILD[1] * SCALE
    )?;
    for c in coupons {
        coupon(&mut s, c)?;
    }
    let legend = [
        ("#2b6cb0", "0.40 mm necks, 16 heads"),
        (
            "#2f855a",
            "0.55 mm necks: 9 heads (standard, short/long hold, late load), strip 8, ring 12; dashed = 4-head control",
        ),
        ("#dd6b20", "0.80 mm necks, 4 heads"),
        ("#805ad5", "1.20 mm plain waisted columns, 4 per coupon"),
        ("#c53030", "formation witness: one head, small body, R9"),
    ];
    for (i, (col, text)) in legend.iter().enumerate() {
        let y = 1456.0 + 28.0 * i as f64;
        writeln!(
            s,
            "<rect x=\"100\" y=\"{:.0}\" width=\"26\" height=\"20\" fill=\"{col}\" fill-opacity=\"0.22\" stroke=\"{col}\" stroke-width=\"3\"/><text x=\"140\" y=\"{:.0}\" font-size=\"22\">{text}</text>",
            y - 16.0,
            y
        )?;
    }
    writeln!(
        s,
        "<text x=\"1200\" y=\"{:.0}\" text-anchor=\"middle\" font-size=\"30\" font-weight=\"bold\">BACK EDGE</text><text x=\"1500\" y=\"1470\" text-anchor=\"middle\" font-size=\"36\" font-weight=\"bold\">FRONT EDGE: peels first (-Y in ChituBox)</text><path d=\"M1500 1482 L1500 1520 M1486 1505 L1500 1520 L1514 1505\" fill=\"none\" stroke=\"#171717\" stroke-width=\"4\"/><text x=\"100\" y=\"1630\" font-size=\"22\">Black square = orientation tab, front-left corner of the top face. Raised number on the top face = coupon number (none on rings and small bodies).</text><text x=\"100\" y=\"1660\" font-size=\"22\">Circles: raft pads (2.2 mm) with one black dot per head. Outline = body at full load, to scale; dashed inner square = stem of a short hold.</text><text x=\"100\" y=\"1690\" font-size=\"22\">{}</text>",
        y0 - 12.0,
        cut
    )?;
    s.push_str("</g></svg>");
    Ok(s)
}
