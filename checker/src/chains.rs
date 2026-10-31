//! Contact chains, found from record types and end-point relations only. The file has no
//! parent id (format reference section 9.3) and the generator's own bookkeeping is not trusted.
//!
//! * A **tip chain** is a tip ball (7), the tip cone (1) that starts at the ball's centre and
//!   the joint ball (9) at the cone's far end.
//! * A **foot chain** is a foot cone (2) and the foot ball (8) at one of its ends.
//! * Every other part is a chain of its own: rods, flares, pads and raft parts, and any ball
//!   or cone that has no partner. A shared trunk below several heads therefore never joins
//!   their patches.
//!
//! A part with two possible partners is an ambiguous ownership: the chain is marked, and the
//! rules that depend on ownership (H1, H2) are unverified for it, never decided on the
//! favourable grouping.

use crate::parts::{Part, Role};
use crate::v3::dist;

/// End points that coincide this closely (mm) are the same point. Native examples agree to
/// 2.4e-7 (format reference section 10.2).
pub const JOIN_MM: f64 = 1e-3;

/// One contact chain.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Chain {
    /// Indices into the part list.
    pub parts: Vec<usize>,
    /// The part that makes the contact patch: the tip ball or the foot ball.
    pub contact: Option<usize>,
    /// The cone of the head, when there is one.
    pub cone: Option<usize>,
    /// The joint ball at the cone's far end, when there is one.
    pub joint: Option<usize>,
    /// Why ownership is unclear, if it is.
    pub ambiguous: Option<String>,
}

/// The chains of a part list.
#[derive(Debug, Clone, Default)]
pub struct Chains {
    pub chains: Vec<Chain>,
    /// Chain index of every part.
    pub of_part: Vec<usize>,
}

fn near(a: [f64; 3], b: [f64; 3]) -> bool {
    dist(a, b) <= JOIN_MM
}

/// Find the chains of `parts` (all of them valid or not: an invalid part is still owned).
#[must_use]
pub fn find(parts: &[Part]) -> Chains {
    let n = parts.len();
    let mut owner: Vec<Option<usize>> = vec![None; n];
    let mut chains: Vec<Chain> = Vec::new();
    let centre = |p: &Part| crate::v3::mul(crate::v3::add(p.start, p.end), 0.5);
    for (i, cone) in parts.iter().enumerate() {
        match cone.role {
            Role::TipCone => {
                let balls: Vec<usize> = (0..n)
                    .filter(|&j| {
                        parts[j].role == Role::TipBall && near(centre(&parts[j]), cone.start)
                    })
                    .collect();
                let joints: Vec<usize> = (0..n)
                    .filter(|&j| {
                        parts[j].role == Role::JointBall && near(centre(&parts[j]), cone.end)
                    })
                    .collect();
                let mut c = Chain {
                    cone: Some(i),
                    ..Chain::default()
                };
                c.parts.push(i);
                note_partner(&mut c, &balls, "tip ball", true);
                note_partner(&mut c, &joints, "joint ball", false);
                chains.push(c);
            }
            Role::FootCone => {
                let balls: Vec<usize> = (0..n)
                    .filter(|&j| {
                        parts[j].role == Role::FootBall
                            && (near(centre(&parts[j]), cone.start)
                                || near(centre(&parts[j]), cone.end))
                    })
                    .collect();
                let mut c = Chain {
                    cone: Some(i),
                    ..Chain::default()
                };
                c.parts.push(i);
                note_partner(&mut c, &balls, "foot ball", true);
                chains.push(c);
            }
            _ => {}
        }
    }
    // A ball claimed by two chains, or already owned, makes both ambiguous.
    for ci in 0..chains.len() {
        for slot in 0..chains[ci].parts.len() {
            let p = chains[ci].parts[slot];
            match owner[p] {
                None => owner[p] = Some(ci),
                Some(other) if other != ci => {
                    let msg = format!(
                        "record {} is claimed by two chains (records {} and {})",
                        parts[p].rec,
                        parts[chains[other].parts[0]].rec,
                        parts[chains[ci].parts[0]].rec
                    );
                    chains[ci].ambiguous.get_or_insert_with(|| msg.clone());
                    chains[other].ambiguous.get_or_insert(msg);
                }
                Some(_) => {}
            }
        }
    }
    let mut of_part = vec![usize::MAX; n];
    for (ci, c) in chains.iter().enumerate() {
        for &p in &c.parts {
            if of_part[p] == usize::MAX {
                of_part[p] = ci;
            }
        }
    }
    for (p, slot) in of_part.iter_mut().enumerate() {
        if *slot == usize::MAX {
            let contact = matches!(parts[p].role, Role::TipBall | Role::FootBall).then_some(p);
            chains.push(Chain {
                parts: vec![p],
                contact,
                ..Chain::default()
            });
            *slot = chains.len() - 1;
        }
    }
    Chains { chains, of_part }
}

/// Record the partner of a cone: none, one (added to the chain), or several (ambiguous).
fn note_partner(c: &mut Chain, found: &[usize], what: &str, is_contact: bool) {
    match found {
        [] => {}
        [one] => {
            c.parts.push(*one);
            if is_contact {
                c.contact = Some(*one);
            } else {
                c.joint = Some(*one);
            }
        }
        many => {
            c.ambiguous = Some(format!(
                "a cone could belong to any of {} {what}s",
                many.len()
            ));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chitubox::SupportRecord;
    use crate::parts::decode;

    fn rec(kind: u32, s: [f32; 3], e: [f32; 3], r1: f32, r2: f32) -> SupportRecord {
        SupportRecord {
            magic: crate::chitubox::RECORD_MAGIC,
            kind,
            start: s,
            end: e,
            r1,
            r2,
            mesh_off: 0,
            mesh_len: 0,
            tail: [0; 6],
        }
    }

    fn part(i: usize, kind: u32, s: [f32; 3], e: [f32; 3]) -> Part {
        decode(i, &rec(kind, s, e, 0.1, 0.1), &[])
    }

    #[test]
    fn tip_chains_group_ball_cone_joint_and_leave_the_rest_alone() {
        let parts = vec![
            part(0, 7, [0.0, 0.0, -0.1], [0.0, 0.0, 0.1]),
            part(1, 1, [0.0, 0.0, 0.0], [0.0, 0.0, -2.0]),
            part(2, 9, [0.0, 0.0, -2.2], [0.0, 0.0, -1.8]),
            part(3, 3, [0.0, 0.0, -2.0], [0.0, 0.0, -9.0]),
        ];
        let c = find(&parts);
        assert_eq!(c.chains.len(), 2);
        assert_eq!(c.of_part[0], c.of_part[1]);
        assert_eq!(c.of_part[1], c.of_part[2]);
        assert_ne!(c.of_part[2], c.of_part[3]);
        let tip = &c.chains[c.of_part[0]];
        assert_eq!(
            (tip.contact, tip.cone, tip.joint),
            (Some(0), Some(1), Some(2))
        );
        assert!(tip.ambiguous.is_none());
    }

    #[test]
    fn two_balls_at_one_cone_start_are_an_ambiguous_ownership() {
        let parts = vec![
            part(0, 7, [0.0, 0.0, -0.1], [0.0, 0.0, 0.1]),
            part(1, 7, [0.0, 0.0, -0.1], [0.0, 0.0, 0.1]),
            part(2, 1, [0.0, 0.0, 0.0], [0.0, 0.0, -2.0]),
        ];
        let c = find(&parts);
        assert!(c.chains[c.of_part[2]].ambiguous.is_some());
    }
}
