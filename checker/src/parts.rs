//! Support records as solids.
//!
//! Each record is decoded twice: the analytic solid its fields name (a ball from the two
//! poles, a cone frustum from the two end points and radii; raft parts have no fields to
//! build from and use their mesh) and the closed mesh the file holds. The mesh must be the
//! canonical mesh for the fields (`canon`): the exact mesh the native shape is defined to have,
//! rebuilt here from the fields alone, to within two units in the last place per coordinate,
//! with the native topology. It must also be a closed, consistently oriented, convex, outward
//! shell. Anything else makes the part `Invalid`, and every geometric rule that needs it is
//! then unverified, never decided on a guess (design `step11-construction.md` section 7.1).
//!
//! The hard-limit verdicts (H1, H2, H3, H6, H9) are measured on the written mesh. The analytic
//! solid is kept for information, and for two derived bounds of the written mesh that follow
//! from its validation: `Part::tol` (the written mesh stands at most that outside the fields'
//! solid: the vertex rounding) and `Part::sag` (the fields' solid stands at most that outside
//! the written mesh: the inscribed polygon, which also gives the certified in-sphere of a
//! written ball).

use crate::bytes::f32_batch;
use crate::canon::{self, Canon};
use crate::chitubox::SupportRecord;
use crate::mesh;
use crate::solid::Solid;
use crate::tri::Tri;
use crate::v3::{Aabb, V, add, dist, dot, f, mul, sub, unit};

/// What a part is, by record type (format reference section 10.1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    TipBall,
    TipCone,
    FootCone,
    Rod,
    Flare,
    BasePad,
    Raft,
    FootBall,
    JointBall,
    Unknown,
}

impl Role {
    #[must_use]
    pub fn of_type(t: u32) -> Self {
        match t {
            7 => Self::TipBall,
            1 => Self::TipCone,
            2 => Self::FootCone,
            3 => Self::Rod,
            4 => Self::Flare,
            5 => Self::BasePad,
            6 => Self::Raft,
            8 => Self::FootBall,
            9 => Self::JointBall,
            _ => Self::Unknown,
        }
    }

    #[must_use]
    pub fn is_ball(self) -> bool {
        matches!(self, Self::TipBall | Self::FootBall | Self::JointBall)
    }

    /// Parts that stand on the build plate: flares, mat pads and raft parts (H4).
    #[must_use]
    pub fn is_plate_part(self) -> bool {
        matches!(self, Self::Flare | Self::BasePad | Self::Raft)
    }

    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::TipBall => "tip ball",
            Self::TipCone => "tip cone",
            Self::FootCone => "foot cone",
            Self::Rod => "rod",
            Self::Flare => "base flare",
            Self::BasePad => "base pad",
            Self::Raft => "raft part",
            Self::FootBall => "foot ball",
            Self::JointBall => "joint ball",
            Self::Unknown => "unknown part",
        }
    }
}

/// The shape a record's fields name.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Geom {
    Ball {
        c: V,
        r: f64,
    },
    /// Radii at `a` and at `b`.
    Frust {
        a: V,
        b: V,
        ra: f64,
        rb: f64,
    },
    /// A raft pad or bridge: described by its mesh only.
    Raft,
}

/// One decoded record.
#[derive(Debug, Clone)]
pub struct Part {
    /// Record index in the file.
    pub rec: usize,
    pub role: Role,
    pub geom: Geom,
    /// Why the mesh is not the native shape for the fields, if it is not.
    pub invalid: Option<String>,
    /// The analytic solid (the mesh polytope for raft parts); `None` when invalid. Information
    /// only: no verdict is read from it.
    pub analytic: Option<Solid>,
    /// The written mesh as a convex polytope; `None` when invalid. The verdict source.
    pub mesh: Option<Solid>,
    /// The mesh triangles as written.
    pub tris: Vec<Tri>,
    /// Covers the written mesh and the analytic solid.
    pub bbox: Aabb,
    /// The record's own start, end and radii (as stored).
    pub start: V,
    pub end: V,
    /// The written mesh lies within this distance of the analytic solid (mm): the vertex
    /// rounding of the canonical mesh (`canon`). Zero for a raft part, which has no analytic
    /// solid but its mesh.
    pub tol: f64,
    /// The analytic solid lies within this distance of the written mesh (mm): the inscribed
    /// polygon of the canonical mesh plus the vertex rounding. At least `tol`. A written ball
    /// contains the sphere of its field radius less this (H6 departure), and the report names
    /// it as the sideways difference of the two solids.
    pub sag: f64,
    /// The written vertices of the two end rings of a frustum, in the order of the geometry's
    /// two ends (ring `j` of one end is joined to ring `j` of the other); empty otherwise.
    pub rings: [Vec<V>; 2],
    /// The largest per-coordinate distance of a written vertex from its canonical position, in
    /// units in the last place of the part's largest coordinate (at most `canon::ULPS`).
    pub canon_ulps: f64,
}

/// What a valid record decodes to.
struct Valid {
    analytic: Solid,
    mesh: Solid,
    canon_ulps: f64,
    tol: f64,
    sag: f64,
    rings: [Vec<V>; 2],
}

fn tris_of(coords: &[f32]) -> Vec<Tri> {
    coords
        .chunks_exact(9)
        .map(|t| {
            [
                f([t[0], t[1], t[2]]),
                f([t[3], t[4], t[5]]),
                f([t[6], t[7], t[8]]),
            ]
        })
        .collect()
}

/// Decode and validate one record whose mesh bytes are `mesh_bytes`.
#[must_use]
pub fn decode(rec: usize, r: &SupportRecord, mesh_bytes: &[u8]) -> Part {
    let role = Role::of_type(r.kind);
    let coords = f32_batch(mesh_bytes);
    let tris = tris_of(&coords);
    let (start, end) = (f(r.start), f(r.end));
    let mut bbox = Aabb::of_points(tris.iter().flatten().copied());
    let geom = match role {
        Role::TipBall | Role::FootBall | Role::JointBall => Geom::Ball {
            c: mul(add(start, end), 0.5),
            r: f64::from(r.r1),
        },
        Role::Raft | Role::Unknown => Geom::Raft,
        _ => {
            // Type 5 stores its radii reversed (format reference section 10.1).
            let (ra, rb) = if role == Role::BasePad {
                (f64::from(r.r2), f64::from(r.r1))
            } else {
                (f64::from(r.r1), f64::from(r.r2))
            };
            Geom::Frust {
                a: start,
                b: end,
                ra,
                rb,
            }
        }
    };
    let checked = validate(role, r, &geom, &coords, &tris);
    let (invalid, analytic, mesh, canon_ulps, tol, sag, rings) = match checked {
        Ok(v) => (
            None,
            Some(v.analytic),
            Some(v.mesh),
            v.canon_ulps,
            v.tol,
            v.sag,
            v.rings,
        ),
        Err(why) => (
            Some(why),
            None,
            None,
            f64::NAN,
            0.0,
            0.0,
            [Vec::new(), Vec::new()],
        ),
    };
    if let Some(a) = &analytic {
        bbox.grow_box(&a.aabb());
    }
    Part {
        rec,
        role,
        geom,
        invalid,
        analytic,
        mesh,
        tris,
        bbox,
        start,
        end,
        tol,
        sag,
        rings,
        canon_ulps,
    }
}

fn validate(
    role: Role,
    r: &SupportRecord,
    geom: &Geom,
    coords: &[f32],
    tris: &[Tri],
) -> Result<Valid, String> {
    if role == Role::Unknown {
        return Err(format!(
            "record type {} is not a known support part",
            r.kind
        ));
    }
    if !r
        .start
        .iter()
        .chain(&r.end)
        .chain([&r.r1, &r.r2])
        .all(|x| x.is_finite())
    {
        return Err("a record field is not finite".to_owned());
    }
    let welded = mesh::weld(coords).ok_or("the record mesh is empty or not finite")?;
    if welded.tris.is_empty() {
        return Err("the record mesh has no triangles".to_owned());
    }
    let edges = mesh::edge_report(&welded);
    if !edges.closed_and_oriented() {
        return Err(format!(
            "the record mesh is not closed and consistently oriented ({} bad edges)",
            edges.bad
        ));
    }
    let (shell_of, n_shells) = mesh::shells(&welded);
    let volume = mesh::shell_volumes(&welded, &shell_of, n_shells);
    if n_shells != 1 || volume[0] <= 0.0 {
        return Err("the record mesh is not one outward-facing shell".to_owned());
    }
    let poly = Solid::polytope(tris.to_vec());
    check_convex(&poly)?;
    let mut first_error: Option<String> = None;
    let mut found: Option<(Canon, Vec<V>)> = None;
    for c in canonical(role, r, geom, &welded)? {
        match check_canonical(&c, geom, &welded) {
            Ok(written) => {
                found = Some((c, written));
                break;
            }
            Err(e) => {
                first_error.get_or_insert(e);
            }
        }
    }
    let Some((canon, written)) = found else {
        return Err(first_error.unwrap_or_else(|| "no canonical mesh".to_owned()));
    };
    let canon_ulps = canon::worst_ulps(&canon, &written);
    // The measured deviation of the written vertices from the canonical mesh rides on the
    // mesh solid, so every bound that reads the mesh can allow for it.
    let poly = poly.with_dev(canon::deviation_mm(&canon, &written));
    match *geom {
        Geom::Ball { c, r: rad } => Ok(Valid {
            analytic: Solid::Ball { c, r: rad },
            mesh: poly,
            canon_ulps,
            tol: canon.round,
            sag: canon.sag + canon.round,
            rings: [Vec::new(), Vec::new()],
        }),
        Geom::Frust { a, b, ra, rb } => {
            let analytic = Solid::frustum(a, b, ra, rb).ok_or("the frustum has zero length")?;
            // Label 0..16 is the start ring of the canonical mesh, 16..32 its end ring. For
            // type 5 the canonical start is the record's end, so the rings swap.
            let ring = |lo: usize| -> Vec<V> { written[lo..lo + canon::RING].to_vec() };
            let rings = if role == Role::BasePad {
                [ring(canon::RING), ring(0)]
            } else {
                [ring(0), ring(canon::RING)]
            };
            Ok(Valid {
                analytic,
                mesh: poly,
                canon_ulps,
                tol: canon.round,
                sag: canon.sag + canon.round,
                rings,
            })
        }
        Geom::Raft => Ok(Valid {
            analytic: poly.clone(),
            canon_ulps,
            mesh: poly,
            tol: 0.0,
            sag: 0.0,
            rings: [Vec::new(), Vec::new()],
        }),
    }
}

fn check_convex(poly: &Solid) -> Result<(), String> {
    let Solid::Poly {
        planes,
        slack,
        verts,
        ..
    } = poly
    else {
        return Ok(());
    };
    for ((n, o), s) in planes.iter().zip(slack) {
        if verts.iter().any(|v| dot(*n, *v) > o + s) {
            return Err("the record mesh is not convex".to_owned());
        }
    }
    Ok(())
}

/// The canonical meshes the record may have (more than one only where the producer's turn rule
/// is ambiguous), or why it has none.
fn canonical(
    role: Role,
    r: &SupportRecord,
    geom: &Geom,
    w: &mesh::Welded,
) -> Result<Vec<Canon>, String> {
    let (start, end) = (f(r.start), f(r.end));
    match *geom {
        Geom::Ball { r: rad, .. } => {
            if (f64::from(r.r1) - f64::from(r.r2)).abs() > 1e-6 {
                return Err("a ball needs equal positive radii r1 = r2".to_owned());
            }
            let out: Vec<Canon> = canon::turns(unit(sub(end, start)))
                .into_iter()
                .filter_map(|t| canon::ball(start, end, rad, t))
                .collect();
            let Some(first) = out.first() else {
                return Err("a ball needs a positive radius and two poles".to_owned());
            };
            let span = dist(start, end);
            if (span - 2.0 * rad).abs() > 2.0 * 3.0f64.sqrt() * first.tol_coord {
                return Err(format!(
                    "the poles are {span} mm apart, not 2r = {}",
                    2.0 * rad
                ));
            }
            Ok(out)
        }
        Geom::Frust { .. } => {
            // Type 5 is built from the record's end (radius r1) to its start (radius r2).
            let (r1, r2) = (f64::from(r.r1), f64::from(r.r2));
            let (from, to) = if role == Role::BasePad {
                (end, start)
            } else {
                (start, end)
            };
            let out: Vec<Canon> = canon::turns(unit(sub(to, from)))
                .into_iter()
                .filter_map(|t| canon::frustum(from, to, r1, r2, t))
                .collect();
            if out.is_empty() {
                return Err("a frustum needs positive radii and a nonzero length".to_owned());
            }
            Ok(out)
        }
        Geom::Raft => {
            let z_top = w
                .verts
                .iter()
                .map(|v| f64::from(v[2]))
                .fold(f64::NEG_INFINITY, f64::max);
            let (diam, thick) = (f64::from(r.r1), f64::from(r.r2));
            let xy = |p: V| [p[0], p[1]];
            let c = match (w.verts.len(), w.tris.len()) {
                (42, 80) => {
                    if dist([start[0], start[1], 0.0], [end[0], end[1], 0.0]) > 1e-6 {
                        return Err(
                            "a raft pad's start and end are not over one another".to_owned()
                        );
                    }
                    canon::pad(xy(start), diam, thick, z_top)
                }
                (8, 12) => canon::bridge(xy(start), xy(end), diam, thick, z_top),
                (v, t) => {
                    return Err(format!(
                        "a raft mesh of {v} vertices and {t} triangles is neither a native pad (42, 80) nor a bridge (8, 12)"
                    ));
                }
            };
            c.map(|c| vec![c])
                .ok_or_else(|| "the raft part's fields do not name a pad or a bridge".to_owned())
        }
    }
}

/// Is the written mesh the canonical one? The written vertex of every canonical label, in the
/// documented order, for the shapes that have one.
fn check_canonical(c: &Canon, geom: &Geom, w: &mesh::Welded) -> Result<Vec<V>, String> {
    match geom {
        Geom::Raft => canon::match_set(c, w),
        _ => canon::match_ordered(c, w),
    }
}

/// True if the part can be used for a geometric rule.
impl Part {
    #[must_use]
    pub fn is_valid(&self) -> bool {
        self.invalid.is_none()
    }

    /// The unit axis of a frustum.
    #[must_use]
    pub fn axis(&self) -> Option<V> {
        match self.geom {
            Geom::Frust { a, b, .. } => Some(unit(sub(b, a))),
            _ => None,
        }
    }
}
