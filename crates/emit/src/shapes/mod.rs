//! Native ChituBox v1.4.0 support shapes: records and their mesh blobs, built from
//! parameters (design `dev-loop.md` section 10, step 10).
//!
//! A [`Spec`] names one support part by the parameters that decide it; [`build`] returns
//! the 72-byte record fields and the part's mesh. What was reproduced from the owner's
//! native samples, and how closely, is in `docs/format/chitubox-v1.4.0.md` section 19 and
//! is re-measured by `tests/it/native_shapes.rs` on every run.
//!
//! * Record fields (type, start, end, r1, r2, tail words) come out bit for bit. The
//!   parameters are the record's own start and end points, because ChituBox stores the
//!   two poles of a ball, not its centre.
//! * Mesh coordinates are within [`MESH_TOLERANCE_ULPS`] of the native f32 values. The
//!   native vertices come out of a single-precision rotation whose exact order of
//!   operations is not known, so 91.8 percent of coordinates match bit for bit and the
//!   rest differ by at most 1.5 units in the last place.
//! * A shape the native files do not let us reproduce is refused with
//!   [`ShapeError::Unverified`]; it is never guessed. Today that is the hex raft mat.

// The pure f32 mesh kernel lives in `plinth_geom::shapes` (design step11-construction.md
// section 10), so `plinth-support` can build and check the same vertices. This module keeps
// the record building and re-exports the kernel's names, so callers need not change.
pub use plinth_geom::shapes::{
    BRIDGE_TRIANGLES, FRUSTUM_SIDES, FRUSTUM_TRIANGLES, MESH_TOLERANCE_ULPS, PAD_SIDES,
    PAD_TRIANGLES, SPHERE_RINGS, SPHERE_SIDES, SPHERE_TRIANGLES, V3, mesh_error_ulps, ulp,
};

use plinth_chitubox::{RecordType, SupportRecord};
use plinth_geom::shapes::{bridge, frustum, pad, sphere};

/// Type-2 tail word 0 in every native record.
pub const FOOT_CONE_TAIL: f32 = 0.15;
/// Raft part tail word 1 in every native record (meaning not known).
pub const RAFT_TAIL_W1: f32 = 4.0;
/// Raft part tail word 0 for a bridge (a pad has 0.0).
pub const BRIDGE_TAIL_W0: f32 = 1.0;
/// Attr value of every raft record: kind byte 0x1e, preset byte 0.
pub const RAFT_ATTR: u64 = 0x1e00;

/// Why a shape was not built.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ShapeError {
    /// No native sample lets us reproduce this shape, so none is built.
    Unverified {
        /// The shape.
        kind: &'static str,
        /// What is missing.
        reason: &'static str,
    },
    /// The parameters cannot make a solid.
    Degenerate(&'static str),
}

impl std::fmt::Display for ShapeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unverified { kind, reason } => write!(f, "{kind}: unverified ({reason})"),
            Self::Degenerate(why) => write!(f, "degenerate shape: {why}"),
        }
    }
}

impl std::error::Error for ShapeError {}

/// The ball roles (record types 7, 8 and 9).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SphereRole {
    /// Type 7: upper tip ball at the model contact.
    TipBall,
    /// Type 8: lower foot ball of a model-to-model support.
    FootBall,
    /// Type 9: joint ball.
    JointBall,
}

/// The plain frustum roles (record types 2, 3, 4 and 5).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrustumRole {
    /// Type 2: lower foot cone of a model-to-model support; `r1` is the small end.
    FootCone,
    /// Type 3: rod (pillar, trunk, branch, link, leg, post or brace).
    Rod,
    /// Type 4: base flare.
    BaseFlare,
    /// Type 5: base pad of the default mat. The record's `start` is the top, but the mesh
    /// is built from `end` (radius `r1`) to `start` (radius `r2`), so the mesh is wide at
    /// the top (format spec section 10.1). This builder takes the fields as stored.
    BasePad,
}

/// One support part, by the parameters that decide it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Spec {
    /// A ball. `start` and `end` are its two poles; `radius` is the stored radius.
    Sphere {
        /// Which ball.
        role: SphereRole,
        /// Pole the mesh starts at.
        start: V3,
        /// Opposite pole.
        end: V3,
        /// Radius stored in `r1` and `r2`.
        radius: f32,
    },
    /// Type 1: the tip cone, from the tip ball centre to the first joint centre.
    TipCone {
        /// Start (tip ball centre).
        start: V3,
        /// End (joint centre).
        end: V3,
        /// Radius at `start`.
        r1: f32,
        /// Radius at `end`.
        r2: f32,
        /// Tail word 0 (the contact value; native: 0.1, 0.2, 0.3, 0.0404).
        contact: f32,
    },
    /// Types 2 to 5: a frustum with the radii as the record stores them.
    Frustum {
        /// Which frustum.
        role: FrustumRole,
        /// Start point.
        start: V3,
        /// End point.
        end: V3,
        /// Record field `r1`.
        r1: f32,
        /// Record field `r2`.
        r2: f32,
    },
    /// Type 6, pad: a 20-sided prism under one support base.
    RaftPad {
        /// Centre x, y.
        centre: [f32; 2],
        /// Top face z (it is stored only in the mesh).
        z_top: f32,
        /// Thickness (stored in `r2`).
        thickness: f32,
        /// Diameter (stored in `r1`).
        diameter: f32,
    },
    /// Type 6, bridge: a box joining two pad centres.
    RaftBridge {
        /// Centre x, y of the first pad.
        from: [f32; 2],
        /// Centre x, y of the second pad.
        to: [f32; 2],
        /// Top face z (stored only in the mesh).
        z_top: f32,
        /// Thickness (stored in `r2`).
        thickness: f32,
        /// Width (stored in `r1`).
        width: f32,
    },
    /// Type 6, hex mat. Not reproducible from the samples: [`build`] refuses it.
    HexMat,
}

fn check_finite(vals: &[f32], what: &'static str) -> Result<(), ShapeError> {
    if vals.iter().all(|v| v.is_finite()) {
        Ok(())
    } else {
        Err(ShapeError::Degenerate(what))
    }
}

fn axis_ok(start: V3, end: V3) -> Result<(), ShapeError> {
    check_finite(
        &[start[0], start[1], start[2], end[0], end[1], end[2]],
        "non-finite point coordinate",
    )?;
    let d = plinth_geom::shapes::math::sub(end, start);
    if plinth_geom::shapes::math::length(d) > 0.0 {
        Ok(())
    } else {
        Err(ShapeError::Degenerate("start and end coincide"))
    }
}

fn record(
    ty: RecordType,
    (start, end): (V3, V3),
    (r1, r2): (f32, f32),
    tail: [f32; 2],
    mesh: Vec<plinth_chitubox::Triangle>,
) -> SupportRecord {
    let mut words = [0u32; 6];
    words[0] = tail[0].to_bits();
    words[1] = tail[1].to_bits();
    SupportRecord {
        record_type: ty,
        start,
        end,
        r1,
        r2,
        tail: words,
        mesh,
    }
}

/// Build the record fields and mesh of one part, or say why not.
///
/// # Errors
/// [`ShapeError::Unverified`] for the hex mat; [`ShapeError::Degenerate`] for a zero-length
/// axis, a non-positive radius or a non-finite value.
pub fn build(spec: &Spec) -> Result<SupportRecord, ShapeError> {
    match *spec {
        Spec::Sphere {
            role,
            start,
            end,
            radius,
        } => {
            axis_ok(start, end)?;
            check_positive(&[radius])?;
            let ty = match role {
                SphereRole::TipBall => RecordType::TIP_BALL,
                SphereRole::FootBall => RecordType::FOOT_BALL,
                SphereRole::JointBall => RecordType::JOINT_BALL,
            };
            let mesh = sphere(start, end, radius);
            Ok(record(ty, (start, end), (radius, radius), [0.0, 0.0], mesh))
        }
        Spec::TipCone {
            start,
            end,
            r1,
            r2,
            contact,
        } => {
            axis_ok(start, end)?;
            check_positive(&[r1, r2])?;
            check_finite(&[contact], "non-finite contact value")?;
            let mesh = frustum(start, end, r1, r2);
            Ok(record(
                RecordType::TIP_CONE,
                (start, end),
                (r1, r2),
                [contact, 0.0],
                mesh,
            ))
        }
        Spec::Frustum {
            role,
            start,
            end,
            r1,
            r2,
        } => {
            axis_ok(start, end)?;
            check_positive(&[r1, r2])?;
            let (ty, tail, mesh_ends) = match role {
                FrustumRole::FootCone => (RecordType::FOOT_CONE, FOOT_CONE_TAIL, (start, end)),
                FrustumRole::Rod => (RecordType::ROD, 0.0, (start, end)),
                FrustumRole::BaseFlare => (RecordType::BASE_FLARE, 0.0, (start, end)),
                // Native type 5: the record's start is the top, but the mesh is built from
                // the bottom (r1) up to the top (r2).
                FrustumRole::BasePad => (RecordType::BASE_PAD, 0.0, (end, start)),
            };
            let mesh = frustum(mesh_ends.0, mesh_ends.1, r1, r2);
            Ok(record(ty, (start, end), (r1, r2), [tail, 0.0], mesh))
        }
        Spec::RaftPad {
            centre,
            z_top,
            thickness,
            diameter,
        } => {
            check_finite(&[centre[0], centre[1], z_top], "non-finite pad position")?;
            check_positive(&[thickness, diameter])?;
            let mesh = pad(centre, z_top, z_top - thickness, diameter / 2.0);
            let p = [centre[0], centre[1], 0.0];
            Ok(record(
                RecordType::RAFT,
                (p, p),
                (diameter, thickness),
                [0.0, RAFT_TAIL_W1],
                mesh,
            ))
        }
        Spec::RaftBridge {
            from,
            to,
            z_top,
            thickness,
            width,
        } => {
            check_finite(
                &[from[0], from[1], to[0], to[1], z_top],
                "non-finite bridge position",
            )?;
            check_positive(&[thickness, width])?;
            // Numeric length, not bit equality: -0.0 and 0.0 are the same position, and a
            // displacement whose square underflows to 0 would divide by zero below.
            let (dx, dy) = (to[0] - from[0], to[1] - from[1]);
            let length_squared = dx * dx + dy * dy;
            if !(length_squared > 0.0 && length_squared.is_finite()) {
                return Err(ShapeError::Degenerate("bridge ends coincide"));
            }
            let mesh = bridge(from, to, z_top, z_top - thickness, width / 2.0);
            let (a, b) = ([from[0], from[1], 0.0], [to[0], to[1], 0.0]);
            Ok(record(
                RecordType::RAFT,
                (a, b),
                (width, thickness),
                [BRIDGE_TAIL_W0, RAFT_TAIL_W1],
                mesh,
            ))
        }
        Spec::HexMat => Err(ShapeError::Unverified {
            kind: "hex raft mat",
            reason: "one native record (04-round25-supports-raft-hex); the hole layout and the \
                     -1e8/-1e9 start and end values have no known rule",
        }),
    }
}

fn check_positive(vals: &[f32]) -> Result<(), ShapeError> {
    if vals.iter().all(|v| v.is_finite() && *v > 0.0) {
        Ok(())
    } else {
        Err(ShapeError::Degenerate(
            "radius, width or thickness not positive",
        ))
    }
}

/// Build a whole list of parts in one call. The first failure ends the batch.
///
/// # Errors
/// The first [`ShapeError`] [`build`] returns, as `(index, error)`.
pub fn build_all(specs: &[Spec]) -> Result<Vec<SupportRecord>, (usize, ShapeError)> {
    specs
        .iter()
        .enumerate()
        .map(|(i, s)| build(s).map_err(|e| (i, e)))
        .collect()
}

/// A record's attr: `kind << 8 | preset_id << 2` (format spec section 8.3). Both
/// halves of a brace between two kinds are built by the caller with [`attr_pair`].
#[must_use]
pub fn attr(kind: u8, preset_id: u8) -> u64 {
    (u64::from(kind) << 8) | (u64::from(preset_id) << 2)
}

/// The attr of a brace between two supports of different kinds: `high << 32 | low`. Which
/// half belongs to which support is not known (format spec section 15, question 10).
#[must_use]
pub fn attr_pair(low: u64, high: u64) -> u64 {
    (high << 32) | low
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_mat_is_refused_not_guessed() {
        assert!(matches!(
            build(&Spec::HexMat),
            Err(ShapeError::Unverified { .. })
        ));
    }

    #[test]
    fn zero_length_bridges_are_refused_whatever_the_zero_bits() {
        let bridge = |from: [f32; 2], to: [f32; 2]| Spec::RaftBridge {
            from,
            to,
            z_top: -4.55,
            thickness: 0.2,
            width: 2.2,
        };
        // Signed zero, identical points and an underflowing displacement all have length 0.
        for (from, to) in [
            ([0.0, 0.0], [-0.0, 0.0]),
            ([0.0, 0.0], [0.0, -0.0]),
            ([1.5, 2.5], [1.5, 2.5]),
            ([1.0e-30, 0.0], [0.0, 0.0]),
        ] {
            assert!(
                matches!(build(&bridge(from, to)), Err(ShapeError::Degenerate(_))),
                "{from:?} to {to:?}"
            );
        }
        // A real bridge still builds, with finite coordinates.
        let ok = build(&bridge([0.0, 0.0], [3.0, 0.0])).unwrap();
        assert!(ok.mesh.iter().flatten().flatten().all(|c| c.is_finite()));
    }

    #[test]
    fn degenerate_parameters_are_refused() {
        let zero = Spec::Sphere {
            role: SphereRole::JointBall,
            start: [0.0; 3],
            end: [0.0; 3],
            radius: 1.0,
        };
        assert!(matches!(build(&zero), Err(ShapeError::Degenerate(_))));
        let nan = Spec::Frustum {
            role: FrustumRole::Rod,
            start: [0.0; 3],
            end: [0.0, 0.0, f32::NAN],
            r1: 1.0,
            r2: 1.0,
        };
        assert!(build(&nan).is_err());
        let neg = Spec::RaftPad {
            centre: [0.0; 2],
            z_top: 0.0,
            thickness: -0.2,
            diameter: 2.2,
        };
        assert!(build(&neg).is_err());
        let same = Spec::RaftBridge {
            from: [1.0, 1.0],
            to: [1.0, 1.0],
            z_top: 0.0,
            thickness: 0.2,
            width: 2.2,
        };
        assert!(build(&same).is_err());
    }

    #[test]
    fn base_pad_mesh_is_wide_at_the_start_and_keeps_fields() {
        let r = build(&Spec::Frustum {
            role: FrustumRole::BasePad,
            start: [0.0, 0.0, -3.95],
            end: [0.0, 0.0, -4.75],
            r1: 3.64,
            r2: 5.0,
        })
        .unwrap();
        assert_eq!((r.r1, r.r2), (3.64, 5.0));
        // Second vertex of the first triangle: on the ring at `start`, radius r2 = 5.0, on
        // the -x side (the mesh runs upward from `end`, a half turn about y).
        assert!((r.mesh[0][1][0] + 5.0).abs() < 1e-5);
        assert!((r.mesh[0][0][0] + 3.64).abs() < 1e-5);
    }

    #[test]
    fn tails_follow_the_native_rules() {
        let cone = build(&Spec::TipCone {
            start: [0.0; 3],
            end: [1.0, 0.0, -1.0],
            r1: 0.3,
            r2: 0.75,
            contact: 0.3,
        })
        .unwrap();
        assert_eq!(cone.tail_f32(0).to_bits(), 0.3f32.to_bits());
        let foot = build(&Spec::Frustum {
            role: FrustumRole::FootCone,
            start: [0.0; 3],
            end: [0.0, 0.0, 1.0],
            r1: 0.05,
            r2: 0.1,
        })
        .unwrap();
        assert_eq!(foot.tail_f32(0).to_bits(), FOOT_CONE_TAIL.to_bits());
        let bridge = build(&Spec::RaftBridge {
            from: [0.0, 0.0],
            to: [2.0, 0.0],
            z_top: -4.55,
            thickness: 0.2,
            width: 2.2,
        })
        .unwrap();
        assert_eq!(bridge.tail_f32(0).to_bits(), BRIDGE_TAIL_W0.to_bits());
        assert_eq!(bridge.tail_f32(1).to_bits(), RAFT_TAIL_W1.to_bits());
    }

    #[test]
    fn attr_matches_the_native_encoding() {
        assert_eq!(attr(1, 2), 0x0108);
        assert_eq!(attr(3, 1), 0x0304);
        assert_eq!(attr_pair(0x0304, 0x0204), 0x0000_0204_0000_0304);
    }
}
