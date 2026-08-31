// Standalone generator for `marginal_but_adequate.chitubox` -- NOT part of
// the `plinth-audit` crate, NOT built by its `Cargo.toml`. Compiled ad hoc
// against the already-built `plinth_audit` rlib (same convention as
// `collinear_arms` / `double_load` / `starved_patch`).
//
// This is the FIRST known-good fixture in `tools/plinth-audit/fixtures/`:
// every prior fixture plants exactly one defect and expects a check to
// refuse it. This one plants NO defect. It is `docs/specs/audit-known-good.md`
// section 3.2's KG-2 ("kg_slab_min_seat"), built to this directory's assigned
// name `marginal_but_adequate` because that is precisely what the file is --
// deliberately the thinnest support margin in that document's five-file set,
// still genuinely adequate by the physics in `docs/physics.md`.
//
// Geometry: a 4.00 x 4.00 x 2.00 mm slab, floating 4.00 mm above the plate on
// four identical posts at (+/-1.00, +/-1.00) mm -- the perpendicular-bisector
// centres of the square, so the Voronoi partition of the underside is exactly
// the four 2x2 mm quadrants, each 4.00 mm^2, by symmetry (no ownership
// routing to reason about). Every post's tip is sized to exactly X = 12 and
// seated at h = 0.54 * r_tip -- 41% over the ruled seat-depth minimum, and
// deliberately closer to it than any other file in the known-good set. Every
// number below is derived in this file's README from `docs/physics.md` and
// the printer profile in `docs/specs/audit-invariants.md` / `audit-span-
// criterion.md`, NOT reverse-engineered from any `src/checks/*.rs` file --
// see the README's "What I did and did not open" section.
//
// Support chain per post (audit-known-good.md sec 2.2, four records):
//   TIP   (ContactSphere) : p0 -> p0 - 2R*z_hat,           r = R = R
//   TAPER (ContactTaper)  : ball centre -> node 0.50mm below, r_tip -> arm_r
//   MEMBER(Member)        : node+0.10 (into taper) -> z_plate+0.90, r = arm_r
//   FLARE (RootFlare)     : z_plate+1.00 (into member) -> z_plate, arm_r -> 0.90mm
//
// No raft pad: a FLARE landing on the plate is its own closed load path
// (`audit-invariants.md` sec 3.5 clause (i)) and sidesteps a documented
// fixture-tool defect -- `SynthRecord::with_matching_soup` returns an EMPTY
// soup for `RecordKind::RaftPadOrBridge` by its own contract, which several
// sibling fixtures' own READMEs report tripping `i8_no_dead_geometry`'s
// `GEOM_DEGENERATE reason=empty_soup` for reasons that have nothing to do
// with their planted defect.

use plinth_audit::consts as c;
use plinth_audit::reader::RecordKind;
use plinth_audit::synth::{box_soup, sphere_soup, frustum_soup, SynthFile, SynthModel, SynthRecord};

/// Step an f32 value by `n` ULPs (n may be negative). Same helper
/// `starved_patch/generate.rs` uses for the identical purpose: searching
/// nearby f32 representations of a real number for one that lands closest to
/// an f64 target after the write-time f32 rounding every field in this
/// format goes through.
fn step_f32(x: f32, n: i64) -> f32 {
    let bits = x.to_bits() as i64;
    f32::from_bits((bits + n) as u32)
}

/// Same zero-area-triangle workaround `collinear_arms` uses: `sphere_soup`'s
/// pole ring emits triangles with two bit-identical vertices at `la==0` and
/// `la==lat`, which `i8_no_dead_geometry` correctly flags as
/// `GEOM_DEGENERATE reason=zero_area_triangle`. Pre-existing defect in the
/// shared test helper, not owned by this fixture; dropped here so it does
/// not pollute this fixture's own i8/i6 reading with an unrelated artefact.
fn drop_zero_area(tris: Vec<[[f32; 3]; 3]>) -> Vec<[[f32; 3]; 3]> {
    tris.into_iter()
        .filter(|t| {
            let a = [
                (t[1][0] - t[0][0]) as f64,
                (t[1][1] - t[0][1]) as f64,
                (t[1][2] - t[0][2]) as f64,
            ];
            let b = [
                (t[2][0] - t[0][0]) as f64,
                (t[2][1] - t[0][1]) as f64,
                (t[2][2] - t[0][2]) as f64,
            ];
            let cx = a[1] * b[2] - a[2] * b[1];
            let cy = a[2] * b[0] - a[0] * b[2];
            let cz = a[0] * b[1] - a[1] * b[0];
            (cx * cx + cy * cy + cz * cz).sqrt() > 1e-9
        })
        .collect()
}

fn main() {
    let pi = std::f64::consts::PI;
    let twelve_pi = 12.0 * pi; // 37.6991118431...

    // ------------------------------------------------------------------
    // 1. Sizing procedure (audit-known-good.md sec 2.1), by hand
    // ------------------------------------------------------------------
    let a_face = 16.0_f64; // 4.00 x 4.00 mm slab underside
    let n_posts = 4usize;
    let a_cell = a_face / n_posts as f64; // 4.0 exactly, by symmetry
    let a_req = a_cell / c::X_ALLOWED; // 0.333333... mm^2

    // r_tip = sqrt(a_cell / (12 pi)), then ULP-search the f32 neighbourhood
    // so the STORED value makes Sigma(4 * 12*pi*r_tip_f32^2) agree with
    // D_total = 16.0 as tightly as an f32-quantised radius can manage --
    // I4 clause (d) is an EQUALITY test (audit-invariants.md sec 3.4(d)),
    // tolerance max(4096*eps64*D_total, 1e-9) mm^2, and three sibling
    // fixtures' READMEs independently report missing it by 33x-370x when a
    // literal is used instead of a swept one (audit-known-good.md sec 2.5).
    // **Measured finding, corrected here after direct observation of the
    // reconstruction (`--dump-graph`), not assumed in advance**: I4's own
    // `a_implied` is NOT `12*pi*taper.r_start^2`. `load_ownership`'s
    // `r_tip_mm` / `a_implied_mm2` for every contact in a first build of
    // this file matched `12*pi*(R_stored_f32 / ball_factor)^2` to full f64
    // precision (`R` read from the TIP record's own r1/r2 field, divided by
    // 1.5 with NO further f32 rounding) -- e.g. contact 12's reported
    // `r_tip_mm = 0.3255778948465983` is bit-for-bit `R/1.5` in f64, while
    // that contact's TAPER record actually stores `r_start =
    // 0.32557788491249084`, a full ULP-ish away. The two are close enough
    // that I1's own three-way agreement check passed regardless
    // (`r_tip_three_way_agrees: true` for all four contacts, spread <=
    // 1.9e-8 mm, inside its own tolerance) -- but I4 clause (d) is a much
    // tighter EQUALITY (1e-9 mm^2 floor) and is sensitive to exactly which
    // of the three candidate `r_tip` readings it uses. So the quantity that
    // must be swept for clause (d) is `R` (the TIP ball radius), not the
    // taper's own `r_start` -- sweeping the taper field alone (a first,
    // wrong attempt at this generator: residual 2.4e-7 mm^2, four orders
    // over tolerance) tunes a number `load_ownership` does not read.
    let r_tip_exact = (a_cell / twelve_pi).sqrt();
    let big_r_exact = c::BALL_FACTOR_STUB * r_tip_exact;
    let big_r_seed = big_r_exact as f32;

    // Sweep THREE contacts' `R` to a common value and solve the fourth in
    // closed form against the exact remaining demand (audit-known-good.md
    // sec 2.5's technique, applied to `R` rather than `r_tip`): as `R_a` is
    // swept over its own f32 neighbourhood, `R_n`'s post-rounding error
    // behaves like an equidistributed sawtooth, so pigeonholing ~1e5-1e6
    // candidates finds one landing within about (one-ULP-area)/N_sweep of
    // the target -- "one ULP of r ~ 0.34 moves 12*pi*r^2 by ~7.3e-7 mm^2, so
    // 1e5 candidates land the residual near 1e-11 mm^2" (sec 2.5, verbatim).
    // Window kept deliberately tight (+/- 100_000 ULPs =~ +/- 5.8e-3 mm at
    // this magnitude, below the printer's own pixel pitch) so the "4th
    // contact" cannot drift far from the other three's radius -- each
    // contact must still individually satisfy the area law, not merely the
    // four-contact sum (audit-known-good.md sec 1). A first, wider attempt
    // (+/- 600_000 ULPs, WITH an early break on the first sub-1e-12 hit)
    // found a mathematically-exact-sum candidate 1.37 million ULPs from
    // seed -- R_a=0.474, R_n=0.530, a spread that would badly violate "every
    // arm individually adequate". No early break here: scan the whole
    // bounded window and keep the genuine best.
    const SWEEP: i64 = 100_000;
    let mut best: Option<(f32, f32, f64)> = None; // (R_a, R_n, residual)
    for n in -SWEEP..=SWEEP {
        let big_r_a = step_f32(big_r_seed, n);
        if big_r_a <= 0.0 {
            continue;
        }
        let r_eff_a = f64::from(big_r_a) / c::BALL_FACTOR_STUB; // what load_ownership actually uses
        let three_area = 3.0 * twelve_pi * r_eff_a * r_eff_a;
        let remaining = a_face - three_area;
        if remaining <= 0.0 {
            continue;
        }
        let r_eff_n_exact = (remaining / twelve_pi).sqrt(); // target R_n/1.5, unrounded
        let big_r_n_exact = c::BALL_FACTOR_STUB * r_eff_n_exact;
        let big_r_n = big_r_n_exact as f32; // the single unavoidable final rounding, on R
        let r_eff_n = f64::from(big_r_n) / c::BALL_FACTOR_STUB;
        let total = three_area + twelve_pi * r_eff_n * r_eff_n;
        let residual = (total - a_face).abs();
        let better = best.is_none_or(|(_, _, best_res)| residual < best_res);
        if better {
            best = Some((big_r_a, big_r_n, residual));
        }
    }
    let (big_r_a, big_r_n, d_residual) = best.expect("sweep found a candidate");
    eprintln!(
        "R_a (3 contacts, f32) = {big_r_a:.9}  R_n (4th contact, f32) = {big_r_n:.9}  diff = {:.3e} mm ({} ULP)",
        f64::from(big_r_a) - f64::from(big_r_n),
        (big_r_a.to_bits() as i64 - big_r_n.to_bits() as i64)
    );
    let big_rs = [big_r_a, big_r_a, big_r_a, big_r_n];

    let beta = 0.54_f64; // KG-2's one changed number vs KG-1's 0.60

    eprintln!("--- sizing (f64, before f32 storage) ---");
    eprintln!("a_face = {a_face}  a_cell = {a_cell}  a_req = {a_req:.9}");
    eprintln!("shared sweep residual (Sigma(12*pi*r^2) vs 16.0) = {d_residual:.3e} mm^2  (tolerance floor 1e-9 mm^2)");

    // ------------------------------------------------------------------
    // 2. Frame
    //
    // BUILD-DEFECT CORRECTION (found by measurement, not by reading a check):
    // a first build of this generator set `translate_z = 5.0`, following
    // `consts.rs`'s own doc comment on `M_TRANSLATE_Z` ("the plate sits at
    // z = -translate_z in the model-centred frame") and `audit-invariants.md`
    // sec 1.2's prose. The audit's ACTUAL reconstruction is
    // `z_plate = mesh.bbox.min.z - translate_z`, which only coincides with
    // "-translate_z" when the mesh's own bbox bottom sits at local z=0 -- not
    // true here (the slab's bbox bottom is z=-1.0). With translate_z=5.0 the
    // audit reconstructed z_plate=-6.0 (`--dump-graph`: "z_plate=-6"), 1.00 mm
    // below every flare's foot, and i5_closed_load_path correctly reported
    // `PATH_FREE_END` on all four flares (cascading to i3_min_section's four
    // `MIN_SECTION_INDETERMINATE reason=path_open`, since a section can't be
    // measured along a path that never reaches ground). Confirmed against the
    // sibling `trivial_cube` fixture in this same tree, which independently
    // measured and fixed the identical defect. Corrected here: choose
    // `translate_z` so the RECONSTRUCTED z_plate lands where this file's
    // design intends, `translate_z = z_face - z_plate`.
    let z_face = -1.0_f64; // slab underside, bounding-box-centred frame
    let z_plate = -5.0_f64; // design intent: 4.00 mm below z_face
    let translate_z = z_face - z_plate; // = 4.0, NOT -z_plate

    let mesh = box_soup([-2.0, -2.0, -1.0], [2.0, 2.0, 1.0]);

    let posts = [(-1.0_f64, -1.0_f64), (1.0, -1.0), (-1.0, 1.0), (1.0, 1.0)];

    let mut records = Vec::new();
    let mut per_contact_report = Vec::new();

    for (i, &(x0, y0)) in posts.iter().enumerate() {
        // R chosen by the sweep above; r_tip (the TAPER's own narrow radius)
        // is DERIVED from it by the one unavoidable rounding step, so the
        // taper field and I4's independently-recomputed R/1.5 stay within a
        // single f32 ULP of one another (measured spread below).
        let big_r = big_rs[i];
        let big_r_f64 = f64::from(big_r);
        let r_tip_f64 = f64::from((big_r_f64 / c::BALL_FACTOR_STUB) as f32);
        let arm_r = big_r; // step 5 of sec 2.1: r_arm = 1.5 * r_tip = R exactly
        let h_design = beta * r_tip_f64;
        let h_req_ball_term = c::ball_seat_factor(c::BALL_FACTOR_STUB) * r_tip_f64;
        assert!(h_design > h_req_ball_term, "h below the ruled minimum");
        assert!(h_design < big_r_f64, "R3 violated: ball centre would sit inside the model");
        let a_ideal = {
            // a(h) = sqrt(2*f*r_tip*h - h^2), the cut-disc radius at the design h.
            let f = c::BALL_FACTOR_STUB;
            (2.0 * f * r_tip_f64 * h_design - h_design * h_design).sqrt()
        };

        // p0: the ball's deepest/topmost point, h above the down-face,
        // stored as f32 exactly as it will hit the wire.
        let p0_z = step_f32((z_face + h_design) as f32, 0);
        let z_face_f32 = step_f32(z_face as f32, 0);
        // Rule R7: the DECLARED depth is derived from the STORED f32 values,
        // never from the intended real h_design -- the two can differ by an
        // f32 ULP or two after independent rounding of z_face and p0.z.
        let h_delivered = f64::from(p0_z) - f64::from(z_face_f32);

        let p0 = [x0, y0, f64::from(p0_z)];
        let ball_centre_z = f64::from(p0_z) - big_r_f64;
        let tip_end = [x0, y0, f64::from(p0_z) - 2.0 * big_r_f64];
        let ball_centre = [x0, y0, ball_centre_z];

        let tip = SynthRecord::new(RecordKind::ContactSphere, p0, tip_end, big_r_f64, big_r_f64)
            .with_soup(drop_zero_area(sphere_soup(ball_centre, big_r_f64, 8, 16)));

        let taper_end = [x0, y0, ball_centre_z - 0.50];
        let taper = SynthRecord::new(
            RecordKind::ContactTaper,
            ball_centre,
            taper_end,
            r_tip_f64,
            arm_r as f64,
        )
        .with_declared_depth(h_delivered)
        .with_matching_soup();

        let member_start = [x0, y0, taper_end[2] + 0.10]; // 0.10mm overlap into the taper
        let member_end = [x0, y0, z_plate + 0.90];
        let member = SynthRecord::new(
            RecordKind::Member,
            member_start,
            member_end,
            arm_r as f64,
            arm_r as f64,
        )
        .with_matching_soup();

        let flare_start = [x0, y0, z_plate + 1.00]; // 0.10mm overlap into the member
        let flare_end = [x0, y0, z_plate]; // grounded exactly on the plate
        let flare = SynthRecord::new(
            RecordKind::RootFlare,
            flare_start,
            flare_end,
            arm_r as f64,
            0.90,
        )
        .with_soup(frustum_soup(flare_start, arm_r as f64, flare_end, 0.90, 24));

        per_contact_report.push((x0, y0, r_tip_f64, big_r_f64, h_delivered, a_ideal));
        records.extend([tip, taper, member, flare]);
    }

    eprintln!("--- per-contact sizing (R6/R7: from stored f32s) ---");
    for (x0, y0, rt, rr, h, a) in &per_contact_report {
        eprintln!(
            "  ({x0:+.2},{y0:+.2}): r_tip={rt:.9}  R={rr:.9}  h_delivered={h:.9}  a_ideal={a:.9}  A_ideal={:.9}",
            pi * a * a
        );
    }

    let file = SynthFile::new().with_model(SynthModel {
        name: "marginal_but_adequate".into(),
        translate_mm: [0.0, 0.0, translate_z],
        mesh,
        records,
    });

    let mut bytes = file.to_bytes();

    // ------------------------------------------------------------------
    // 3. ChituBox-openability splice (audit-known-good.md sec 7.2)
    //
    // `SynthFile::to_bytes` writes settings_size=0 / off8=24, matching the
    // eleven pre-existing (broken) fixtures. A real populated file carries
    // settings_size=420 and a 396-byte settings blob (measured all-zero even
    // in populated specimens -- docs/specs/chitubox-format.md sec 6) plus a
    // non-empty preview blob at [420, off8). Insert both, and shift every
    // absolute file offset the format stores by the same amount, so a human
    // opening this in ChituBox sees a normal populated project rather than
    // the same-shape-but-different-provenance file the broken fixtures are.
    // ------------------------------------------------------------------
    let preview_path = std::env::args()
        .nth(2)
        .unwrap_or_else(|| "../../../../exports/sphere-first-artifact.chitubox".to_string());
    let donor = std::fs::read(&preview_path)
        .unwrap_or_else(|e| panic!("reading preview donor {preview_path}: {e}"));
    // Donor header: off8 at bytes [8,12), settings_size at [12,16) (must be 420).
    let donor_off8 = u32::from_le_bytes(donor[8..12].try_into().unwrap()) as usize;
    let donor_settings = u32::from_le_bytes(donor[12..16].try_into().unwrap());
    assert_eq!(donor_settings, 420, "preview donor is not a populated file");
    let preview = &donor[420..donor_off8];
    eprintln!(
        "--- splice: preview donor {preview_path}, {} bytes [420,{donor_off8})",
        preview.len()
    );

    let insert_len = 396 + preview.len();
    let mut spliced = Vec::with_capacity(bytes.len() + insert_len);
    spliced.extend_from_slice(&bytes[0..24]); // header, patched below
    spliced.extend(std::iter::repeat(0u8).take(396)); // settings blob, all-zero
    spliced.extend_from_slice(preview); // preview blob, copied verbatim
    spliced.extend_from_slice(&bytes[24..]); // model table + everything after

    // Patch header field 12 (settings_size) -> 420.
    spliced[12..16].copy_from_slice(&420u32.to_le_bytes());
    // Every stored absolute offset >= 24 must move forward by insert_len:
    // header off8 (8), header off20 (20), each model's mesh_off (+272) and
    // support_index_ptr (+268), each support index's own records_off (+4)
    // and mesh_off (+8), and each segment record's own mesh_off (+40) when
    // nonzero (a record with no soup stores 0, which must stay 0).
    let shift_u32 = |b: &mut [u8], off: usize| {
        let v = u32::from_le_bytes(b[off..off + 4].try_into().unwrap());
        if v != 0 {
            b[off..off + 4].copy_from_slice(&(v + insert_len as u32).to_le_bytes());
        }
    };
    shift_u32(&mut spliced, 8); // H_OFF8
    shift_u32(&mut spliced, 20); // H_OFF20

    let off8_new = u32::from_le_bytes(spliced[8..12].try_into().unwrap()) as usize;
    let model_count = u32::from_le_bytes(spliced[4..8].try_into().unwrap()) as usize;
    for mi in 0..model_count {
        let moff = off8_new + mi * c::MODEL_RECORD_LEN;
        shift_u32(&mut spliced, moff + c::M_MESH_OFF);
        shift_u32(&mut spliced, moff + c::M_SUPPORT_INDEX_PTR);
        let sip = u32::from_le_bytes(
            spliced[moff + c::M_SUPPORT_INDEX_PTR..moff + c::M_SUPPORT_INDEX_PTR + 4]
                .try_into()
                .unwrap(),
        ) as usize;
        if sip != 0 {
            shift_u32(&mut spliced, sip + c::SI_RECORDS_OFF);
            shift_u32(&mut spliced, sip + c::SI_MESH_OFF);
            let n_rec = u32::from_le_bytes(
                spliced[sip + c::SI_N_RECORDS..sip + c::SI_N_RECORDS + 4]
                    .try_into()
                    .unwrap(),
            ) as usize;
            let records_off = u32::from_le_bytes(
                spliced[sip + c::SI_RECORDS_OFF..sip + c::SI_RECORDS_OFF + 4]
                    .try_into()
                    .unwrap(),
            ) as usize;
            for ri in 0..n_rec {
                let ro = records_off + ri * c::SEG_RECORD_LEN;
                shift_u32(&mut spliced, ro + c::S_MESH_OFF);
            }
        }
    }
    bytes = spliced;

    let out = std::env::args()
        .nth(1)
        .expect("usage: gen_marginal_but_adequate <out.chitubox> [preview_donor.chitubox]");
    std::fs::write(&out, &bytes).expect("write");
    eprintln!("wrote {out} ({} bytes)", bytes.len());
}
