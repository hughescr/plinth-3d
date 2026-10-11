//! The engine log and `poses.tsv`, read as text. Nothing here is trusted over
//! the emitted bytes: it feeds the pose class (the committed transform), the H5
//! log invariants, and N5's sway value, and it nowhere decides whether the file
//! is physically sound.
//!
//! Every value is an `Option`. `None` means "the phrase is not in the log", and
//! a comparison that needs it then reports INDETERMINATE or "not logged"; it
//! never reads a missing value as zero.

use std::path::{Path, PathBuf};

/// The numbers H5 and N5 read from one engine log.
#[derive(Clone, Debug, Default)]
pub struct LogFacts {
    pub path: Option<PathBuf>,
    /// `load gap (injected - plate reaction - uncarried) X mm2` (smallest seen).
    pub load_gap_mm2: Option<f64>,
    /// Sum of every `N unrooted / M invalid` pair printed after a pass.
    pub unrooted_or_invalid: Option<u64>,
    /// Sum of every `N newly overloaded members`.
    pub newly_overloaded: Option<u64>,
    /// `slenderness merges refused because the arm carries someone N`.
    pub slenderness_refused: Option<u64>,
    /// `on the emitted field A graded, N still cut` (lip seats).
    pub lip_still_cut: Option<u64>,
    /// `lip-seat bites on headed arms N`.
    pub lip_bites_headed: Option<u64>,
    /// Lines `WARNING member ... final gap`.
    pub final_gap_warnings: Option<u64>,
    /// `N layers round-bound-exhausted`.
    pub insertion_rounds_exhausted: Option<u64>,
    /// `route non-convergences N` or `N routing non-convergences`.
    pub route_nonconvergences: Option<u64>,
    /// `merge_order_cycles N` (printed by no known log yet).
    pub merge_order_cycles: Option<u64>,
    /// `worst tied-post sway X px` (largest seen).
    pub tied_post_sway_px: Option<f64>,
    /// `slenderness lane: S = N`.
    pub slenderness_s: Option<f64>,
    /// The text after `rotation committed:` in the log, if any.
    pub rotation_text: Option<String>,
}

fn is_num_char(c: char) -> bool {
    c.is_ascii_digit() || c == '.' || c == '-' || c == '+' || c == 'e' || c == 'E'
}

/// The number that ends just before byte `pos` (spaces skipped), if any.
fn number_before(s: &str, pos: usize) -> Option<f64> {
    let head = s[..pos].trim_end();
    let start = head
        .char_indices()
        .rev()
        .take_while(|(_, c)| is_num_char(*c))
        .last()
        .map(|(i, _)| i)?;
    // `e` alone, or a word ending in e, is not a number: require a digit.
    let tok = &head[start..];
    if !tok.chars().any(|c| c.is_ascii_digit()) {
        return None;
    }
    // A number glued to a letter before it ("arm33") is not a standalone count.
    if let Some(prev) = head[..start].chars().next_back() {
        if prev.is_ascii_alphanumeric() || prev == '_' {
            return None;
        }
    }
    tok.parse::<f64>().ok()
}

/// The number that starts at byte `pos` (spaces and `=`/`:` skipped), if any.
fn number_after(s: &str, pos: usize) -> Option<f64> {
    let tail = s[pos..].trim_start_matches([' ', '=', ':']);
    let end = tail
        .char_indices()
        .take_while(|(_, c)| is_num_char(*c))
        .last()
        .map(|(i, c)| i + c.len_utf8())?;
    let tok = &tail[..end];
    let tok = tok.trim_end_matches(['e', 'E', '-', '+', '.']);
    tok.parse::<f64>().ok()
}

fn as_count(v: f64) -> Option<u64> {
    (v.is_finite() && v >= 0.0 && v.fract() == 0.0).then_some(v as u64)
}

/// Every byte offset where `needle` occurs in `hay`.
fn find_all(hay: &str, needle: &str) -> Vec<usize> {
    let mut out = Vec::new();
    let mut from = 0;
    while let Some(i) = hay[from..].find(needle) {
        out.push(from + i);
        from += i + needle.len().max(1);
        if from >= hay.len() {
            break;
        }
    }
    out
}

fn add(slot: &mut Option<u64>, v: u64) {
    *slot = Some(slot.unwrap_or(0) + v);
}
fn max_u(slot: &mut Option<u64>, v: u64) {
    *slot = Some(slot.map_or(v, |o| o.max(v)));
}

impl LogFacts {
    /// Parse the whole text of one log.
    #[must_use]
    pub fn parse(text: &str, path: Option<PathBuf>) -> Self {
        let mut f = Self {
            path,
            ..Self::default()
        };
        let mut gap_warn = 0u64;
        for line in text.lines() {
            for p in find_all(line, "load gap (injected - plate reaction - uncarried)") {
                let end = p + "load gap (injected - plate reaction - uncarried)".len();
                if let Some(v) = number_after(line, end) {
                    f.load_gap_mm2 = Some(f.load_gap_mm2.map_or(v, |o| o.min(v)));
                }
            }
            for p in find_all(line, " unrooted / ") {
                let n = number_before(line, p).and_then(as_count);
                let m = number_after(line, p + " unrooted / ".len()).and_then(as_count);
                if let (Some(n), Some(m)) = (n, m) {
                    if line[p..].contains(" invalid") {
                        add(&mut f.unrooted_or_invalid, n + m);
                    }
                }
            }
            for p in find_all(line, " newly overloaded members") {
                if let Some(n) = number_before(line, p).and_then(as_count) {
                    add(&mut f.newly_overloaded, n);
                }
            }
            for p in find_all(line, "slenderness merges refused because the arm carries someone") {
                let end = p + "slenderness merges refused because the arm carries someone".len();
                if let Some(n) = number_after(line, end).and_then(as_count) {
                    max_u(&mut f.slenderness_refused, n);
                }
            }
            for p in find_all(line, "graded, ") {
                let after = &line[p + "graded, ".len()..];
                if let Some(rest) = after.find(" still cut") {
                    let head = &after[..rest];
                    if let Ok(n) = head.trim().parse::<u64>() {
                        max_u(&mut f.lip_still_cut, n);
                    }
                }
            }
            for p in find_all(line, "lip-seat bites on headed arms") {
                let end = p + "lip-seat bites on headed arms".len();
                if let Some(n) = number_after(line, end).and_then(as_count) {
                    max_u(&mut f.lip_bites_headed, n);
                }
            }
            if line.contains("WARNING member ") && line.contains("final gap") {
                gap_warn += 1;
            }
            for p in find_all(line, " layers round-bound-exhausted") {
                if let Some(n) = number_before(line, p).and_then(as_count) {
                    max_u(&mut f.insertion_rounds_exhausted, n);
                }
            }
            for p in find_all(line, "route non-convergences") {
                let end = p + "route non-convergences".len();
                if let Some(n) = number_after(line, end).and_then(as_count) {
                    max_u(&mut f.route_nonconvergences, n);
                }
            }
            for p in find_all(line, " routing non-convergences") {
                if let Some(n) = number_before(line, p).and_then(as_count) {
                    max_u(&mut f.route_nonconvergences, n);
                }
            }
            for key in ["merge_order_cycles", "merge order cycles"] {
                for p in find_all(line, key) {
                    if let Some(n) = number_after(line, p + key.len()).and_then(as_count) {
                        max_u(&mut f.merge_order_cycles, n);
                    }
                }
            }
            for p in find_all(line, "worst tied-post sway") {
                let end = p + "worst tied-post sway".len();
                if let Some(v) = number_after(line, end) {
                    f.tied_post_sway_px = Some(f.tied_post_sway_px.map_or(v, |o| o.max(v)));
                }
            }
            if line.contains("slenderness lane:") {
                if let Some(p) = line.find("S = ") {
                    if let Some(v) = number_after(line, p + 4) {
                        f.slenderness_s = Some(v);
                    }
                }
            }
            if f.rotation_text.is_none() {
                if let Some(p) = line.find("rotation committed:") {
                    f.rotation_text = Some(line[p + "rotation committed:".len()..].trim().to_string());
                }
            }
        }
        if text.contains("WARNING") || text.contains("stage: model") {
            f.final_gap_warnings = Some(gap_warn);
        }
        f
    }

    #[must_use]
    pub fn read(path: &Path) -> Option<Self> {
        let text = std::fs::read(path).ok()?;
        Some(Self::parse(&String::from_utf8_lossy(&text), Some(path.to_path_buf())))
    }
}

// ---------------------------------------------------------------------------
// The committed rotation
// ---------------------------------------------------------------------------

/// A committed rotation as the CLI prints it.
#[derive(Clone, Debug, PartialEq)]
pub enum Rot {
    Identity,
    /// `X 30.000 deg`
    AxisDeg(char, f64),
    /// `searched up-direction (a, b, c) in the model frame`
    Up([f64; 3]),
    /// Anything else, compared as text.
    Opaque(String),
}

fn parse_triplet(s: &str) -> Option<[f64; 3]> {
    let open = s.find('(')?;
    let close = s[open..].find(')')? + open;
    let parts: Vec<f64> = s[open + 1..close]
        .split(',')
        .filter_map(|t| t.trim().parse::<f64>().ok())
        .collect();
    (parts.len() == 3).then(|| [parts[0], parts[1], parts[2]])
}

impl Rot {
    /// Parse the text after `rotation committed:`.
    #[must_use]
    pub fn parse(text: &str) -> Self {
        let t = text.trim();
        if t.starts_with("identity") {
            return Self::Identity;
        }
        if let Some(rest) = t.strip_prefix("searched up-direction") {
            if let Some(v) = parse_triplet(rest) {
                return Self::Up(v);
            }
        }
        let mut it = t.split_whitespace();
        if let (Some(a), Some(d), Some(unit)) = (it.next(), it.next(), it.next()) {
            if a.len() == 1 && "XYZxyz".contains(a) && unit.starts_with("deg") {
                if let Ok(deg) = d.parse::<f64>() {
                    return Self::AxisDeg(a.chars().next().unwrap_or('X').to_ascii_uppercase(), deg);
                }
            }
        }
        Self::Opaque(t.to_string())
    }

    /// The angle between two committed rotations in radians, or `None` when
    /// they are of kinds that cannot be compared (then they count as different
    /// unless the texts are equal).
    #[must_use]
    pub fn distance_rad(&self, other: &Self) -> f64 {
        fn ang(a: [f64; 3], b: [f64; 3]) -> f64 {
            let na = (a[0] * a[0] + a[1] * a[1] + a[2] * a[2]).sqrt();
            let nb = (b[0] * b[0] + b[1] * b[1] + b[2] * b[2]).sqrt();
            if na == 0.0 || nb == 0.0 {
                return std::f64::consts::PI;
            }
            let (a, b) = ([a[0] / na, a[1] / na, a[2] / na], [b[0] / nb, b[1] / nb, b[2] / nb]);
            let cr = [
                a[1] * b[2] - a[2] * b[1],
                a[2] * b[0] - a[0] * b[2],
                a[0] * b[1] - a[1] * b[0],
            ];
            let s = (cr[0] * cr[0] + cr[1] * cr[1] + cr[2] * cr[2]).sqrt();
            let d = a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
            s.atan2(d)
        }
        let up_of = |r: &Self| match r {
            Self::Identity => Some([0.0, 0.0, 1.0]),
            Self::Up(v) => Some(*v),
            _ => None,
        };
        match (self, other) {
            (Self::AxisDeg(a, x), Self::AxisDeg(b, y)) if a == b => (x - y).abs().to_radians(),
            (Self::AxisDeg(_, d), Self::Identity) | (Self::Identity, Self::AxisDeg(_, d)) => {
                d.abs().to_radians()
            }
            (Self::Opaque(a), Self::Opaque(b)) => {
                if a == b {
                    0.0
                } else {
                    1.0
                }
            }
            _ => match (up_of(self), up_of(other)) {
                (Some(a), Some(b)) => ang(a, b),
                _ => 1.0,
            },
        }
    }
}

/// What the run said about the committed pose.
#[derive(Clone, Debug)]
pub struct PoseSpec {
    /// `poses.tsv` or `log`.
    pub source: &'static str,
    pub how: String,
    pub committed: String,
    pub rot: Rot,
}

/// Read the row of `poses.tsv` for `model` (or the only row).
#[must_use]
pub fn read_poses(path: &Path, model_stem: &str) -> Option<PoseSpec> {
    let text = std::fs::read_to_string(path).ok()?;
    let mut rows: Vec<(String, String, String)> = Vec::new();
    for (i, line) in text.lines().enumerate() {
        if i == 0 && line.starts_with("model\t") {
            continue;
        }
        let mut c = line.splitn(3, '\t');
        if let (Some(m), Some(h), Some(r)) = (c.next(), c.next(), c.next()) {
            rows.push((m.to_string(), h.to_string(), r.to_string()));
        }
    }
    let norm = |s: &str| s.replace([' ', '_'], "").to_lowercase();
    let want = norm(model_stem);
    let pick = rows
        .iter()
        .find(|r| norm(&r.0) == want)
        .or_else(|| (rows.len() == 1).then(|| &rows[0]))?;
    let committed = pick
        .2
        .find("rotation committed:")
        .map_or(pick.2.as_str(), |p| &pick.2[p + "rotation committed:".len()..])
        .trim()
        .to_string();
    Some(PoseSpec {
        source: "poses.tsv",
        how: pick.1.clone(),
        rot: Rot::parse(&committed),
        committed,
    })
}

/// Where a file's sidecar text lives, by the run recipe's layout:
/// `<out>/<model>/<model>.chitubox`, `<out>/<model>.log`, `<out>/poses.tsv`.
#[must_use]
pub fn discover(chitubox: &Path) -> (Option<PathBuf>, Option<PathBuf>) {
    let stem = chitubox
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    let dir = chitubox.parent().map(Path::to_path_buf).unwrap_or_default();
    let up = dir.parent().map(Path::to_path_buf);
    let mut logs = vec![dir.join(format!("{stem}.log"))];
    let mut poses = vec![dir.join("poses.tsv")];
    if let Some(u) = &up {
        logs.push(u.join(format!("{stem}.log")));
        poses.push(u.join("poses.tsv"));
    }
    (
        logs.into_iter().find(|p| p.is_file()),
        poses.into_iter().find(|p| p.is_file()),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers_are_read_next_to_their_phrase() {
        let t = "x; load gap (injected - plate reaction - uncarried) -0.0100 mm2, of which\n\
                 After each pass: up-sweep and down-sweep 0 unrooted / 2 invalid; removal 1 unrooted / 0 invalid; z\n\
                 slenderness merges refused because the arm carries someone 4, shafts\n\
                 on the emitted field 34 graded, 4 still cut\n\
                 lip-seat bites on headed arms 0 (must be 0)\n\
                 (1 insertion rounds, 3 layers round-bound-exhausted [UNDER]\n\
                 chamfer non-convergences 0; route non-convergences 2; arms\n\
                 0 routing non-convergences (they lose load)\n\
                 worst tied-post sway 0.918 px); runs refused\n\
                 [sweep M] WARNING member 3 (Arm) stands 0.2 mm from the model, inside the 0.300 mm final gap\n\
                 [sweep M] slenderness lane: S = 40.0 in force; brace reach\n\
                 orientation  original +Z is 30.0 deg off; rotation committed: X 30.000 deg\n";
        let f = LogFacts::parse(t, None);
        assert_eq!(f.load_gap_mm2, Some(-0.01));
        assert_eq!(f.unrooted_or_invalid, Some(3));
        assert_eq!(f.slenderness_refused, Some(4));
        assert_eq!(f.lip_still_cut, Some(4));
        assert_eq!(f.lip_bites_headed, Some(0));
        assert_eq!(f.insertion_rounds_exhausted, Some(3));
        assert_eq!(f.route_nonconvergences, Some(2));
        assert_eq!(f.tied_post_sway_px, Some(0.918));
        assert_eq!(f.final_gap_warnings, Some(1));
        assert_eq!(f.slenderness_s, Some(40.0));
        assert_eq!(f.merge_order_cycles, None, "a phrase the log never prints stays None");
        assert_eq!(f.rotation_text.as_deref(), Some("X 30.000 deg"));
    }

    #[test]
    fn rotations_parse_and_measure() {
        let a = Rot::parse("searched up-direction (-0.9949, -0.0801, -0.0605) in the model frame");
        let b = Rot::parse("searched up-direction (-0.9949, -0.0801, -0.0605) in the model frame");
        assert_eq!(a.distance_rad(&b), 0.0);
        let c = Rot::parse("searched up-direction (+0.9987, +0.0460, -0.0229) in the model frame");
        assert!(a.distance_rad(&c) > 1.0);
        assert_eq!(Rot::parse("identity").distance_rad(&Rot::Identity), 0.0);
        let x = Rot::parse("X 30.000 deg");
        assert!((x.distance_rad(&Rot::Identity) - 30f64.to_radians()).abs() < 1e-12);
        assert!(Rot::parse("Z 180.000 deg").distance_rad(&Rot::Identity) > 3.0);
    }
}
