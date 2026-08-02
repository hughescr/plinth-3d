//! Current-Python-parity lowering from the final live forest to semantic emit primitives.
//!
//! This is deliberately a pure, serial lowering pass: it performs no geometry queries and
//! preserves the frozen member/ball/raft record order. Mesh construction and ChituBox record
//! identities remain owned by `plinth-emit`.

use std::mem::size_of;

use plinth_core::{m0_constants, ContentDigest, Finite, LayerPlan, PositiveFinite};
use thiserror::Error;

use crate::{
    forest::{Forest, Member, MemberKey, MemberKind},
    joints::{JointConfig, JointError, JointPlan},
    raft::RaftPlan,
    BuildFrame, FrustumRingNormal, SphereGeometry, SupportPrimitive, SupportSegmentKind,
};

const ZERO_LENGTH_MM: f64 = 1e-9;

/// Bounds and frozen geometry policy for one lowering pass.
///
/// Fields are private so callers cannot substitute emitter geometry constants. The only
/// constructor reads the registered current-parity values and rejects variable-height plans,
/// whose root-boss behavior the Python oracle never defined.
#[derive(Clone, Copy, Debug)]
pub(crate) struct LoweringConfig {
    frame: BuildFrame,
    layer_height_mm: PositiveFinite,
    contact_depth_mm: PositiveFinite,
    tip_ball_factor: PositiveFinite,
    root_layers: u64,
    root_no_bend_mm: PositiveFinite,
    raft_height_mm: PositiveFinite,
    flare_length_mm: PositiveFinite,
    flare_factor: PositiveFinite,
    plate_band_mm: PositiveFinite,
    joint: JointConfig,
    max_primitives: usize,
    max_primitive_bytes: usize,
}

impl LoweringConfig {
    pub(crate) fn current(
        frame: BuildFrame,
        layers: &LayerPlan,
        max_primitives: usize,
        max_primitive_bytes: usize,
    ) -> Result<Self, LoweringError> {
        if max_primitives == 0 || max_primitive_bytes == 0 {
            return Err(LoweringError::InvalidLimits);
        }
        let Some((&first, remaining)) = layers.heights_mm().split_first() else {
            return Err(LoweringError::MissingLayerHeight);
        };
        if remaining
            .iter()
            .any(|height| height.to_bits() != first.to_bits())
        {
            return Err(LoweringError::NonConstantLayerPlan);
        }
        let layer_height_mm =
            PositiveFinite::new(first).map_err(|_| LoweringError::MissingLayerHeight)?;
        let constants = m0_constants()
            .map_err(|error| LoweringError::RegisteredConstants(error.to_string()))?;
        let policy = constants.placeholders();
        let joint_factor = *policy.joint_ball_factor().diagnostic();
        Ok(Self {
            frame,
            layer_height_mm,
            contact_depth_mm: *policy.default_contact_depth_mm().diagnostic(),
            tip_ball_factor: *policy.ball_factor_stub().diagnostic(),
            root_layers: policy.chitubox_root_layers().diagnostic().get(),
            root_no_bend_mm: *policy.chitubox_root_height_mm().diagnostic(),
            raft_height_mm: *policy.chitubox_raft_height_mm().diagnostic(),
            flare_length_mm: *policy.flare_len_mm().diagnostic(),
            flare_factor: *policy.flare_r_factor().diagnostic(),
            plate_band_mm: *policy.plate_band_mm().diagnostic(),
            joint: JointConfig::current(frame.plate_z_mm(), joint_factor),
            max_primitives,
            max_primitive_bytes,
        })
    }
}

/// Opaque, authority-bound lowering result. Callers can inspect but cannot forge its primitive
/// stream or counters.
#[derive(Clone, Debug)]
pub(crate) struct LoweredSupportPlan {
    forest_digest: u64,
    frame_bits: [u64; 2],
    primitives: Vec<SupportPrimitive>,
    support_primitive_count: usize,
    raft_primitive_count: usize,
    digest: ContentDigest,
}

impl LoweredSupportPlan {
    pub(crate) fn validate<'a>(
        &'a self,
        forest: &Forest,
        frame: BuildFrame,
    ) -> Result<&'a [SupportPrimitive], LoweringError> {
        if self.forest_digest != forest.digest()
            || self.frame_bits
                != [
                    frame.plate_z_mm().get().to_bits(),
                    frame.lift_mm().get().to_bits(),
                ]
        {
            return Err(LoweringError::StaleAuthority);
        }
        Ok(&self.primitives)
    }

    pub(crate) const fn support_primitive_count(&self) -> usize {
        self.support_primitive_count
    }

    pub(crate) const fn raft_primitive_count(&self) -> usize {
        self.raft_primitive_count
    }

    pub(crate) const fn digest(&self) -> ContentDigest {
        self.digest
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Error)]
pub(crate) enum LoweringError {
    #[error("current-parity support lowering requires a non-empty layer plan")]
    MissingLayerHeight,
    #[error("current-parity support lowering does not define variable layer heights")]
    NonConstantLayerPlan,
    #[error("support lowering requires nonzero work and memory limits")]
    InvalidLimits,
    #[error("support lowering would exceed its primitive-count limit")]
    PrimitiveLimit,
    #[error("support lowering would exceed its primitive-memory limit")]
    PrimitiveBytesLimit,
    #[error("support lowering authority is stale or belongs to another forest/frame")]
    StaleAuthority,
    #[error("support member {member:?} has zero length")]
    ZeroLengthMember { member: MemberKey },
    #[error("plate root {member:?} has insufficient no-bend height")]
    RootRunTooShort { member: MemberKey },
    #[error("support lowering produced invalid finite geometry")]
    InvalidGeometry,
    #[error("registered current-parity constants are unavailable: {0}")]
    RegisteredConstants(String),
    #[error(transparent)]
    Joint(#[from] JointError),
}

/// Lower the exact final live view in frozen Python record order.
///
/// The view is gathered once in forest creation order. Retired and quarantined slots are absent
/// by construction; a live stub is retained only when it is still the registered route for a
/// terminal contact. This prevents an abandoned/unrouted stub from leaking into physical output.
pub(crate) fn lower_current(
    forest: &Forest,
    joints: &JointPlan,
    raft: &RaftPlan,
    config: LoweringConfig,
) -> Result<LoweredSupportPlan, LoweringError> {
    let balls = joints.validate(forest, config.joint)?;
    raft.validate(forest, config.frame.plate_z_mm())
        .map_err(|_| LoweringError::StaleAuthority)?;

    let live = forest
        .live_members_in_creation_order()
        .into_iter()
        .filter_map(|key| forest.member(key).map(|member| (key, member)))
        .filter(|(key, member)| {
            member.kind() != &MemberKind::Stub || is_routed_stub(forest, *key, member)
        })
        .collect::<Vec<_>>();
    let live_keys = live
        .iter()
        .map(|(key, _)| *key)
        .collect::<std::collections::BTreeSet<_>>();
    let authorized_ball_count = balls
        .iter()
        .filter(|ball| live_keys.contains(&ball.owner_key()))
        .count();

    let support_count = checked_support_count(&live, authorized_ball_count, config)?;
    let raft_count = raft.primitives().len();
    let total = support_count
        .checked_add(raft_count)
        .ok_or(LoweringError::PrimitiveLimit)?;
    if total > config.max_primitives {
        return Err(LoweringError::PrimitiveLimit);
    }
    let bytes = total
        .checked_mul(size_of::<SupportPrimitive>())
        .ok_or(LoweringError::PrimitiveBytesLimit)?;
    if bytes > config.max_primitive_bytes {
        return Err(LoweringError::PrimitiveBytesLimit);
    }

    let mut primitives = Vec::with_capacity(total);
    for (key, member) in live {
        lower_member(key, member, config, &mut primitives)?;
    }
    for ball in balls
        .iter()
        .filter(|ball| live_keys.contains(&ball.owner_key()))
    {
        let geometry = SphereGeometry::joint(ball.center_mm());
        let (start_mm, end_mm) = geometry
            .canonical_poles(ball.radius_mm())
            .map_err(|_| LoweringError::InvalidGeometry)?;
        primitives.push(SupportPrimitive {
            segment_kind: SupportSegmentKind::JointSphere,
            start_mm,
            end_mm,
            start_radius_mm: ball.radius_mm(),
            end_radius_mm: ball.radius_mm(),
            contact_depth_mm: None,
            ring_normal: FrustumRingNormal::SegmentAxis,
            sphere_geometry: Some(geometry),
        });
    }
    primitives.extend_from_slice(raft.primitives());
    debug_assert_eq!(primitives.len(), total);
    if primitives
        .iter()
        .any(|primitive| !primitive.is_semantically_valid())
    {
        return Err(LoweringError::InvalidGeometry);
    }
    let digest = primitive_digest(&primitives, support_count, raft_count);
    Ok(LoweredSupportPlan {
        forest_digest: forest.digest(),
        frame_bits: [
            config.frame.plate_z_mm().get().to_bits(),
            config.frame.lift_mm().get().to_bits(),
        ],
        primitives,
        support_primitive_count: support_count,
        raft_primitive_count: raft_count,
        digest,
    })
}

fn is_routed_stub(forest: &Forest, key: MemberKey, member: &Member) -> bool {
    match member.owner() {
        crate::forest::Owner::Contact(contact) => forest.terminal(*contact) == Some(key),
        crate::forest::Owner::ModelAnchor(_) | crate::forest::Owner::Generated { .. } => false,
    }
}

fn checked_support_count(
    live: &[(MemberKey, &Member)],
    ball_count: usize,
    config: LoweringConfig,
) -> Result<usize, LoweringError> {
    let mut count = ball_count;
    for (key, member) in live {
        count = count
            .checked_add(if member.kind() == &MemberKind::Stub {
                2
            } else if is_plate_root(member, config) {
                let column_len = distance(member.p0_mm(), root_boss_top(*key, member, config)?);
                usize::from(column_len > ZERO_LENGTH_MM) + 1
            } else {
                if distance(member.p0_mm(), member.p1_mm()) <= ZERO_LENGTH_MM {
                    return Err(LoweringError::ZeroLengthMember { member: *key });
                }
                1
            })
            .ok_or(LoweringError::PrimitiveLimit)?;
        if count > config.max_primitives {
            return Err(LoweringError::PrimitiveLimit);
        }
    }
    Ok(count)
}

fn lower_member(
    key: MemberKey,
    member: &Member,
    config: LoweringConfig,
    out: &mut Vec<SupportPrimitive>,
) -> Result<(), LoweringError> {
    if member.kind() == &MemberKind::Stub {
        return lower_stub(key, member, config, out);
    }

    if is_plate_root(member, config) {
        let boss_top = root_boss_top(key, member, config)?;
        if distance(member.p0_mm(), boss_top) > ZERO_LENGTH_MM {
            out.push(frustum(
                SupportSegmentKind::Member,
                member.p0_mm(),
                boss_top,
                member.terminal_radius_mm().get(),
                member.rootward_radius_mm().get(),
                None,
                FrustumRingNormal::PlateZ,
            ));
        }
        let bottom = [
            member.p1_mm()[0],
            member.p1_mm()[1],
            finite(config.frame.plate_z_mm().get() + config.raft_height_mm.get())?,
        ];
        let flare_radius =
            positive(member.rootward_radius_mm().get().get() * config.flare_factor.get())?;
        out.push(frustum(
            SupportSegmentKind::RootFlare,
            boss_top,
            bottom,
            member.rootward_radius_mm().get(),
            flare_radius,
            None,
            FrustumRingNormal::SegmentAxis,
        ));
        return Ok(());
    }

    if distance(member.p0_mm(), member.p1_mm()) <= ZERO_LENGTH_MM {
        return Err(LoweringError::ZeroLengthMember { member: key });
    }
    let embedded_taper = member.embed_end().is_some()
        && member.terminal_radius_mm().get() != member.rootward_radius_mm().get();
    out.push(frustum(
        if embedded_taper {
            SupportSegmentKind::ContactTaper
        } else {
            SupportSegmentKind::Member
        },
        member.p0_mm(),
        member.p1_mm(),
        member.terminal_radius_mm().get(),
        member.rootward_radius_mm().get(),
        embedded_taper.then_some(config.contact_depth_mm),
        FrustumRingNormal::SegmentAxis,
    ));
    Ok(())
}

fn lower_stub(
    key: MemberKey,
    member: &Member,
    config: LoweringConfig,
    out: &mut Vec<SupportPrimitive>,
) -> Result<(), LoweringError> {
    let p0 = member.p0_mm().map(Finite::get);
    let p1 = member.p1_mm().map(Finite::get);
    let axis = std::array::from_fn(|i| p1[i] - p0[i]);
    let length = axis.iter().map(|value| value * value).sum::<f64>().sqrt();
    if length <= ZERO_LENGTH_MM {
        return Err(LoweringError::ZeroLengthMember { member: key });
    }
    let unit = axis.map(|value| value / length);
    let ball_radius =
        positive(config.tip_ball_factor.get() * member.terminal_radius_mm().get().get())?;
    let center = std::array::from_fn(|i| p0[i] + ball_radius.get() * unit[i]);
    let geometry =
        SphereGeometry::tip_from_f64(center, unit).map_err(|_| LoweringError::InvalidGeometry)?;
    let (start_mm, end_mm) = geometry
        .canonical_poles(ball_radius)
        .map_err(|_| LoweringError::InvalidGeometry)?;
    out.push(SupportPrimitive {
        segment_kind: SupportSegmentKind::ContactSphere,
        start_mm,
        end_mm,
        start_radius_mm: ball_radius,
        end_radius_mm: ball_radius,
        contact_depth_mm: None,
        ring_normal: FrustumRingNormal::SegmentAxis,
        sphere_geometry: Some(geometry),
    });
    out.push(frustum(
        SupportSegmentKind::ContactTaper,
        center
            .map(finite)
            .into_iter()
            .collect::<Result<Vec<_>, _>>()?
            .try_into()
            .map_err(|_| LoweringError::InvalidGeometry)?,
        member.p1_mm(),
        member.terminal_radius_mm().get(),
        member.rootward_radius_mm().get(),
        Some(config.contact_depth_mm),
        FrustumRingNormal::SegmentAxis,
    ));
    Ok(())
}

fn is_plate_root(member: &Member, config: LoweringConfig) -> bool {
    (member.p1_mm()[2].get() - config.frame.plate_z_mm().get()).abs() <= config.plate_band_mm.get()
}

fn root_boss_top(
    key: MemberKey,
    member: &Member,
    config: LoweringConfig,
) -> Result<[Finite; 3], LoweringError> {
    let plate = config.frame.plate_z_mm().get();
    let min_layer_run = config.root_layers as f64 * config.layer_height_mm.get();
    let min_no_bend = config.root_no_bend_mm.get().max(min_layer_run);
    let available_from_plate = member.p0_mm()[2].get() - plate;
    if available_from_plate < min_no_bend - 1e-5 {
        return Err(LoweringError::RootRunTooShort { member: key });
    }
    let raft_top = plate + config.raft_height_mm.get();
    let available_above_raft = member.p0_mm()[2].get() - raft_top;
    let minimum_boss_height = (min_no_bend - config.raft_height_mm.get()).max(0.0);
    let effective =
        minimum_boss_height.max(config.flare_length_mm.get().min(available_above_raft / 2.0));
    Ok([
        member.p1_mm()[0],
        member.p1_mm()[1],
        finite(raft_top + effective)?,
    ])
}

fn frustum(
    segment_kind: SupportSegmentKind,
    start_mm: [Finite; 3],
    end_mm: [Finite; 3],
    start_radius_mm: PositiveFinite,
    end_radius_mm: PositiveFinite,
    contact_depth_mm: Option<PositiveFinite>,
    ring_normal: FrustumRingNormal,
) -> SupportPrimitive {
    SupportPrimitive {
        segment_kind,
        start_mm,
        end_mm,
        start_radius_mm,
        end_radius_mm,
        contact_depth_mm,
        ring_normal,
        sphere_geometry: None,
    }
}

fn distance(a: [Finite; 3], b: [Finite; 3]) -> f64 {
    a.into_iter()
        .zip(b)
        .map(|(x, y)| (x.get() - y.get()).powi(2))
        .sum::<f64>()
        .sqrt()
}

fn finite(value: f64) -> Result<Finite, LoweringError> {
    Finite::new(value).map_err(|_| LoweringError::InvalidGeometry)
}

fn positive(value: f64) -> Result<PositiveFinite, LoweringError> {
    PositiveFinite::new(value).map_err(|_| LoweringError::InvalidGeometry)
}

fn primitive_digest(
    primitives: &[SupportPrimitive],
    support_count: usize,
    raft_count: usize,
) -> ContentDigest {
    let mut digest = DigestWriter::new();
    digest.usize(support_count);
    digest.usize(raft_count);
    digest.usize(primitives.len());
    for primitive in primitives {
        digest.byte(primitive.segment_kind as u8);
        for coordinate in primitive.start_mm.into_iter().chain(primitive.end_mm) {
            digest.u64(coordinate.get().to_bits());
        }
        digest.u64(primitive.start_radius_mm.get().to_bits());
        digest.u64(primitive.end_radius_mm.get().to_bits());
        match primitive.contact_depth_mm {
            Some(depth) => {
                digest.byte(1);
                digest.u64(depth.get().to_bits());
            }
            None => digest.byte(0),
        }
        digest.byte(match primitive.ring_normal {
            FrustumRingNormal::SegmentAxis => 0,
            FrustumRingNormal::PlateZ => 1,
        });
        digest.byte(u8::from(primitive.sphere_geometry.is_some()));
    }
    ContentDigest(digest.finish())
}

struct DigestWriter(u64);

impl DigestWriter {
    const fn new() -> Self {
        Self(0xcbf2_9ce4_8422_2325)
    }

    fn byte(&mut self, value: u8) {
        self.0 ^= u64::from(value);
        self.0 = self.0.wrapping_mul(0x0000_0100_0000_01b3);
    }

    fn u64(&mut self, value: u64) {
        for byte in value.to_le_bytes() {
            self.byte(byte);
        }
    }

    fn usize(&mut self, value: usize) {
        self.u64(u64::try_from(value).expect("platform usize fits u64"));
    }

    const fn finish(self) -> u64 {
        self.0
    }
}

#[cfg(test)]
mod tests {
    use std::process::Command;

    use super::*;
    use crate::{
        forest::{EmbedEnd, MemberSpec, Owner, RootwardEndRadius, TerminalEndRadius},
        joints::plan_joint_balls,
        raft::plan_bottom_connected,
    };

    const THREAD_SIGNATURE: &str = "LOWERING_DIGEST=";

    fn f(value: f64) -> Finite {
        Finite::new(value).unwrap()
    }

    fn r(value: f64) -> PositiveFinite {
        PositiveFinite::new(value).unwrap()
    }

    fn member(
        p0: [f64; 3],
        p1: [f64; 3],
        radii: [f64; 2],
        kind: MemberKind,
        ordinal: u32,
        embed_end: Option<EmbedEnd>,
    ) -> Member {
        let owner = if kind == MemberKind::Stub {
            Owner::Contact(ordinal)
        } else {
            Owner::Generated {
                family: "lowering-vector".into(),
                ordinal,
            }
        };
        Member::new(MemberSpec {
            p0_mm: p0.map(f),
            p1_mm: p1.map(f),
            r0_mm: TerminalEndRadius::new(r(radii[0])),
            r1_mm: RootwardEndRadius::new(r(radii[1])),
            kind,
            owner,
            embed_end,
            carried_mm2: f(1.0),
        })
    }

    fn fixture() -> (Forest, BuildFrame, LayerPlan) {
        let mut forest = Forest::new();
        let stub = forest.add_member(member(
            [0.0, 0.0, 5.0],
            [0.0, 0.0, 3.0],
            [0.2, 0.4],
            MemberKind::Stub,
            0,
            Some(EmbedEnd::Terminalward),
        ));
        let root = forest.add_member(member(
            [0.0, 0.0, 3.0],
            [0.0, 0.0, 0.0],
            [0.4, 0.6],
            MemberKind::Trunk,
            1,
            None,
        ));
        forest.set_parent(stub, Some(root)).unwrap();
        forest.register_terminal(0, stub).unwrap();
        forest.add_brace(member(
            [0.0, 0.0, 2.0],
            [2.0, 0.0, 2.0],
            [0.25, 0.25],
            MemberKind::Brace,
            2,
            None,
        ));
        forest.add_member(member(
            [2.0, 0.0, 3.0],
            [2.0, 0.0, 0.0],
            [0.5, 0.7],
            MemberKind::Trunk,
            3,
            None,
        ));
        // A live but unregistered stub is an abandoned route, not physical output.
        forest.add_member(member(
            [9.0, 0.0, 5.0],
            [9.0, 0.0, 3.0],
            [0.2, 0.4],
            MemberKind::Stub,
            4,
            Some(EmbedEnd::Terminalward),
        ));
        (
            forest,
            BuildFrame::new(f(0.0), r(5.0)),
            LayerPlan::new(0.0, vec![0.05; 200]).unwrap(),
        )
    }

    fn lower_fixture(max_primitives: usize, max_bytes: usize) -> LoweredSupportPlan {
        let (forest, frame, layers) = fixture();
        let cfg = LoweringConfig::current(frame, &layers, max_primitives, max_bytes).unwrap();
        let joints = plan_joint_balls(&forest, cfg.joint).unwrap();
        let raft = plan_bottom_connected(&forest, frame.plate_z_mm()).unwrap();
        lower_current(&forest, &joints, &raft, cfg).unwrap()
    }

    #[test]
    fn frozen_member_tip_brace_joint_root_and_raft_vectors_preserve_python_order() {
        let plan = lower_fixture(100, 100 * size_of::<SupportPrimitive>());
        let p = &plan.primitives;
        assert_eq!(
            p[..7]
                .iter()
                .map(|primitive| primitive.segment_kind)
                .collect::<Vec<_>>(),
            vec![
                SupportSegmentKind::ContactSphere,
                SupportSegmentKind::ContactTaper,
                SupportSegmentKind::Member,
                SupportSegmentKind::RootFlare,
                SupportSegmentKind::Member,
                SupportSegmentKind::Member,
                SupportSegmentKind::RootFlare,
            ]
        );
        assert_eq!(p[0].start_mm.map(Finite::get), [0.0, 0.0, 5.0]);
        assert_eq!(p[0].end_mm.map(Finite::get), [0.0, 0.0, 4.4]);
        assert_eq!(p[1].start_mm.map(Finite::get), [0.0, 0.0, 4.7]);
        assert_eq!(p[1].end_mm.map(Finite::get), [0.0, 0.0, 3.0]);
        assert!((p[2].end_mm[2].get() - 1.6).abs() < 1e-12);
        assert!((p[3].start_mm[2].get() - 1.6).abs() < 1e-12);
        assert_eq!(p[3].end_mm.map(Finite::get), [0.0, 0.0, 0.2]);
        assert_eq!(
            p.iter()
                .filter(|primitive| primitive.segment_kind == SupportSegmentKind::RootFlare)
                .count(),
            2,
            "one member-owned boss per physical root; raft owns none"
        );

        let first_raft = plan.support_primitive_count();
        assert!(p[7..first_raft]
            .iter()
            .all(|primitive| primitive.segment_kind == SupportSegmentKind::JointSphere));
        for joint in &p[7..first_raft] {
            assert_eq!(joint.start_mm[0], joint.end_mm[0]);
            assert_eq!(joint.start_mm[1], joint.end_mm[1]);
            assert!(joint.start_mm[2].get() > joint.end_mm[2].get());
        }
        assert_eq!(
            p[first_raft..]
                .iter()
                .map(|primitive| primitive.segment_kind)
                .collect::<Vec<_>>(),
            vec![
                SupportSegmentKind::RaftPadOrBridge,
                SupportSegmentKind::RaftPadOrBridge,
                SupportSegmentKind::RaftPadOrBridge,
            ],
            "one bridge followed by two pads"
        );
        assert_eq!(plan.raft_primitive_count(), 3);
    }

    #[test]
    fn nonconstant_layers_stale_authority_and_caps_fail_by_name() {
        let (mut forest, frame, layers) = fixture();
        let variable = LayerPlan::new(0.0, vec![0.05, 0.04]).unwrap();
        assert_eq!(
            LoweringConfig::current(frame, &variable, 100, 100_000).unwrap_err(),
            LoweringError::NonConstantLayerPlan
        );

        let cfg = LoweringConfig::current(frame, &layers, 100, 100_000).unwrap();
        let joints = plan_joint_balls(&forest, cfg.joint).unwrap();
        let raft = plan_bottom_connected(&forest, frame.plate_z_mm()).unwrap();
        let plan = lower_current(&forest, &joints, &raft, cfg).unwrap();
        forest.add_member(member(
            [4.0, 0.0, 3.0],
            [4.0, 0.0, 0.0],
            [0.4, 0.5],
            MemberKind::Trunk,
            8,
            None,
        ));
        assert_eq!(
            plan.validate(&forest, frame).unwrap_err(),
            LoweringError::StaleAuthority
        );

        let exact = lower_fixture(100, 100_000);
        let total = exact.primitives.len();
        let (forest, frame, layers) = fixture();
        let joints_cfg = LoweringConfig::current(frame, &layers, 100, 100_000).unwrap();
        let joints = plan_joint_balls(&forest, joints_cfg.joint).unwrap();
        let raft = plan_bottom_connected(&forest, frame.plate_z_mm()).unwrap();
        let capped = LoweringConfig::current(frame, &layers, total - 1, 100_000).unwrap();
        assert_eq!(
            lower_current(&forest, &joints, &raft, capped).unwrap_err(),
            LoweringError::PrimitiveLimit
        );
        let byte_cap = total * size_of::<SupportPrimitive>();
        let capped = LoweringConfig::current(frame, &layers, total, byte_cap - 1).unwrap();
        assert_eq!(
            lower_current(&forest, &joints, &raft, capped).unwrap_err(),
            LoweringError::PrimitiveBytesLimit
        );
        let exact = LoweringConfig::current(frame, &layers, total, byte_cap).unwrap();
        assert!(lower_current(&forest, &joints, &raft, exact).is_ok());
    }

    #[test]
    fn digest_is_identical_at_one_four_and_sixteen_threads() {
        if let Ok(expected) = std::env::var("LOWERING_THREAD_CHILD") {
            assert_eq!(geom_kernel::current_threads().to_string(), expected);
            println!(
                "{THREAD_SIGNATURE}{:016x}",
                lower_fixture(100, 100_000).digest().0
            );
            return;
        }
        let current = std::env::current_exe().unwrap();
        let test_name = "lower::tests::digest_is_identical_at_one_four_and_sixteen_threads";
        let mut outputs = Vec::new();
        for threads in [1, 4, 16] {
            let output = Command::new(&current)
                .arg("--exact")
                .arg(test_name)
                .arg("--nocapture")
                .env("PLINTH_THREADS", threads.to_string())
                .env("LOWERING_THREAD_CHILD", threads.to_string())
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
            let stdout = String::from_utf8(output.stdout).unwrap();
            outputs.push(
                stdout
                    .lines()
                    .find_map(|line| line.strip_prefix(THREAD_SIGNATURE))
                    .unwrap()
                    .to_owned(),
            );
        }
        assert!(outputs.windows(2).all(|pair| pair[0] == pair[1]));
    }
}
