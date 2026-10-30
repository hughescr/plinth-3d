//! T1: field changes between the sample pairs, and the support facts of the samples
//! (spec sections 6, 7, 10, 11, 12). Ground truth is the spec's measurements of the
//! files ChituBox wrote; nothing here compares against this crate's own output.

use crate::common::{bits, only_model, sample};
use plinth_chitubox::{Project, RecordType, SupportRecord, Supports, Triangle};

fn near(a: f32, b: f32, tol: f32) -> bool {
    (a - b).abs() <= tol
}

fn sup(p: &Project, model: usize) -> &Supports {
    p.models[model].supports.as_ref().expect("supported model")
}

fn types(recs: &[SupportRecord]) -> Vec<u32> {
    recs.iter().map(|r| r.record_type.0).collect()
}

/// Same triangles in the same order, each equal up to a cyclic rotation of its
/// vertices and `tol` per coordinate.
fn meshes_close(a: &[Triangle], b: &[Triangle], tol: f32) -> bool {
    a.len() == b.len()
        && a.iter().zip(b).all(|(ta, tb)| {
            (0..3).any(|rot| {
                (0..3).all(|i| (0..3).all(|c| near(ta[i][c], tb[(i + rot) % 3][c], tol)))
            })
        })
}

fn z_range(mesh: &[Triangle]) -> (f32, f32) {
    let zs = mesh.iter().flatten().map(|v| v[2]);
    (
        zs.clone().fold(f32::INFINITY, f32::min),
        zs.fold(f32::NEG_INFINITY, f32::max),
    )
}

#[test]
fn native_pair_01_to_02_only_translate_z_changes() {
    let (a, b) = (sample("01-round25-plain"), sample("02-round25-lifted"));
    let (ma, mb) = (only_model(&a), only_model(&b));
    assert_eq!(ma.translate[2].to_bits(), 0.0f32.to_bits());
    assert_eq!(mb.translate[2].to_bits(), 5.0f32.to_bits());
    assert_eq!(ma.translate[..2], mb.translate[..2]);
    assert_eq!(ma.name, mb.name);
    // The mesh did not move. ChituBox saves coordinates with up to 4e-6 mm of noise
    // (and may rotate a triangle's first vertex), so compare with a tolerance (spec 5).
    assert!(meshes_close(&ma.mesh, &mb.mesh, 1e-5), "the mesh moved");
    assert!(ma.supports.is_none() && mb.supports.is_none());
    assert_eq!(a.header_f16, b.header_f16);
    // The preview is drawn again on each save: same size, 98 pixels differ (spec 3).
    let (pa, pb) = (a.preview.unwrap(), b.preview.unwrap());
    assert_eq!((pa.width, pa.height), (pb.width, pb.height));
    let changed = pa
        .pixels
        .iter()
        .zip(&pb.pixels)
        .filter(|(x, y)| x != y)
        .count();
    assert_eq!(changed, 98);
}

#[test]
fn native_pair_01_to_06_rotation_is_baked_into_vertices() {
    let (a, b) = (sample("01-round25-plain"), sample("06-round25-flipped"));
    let (ma, mb) = (only_model(&a), only_model(&b));
    // No transform field: the model record holds a name, a translate, and offsets.
    // (Any other nonzero byte in the record is a parse error.) Z stays 0.
    assert_eq!(ma.translate[2].to_bits(), mb.translate[2].to_bits());
    assert!(
        near(mb.translate[1], -0.353_552_8, 1e-6),
        "{:?}",
        mb.translate
    );
    assert_eq!(ma.name, mb.name);
    assert_eq!(ma.mesh.len(), mb.mesh.len());
    // (x, y, z) -> (x, -y, -z) within 5e-6 mm, same triangle and vertex order;
    // X is bit-identical.
    let mut max_err = 0.0f32;
    for (ta, tb) in ma.mesh.iter().zip(&mb.mesh) {
        for (va, vb) in ta.iter().zip(tb) {
            assert_eq!(va[0].to_bits(), vb[0].to_bits());
            max_err = max_err
                .max((va[1] + vb[1]).abs())
                .max((va[2] + vb[2]).abs());
        }
    }
    assert!(max_err < 5e-6, "max error {max_err}");
}

#[test]
fn native_sample_03_support_kinds_and_chains() {
    let p = sample("03-round25-supports");
    let s = sup(&p, 0);
    assert_eq!(s.records.len(), 98);
    assert_eq!(s.attrs.len(), 98);
    // Support groups of the owner's README: preset byte and kind byte per group.
    let groups: [(usize, usize, u64); 13] = [
        (0, 3, 0x0108),   // Heavy adaptive
        (3, 6, 0x010c),   // Middle adaptive
        (6, 9, 0x0110),   // Light adaptive
        (9, 12, 0x0104),  // Ultralight adaptive
        (12, 15, 0x0204), // vertical
        (15, 18, 0x0304), // upright
        (18, 21, 0x0404), // forked branch
        (21, 24, 0x0504), // forked joint
        (24, 27, 0x0604), // forked small
        (27, 36, 0x0804), // forked tree pair
        (66, 72, 0x0104),
        (72, 78, 0x0204),
        (78, 84, 0x0604),
    ];
    for (from, to, attr) in groups {
        assert!(s.attrs[from..to].iter().all(|&a| a == attr), "{from}..{to}");
    }
    assert!(s.attrs[84..90].iter().all(|&a| a == 0x0804));
    assert!(s.attrs[91..97].iter().all(|&a| a == 0x0604));
    // The preset byte is id << 2 and names an entry of this model's own table.
    let name_of = |attr: u64| {
        let id = (attr & 0xff) >> 2;
        s.presets
            .iter()
            .find(|p| p.id == id)
            .map(|p| p.name.as_str())
    };
    assert_eq!(name_of(0x0108), Some("Heavy"));
    assert_eq!(name_of(0x010c), Some("Middle"));
    assert_eq!(name_of(0x0110), Some("Light"));
    assert_eq!(name_of(0x0804), Some("Ultralight"));
    // Part chains: head triples, the tree, ten bases, five model-to-model supports.
    let t = types(&s.records);
    assert!(t[..27].chunks(3).all(|c| c == [7, 1, 9]));
    assert_eq!(t[27..36], [7, 1, 9, 3, 9, 7, 1, 9, 3]);
    assert!(t[36..66].chunks(3).all(|c| c == [5, 4, 3]));
    assert!(t[66..84].chunks(6).all(|c| c == [7, 1, 9, 9, 3, 2]));
    assert_eq!(t[84..91], [7, 1, 9, 9, 3, 2, 8]);
    assert_eq!(t[91..98], [7, 1, 9, 9, 3, 2, 8]);
    // Each base carries its support's attr.
    for k in 0..10 {
        let head = [
            0x108, 0x10c, 0x110, 0x104, 0x204, 0x304, 0x404, 0x504, 0x604, 0x804,
        ][k];
        assert!(s.attrs[36 + 3 * k..39 + 3 * k].iter().all(|&a| a == head));
    }
    // No raft record and no brace in 03; ten "no raft" pads (type 5).
    assert!(!t.contains(&6));
    assert_eq!(t.iter().filter(|&&x| x == 5).count(), 10);
    assert!(s.records.iter().all(|r| !(r.record_type == RecordType::ROD
        && r.r1.to_bits() == 0.15f32.to_bits()
        && r.r2.to_bits() == 0.15f32.to_bits())));
}

#[test]
fn native_sample_03_tail_words_and_type5_radii() {
    let p = sample("03-round25-supports");
    let s = sup(&p, 0);
    for (i, want) in [(1, 0.3f32), (4, 0.2), (7, 0.1), (10, 0.1)] {
        assert!(near(s.records[i].tail_f32(0), want, 1e-6), "record {i}");
    }
    for r in &s.records {
        assert!(r.tail[1..].iter().all(|&w| w == 0));
        if r.record_type == RecordType::FOOT_CONE {
            assert!(near(r.tail_f32(0), 0.15, 1e-6));
        } else if r.record_type != RecordType::TIP_CONE {
            assert_eq!(r.tail[0], 0);
        }
    }
    // Type 5: record says r1 = 3.64 at start (top), r2 = 5.0; the mesh is the other
    // way round: radius 5.0 at the top, 3.64 at the bottom (spec 10.1).
    for r in s
        .records
        .iter()
        .filter(|r| r.record_type == RecordType::BASE_PAD)
    {
        assert!(near(r.r1, 3.64, 1e-5) && near(r.r2, 5.0, 1e-5));
        assert!(near(r.start[2], -3.95, 1e-4) && near(r.end[2], -4.75, 1e-4));
        let (cx, cy) = (r.start[0], r.start[1]);
        let radius = |top: bool| {
            r.mesh
                .iter()
                .flatten()
                .filter(|v| (v[2] > -4.35) == top)
                .map(|v| ((v[0] - cx).powi(2) + (v[1] - cy).powi(2)).sqrt())
                .fold(0.0f32, f32::max)
        };
        assert!(near(radius(true), 5.0, 0.01), "top radius {}", radius(true));
        assert!(
            near(radius(false), 3.64, 0.01),
            "bottom radius {}",
            radius(false)
        );
    }
}

#[test]
fn native_pair_03_to_04_connected_raft_records() {
    let (a, b) = (
        sample("03-round25-supports"),
        sample("04-round25-supports-raft-connected"),
    );
    let (sa, sb) = (sup(&a, 0), sup(&b, 0));
    assert_eq!((sa.records.len(), sb.records.len()), (98, 107));
    assert_eq!(sb.attrs.len(), 107);
    assert_eq!(a.models[0].mesh.len(), b.models[0].mesh.len());
    // Heads (0..36) and model-to-model supports (03: 66..98, 04: 36..68) keep every field.
    let same = |x: &SupportRecord, y: &SupportRecord| {
        x.record_type == y.record_type
            && bits(&x.start) == bits(&y.start)
            && bits(&x.end) == bits(&y.end)
            && x.r1.to_bits() == y.r1.to_bits()
            && x.r2.to_bits() == y.r2.to_bits()
            && x.tail == y.tail
            && x.mesh.len() == y.mesh.len()
    };
    for i in 0..36 {
        assert!(same(&sa.records[i], &sb.records[i]), "head {i}");
        assert_eq!(sa.attrs[i], sb.attrs[i]);
    }
    for i in 0..32 {
        assert!(same(&sa.records[66 + i], &sb.records[36 + i]), "m2m {i}");
    }
    // 19 raft records: 9 bridges then 10 pads; ten type-5 records are gone.
    let t = types(&sb.records);
    assert!(!t.contains(&5));
    assert!(t[68..87].iter().all(|&x| x == 6) && t.iter().filter(|&&x| x == 6).count() == 19);
    let (bridges, pads) = sb.records[68..87].split_at(9);
    for r in bridges.iter().chain(pads) {
        assert!(near(r.r1, 2.2, 1e-6) && near(r.r2, 0.2, 1e-6));
        assert!(r.start[2].to_bits() == 0 && r.end[2].to_bits() == 0);
        assert_eq!(r.tail[1], 4.0f32.to_bits());
        assert!(r.tail[2..].iter().all(|&w| w == 0));
        let (lo, hi) = z_range(&r.mesh);
        assert!(
            near(lo, -4.75, 1e-3) && near(hi, -4.55, 1e-3),
            "z {lo}..{hi}"
        );
    }
    assert!(sb.attrs[68..87].iter().all(|&x| x == 0x1e00));
    assert!(
        bridges
            .iter()
            .all(|r| r.tail[0] == 1.0f32.to_bits() && r.mesh.len() == 12)
    );
    assert!(pads.iter().all(|r| r.tail[0] == 0 && r.mesh.len() == 80));
    // The nine bridges join pads (k, k+1) of the pad list, each pair once, and come in
    // ascending length (the order Kruskal's method accepts them).
    let pad_index = |pt: [f32; 3]| {
        pads.iter()
            .position(|p| near(p.start[0], pt[0], 1e-5) && near(p.start[1], pt[1], 1e-5))
            .expect("bridge end is a pad centre")
    };
    let mut joined: Vec<(usize, usize)> = bridges
        .iter()
        .map(|b| {
            let (u, v) = (pad_index(b.start), pad_index(b.end));
            (u.min(v), u.max(v))
        })
        .collect();
    joined.sort_unstable();
    assert_eq!(joined, (0..9).map(|k| (k, k + 1)).collect::<Vec<_>>());
    let lengths: Vec<f32> = bridges
        .iter()
        .map(|b| (b.end[0] - b.start[0]).hypot(b.end[1] - b.start[1]))
        .collect();
    assert!(lengths.windows(2).all(|w| w[0] <= w[1]), "{lengths:?}");
    // Feet move down 0.6 mm as a whole; flare then pillar per support, at the end.
    for k in 0..10 {
        let (fa, pa) = (&sa.records[37 + 3 * k], &sa.records[38 + 3 * k]);
        let (fb, pb) = (&sb.records[87 + 2 * k], &sb.records[88 + 2 * k]);
        assert_eq!(fb.record_type, RecordType::BASE_FLARE);
        assert_eq!(pb.record_type, RecordType::ROD);
        assert!(near(fb.start[2] - fa.start[2], -0.6, 2e-3), "flare {k}");
        assert!(near(fb.end[2] - fa.end[2], -0.6, 2e-3));
        assert!(near(pb.start[2], pa.start[2], 1e-4));
        assert!(near(pb.end[2] - pa.end[2], -0.6, 2e-3));
        assert_eq!(fb.start[..2], fa.start[..2]);
        assert_eq!(sb.attrs[87 + 2 * k], sa.attrs[37 + 3 * k]);
    }
}

#[test]
fn native_sample_04_hex_raft_is_one_record() {
    let p = sample("04-round25-supports-raft-hex");
    let s = sup(&p, 0);
    assert_eq!(s.records.len(), 89);
    let rafts: Vec<usize> = (0..s.records.len())
        .filter(|&i| s.records[i].record_type == RecordType::RAFT)
        .collect();
    assert_eq!(rafts, [68]);
    let r = &s.records[68];
    assert_eq!(s.attrs[68], 0x1e00);
    assert_eq!(bits(&r.start), bits(&[0.0, 0.0, -1.0e8]));
    assert_eq!(bits(&r.end), bits(&[0.0, 0.0, -1.0e9]));
    assert!(near(r.r1, 1.0, 0.0) && near(r.r2, 1.0, 0.0));
    assert!(r.tail.iter().all(|&w| w == 0));
    assert_eq!(r.mesh.len(), 2772);
    // 04h numbers the presets differently from 03: the attr preset byte follows its own table.
    let id_of = |n: &str| s.presets.iter().find(|p| p.name == n).map(|p| p.id);
    assert_eq!((id_of("Middle"), id_of("Heavy")), (Some(2), Some(3)));
    assert_eq!(s.attrs[0], 0x010c); // the Heavy support (03 wrote 0x0108)
    assert_eq!(s.attrs[3], 0x0108); // the Middle support (03 wrote 0x010c)
}

#[test]
fn native_sample_05_two_plates_are_two_models() {
    let c = sample("04-round25-supports-raft-connected");
    let p = sample("05-two-plates");
    assert_eq!(p.models.len(), 2);
    let (m0, m1) = (&p.models[0], &p.models[1]);
    assert_eq!(m0.name, "Round 25.stl");
    assert_eq!(m1.name, "Round 25.stl #1");
    assert_eq!(bits(&m0.translate), bits(&[0.0, 0.0, 3.0]));
    assert_eq!(
        bits(&m1.translate),
        [0x437e_fffe, 0.0f32.to_bits(), 3.0f32.to_bits()]
    );
    assert!(m0.mesh == m1.mesh);
    // Support data is in the model-local frame: both models carry identical records
    // and attrs, and model 0's equal sample 04-connected's.
    let (s0, s1, sc) = (sup(&p, 0), sup(&p, 1), sup(&c, 0));
    assert!(s0.records == s1.records && s0.attrs == s1.attrs);
    assert!(s0.records == sc.records && s0.attrs == sc.attrs);
    // Same presets and ids; model 1's table is in another order.
    let sorted = |s: &Supports| {
        let mut v: Vec<_> = s.presets.iter().map(|p| (p.id, p.name.clone())).collect();
        v.sort();
        v
    };
    assert_eq!(sorted(s0), sorted(s1));
    assert_ne!(s0.presets, s1.presets);
    assert_eq!(s1.presets[0].name, "Light");
}

#[test]
fn native_sample_07_braces_are_plain_rods() {
    let p = sample("07-round25-on-edge-braced");
    let m = only_model(&p);
    assert!(near(m.translate[0], 0.355_308_06, 1e-7) && near(m.translate[1], 0.355_394_36, 1e-7));
    assert_eq!(m.translate[2].to_bits(), 3.0f32.to_bits());
    let s = sup(&p, 0);
    assert_eq!(s.records.len(), 146);
    assert_eq!(s.attrs.len(), 146);
    let braces: Vec<usize> = (0..146)
        .filter(|&i| {
            let r = &s.records[i];
            r.record_type == RecordType::ROD
                && r.r1.to_bits() == 0.15f32.to_bits()
                && r.r2.to_bits() == 0.15f32.to_bits()
        })
        .collect();
    let want: Vec<usize> = [6..12, 29..33, 38..40, 56..60, 119..121, 134..136]
        .into_iter()
        .flatten()
        .collect();
    assert_eq!(braces, want);
    for &i in &braces {
        let r = &s.records[i];
        assert_eq!(r.mesh.len(), 60);
        assert!(r.tail.iter().all(|&w| w == 0));
        let run = (r.end[0] - r.start[0]).hypot(r.end[1] - r.start[1]);
        assert!(
            near(run, (r.end[2] - r.start[2]).abs(), 1e-4),
            "brace {i} is not 45 degrees"
        );
    }
    // A brace between two different kinds holds both attrs, one in each u32 half.
    assert_eq!(s.attrs[38], 0x0000_0204_0000_0304);
    assert_eq!(s.attrs[39], 0x0000_0204_0000_0304);
    // Every non-raft record is an Ultralight part (preset byte 4).
    for (r, &a) in s.records.iter().zip(&s.attrs) {
        if r.record_type != RecordType::RAFT {
            assert_eq!(a & 0xff, 4, "attr {a:#x}");
        }
    }
    // Rafts: 13 pads and 12 bridges, joined as a tree over the pad centres.
    let rafts: Vec<_> = s
        .records
        .iter()
        .filter(|r| r.record_type == RecordType::RAFT)
        .collect();
    let pads: Vec<_> = rafts.iter().filter(|r| r.tail[0] == 0).collect();
    let bridges: Vec<_> = rafts.iter().filter(|r| r.tail[0] != 0).collect();
    assert_eq!((pads.len(), bridges.len()), (13, 12));
    let index_of = |pt: [f32; 3]| {
        pads.iter()
            .position(|p| near(p.start[0], pt[0], 1e-4) && near(p.start[1], pt[1], 1e-4))
            .expect("bridge end is a pad centre")
    };
    let mut parent: Vec<usize> = (0..13).collect();
    fn root(parent: &mut [usize], mut x: usize) -> usize {
        while parent[x] != x {
            x = parent[x];
        }
        x
    }
    for b in &bridges {
        let (u, v) = (
            root(&mut parent, index_of(b.start)),
            root(&mut parent, index_of(b.end)),
        );
        assert_ne!(u, v, "a bridge closes a cycle");
        parent[u] = v;
    }
    let r0 = root(&mut parent, 0);
    assert!(
        (0..13).all(|i| root(&mut parent, i) == r0),
        "pads are not connected"
    );
}
