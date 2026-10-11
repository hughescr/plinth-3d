//! G2: every mutation fixture of the similarity gate with its expected verdict,
//! plus the sabotage (band set to zero), run on the known-good `trivial_cube`
//! file and on a braced variant of it. Each fixture is built in memory with
//! `compare::mutate`, written under the system temp directory, and compared
//! through the same `compare_paths` the command line uses.
//!
//! The same mutations apply to a real file with
//! `plinth-audit mutate <in> <out> <spec>`; `G-REPORT.md` carries the real-file
//! verdicts. Expected verdicts here come from the plan, section 1; where the
//! tool taught otherwise, the comment on that test says so.

use plinth_audit::compare::classify::Options;
use plinth_audit::compare::mutate::{apply, to_synth, Mutation};
use plinth_audit::compare::verdict::{Status, Verdict};
use plinth_audit::compare::{compare_paths, Args, Run};
use plinth_audit::reader::{ChituboxFile, RecordKind};
use plinth_audit::synth::{SynthFile, SynthRecord};
use std::path::{Path, PathBuf};

const BASE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/fixtures/trivial_cube/trivial_cube.chitubox");

fn dir(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("plinth-compare-tests-{}", std::process::id())).join(name);
    std::fs::create_dir_all(&d).expect("temp dir");
    d
}

fn base() -> SynthFile {
    to_synth(&ChituboxFile::open(Path::new(BASE)).expect("trivial_cube parses"))
}

fn write(s: &SynthFile, d: &Path, stem: &str) -> PathBuf {
    let p = d.join(format!("{stem}.chitubox"));
    s.write(&p).expect("write");
    p
}

fn poses(d: &Path, stem: &str, committed: &str) {
    std::fs::write(d.join("poses.tsv"), format!("model\thow\tcommitted\n{stem}\torientation searched\tnote; rotation committed: {committed}\n")).expect("poses");
}

fn log_with_s(d: &Path, stem: &str, s: f64) {
    std::fs::write(d.join(format!("{stem}.log")), format!("[sweep M] slenderness lane: S = {s} in force; brace reach\n")).expect("log");
}

fn args(r: &Path, c: &Path) -> Args {
    Args { reference: r.to_path_buf(), candidate: c.to_path_buf(), band: 1, threads: 2, ..Args::default() }
}

/// Compare a mutated base against the base. `name` keeps the temp dirs apart.
fn run_mut(name: &str, muts: &[Mutation]) -> Run {
    let b = base();
    let mut m = b.clone();
    for x in muts {
        apply(&mut m, x);
    }
    let rd = dir(&format!("{name}-ref"));
    let cd = dir(&format!("{name}-cand"));
    poses(&rd, "x", "identity");
    poses(&cd, "x", "identity");
    let r = write(&b, &rd, "x");
    let c = write(&m, &cd, "x");
    compare_paths(&args(&r, &c))
}

fn row<'a>(run: &'a Run, id: &str) -> &'a plinth_audit::compare::verdict::Row {
    run.outcome.rows.iter().find(|r| r.id == id).unwrap_or_else(|| panic!("no row {id}: {:?}", run.outcome.rows.iter().map(|r| &r.id).collect::<Vec<_>>()))
}

fn show(run: &Run) -> String {
    run.outcome.rows.iter().map(|r| format!("{} {} {}", r.id, r.status.as_str(), r.value)).collect::<Vec<_>>().join("\n")
}

fn kind_index(s: &SynthFile, k: RecordKind, nth: usize) -> usize {
    s.models[0].records.iter().enumerate().filter(|(_, r)| r.kind == k).nth(nth).map(|(i, _)| i).expect("record of that kind")
}

#[test]
fn identical_copy_is_identical_by_sha() {
    let run = run_mut("identical", &[]);
    assert_eq!(run.outcome.verdict, Verdict::Identical);
    assert_eq!(run.outcome.verdict.exit_code(), 0);
}

#[test]
fn f32_ulp_jitter_is_equivalent() {
    let run = run_mut("ulp", &[Mutation::Ulp]);
    assert_eq!(run.outcome.verdict, Verdict::Equivalent, "{}", show(&run));
}

#[test]
fn sub_voxel_lattice_jitter_is_equivalent_and_zero_band_fails_it() {
    // The jitter fixture: the whole design (model and supports) moved by a third of a voxel in
    // plan and a fifth of a layer in Z. Every raster boundary moves; nothing physical changes.
    // Sabotage: with the band set to zero the same fixture must NOT pass as equivalent.
    let m = [Mutation::ShiftAll { delta: [0.004, 0.006, 0.01] }];
    let run = run_mut("jitter", &m);
    assert_eq!(run.outcome.verdict, Verdict::Equivalent, "{}", show(&run));
    let b = base();
    let mut j = b.clone();
    apply(&mut j, &m[0]);
    let rd = dir("jitter0-ref");
    let cd = dir("jitter0-cand");
    poses(&rd, "x", "identity");
    poses(&cd, "x", "identity");
    let (r, c) = (write(&b, &rd, "x"), write(&j, &cd, "x"));
    let mut a = args(&r, &c);
    a.band = 0;
    let zero = compare_paths(&a);
    assert!(
        !matches!(zero.outcome.verdict, Verdict::Equivalent | Verdict::Identical),
        "band 0 let the jitter fixture through: {:?}\n{}",
        zero.outcome.verdict,
        show(&zero)
    );
}

/// The largest distance of any emitted mesh vertex from its record's own header solid, over the model.
fn worst_mesh_gap_mm(s: &SynthFile) -> f64 {
    let bytes = s.to_bytes();
    let f = ChituboxFile::parse(Path::new("gap"), &bytes).expect("parses");
    let mut worst = 0.0f64;
    for r in &f.models[0].records {
        if let Ok(p) = plinth_audit::primitives::Primitive::from_record(r) {
            worst = worst.max(p.verify_against_soup(r).max_surface_deviation_mm);
        }
    }
    worst
}

/// Noise that keeps the design a design: the mesh follows its header. The independent-noise
/// fixture does not, and on a short fat member (0.02 mm long, 0.6 mm across) a micron on each end
/// turns the header axis by a few degrees and parts mesh and header by tens of microns.
#[test]
fn consistent_jitter_keeps_every_mesh_on_its_header_and_independent_jitter_does_not() {
    let mut b = base();
    let slab = SynthRecord::new(RecordKind::Member, [3.0, 3.0, -3.0], [3.0, 3.0, -2.98], 0.58, 0.60).with_matching_soup();
    b.models[0].records.push(slab);
    assert!(worst_mesh_gap_mm(&b) < 1e-5, "the fixture itself is consistent");
    let mut c = b.clone();
    apply(&mut c, &Mutation::JitterConsistent { amp_mm: 1.0e-3, model: false, support: true, radii: true, seed: 0 });
    assert!(worst_mesh_gap_mm(&c) < 1.0e-4, "mesh left its header by {} mm", worst_mesh_gap_mm(&c));
    let mut i = b.clone();
    apply(&mut i, &Mutation::Jitter { amp_mm: 1.0e-3, model: false, support: true, radii: true, seed: 0 });
    assert!(worst_mesh_gap_mm(&i) > 3.0e-3, "independent noise left only {} mm", worst_mesh_gap_mm(&i));
    // a different seed is a different draw
    let mut d = b.clone();
    apply(&mut d, &Mutation::JitterConsistent { amp_mm: 1.0e-3, model: false, support: true, radii: true, seed: 1 });
    assert_ne!(c.to_bytes(), d.to_bytes());
}

#[test]
fn f16_conversion_is_seen() {
    // trivial_cube sits within 5 mm of the origin, so half precision costs at most 0.002 mm there:
    // the verdict is whatever the numbers say, but it must not be IDENTICAL, and it must not crash.
    let run = run_mut("f16", &[Mutation::F16]);
    assert_ne!(run.outcome.verdict, Verdict::Identical);
}

#[test]
fn one_percent_member_mesh_shrink_is_seen_and_is_below_the_band() {
    let b = base();
    let post = kind_index(&b, RecordKind::Member, 0);
    let run = run_mut("shrink1", &[Mutation::ShrinkMesh { record: post, factor: 0.99 }]);
    let (r, c) = (run.ref_facts.as_ref().expect("ref"), run.cand_facts.as_ref().expect("cand"));
    assert_ne!(r.models[0].s.pixel_count, c.models[0].s.pixel_count, "the support raster comes from the emitted mesh, so a mesh-only shrink must change it");
    assert_eq!(run.outcome.verdict, Verdict::Equivalent, "1 % of a 0.49 mm radius is 0.005 mm, under one voxel:\n{}", show(&run));
}

#[test]
fn one_percent_all_support_mesh_shrink_is_seen() {
    let run = run_mut("shrinkall1", &[Mutation::ShrinkAllMesh { factor: 0.99 }]);
    let (r, c) = (run.ref_facts.as_ref().expect("ref"), run.cand_facts.as_ref().expect("cand"));
    assert!(c.models[0].s.pixel_count < r.models[0].s.pixel_count);
    // Under a voxel everywhere, so no band is broken, but every tip ball now disagrees with its
    // header by 5 microns, so I4 can no longer establish the tip radius that the reference
    // established: the gate cannot say the file is the same, and it does not say it is fine.
    assert_eq!(run.outcome.verdict, Verdict::Indeterminate, "{}", show(&run));
    assert_eq!(row(&run, "H4.i4").status, Status::Indeterminate);
}

#[test]
fn twenty_percent_member_mesh_shrink_fails_tier_zero() {
    let b = base();
    let post = kind_index(&b, RecordKind::Member, 0);
    let run = run_mut("shrink20", &[Mutation::ShrinkMesh { record: post, factor: 0.8 }]);
    assert_eq!(run.outcome.verdict, Verdict::Regression, "{}", show(&run));
    assert_eq!(row(&run, "T0.mesh_header").status, Status::Fail);
}

#[test]
fn one_member_halved_is_a_regression() {
    let b = base();
    let post = kind_index(&b, RecordKind::Member, 0);
    let run = run_mut("halved", &[Mutation::ScaleRecord { record: post, factor: 0.5 }]);
    assert_eq!(run.outcome.verdict, Verdict::Regression, "{}", show(&run));
    assert_eq!(row(&run, "N3").status, Status::Fail, "{}", show(&run));
}

/// The first Member split in two collinear halves in series on the same path: a thick one and
/// a thin one. Returns the file and the index of the thick half.
fn thick_then_thin() -> (SynthFile, usize) {
    let mut s = base();
    let mi = kind_index(&s, RecordKind::Member, 0);
    let r = s.models[0].records[mi].clone();
    let mid = [(r.start_mm[0] + r.end_mm[0]) / 2.0, (r.start_mm[1] + r.end_mm[1]) / 2.0, (r.start_mm[2] + r.end_mm[2]) / 2.0];
    let lo = SynthRecord::new(RecordKind::Member, r.start_mm, mid, 0.6, 0.6).with_matching_soup();
    let hi = SynthRecord::new(RecordKind::Member, mid, r.end_mm, 0.3, 0.3).with_matching_soup();
    s.models[0].records[mi] = lo;
    s.models[0].records.insert(mi + 1, hi);
    (s, mi)
}

/// A narrow unchanged member on the same path must not hide the thinning of a thicker one:
/// halving the 0.6 mm half leaves it at 0.3 mm, the same as the path minimum, so a single
/// minimum over the path sees no change at all.
#[test]
fn thinning_a_thick_member_behind_a_narrow_unchanged_one_fails_n3() {
    let (b, thick) = thick_then_thin();
    let mut m = b.clone();
    apply(&mut m, &Mutation::ScaleRecord { record: thick, factor: 0.5 });
    let (rd, cd) = (dir("masked-ref"), dir("masked-cand"));
    poses(&rd, "x", "identity");
    poses(&cd, "x", "identity");
    let (r, c) = (write(&b, &rd, "x"), write(&m, &cd, "x"));
    let run = compare_paths(&args(&r, &c));
    let (rf, cf) = (run.ref_facts.as_ref().expect("ref"), run.cand_facts.as_ref().expect("cand"));
    // The masking premise: the path minimum of every tip is the same in both files.
    let (ra, rc) = (&rf.models[0], &cf.models[0]);
    let masked = ra.path_sec_r.values().zip(ra.path_sec_r.keys()).any(|(x, t)| rc.path_sec_r.get(t).is_some_and(|y| (y / x - 1.0).abs() < 0.05) && ra.path_members[t].len() > 1);
    assert!(masked, "fixture must be one where the path minimum is unchanged: {:?} -> {:?}", ra.path_sec_r, rc.path_sec_r);
    assert_eq!(row(&run, "N3").status, Status::Fail, "{}", show(&run));
}

#[test]
fn contact_attachment_area_halved_fails_i2() {
    // Seat depth h = R - d with d = 0.2932 mm, R = 0.4886 mm: the cut disc has a^2 = R^2 - d^2.
    // Half the area needs R'^2 = (R^2 - d^2) / 2 + d^2, a factor of 0.8246 on R. Centre and count are kept.
    let b = base();
    let tip = kind_index(&b, RecordKind::ContactSphere, 0);
    let run = run_mut("i2half", &[Mutation::ScaleRecord { record: tip, factor: 0.8246 }]);
    assert_eq!(row(&run, "H4.i2").status, Status::Fail, "{}", show(&run));
    assert_eq!(run.outcome.verdict, Verdict::Regression);
    assert_eq!(row(&run, "N2").status, Status::Pass, "the contact is still there and in place:\n{}", show(&run));
}

#[test]
fn non_head_member_moved_into_the_model_fails_i6() {
    let b = base();
    let post = kind_index(&b, RecordKind::Member, 0);
    let run = run_mut("i6", &[Mutation::Move { record: post, delta: [0.0, 0.0, 1.5] }]);
    assert_eq!(row(&run, "H4.i6").status, Status::Fail, "{}", show(&run));
    assert_eq!(run.outcome.verdict, Verdict::Regression);
}

#[test]
fn planted_float_of_about_twenty_pixels_fails_h1() {
    let run = run_mut("float", &[Mutation::Float { at: [3.0, 3.0, -3.0], radius: 0.055 }]);
    let (r, c) = (run.ref_facts.as_ref().expect("ref"), run.cand_facts.as_ref().expect("cand"));
    // trivial_cube already has floating pieces of its own (a slab on tangent balls); only new ones count.
    let biggest = c.models[0]
        .floating
        .iter()
        .filter(|fb| !r.models[0].floating.iter().any(|fa| (fa.k - fb.k).abs() <= 1 && (fa.cx - fb.cx).hypot(fa.cy - fb.cy) < 0.2))
        .map(|f| f.px)
        .max()
        .unwrap_or(0);
    assert!((16..=64).contains(&biggest), "the planted piece is {biggest} px");
    assert_eq!(row(&run, "H1").status, Status::Fail, "{}", show(&run));
    assert_eq!(run.outcome.verdict, Verdict::Regression);
}

#[test]
fn rotated_file_is_pose_changed_from_the_transform() {
    let b = base();
    let mut m = b.clone();
    apply(&mut m, &Mutation::RotateZ180);
    let (rd, cd) = (dir("rot-ref"), dir("rot-cand"));
    poses(&rd, "x", "identity");
    poses(&cd, "x", "Z 180.000 deg");
    let (r, c) = (write(&b, &rd, "x"), write(&m, &cd, "x"));
    let run = compare_paths(&args(&r, &c));
    assert_eq!(run.outcome.verdict, Verdict::PoseChanged, "{}", show(&run));
    assert!(run.outcome.classes.pose_changed);
    assert_eq!(row(&run, "H1").status, Status::Pass, "{}", show(&run));
    assert_eq!(row(&run, "N1-N5").status, Status::Skipped, "Tier 2 needs matched members");
}

#[test]
fn a_regression_overrides_the_pose_class() {
    // The same rotation, but a member halved as well: hard tier failure wins over POSE-CHANGED.
    let b = base();
    let post = kind_index(&b, RecordKind::Member, 0);
    let mut m = b.clone();
    apply(&mut m, &Mutation::ScaleRecord { record: post, factor: 0.5 });
    apply(&mut m, &Mutation::RotateZ180);
    let (rd, cd) = (dir("rotbad-ref"), dir("rotbad-cand"));
    poses(&rd, "x", "identity");
    poses(&cd, "x", "Z 180.000 deg");
    let (r, c) = (write(&b, &rd, "x"), write(&m, &cd, "x"));
    let run = compare_paths(&args(&r, &c));
    assert_eq!(run.outcome.verdict, Verdict::Regression, "{}", show(&run));
    assert!(run.outcome.classes.pose_changed && run.outcome.classes.regression);
}

#[test]
fn decimated_model_at_the_same_pose_is_mesh_changed_not_pose_changed() {
    // The reference carries a finer tessellation of the same surface; the candidate is the coarse one.
    let b = base();
    let mut fine = b.clone();
    apply(&mut fine, &Mutation::SplitModel);
    let (rd, cd) = (dir("dec-ref"), dir("dec-cand"));
    poses(&rd, "x", "identity");
    poses(&cd, "x", "identity");
    let (r, c) = (write(&fine, &rd, "x"), write(&b, &cd, "x"));
    let run = compare_paths(&args(&r, &c));
    assert_eq!(run.outcome.verdict, Verdict::MeshChanged, "{}", show(&run));
    assert!(run.outcome.classes.mesh_changed && !run.outcome.classes.pose_changed);
}

// ---- the braced variant -------------------------------------------------------------

/// Split the first two posts' Member at their midpoints and tie the midpoints with a brace.
fn braced() -> (SynthFile, usize) {
    let mut s = base();
    let m0 = kind_index(&s, RecordKind::Member, 0);
    let m1 = kind_index(&s, RecordKind::Member, 1);
    let mut nodes = Vec::new();
    // Split the later record first so the earlier index stays valid.
    for &mi in &[m1, m0] {
        let r = s.models[0].records[mi].clone();
        let mid = [(r.start_mm[0] + r.end_mm[0]) / 2.0, (r.start_mm[1] + r.end_mm[1]) / 2.0, (r.start_mm[2] + r.end_mm[2]) / 2.0];
        let rm = (r.r_start + r.r_end) / 2.0;
        let lo = SynthRecord::new(RecordKind::Member, r.start_mm, mid, r.r_start, rm).with_matching_soup();
        let hi = SynthRecord::new(RecordKind::Member, mid, r.end_mm, rm, r.r_end).with_matching_soup();
        s.models[0].records[mi] = lo;
        s.models[0].records.insert(mi + 1, hi);
        nodes.push(mid);
    }
    let (n1, n0) = (nodes[0], nodes[1]);
    let brace = SynthRecord::new(RecordKind::Member, n0, n1, 0.2, 0.2).with_matching_soup();
    s.models[0].records.push(brace);
    let at = s.models[0].records.len() - 1;
    (s, at)
}

#[test]
fn brace_removed_with_every_tip_to_plate_path_kept_fails_n5() {
    let (b, brace) = braced();
    let mut m = b.clone();
    apply(&mut m, &Mutation::Remove { records: vec![brace] });
    let (rd, cd) = (dir("brace-ref"), dir("brace-cand"));
    poses(&rd, "x", "identity");
    poses(&cd, "x", "identity");
    // A limit the braced halves meet and the merged post does not.
    log_with_s(&rd, "x", 2.0);
    log_with_s(&cd, "x", 2.0);
    let (r, c) = (write(&b, &rd, "x"), write(&m, &cd, "x"));
    let run = compare_paths(&args(&r, &c));
    let (rf, cf) = (run.ref_facts.as_ref().expect("ref"), run.cand_facts.as_ref().expect("cand"));
    assert!(
        cf.models[0].posts.len() < rf.models[0].posts.len(),
        "without the brace the halves merge into one post each: {} posts against {}",
        cf.models[0].posts.len(),
        rf.models[0].posts.len()
    );
    assert_eq!(row(&run, "N5").status, Status::Fail, "{}", show(&run));
    assert_eq!(run.outcome.verdict, Verdict::Regression);
}

#[test]
fn contact_removed_under_a_wing_already_joined_sideways_fails_n2() {
    let (b, _) = braced();
    let tip = kind_index(&b, RecordKind::ContactSphere, 1);
    let taper = b.models[0].records.iter().position(|r| r.kind == RecordKind::ContactTaper && r.start_mm == b.models[0].records[tip].start_mm || r.end_mm == b.models[0].records[tip].end_mm && r.kind == RecordKind::ContactTaper);
    let mut gone = vec![tip];
    gone.extend(taper);
    let mut m = b.clone();
    apply(&mut m, &Mutation::Remove { records: gone });
    let (rd, cd) = (dir("wing-ref"), dir("wing-cand"));
    poses(&rd, "x", "identity");
    poses(&cd, "x", "identity");
    let (r, c) = (write(&b, &rd, "x"), write(&m, &cd, "x"));
    let run = compare_paths(&args(&r, &c));
    assert_eq!(row(&run, "N2").status, Status::Fail, "{}", show(&run));
    assert_eq!(run.outcome.verdict, Verdict::Regression);
}

#[test]
fn options_default_is_the_gate() {
    let o = Options::default();
    assert_eq!(o.band_voxels, 1);
    assert!(!o.assume_consumer_matched);
}

// ---- G7: the false alarms of the first real use (Q12) --------------------------------------------

/// Lowest and highest z of the cube model of the base file.
fn cube_z() -> (f64, f64) {
    let f = ChituboxFile::open(Path::new(BASE)).expect("parses");
    let zs = f.models[0].model_soup.tris.iter().flatten().map(|p| f64::from(p[2]));
    (zs.clone().fold(f64::MAX, f64::min), zs.fold(f64::MIN, f64::max))
}

/// Compare `reference` with `candidate`, both written as given (no mutation applied).
fn run_pair(name: &str, reference: &SynthFile, candidate: &SynthFile, tweak: impl Fn(&mut Args)) -> Run {
    let (rd, cd) = (dir(&format!("{name}-ref")), dir(&format!("{name}-cand")));
    poses(&rd, "x", "identity");
    poses(&cd, "x", "identity");
    let (r, c) = (write(reference, &rd, "x"), write(candidate, &cd, "x"));
    let mut a = args(&r, &c);
    tweak(&mut a);
    compare_paths(&a)
}

#[test]
fn a_piece_that_floats_in_both_files_is_not_new_floating_material() {
    // The reference floats a small piece. The candidate floats the same piece one layer higher: its
    // break moved by a layer. That is not new floating material (H1 matches pieces, voxel by voxel).
    let float = |z: f64| Mutation::Float { at: [3.0, 3.0, z], radius: 0.055 };
    let mut r = base();
    apply(&mut r, &float(-3.0));
    let mut c = base();
    apply(&mut c, &float(-2.98));
    let run = run_pair("h1-both", &r, &c, |_| {});
    assert_eq!(row(&run, "H1").status, Status::Pass, "{}", show(&run));
    // Control: the same piece in a different place is a new piece, and the old one is gone.
    let mut elsewhere = base();
    apply(&mut elsewhere, &Mutation::Float { at: [-3.0, -3.0, -3.0], radius: 0.055 });
    let run = run_pair("h1-moved", &r, &elsewhere, |_| {});
    assert_eq!(row(&run, "H1").status, Status::Fail, "a float 6 mm away is new material:\n{}", show(&run));
}

#[test]
fn a_mesh_cut_by_the_model_agrees_with_its_header_and_a_short_one_does_not() {
    let (zmin, _) = cube_z();
    // A post whose header goes 0.3 mm into the cube; its mesh stops at the cube's underside.
    let post = |mesh_top: f64| {
        let mut r = SynthRecord::new(RecordKind::Member, [0.0, 0.0, -3.0], [0.0, 0.0, zmin + 0.3], 0.2, 0.2);
        r.soup = SynthRecord::new(RecordKind::Member, [0.0, 0.0, -3.0], [0.0, 0.0, mesh_top], 0.2, 0.2).with_matching_soup().soup;
        r
    };
    let mut cut = base();
    cut.models[0].records.push(post(zmin));
    let run = run_pair("t0-cut", &base(), &cut, |_| {});
    assert_eq!(row(&run, "T0.mesh_header").status, Status::Pass, "the model made the cut:\n{}", show(&run));
    // Control: the mesh ends 0.3 mm short of the cube. Nothing explains that.
    let mut short = base();
    short.models[0].records.push(post(zmin - 0.3));
    let run = run_pair("t0-short", &base(), &short, |_| {});
    assert_eq!(row(&run, "T0.mesh_header").status, Status::Fail, "{}", show(&run));
}

#[test]
fn a_member_standing_on_the_model_surface_is_grounded_and_one_above_it_is_an_orphan() {
    let (_, zmax) = cube_z();
    // A post standing on the cube's top face (owner ruling: a valid seat is the plate, the model
    // surface, or another support).
    let post = |foot: f64| SynthRecord::new(RecordKind::Member, [0.0, 0.0, foot], [0.0, 0.0, foot + 2.0], 0.2, 0.2).with_matching_soup();
    let mut on = base();
    on.models[0].records.push(post(zmax - 0.005));
    let run = run_pair("i5-on", &base(), &on, |_| {});
    let i5 = row(&run, "H4.i5");
    assert!(!i5.detail.iter().any(|d| d.contains("PATH_ORPHAN")), "a post on the model surface is not an orphan:\n{}\n{:?}", show(&run), i5.detail);
    // Control: 0.5 mm above the cube it touches nothing.
    let mut above = base();
    above.models[0].records.push(post(zmax + 0.5));
    let run = run_pair("i5-above", &base(), &above, |_| {});
    let i5 = row(&run, "H4.i5");
    assert!(i5.detail.iter().any(|d| d.contains("PATH_ORPHAN")), "a post in the air is an orphan:\n{}\n{:?}", show(&run), i5.detail);
}

#[test]
fn a_declared_contact_merge_passes_a_twin_and_still_fails_a_contact_lost_without_a_survivor() {
    // The reference has a second contact in the first contact's column (a twin 0.12 mm aside); the
    // candidate has merged it away. Undeclared, that is a lost contact (N2). Declared, it is a merge.
    let tip = kind_index(&base(), RecordKind::ContactSphere, 0);
    let twin_of = |delta: [f64; 3]| {
        let mut r = base();
        let t = r.models[0].records[tip].clone();
        r.models[0].records.push(t);
        let at = r.models[0].records.len() - 1;
        apply(&mut r, &Mutation::Move { record: at, delta });
        r
    };
    let declare = |a: &mut Args| {
        a.declared = Some(plinth_audit::compare::classify::DeclaredClass::PlanChanged);
        a.declared_changes.contact_merge = true;
    };
    let near = twin_of([0.12, 0.0, 0.0]);
    let undeclared = run_pair("n2-undeclared", &near, &base(), |_| {});
    assert_eq!(row(&undeclared, "N2").status, Status::Fail, "{}", show(&undeclared));
    let declared = run_pair("n2-declared", &near, &base(), declare);
    assert_eq!(row(&declared, "N2").status, Status::Pass, "{}", show(&declared));
    // A contact lost 4 mm from every survivor is not a merge, declared or not.
    let far = twin_of([3.0, 0.0, 0.0]);
    let declared = run_pair("n2-far", &far, &base(), declare);
    assert_eq!(row(&declared, "N2").status, Status::Fail, "{}", show(&declared));
    // The declaration is recorded and the verdict stays the computed one.
    assert!(declared.outcome.rows.iter().any(|r| r.id == "C.declared"));
}

#[test]
fn a_declared_change_needs_a_declared_plan_change() {
    use plinth_audit::compare::parse_args;
    let argv = |v: &[&str]| v.iter().map(|s| (*s).to_string()).collect::<Vec<_>>();
    assert!(parse_args(&argv(&["a", "b", "--declare-change", "contact-merge"])).is_err());
    assert!(parse_args(&argv(&["a", "b", "--declared", "EQUIVALENT", "--declare-change", "contact-merge"])).is_err());
    let ok = parse_args(&argv(&["a", "b", "--declared", "PLAN-CHANGED", "--declare-change", "contact-merge"])).expect("parses");
    assert!(ok.declared_changes.contact_merge);
    assert!(parse_args(&argv(&["a", "b", "--declared", "PLAN-CHANGED", "--declare-change", "nonsense"])).is_err());
    assert!(parse_args(&argv(&["a", "b", "--declared", "SORT-OF"])).is_err());
}

/// A declared contact merge excuses a contact that moved beyond the 0.1 mm match in N2, but the
/// load path of that contact (the survivor) is still held to N3's thinning limit. The reference's
/// first contact moves 0.11 mm and the first member is thinned by 20 % (below the 15 % limit
/// without an I3 margin of 1.5x).
#[test]
fn a_declared_survivor_is_judged_by_n3_and_a_relocated_contact_alone_passes() {
    let b = base();
    // Records 0 to 3 are the first support chain (contact down to the plate); record 2 is a member on it.
    let tip = kind_index(&b, RecordKind::ContactSphere, 0);
    let member = 2;
    assert_eq!(b.models[0].records[member].kind, RecordKind::Member, "fixture layout");
    let declare = |a: &mut Args| {
        a.declared = Some(plinth_audit::compare::classify::DeclaredClass::PlanChanged);
        a.declared_changes.contact_merge = true;
    };
    // A contact moved alone is cut from its path: the comparison cannot be made, which is
    // INDETERMINATE, not a pass.
    {
        let mut m = b.clone();
        apply(&mut m, &Mutation::Move { record: tip, delta: [0.11, 0.0, 0.0] });
        let run = run_pair("n3-survivor-alone", &b, &m, declare);
        assert_eq!(row(&run, "N3").status, Status::Indeterminate, "{}", show(&run));
    }
    let relocated = |thin: Option<f64>| {
        let mut m = b.clone();
        for record in 0..=3 {
            apply(&mut m, &Mutation::Move { record, delta: [0.11, 0.0, 0.0] });
        }
        if let Some(f) = thin {
            apply(&mut m, &Mutation::ScaleRecord { record: member, factor: f });
        }
        m
    };
    // Control: relocation alone, declared, is a merge and the path is unchanged.
    let run = run_pair("n3-survivor-move", &b, &relocated(None), declare);
    assert_eq!(row(&run, "N2").status, Status::Pass, "{}", show(&run));
    assert_eq!(row(&run, "N3").status, Status::Pass, "{}", show(&run));
    // The defect: relocation plus a 20 % thinner member on the same path.
    let run = run_pair("n3-survivor-thin", &b, &relocated(Some(0.8)), declare);
    assert_eq!(row(&run, "N2").status, Status::Pass, "N2 excuses the relocation:\n{}", show(&run));
    assert_eq!(row(&run, "N3").status, Status::Fail, "N3 must still see the survivor's thinner path:\n{}", show(&run));
    // Control: the same thinning without the relocation fails N3 as before.
    let run = run_pair("n3-thin-only", &b, &{
        let mut m = b.clone();
        apply(&mut m, &Mutation::ScaleRecord { record: member, factor: 0.8 });
        m
    }, declare);
    assert_eq!(row(&run, "N3").status, Status::Fail, "{}", show(&run));
}
