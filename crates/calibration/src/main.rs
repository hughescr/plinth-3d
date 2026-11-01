//! Dedicated calibration entry point, never a production command flag.
use plinth_calibration::{
    LABEL, Result, build, light,
    section::{self, SectionMesh},
    spec::{Body, PlateSpec},
};
use plinth_chitubox::{Project, RecordType as Ty};
use std::{fmt::Write as _, path::Path};

fn save(path: &Path, p: &Project) -> Result<Project> {
    std::fs::write(path, p.to_bytes()?)?;
    Ok(plinth_chitubox::parse(&std::fs::read(path)?)?)
}
fn measure_piece(
    s: &plinth_calibration::spec::Specimen,
    p: &Project,
    out: &mut String,
) -> Result<()> {
    let m = &p.models[0];
    let face = if m.mesh.is_empty() {
        0.0
    } else {
        m.mesh
            .iter()
            .flatten()
            .map(|p| f64::from(p[2]))
            .fold(f64::INFINITY, f64::min)
    };
    let plate = [0.0, 0.0, face - f64::from(s.lift)];
    if let Some(sup) = &m.supports {
        for (i, head) in sup.records[..s.contact_sites.len() * build::PARTS_PER_HEAD]
            .chunks_exact(build::PARTS_PER_HEAD)
            .enumerate()
        {
            let pad = &sup.records[sup.records.len() - s.contact_sites.len() + i];
            let meshes: Vec<_> = head
                .iter()
                .chain(std::iter::once(pad))
                .map(|r| SectionMesh::new(&r.mesh, plate, [0.0, 0.0, 1.0]))
                .collect();
            let hi = f64::from(s.lift) - if s.body == Body::Witness { 0.12 } else { 0.0 };
            let min = section::minimum(&meshes, 0.001, hi, 1000);
            let design = 8.0
                * (std::f64::consts::TAU / 16.0).sin()
                * (f64::from(s.waist_diameters[i]) / 2.0).powi(2);
            let waist_at = min.at > f64::from(s.lift) - 0.83 && min.at < f64::from(s.lift) - 0.12;
            let ok = (min.area / design - 1.0).abs() <= 0.10 && waist_at;
            writeln!(
                out,
                "{}\t{}\t{:.9}\t{:.9}\t{:.6}\twaist\t{:.6}\t{:.3}\t{}\t{}",
                s.id,
                i,
                min.area,
                design,
                min.at,
                min.step,
                (min.area / design - 1.0) * 100.0,
                if ok { "mesh criterion met" } else { "FAIL" },
                LABEL
            )?;
            if !ok {
                return Err(format!(
                    "{} head {i}: written waist fails the design ±10% criterion",
                    s.id
                )
                .into());
            }
        }
    } else {
        let mesh = SectionMesh::new(&m.mesh, plate, [0.0, 0.0, 1.0]);
        let hi = m
            .mesh
            .iter()
            .flatten()
            .map(|p| f64::from(p[2]))
            .fold(f64::NEG_INFINITY, f64::max)
            - plate[2]
            - 0.001;
        let min = section::minimum(&[mesh], 0.001, hi, 2000);
        let r = if s.body == Body::K15 { 0.798f64 } else { 0.399 };
        let design = 24.0 * (std::f64::consts::TAU / 48.0).sin() * r * r;
        let ok = (min.area / design - 1.0).abs() <= 0.10 && min.at > 4.5 && min.at < 8.5;
        writeln!(
            out,
            "{}\t0\t{:.9}\t{:.9}\t{:.6}\tarm\t{:.6}\t{:.3}\t{}\t{}",
            s.id,
            min.area,
            design,
            min.at,
            min.step,
            (min.area / design - 1.0) * 100.0,
            if ok { "mesh criterion met" } else { "FAIL" },
            LABEL
        )?;
        if !ok {
            return Err(format!("{}: written arm fails design criterion", s.id).into());
        }
    }
    Ok(())
}

fn attributes(p: &Project) -> serde_json::Value {
    let s = p.models[0].supports.as_ref().unwrap();
    let mut counts = std::collections::BTreeMap::new();
    for (&attr, r) in s.attrs.iter().zip(&s.records) {
        *counts
            .entry(format!("0x{attr:x}, type {}", r.record_type.0))
            .or_insert(0u64) += 1;
    }
    serde_json::json!({"label":LABEL,"presets":s.presets.iter().map(|p|serde_json::json!({"id":p.id,"name":p.name})).collect::<Vec<_>>(),"counts":counts})
}
fn run() -> Result<()> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    let [spec_path, owner_path, out_path] = args.as_slice() else {
        return Err(
            "usage: plinth-calibration <cal1.json> <read-only-owner.chitubox> <output-directory>"
                .into(),
        );
    };
    let spec: PlateSpec = serde_json::from_slice(&std::fs::read(spec_path)?)?;
    spec.validate()?;
    let dir = Path::new(out_path);
    std::fs::create_dir_all(dir)?;
    // No overwrite: never destroy a previous review set or an owner's input.
    if std::fs::read_dir(dir)?.next().is_some() {
        return Err("output directory must be empty".into());
    }
    let mut table = String::from(
        "id\tpath\tmeasured_min_area_mm2\tdesign_area_mm2\tworld_z_mm\twhere\tstation_step_mm\terror_percent\tmesh_result\tlabel\n",
    );
    for s in &spec.specimens {
        let path = dir.join(format!("{}-calibration-not-print-candidate.chitubox", s.id));
        let p = save(&path, &build::specimen(s, &spec.head))?;
        measure_piece(s, &p, &mut table)?;
        if !p.models[0].mesh.is_empty() {
            std::fs::write(
                dir.join(format!("{}-calibration-not-print-candidate.stl", s.id)),
                plinth_calibration::reference_stl(&p.models[0]),
            )?;
        }
    }
    let source = plinth_chitubox::parse(&std::fs::read(owner_path)?)?;
    let a0 = save(
        &dir.join("A0-calibration-not-print-candidate.chitubox"),
        &light::extract(&source)?,
    )?;
    let a1 = save(
        &dir.join("A1-calibration-not-print-candidate.chitubox"),
        &light::thin(&a0)?,
    )?;
    let tips = light::compare(&a0, &a1)?;
    let eligible = tips.iter().all(|m| m.area_ratio >= 2.0);
    let mut tip_table = String::from(
        "record\tA0_exposed_area_mm2\tA1_exposed_area_mm2\tarea_ratio\tA0_axial_mm\tA1_axial_mm\tstation_step_mm\tlabel\n",
    );
    for t in &tips {
        writeln!(
            tip_table,
            "{}\t{:.9}\t{:.9}\t{:.6}\t{:.7}\t{:.7}\t{:.7}\t{}",
            t.record,
            t.a0_area_mm2,
            t.a1_area_mm2,
            t.area_ratio,
            t.a0_axial_mm,
            t.a1_axial_mm,
            t.station_step_mm,
            LABEL
        )?;
    }
    std::fs::write(dir.join("A0-A1-tips.tsv"), tip_table)?;
    let diff = serde_json::json!({"label":LABEL,"measurement":"sampled axis-normal union area of written ball and cone triangles, not field radius; no slicing claim",
        "mesh_eligible_for_A1":eligible,"A1_status":if eligible {"mesh criterion met; slice/save checks Unverified"} else {"A1 = second A0; do not use the thinned diagnostic as an A1 contrast"},
        "field_diff":"ten r1 fields and ten meshes only; reverting precisely these returns byte-identical A0",
        "written_main_tips":tips,"attribute_evidence":attributes(&a0)});
    std::fs::write(
        dir.join("A0-A1-field-diff.json"),
        serde_json::to_vec_pretty(&diff)?,
    )?;
    // Place only the reference STL at the source model's recorded XY pose.
    // Never transform A0 itself.
    std::fs::write(
        dir.join("A-reference-calibration-not-print-candidate.stl"),
        plinth_calibration::reference_stl(&a0.models[0]),
    )?;
    std::fs::write(dir.join("measurements.tsv"), table)?;
    std::fs::write(dir.join("spec.json"), std::fs::read(spec_path)?)?;
    std::fs::write(
        dir.join("provenance.json"),
        serde_json::to_vec_pretty(&serde_json::json!({
            "label":LABEL,"source_read_only":owner_path,"source_model_index":2,"band":spec.band,"shuffle_seed":spec.shuffle_seed,
            "assembled_plate":false,"pose":"translation only; individual pieces, not a plate layout",
            "K_base_choice_S":"48-sided diameter 8 mm pedestal, z 0 to 4.5; arm, flare and cap follow plan reconstruction",
            "W":"native supports with no dummy model mesh; checker may be unable to establish its plate frame",
            "section_scope":"sampled mesh measurements; no certified continuous lower bound, no sliced pixel or physical-load claim",
            "ChituBox_open_save_slice":"Unverified, not run",
            "a0_unchanged":"native mesh, translate, presets, attrs and all records retained; model name labelled; three-model preview replaced with an 800x600 native-background preview",
            "a1_writes":"fields AND stored cone meshes consistently; what ChituBox regenerates is Unverified",
            "tip_ball_count":a0.models[0].supports.as_ref().unwrap().records.iter().filter(|r|r.record_type==Ty::TIP_BALL).count()
        }))?,
    )?;
    println!(
        "{LABEL}; wrote 8 individual projects; A1 mesh eligible: {eligible}; ChituBox open/save/slice Unverified"
    );
    Ok(())
}
fn main() -> std::process::ExitCode {
    match run() {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("{LABEL}: {e}");
            std::process::ExitCode::FAILURE
        }
    }
}
