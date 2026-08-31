//! Generator for `bare_cavity_ceiling.chitubox`. Not built by this crate's
//! own `Cargo.toml` (there is no `[[bin]]` entry for it) -- it is a record of
//! how the fixture was produced, in the same spirit as
//! `fixtures/double_load/generate.rs` and `fixtures/buried_arm/generate.rs`.
//! To regenerate: copy this file into a throwaway crate with `plinth-audit`
//! as a path dependency (`plinth-audit = { path = "../../.." }` from a crate
//! rooted at this file's own directory) and run it with two arguments: the
//! output path, and the variant (`witness`, the shipped fixture, or
//! `zero_support`, kept for reproducing the crash documented in the README).
//!
//! ## The planted defect
//!
//! A closed hollow box: 7.0 x 7.0 x 5.0 mm outer shell, 5.0 x 5.0 x 3.0 mm
//! cavity (1.0 mm floor, walls, and ceiling), resting flat on the plate.
//! NO support of any kind touches the cavity -- not from its floor, not from
//! its walls, nothing rising into it. The ceiling closes over at its own
//! birth layer with the only carrier anywhere nearby being the cavity's own
//! side wall, 2.5 mm away at the centre: `i9_unsupported_span`'s measured
//! allowance is 1.344976 mm, so this must fail it. `i7_island_coverage`
//! cannot see this defect at all -- the ceiling is attached along its entire
//! rim, so its early-exit (`if is_attached(&comp, ...) { continue; }`) skips
//! the whole component before ever asking how far its centre is from
//! anything holding it up. This is Finding 1's cavity-ceiling case, made
//! physical.
//!
//! ## Why a witness island exists
//!
//! A model with literally zero support records (the textbook shape
//! `docs/specs/audit-span-criterion.md` §10 calls `span_cavity_open`) makes
//! `checks::load_ownership::LoadOwnership::run`'s clause (d) compute
//! `ratio = f64::INFINITY` (since `lo`, the sum of unclamped implied areas,
//! is `0.0` over zero contacts) and feed it straight to `Json::num`, which
//! asserts finiteness and panics -- **crashing the entire audit before I7 or
//! I9 ever run.** Confirmed against the real, current tree; see the README's
//! "The load_ownership.rs crash" section, and
//! `fixtures/long_ledge/README.md`, which independently found and documented
//! the identical panic at the identical line. Because `tests/fixtures.rs`'s
//! `every_fixture_trips_the_check_it_names` runs every discovered fixture in
//! one `for` loop with no `catch_unwind`, shipping the zero-support shape as
//! the harness-discovered fixture would abort that ENTIRE test partway
//! through, silently skipping every other fixture sorting after this one --
//! exactly the "a gate rested on nothing" failure class this project keeps
//! reproducing. So the shipped fixture instead carries ONE well-formed
//! support contact, on a SEPARATE, fully detached witness island far away in
//! +X (x in [19.4, 20.6]), whose only job is to make `ctx.seats` non-empty.
//! It shares no mesh connectivity, no raster footprint, and no I4 surface
//! component with the cavity box, and its own footprint (1.2 x 1.2 mm) is
//! sized small enough that it does not ALSO trip `i9_unsupported_span` on
//! itself (see the comment at `witness_chain_and_mesh` for the arithmetic).
//!
//! `zero_support` is kept here, unused by the shipped fixture, purely so the
//! crash it triggers stays reproducible without hunting through git history.

use plinth_audit::consts as c;
use plinth_audit::context::{AuditContext, AuditOptions};
use plinth_audit::reader::RecordKind;
use plinth_audit::run;
use plinth_audit::synth::{box_soup, frustum_soup, sphere_soup, SynthFile, SynthModel, SynthRecord};

/// Reverse the winding of every triangle in a soup -- flips which side is
/// "outward". Required for the cavity's own surface: the rasteriser's
/// scanline fill (`raster.rs::slice_layer`, via its signed `dir` field, and
/// `RunRaster::from_mesh`) is a nonzero-winding-rule accumulation, not a
/// plain even-odd count -- confirmed empirically below, where the combined
/// mesh's `component_volume_mm3` comes back `[245.0, -75.0, ...]`: the
/// cavity's box_soup, wound opposite to the outer shell, contributes a
/// genuinely NEGATIVE signed volume, which is what carves the hole. Point
/// containment itself (`Mesh::point_inside`, pure ray parity) would have
/// worked with either winding; the rasteriser would not.
fn reverse_winding(soup: Vec<[[f32; 3]; 3]>) -> Vec<[[f32; 3]; 3]> {
    soup.into_iter().map(|[a, b, c]| [a, c, b]).collect()
}

/// Drop `synth::sphere_soup`'s zero-area pole triangles -- a pre-existing,
/// documented property of the shared test helper (both poles share a
/// vertex), reproduced verbatim from `fixtures/double_load/generate.rs` and
/// `fixtures/buried_arm/generate.rs`'s own copies of this same filter, and
/// not something this fixture owns or may fix.
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

/// The hollow-box mesh carrying the planted defect, shared by both variants.
///
/// Outer shell: 7.0 x 7.0 mm footprint, 5.0 mm tall, resting directly on the
/// plate (`translate_z = 0` => `z_plate = bbox.min.z - 0 = 0`).
/// Cavity (void): 5.0 x 5.0 mm interior, z in [1.0, 4.0] -- 1.0 mm floor,
/// 1.0 mm walls all round, 1.0 mm ceiling.
fn hollow_box_mesh() -> Vec<[[f32; 3]; 3]> {
    let outer_min = [-3.5, -3.5, 0.0];
    let outer_max = [3.5, 3.5, 5.0];
    let cavity_min = [-2.5, -2.5, 1.0];
    let cavity_max = [2.5, 2.5, 4.0];
    let mut mesh = box_soup(outer_min, outer_max);
    mesh.extend(reverse_winding(box_soup(cavity_min, cavity_max)));
    mesh
}

/// The detached witness island and its one support chain -- see the module
/// doc's "Why a witness island exists".
///
/// Pattern reused from `fixtures/double_load/generate.rs`'s own `column()`
/// closure (one instance, not two): a floating box sized so ONE tip's
/// implied area equals its own single down-facing patch exactly.
///
/// A single `SynthModel::translate_mm` is shared by the WHOLE mesh (both the
/// cavity box and this island), and `z_plate = combined_bbox.min.z -
/// translate_z`. The cavity box's own min z is 0.0 and `translate_mm` stays
/// `[0,0,0]` so the cavity box's own `z_plate = 0` is undisturbed -- so this
/// island's LOCAL z-coordinates are written to already float above that
/// shared `z_plate = 0` directly, rather than via a second `translate_z`
/// (there is only one, and it is spoken for).
///
/// The island's own footprint must ALSO clear `i9_unsupported_span`'s
/// allowance at its own birth layer. A first cut at 3.0 x 2.0 mm (matching
/// `double_load`'s own proportions) tripped `SPAN_UNSUPPORTED` on itself: a
/// single point-like tip under a 6 mm^2 rectangle leaves a far corner at up
/// to `sqrt(1.5^2 + 1.0^2) = 1.803` mm Euclidean, which the chamfer metric's
/// up-to-+12.04% overestimate (`audit-span-criterion.md` §3.6) pushes to
/// ~2.02 mm -- over the 1.344976 mm allowance regardless of where the tip
/// sits under it. A square 1.2 x 1.2 mm footprint keeps every corner at
/// 0.849 mm from a centred tip, ~0.951 mm even under the chamfer
/// overestimate: comfortable margin.
fn witness_chain_and_mesh() -> (Vec<[[f32; 3]; 3]>, Vec<SynthRecord>) {
    let f = std::f64::consts::PI;
    let x_allowed = c::X_ALLOWED; // 12.0
    let ball_factor = c::BALL_FACTOR_STUB; // 1.5

    let x0 = 20.0_f64; // far clear of the cavity box (x in [-3.5, 3.5])
    let d_total = 1.44_f64; // 1.2 x 1.2 mm bottom face
    let mesh = box_soup([x0 - 0.6, -0.6, 3.0], [x0 + 0.6, 0.6, 5.0]);

    let r_tip = (d_total / (x_allowed * f)).sqrt();
    let r_ball = ball_factor * r_tip;

    let h_req_ball_term = c::ball_seat_factor(ball_factor) * r_tip;
    // I1's floor is max(0.100, 0.050, ball_term); ball_term is small at this
    // r_tip, so 0.100 mm governs -- deliver comfortably above it.
    let h_delivered = 0.15_f64;
    assert!(h_delivered > h_req_ball_term);
    assert!(h_delivered > 0.100);

    let p0 = [x0, 0.0, 3.0 + h_delivered];
    let end = [x0, 0.0, p0[2] - 2.0 * r_ball];
    let centre = [x0, 0.0, (p0[2] + end[2]) * 0.5];

    let tip = SynthRecord::new(RecordKind::ContactSphere, p0, end, r_ball, r_ball)
        .with_soup(drop_zero_area(sphere_soup(centre, r_ball, 8, 16)));

    let taper_end = [x0, 0.0, 1.2];
    let arm_r = 0.5_f64;
    let taper = SynthRecord::new(RecordKind::ContactTaper, centre, taper_end, r_tip, arm_r)
        .with_declared_depth(h_delivered)
        .with_matching_soup();

    // z_plate = 0.0, the plate shared with the cavity box.
    let member_end = [x0, 0.0, 0.0];
    let member = SynthRecord::new(RecordKind::Member, taper_end, member_end, arm_r, arm_r)
        .with_matching_soup();

    let raft_r = c::RAFT_PAD_DIAMETER_MM * 0.5;
    let raft_top = [member_end[0], member_end[1], member_end[2] + c::RAFT_PAD_HEIGHT_MM];
    let raft = SynthRecord::new(
        RecordKind::RaftPadOrBridge,
        member_end,
        member_end,
        c::RAFT_PAD_DIAMETER_MM,
        c::RAFT_PAD_HEIGHT_MM,
    )
    .as_pad()
    .with_soup(frustum_soup(member_end, raft_r, raft_top, raft_r, 20));

    eprintln!(
        "witness island: r_tip={r_tip:.9} r_ball={r_ball:.9} h_req_ball_term={h_req_ball_term:.9} \
         a_implied={:.9} (should equal d_total={d_total})",
        x_allowed * f * r_tip * r_tip
    );

    (mesh, vec![tip, taper, member, raft])
}

fn build(variant: &str) -> SynthFile {
    let mut mesh = hollow_box_mesh();
    let records = match variant {
        "zero_support" => Vec::new(),
        "witness" => {
            let (island_mesh, records) = witness_chain_and_mesh();
            mesh.extend(island_mesh);
            records
        }
        other => panic!("unknown variant {other:?}, want zero_support or witness"),
    };
    SynthFile::new().with_model(SynthModel {
        name: "bare-cavity-ceiling".into(),
        translate_mm: [0.0, 0.0, 0.0],
        mesh,
        records,
    })
}

/// Diagnostic-only entry point: writes the fixture, re-parses it through the
/// REAL reader, spot-checks closure and point-containment, then runs the
/// real audit (panic caught, so `zero_support` prints its crash instead of
/// aborting this whole program) and prints the resulting JSON document.
fn main() {
    let args: Vec<String> = std::env::args().collect();
    let out_path = args.get(1).expect("usage: gen_bare_cavity_ceiling <out.chitubox> <witness|zero_support>");
    let variant = args.get(2).map(String::as_str).unwrap_or("witness");

    let sf = build(variant);
    let bytes = sf.to_bytes();
    std::fs::write(out_path, &bytes).expect("write fixture");
    eprintln!("[{variant}] wrote {} bytes to {out_path}", bytes.len());

    let file = plinth_audit::reader::ChituboxFile::parse(std::path::Path::new(out_path), &bytes)
        .expect("re-parse must succeed");
    let options = AuditOptions::default();
    let audit = AuditContext {
        file: &file,
        options: &options,
        sidecar: None,
        sidecar_absence: None,
    };
    let ctx = plinth_audit::context::ModelContext::build(&audit, 0);
    eprintln!("closure: {}", ctx.model_mesh.closure.summary());
    eprintln!("is_closed: {}", ctx.model_mesh.closure.is_closed());
    eprintln!("bbox: {:?}", ctx.model_mesh.bbox);
    eprintln!(
        "raw_signed_volume_mm3: {}, global_orientation_sign: {}",
        ctx.model_mesh.raw_signed_volume_mm3, ctx.model_mesh.global_orientation_sign
    );
    eprintln!("component_volume_mm3: {:?}", ctx.model_mesh.component_volume_mm3);

    use plinth_audit::mesh::Inside;
    let checks = [
        ("cavity_centre", [0.0, 0.0, 2.5], Inside::Outside),
        ("wall", [3.0, 0.0, 2.5], Inside::Inside),
        ("floor", [0.0, 0.0, 0.5], Inside::Inside),
        ("ceiling", [0.0, 0.0, 4.5], Inside::Inside),
        ("outside", [10.0, 0.0, 2.5], Inside::Outside),
    ];
    for (name, p, want) in checks {
        let got = ctx.model_mesh.point_inside(p);
        eprintln!("point_inside {name}{p:?} = {got:?} (want {want:?})");
        assert_eq!(got, want, "{name}");
    }

    let result =
        std::panic::catch_unwind(|| run::audit_file(std::path::Path::new(out_path), &options, None));
    match result {
        Ok(r) => println!("{}", r.document.to_pretty()),
        Err(_) => eprintln!(
            "[{variant}] audit_file PANICKED (see the backtrace above) -- no document produced"
        ),
    }
}
