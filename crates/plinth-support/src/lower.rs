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
    joints::{FilteredLiveView, JointConfig, JointError, JointPlan},
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
    pub(crate) const fn joint_config(self) -> JointConfig {
        self.joint
    }

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
    live_view_digest: u64,
    frame_bits: [u64; 2],
    primitives: Vec<SupportPrimitive>,
    member_keys: Vec<MemberKey>,
    support_primitive_count: usize,
    raft_primitive_count: usize,
    digest: ContentDigest,
}

impl LoweredSupportPlan {
    pub(crate) fn validate<'a>(
        &'a self,
        forest: &Forest,
        view: &FilteredLiveView,
        frame: BuildFrame,
    ) -> Result<&'a [SupportPrimitive], LoweringError> {
        view.validate(forest)
            .map_err(|_| LoweringError::StaleAuthority)?;
        if self.forest_digest != forest.digest()
            || self.live_view_digest != view.digest()
            || self.frame_bits
                != [
                    frame.plate_z_mm().get().to_bits(),
                    frame.lift_mm().get().to_bits(),
                ]
        {
            return Err(LoweringError::StaleAuthority);
        }
        if self.digest
            != primitive_digest(
                self.live_view_digest,
                &self.primitives,
                self.support_primitive_count,
                self.raft_primitive_count,
            )
        {
            return Err(LoweringError::StaleAuthority);
        }
        if self.member_keys != view.keys() {
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

    pub(crate) fn member_keys(&self) -> &[MemberKey] {
        &self.member_keys
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
    #[error("support lowering authority is stale or belongs to another forest/view/frame")]
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
    view: &FilteredLiveView,
    joints: &JointPlan,
    raft: &RaftPlan,
    config: LoweringConfig,
) -> Result<LoweredSupportPlan, LoweringError> {
    let balls = joints.validate_filtered(forest, view, config.joint)?;
    raft.validate(forest, view, config.frame.plate_z_mm())
        .map_err(|_| LoweringError::StaleAuthority)?;

    let support_count = checked_support_count(forest, view, balls.len(), config)?;
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
    let mut member_keys = Vec::with_capacity(view.keys().len());
    for key in view.keys() {
        let member = forest.member(*key).ok_or(LoweringError::StaleAuthority)?;
        lower_member(*key, member, config, &mut primitives)?;
        member_keys.push(*key);
    }
    for ball in balls {
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
    let digest = primitive_digest(view.digest(), &primitives, support_count, raft_count);
    Ok(LoweredSupportPlan {
        forest_digest: forest.digest(),
        live_view_digest: view.digest(),
        frame_bits: [
            config.frame.plate_z_mm().get().to_bits(),
            config.frame.lift_mm().get().to_bits(),
        ],
        primitives,
        member_keys,
        support_primitive_count: support_count,
        raft_primitive_count: raft_count,
        digest,
    })
}

fn checked_support_count(
    forest: &Forest,
    view: &FilteredLiveView,
    ball_count: usize,
    config: LoweringConfig,
) -> Result<usize, LoweringError> {
    let mut count = ball_count;
    for key in view.keys() {
        let member = forest.member(*key).ok_or(LoweringError::StaleAuthority)?;
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
            let embedded_taper = is_embedded_taper(member);
            out.push(frustum(
                if embedded_taper {
                    SupportSegmentKind::ContactTaper
                } else {
                    SupportSegmentKind::Member
                },
                member.p0_mm(),
                boss_top,
                member.terminal_radius_mm().get(),
                member.rootward_radius_mm().get(),
                embedded_taper.then_some(config.contact_depth_mm),
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
    let embedded_taper = is_embedded_taper(member);
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

fn is_embedded_taper(member: &Member) -> bool {
    member.model_contact_end().is_some()
        && (member.terminal_radius_mm().get().get() - member.rootward_radius_mm().get().get()).abs()
            > ZERO_LENGTH_MM
}

fn lower_stub(
    key: MemberKey,
    member: &Member,
    config: LoweringConfig,
    out: &mut Vec<SupportPrimitive>,
) -> Result<(), LoweringError> {
    let p0 = member.p0_mm().map(Finite::get);
    let p1 = member.p1_mm().map(Finite::get);
    let axis: [f64; 3] = std::array::from_fn(|i| p1[i] - p0[i]);
    let length = axis.iter().map(|value| value * value).sum::<f64>().sqrt();
    if length <= ZERO_LENGTH_MM {
        return Err(LoweringError::ZeroLengthMember { member: key });
    }
    let unit = axis.map(|value| value / length);
    let ball_radius =
        positive(config.tip_ball_factor.get() * member.terminal_radius_mm().get().get())?;
    let center: [f64; 3] = std::array::from_fn(|i| p0[i] + ball_radius.get() * unit[i]);
    let start_mm = member.p0_mm();
    let end_raw: [f64; 3] = std::array::from_fn(|i| p0[i] + 2.0 * ball_radius.get() * unit[i]);
    let end_mm = finite3(end_raw)?;
    let geometry = SphereGeometry::tip_with_exact_record_poles(
        finite3(center)?,
        finite3(unit)?,
        ball_radius,
        start_mm,
        end_mm,
    )
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
        finite3(center)?,
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
    match root_boss_top_at(
        member.p0_mm()[2].get(),
        [member.p1_mm()[0], member.p1_mm()[1]],
        config,
    ) {
        Some(result) => result,
        None => Err(LoweringError::RootRunTooShort { member: key }),
    }
}

/// The plate-root boss arithmetic with no forest identity in it.
///
/// Extracted verbatim from [`root_boss_top`], which now delegates here, so the
/// slice-sweep adapter below computes the SAME boss top as the legacy
/// lowering rather than a second, drifting copy of it. `None` is the
/// too-short-run case that [`root_boss_top`] reports as
/// `RootRunTooShort` — the caller decides what to do about it, because the
/// sweep warns and emits where the legacy path refuses.
fn root_boss_top_at(
    terminal_z_mm: f64,
    rootward_xy: [Finite; 2],
    config: LoweringConfig,
) -> Option<Result<[Finite; 3], LoweringError>> {
    let plate = config.frame.plate_z_mm().get();
    let min_layer_run = config.root_layers as f64 * config.layer_height_mm.get();
    let min_no_bend = config.root_no_bend_mm.get().max(min_layer_run);
    let available_from_plate = terminal_z_mm - plate;
    if available_from_plate < min_no_bend - 1e-5 {
        return None;
    }
    let raft_top = plate + config.raft_height_mm.get();
    let available_above_raft = terminal_z_mm - raft_top;
    let minimum_boss_height = (min_no_bend - config.raft_height_mm.get()).max(0.0);
    let effective =
        minimum_boss_height.max(config.flare_length_mm.get().min(available_above_raft / 2.0));
    Some(
        finite(raft_top + effective)
            .map(|top| [rootward_xy[0], rootward_xy[1], top]),
    )
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

fn finite3(values: [f64; 3]) -> Result<[Finite; 3], LoweringError> {
    Ok([finite(values[0])?, finite(values[1])?, finite(values[2])?])
}

fn positive(value: f64) -> Result<PositiveFinite, LoweringError> {
    PositiveFinite::new(value).map_err(|_| LoweringError::InvalidGeometry)
}

fn primitive_digest(
    live_view_digest: u64,
    primitives: &[SupportPrimitive],
    support_count: usize,
    raft_count: usize,
) -> ContentDigest {
    let mut digest = DigestWriter::new();
    digest.u64(live_view_digest);
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
        match primitive.sphere_geometry.as_ref() {
            Some(geometry) => {
                if let Some((center, axis)) = geometry.tip_authority() {
                    digest.byte(1);
                    for value in center.into_iter().chain(axis) {
                        digest.u64(value.get().to_bits());
                    }
                } else if let Some(center) = geometry.joint_center() {
                    digest.byte(2);
                    for value in center {
                        digest.u64(value.get().to_bits());
                    }
                } else {
                    unreachable!("closed sphere geometry kind has one authority")
                }
            }
            None => digest.byte(0),
        }
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

// ---------------------------------------------------------------------------
// The slice-sweep adapter's geometry door.
//
// `docs/plans/slice-sweep-core-2026-09-02.md` §9.2: "The adapter must not
// re-derive this geometry by hand." It lives HERE, in the module that already
// owns the canonical ball, boss and flare arithmetic, rather than in
// `plinth-sweep`, because `LoweringConfig`, `MemberKey` and `Member` are all
// crate-private and promoting them would expose the whole forest data model in
// order to reach four numbers. Nothing below computes a ball pole, a boss top
// or a flare radius a second time: the poles come from
// `SphereGeometry::canonical_poles`, the boss top from `root_boss_top_at`
// (which `root_boss_top` now delegates to), and the flare from the same
// registered `FLARE_R_FACTOR` the legacy path reads.
// ---------------------------------------------------------------------------

/// Where one sweep arm's bottom end sits.
///
/// The sweep's own `Landing` carries a layer index and an arm id that this
/// stage has no use for; the only distinction geometry cares about is whether
/// the plate-root chain applies.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SweepArmLanding {
    /// The build plate: the arm gets the legacy plate-root chain — a boss
    /// column down to the boss top, then a `RootFlare` down to the raft top.
    Plate,
    /// Cured model material or another arm. The arm ends there. v1 emits no
    /// joint sphere at an arm-onto-arm junction; that is disclosed in the run
    /// report rather than hidden.
    Landed,
}

/// One straight vertical arm as the slice sweep placed and sized it.
///
/// *UNITS TRAP*: these are RADII. The registry speaks radius; ChituBox and the
/// owner speak diameter.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SweepArm {
    /// The seat, on the model's down-facing surface — the BOTTOM plane of the
    /// first cured model lamina in this column. The tip is seated INTO the
    /// model from here by [`contact::seat_depth_of`]; this field is the
    /// surface, never the finished pole.
    pub seat_mm: [f64; 3],
    /// The landing point, directly below the seat.
    pub base_mm: [f64; 3],
    /// Radius at the seat — the narrowest section, and the one the area law is
    /// evaluated at.
    pub tip_radius_mm: f64,
    /// Radius at the landing. Never smaller than the tip: carried load is
    /// non-decreasing downward, so an arm flares toward its root.
    pub root_radius_mm: f64,
    /// OUTWARD unit normal of the model face this tip carries — the direction
    /// that points AWAY from the material, so seating moves against it.
    ///
    /// It sets `theta` in the seat law's full-cross-section term. A zero or
    /// non-finite vector means "unmeasured", and the arm's own axis stands in,
    /// which is `theta = 0` — exactly what `lower_stub` does when it uses the
    /// member direction in place of a face normal.
    pub seat_normal_mm: [f64; 3],
    /// Cured model thickness standing ABOVE `seat_mm` in this arm's own seat
    /// column, in mm, as the sweep's own layer rasters measured it.
    ///
    /// This is the land-at-strike bound applied at the TIP end: a ball may be
    /// seated no deeper than the material it strikes is thick, so it can never
    /// exit the far face of a thin skin. Where it binds, the seat is
    /// TRUNCATED to the available depth (owner ruling 2026-08-30: truncate,
    /// never retract) and counted in
    /// [`SweepLowering::tips_truncated_by_thin_wall`] — the arm is never
    /// dropped.
    pub available_seat_depth_mm: f64,
    pub landing: SweepArmLanding,
}

/// What lowering one sweep field produced, and everything it had to warn about.
///
/// **Emit and warn, never refuse** (owner ruling 2026-09-01): an arm this
/// stage cannot build is DROPPED and COUNTED, and the rest of the field still
/// emits.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SweepLowering {
    pub primitives: Vec<SupportPrimitive>,
    /// Arms whose seat and landing coincide to within a nanometre. There is no
    /// geometry to build and none is invented.
    pub arms_dropped_zero_length: usize,
    /// Plate arms whose run from the plate is shorter than the registered
    /// no-bend height, so no root boss fits. They emit as a straight column to
    /// the raft top with NO flare, and are counted here.
    pub arms_without_root_flare: usize,
    /// Arms whose radii or points would not cross the finite boundary, or
    /// which the emit boundary's own `is_semantically_valid` rejected.
    pub arms_dropped_invalid_geometry: usize,
    /// Tips whose seat was cut short because the model material above the seat
    /// is thinner than the law asked for. TRUNCATED, never retracted and never
    /// dropped: the ball is seated to every micron that is actually there.
    pub tips_truncated_by_thin_wall: usize,
    /// Tips the law seated at its own smallest term — the registered
    /// `DEFAULT_CONTACT_DEPTH_MM` — because neither the resolution/flank term
    /// nor the ball-cap term asked for more.
    pub tips_at_law_minimum: usize,
    /// Every tip's DELIVERED seat depth, normal-measured, in arm order.
    ///
    /// Published rather than summarised in place so the caller can render the
    /// distribution it wants and a test can assert on the raw numbers. Empty
    /// exactly when no arm was built.
    pub tip_seat_depth_mm: Vec<f64>,
    /// Every tip's DELIVERED cut-disc radius `a = sqrt(2·f·r_tip·h − h²)` at
    /// its own delivered depth, in the same order — the ACTUAL minimal
    /// separating cut the area law is evaluated on. The `a ≥ r_tip` guarantee
    /// is checked against this, never assumed.
    pub tip_cut_radius_mm: Vec<f64>,
    /// Every tip's radius, in the same order, so `a` and `r_tip` can be
    /// compared arm by arm without a second pass over the primitives.
    pub tip_radius_mm: Vec<f64>,
}

impl SweepLowering {
    /// `(min, p50, max)` of a per-arm series, or `None` when no arm was built.
    ///
    /// One sort of a copy, not a per-arm scan per statistic.
    #[must_use]
    pub fn distribution(series: &[f64]) -> Option<(f64, f64, f64)> {
        if series.is_empty() {
            return None;
        }
        let mut sorted = series.to_vec();
        sorted.sort_by(f64::total_cmp);
        Some((
            sorted[0],
            sorted[sorted.len() / 2],
            sorted[sorted.len() - 1],
        ))
    }

    /// The arms whose delivered cut disc is NARROWER than their own tip — the
    /// `a ≥ r_tip` guarantee failing. Zero is the guarantee holding, and it is
    /// counted rather than assumed.
    ///
    /// **Why the comparison carries a tolerance.** The ball clause is designed
    /// to make `a` EQUAL `r_tip` at its own break-even depth
    /// (`BALL_SEAT_FACTOR · r_tip`), and since the smallest seat depth became
    /// 0.100 mm that clause is the binding term for most tips — so the common
    /// case is exact equality, which float rounding straddles in both
    /// directions. A bare `<` counts those ULPs as failures and reports noise
    /// where the law is being met precisely. The tolerance is relative and
    /// tiny: a shortfall has to be real to be counted.
    #[must_use]
    pub fn tips_below_area_guarantee(&self) -> usize {
        self.tip_cut_radius_mm
            .iter()
            .zip(&self.tip_radius_mm)
            .filter(|(cut, tip)| **cut < **tip * (1.0 - Self::AREA_GUARANTEE_TOLERANCE))
            .count()
    }

    /// The relative slack in the `a ≥ r_tip` comparison — float noise around an
    /// equality the law designs for, and nothing more.
    pub const AREA_GUARANTEE_TOLERANCE: f64 = 1.0e-9;
}

/// The failures that are about the REQUEST, never about a model.
#[derive(Clone, Debug, PartialEq, Eq, Error)]
pub enum SweepLoweringError {
    #[error("sweep lowering requires a non-empty layer plan")]
    MissingLayerHeight,
    #[error("sweep lowering does not define variable layer heights")]
    NonConstantLayerPlan,
    #[error("sweep lowering requires nonzero work and memory limits")]
    InvalidLimits,
    #[error("sweep lowering would exceed its primitive-count limit of {limit}")]
    PrimitiveLimit { limit: usize },
    #[error("registered current-parity constants are unavailable: {0}")]
    RegisteredConstants(String),
}

impl From<LoweringError> for SweepLoweringError {
    fn from(error: LoweringError) -> Self {
        match error {
            LoweringError::MissingLayerHeight => Self::MissingLayerHeight,
            LoweringError::NonConstantLayerPlan => Self::NonConstantLayerPlan,
            LoweringError::RegisteredConstants(message) => Self::RegisteredConstants(message),
            _ => Self::InvalidLimits,
        }
    }
}

/// Lower a slice-sweep support field to the emit boundary's own primitives.
///
/// The chain per arm is the one the legacy lowering already emits for a
/// plate-rooted column carrying a contact ball:
///
/// 1. `ContactSphere` — the tip ball at the seat, poles from
///    [`SphereGeometry::canonical_poles`], never recomputed here.
/// 2. `ContactTaper` — the ball centre down to the boss top (plate landing) or
///    to the landing point (model/arm landing), carrying the registered
///    `DEFAULT_CONTACT_DEPTH_MM`.
/// 3. `RootFlare` — plate landings only, boss top down to the raft top.
///
/// # Errors
///
/// Only about the request: an empty or variable-height layer plan, zero
/// limits, an unreadable registry, or a field larger than `max_primitives`. A
/// shortfall in one ARM is a counter on [`SweepLowering`], never an error.
pub fn lower_sweep_arms(
    arms: &[SweepArm],
    frame: BuildFrame,
    layers: &LayerPlan,
    max_primitives: usize,
    max_primitive_bytes: usize,
) -> Result<SweepLowering, SweepLoweringError> {
    let config = LoweringConfig::current(frame, layers, max_primitives, max_primitive_bytes)?;
    let plate = config.frame.plate_z_mm().get();
    let raft_top = plate + config.raft_height_mm.get();
    let mut out = SweepLowering {
        primitives: Vec::with_capacity(arms.len().saturating_mul(3)),
        tip_seat_depth_mm: Vec::with_capacity(arms.len()),
        tip_cut_radius_mm: Vec::with_capacity(arms.len()),
        tip_radius_mm: Vec::with_capacity(arms.len()),
        ..SweepLowering::default()
    };
    for arm in arms {
        let lowered = lower_one_sweep_arm(arm, config, raft_top, &mut out.primitives);
        match lowered.outcome {
            ArmOutcome::Built => {}
            ArmOutcome::BuiltWithoutFlare => out.arms_without_root_flare += 1,
            ArmOutcome::ZeroLength => out.arms_dropped_zero_length += 1,
            ArmOutcome::InvalidGeometry => out.arms_dropped_invalid_geometry += 1,
        }
        if let Some(seat) = lowered.seat {
            out.tip_seat_depth_mm.push(seat.depth_mm);
            out.tip_cut_radius_mm.push(seat.cut_radius_mm);
            out.tip_radius_mm.push(seat.tip_radius_mm);
            out.tips_truncated_by_thin_wall += usize::from(seat.truncated);
            out.tips_at_law_minimum += usize::from(seat.at_law_minimum);
        }
        if out.primitives.len() > max_primitives {
            return Err(SweepLoweringError::PrimitiveLimit {
                limit: max_primitives,
            });
        }
    }
    Ok(out)
}

enum ArmOutcome {
    Built,
    BuiltWithoutFlare,
    ZeroLength,
    InvalidGeometry,
}

/// What one arm's tip seat actually delivered, recorded only for an arm that
/// reached the file.
struct TipSeat {
    /// The delivered depth, measured along the carried face's own normal.
    depth_mm: f64,
    /// `a = sqrt(2·f·r_tip·h − h²)` at that delivered depth — the ACTUAL
    /// minimal separating cut, not `r_tip`.
    cut_radius_mm: f64,
    tip_radius_mm: f64,
    /// The thin-wall bound cut the law's depth short.
    truncated: bool,
    /// The law's smallest term — the registered contact depth — was the
    /// binding one.
    at_law_minimum: bool,
}

struct ArmLowering {
    outcome: ArmOutcome,
    seat: Option<TipSeat>,
}

impl ArmLowering {
    const fn refused(outcome: ArmOutcome) -> Self {
        Self {
            outcome,
            seat: None,
        }
    }
}

/// The tip seat for one arm, by the law in `docs/physics.md` §9, bounded by
/// the material that is actually there.
///
/// `unit` runs seat → base, so it points OUT of the model along the arm, and
/// seating moves against it. Returns `(seated_point, TipSeat)`.
///
/// **Direction.** The law's `h` is measured along the carried face's normal
/// (`contact::seat_endpoint`'s own convention), but a sweep arm is a placed
/// COLUMN: moving its tip off its own axis would bend a member the placement
/// pass already routed and cleared. So the tip travels along the ARM AXIS by
/// `h / |cos θ|`, which lands the pole at normal-depth exactly `h`. For the
/// case this defect was reported on — a vertical arm under a horizontal
/// underside, `θ = 0` — the two are the same move.
///
/// **The thin-wall bound.** The pole may rise no further than the cured model
/// standing above the seat in this arm's own column, so a ball can never exit
/// the far face of a thin skin (land-at-strike). Where that binds the seat is
/// TRUNCATED to what is there and flagged; the arm still emits.
fn seat_tip(
    arm: &SweepArm,
    unit: [f64; 3],
    tip_radius_mm: f64,
    config: LoweringConfig,
) -> ([f64; 3], TipSeat) {
    // An unmeasured normal falls back to the arm's own axis (theta = 0),
    // exactly as `lower_stub` uses the member direction in place of a face
    // normal.
    let outward = geom_kernel::vec3::normalize(arm.seat_normal_mm).unwrap_or(unit);
    let abs_cos = geom_kernel::vec3::dot(unit, outward).abs().min(1.0);
    let floor_depth_mm = config.contact_depth_mm.get();
    let law_depth_mm = crate::contact::seat_depth_of(
        floor_depth_mm,
        config.layer_height_mm.get(),
        config.tip_ball_factor.get(),
        abs_cos,
        tip_radius_mm,
        true,
    );
    // **The seat is never driven past its own ball's equator.** The disc the
    // seat plane cuts through the ball is `a² = 2·f·r·h − h²`, which PEAKS at
    // `h = f·r` — the ball radius — and narrows again beyond it. Burial deeper
    // than that buys less contact, not more, while the scar keeps growing:
    // exactly the owner's ruling of 2026-08-30 that burial is not adhesion,
    // that depth stays small, and that rasterised CONTACT AREA governs. It is
    // a property of the emitted ball, not a new number, and it is what makes
    // `a ≥ r_tip` hold at BOTH ends of the law's range rather than only the
    // shallow one.
    let equator_mm = config.tip_ball_factor.get() * tip_radius_mm;
    let target_depth_mm = law_depth_mm.min(equator_mm);
    // Axial travel that delivers NORMAL depth `target_depth_mm`, bounded by the
    // ball's own length along the arm. A nearly-parallel seat would otherwise
    // ask to travel to infinity along an axis that never gets any deeper; past
    // its own diameter the ball is wholly inside the model in any case, and
    // only the shaft still crosses the surface.
    let axial_for_target_mm = if abs_cos > 0.0 {
        target_depth_mm / abs_cos
    } else {
        f64::INFINITY
    };
    // The arm descends from its seat, so travelling against `unit` RAISES the
    // pole; the rise is bounded by the cured column above the seat. A leaned
    // arm also travels sideways, and bounding its VERTICAL rise by the
    // column's own vertical extent is the conservative reading of the same
    // raster.
    let rise_per_axial = -unit[2];
    let axial_bound_mm = if rise_per_axial > 0.0 {
        arm.available_seat_depth_mm / rise_per_axial
    } else {
        0.0
    };
    let axial_mm = axial_for_target_mm
        .min(2.0 * equator_mm)
        .min(axial_bound_mm)
        .max(0.0);
    let axial_mm = if axial_mm.is_finite() { axial_mm } else { 0.0 };
    let depth_mm = axial_mm * abs_cos;
    let seated = std::array::from_fn(|index| arm.seat_mm[index] - unit[index] * axial_mm);
    // THE DELIVERED MINIMAL SEPARATING CUT at the seat plane — the disc the
    // model surface actually takes through the emitted ball, which is what the
    // area law has to be evaluated on. It is not `r_tip`, and it is measured
    // here rather than assumed anywhere.
    let cut_radius_mm =
        crate::contact::ball_cut_radius_mm(tip_radius_mm, depth_mm, config.tip_ball_factor.get());
    (
        seated,
        TipSeat {
            depth_mm,
            cut_radius_mm,
            tip_radius_mm,
            // A shortfall against the depth this seat was ASKED for. The
            // equator cap is not a shortfall — it is the law satisfied by a
            // shallower seat that cuts a wider disc — so the target it is
            // measured against already carries it. What does fire here is the
            // thin-wall bound, and a genuinely grazing seat.
            truncated: depth_mm < target_depth_mm - 1e-12,
            // `law_depth_mm` is a `max` over three terms of which the floor is
            // one, so it can never be below the floor; `<=` is equality.
            at_law_minimum: law_depth_mm <= floor_depth_mm,
        },
    )
}

fn lower_one_sweep_arm(
    arm: &SweepArm,
    config: LoweringConfig,
    raft_top: f64,
    out: &mut Vec<SupportPrimitive>,
) -> ArmLowering {
    let delta: [f64; 3] = std::array::from_fn(|index| arm.base_mm[index] - arm.seat_mm[index]);
    let length = delta.iter().map(|value| value * value).sum::<f64>().sqrt();
    if !length.is_finite() || length <= ZERO_LENGTH_MM {
        return ArmLowering::refused(ArmOutcome::ZeroLength);
    }
    let unit = delta.map(|value| value / length);
    let Ok(tip_radius) = positive(arm.tip_radius_mm) else {
        return ArmLowering::refused(ArmOutcome::InvalidGeometry);
    };
    let Ok(root_radius) = positive(arm.root_radius_mm.max(arm.tip_radius_mm)) else {
        return ArmLowering::refused(ArmOutcome::InvalidGeometry);
    };
    let Ok(ball_radius) = positive(config.tip_ball_factor.get() * tip_radius.get()) else {
        return ArmLowering::refused(ArmOutcome::InvalidGeometry);
    };
    // THE SEAT. Until 2026-09-05 the ball's pole sat exactly on `arm.seat_mm`
    // — the BOTTOM plane of the first cured model lamina — so the tip touched
    // the model at a single point and carried an area of zero. It is now
    // driven INTO the model by the seat law every other seating family in this
    // crate reads (`contact::seat_depth_of`), bounded by the material that is
    // there.
    let (seated_seat, tip_seat) = seat_tip(arm, unit, tip_radius.get(), config);
    let center_raw: [f64; 3] =
        std::array::from_fn(|index| seated_seat[index] + ball_radius.get() * unit[index]);
    let (Ok(center), Ok(unit_finite), Ok(base)) =
        (finite3(center_raw), finite3(unit), finite3(arm.base_mm))
    else {
        return ArmLowering::refused(ArmOutcome::InvalidGeometry);
    };
    let Ok(geometry) = SphereGeometry::tip(center, unit_finite) else {
        return ArmLowering::refused(ArmOutcome::InvalidGeometry);
    };
    // The producer's own poles. A second implementation of this arithmetic is
    // exactly the defect `is_semantically_valid` exists to catch.
    let Ok((sphere_start, sphere_end)) = geometry.canonical_poles(ball_radius) else {
        return ArmLowering::refused(ArmOutcome::InvalidGeometry);
    };
    let mut staged: Vec<SupportPrimitive> = Vec::with_capacity(3);
    staged.push(SupportPrimitive {
        segment_kind: SupportSegmentKind::ContactSphere,
        start_mm: sphere_start,
        end_mm: sphere_end,
        start_radius_mm: ball_radius,
        end_radius_mm: ball_radius,
        contact_depth_mm: None,
        ring_normal: FrustumRingNormal::SegmentAxis,
        sphere_geometry: Some(geometry),
    });

    let outcome = match arm.landing {
        SweepArmLanding::Landed => {
            if distance(center, base) <= ZERO_LENGTH_MM {
                return ArmLowering::refused(ArmOutcome::ZeroLength);
            }
            staged.push(frustum(
                SupportSegmentKind::ContactTaper,
                center,
                base,
                tip_radius,
                root_radius,
                Some(config.contact_depth_mm),
                FrustumRingNormal::SegmentAxis,
            ));
            ArmOutcome::Built
        }
        SweepArmLanding::Plate => {
            let Ok(raft_z) = finite(raft_top) else {
                return ArmLowering::refused(ArmOutcome::InvalidGeometry);
            };
            let raft_point = [base[0], base[1], raft_z];
            // The SEATED terminal z, exactly as the legacy path reads
            // `member.p0_mm()[2]` — which `initial.rs` already minted embedded.
            let boss_top =
                root_boss_top_at(seated_seat[2], [base[0], base[1]], config).and_then(Result::ok);
            match boss_top {
                Some(top)
                    if distance(center, top) > ZERO_LENGTH_MM
                        && distance(top, raft_point) > ZERO_LENGTH_MM =>
                {
                    let Ok(flare_radius) = positive(root_radius.get() * config.flare_factor.get())
                    else {
                        return ArmLowering::refused(ArmOutcome::InvalidGeometry);
                    };
                    staged.push(frustum(
                        SupportSegmentKind::ContactTaper,
                        center,
                        top,
                        tip_radius,
                        root_radius,
                        Some(config.contact_depth_mm),
                        FrustumRingNormal::PlateZ,
                    ));
                    staged.push(frustum(
                        SupportSegmentKind::RootFlare,
                        top,
                        raft_point,
                        root_radius,
                        flare_radius,
                        None,
                        FrustumRingNormal::SegmentAxis,
                    ));
                    ArmOutcome::Built
                }
                _ => {
                    // The run from the plate is shorter than the registered
                    // no-bend height, or leaves no room between boss top and
                    // raft, so no flare fits. Emit the straight column and
                    // WARN: refusing here would destroy the evidence.
                    if distance(center, raft_point) <= ZERO_LENGTH_MM {
                        return ArmLowering::refused(ArmOutcome::ZeroLength);
                    }
                    staged.push(frustum(
                        SupportSegmentKind::ContactTaper,
                        center,
                        raft_point,
                        tip_radius,
                        root_radius,
                        Some(config.contact_depth_mm),
                        FrustumRingNormal::PlateZ,
                    ));
                    ArmOutcome::BuiltWithoutFlare
                }
            }
        }
    };
    // The emit boundary's own contract, asked HERE, so one bad arm is dropped
    // and counted rather than failing the whole field at
    // `SupportedModel::from_parts` with nothing said about which arm did it.
    if staged.iter().any(|primitive| !primitive.is_semantically_valid()) {
        return ArmLowering::refused(ArmOutcome::InvalidGeometry);
    }
    out.append(&mut staged);
    ArmLowering {
        outcome,
        seat: Some(tip_seat),
    }
}

#[cfg(test)]
mod tests {
    use std::process::Command;

    use super::*;
    use crate::{
        forest::{
            ContactEnd, MemberSpec, Owner, RootwardEndRadius, TerminalEndRadius,
            TerminalLedgerState,
        },
        joints::{plan_joint_balls_filtered, JointStagingLimits, LiveViewLimits},
        raft::{plan_bottom_connected, RaftLimits},
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
        model_contact_end: Option<ContactEnd>,
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
            model_contact_end,
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
            Some(ContactEnd::Terminalward),
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
        // Failed contact provenance is retired before the shared filtered view is built.
        let unrouted = forest.add_member(member(
            [9.0, 0.0, 5.0],
            [9.0, 0.0, 3.0],
            [0.2, 0.4],
            MemberKind::Stub,
            4,
            Some(ContactEnd::Terminalward),
        ));
        forest.register_terminal(4, unrouted).unwrap();
        forest.retire(unrouted).unwrap();
        (
            forest,
            BuildFrame::new(f(0.0), r(5.0)),
            LayerPlan::new(0.0, vec![0.05; 200]).unwrap(),
        )
    }

    fn lower_fixture(max_primitives: usize, max_bytes: usize) -> LoweredSupportPlan {
        let (forest, frame, layers) = fixture();
        let cfg = LoweringConfig::current(frame, &layers, max_primitives, max_bytes).unwrap();
        let view = FilteredLiveView::current(&forest, &[4], LiveViewLimits::new(100, 10, 100_000))
            .unwrap();
        let joints = plan_joint_balls_filtered(
            &forest,
            &view,
            cfg.joint,
            JointStagingLimits::from_joint_config(cfg.joint),
        )
        .unwrap();
        let raft = plan_bottom_connected(
            &forest,
            &view,
            frame.plate_z_mm(),
            RaftLimits::new(100, 100, 200, 100_000),
        )
        .unwrap();
        lower_current(&forest, &view, &joints, &raft, cfg).unwrap()
    }

    #[test]
    #[expect(
        clippy::float_cmp,
        reason = "test-local arithmetic and exact pins over hand-written, bounded fixture values. See \
              docs/evidence/remediation-2026-08-26/census/cast-audit.md"
    )]
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
                SupportSegmentKind::RaftBridge,
                SupportSegmentKind::RaftPad,
                SupportSegmentKind::RaftPad,
            ],
            "one bridge followed by two pads"
        );
        assert_eq!(plan.raft_primitive_count(), 3);
    }

    #[test]
    fn angled_contact_poles_and_embedded_taper_boundary_match_python_float_arithmetic() {
        let mut forest = Forest::new();
        let p0 = [0.1, -0.2, 3.0];
        let p1 = [1.7, 2.3, 4.1];
        let stub = forest.add_member(member(
            p0,
            p1,
            [0.2, 0.4],
            MemberKind::Stub,
            11,
            Some(ContactEnd::Terminalward),
        ));
        forest.register_terminal(11, stub).unwrap();
        for (ordinal, delta) in [(20, 0.5e-9), (21, 1.0e-9), (22, 2.0e-9)] {
            forest.add_member(member(
                [ordinal as f64, 0.3, 4.0],
                [ordinal as f64 + 1.0, 1.7, 5.2],
                [0.5, 0.5 + delta],
                MemberKind::Shoot,
                ordinal,
                Some(ContactEnd::Terminalward),
            ));
        }
        let frame = BuildFrame::new(f(-10.0), r(5.0));
        let layers = LayerPlan::new(0.0, vec![0.05; 400]).unwrap();
        let cfg = LoweringConfig::current(frame, &layers, 100, 100_000).unwrap();
        let view =
            FilteredLiveView::current(&forest, &[], LiveViewLimits::new(10, 1, 10_000)).unwrap();
        let joints = plan_joint_balls_filtered(
            &forest,
            &view,
            cfg.joint,
            JointStagingLimits::from_joint_config(cfg.joint),
        )
        .unwrap();
        let raft = plan_bottom_connected(
            &forest,
            &view,
            frame.plate_z_mm(),
            RaftLimits::new(10, 10, 20, 20_000),
        )
        .unwrap();
        let plan = lower_current(&forest, &view, &joints, &raft, cfg).unwrap();
        let sphere = &plan.primitives[0];
        assert_eq!(
            sphere.start_mm.map(Finite::get).map(f64::to_bits),
            p0.map(f64::to_bits)
        );
        let axis = std::array::from_fn::<_, 3, _>(|i| p1[i] - p0[i]);
        let length = axis.iter().map(|value| value * value).sum::<f64>().sqrt();
        let unit = axis.map(|value| value / length);
        let radius = 1.5 * 0.2;
        let expected_end = std::array::from_fn::<_, 3, _>(|i| p0[i] + 2.0 * radius * unit[i]);
        assert_eq!(
            sphere.end_mm.map(Finite::get).map(f64::to_bits),
            expected_end.map(f64::to_bits),
            "record poles must be authored directly, not reconstructed from rounded center"
        );

        let kinds = &plan.primitives[2..5];
        for (primitive, delta) in kinds.iter().zip([0.5e-9, 1.0e-9, 2.0e-9]) {
            let expected_taper = ((0.5_f64 + delta) - 0.5).abs() > 1e-9;
            assert_eq!(
                primitive.segment_kind == SupportSegmentKind::ContactTaper,
                expected_taper
            );
        }
    }

    #[test]
    fn shortened_embedded_plate_root_keeps_python_taper_and_contact_depth_semantics() {
        let frame = BuildFrame::new(f(0.0), r(5.0));
        let layers = LayerPlan::new(0.0, vec![0.05; 200]).unwrap();
        let config = LoweringConfig::current(frame, &layers, 10, 100_000).unwrap();
        let member = member(
            [0.0, 0.0, 3.0],
            [0.0, 0.0, 0.0],
            [0.4, 0.6],
            MemberKind::Trunk,
            12,
            Some(ContactEnd::Terminalward),
        );
        let mut forest = Forest::new();
        forest.add_member(member);
        let view =
            FilteredLiveView::current(&forest, &[], LiveViewLimits::new(1, 1, 10_000)).unwrap();
        let joints = plan_joint_balls_filtered(
            &forest,
            &view,
            config.joint,
            JointStagingLimits::from_joint_config(config.joint),
        )
        .unwrap();
        let raft = plan_bottom_connected(
            &forest,
            &view,
            frame.plate_z_mm(),
            RaftLimits::new(1, 1, 2, 100_000),
        )
        .unwrap();
        let plan = lower_current(&forest, &view, &joints, &raft, config).unwrap();
        let primitives = plan.validate(&forest, &view, frame).unwrap();
        assert!(primitives.len() >= 3);
        assert_eq!(primitives[0].segment_kind, SupportSegmentKind::ContactTaper);
        assert_eq!(
            primitives[0].contact_depth_mm,
            Some(config.contact_depth_mm)
        );
        assert_eq!(primitives[0].ring_normal, FrustumRingNormal::PlateZ);
        assert_eq!(primitives[1].segment_kind, SupportSegmentKind::RootFlare);
        assert_eq!(
            primitives.last().unwrap().segment_kind,
            SupportSegmentKind::RaftPad,
            "a single root has no bridge, only its own pad"
        );
    }

    #[test]
    fn consumed_terminal_is_the_only_mutable_live_terminal_path_into_production_lowering() {
        let (mut forest, frame, layers) = fixture();
        let stub = forest.terminal(0).unwrap();
        assert_eq!(
            forest.set_radii(
                stub,
                TerminalEndRadius::new(r(0.2)),
                RootwardEndRadius::new(r(0.45)),
            ),
            Err(crate::forest::ForestError::RegisteredTerminalMutation { terminal: 0 })
        );
        forest.consume_terminal(0, stub).unwrap();
        forest
            .set_radii(
                stub,
                TerminalEndRadius::new(r(0.2)),
                RootwardEndRadius::new(r(0.45)),
            )
            .unwrap();
        assert_eq!(
            forest.terminal_ledger(0).unwrap().state,
            TerminalLedgerState::Consumed
        );
        let config = LoweringConfig::current(frame, &layers, 100, 100_000).unwrap();
        let view = FilteredLiveView::current(&forest, &[4], LiveViewLimits::new(100, 10, 100_000))
            .unwrap();
        let joints = plan_joint_balls_filtered(
            &forest,
            &view,
            config.joint,
            JointStagingLimits::from_joint_config(config.joint),
        )
        .unwrap();
        let raft = plan_bottom_connected(
            &forest,
            &view,
            frame.plate_z_mm(),
            RaftLimits::new(100, 100, 200, 100_000),
        )
        .unwrap();
        let lowered = lower_current(&forest, &view, &joints, &raft, config).unwrap();
        assert!(lowered
            .validate(&forest, &view, frame)
            .unwrap()
            .iter()
            .any(|primitive| primitive.segment_kind == SupportSegmentKind::ContactSphere));
    }

    #[test]
    fn lowered_plan_rejects_a_different_valid_filtered_view_for_the_same_forest() {
        let (forest, frame, layers) = fixture();
        let config = LoweringConfig::current(frame, &layers, 100, 100_000).unwrap();
        let view = FilteredLiveView::current(&forest, &[4], LiveViewLimits::new(100, 10, 100_000))
            .unwrap();
        let joints = plan_joint_balls_filtered(
            &forest,
            &view,
            config.joint,
            JointStagingLimits::from_joint_config(config.joint),
        )
        .unwrap();
        let raft = plan_bottom_connected(
            &forest,
            &view,
            frame.plate_z_mm(),
            RaftLimits::new(100, 100, 200, 100_000),
        )
        .unwrap();
        let plan = lower_current(&forest, &view, &joints, &raft, config).unwrap();

        let different_view =
            FilteredLiveView::test_view_with_keys(&forest, view.keys()[1..].to_vec());
        assert!(different_view.validate(&forest).is_ok());
        assert_ne!(view.digest(), different_view.digest());
        assert_eq!(
            plan.validate(&forest, &different_view, frame).unwrap_err(),
            LoweringError::StaleAuthority,
            "the forest is unchanged; the rejected authority is only the filtered view"
        );
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
        let view = FilteredLiveView::current(&forest, &[4], LiveViewLimits::new(100, 10, 100_000))
            .unwrap();
        let joints = plan_joint_balls_filtered(
            &forest,
            &view,
            cfg.joint,
            JointStagingLimits::from_joint_config(cfg.joint),
        )
        .unwrap();
        let raft = plan_bottom_connected(
            &forest,
            &view,
            frame.plate_z_mm(),
            RaftLimits::new(100, 100, 200, 100_000),
        )
        .unwrap();
        let plan = lower_current(&forest, &view, &joints, &raft, cfg).unwrap();
        forest.add_member(member(
            [4.0, 0.0, 3.0],
            [4.0, 0.0, 0.0],
            [0.4, 0.5],
            MemberKind::Trunk,
            8,
            None,
        ));
        assert_eq!(
            plan.validate(&forest, &view, frame).unwrap_err(),
            LoweringError::StaleAuthority
        );

        let exact = lower_fixture(100, 100_000);
        let total = exact.primitives.len();
        let (forest, frame, layers) = fixture();
        let joints_cfg = LoweringConfig::current(frame, &layers, 100, 100_000).unwrap();
        let view = FilteredLiveView::current(&forest, &[4], LiveViewLimits::new(100, 10, 100_000))
            .unwrap();
        let joints = plan_joint_balls_filtered(
            &forest,
            &view,
            joints_cfg.joint,
            JointStagingLimits::from_joint_config(joints_cfg.joint),
        )
        .unwrap();
        let raft = plan_bottom_connected(
            &forest,
            &view,
            frame.plate_z_mm(),
            RaftLimits::new(100, 100, 200, 100_000),
        )
        .unwrap();
        let capped = LoweringConfig::current(frame, &layers, total - 1, 100_000).unwrap();
        assert_eq!(
            lower_current(&forest, &view, &joints, &raft, capped).unwrap_err(),
            LoweringError::PrimitiveLimit
        );
        let byte_cap = total * size_of::<SupportPrimitive>();
        let capped = LoweringConfig::current(frame, &layers, total, byte_cap - 1).unwrap();
        assert_eq!(
            lower_current(&forest, &view, &joints, &raft, capped).unwrap_err(),
            LoweringError::PrimitiveBytesLimit
        );
        let exact = LoweringConfig::current(frame, &layers, total, byte_cap).unwrap();
        assert!(lower_current(&forest, &view, &joints, &raft, exact).is_ok());
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
