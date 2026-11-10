//! Read-only CAL-1 sliced-file measurements. Writes only into a new output folder.
use plinth_calibration::{
    Result,
    goo::{self, Goo},
    section::{self, SectionMesh},
    spec::PlateSpec,
};
use plinth_chitubox::{Model, Project, SupportRecord, Triangle};
use serde_json::{Value, json};
use std::{collections::BTreeMap, fmt::Write as _, path::Path};

fn project(dir: &Path, name: &str) -> Result<Project> {
    Ok(plinth_chitubox::parse(&std::fs::read(dir.join(name))?)?)
}
fn bounds(mesh: &[Triangle]) -> [[f32; 3]; 2] {
    let mut b = [[f32::INFINITY; 3], [f32::NEG_INFINITY; 3]];
    for p in mesh.iter().flatten() {
        for i in 0..3 {
            b[0][i] = b[0][i].min(p[i]);
            b[1][i] = b[1][i].max(p[i]);
        }
    }
    b
}
fn bits(mesh: &[Triangle]) -> Vec<u32> {
    mesh.iter()
        .flatten()
        .flatten()
        .map(|f| f.to_bits())
        .collect()
}
fn mesh_diff(a: &[Triangle], b: &[Triangle]) -> Value {
    let (a, b) = (bits(a), bits(b));
    let changed = a.iter().zip(&b).filter(|(x, y)| x != y).count();
    let max = a
        .iter()
        .zip(&b)
        .map(|(x, y)| (f32::from_bits(*x) - f32::from_bits(*y)).abs())
        .fold(0f32, f32::max);
    json!({"equal_bits":a==b,"coordinates_before":a.len(),"coordinates_after":b.len(),"changed_paired_coordinates":changed,"max_abs_change_mm":max})
}
fn fields(r: &SupportRecord) -> BTreeMap<String, Value> {
    let mut f = BTreeMap::new();
    f.insert("type".into(), json!(r.record_type.0));
    for (key, v) in [("start", r.start), ("end", r.end)] {
        for (i, v) in v.into_iter().enumerate() {
            f.insert(
                format!("{key}[{i}]"),
                json!({"value":v,"bits":format!("{:08x}",v.to_bits())}),
            );
        }
    }
    for (key, v) in [("r1", r.r1), ("r2", r.r2)] {
        f.insert(
            key.into(),
            json!({"value":v,"bits":format!("{:08x}",v.to_bits())}),
        );
    }
    for (i, v) in r.tail.into_iter().enumerate() {
        f.insert(format!("tail[{i}]"), json!(format!("{v:08x}")));
    }
    f
}
fn comparison(dir: &Path) -> Result<Value> {
    let a = project(dir, "A1-calibration-not-print-candidate.chitubox")?;
    let b = project(dir, "A1-resaved.chitubox")?;
    let models=a.models.iter().zip(&b.models).map(|(a,b)| {
        let supports=match (&a.supports,&b.supports) {
            (Some(a),Some(b)) => {
                let records:Vec<_>=a.records.iter().zip(&b.records).enumerate().map(|(i,(a,b))| {
                    let fa=fields(a); let fb=fields(b);
                    let diff: BTreeMap<_,_>=fa.iter().filter(|(k,v)|Some(*v)!=fb.get(*k)).map(|(k,v)|(k,json!({"before":v,"after":fb.get(k)}))).collect();
                    json!({"record":i,"fields_equal":fa==fb,"field_diff":diff,"before_fields":fa,"after_fields":fb,"mesh":mesh_diff(&a.mesh,&b.mesh)})
                }).collect();
                json!({"presets_equal":a.presets==b.presets,"attrs_equal":a.attrs==b.attrs,"attrs_before":a.attrs,"attrs_after":b.attrs,"presets_before":a.presets.iter().map(|p|json!([p.id,p.name])).collect::<Vec<_>>(),"presets_after":b.presets.iter().map(|p|json!([p.id,p.name])).collect::<Vec<_>>(),"unreferenced_blob_equal":a.unreferenced_blob==b.unreferenced_blob,"record_counts":[a.records.len(),b.records.len()],"records":records})
            }, _=>json!({"presence_equal":a.supports.is_some()==b.supports.is_some()})
        };
        json!({"name_before":a.name,"name_after":b.name,"translate_before":a.translate,"translate_after":b.translate,"translate_bits_equal":a.translate.map(f32::to_bits)==b.translate.map(f32::to_bits),"body_mesh":mesh_diff(&a.mesh,&b.mesh),"supports":supports})
    }).collect::<Vec<_>>();
    Ok(
        json!({"header_f16":[a.header_f16,b.header_f16],"preview_equal":a.preview==b.preview,"preview_size_before":a.preview.as_ref().map(|p|[p.width,p.height]),"preview_size_after":b.preview.as_ref().map(|p|[p.width,p.height]),"model_counts":[a.models.len(),b.models.len()],"models":models}),
    )
}
fn mesh_summary(m: &Model) -> Value {
    json!({"translate":m.translate,"body_bounds_local":bounds(&m.mesh),"supports_bounds_local":m.supports.as_ref().map(|s|s.records.iter().map(|r|bounds(&r.mesh)).collect::<Vec<_>>())})
}
fn track_tips(
    m: &Model,
    goo: &Goo,
    layer: usize,
    spans: &[goo::Span],
    blobs: &[goo::Blob],
) -> Vec<Value> {
    let Some(sup) = &m.supports else {
        return vec![];
    };
    let pitch = goo.header.pitch();
    // GOO Z labels the top of a layer. Also record the midpoint sampling hypothesis;
    // the PDF does not specify ChituBox's exact intersection plane.
    let z = f64::from(goo.layers[layer].z);
    let local_z = z - f64::from(m.translate[2]) + f64::from(bounds(&m.mesh)[0][2]);
    sup.records.iter().enumerate().filter(|(i,r)| *i<20 && *i%2==1 && r.record_type==plinth_chitubox::RecordType::TIP_CONE).filter_map(|(i,r)| {
        let d = section::sub(r.end.map(f64::from),r.start.map(f64::from));
        let f = (local_z-f64::from(r.start[2]))/d[2];
        if !(-0.1..=0.6).contains(&f) { return None; }
        let xy: [f64;2] = std::array::from_fn(|k| f64::from(goo.header.resolution[k])/2.0 + 0.5 - (f64::from(r.start[k])+f*d[k]+f64::from(m.translate[k]))/pitch[k]);
        let hit=blobs.iter().enumerate().find(|(_,b)| b.span_ids.iter().any(|&j| {
            let s=spans[j]; xy[1]>=f64::from(s.y) && xy[1]<f64::from(s.y+1) && xy[0]>=f64::from(s.x0) && xy[0]<f64::from(s.x1)
        }));
        Some(json!({"record":i,"layer":layer+1,"z_top":z,"axis_fraction_at_z_top":f,"predicted_xy_at_z_top":xy,"blob_index":hit.map(|(j,_)|j),"blob":hit.map(|(_,b)|b)}))
    }).collect()
}
fn process(dir: &Path, out: &Path, name: &str, project_name: &str) -> Result<()> {
    let bytes = std::fs::read(dir.join(format!("{name}.goo")))?;
    let goo = Goo::parse(&bytes)?;
    if goo.to_bytes()? != bytes {
        return Err("roundtrip differs".into());
    }
    let p = project(dir, project_name)?;
    let pitch = goo.header.pitch();
    let pixel_area = pitch[0] * pitch[1];
    let mut table = String::from(
        "file\tlayer_1based\tz_mm\tblob\tpixels\tweighted_pixels\tarea_mm2\tweighted_area_mm2\tcx\tcy\tx0\ty0\tx1\ty1\toverlap_below_pixels\tisland\n",
    );
    let mut previous = Vec::new();
    let mut layers = Vec::new();
    let mut islands = Vec::new();
    let mut grey_hist = BTreeMap::<u8, u64>::new();
    let mut tips = Vec::new();
    for (i, layer) in goo.layers.iter().enumerate() {
        let spans = goo.spans(i)?;
        for s in &spans {
            *grey_hist.entry(s.value).or_default() += u64::from(s.x1 - s.x0);
        }
        let blobs = goo::blobs(&spans, &previous);
        if name.starts_with('a') {
            tips.extend(track_tips(&p.models[0], &goo, i, &spans, &blobs));
        }
        for (j, b) in blobs.iter().enumerate() {
            let island = i > 0 && b.overlap_below_pixels == 0;
            writeln!(
                table,
                "{name}\t{}\t{:.6}\t{j}\t{}\t{:.9}\t{:.9}\t{:.9}\t{:.4}\t{:.4}\t{}\t{}\t{}\t{}\t{}\t{island}",
                i + 1,
                layer.z,
                b.pixels,
                b.weighted_pixels,
                b.pixels as f64 * pixel_area,
                b.weighted_pixels * pixel_area,
                b.centroid[0],
                b.centroid[1],
                b.bounds[0],
                b.bounds[1],
                b.bounds[2],
                b.bounds[3],
                b.overlap_below_pixels
            )?;
            if island {
                islands.push(json!({"layer":i+1,"z":layer.z,"blob":b}));
            }
        }
        layers.push(json!({"layer":i+1,"z":layer.z,"exposure":layer.exposure,"definition_hex":layer.definition.iter().map(|v|format!("{v:02x}")).collect::<String>(),"blobs":blobs}));
        previous = spans;
    }
    std::fs::write(out.join(format!("{name}-blobs.tsv")), table)?;
    std::fs::write(
        out.join(format!("{name}.json")),
        serde_json::to_vec_pretty(
            &json!({"status":"M","header":goo.header,"pixel_pitch_mm":pitch,"pixel_area_mm2":pixel_area,"mesh":mesh_summary(&p.models[0]),"nonzero_grey_histogram":grey_hist,"islands":islands,"layers":layers,"tip_tracks":tips}),
        )?,
    )?;
    println!(
        "{name}: {} layers, {} islands, pitch {pitch:?}",
        goo.layers.len(),
        islands.len()
    );
    Ok(())
}
fn sections(dir: &Path, out: &Path) -> Result<()> {
    let spec: PlateSpec = serde_json::from_slice(&std::fs::read(dir.join("spec.json"))?)?;
    let mut table = String::from("piece\tcontact\tz_mm\tmesh_area_mm2\n");
    for s in &spec.specimens {
        let p = project(
            dir,
            &format!("{}-calibration-not-print-candidate.chitubox", s.id),
        )?;
        let m = &p.models[0];
        let min = bounds(&m.mesh)[0][2];
        let origin = [0.0, 0.0, f64::from(min - m.translate[2])];
        let groups: Vec<Vec<SectionMesh>> = if let Some(sup) = &m.supports {
            (0..s.contact_sites.len())
                .map(|i| {
                    sup.records[i * plinth_calibration::build::PARTS_PER_HEAD
                        ..(i + 1) * plinth_calibration::build::PARTS_PER_HEAD]
                        .iter()
                        .map(|r| SectionMesh::new(&r.mesh, origin, [0.0, 0.0, 1.0]))
                        .collect()
                })
                .collect()
        } else if s.id == "W" {
            s.contact_sites
                .iter()
                .map(|xy| {
                    vec![SectionMesh::new(
                        &plinth_calibration::witness::column(&m.mesh, *xy),
                        origin,
                        [0.0, 0.0, 1.0],
                    )]
                })
                .collect()
        } else {
            vec![vec![SectionMesh::new(&m.mesh, origin, [0.0, 0.0, 1.0])]]
        };
        for (i, g) in groups.iter().enumerate() {
            let stations: Vec<f64> = if s.id.starts_with('K') {
                vec![4.55, 5.0, 6.0, 8.45, 8.55, 9.0, 9.1, 9.2]
            } else {
                vec![
                    4.40, 4.45, 4.50, 4.55, 4.60, 4.85, 4.88, 4.90, 4.95, 5.0, 5.05, 5.1, 5.125,
                ]
            };
            for z in stations {
                writeln!(
                    table,
                    "{}\t{i}\t{z:.6}\t{:.9}",
                    s.id,
                    section::union_area(g, z)
                )?;
            }
        }
    }
    std::fs::write(out.join("mesh-sections.tsv"), table)?;
    Ok(())
}
fn run() -> Result<()> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    let [input, output] = args.as_slice() else {
        return Err("usage: cal1-goo READ_ONLY_INPUT NEW_OUTPUT".into());
    };
    let (dir, out) = (Path::new(input), Path::new(output));
    std::fs::create_dir(out)?;
    for (name, p) in [
        ("a0", "A0"),
        ("a1", "A1"),
        ("a1-resave", "A1"),
        ("b100", "B-100"),
        ("s100", "S-100"),
        ("s324", "S-324"),
        ("k15", "K-15"),
        ("k60", "K-60"),
        ("w", "W"),
    ] {
        process(
            dir,
            out,
            name,
            &format!("{p}-calibration-not-print-candidate.chitubox"),
        )?;
    }
    sections(dir, out)?;
    std::fs::write(
        out.join("a1-save-comparison.json"),
        serde_json::to_vec_pretty(&comparison(dir)?)?,
    )?;
    Ok(())
}
fn main() {
    if let Err(e) = run() {
        eprintln!("{e}");
        std::process::exit(1);
    }
}
