//! Write a new CAL-1 A1b and diagnose its midpoint continuity; never overwrite inputs.
use plinth_calibration::{
    Result,
    goo::Goo,
    goo_grid::{self, Boundary, Grid},
    light,
};
use plinth_chitubox::{Project, Triangle};
use serde_json::{Value, json};
use std::path::Path;
fn read(dir: &Path, name: &str) -> Result<Project> {
    Ok(plinth_chitubox::parse(&std::fs::read(dir.join(name))?)?)
}
fn assess(name: &str, p: &Project, frame: [f64; 3], grid: Grid) -> Result<Vec<Value>> {
    let m = &p.models[0];
    let sup = m.supports.as_ref().unwrap();
    let mesh: Vec<Triangle> = m
        .mesh
        .iter()
        .chain(sup.records.iter().flat_map(|r| r.mesh.iter()))
        .copied()
        .collect();
    let mut out = Vec::new();
    for mode in [Boundary::Centre, Boundary::Touch] {
        let layers = goo_grid::slice(&mesh, frame, grid, mode)?;
        let islands = goo_grid::islands(&layers, grid);
        println!(
            "{name} {mode:?}: {} layers, {} islands",
            layers.len(),
            islands.len()
        );
        if name == "A1b" && !islands.is_empty() {
            return Err("A1b whole-project midpoint island: refuse output".into());
        }
        out.push(json!({"file":name,"mode":format!("{mode:?}"),"layers_checked":layers.len(),"islands":islands,"onset_layers":([61,62,63].map(|i|json!({"layer":i,"blobs":plinth_calibration::goo::blobs(&layers[i-1],&layers[i-2])})))}));
    }
    Ok(out)
}
fn onset(name: &str, p: &Project, frame: [f64; 3], grid: Grid) -> Result<Vec<Value>> {
    let s = p.models[0].supports.as_ref().unwrap();
    let mut out = Vec::new();
    for (part, mesh) in [
        ("contact_ball_record_4", &s.records[4].mesh),
        ("cone_record_5", &s.records[5].mesh),
    ] {
        for boundary in [Boundary::Centre, Boundary::Touch] {
            let layers = goo_grid::slice(mesh, frame, grid, boundary)?;
            out.push(json!({"file":name,"part":part,"mode":format!("{boundary:?}"),"layers":([61,62,63].map(|i|json!({"layer":i,"blobs":plinth_calibration::goo::blobs(&layers[i-1],&layers[i-2])})))}));
        }
    }
    Ok(out)
}
fn attribution(goo: &Goo, models: [&Project; 3], frame: [f64; 3], grid: Grid) -> Result<Value> {
    use plinth_calibration::goo::{Span, blobs};
    let current = goo.spans(61)?;
    let previous = goo.spans(60)?;
    let island = blobs(&current, &previous)
        .into_iter()
        .find(|b| b.overlap_below_pixels == 0)
        .ok_or("original A1 island witness absent")?;
    let mask: Vec<Span> = island.span_ids.iter().map(|&i| current[i]).collect();
    let overlap = |spans: &[Span]| {
        mask.iter()
            .map(|a| {
                spans
                    .iter()
                    .filter(|b| a.y == b.y)
                    .map(|b| u64::from(a.x1.min(b.x1).saturating_sub(a.x0.max(b.x0))))
                    .sum::<u64>()
            })
            .sum::<u64>()
    };
    let mut parts = Vec::new();
    for (name, p) in ["A0", "A1", "A1b"].into_iter().zip(models) {
        let records = &p.models[0].supports.as_ref().unwrap().records;
        for i in [4, 5] {
            let layers = goo_grid::slice(&records[i].mesh, frame, grid, Boundary::Touch)?;
            parts.push(json!({"file":name,"record":i,"native_island_pixels_in_part_layer_61":overlap(&layers[60]),"native_island_pixels_in_part_layer_62":overlap(&layers[61])}));
        }
    }
    Ok(json!({"native_island":island,"parts":parts}))
}
fn run() -> Result<()> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    let [input, output] = args.as_slice() else {
        return Err("usage: cal1-a1b INPUT NEW_OUTPUT".into());
    };
    let (dir, out) = (Path::new(input), Path::new(output));
    std::fs::create_dir(out)?;
    let a0 = read(dir, "A0-calibration-not-print-candidate.chitubox")?;
    let a1 = read(dir, "A1-calibration-not-print-candidate.chitubox")?;
    let goo = Goo::parse(&std::fs::read(dir.join("a1.goo"))?)?;
    let grid = Grid::from_header(&goo.header);
    let frame = goo_grid::offset(&a0.models[0]);
    let bytes = light::thin(&a0)?.to_bytes()?;
    // Measure the emitted file, not the planned fields or an in-memory mesh.
    std::fs::write(
        out.join("A1b-calibration-not-print-candidate.chitubox"),
        bytes,
    )?;
    let fixed = read(out, "A1b-calibration-not-print-candidate.chitubox")?;
    let tips = light::compare(&a0, &fixed)?;
    let mut data = Vec::new();
    let mut parts = Vec::new();
    for (name, p) in [("A0", &a0), ("A1", &a1), ("A1b", &fixed)] {
        data.extend(assess(name, p, frame, grid)?);
        parts.extend(onset(name, p, frame, grid)?);
    }
    let radii: Vec<_> = fixed.models[0]
        .supports
        .as_ref()
        .unwrap()
        .records
        .iter()
        .enumerate()
        .filter(|(i, _)| *i < 20 && *i % 2 == 1)
        .map(|(i, r)| json!({"record":i,"r1_mm":r.r1}))
        .collect();
    std::fs::write(
        out.join("A1b-grid-check.json"),
        serde_json::to_vec_pretty(
            &json!({"label":plinth_calibration::LABEL,"ChituBox_A1b_slice":"Unverified: owner must slice a1b.goo","grid":{"pitch_mm":grid.pitch,"height_mm":grid.height,"resolution":grid.resolution,"sample":"layer midpoint"},"scope":"written triangles for complete body and every support; exact same-XY support overlap; 8-connected components; no AA model","baseline_caution":"Touch predicts original A1 same island layer/bounds but eight pixels, versus ChituBox six; Centre also flags record 3. This is a witness, not an exact ChituBox rasterizer.","field_diff":"Exactly ten main-cone r1 fields and ten canonical stored meshes differ from A0; reverting precisely them returns byte-identical A0. Every tip endpoint/ball/body/other record unchanged.","radii":radii,"whole_project":data,"record_5_onset_parts":parts,"native_island_attribution":attribution(&goo,[&a0,&a1,&fixed],frame,grid)?,"axis_normal_diagnostics_not_C4_qualification":tips}),
        )?,
    )?;
    println!("A1b radii: {}", serde_json::to_string(&radii)?);
    Ok(())
}
fn main() {
    if let Err(e) = run() {
        eprintln!("{e}");
        std::process::exit(1);
    }
}
