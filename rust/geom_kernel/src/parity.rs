//! Axis-aligned ray-parity inside/outside, ported VERBATIM from
//! geometry_exact.py VerticalParity/ParityVote (lines 171-284). Per axis a CSR
//! grid replaces the oracle's dict — keyed by the IDENTICAL flat cell id
//! `ix*NY + iy`, so candidate sets match the oracle exactly (including its
//! benign out-of-range aliasing; an unknown key => empty, spec sec 3.4). The
//! parity dedup is the consecutive-difference CLUSTER COUNT `1 + Σ(diff >
//! zdedup)`, NOT dedup_loc keep-first (Amendment A2).

use crate::consts::{PARITY_DEN_EPS, PARITY_SLACK, ZDEDUP};
use crate::mesh::Mesh;

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum ParityBuildError {
    GridBudget {
        axis: usize,
        projected_bytes: usize,
        limit_bytes: usize,
    },
    Other(String),
}

impl std::fmt::Display for ParityBuildError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::GridBudget { axis, projected_bytes, limit_bytes } => write!(formatter, "parity grid axis {axis} exceeds memory budget: {projected_bytes} bytes projected > {limit_bytes} budget (spec sec 3.5 / Amendment A17)"),
            Self::Other(message) => formatter.write_str(message),
        }
    }
}

#[derive(Clone, Copy)]
struct FaceRef {
    face_id: u32,
    slot: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ParityMemoryEstimate {
    pub retained_bytes: usize,
    pub peak_bytes: usize,
    pub max_axis_build_bytes: usize,
    pub axis_references: [usize; 3],
}

fn projection_axes(axis: usize) -> (usize, usize) {
    match axis {
        0 => (1, 2),
        1 => (0, 2),
        2 => (0, 1),
        _ => unreachable!("parity has exactly three axes"),
    }
}

fn projected_cell_bounds(
    a: [f64; 3],
    b: [f64; 3],
    c: [f64; 3],
    axes: [usize; 2],
    origin: [f64; 2],
    h: f64,
) -> (i64, i64, i64, i64) {
    let [p0, p1] = axes;
    let [origin0, origin1] = origin;
    let xmin = a[p0].min(b[p0]).min(c[p0]);
    let xmax = a[p0].max(b[p0]).max(c[p0]);
    let ymin = a[p1].min(b[p1]).min(c[p1]);
    let ymax = a[p1].max(b[p1]).max(c[p1]);
    (
        ((xmin - origin0) / h).floor() as i64,
        ((xmax - origin0) / h).floor() as i64,
        ((ymin - origin1) / h).floor() as i64,
        ((ymax - origin1) / h).floor() as i64,
    )
}

/// Allocation-free upper bound for all three parity indexes. The retained
/// estimate assumes every triangle reference has a distinct CSR key; the peak
/// additionally includes the transient `(cell, triangle)` vector while each
/// axis is built. Every add/multiply is checked and returns a named error.
pub(crate) fn estimate_indexed_memory(
    vertices: &[[f64; 3]],
    faces: &[[u32; 3]],
    bounds: [[f64; 3]; 2],
    h: f64,
) -> Result<ParityMemoryEstimate, String> {
    let per_axis_triangle_bytes = faces
        .len()
        .checked_mul(11 * std::mem::size_of::<f64>() + std::mem::size_of::<bool>())
        .ok_or_else(|| "parity triangle memory estimate overflow".to_string())?;
    let mut axis_references = [0_usize; 3];
    let mut retained_axes = [0_usize; 3];
    let mut build_axes = [0_usize; 3];
    for ax in 0..3 {
        let (p0, p1) = projection_axes(ax);
        let origin0 = bounds[0][p0] - 1.0;
        let origin1 = bounds[0][p1] - 1.0;
        let mut references = 0_usize;
        for (face, indices) in faces.iter().enumerate() {
            let a = vertices
                .get(indices[0] as usize)
                .copied()
                .ok_or_else(|| format!("indexed face {face} is out of bounds"))?;
            let source_b = vertices
                .get(indices[1] as usize)
                .copied()
                .ok_or_else(|| format!("indexed face {face} is out of bounds"))?;
            let source_c = vertices
                .get(indices[2] as usize)
                .copied()
                .ok_or_else(|| format!("indexed face {face} is out of bounds"))?;
            // `Mesh::from_indexed` stores A plus edge vectors; AxisGrid later
            // reconstructs B/C from those arrays. Mirror that exact floating-
            // point path here so a cell-boundary rounding bit cannot make the
            // supposedly conservative preflight undercount references.
            let b = [
                a[0] + (source_b[0] - a[0]),
                a[1] + (source_b[1] - a[1]),
                a[2] + (source_b[2] - a[2]),
            ];
            let c = [
                a[0] + (source_c[0] - a[0]),
                a[1] + (source_c[1] - a[1]),
                a[2] + (source_c[2] - a[2]),
            ];
            let (ix0, ix1, iy0, iy1) =
                projected_cell_bounds(a, b, c, [p0, p1], [origin0, origin1], h);
            let nx = i128::from(ix1) - i128::from(ix0) + 1;
            let ny = i128::from(iy1) - i128::from(iy0) + 1;
            if nx <= 0 || ny <= 0 {
                return Err(format!("parity projected range is invalid at face {face}"));
            }
            let refs = usize::try_from(
                nx.checked_mul(ny)
                    .ok_or_else(|| format!("parity reference estimate overflow at face {face}"))?,
            )
            .map_err(|_| format!("parity reference estimate exceeds usize at face {face}"))?;
            references = references
                .checked_add(refs)
                .ok_or_else(|| format!("parity reference total overflows usize at face {face}"))?;
        }
        if references > u32::MAX as usize {
            return Err("parity grid exceeds 2^32 triangle references".into());
        }
        axis_references[ax] = references;
        let transient = references
            .checked_mul(std::mem::size_of::<(i64, u32)>())
            .and_then(|bytes| bytes.checked_add(per_axis_triangle_bytes))
            .ok_or_else(|| "parity transient memory estimate overflow".to_string())?;
        let retained = references
            .checked_mul(
                std::mem::size_of::<i64>()
                    + std::mem::size_of::<u32>()
                    + std::mem::size_of::<u32>(),
            )
            .and_then(|bytes| bytes.checked_add(std::mem::size_of::<u32>()))
            .and_then(|bytes| bytes.checked_add(per_axis_triangle_bytes))
            .ok_or_else(|| "parity retained memory estimate overflow".to_string())?;
        build_axes[ax] = transient;
        retained_axes[ax] = retained;
    }
    let mut retained_before = 0_usize;
    let mut peak_bytes = 0_usize;
    for axis in 0..3 {
        peak_bytes = peak_bytes.max(
            retained_before
                .checked_add(build_axes[axis])
                .ok_or_else(|| "parity peak memory estimate overflow".to_string())?,
        );
        retained_before = retained_before
            .checked_add(retained_axes[axis])
            .ok_or_else(|| "parity retained memory estimate overflow".to_string())?;
        peak_bytes = peak_bytes.max(retained_before);
    }
    Ok(ParityMemoryEstimate {
        retained_bytes: retained_before,
        peak_bytes,
        max_axis_build_bytes: *build_axes.iter().max().unwrap_or(&0),
        axis_references,
    })
}

/// One axis' projected triangle data + CSR grid.
pub struct AxisGrid {
    pub ax: usize,
    pub p0: usize,
    pub p1: usize,
    pub origin0: f64,
    pub origin1: f64,
    pub h: f64,
    pub ny: i64,
    // Per-triangle projected quantities are compact and indexed by a selected
    // local slot retained alongside the global ID in each CSR reference.
    a2x: Vec<f64>,
    a2y: Vec<f64>,
    v0x: Vec<f64>,
    v0y: Vec<f64>,
    v1x: Vec<f64>,
    v1y: Vec<f64>,
    den: Vec<f64>,
    good: Vec<bool>,
    az: Vec<f64>,
    bz: Vec<f64>,
    cz: Vec<f64>,
    // CSR: sorted unique flat cell ids -> contiguous selected-face references.
    // Every reference retains its global ID; the slot keeps the hot query path
    // O(1) without allocating global-face-count-sized projected arrays.
    keys: Vec<i64>,
    offs: Vec<u32>,
    tri_refs: Vec<FaceRef>,
    // telemetry
    pub n_cells: usize,
    pub n_refs: usize,
    pub max_occ: usize,
}

impl AxisGrid {
    pub(crate) fn build(
        mesh: &Mesh,
        ax: usize,
        h: f64,
        max_grid_bytes: usize,
    ) -> Result<AxisGrid, ParityBuildError> {
        let n = mesh.nfaces();
        // Peak-memory accounting (Amendment A17 / spec sec 3.5): a large
        // projected triangle bins into every overlapped cell, so the CSR ref
        // count — not nfaces — dominates. Fail CLEANLY past the configured
        // budget instead of thrashing toward OOM. per-tri projected arrays are
        // fixed (11 f64 + 1 bool); the transient (flat_id, tri) pair vector is
        // the growing term we bound as it accumulates.
        let per_tri_bytes = n.saturating_mul(11 * 8 + 1);
        let pair_bytes = std::mem::size_of::<(i64, FaceRef)>();
        let (p0, p1) = projection_axes(ax);

        // origin = bounds[0, p] - 1.0 ; NY = floor((bounds[1,p1]+1 - origin1)/h)+3
        let bounds = mesh.bounds();
        let origin0 = bounds[0][p0] - 1.0;
        let origin1 = bounds[0][p1] - 1.0;
        let ny = ((bounds[1][p1] + 1.0 - origin1) / h).floor() as i64 + 3;

        let mut a2x = vec![0.0; n];
        let mut a2y = vec![0.0; n];
        let mut v0x = vec![0.0; n];
        let mut v0y = vec![0.0; n];
        let mut v1x = vec![0.0; n];
        let mut v1y = vec![0.0; n];
        let mut den = vec![0.0; n];
        let mut good = vec![false; n];
        let mut az = vec![0.0; n];
        let mut bz = vec![0.0; n];
        let mut cz = vec![0.0; n];

        // (flat_id, tri) pairs for the CSR
        let mut pairs: Vec<(i64, FaceRef)> = Vec::with_capacity(n * 2);

        for (slot, &face_id) in mesh.face_ids().iter().enumerate() {
            let face = face_id as usize;
            let a = mesh.v0(face);
            let b = mesh.vertex_b(face);
            let cc = mesh.vertex_c(face);
            // 2D projected verts
            let ax0 = a[p0];
            let ay0 = a[p1];
            let bx = b[p0];
            let by = b[p1];
            let cx = cc[p0];
            let cy = cc[p1];
            a2x[slot] = ax0;
            a2y[slot] = ay0;
            // v0 = C2 - A2 ; v1 = B2 - A2 (geometry_exact.py:232)
            let v0xf = cx - ax0;
            let v0yf = cy - ay0;
            let v1xf = bx - ax0;
            let v1yf = by - ay0;
            v0x[slot] = v0xf;
            v0y[slot] = v0yf;
            v1x[slot] = v1xf;
            v1y[slot] = v1yf;
            // den = v0x*v1y - v1x*v0y (line 233)
            let d = v0xf * v1yf - v1xf * v0yf;
            den[slot] = d;
            good[slot] = d.abs() > PARITY_DEN_EPS;
            az[slot] = a[ax];
            bz[slot] = b[ax];
            cz[slot] = cc[ax];

            // cell range of the 2D AABB; shared with the allocation-free
            // estimator so the preflight and builder cannot drift.
            let (ix0, ix1, iy0, iy1) =
                projected_cell_bounds(a, b, cc, [p0, p1], [origin0, origin1], h);
            for ix in ix0..=ix1 {
                for iy in iy0..=iy1 {
                    let flat = ix
                        .checked_mul(ny)
                        .and_then(|v| v.checked_add(iy))
                        .ok_or_else(|| {
                            ParityBuildError::Other("parity flat-id overflow".to_string())
                        })?;
                    pairs.push((
                        flat,
                        FaceRef {
                            face_id,
                            slot: slot as u32,
                        },
                    ));
                }
            }
            // Budget check per triangle: bounds overshoot to one triangle's
            // cell fan-out, so we fail before the pair vector grows unbounded.
            let projected = per_tri_bytes.saturating_add(pairs.len().saturating_mul(pair_bytes));
            if projected > max_grid_bytes {
                return Err(ParityBuildError::GridBudget {
                    axis: ax,
                    projected_bytes: projected,
                    limit_bytes: max_grid_bytes,
                });
            }
        }

        if pairs.len() > u32::MAX as usize {
            return Err(ParityBuildError::Other(
                "parity grid exceeds 2^32 triangle references".into(),
            ));
        }

        // sort by flat id (stable-not-required; intra-cell order is irrelevant
        // to parity because crossings are re-sorted per query)
        pairs.sort_unstable_by_key(|&(key, face)| (key, face.face_id));

        let mut keys: Vec<i64> = Vec::new();
        let mut offs: Vec<u32> = vec![0];
        let mut tri_refs: Vec<FaceRef> = Vec::with_capacity(pairs.len());
        let mut max_occ = 0usize;
        let mut i = 0usize;
        while i < pairs.len() {
            let k = pairs[i].0;
            let mut j = i;
            while j < pairs.len() && pairs[j].0 == k {
                tri_refs.push(pairs[j].1);
                j += 1;
            }
            keys.push(k);
            offs.push(tri_refs.len() as u32);
            let occ = j - i;
            if occ > max_occ {
                max_occ = occ;
            }
            i = j;
        }

        Ok(AxisGrid {
            ax,
            p0,
            p1,
            origin0,
            origin1,
            h,
            ny,
            a2x,
            a2y,
            v0x,
            v0y,
            v1x,
            v1y,
            den,
            good,
            az,
            bz,
            cz,
            n_cells: keys.len(),
            n_refs: tri_refs.len(),
            max_occ,
            keys,
            offs,
            tri_refs,
        })
    }

    /// Approx bytes for telemetry.
    pub fn bytes(&self) -> usize {
        let per_tri = 11 * 8 + 1; // 11 f64 + 1 bool per face
        per_tri * self.a2x.len()
            + self.keys.len() * 8
            + self.offs.len() * 4
            + self.tri_refs.len() * std::mem::size_of::<FaceRef>()
    }

    pub(crate) fn referenced_face_ids(&self) -> impl Iterator<Item = u32> + '_ {
        self.tri_refs.iter().map(|face| face.face_id)
    }

    #[inline]
    fn cell_lookup(&self, flat: i64) -> Option<(usize, usize)> {
        match self.keys.binary_search(&flat) {
            Ok(pos) => {
                let s = self.offs[pos] as usize;
                let e = self.offs[pos + 1] as usize;
                Some((s, e))
            }
            Err(_) => None,
        }
    }

    /// Parity for one point along this axis: true iff an odd number of
    /// deduped crossings lie strictly above along +axis.
    #[inline]
    pub fn inside_point(&self, pt: [f64; 3], scratch: &mut Vec<f64>) -> bool {
        let c0 = pt[self.p0];
        let c1 = pt[self.p1];
        let pch = pt[self.ax];
        let ix = ((c0 - self.origin0) / self.h).floor() as i64;
        let iy = ((c1 - self.origin1) / self.h).floor() as i64;
        let flat = match ix.checked_mul(self.ny).and_then(|v| v.checked_add(iy)) {
            Some(v) => v,
            None => return false, // wildly out of range => empty
        };
        let (s, e) = match self.cell_lookup(flat) {
            Some(r) => r,
            None => return false,
        };

        scratch.clear();
        for face in &self.tri_refs[s..e] {
            let t = face.slot as usize;
            if !self.good[t] {
                continue;
            }
            let den = self.den[t];
            let d0 = c0 - self.a2x[t];
            let d1 = c1 - self.a2y[t];
            // a = (d0*v1y - v1x*d1)/den ; b = (v0x*d1 - d0*v0y)/den (lines 245-246)
            let a = (d0 * self.v1y[t] - self.v1x[t] * d1) / den;
            let b = (self.v0x[t] * d1 - d0 * self.v0y[t]) / den;
            let insxy =
                (a >= -PARITY_SLACK) && (b >= -PARITY_SLACK) && (a + b <= 1.0 + PARITY_SLACK);
            if !insxy {
                continue;
            }
            // z = (1-a-b)*Az + a*Cz + b*Bz  (line 252, assoc ((.)+a*Cz)+b*Bz)
            let z = (1.0 - a - b) * self.az[t] + a * self.cz[t] + b * self.bz[t];
            if z > pch + PARITY_SLACK {
                scratch.push(z);
            }
        }
        if scratch.is_empty() {
            return false;
        }
        // cluster count = 1 + Σ(diff > zdedup) over ascending z (Amendment A2)
        scratch.sort_unstable_by(|x, y| x.partial_cmp(y).unwrap());
        let mut uniqn = 1usize;
        for w in scratch.windows(2) {
            if w[1] - w[0] > ZDEDUP {
                uniqn += 1;
            }
        }
        uniqn % 2 == 1
    }
}

/// 3-axis majority vote (ParityVote): inside iff >= 2 of 3 axes report inside.
pub struct ParityVote {
    pub grids: [AxisGrid; 3],
}

impl ParityVote {
    pub(crate) fn build(
        mesh: &Mesh,
        h: f64,
        max_grid_bytes: usize,
    ) -> Result<ParityVote, ParityBuildError> {
        let g0 = AxisGrid::build(mesh, 0, h, max_grid_bytes)?;
        let g1 = AxisGrid::build(mesh, 1, h, max_grid_bytes)?;
        let g2 = AxisGrid::build(mesh, 2, h, max_grid_bytes)?;
        Ok(ParityVote {
            grids: [g0, g1, g2],
        })
    }

    #[inline]
    pub fn inside(&self, pt: [f64; 3], scratch: &mut Vec<f64>) -> bool {
        let mut s = 0u8;
        for g in &self.grids {
            if g.inside_point(pt, scratch) {
                s += 1;
            }
        }
        s >= 2
    }
}
