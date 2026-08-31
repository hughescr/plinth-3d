//! A minimal `.chitubox` writer, for building planted-defect fixtures.
//!
//! **This is a test instrument, not a second emitter.** It exists so an agent
//! can plant one specific defect in an otherwise valid file and prove that its
//! check fires on it. It writes the container layout the reader of
//! [`crate::reader`] navigates, and nothing else: no settings blob, no
//! placement beyond the three translate floats.
//!
//! It deliberately does **not** validate what you build. Writing a zero-length
//! member, a tip with no taper, a record whose soup disagrees with its fields,
//! or a raft below the plate is the entire point. If the writer refused
//! defects, it could not produce a fixture.
//!
//! ## Layout produced
//!
//! ```text
//! [0, 24)                     header
//! [24, 24 + 680*n)            model table
//! per model:  model mesh blob
//!             support mesh region  (every record's soup, concatenated)
//!             support index (16 B)   <- region ends exactly here
//!             group header (400 B, zero)
//!             records (72 B each)
//! ```

use crate::consts as c;
use crate::reader::RecordKind;

/// One support record to write. Fields are the wire fields, unvalidated.
#[derive(Clone, Debug)]
pub struct SynthRecord {
    pub kind: RecordKind,
    pub start_mm: [f64; 3],
    pub end_mm: [f64; 3],
    /// A RADIUS for types 1/3/4/7/9; the pad DIAMETER or bridge WIDTH for
    /// type 6 (see [`crate::consts::RAFT_FIELDS_ARE_NOT_RADII`]).
    pub r_start: f64,
    /// A RADIUS for types 1/3/4/7/9; the raft HEIGHT for type 6.
    pub r_end: f64,
    pub trailing: [f64; c::S_TRAILING_SLOTS],
    /// The record's own triangle soup, as `f32` exactly as it will be stored.
    pub soup: Vec<[[f32; 3]; 3]>,
}

impl SynthRecord {
    /// A record with an empty soup and zero trailing. Fill what you need.
    #[must_use]
    pub fn new(
        kind: RecordKind,
        start_mm: [f64; 3],
        end_mm: [f64; 3],
        r_start: f64,
        r_end: f64,
    ) -> Self {
        Self {
            kind,
            start_mm,
            end_mm,
            r_start,
            r_end,
            trailing: [0.0; c::S_TRAILING_SLOTS],
            soup: Vec::new(),
        }
    }
    /// Set the declared contact depth in trailing slot 0. Only a TAPER carries
    /// one, and the audit reads it as DECLARED.
    #[must_use]
    pub fn with_declared_depth(mut self, mm: f64) -> Self {
        self.trailing[0] = mm;
        self
    }
    /// Mark a type-6 record as a bridge rather than a pad.
    #[must_use]
    pub fn as_bridge(mut self) -> Self {
        self.trailing[0] = 1.0;
        self.trailing[1] = 4.0;
        self
    }
    /// The `4.0` every measured type-6 record carries in slot 1.
    #[must_use]
    pub fn as_pad(mut self) -> Self {
        self.trailing[0] = 0.0;
        self.trailing[1] = 4.0;
        self
    }
    #[must_use]
    pub fn with_soup(mut self, soup: Vec<[[f32; 3]; 3]>) -> Self {
        self.soup = soup;
        self
    }
    /// Fill the soup with the analytic solid this record's fields describe, so
    /// the mesh-versus-record check agrees unless you meant it not to.
    #[must_use]
    pub fn with_matching_soup(mut self) -> Self {
        self.soup = match self.kind {
            RecordKind::ContactSphere | RecordKind::JointSphere => {
                let cen = [
                    (self.start_mm[0] + self.end_mm[0]) * 0.5,
                    (self.start_mm[1] + self.end_mm[1]) * 0.5,
                    (self.start_mm[2] + self.end_mm[2]) * 0.5,
                ];
                sphere_soup(cen, self.r_start, 8, 16)
            }
            RecordKind::ContactTaper | RecordKind::Member | RecordKind::RootFlare => {
                frustum_soup(self.start_mm, self.r_start, self.end_mm, self.r_end, 16)
            }
            RecordKind::RaftPadOrBridge => Vec::new(),
        };
        self
    }
}

/// One model to write.
#[derive(Clone, Debug, Default)]
pub struct SynthModel {
    pub name: String,
    pub translate_mm: [f64; 3],
    /// The model mesh, bounding-box-centred, exactly as it will be stored.
    pub mesh: Vec<[[f32; 3]; 3]>,
    pub records: Vec<SynthRecord>,
}

/// Build a `.chitubox` byte image.
#[derive(Clone, Debug, Default)]
pub struct SynthFile {
    pub models: Vec<SynthModel>,
    /// Header field 12. `0` or `420`; anything else is a reader finding.
    pub settings_size: u32,
    /// Header field 16. `412`, or something else to plant a finding.
    pub const16: Option<u32>,
}

impl SynthFile {
    #[must_use]
    pub fn new() -> Self {
        Self {
            models: Vec::new(),
            settings_size: 0,
            const16: None,
        }
    }
    #[must_use]
    pub fn with_model(mut self, m: SynthModel) -> Self {
        self.models.push(m);
        self
    }

    /// Serialise. Never validates; a defect you planted stays planted.
    #[must_use]
    pub fn to_bytes(&self) -> Vec<u8> {
        let n = self.models.len();
        let table = c::HEADER_LEN;
        let mut out = vec![0u8; table + n * c::MODEL_RECORD_LEN];
        put_u32(&mut out, c::H_MAGIC, c::FILE_MAGIC);
        put_u32(&mut out, c::H_MODEL_COUNT, n as u32);
        put_u32(&mut out, c::H_OFF8, table as u32);
        put_u32(&mut out, c::H_SETTINGS_SIZE, self.settings_size);
        put_u32(&mut out, c::H_CONST16, self.const16.unwrap_or(c::CONST16));
        put_u32(
            &mut out,
            c::H_OFF20,
            (table + n * c::MODEL_RECORD_LEN) as u32,
        );

        for (mi, m) in self.models.iter().enumerate() {
            let moff = table + mi * c::MODEL_RECORD_LEN;
            let name = m.name.as_bytes();
            let keep = name.len().min(c::MAX_NAME_LEN);
            out[moff..moff + keep].copy_from_slice(&name[..keep]);
            put_f32(&mut out, moff + c::M_TRANSLATE_X, m.translate_mm[0] as f32);
            put_f32(&mut out, moff + c::M_TRANSLATE_Y, m.translate_mm[1] as f32);
            put_f32(&mut out, moff + c::M_TRANSLATE_Z, m.translate_mm[2] as f32);

            let mesh_off = out.len();
            for t in &m.mesh {
                push_tri(&mut out, t);
            }
            put_u32(&mut out, moff + c::M_MESH_OFF, mesh_off as u32);
            put_u32(
                &mut out,
                moff + c::M_MESH_SIZE,
                (m.mesh.len() * c::MESH_BYTES_PER_TRI) as u32,
            );

            if m.records.is_empty() {
                put_u32(&mut out, moff + c::M_SUPPORT_INDEX_PTR, 0);
                continue;
            }

            // The support-mesh region: every record's soup, concatenated, and
            // it must end exactly where the 16-byte index begins.
            let region_off = out.len();
            let mut soup_spans = Vec::with_capacity(m.records.len());
            for r in &m.records {
                let off = out.len();
                for t in &r.soup {
                    push_tri(&mut out, t);
                }
                soup_spans.push((off, r.soup.len() * c::MESH_BYTES_PER_TRI));
            }
            let region_len = out.len() - region_off;
            let index_off = out.len();
            let records_off = index_off + c::SUPPORT_INDEX_LEN + c::GROUP_HEADER_LEN;

            out.extend_from_slice(&[0u8; c::SUPPORT_INDEX_LEN]);
            put_u32(
                &mut out,
                index_off + c::SI_N_RECORDS,
                m.records.len() as u32,
            );
            put_u32(&mut out, index_off + c::SI_RECORDS_OFF, records_off as u32);
            put_u32(&mut out, index_off + c::SI_MESH_OFF, region_off as u32);
            put_u32(&mut out, index_off + c::SI_MESH_SIZE, region_len as u32);
            out.resize(out.len() + c::GROUP_HEADER_LEN, 0);

            for (r, (soff, slen)) in m.records.iter().zip(soup_spans) {
                let ro = out.len();
                out.resize(ro + c::SEG_RECORD_LEN, 0);
                put_u32(&mut out, ro + c::S_MAGIC, c::SEG_MAGIC);
                put_u32(&mut out, ro + c::S_TYPE, r.kind.wire());
                for k in 0..3 {
                    put_f32(&mut out, ro + c::S_START + k * 4, r.start_mm[k] as f32);
                    put_f32(&mut out, ro + c::S_END + k * 4, r.end_mm[k] as f32);
                }
                put_f32(&mut out, ro + c::S_R1, r.r_start as f32);
                put_f32(&mut out, ro + c::S_R2, r.r_end as f32);
                put_u32(
                    &mut out,
                    ro + c::S_MESH_OFF,
                    if slen == 0 { 0 } else { soff as u32 },
                );
                put_u32(&mut out, ro + c::S_MESH_LEN, slen as u32);
                for (s, v) in r.trailing.iter().enumerate() {
                    put_f32(&mut out, ro + c::S_TRAILING + s * 4, *v as f32);
                }
            }
            put_u32(&mut out, moff + c::M_SUPPORT_INDEX_PTR, index_off as u32);
        }
        out
    }

    /// Write the image to `path`.
    pub fn write(&self, path: &std::path::Path) -> std::io::Result<()> {
        std::fs::write(path, self.to_bytes())
    }
}

fn put_u32(b: &mut [u8], off: usize, v: u32) {
    b[off..off + 4].copy_from_slice(&v.to_le_bytes());
}
fn put_f32(b: &mut [u8], off: usize, v: f32) {
    b[off..off + 4].copy_from_slice(&v.to_le_bytes());
}
fn push_tri(out: &mut Vec<u8>, t: &[[f32; 3]; 3]) {
    for v in t {
        for c in v {
            out.extend_from_slice(&c.to_le_bytes());
        }
    }
}

// ---------------------------------------------------------------------------
// Soup generators
// ---------------------------------------------------------------------------

/// A closed, outward-wound axis-aligned box.
#[must_use]
pub fn box_soup(min: [f64; 3], max: [f64; 3]) -> Vec<[[f32; 3]; 3]> {
    let p = |x: f64, y: f64, z: f64| [x as f32, y as f32, z as f32];
    let q = [
        p(min[0], min[1], min[2]),
        p(max[0], min[1], min[2]),
        p(max[0], max[1], min[2]),
        p(min[0], max[1], min[2]),
        p(min[0], min[1], max[2]),
        p(max[0], min[1], max[2]),
        p(max[0], max[1], max[2]),
        p(min[0], max[1], max[2]),
    ];
    const FACES: [[usize; 3]; 12] = [
        [0, 2, 1],
        [0, 3, 2],
        [4, 5, 6],
        [4, 6, 7],
        [0, 1, 5],
        [0, 5, 4],
        [1, 2, 6],
        [1, 6, 5],
        [2, 3, 7],
        [2, 7, 6],
        [3, 0, 4],
        [3, 4, 7],
    ];
    FACES.iter().map(|f| [q[f[0]], q[f[1]], q[f[2]]]).collect()
}

/// A closed, outward-wound UV sphere. `8 x 16` reproduces the emitter's own
/// 224-triangle tessellation, which is also what classifies a file as emitted.
#[must_use]
pub fn sphere_soup(centre: [f64; 3], r: f64, lat: usize, lon: usize) -> Vec<[[f32; 3]; 3]> {
    let pt = |la: usize, lo: usize| -> [f32; 3] {
        let th = std::f64::consts::PI * la as f64 / lat as f64;
        let ph = 2.0 * std::f64::consts::PI * (lo % lon) as f64 / lon as f64;
        [
            (centre[0] + r * th.sin() * ph.cos()) as f32,
            (centre[1] + r * th.sin() * ph.sin()) as f32,
            (centre[2] + r * th.cos()) as f32,
        ]
    };
    let mut tris = Vec::new();
    for la in 0..lat {
        for lo in 0..lon {
            let (a, b, cc, d) = (
                pt(la, lo),
                pt(la + 1, lo),
                pt(la + 1, lo + 1),
                pt(la, lo + 1),
            );
            if la > 0 {
                tris.push([a, b, cc]);
            }
            if la + 1 < lat {
                tris.push([a, cc, d]);
            }
        }
    }
    tris
}

/// A closed, outward-wound frustum with both end caps.
#[must_use]
pub fn frustum_soup(
    a: [f64; 3],
    ra: f64,
    b: [f64; 3],
    rb: f64,
    sides: usize,
) -> Vec<[[f32; 3]; 3]> {
    let axis = crate::vec3::sub(b, a);
    let Some(u) = crate::vec3::normalize(axis) else {
        return Vec::new();
    };
    let (e1, e2, _) = crate::vec3::basis_from_axis(u);
    let ring = |c: [f64; 3], r: f64, i: usize| -> [f32; 3] {
        let t = 2.0 * std::f64::consts::PI * i as f64 / sides as f64;
        let p = crate::vec3::add(
            c,
            crate::vec3::add(
                crate::vec3::mul(e1, r * t.cos()),
                crate::vec3::mul(e2, r * t.sin()),
            ),
        );
        [p[0] as f32, p[1] as f32, p[2] as f32]
    };
    let ca = [a[0] as f32, a[1] as f32, a[2] as f32];
    let cb = [b[0] as f32, b[1] as f32, b[2] as f32];
    let mut tris = Vec::with_capacity(sides * 4);
    for i in 0..sides {
        let j = (i + 1) % sides;
        let (a0, a1) = (ring(a, ra, i), ring(a, ra, j));
        let (b0, b1) = (ring(b, rb, i), ring(b, rb, j));
        tris.push([a0, b0, b1]);
        tris.push([a0, b1, a1]);
        tris.push([ca, a1, a0]);
        tris.push([cb, b0, b1]);
    }
    tris
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::reader::ChituboxFile;
    use std::path::Path;

    #[test]
    fn a_synthesised_file_round_trips_through_the_reader() {
        let f = SynthFile::new().with_model(SynthModel {
            name: "fixture".into(),
            translate_mm: [0.0, 0.0, 5.0],
            mesh: box_soup([-5.0, -5.0, -2.0], [5.0, 5.0, 2.0]),
            records: vec![
                SynthRecord::new(
                    RecordKind::ContactSphere,
                    [0.0, 0.0, -1.9],
                    [0.0, 0.0, -2.5],
                    0.3,
                    0.3,
                )
                .with_matching_soup(),
                SynthRecord::new(
                    RecordKind::ContactTaper,
                    [0.0, 0.0, -2.2],
                    [0.0, 0.0, -4.0],
                    0.2,
                    0.35,
                )
                .with_declared_depth(0.1)
                .with_matching_soup(),
                SynthRecord::new(
                    RecordKind::Member,
                    [0.0, 0.0, -4.0],
                    [0.0, 0.0, -6.8],
                    0.35,
                    0.35,
                )
                .with_matching_soup(),
                SynthRecord::new(
                    RecordKind::RaftPadOrBridge,
                    [0.0, 0.0, -7.0],
                    [0.0, 0.0, -7.0],
                    c::RAFT_PAD_DIAMETER_MM,
                    c::RAFT_PAD_HEIGHT_MM,
                )
                .as_pad(),
            ],
        });
        let bytes = f.to_bytes();
        let parsed = ChituboxFile::parse(Path::new("synth.chitubox"), &bytes).expect("parse");
        assert_eq!(parsed.models.len(), 1);
        let m = &parsed.models[0];
        assert_eq!(m.name, "fixture");
        assert_eq!(m.records.len(), 4);
        assert_eq!(m.model_soup.len(), 12);
        assert_eq!(parsed.findings.total(), 0, "a clean synth has no findings");
        // The tip's declared depth lives on the TAPER, not on the tip.
        assert_eq!(m.records[0].declared_contact_depth_mm(), None);
        assert_eq!(
            m.records[1].declared_contact_depth_mm(),
            Some(f64::from(0.1_f32))
        );
        assert_eq!(m.records[3].is_bridge(), Some(false));
        // The model mesh welds closed, so inside/outside is defined.
        let mesh = crate::mesh::Mesh::weld_exact(&m.model_soup);
        assert!(mesh.closure.is_closed(), "{}", mesh.closure.summary());
    }

    #[test]
    fn the_generated_soups_are_closed_and_match_their_records() {
        let rec = SynthRecord::new(
            RecordKind::Member,
            [0.0, 0.0, 0.0],
            [1.0, 2.0, 3.0],
            0.35,
            0.5,
        )
        .with_matching_soup();
        let f = SynthFile::new().with_model(SynthModel {
            name: "x".into(),
            translate_mm: [0.0; 3],
            mesh: box_soup([-1.0, -1.0, -1.0], [1.0, 1.0, 1.0]),
            records: vec![rec],
        });
        let bytes = f.to_bytes();
        let parsed = ChituboxFile::parse(Path::new("s"), &bytes).expect("parse");
        let r = &parsed.models[0].records[0];
        let p = crate::primitives::Primitive::from_record(r).expect("solid");
        let agree = p.verify_against_soup(r);
        assert!(
            agree.agrees,
            "generated soup must match its record: dev={} tol={}",
            agree.max_surface_deviation_mm, agree.tolerance_mm
        );
    }
}
