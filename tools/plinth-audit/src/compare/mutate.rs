//! Planted changes for the gate's own tests and for the G3 pairs: read a
//! `.chitubox`, change one thing, write it back. A test instrument, like
//! [`crate::synth`], which it writes through; it validates nothing, so the
//! change you plant stays planted.
//!
//! The noise model (`jitter`) is the calibration prototype's: noise keyed on
//! the vertex's own f32 bits, so welded and coincident points move together
//! and a closed mesh stays closed.

use crate::reader::{ChituboxFile, RecordKind};
use crate::synth::{SynthFile, SynthModel, SynthRecord};
use crate::vec3::{self as v, V3};

fn mix(mut z: u64) -> u64 {
    z = z.wrapping_add(0x9E37_79B9_7F4A_7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// Uniform noise in `[-e, e]` keyed on the point's f32 bits and the axis.
#[must_use]
pub fn vnoise(p: [f32; 3], axis: u64, e: f64) -> f64 {
    let h = mix(u64::from(p[0].to_bits()) | (u64::from(p[1].to_bits()) << 32))
        ^ mix(u64::from(p[2].to_bits()).wrapping_add(axis.wrapping_mul(0x1234_5678_9ABC)));
    ((h >> 11) as f64 / (1u64 << 53) as f64 * 2.0 - 1.0) * e
}

/// Round to the nearest IEEE half-precision value (ties to even), back as f32.
#[must_use]
pub fn round_f16(x: f32) -> f32 {
    if !x.is_finite() || x == 0.0 {
        return x;
    }
    let a = x.abs();
    if a >= 65520.0 {
        return f32::INFINITY.copysign(x);
    }
    let step = if a < 6.103_515_6e-5 { 2f32.powi(-24) } else { 2f32.powi(a.log2().floor() as i32 - 10) };
    ((a / step).round_ties_even() * step).copysign(x)
}

/// Move an f32 by -1, 0 or +1 units in the last place, keyed on its own bits.
#[must_use]
pub fn ulp_step(x: f32, key: u64) -> f32 {
    // Zero (either sign), NaN and infinity have no neighbour "one unit in the last place" away that
    // arithmetic would produce. Stepping +0.0 down by bits gave 0xFFFF_FFFF, a NaN (found on real files,
    // which hold exact zeros), so these values stay put.
    if x == 0.0 || !x.is_finite() {
        return x;
    }
    let d =(mix(u64::from(x.to_bits()) ^ key.wrapping_mul(0x9E37)) % 3) as i64 - 1;
    f32::from_bits((i64::from(x.to_bits()) + d * if x < 0.0 { -1 } else { 1 }) as u32)
}

/// Parse the file into the writer's form (header values as parsed, meshes as stored).
#[must_use]
pub fn to_synth(file: &ChituboxFile) -> SynthFile {
    SynthFile {
        models: file
            .models
            .iter()
            .map(|m| SynthModel {
                name: m.name.clone(),
                translate_mm: m.translate_mm,
                mesh: m.model_soup.tris.clone(),
                records: m
                    .records
                    .iter()
                    .map(|r| SynthRecord {
                        kind: r.kind,
                        start_mm: r.start_mm,
                        end_mm: r.end_mm,
                        r_start: r.r_start,
                        r_end: r.r_end,
                        trailing: r.trailing,
                        soup: r.soup.tris.clone(),
                    })
                    .collect(),
            })
            .collect(),
        settings_size: 0,
        const16: None,
    }
}

/// One planted change.
#[derive(Clone, Debug)]
pub enum Mutation {
    /// Noise of up to `amp_mm` on model vertices, record soups and record headers.
    /// `seed` picks another noise draw (0 is the calibration prototype's own).
    Jitter { amp_mm: f64, model: bool, support: bool, radii: bool, seed: u64 },
    /// Noise on the design that stays a design: every support record's header ends and radii
    /// move by up to `amp_mm`, and the record's mesh is carried with them by the similarity map
    /// that takes the old header solid onto the new one (rotate, stretch along the axis, scale
    /// the radius). The mesh therefore still IS the header's solid, as an engine's own output
    /// is. `Jitter` moves header and mesh independently, which no engine can emit and which on
    /// a short fat member (0.02 mm long, 0.58 mm across) turns a 1 micron end move into a
    /// 4 degree axis turn and a 0.04 mm mesh-versus-header gap.
    JitterConsistent { amp_mm: f64, model: bool, support: bool, radii: bool, seed: u64 },
    /// Every coordinate moves by -1, 0 or +1 ulp (what f32 arithmetic would change).
    Ulp,
    /// The same with another draw: `ulp:<seed>`.
    UlpSeed(u64),
    /// Every coordinate and radius rounded through half precision.
    F16,
    /// Scale one record's emitted mesh about its own axis; the header is left alone.
    ShrinkMesh { record: usize, factor: f64 },
    /// The same for every support record.
    ShrinkAllMesh { factor: f64 },
    /// Scale one record's radii in header AND mesh.
    ScaleRecord { record: usize, factor: f64 },
    /// Delete records (all indices refer to the file as read).
    Remove { records: Vec<usize> },
    /// Translate a record (header and mesh).
    Move { record: usize, delta: V3 },
    /// Add a free-standing short cylinder: a floating piece.
    Float { at: V3, radius: f64 },
    /// Rotate the whole file (model and supports) by 180 degrees about the vertical axis.
    RotateZ180,
    /// Split every model triangle into three about its centroid (the surface is unchanged).
    SplitModel,
    /// Move the model alone by a vector (supports stay).
    ShiftModel { delta: V3 },
    /// Move model and supports together by a sub-voxel vector: the file is the same
    /// design on a lattice offset by `delta`. Consistent to the last bit, so the audit's
    /// exact tests see nothing; only the raster boundaries move.
    ShiftAll { delta: V3 },
}

fn map_tri(t: &mut [[f32; 3]; 3], mut f: impl FnMut(V3) -> V3) {
    for p in t.iter_mut() {
        let q = f([f64::from(p[0]), f64::from(p[1]), f64::from(p[2])]);
        *p = [q[0] as f32, q[1] as f32, q[2] as f32];
    }
}

fn scale_soup(rec: &mut SynthRecord, factor: f64, header_radii: bool) {
    let (a, b) = (rec.start_mm, rec.end_mm);
    let len = v::dist(a, b);
    let ball = matches!(rec.kind, RecordKind::ContactSphere | RecordKind::JointSphere);
    let centre = v::midpoint(a, b);
    let ax = if len > 0.0 { v::mul(v::sub(b, a), 1.0 / len) } else { [0.0, 0.0, 1.0] };
    for t in &mut rec.soup {
        map_tri(t, |p| {
            if ball {
                v::add(centre, v::mul(v::sub(p, centre), factor))
            } else {
                let d = v::sub(p, a);
                let along = v::dot(d, ax);
                let perp = v::sub(d, v::mul(ax, along));
                v::add(v::add(a, v::mul(ax, along)), v::mul(perp, factor))
            }
        });
    }
    if header_radii {
        rec.r_start *= factor;
        if !ball {
            rec.r_end *= factor;
        }
    }
}

fn rodrigues(vv: V3, k: V3, cos: f64, sin: f64) -> V3 {
    // v cos + (k x v) sin + k (k . v) (1 - cos)
    v::add(v::add(v::mul(vv, cos), v::mul(v::cross(k, vv), sin)), v::mul(k, v::dot(k, vv) * (1.0 - cos)))
}

/// Move one record's header ends and radii by noise and carry its mesh onto the new header
/// solid (see [`Mutation::JitterConsistent`]). Ends are keyed on their own f32 bits exactly as
/// in [`Mutation::Jitter`], so records that share an end still share it.
fn jitter_record_consistent(r: &mut SynthRecord, e: f64, sk: u64, radii: bool) {
    let raft = r.kind == RecordKind::RaftPadOrBridge;
    let (a0, b0, ra0, rb0) = (r.start_mm, r.end_mm, r.r_start, r.r_end);
    for end in [&mut r.start_mm, &mut r.end_mm] {
        let f = [end[0] as f32, end[1] as f32, end[2] as f32];
        for a in 0..3 {
            if raft && a == 2 {
                continue;
            }
            end[a] = f64::from((end[a] + vnoise(f, a as u64 + sk, e)) as f32);
        }
    }
    if radii && !raft {
        r.r_start = (r.r_start + vnoise([r.r_start as f32, 1.0, 2.0], 7 + sk, e)).max(0.0);
        r.r_end = (r.r_end + vnoise([r.r_end as f32, 1.0, 2.0], 7 + sk, e)).max(0.0);
    }
    let (a1, b1, ra1, rb1) = (r.start_mm, r.end_mm, r.r_start, r.r_end);
    if r.kind.is_ball() {
        let (c0, c1) = (v::midpoint(a0, b0), v::midpoint(a1, b1));
        let f = if ra0 > 1e-9 { ra1 / ra0 } else { 1.0 };
        for t in &mut r.soup {
            map_tri(t, |p| v::add(c1, v::mul(v::sub(p, c0), f)));
        }
        return;
    }
    let (len0, len1) = (v::dist(a0, b0), v::dist(a1, b1));
    if len0 <= 1e-9 || len1 <= 1e-9 {
        let d = v::sub(a1, a0);
        for t in &mut r.soup {
            map_tri(t, |p| v::add(p, d));
        }
        return;
    }
    let ax0 = v::mul(v::sub(b0, a0), 1.0 / len0);
    let ax1 = v::mul(v::sub(b1, a1), 1.0 / len1);
    let cr = v::cross(ax0, ax1);
    let sin = v::norm(cr);
    let cos = v::dot(ax0, ax1);
    let k = if sin > 1e-15 { v::mul(cr, 1.0 / sin) } else { [0.0, 0.0, 1.0] };
    let stretch = len1 / len0;
    for t in &mut r.soup {
        map_tri(t, |p| {
            let d = v::sub(p, a0);
            let along = v::dot(d, ax0);
            let perp = v::sub(d, v::mul(ax0, along));
            let tr = (along / len0).clamp(0.0, 1.0);
            let r0 = ra0 + (rb0 - ra0) * tr;
            let r1 = ra1 + (rb1 - ra1) * tr;
            let f = if r0 > 1e-9 { r1 / r0 } else { 1.0 };
            let perp1 = if sin > 1e-15 { rodrigues(v::mul(perp, f), k, cos, sin) } else { v::mul(perp, f) };
            v::add(v::add(a1, v::mul(ax1, along * stretch)), perp1)
        });
    }
}

/// Apply one change.
pub fn apply(s: &mut SynthFile, m: &Mutation) {
    match m {
        Mutation::JitterConsistent { amp_mm, model, support, radii, seed } => {
            let sk = seed.wrapping_mul(0x51_7C_C1);
            for md in &mut s.models {
                if *model {
                    for t in &mut md.mesh {
                        for p in t.iter_mut() {
                            let o = *p;
                            for a in 0..3 {
                                p[a] = (f64::from(o[a]) + vnoise(o, a as u64 + sk, *amp_mm)) as f32;
                            }
                        }
                    }
                }
                if *support {
                    for r in &mut md.records {
                        jitter_record_consistent(r, *amp_mm, sk, *radii);
                    }
                }
            }
        }
        Mutation::Jitter { amp_mm, model, support, radii, seed } => {
            let e = *amp_mm;
            // Seed 0 keys the axes 0, 1, 2 and 7 exactly as the prototype did.
            let sk = seed.wrapping_mul(0x51_7C_C1);
            for md in &mut s.models {
                if *model {
                    for t in &mut md.mesh {
                        for p in t.iter_mut() {
                            let o = *p;
                            for a in 0..3 {
                                p[a] = (f64::from(o[a]) + vnoise(o, a as u64 + sk, e)) as f32;
                            }
                        }
                    }
                }
                if *support {
                    // Noise keyed on the point's own f32 bits, so every record that shares an
                    // end moves that end identically and the chains stay joined: what a lane
                    // that computes in lower precision would emit. Mesh vertices move the same
                    // way. Radii are left alone unless asked for (`radii`), because the audit's
                    // three-way tip radius test is exact to 1e-6 mm.
                    for r in &mut md.records {
                        let raft = r.kind == RecordKind::RaftPadOrBridge;
                        for t in &mut r.soup {
                            for p in t.iter_mut() {
                                let o = *p;
                                for a in 0..3 {
                                    p[a] = (f64::from(o[a]) + vnoise(o, a as u64 + sk, e)) as f32;
                                }
                            }
                        }
                        for end in [&mut r.start_mm, &mut r.end_mm] {
                            let f = [end[0] as f32, end[1] as f32, end[2] as f32];
                            for a in 0..3 {
                                if raft && a == 2 {
                                    continue;
                                }
                                end[a] = f64::from((end[a] + vnoise(f, a as u64 + sk, e)) as f32);
                            }
                        }
                        if *radii && !raft {
                            r.r_start += vnoise([r.r_start as f32, 1.0, 2.0], 7 + sk, e);
                            r.r_end += vnoise([r.r_end as f32, 1.0, 2.0], 7 + sk, e);
                        }
                    }
                }
            }
        }
        Mutation::Ulp | Mutation::UlpSeed(_) => {
            let sk = if let Mutation::UlpSeed(s) = m { s.wrapping_mul(0x51_7C_C1) } else { 0 };
            for md in &mut s.models {
                for t in &mut md.mesh {
                    for p in t.iter_mut() {
                        for a in 0..3 {
                            p[a] = ulp_step(p[a], a as u64 + sk);
                        }
                    }
                }
                for r in &mut md.records {
                    for t in &mut r.soup {
                        for p in t.iter_mut() {
                            for a in 0..3 {
                                p[a] = ulp_step(p[a], a as u64 + sk);
                            }
                        }
                    }
                    for end in [&mut r.start_mm, &mut r.end_mm] {
                        for a in 0..3 {
                            end[a] = f64::from(ulp_step(end[a] as f32, a as u64 + sk));
                        }
                    }
                    r.r_start = f64::from(ulp_step(r.r_start as f32, 9 + sk));
                    r.r_end = f64::from(ulp_step(r.r_end as f32, 9 + sk));
                }
            }
        }
        Mutation::F16 => {
            for md in &mut s.models {
                for t in &mut md.mesh {
                    for p in t.iter_mut() {
                        for a in 0..3 {
                            p[a] = round_f16(p[a]);
                        }
                    }
                }
                for r in &mut md.records {
                    for t in &mut r.soup {
                        for p in t.iter_mut() {
                            for a in 0..3 {
                                p[a] = round_f16(p[a]);
                            }
                        }
                    }
                    for end in [&mut r.start_mm, &mut r.end_mm] {
                        for a in 0..3 {
                            end[a] = f64::from(round_f16(end[a] as f32));
                        }
                    }
                    r.r_start = f64::from(round_f16(r.r_start as f32));
                    r.r_end = f64::from(round_f16(r.r_end as f32));
                }
            }
        }
        Mutation::ShrinkMesh { record, factor } => {
            if let Some(r) = s.models.first_mut().and_then(|md| md.records.get_mut(*record)) {
                scale_soup(r, *factor, false);
            }
        }
        Mutation::ShrinkAllMesh { factor } => {
            for md in &mut s.models {
                for r in &mut md.records {
                    if r.kind != RecordKind::RaftPadOrBridge {
                        scale_soup(r, *factor, false);
                    }
                }
            }
        }
        Mutation::ScaleRecord { record, factor } => {
            if let Some(r) = s.models.first_mut().and_then(|md| md.records.get_mut(*record)) {
                scale_soup(r, *factor, true);
            }
        }
        Mutation::Remove { records } => {
            for md in &mut s.models {
                let mut i = 0usize;
                md.records.retain(|_| {
                    let keep = !records.contains(&i);
                    i += 1;
                    keep
                });
            }
        }
        Mutation::Move { record, delta } => {
            if let Some(r) = s.models.first_mut().and_then(|md| md.records.get_mut(*record)) {
                r.start_mm = v::add(r.start_mm, *delta);
                r.end_mm = v::add(r.end_mm, *delta);
                for t in &mut r.soup {
                    map_tri(t, |p| v::add(p, *delta));
                }
            }
        }
        Mutation::Float { at, radius } => {
            if let Some(md) = s.models.first_mut() {
                let end = [at[0], at[1], at[2] + 0.05];
                md.records.push(
                    SynthRecord::new(RecordKind::Member, *at, end, *radius, *radius).with_matching_soup(),
                );
            }
        }
        Mutation::RotateZ180 => {
            for md in &mut s.models {
                for t in &mut md.mesh {
                    map_tri(t, |p| [-p[0], -p[1], p[2]]);
                }
                for r in &mut md.records {
                    for t in &mut r.soup {
                        map_tri(t, |p| [-p[0], -p[1], p[2]]);
                    }
                    if r.kind == RecordKind::RaftPadOrBridge {
                        // Type-6 records keep their own plan layout; the base files have none.
                        continue;
                    }
                    r.start_mm = [-r.start_mm[0], -r.start_mm[1], r.start_mm[2]];
                    r.end_mm = [-r.end_mm[0], -r.end_mm[1], r.end_mm[2]];
                }
            }
        }
        Mutation::SplitModel => {
            for md in &mut s.models {
                let mut out = Vec::with_capacity(md.mesh.len() * 3);
                for t in &md.mesh {
                    let g = [
                        ((f64::from(t[0][0]) + f64::from(t[1][0]) + f64::from(t[2][0])) / 3.0) as f32,
                        ((f64::from(t[0][1]) + f64::from(t[1][1]) + f64::from(t[2][1])) / 3.0) as f32,
                        ((f64::from(t[0][2]) + f64::from(t[1][2]) + f64::from(t[2][2])) / 3.0) as f32,
                    ];
                    out.push([t[0], t[1], g]);
                    out.push([t[1], t[2], g]);
                    out.push([t[2], t[0], g]);
                }
                md.mesh = out;
            }
        }
        Mutation::ShiftModel { delta } => {
            for md in &mut s.models {
                for t in &mut md.mesh {
                    map_tri(t, |p| v::add(p, *delta));
                }
            }
        }
        Mutation::ShiftAll { delta } => {
            for md in &mut s.models {
                for t in &mut md.mesh {
                    map_tri(t, |p| v::add(p, *delta));
                }
                for r in &mut md.records {
                    for t in &mut r.soup {
                        map_tri(t, |p| v::add(p, *delta));
                    }
                    if r.kind != RecordKind::RaftPadOrBridge {
                        r.start_mm = v::add(r.start_mm, *delta);
                        r.end_mm = v::add(r.end_mm, *delta);
                    }
                }
            }
        }
    }
}

/// Parse one mutation spec: `jitter:<mm>[:model|support|both]`, `ulp`, `f16`,
/// `shrink:<rec>:<f>`, `shrink-all:<f>`, `scale:<rec>:<f>`, `remove:<a,b,..>`,
/// `move:<rec>:<dx,dy,dz>`, `float:<x,y,z>:<r>`, `rotz180`, `split`, `shift:<dx,dy,dz>`.
pub fn parse_spec(spec: &str) -> Result<Mutation, String> {
    let p: Vec<&str> = spec.split(':').collect();
    let f = |s: &str| s.parse::<f64>().map_err(|_| format!("bad number {s:?} in {spec:?}"));
    let u = |s: &str| s.parse::<usize>().map_err(|_| format!("bad index {s:?} in {spec:?}"));
    let v3 = |s: &str| -> Result<V3, String> {
        let q: Vec<f64> = s.split(',').map(f).collect::<Result<_, _>>()?;
        (q.len() == 3).then(|| [q[0], q[1], q[2]]).ok_or_else(|| format!("want x,y,z in {spec:?}"))
    };
    let need = |n: usize| (p.len() == n).then_some(()).ok_or_else(|| format!("wrong arguments in {spec:?}"));
    Ok(match p[0] {
        "jitter" => {
            let which = p.get(2).copied().unwrap_or("both");
            Mutation::Jitter {
                amp_mm: f(p.get(1).ok_or("jitter needs an amplitude")?)?,
                model: which != "support",
                support: which != "model",
                radii: p.get(3).is_some_and(|x| *x == "radii"),
                seed: p.get(4).map_or(Ok(0), |s| s.parse::<u64>().map_err(|_| format!("bad seed {s:?} in {spec:?}")))?,
            }
        }
        "cjitter" => {
            let which = p.get(2).copied().unwrap_or("both");
            Mutation::JitterConsistent {
                amp_mm: f(p.get(1).ok_or("cjitter needs an amplitude")?)?,
                model: which != "support",
                support: which != "model",
                radii: p.get(3).is_some_and(|x| *x == "radii"),
                seed: p.get(4).map_or(Ok(0), |s| s.parse::<u64>().map_err(|_| format!("bad seed {s:?} in {spec:?}")))?,
            }
        }
        "ulp" => match p.get(1) {
            None => Mutation::Ulp,
            Some(s) => Mutation::UlpSeed(s.parse::<u64>().map_err(|_| format!("bad seed {s:?} in {spec:?}"))?),
        },
        "f16" => Mutation::F16,
        "shrink" => {
            need(3)?;
            Mutation::ShrinkMesh { record: u(p[1])?, factor: f(p[2])? }
        }
        "shrink-all" => {
            need(2)?;
            Mutation::ShrinkAllMesh { factor: f(p[1])? }
        }
        "scale" => {
            need(3)?;
            Mutation::ScaleRecord { record: u(p[1])?, factor: f(p[2])? }
        }
        "remove" => {
            need(2)?;
            Mutation::Remove { records: p[1].split(',').map(u).collect::<Result<_, _>>()? }
        }
        "move" => {
            need(3)?;
            Mutation::Move { record: u(p[1])?, delta: v3(p[2])? }
        }
        "float" => {
            need(3)?;
            Mutation::Float { at: v3(p[1])?, radius: f(p[2])? }
        }
        "rotz180" => Mutation::RotateZ180,
        "split" => Mutation::SplitModel,
        "shift" => {
            need(2)?;
            Mutation::ShiftModel { delta: v3(p[1])? }
        }
        "shiftall" => {
            need(2)?;
            Mutation::ShiftAll { delta: v3(p[1])? }
        }
        other => return Err(format!("unknown mutation {other:?}")),
    })
}

/// `plinth-audit mutate <in.chitubox> <out.chitubox> <spec>...`
pub fn main(argv: &[String]) -> i32 {
    if argv.len() < 3 {
        eprintln!("usage: plinth-audit mutate <in.chitubox> <out.chitubox> <spec>...\n  specs: jitter:<mm>[:model|support|both[:radii[:seed]]] cjitter:<mm>[...same] (mesh follows the header) ulp[:seed] f16 shrink:<rec>:<f> shrink-all:<f> scale:<rec>:<f> remove:<a,b> move:<rec>:<dx,dy,dz> float:<x,y,z>:<r> rotz180 split shift:<dx,dy,dz> shiftall:<dx,dy,dz>");
        return 64;
    }
    let file = match ChituboxFile::open(std::path::Path::new(&argv[0])) {
        Ok(f) => f,
        Err(e) => {
            eprintln!("plinth-audit mutate: {e}");
            return 64;
        }
    };
    let mut s = to_synth(&file);
    for spec in &argv[2..] {
        match parse_spec(spec) {
            Ok(m) => apply(&mut s, &m),
            Err(e) => {
                eprintln!("plinth-audit mutate: {e}");
                return 64;
            }
        }
    }
    if let Err(e) = s.write(std::path::Path::new(&argv[1])) {
        eprintln!("plinth-audit mutate: cannot write {}: {e}", argv[1]);
        return 64;
    }
    0
}

/// `plinth-audit stl-jitter <in.stl> <out.stl> <rot-rad> <scale-ulps>`: rotate the
/// mesh about the vertical axis by `rot-rad` and scale it by `1 + n * 2^-23`, in
/// f64, then store as f32. A binary STL only. The G3 real-engine jitter input.
pub fn stl_main(argv: &[String]) -> i32 {
    if argv.len() != 4 {
        eprintln!("usage: plinth-audit stl-jitter <in.stl> <out.stl> <rot-rad> <scale-ulps>");
        return 64;
    }
    let (Ok(rot), Ok(ulps)) = (argv[2].parse::<f64>(), argv[3].parse::<f64>()) else {
        eprintln!("plinth-audit stl-jitter: bad numbers");
        return 64;
    };
    let bytes = match std::fs::read(&argv[0]) {
        Ok(b) => b,
        Err(e) => {
            eprintln!("plinth-audit stl-jitter: {e}");
            return 64;
        }
    };
    if bytes.len() < 84 {
        eprintln!("plinth-audit stl-jitter: not a binary STL");
        return 64;
    }
    let n = u32::from_le_bytes([bytes[80], bytes[81], bytes[82], bytes[83]]) as usize;
    if bytes.len() != 84 + 50 * n {
        eprintln!("plinth-audit stl-jitter: not a binary STL (size {} for {n} triangles)", bytes.len());
        return 64;
    }
    let scale = 1.0 + ulps * 2f64.powi(-23);
    let (c, s) = (rot.cos(), rot.sin());
    let mut out = bytes.clone();
    for i in 0..n {
        let base = 84 + 50 * i;
        for vtx in 0..3 {
            let o = base + 12 + 12 * vtx;
            let g = |k: usize| f64::from(f32::from_le_bytes([bytes[o + 4 * k], bytes[o + 4 * k + 1], bytes[o + 4 * k + 2], bytes[o + 4 * k + 3]]));
            let (x, y, z) = (g(0), g(1), g(2));
            let q = [(x * c - y * s) * scale, (x * s + y * c) * scale, z * scale];
            for (k, val) in q.iter().enumerate() {
                out[o + 4 * k..o + 4 * k + 4].copy_from_slice(&(*val as f32).to_le_bytes());
            }
        }
        // The stored normal is rotated too (unit length is kept by a pure rotation).
        let gn = |k: usize| f64::from(f32::from_le_bytes([bytes[base + 4 * k], bytes[base + 4 * k + 1], bytes[base + 4 * k + 2], bytes[base + 4 * k + 3]]));
        let (nx, ny, nz) = (gn(0), gn(1), gn(2));
        let nn = [nx * c - ny * s, nx * s + ny * c, nz];
        for (k, val) in nn.iter().enumerate() {
            out[base + 4 * k..base + 4 * k + 4].copy_from_slice(&(*val as f32).to_le_bytes());
        }
    }
    if let Err(e) = std::fs::write(&argv[1], out) {
        eprintln!("plinth-audit stl-jitter: cannot write {}: {e}", argv[1]);
        return 64;
    }
    0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn f16_rounds_to_the_half_grid() {
        assert_eq!(round_f16(1.0), 1.0);
        assert_eq!(round_f16(1.0004), 1.0);
        assert_eq!(round_f16(1.0006), 1.000_976_6);
        assert_eq!(round_f16(-2.5), -2.5);
        assert_eq!(round_f16(70000.0), f32::INFINITY);
        // 20 mm sits on a 0.0156 mm grid, about one pixel.
        let q = round_f16(20.01);
        assert!((q - 20.01).abs() <= 0.0079, "{q}");
    }

    #[test]
    fn ulp_steps_move_by_at_most_one_unit() {
        for x in [0.3f32, -4.25, 17.7, 1e-6] {
            let y = ulp_step(x, 1);
            assert!((i64::from(x.to_bits()) - i64::from(y.to_bits())).abs() <= 1);
        }
        // exact zeros stay zero for every key (a real file holds many), and no key makes a NaN
        for key in 0..64u64 {
            assert_eq!(ulp_step(0.0, key).to_bits(), 0.0f32.to_bits());
            assert_eq!(ulp_step(-0.0, key).to_bits(), (-0.0f32).to_bits());
            assert!(ulp_step(1.5, key).is_finite() && ulp_step(-1.5, key).is_finite());
        }
    }

    #[test]
    fn specs_parse() {
        assert!(matches!(parse_spec("jitter:0.001:support"), Ok(Mutation::Jitter { model: false, support: true, .. })));
        assert!(matches!(parse_spec("remove:1,2"), Ok(Mutation::Remove { .. })));
        assert!(parse_spec("shrink:3").is_err());
    }
}
