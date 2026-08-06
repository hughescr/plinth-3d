//! O-10's deliberately narrow bridge for live legacy support/raft parity.
//!
//! This module is not a sizing API for new support algorithms.  It is the one
//! compatibility boundary allowed to use the frozen 0.01 N shaft-drag input
//! and 18 mm brace-free span while the current support/raft path is ported.
//! Both values retain `Placeholder` provenance and are never acceptance or
//! calibration evidence.

use plinth_core::{Finite, NonNegativeFinite, Placeholder, PositiveFinite};
use thiserror::Error;

use crate::{
    forest::{Forest, MemberKey, RootwardEndRadius, TerminalEndRadius},
    member_sizing::{
        diagnostic_deflection_radius_for_geometry, diagnostic_required_radius_for_geometry,
        ActualLateralLoad, DiagnosticMemberSizingPolicy, LeanAngleDeg, MemberSizingError,
        MemberSpan,
    },
};

/// The owner-approved policy which scopes this compatibility exception.
pub const O10_POLICY_ID: &str = "O-10";

/// Frozen, uncalibrated support/raft values permitted by O-10.
///
/// The private field and the sole named constructor intentionally prevent
/// callers from constructing a differently-valued compatibility policy or
/// treating it as a reusable sizing adapter.
#[derive(Clone, Debug, PartialEq)]
pub struct LegacyCompatibility {
    shaft_drag_force_n: Placeholder<NonNegativeFinite>,
    brace_free_span_mm: Placeholder<PositiveFinite>,
}

impl LegacyCompatibility {
    /// Returns the only live current-support/raft compatibility policy.
    #[must_use]
    #[allow(
        dead_code,
        reason = "the future current-support controller is the sole O-10 caller"
    )]
    pub(crate) fn o10_live_current_support_and_raft() -> Self {
        Self {
            shaft_drag_force_n: Placeholder::new(
                NonNegativeFinite::new(0.01).expect("finite O-10 shaft-drag vector"),
                "O-10 LegacyCompatibility only: frozen uncalibrated shaft drag for live current support/raft parity; not Measured, Derived, or AcceptSafe.",
            ),
            brace_free_span_mm: Placeholder::new(
                PositiveFinite::new(18.0).expect("positive O-10 brace-span vector"),
                "O-10 LegacyCompatibility only: frozen uncalibrated brace-free span for live current support/raft parity; not Measured, Derived, or AcceptSafe.",
            ),
        }
    }

    /// Identifies the owner-approved exception without changing provenance.
    #[must_use]
    pub const fn policy_id(&self) -> &'static str {
        O10_POLICY_ID
    }

    /// Single typed authority for the frozen brace cadence used by the live
    /// CCO candidate price.  Crate-private prevents the placeholder from
    /// becoming a downstream configuration or calibration claim.
    #[must_use]
    pub(crate) const fn brace_free_span_mm(&self) -> &Placeholder<PositiveFinite> {
        &self.brace_free_span_mm
    }

    /// Sizes one live tree member and evaluates that member's frozen brace
    /// trigger. Retired members and braces are excluded through `Forest`'s
    /// official live-tree APIs; this never invents or places a brace.
    ///
    /// This is the sole bridge that may turn O-10's diagnostic rootward value
    /// into current production parity geometry.  Its terminal end is returned
    /// exactly from the member, so a one-sided taper cannot be accidentally
    /// replaced by uniform sizing.
    #[allow(
        dead_code,
        reason = "the future current-support controller is the sole O-10 caller"
    )]
    pub(crate) fn size_live_current_support_member(
        &self,
        forest: &Forest,
        key: MemberKey,
    ) -> Result<LiveCurrentSupportRaftSizing, LegacyCompatibilityError> {
        let live_tree_members = forest.live_tree_members();
        if !live_tree_members.contains(&key) {
            return Err(LegacyCompatibilityError::NotLiveTreeMember(key));
        }

        let member = forest
            .member(key)
            .ok_or(LegacyCompatibilityError::MissingLiveMember(key))?;
        let span = MemberSpan::new(member.p0_mm(), member.p1_mm())?;
        let child_rootward_radii = forest
            .children_of(key)
            .into_iter()
            // `children_of` presently removes retired slots, but retain this
            // explicit boundary filter: only live, non-brace children may
            // contribute Murray fan-in to O-10 parity sizing.
            .filter(|child| live_tree_members.contains(child))
            .map(|child| {
                forest
                    .member(child)
                    .map(|member| member.rootward_radius_mm())
                    .ok_or(LegacyCompatibilityError::MissingLiveMember(child))
            })
            .collect::<Result<Vec<_>, _>>()?;
        let required = self.required_radius_for_geometry(
            member.kind(),
            &child_rootward_radii,
            member.carried_mm2(),
            span.length_mm(),
            span.lean_angle_deg(),
        )?;
        let rootward = required.get().max(member.terminal_radius_mm().get().get());

        Ok(LiveCurrentSupportRaftSizing {
            terminal_continuity: member.terminal_radius_mm(),
            rootward_radius: RootwardEndRadius::new(
                PositiveFinite::new(rootward).map_err(|_| MemberSizingError::NonFiniteResult)?,
            ),
            brace_span_exceeded: span.length_mm().get()
                > self.brace_free_span_mm.diagnostic().get(),
        })
    }

    /// Frozen O-10 radius floor for an explicitly supplied free length and
    /// the owning member's lean. This remains crate-private and Placeholder-
    /// tainted by policy; it is the single reuse hook for transient sizing.
    pub(crate) fn required_radius_for_geometry(
        &self,
        kind: &crate::forest::MemberKind,
        child_rootward_radii: &[RootwardEndRadius],
        carried_area_mm2: Finite,
        length_3d_mm: PositiveFinite,
        lean_angle_deg: LeanAngleDeg,
    ) -> Result<PositiveFinite, LegacyCompatibilityError> {
        let policy = DiagnosticMemberSizingPolicy::current()?;
        diagnostic_required_radius_for_geometry(
            kind,
            child_rootward_radii,
            carried_area_mm2,
            length_3d_mm,
            lean_angle_deg,
            &ActualLateralLoad::legacy_compatibility_o10_frozen_shaft_drag(
                &self.shaft_drag_force_n,
            ),
            &policy,
        )
        .map_err(Into::into)
    }

    /// Frozen O-10 deflection component for a transient path length. It calls
    /// the same mechanics leaf as full-span sizing and cannot be constructed
    /// outside this crate.
    #[allow(
        dead_code,
        reason = "the crate-private current-support controller integration follows this parity leaf"
    )]
    pub(crate) fn deflection_radius_for_geometry(
        &self,
        length_3d_mm: PositiveFinite,
        lean_angle_deg: LeanAngleDeg,
    ) -> Result<PositiveFinite, LegacyCompatibilityError> {
        let policy = DiagnosticMemberSizingPolicy::current()?;
        Ok(diagnostic_deflection_radius_for_geometry(
            length_3d_mm,
            lean_angle_deg,
            &ActualLateralLoad::legacy_compatibility_o10_frozen_shaft_drag(
                &self.shaft_drag_force_n,
            ),
            &policy,
        )?
        .diagnostic())
    }
}

/// O-10 output for one current live tree member.  It describes the frozen
/// compatibility result only; it is neither a calibration verdict nor a brace
/// construction request.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LiveCurrentSupportRaftSizing {
    terminal_continuity: TerminalEndRadius,
    rootward_radius: RootwardEndRadius,
    brace_span_exceeded: bool,
}

impl LiveCurrentSupportRaftSizing {
    /// The original terminal end, preserved exactly for one-sided tapering.
    #[must_use]
    pub const fn terminal_continuity(&self) -> TerminalEndRadius {
        self.terminal_continuity
    }

    /// The O-10 bridge's rootward parity radius.
    #[must_use]
    pub const fn rootward_radius(&self) -> RootwardEndRadius {
        self.rootward_radius
    }

    /// Whether the frozen strict `member_span_mm > 18 mm` condition holds.
    #[must_use]
    pub const fn brace_span_exceeded(&self) -> bool {
        self.brace_span_exceeded
    }
}

/// Failure while applying the intentionally scoped O-10 compatibility bridge.
#[derive(Clone, Debug, PartialEq, Error)]
pub enum LegacyCompatibilityError {
    #[error("O-10 compatibility requires a live non-brace tree member: {0:?}")]
    NotLiveTreeMember(MemberKey),
    #[error("forest reported a live tree member that cannot be read: {0:?}")]
    MissingLiveMember(MemberKey),
    #[error(transparent)]
    Sizing(#[from] MemberSizingError),
}

#[cfg(test)]
mod tests {
    use plinth_core::{Finite, PositiveFinite};

    use super::*;
    use crate::forest::{Member, MemberKind, MemberSpec, Owner};

    fn finite(value: f64) -> Finite {
        Finite::new(value).expect("finite vector")
    }

    fn positive(value: f64) -> PositiveFinite {
        PositiveFinite::new(value).expect("positive vector")
    }

    fn member(p1_mm: [f64; 3]) -> Member {
        member_with_terminal(p1_mm, 0.2)
    }

    fn member_with_terminal(p1_mm: [f64; 3], terminal_radius_mm: f64) -> Member {
        Member::new(MemberSpec {
            p0_mm: [finite(0.0); 3],
            p1_mm: p1_mm.map(finite),
            r0_mm: TerminalEndRadius::new(positive(terminal_radius_mm)),
            r1_mm: RootwardEndRadius::new(positive(0.2)),
            kind: MemberKind::Shoot,
            owner: Owner::Generated {
                family: "o10-test".to_owned(),
                ordinal: 0,
            },
            model_contact_end: None,
            carried_mm2: finite(0.0),
        })
    }

    #[test]
    fn pins_o10_identity_placeholder_provenance_and_radius_vector() {
        let mut forest = Forest::new();
        let key = forest.add_member(member([50.0, 0.0, 50.0]));
        let policy = LegacyCompatibility::o10_live_current_support_and_raft();
        let result = policy
            .size_live_current_support_member(&forest, key)
            .expect("live compatibility sizing");

        assert_eq!(policy.policy_id(), O10_POLICY_ID);
        assert_eq!(policy.shaft_drag_force_n.diagnostic().get(), 0.01);
        assert_eq!(policy.brace_free_span_mm.diagnostic().get(), 18.0);
        assert_eq!(result.terminal_continuity().get().get(), 0.2);
        assert!((result.rootward_radius().get().get() - 4.422_534_369_337_705).abs() < 1e-12);
        assert_eq!(
            result.terminal_continuity().get().get().to_bits(),
            0x3fc9_9999_9999_999a
        );
        assert_eq!(
            result.rootward_radius().get().get().to_bits(),
            0x4011_b0ac_d986_f739
        );
    }

    #[test]
    fn full_span_and_explicit_geometry_share_the_exact_required_radius_leaf() {
        let mut forest = Forest::new();
        let key = forest.add_member(member([50.0, 0.0, 50.0]));
        let member = forest.member(key).unwrap();
        let span = MemberSpan::new(member.p0_mm(), member.p1_mm()).unwrap();
        let policy = LegacyCompatibility::o10_live_current_support_and_raft();
        let through_member = policy
            .size_live_current_support_member(&forest, key)
            .unwrap();
        let through_geometry = policy
            .required_radius_for_geometry(
                member.kind(),
                &[],
                member.carried_mm2(),
                span.length_mm(),
                span.lean_angle_deg(),
            )
            .unwrap()
            .get()
            .max(member.terminal_radius_mm().get().get());
        assert_eq!(
            through_member.rootward_radius().get().get().to_bits(),
            through_geometry.to_bits()
        );

        let arbitrary = policy
            .deflection_radius_for_geometry(
                positive(17.25),
                LeanAngleDeg::new(finite(58.0)).unwrap(),
            )
            .unwrap();
        let direct = diagnostic_deflection_radius_for_geometry(
            positive(17.25),
            LeanAngleDeg::new(finite(58.0)).unwrap(),
            &ActualLateralLoad::legacy_compatibility_o10_frozen_shaft_drag(
                &policy.shaft_drag_force_n,
            ),
            &DiagnosticMemberSizingPolicy::current().unwrap(),
        )
        .unwrap()
        .diagnostic();
        assert_eq!(arbitrary.get().to_bits(), direct.get().to_bits());
    }

    #[test]
    fn pins_strict_18_mm_boundary_on_the_subject_member() {
        let policy = LegacyCompatibility::o10_live_current_support_and_raft();
        let mut at_boundary = Forest::new();
        let key = at_boundary.add_member(member([0.0, 0.0, 18.0]));
        assert!(!policy
            .size_live_current_support_member(&at_boundary, key)
            .expect("18 mm live sizing")
            .brace_span_exceeded());

        let mut above_boundary = Forest::new();
        let key =
            above_boundary.add_member(member([0.0, 0.0, f64::from_bits(18.0_f64.to_bits() + 1)]));
        assert!(policy
            .size_live_current_support_member(&above_boundary, key)
            .expect("next f64 subject sizing")
            .brace_span_exceeded());
    }

    #[test]
    fn refuses_a_brace_key() {
        let mut forest = Forest::new();
        let brace = forest.add_brace(member([0.0, 0.0, 20.0]));
        assert_eq!(
            LegacyCompatibility::o10_live_current_support_and_raft()
                .size_live_current_support_member(&forest, brace),
            Err(LegacyCompatibilityError::NotLiveTreeMember(brace))
        );
    }

    #[test]
    fn terminal_continuity_dominates_a_smaller_diagnostic_requirement() {
        let mut forest = Forest::new();
        let key = forest.add_member(member_with_terminal([0.0, 0.0, 10.0], 2.0));
        let result = LegacyCompatibility::o10_live_current_support_and_raft()
            .size_live_current_support_member(&forest, key)
            .expect("live compatibility sizing");

        assert_eq!(
            result.terminal_continuity().get().get().to_bits(),
            2.0_f64.to_bits()
        );
        assert_eq!(
            result.rootward_radius().get().get().to_bits(),
            2.0_f64.to_bits()
        );
    }

    #[test]
    fn refuses_a_retired_subject_and_never_counts_a_retired_child() {
        let mut forest = Forest::new();
        let parent = forest.add_member(member([0.0, 0.0, 10.0]));
        let child = forest.add_member(member([0.0, 0.0, 10.0]));
        forest
            .set_parent(child, Some(parent))
            .expect("live tree link");
        forest.retire(child).expect("retire child");

        let policy = LegacyCompatibility::o10_live_current_support_and_raft();
        assert!(policy
            .size_live_current_support_member(&forest, parent)
            .is_ok());
        assert_eq!(
            policy.size_live_current_support_member(&forest, child),
            Err(LegacyCompatibilityError::NotLiveTreeMember(child))
        );
    }
}
