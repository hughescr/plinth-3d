//! PyO3 module `geom_kernel._native` — the Rust ExactSurface hot core.
//!
//! DETERMINISM REVIEW CHECKLIST (spec sec 4.2, R6). Any edit must preserve:
//!  * Parallelism is index-parallel map-collect ONLY (par.rs). No
//!    rayon sum/reduce/fold over floats anywhere.
//!  * No HashMap/HashSet iteration feeds any result (parity grids are CSR;
//!    the pool cache is a Vec).
//!  * No wall-clock reads in the kernel (t_build_ms is wrapper-measured).
//!  * Mutable state is confined: the exterior-mask OnceLock cache and the
//!    result-inert telemetry atomics — nothing else.
//!
//! Violating any of these breaks bit-invariance across thread counts.
//!
//! Ownership (Amendment A16): every input batch is COPIED into Rust-owned
//! buffers while the GIL is held; the GIL is released only for the pure-Rust
//! compute over those buffers; outputs are freshly allocated numpy arrays built
//! after re-acquiring the GIL. No numpy view is ever read outside the GIL.

mod bvh;
mod closest;
mod consts;
mod mesh;
mod moller;
mod par;
mod parity;
mod queries;
mod vec3;

pub mod testkit;

use numpy::ndarray::Array2;
use numpy::{
    IntoPyArray, PyArray1, PyArray2, PyReadonlyArray1, PyReadonlyArray2, PyReadonlyArrayDyn,
};
use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList, PyTuple};

use mesh::Mesh;
use queries::NativeSurface;
use vec3::V3;

pub const KERNEL_API_VERSION: u32 = 1;

fn rows_to_vec(a: &PyReadonlyArray2<f64>) -> PyResult<Vec<V3>> {
    let arr = a.as_array();
    let shape = arr.shape();
    if shape.len() != 2 || shape[1] != 3 {
        return Err(PyValueError::new_err(format!(
            "expected (N,3) array, got shape {:?}",
            shape
        )));
    }
    let mut out = Vec::with_capacity(shape[0]);
    for r in 0..shape[0] {
        out.push([arr[[r, 0]], arr[[r, 1]], arr[[r, 2]]]);
    }
    Ok(out)
}

fn vec_to_pyarray2<'py>(py: Python<'py>, v: &[V3]) -> Bound<'py, PyArray2<f64>> {
    let n = v.len();
    let mut arr = Array2::<f64>::zeros((n, 3));
    for (i, row) in v.iter().enumerate() {
        arr[[i, 0]] = row[0];
        arr[[i, 1]] = row[1];
        arr[[i, 2]] = row[2];
    }
    arr.into_pyarray(py)
}

#[pyclass]
struct NativeSurfacePy {
    inner: NativeSurface,
}

#[pymethods]
impl NativeSurfacePy {
    #[new]
    #[pyo3(signature = (tris, normals, areas, bounds, grid_pitch_mm=2.0))]
    fn new(
        tris: PyReadonlyArrayDyn<f64>,
        normals: PyReadonlyArray2<f64>,
        areas: PyReadonlyArray1<f64>,
        bounds: PyReadonlyArray2<f64>,
        grid_pitch_mm: f64,
    ) -> PyResult<Self> {
        if grid_pitch_mm <= 0.0 {
            return Err(PyValueError::new_err("grid_pitch_mm must be > 0"));
        }
        // Copy inputs into Rust-owned flat buffers (GIL held). Accepts (n,3,3)
        // or (n,9); C-order iteration yields v0x,v0y,v0z,v1x,... per face.
        let tri_arr = tris.as_array();
        let flat_tris: Vec<f64> = tri_arr.iter().copied().collect();
        let nrm = normals.as_array();
        let flat_norm: Vec<f64> = nrm.iter().copied().collect();
        let flat_area: Vec<f64> = areas.as_array().iter().copied().collect();
        let flat_bounds: Vec<f64> = bounds.as_array().iter().copied().collect();

        let mesh = Mesh::new(&flat_tris, &flat_norm, &flat_area, &flat_bounds)
            .map_err(PyValueError::new_err)?;
        // Configured parity-grid memory budget (spec sec 3.5 / Amendment A17):
        // GEOM_KERNEL_MAX_GRID_BYTES caps each per-axis CSR grid so a
        // pathological mesh fails construction cleanly instead of thrashing.
        // Unset => usize::MAX (no cap; production treant-scale grids are tens
        // of MB, telemetered in build_log).
        let max_grid_bytes = std::env::var("GEOM_KERNEL_MAX_GRID_BYTES")
            .ok()
            .and_then(|s| s.parse::<usize>().ok())
            .unwrap_or(usize::MAX);
        let inner = NativeSurface::new(mesh, grid_pitch_mm, max_grid_bytes)
            .map_err(PyValueError::new_err)?;
        Ok(NativeSurfacePy { inner })
    }

    #[getter]
    fn nfaces(&self) -> usize {
        self.inner.mesh.nfaces
    }

    fn current_threads(&self) -> usize {
        par::current_threads()
    }

    #[pyo3(signature = (pts, distance_upper_bound=f64::INFINITY))]
    fn query<'py>(
        &self,
        py: Python<'py>,
        pts: PyReadonlyArray2<f64>,
        distance_upper_bound: f64,
    ) -> PyResult<Bound<'py, PyTuple>> {
        let p = rows_to_vec(&pts)?;
        let (dist, idx) = py.allow_threads(|| self.inner.query(&p, distance_upper_bound));
        let d = PyArray1::from_vec(py, dist);
        let i = PyArray1::from_vec(py, idx);
        PyTuple::new(py, [d.into_any(), i.into_any()])
    }

    fn project<'py>(
        &self,
        py: Python<'py>,
        pts: PyReadonlyArray2<f64>,
    ) -> PyResult<Bound<'py, PyTuple>> {
        let p = rows_to_vec(&pts)?;
        let (dist, cp, idx, nrm) = py.allow_threads(|| self.inner.project(&p));
        let d = PyArray1::from_vec(py, dist).into_any();
        let c = vec_to_pyarray2(py, &cp).into_any();
        let i = PyArray1::from_vec(py, idx).into_any();
        let n = vec_to_pyarray2(py, &nrm).into_any();
        PyTuple::new(py, [d, c, i, n])
    }

    fn inside<'py>(
        &self,
        py: Python<'py>,
        pts: PyReadonlyArray2<f64>,
    ) -> PyResult<Bound<'py, PyArray1<bool>>> {
        let p = rows_to_vec(&pts)?;
        let out = py.allow_threads(|| self.inner.inside(&p));
        Ok(PyArray1::from_vec(py, out))
    }

    #[pyo3(signature = (origins, dirs, t_max=1.0))]
    fn cast_rays<'py>(
        &self,
        py: Python<'py>,
        origins: PyReadonlyArray2<f64>,
        dirs: PyReadonlyArray2<f64>,
        t_max: f64,
    ) -> PyResult<Bound<'py, PyTuple>> {
        let o = rows_to_vec(&origins)?;
        let d = rows_to_vec(&dirs)?;
        if o.len() != d.len() {
            return Err(PyValueError::new_err("origins and dirs length mismatch"));
        }
        let (offsets, t_all, f_all) = py.allow_threads(|| self.inner.cast_rays(&o, &d, t_max));
        let off = PyArray1::from_vec(py, offsets).into_any();
        let ta = PyArray1::from_vec(py, t_all).into_any();
        let fa = PyArray1::from_vec(py, f_all).into_any();
        PyTuple::new(py, [off, ta, fa])
    }

    #[pyo3(signature = (p0s, p1s, r0s, r1s, standoff, inset0s, inset1s))]
    #[allow(clippy::too_many_arguments)]
    fn segment_clearance<'py>(
        &self,
        py: Python<'py>,
        p0s: PyReadonlyArray2<f64>,
        p1s: PyReadonlyArray2<f64>,
        r0s: PyReadonlyArray1<f64>,
        r1s: PyReadonlyArray1<f64>,
        standoff: f64,
        inset0s: PyReadonlyArray1<f64>,
        inset1s: PyReadonlyArray1<f64>,
    ) -> PyResult<Bound<'py, PyTuple>> {
        let p0 = rows_to_vec(&p0s)?;
        let p1 = rows_to_vec(&p1s)?;
        let r0: Vec<f64> = r0s.as_array().iter().copied().collect();
        let r1: Vec<f64> = r1s.as_array().iter().copied().collect();
        let i0: Vec<f64> = inset0s.as_array().iter().copied().collect();
        let i1: Vec<f64> = inset1s.as_array().iter().copied().collect();
        let n = p0.len();
        if [p1.len(), r0.len(), r1.len(), i0.len(), i1.len()]
            .iter()
            .any(|&x| x != n)
        {
            return Err(PyValueError::new_err(
                "segment_clearance input length mismatch",
            ));
        }
        let (clr, pt) = py.allow_threads(|| {
            self.inner
                .segment_clearance(&p0, &p1, &r0, &r1, standoff, &i0, &i1)
        });
        let c = PyArray1::from_vec(py, clr).into_any();
        let p = vec_to_pyarray2(py, &pt).into_any();
        PyTuple::new(py, [c, p])
    }

    #[pyo3(signature = (p0s, p1s, r0s, r1s, n_offset_rays=8, majority_step_mm=0.12, cap=80, min_illegal_len_mm=0.05, rays_all=false))]
    #[allow(clippy::too_many_arguments)]
    fn penetration_intervals_batch<'py>(
        &self,
        py: Python<'py>,
        p0s: PyReadonlyArray2<f64>,
        p1s: PyReadonlyArray2<f64>,
        r0s: PyReadonlyArray1<f64>,
        r1s: PyReadonlyArray1<f64>,
        n_offset_rays: usize,
        majority_step_mm: f64,
        cap: usize,
        min_illegal_len_mm: f64,
        rays_all: bool,
    ) -> PyResult<Bound<'py, PyDict>> {
        let p0 = rows_to_vec(&p0s)?;
        let p1 = rows_to_vec(&p1s)?;
        let r0: Vec<f64> = r0s.as_array().iter().copied().collect();
        let r1: Vec<f64> = r1s.as_array().iter().copied().collect();
        let n = p0.len();
        if [p1.len(), r0.len(), r1.len()].iter().any(|&x| x != n) {
            return Err(PyValueError::new_err(
                "penetration_intervals_batch input length mismatch",
            ));
        }
        let per = py.allow_threads(|| {
            self.inner.penetration_intervals_batch(
                &p0,
                &p1,
                &r0,
                &r1,
                n_offset_rays,
                majority_step_mm,
                cap,
                min_illegal_len_mm,
                rays_all,
            )
        });
        // ragged SoA: member offsets + flat interval fields
        let mut offsets: Vec<i64> = Vec::with_capacity(n + 1);
        offsets.push(0);
        let mut s_a = Vec::new();
        let mut s_b = Vec::new();
        let mut entered: Vec<bool> = Vec::new();
        let mut exited: Vec<bool> = Vec::new();
        let mut depth = Vec::new();
        let mut ray: Vec<i64> = Vec::new();
        for member in &per {
            for iv in member {
                s_a.push(iv.s_a);
                s_b.push(iv.s_b);
                entered.push(iv.entered);
                exited.push(iv.exited);
                depth.push(iv.max_depth);
                ray.push(iv.ray as i64);
            }
            offsets.push(s_a.len() as i64);
        }
        let dict = PyDict::new(py);
        dict.set_item("offsets", PyArray1::from_vec(py, offsets))?;
        dict.set_item("s_a", PyArray1::from_vec(py, s_a))?;
        dict.set_item("s_b", PyArray1::from_vec(py, s_b))?;
        dict.set_item("entered", PyArray1::from_vec(py, entered))?;
        dict.set_item("exited", PyArray1::from_vec(py, exited))?;
        dict.set_item("max_depth_mm", PyArray1::from_vec(py, depth))?;
        dict.set_item("ray", PyArray1::from_vec(py, ray))?;
        Ok(dict)
    }

    #[pyo3(signature = (centers, radii, eps_mm=0.05, half_deg=25.0, n_cone=8))]
    fn sphere_breach<'py>(
        &self,
        py: Python<'py>,
        centers: PyReadonlyArray2<f64>,
        radii: PyReadonlyArray1<f64>,
        eps_mm: f64,
        half_deg: f64,
        n_cone: usize,
    ) -> PyResult<Bound<'py, PyTuple>> {
        let c = rows_to_vec(&centers)?;
        let r: Vec<f64> = radii.as_array().iter().copied().collect();
        if r.len() != c.len() {
            return Err(PyValueError::new_err("centers/radii length mismatch"));
        }
        let (br, th, po) =
            py.allow_threads(|| self.inner.sphere_breach(&c, &r, eps_mm, half_deg, n_cone));
        let b = PyArray1::from_vec(py, br).into_any();
        let t = PyArray1::from_vec(py, th).into_any();
        let p = PyArray1::from_vec(py, po).into_any();
        PyTuple::new(py, [b, t, p])
    }

    fn exterior_face_mask<'py>(&self, py: Python<'py>) -> Bound<'py, PyArray1<bool>> {
        let mask: Vec<bool> = py.allow_threads(|| self.inner.exterior_face_mask().to_vec());
        PyArray1::from_vec(py, mask)
    }

    /// build_log telemetry (excludes any timing — wrapper measures t_build_ms).
    fn build_log<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyDict>> {
        let d = PyDict::new(py);
        d.set_item("nfaces", self.inner.mesh.nfaces)?;
        d.set_item("grid_pitch_mm", self.inner.grid_pitch_mm)?;
        d.set_item("bvh_depth", self.inner.bvh.depth)?;
        d.set_item("bvh_nodes", self.inner.bvh.nodes.len())?;
        d.set_item("n_degenerate_tris", self.inner.mesh.n_degenerate)?;
        // per-axis parity grid telemetry (Amendment A17)
        let axes = PyList::empty(py);
        for g in &self.inner.pv.grids {
            let gd = PyDict::new(py);
            gd.set_item("axis", g.ax)?;
            gd.set_item("cells", g.n_cells)?;
            gd.set_item("refs", g.n_refs)?;
            gd.set_item("max_occ", g.max_occ)?;
            gd.set_item("bytes", g.bytes())?;
            axes.append(gd)?;
        }
        d.set_item("parity_axes", axes)?;
        Ok(d)
    }

    /// result-inert telemetry counters (diagnostics only).
    fn telemetry<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyDict>> {
        use std::sync::atomic::Ordering;
        let d = PyDict::new(py);
        d.set_item(
            "threshold_band",
            self.inner.band_counter.load(Ordering::Relaxed),
        )?;
        d.set_item(
            "measured_tie",
            self.inner.tie_counter.load(Ordering::Relaxed),
        )?;
        Ok(d)
    }
}

#[pymodule]
fn _native(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<NativeSurfacePy>()?;
    m.add("KERNEL_API_VERSION", KERNEL_API_VERSION)?;
    m.add("SOURCE_DIGEST", env!("GEOM_KERNEL_SOURCE_DIGEST"))?;
    m.add_function(wrap_pyfunction!(current_threads, m)?)?;
    Ok(())
}

#[pyfunction]
fn current_threads() -> usize {
    par::current_threads()
}
