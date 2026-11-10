//! Read-only native-support measurement adapter. No production behaviour changes.
#![allow(clippy::all)]
use ck::{
    json::Json as J,
    parts::{Geom, Role},
    solid::Solid,
    v3::*,
};
use plinth_check as ck;
use plinth_chitubox as cb;
use plinth_geom::{Mesh, measure::Measure, slice::LayerGrid};
use plinth_support::load::{self, *};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::Path,
};
fn n(x: f64) -> J {
    J::Num(x)
}
fn u(x: usize) -> J {
    J::Uint(x as u64)
}
fn v(p: [f64; 3]) -> J {
    J::Arr(p.into_iter().map(n).collect())
}
fn arr(a: impl IntoIterator<Item = J>) -> J {
    J::Arr(a.into_iter().collect())
}
fn obj(a: Vec<(&str, J)>) -> J {
    J::from_pairs(a)
}
fn save(dir: &Path, name: &str, j: J) {
    fs::write(dir.join(name), j.render()).unwrap();
}
fn bounds(ts: &[cb::Triangle]) -> ([f64; 3], [f64; 3]) {
    let mut lo = [f64::INFINITY; 3];
    let mut hi = [f64::NEG_INFINITY; 3];
    for p in ts.iter().flatten() {
        for k in 0..3 {
            lo[k] = lo[k].min(p[k] as f64);
            hi[k] = hi[k].max(p[k] as f64);
        }
    }
    (lo, hi)
}
fn orig(p: V) -> V {
    let (s, c) = 65f64.to_radians().sin_cos();
    [p[0], c * p[1] - s * p[2], s * p[1] + c * p[2]]
}
fn section(p: &ck::parts::Part) -> f64 {
    match p.geom {
        Geom::Frust { ra, rb, .. } => 8. * ra.min(rb).powi(2) * (std::f64::consts::PI / 8.).sin(),
        Geom::Ball { r, .. } => std::f64::consts::PI * (r - p.sag - p.tol).max(0.).powi(2),
        _ => f64::INFINITY,
    }
}
fn outcome(o: ck::outcome::Outcome) -> J {
    obj(vec![
        ("status", J::str(format!("{:?}", o.status))),
        ("reason", J::str(o.reason)),
        ("detail", o.detail),
        (
            "located",
            arr(o.located.iter().map(|l| {
                obj(vec![
                    (
                        "record",
                        l.location.record.map_or(J::Null, |x| u(x as usize)),
                    ),
                    ("xyz", l.location.xyz.map_or(J::Null, v)),
                    ("reason", J::str(&l.reason)),
                ])
            })),
        ),
    ])
}
// Bracket contact patches directly from each validated native convex mesh's half-spaces.
// The shrink/expand allowance is the checker's own 2e-6 + face slack + canonical deviation.
fn polys(scene: &ck::scene::Scene, chain: usize, sign: f64) -> Vec<(V, Vec<ck::poly::Poly>)> {
    let ch = &scene.chains.chains[chain];
    let mut bytri: BTreeMap<u32, Vec<ck::poly::Poly>> = BTreeMap::new();
    for &pi in &ch.parts {
        let p = &scene.parts[pi];
        let fallback;
        if p.mesh.is_none() {
            fallback = Some(Solid::polytope(
                p.tris
                    .iter()
                    .copied()
                    .filter(|t| len(cross(sub(t[1], t[0]), sub(t[2], t[0]))) > 1e-8)
                    .collect(),
            ));
        } else {
            fallback = None;
        }
        let Some(Solid::Poly {
            planes, slack, dev, ..
        }) = p.mesh.as_ref().or(fallback.as_ref())
        else {
            continue;
        };
        let hs: Vec<_> = planes
            .iter()
            .zip(slack)
            .map(|((a, b), s)| (*a, b + sign * (2e-6 + s + dev)))
            .collect();
        for ti in scene.model_near(&p.bbox) {
            let poly = ck::poly::clip_all(&scene.model[ti as usize], &hs);
            if ck::poly::area(&poly) > 1e-14 {
                bytri.entry(ti).or_default().push(poly);
            }
        }
    }
    bytri
        .into_iter()
        .map(|(ti, ps)| {
            let t = scene.model[ti as usize];
            (unit(cross(sub(t[1], t[0]), sub(t[2], t[0]))), ps)
        })
        .collect()
}
fn polyarea(ps: &[(V, Vec<ck::poly::Poly>)], z: f64) -> f64 {
    ps.iter()
        .map(|(normal, polys)| {
            let clipped: Vec<_> = polys
                .iter()
                .map(|p| ck::poly::clip(p, ([0., 0., 1.], z)))
                .filter(|p| p.len() >= 3)
                .collect();
            ck::poly::union_area(&clipped, *normal).unwrap_or(0.)
        })
        .sum()
}
fn main() {
    let args: Vec<_> = std::env::args().collect();
    let input = &args[1];
    let out = Path::new(&args[2]);
    fs::create_dir_all(out).unwrap();
    let bytes = fs::read(input).unwrap();
    let project = cb::parse(&bytes).unwrap();
    let file = ck::chitubox::parse(&bytes).unwrap();
    assert_eq!(project.models.len(), file.models.len());
    for m in project.models.iter().skip(1) {
        assert_eq!(
            m.mesh, project.models[0].mesh,
            "all variants must have identical written model triangles"
        );
    }
    let limits = ck::limits::Limits {
        h1_patch_area_mm2: 0.5,
        h4_lift_mm: 5.,
        h6_gap_mm: 0.3,
        h9_thickness_mm: 0.1,
        region_length_mm: 3.,
    };
    for (mi, model) in project.models.iter().enumerate() {
        eprintln!("model {mi} {}", model.name);
        let Some(supports) = &model.supports else {
            continue;
        };
        let scene = ck::scene::Scene::build(&limits, 8, &file, &bytes, mi).unwrap();
        let shift = -scene.plate_z();
        let (lo, hi) = bounds(&model.mesh);
        let mut normal_areas: BTreeMap<[i64; 3], f64> = BTreeMap::new();
        for t in &model.mesh {
            let t = t.map(|p| p.map(f64::from));
            let cr = cross(sub(t[1], t[0]), sub(t[2], t[0]));
            let lencr = len(cr);
            let normal = unit(cr);
            let key = normal.map(|x| (x * 10000.).round() as i64);
            *normal_areas.entry(key).or_default() += lencr / 2.;
        }
        let mut normal_areas: Vec<_> = normal_areas.into_iter().collect();
        normal_areas.sort_by(|a, b| b.1.total_cmp(&a.1));
        let mut counts = BTreeMap::new();
        let recs: Vec<_> = supports
            .records
            .iter()
            .enumerate()
            .map(|(ri, r)| {
                *counts.entry(r.record_type.0).or_insert(0usize) += 1;
                let (l, h) = bounds(&r.mesh);
                obj(vec![
                    ("record", u(ri)),
                    ("type", u(r.record_type.0 as usize)),
                    ("start", v(r.start.map(f64::from))),
                    ("end", v(r.end.map(f64::from))),
                    ("r1", n(r.r1 as f64)),
                    ("r2", n(r.r2 as f64)),
                    (
                        "attr",
                        u(supports.attrs.get(ri).copied().unwrap_or(0) as usize),
                    ),
                    ("tail", arr((0..6).map(|k| n(r.tail_f32(k) as f64)))),
                    ("mesh_lo", v(l)),
                    ("mesh_hi", v(h)),
                    ("triangles", u(r.mesh.len())),
                    (
                        "raft_vertices",
                        if r.record_type.0 == 6 {
                            arr(r.mesh.iter().flatten().map(|p| v(p.map(f64::from))))
                        } else {
                            J::Null
                        },
                    ),
                    (
                        "invalid",
                        scene.parts[ri].invalid.clone().map_or(J::Null, J::str),
                    ),
                ])
            })
            .collect();
        save(
            out,
            &format!("model-{mi}-records.json"),
            obj(vec![
                ("name", J::str(&model.name)),
                ("translate", v(model.translate.map(f64::from))),
                ("mesh_lo", v(lo)),
                ("mesh_hi", v(hi)),
                (
                    "dominant_normals",
                    arr(normal_areas.iter().take(8).map(|(q, a)| {
                        obj(vec![
                            ("normal", v(q.map(|x| x as f64 / 10000.))),
                            ("area", n(*a)),
                        ])
                    })),
                ),
                (
                    "rest_lo",
                    v(model
                        .mesh
                        .iter()
                        .flatten()
                        .map(|p| orig(p.map(f64::from)))
                        .fold([f64::INFINITY; 3], |a, b| {
                            std::array::from_fn(|k| a[k].min(b[k]))
                        })),
                ),
                (
                    "rest_hi",
                    v(model
                        .mesh
                        .iter()
                        .flatten()
                        .map(|p| orig(p.map(f64::from)))
                        .fold([f64::NEG_INFINITY; 3], |a, b| {
                            std::array::from_fn(|k| a[k].max(b[k]))
                        })),
                ),
                (
                    "presets",
                    arr(supports
                        .presets
                        .iter()
                        .map(|p| obj(vec![("id", u(p.id as usize)), ("name", J::str(&p.name))]))),
                ),
                (
                    "type_counts",
                    arr(counts
                        .iter()
                        .map(|(k, c)| obj(vec![("type", u(*k as usize)), ("count", u(*c))]))),
                ),
                ("records", J::Arr(recs)),
            ]),
        );
        if project.models.len() > 1 && mi == 0 {
            eprintln!("retained plinth instance: measured original single-model file separately");
            continue;
        }
        eprintln!("patches");
        let patches = ck::patch::measure(&scene);
        save(
            out,
            &format!("model-{mi}-hard-limits.json"),
            obj(vec![
                ("H1", outcome(ck::h12::h1(&scene, &patches))),
                ("H2", outcome(ck::h12::h2(&scene, &patches))),
                ("H6", outcome(ck::h6::measure(&scene, &patches))),
                ("H9", outcome(ck::h9::measure(&scene))),
            ]),
        );
        let mut xs = Vec::new();
        let mut ys = Vec::new();
        let mut zs = Vec::new();
        for p in model.mesh.iter().flatten() {
            xs.push(p[0] as f64);
            ys.push(p[1] as f64);
            zs.push(p[2] as f64 + shift);
        }
        let mesh = Mesh::from_soup(&xs, &ys, &zs).unwrap();
        let grid = LayerGrid::covering(&mesh, 0.05).unwrap();
        eprintln!("slice {} layers", grid.layers);
        let Measure::Known(layers) = load::build_model_layers(
            &mesh,
            &grid,
            &StackOptions {
                threads: 8,
                ..Default::default()
            },
        ) else {
            panic!("slicing unverified")
        };
        let mut layout = LayoutView::default();
        let mut contacts = Vec::new();
        let mut used_rods = BTreeSet::new();
        let mut minfirst = Vec::new();
        for (chi, ch) in scene.chains.chains.iter().enumerate() {
            let (Some(ballid), Some(coneid)) = (ch.contact, ch.cone) else {
                continue;
            };
            if scene.parts[ballid].role != Role::TipBall {
                continue;
            };
            let ball = &scene.parts[ballid];
            let cone = &scene.parts[coneid];
            let Geom::Ball { c, r } = ball.geom else {
                continue;
            };
            let inner = polys(&scene, chi, -1.);
            let outer = polys(&scene, chi, 1.);
            let plo = polyarea(&inner, f64::INFINITY);
            let phi = polyarea(&outer, f64::INFINITY);
            let zmin = outer
                .iter()
                .flat_map(|(_, ps)| ps.iter().flatten().map(|p| p[2]))
                .fold(f64::INFINITY, f64::min);
            let zmax = outer
                .iter()
                .flat_map(|(_, ps)| ps.iter().flatten().map(|p| p[2]))
                .fold(f64::NEG_INFINITY, f64::max);
            if !zmin.is_finite() {
                eprintln!("no patch chain {chi}");
                continue;
            }
            let first = ((zmin + shift - grid.origin) / grid.height).floor().max(0.) as u32;
            let last = ((zmax + shift - grid.origin) / grid.height)
                .ceil()
                .max(first as f64) as u32;
            assert!(
                plo >= 0. && phi >= plo && phi - plo < 0.01,
                "patch bracket must resolve within checker tolerance"
            );
            let history: Vec<_> = (first..=last)
                .map(|k| (k, polyarea(&inner, grid.z_top(k) - shift)))
                .collect();
            let ci = layout.contacts.len();
            let neck = ck::h6::neck_area(&scene, ball, cone).unwrap_or(0.);
            let centre = [c[0], c[1], c[2] + shift];
            layout.contacts.push(ContactView {
                centre,
                patch_lower_mm2: plo,
                neck_area_mm2: neck,
                first_layer: first,
                site_class: SiteClass::HiddenUnderside,
            });
            layout.patch_history.push(history.clone());
            minfirst.push(first);
            let sec = |kind: PartKind, pi: usize, area: f64| {
                let p = &scene.parts[pi];
                PartSection {
                    kind,
                    record: Some(pi as u32),
                    area_lower_mm2: area,
                    z_range_mm: [p.bbox.lo[2] + shift, p.bbox.hi[2] + shift],
                }
            };
            let mut parts = vec![
                PartSection {
                    kind: PartKind::Patch,
                    record: Some(ballid as u32),
                    area_lower_mm2: plo,
                    z_range_mm: [zmin + shift, zmax + shift],
                },
                sec(PartKind::Neck, coneid, section(cone)),
            ];
            if let Some(ji) = ch.joint {
                parts.push(sec(PartKind::JointThroat, ji, section(&scene.parts[ji])));
            }
            // Native downward endpoint-connected rod path. Prefer the most downward end if there is a brace branch.
            let mut at = cone.end;
            let mut rods = Vec::new();
            let mut visited = BTreeSet::new();
            loop {
                let mut opts: Vec<_> = scene
                    .parts
                    .iter()
                    .enumerate()
                    .filter_map(|(i, p)| {
                        if p.role != Role::Rod || visited.contains(&i) {
                            return None;
                        }
                        let other = if dist(p.start, at) < 0.001 {
                            p.end
                        } else if dist(p.end, at) < 0.001 {
                            p.start
                        } else {
                            return None;
                        };
                        (other[2] < at[2] - 1e-4).then_some((i, other))
                    })
                    .collect();
                opts.sort_by(|a, b| a.1[2].total_cmp(&b.1[2]));
                let Some((ri, next)) = opts.first().copied() else {
                    break;
                };
                visited.insert(ri);
                used_rods.insert(ri);
                rods.push(ri);
                parts.push(sec(PartKind::Column, ri, section(&scene.parts[ri])));
                at = next;
            }
            layout.chains.push(PartChain {
                contact: ci as u32,
                parts,
            });
            let patch_comps: Vec<_> = patches
                .mesh()
                .comps
                .iter()
                .filter(|p| p.chain == chi)
                .map(|p| {
                    obj(vec![
                        ("lower", n(p.certain_lo)),
                        ("upper", n(p.hi)),
                        ("at", v(p.at)),
                    ])
                })
                .collect();
            let len = dist(cone.start, cone.end);
            let angle = ((cone.start[2] - cone.end[2]).abs() / len)
                .clamp(0., 1.)
                .acos()
                .to_degrees();
            contacts.push(obj(vec![
                ("contact", u(ci)),
                ("chain", u(chi)),
                ("ball_record", u(ballid)),
                ("cone_record", u(coneid)),
                ("centre_local", v(c)),
                ("centre_plate", v(centre)),
                ("rest", v(orig(c))),
                ("ball_diameter", n(2. * r)),
                ("patch_lower", n(plo)),
                ("patch_upper", n(phi)),
                (
                    "patch_equivalent_diameter",
                    n(2. * (plo / std::f64::consts::PI).sqrt()),
                ),
                ("checker_patches", J::Arr(patch_comps)),
                ("neck_area", n(neck)),
                (
                    "neck_diameter_equivalent",
                    n(2. * (neck / std::f64::consts::PI).sqrt()),
                ),
                ("head_length", n(len)),
                ("head_angle_from_vertical_deg", n(angle)),
                ("rod_records", arr(rods.into_iter().map(u))),
                ("first_layer", u(first as usize)),
                (
                    "patch_history",
                    arr(history
                        .iter()
                        .map(|(k, a)| obj(vec![("layer", u(*k as usize)), ("area", n(*a))]))),
                ),
            ]));
        }
        let unused: Vec<_> = scene
            .parts
            .iter()
            .enumerate()
            .filter(|(i, p)| p.role == Role::Rod && !used_rods.contains(i))
            .map(|(i, _)| i)
            .collect();
        // Match production ownership by the low endpoint's trunk XY. Braces add self-peel, not capacity.
        for &pi in &unused {
            let p = &scene.parts[pi];
            let low = if p.start[2] < p.end[2] {
                p.start
            } else {
                p.end
            };
            let owner = layout
                .chains
                .iter()
                .take(layout.contacts.len())
                .enumerate()
                .find_map(|(ci, ch)| {
                    ch.parts
                        .iter()
                        .filter_map(|s| s.record)
                        .any(|ri| {
                            let q = &scene.parts[ri as usize];
                            [q.start, q.end]
                                .iter()
                                .any(|at| (at[0] - low[0]).hypot(at[1] - low[1]) < 0.001)
                        })
                        .then_some(ci)
                });
            if let Some(ci) = owner {
                layout.chains.push(PartChain {
                    contact: ci as u32,
                    parts: vec![PartSection {
                        kind: PartKind::Brace,
                        record: Some(pi as u32),
                        area_lower_mm2: section(p),
                        z_range_mm: [p.bbox.lo[2] + shift, p.bbox.hi[2] + shift],
                    }],
                });
            }
        }
        for ci in 0..layout.contacts.len() {
            let ids: Vec<_> = layout.chains[ci]
                .parts
                .iter()
                .filter_map(|p| p.record)
                .collect();
            let flare = scene.parts.iter().enumerate().find(|(_, p)| {
                p.role == Role::Flare
                    && ids.iter().any(|ri| {
                        let r = &scene.parts[*ri as usize];
                        (dist(r.start, p.start) < 0.001) || (dist(r.end, p.start) < 0.001)
                    })
            });
            if let Some((pi, p)) = flare {
                layout.chains[ci].parts.push(PartSection {
                    kind: PartKind::Flare,
                    record: Some(pi as u32),
                    area_lower_mm2: section(p),
                    z_range_mm: [p.bbox.lo[2] + shift, p.bbox.hi[2] + shift],
                });
            }
        }

        save(
            out,
            &format!("model-{mi}-contacts.json"),
            obj(vec![
                ("grid_origin", n(grid.origin)),
                ("grid_height", n(grid.height)),
                ("shift", n(shift)),
                ("contacts", J::Arr(contacts)),
                ("unassigned_rod_records", arr(unused.into_iter().map(u))),
            ]),
        );
        eprintln!("load {} contacts", layout.contacts.len());
        let detail = load::analyse_full(
            &layers.stack,
            &layout,
            &Extras {
                structure: Some(&layers.structure),
                needs: None,
            },
        );
        let sectionjson = arr(detail.sections.iter().map(|s| {
            obj(vec![
                (
                    "contact",
                    u(layout.chains[s.chain as usize].contact as usize),
                ),
                ("part", u(s.part as usize)),
                ("kind", J::str(format!("{:?}", s.kind))),
                ("record", s.record.map_or(J::Null, |x| u(x as usize))),
                ("area", n(s.area_mm2)),
                (
                    "rigid",
                    arr([n(s.rigid_lever.0), u(s.rigid_lever.1 as usize)]),
                ),
                ("additive", arr([n(s.additive.0), u(s.additive.1 as usize)])),
                (
                    "equal",
                    arr([n(s.equal_share.0), u(s.equal_share.1 as usize)]),
                ),
                (
                    "tributary",
                    arr([n(s.tributary.0), u(s.tributary.1 as usize)]),
                ),
                ("sizing", arr([n(s.sizing.0), u(s.sizing.1 as usize)])),
                ("load", n(s.load_mm2)),
            ])
        }));
        let groups = arr(detail.groups.iter().map(|g| {
            obj(vec![
                ("layer", u(g.layer as usize)),
                ("plate_z_top", n(grid.z_top(g.layer))),
                ("component", u(g.component as usize)),
                ("area", n(g.area_mm2)),
                ("active", arr(g.active.iter().map(|x| u(*x as usize)))),
                ("sum_sections", n(g.section_sum_mm2)),
                ("additive", n(g.x_group)),
                ("centroid", arr(g.centroid_xy.into_iter().map(n))),
                ("assessed", J::Bool(g.equilibrium_assessed)),
                ("outside_hull", n(g.outside_hull_mm)),
                ("force_residual", n(g.force_residual)),
                ("moment_residual", n(g.moment_residual)),
                ("condition", n(g.condition)),
                ("position_sensitivity", n(g.position_sensitivity)),
                (
                    "states",
                    arr(g.section_states.iter().map(|s| {
                        obj(vec![
                            ("contact", u(s.contact as usize)),
                            ("part", u(s.part as usize)),
                            ("patch", n(s.patch_mm2)),
                            ("section", n(s.section_mm2)),
                            ("load", n(s.load_mm2)),
                            ("rigid", n(s.ratios.rigid_lever)),
                            ("additive", n(s.ratios.additive)),
                            ("equal", n(s.ratios.equal_share)),
                            ("tributary", n(s.ratios.tributary)),
                        ])
                    })),
                ),
            ])
        }));
        save(
            out,
            &format!("model-{mi}-load.json"),
            obj(vec![
                ("name", J::str(&model.name)),
                ("sections", sectionjson),
                ("groups", groups),
                (
                    "contact_worst",
                    arr(detail.contacts.iter().map(|c| {
                        obj(vec![
                            ("contact", u(c.contact as usize)),
                            ("ratio", n(c.ratio)),
                            ("layer", u(c.worst_layer as usize)),
                            ("first", u(c.first_layer as usize)),
                            ("layers_over", u(c.layers_over as usize)),
                        ])
                    })),
                ),
                (
                    "warnings",
                    arr(detail.report.warnings.iter().map(|w| {
                        obj(vec![
                            ("kind", J::str(&w.kind)),
                            ("message", J::str(&w.message)),
                            ("layers", arr(w.layers.iter().map(|k| u(*k as usize)))),
                            ("contacts", arr(w.contacts.iter().map(|k| u(*k as usize)))),
                            ("at", w.location_mm.map_or(J::Null, v)),
                        ])
                    })),
                ),
            ]),
        );
        let mut optimistic = layout.clone();
        for (c, h) in optimistic
            .contacts
            .iter()
            .zip(&mut optimistic.patch_history)
        {
            *h = vec![(c.first_layer, c.patch_lower_mm2)];
        }
        let od = load::analyse_full(&layers.stack, &optimistic, &Extras::default());
        save(
            out,
            &format!("model-{mi}-full-patch-sensitivity.json"),
            obj(vec![
                (
                    "note",
                    J::str(
                        "[U] optimistic: whole measured patch available at its first exposure; not physical and not the main result",
                    ),
                ),
                (
                    "contacts",
                    arr(od.contacts.iter().map(|c| {
                        obj(vec![
                            ("contact", u(c.contact as usize)),
                            ("ratio", n(c.ratio)),
                            ("layer", u(c.worst_layer as usize)),
                        ])
                    })),
                ),
                (
                    "maxima",
                    arr([
                        (
                            "rigid",
                            od.sections
                                .iter()
                                .map(|s| s.rigid_lever.0)
                                .fold(0., f64::max),
                        ),
                        (
                            "additive",
                            od.sections.iter().map(|s| s.additive.0).fold(0., f64::max),
                        ),
                        (
                            "equal",
                            od.sections
                                .iter()
                                .map(|s| s.equal_share.0)
                                .fold(0., f64::max),
                        ),
                        (
                            "tributary",
                            od.sections.iter().map(|s| s.tributary.0).fold(0., f64::max),
                        ),
                    ]
                    .into_iter()
                    .map(|(k, x)| obj(vec![("model", J::str(k)), ("ratio", n(x))]))),
                ),
            ]),
        );
        let needs = load::needs::overhang_points(&mesh, 45., 0.25);
        let samples = arr(needs.points.iter().map(|p| {
            let local = [p.xyz[0], p.xyz[1], p.xyz[2] - shift];
            let rest = orig(local);
            let near = layout
                .contacts
                .iter()
                .map(|c| dist(p.xyz, c.centre))
                .fold(f64::INFINITY, f64::min);
            obj(vec![
                ("xyz", v(p.xyz)),
                ("rest", v(rest)),
                ("normal", v(p.normal)),
                ("angle", n(p.angle_deg)),
                ("area", n(p.area_mm2)),
                ("nearest_contact_mm", n(near)),
                ("triangle", u(p.triangle as usize)),
            ])
        }));
        save(out, &format!("model-{mi}-overhang.json"), samples);
        eprintln!("done {mi}");
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn inverse_pose_preserves_distances() {
        let a = [1., 2., 3.];
        let b = [4., -5., 6.];
        assert!((dist(a, b) - dist(orig(a), orig(b))).abs() < 1e-12);
    }
    #[test]
    fn clipped_patch_history_is_monotone() {
        let ps = vec![(
            [0., 0., 1.],
            vec![vec![[0., 0., 0.], [2., 0., 0.], [0., 2., 0.]]],
        )];
        assert_eq!(polyarea(&ps, -0.1), 0.);
        assert!((polyarea(&ps, 0.1) - 2.).abs() < 1e-12);
    }
}
