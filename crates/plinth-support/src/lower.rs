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
    /// `DEFAULT_CONTACT_DEPTH_MM`, and since 2026-09-05 a FORMAT FIELD ONLY:
    /// the number written into the ChituBox type-1 taper record's contact-depth
    /// slot. It no longer decides any seat — see [`seat_tip`].
    contact_depth_mm: PositiveFinite,
    /// One addressable pixel in plane, in mm, from the registered printer
    /// profile — the in-plane half of the PR-1(a) rasterisation allowance
    /// (`contact::rasterisation_allowance_mm`).
    pixel_pitch_mm: f64,
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
            pixel_pitch_mm: crate::contact::registered_pixel_pitch_mm(constants),
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
            seat_trim: None,
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
        seat_trim: None,
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
        seat_trim: None,
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
    /// column down to the boss top, then a `RootFlare` down to the PLATE
    /// itself — not the raft top: this path emits no raft (see
    /// [`lower_sweep_arms`], step 3).
    Plate,
    /// Cured model material or another arm. The arm ends there. v1 emits no
    /// joint sphere at an arm-onto-arm junction; that is disclosed in the run
    /// report rather than hidden.
    Landed,
}

/// One straight arm as the slice sweep placed and sized it.
///
/// *UNITS TRAP*: these are RADII. The registry speaks radius; ChituBox and the
/// owner speak diameter.
#[derive(Clone, Debug, PartialEq)]
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
    /// **The area law's section at every layer**, as the down-sweep sized it:
    /// index 0 is the layer the arm lands in, the last entry is the layer under
    /// the seat. Entry `s` covers the `s`-th of `len` equal parts of the rise
    /// from `base_mm` to `seat_mm`.
    ///
    /// Until 2026-09-28 only two numbers crossed this boundary, the tip and the
    /// root, and the arm was drawn as ONE straight taper between them. Since
    /// mid-shaft landings arrived the profile is a STEP at every junction —
    /// thin above, carrying the branch's load below — and a straight line from
    /// a thin tip to a fat root 35 mm lower is still almost at the tip radius
    /// just under the junction: the trunk was printed thinner than its own
    /// ledger and thinner than the branch landing on it (field report
    /// "trunk-thinner", realised X up to 75 against 12). The lowering now draws
    /// the fewest straight spans that stay on or above this profile at every
    /// height ([`drawn_spans`]).
    ///
    /// The ROOT radius is `radius_profile_mm[0]` (the plate flare is sized from
    /// it). An empty profile draws the legacy straight column at the tip
    /// radius, and is counted in [`SweepLowering::arms_without_profile`].
    pub radius_profile_mm: Vec<f64>,
    /// OUTWARD unit normal of the model face this tip carries — the direction
    /// that points AWAY from the material, so seating moves against it.
    ///
    /// It sets `theta` in the seat law's full-cross-section term. A zero or
    /// non-finite vector means "unmeasured", and the arm's own axis stands in,
    /// which is `theta = 0` — exactly what `lower_stub` does when it uses the
    /// member direction in place of a face normal.
    pub seat_normal_mm: [f64; 3],
    /// **A point ON the model face this tip carries** — the closest point of
    /// the face whose normal is [`Self::seat_normal_mm`] — or `None` where no
    /// face was measured (fix/tip-trim, 2026-09-28).
    ///
    /// With the outward normal it is the SEAT PLANE the tip ball and the first
    /// shaft segment are trimmed to ([`crate::SeatTrim`]). It is the model's
    /// own surface, not `seat_mm`: `seat_mm` is the bottom of the first cured
    /// LAMINA, which the layer raster puts up to half a layer above or below
    /// the true face, and a trim a quarter-layer below the face would leave the
    /// contact disc floating under the model. `None` falls back to the plane
    /// through `seat_mm`, which the model difference still makes safe on the
    /// model side.
    ///
    /// It replaces `available_seat_depth_mm`, the cured thickness above the
    /// seat that set the old horizontal far-face clip. That plane lies on the
    /// model side of the seat plane, so the seat trim removes everything it
    /// removed and more.
    pub seat_face_point_mm: Option<[f64; 3]>,
    pub landing: SweepArmLanding,
    /// Whether this member's TOP is a seat INTO the model.
    ///
    /// True for every support arm, which is what the tip ball exists for. False
    /// for the two members the slenderness lane adds: a brace's top seats on
    /// another arm's shaft, and a mast's top stands in free space. Neither
    /// touches the model, so neither is given a ball -- a ball there would be a
    /// contact sphere against nothing, and on a brace it would also be a lump
    /// the owner has to cut off a member that was never meant to be removed
    /// separately.
    pub seats_into_model: bool,
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
    /// the plate with NO flare, and are counted here.
    pub arms_without_root_flare: usize,
    /// Arms whose radii or points would not cross the finite boundary, or
    /// which the emit boundary's own `is_semantically_valid` rejected.
    pub arms_dropped_invalid_geometry: usize,
    /// Tips handed to the emitter with a SEAT TRIM — their ball and the first
    /// segment of their shaft are cut to the support side of the seat face and
    /// outside the model (fix/tip-trim, 2026-09-28). Every seated tip carries
    /// one; the path hit is that this equals the tip count.
    pub tips_trimmed_to_seat: usize,
    /// Of those, tips whose seat plane is the plane through the raster seat
    /// because no carried face was measured.
    pub tips_trimmed_without_a_measured_face: usize,
    /// Every tip's CONTACT DISC radius as the trim leaves it: the circle the
    /// seat plane cuts through the emitted ball, `sqrt(R² − d²)` for a ball
    /// centre `d` from the plane, in arm order. Zero where the ball does not
    /// reach the plane. This is the contact the file actually delivers; the
    /// seat law's `tip_cut_radius_mm` is measured from the raster seat.
    pub tip_contact_disc_radius_mm: Vec<f64>,
    /// Every tip's raster seat (`seat_mm`) measured from its seat plane along
    /// the outward normal, in arm order: positive when the raster seat lies
    /// BELOW a down-facing face (on the support side), which shortens the
    /// delivered burial by that much. The layer raster's own quantisation, up
    /// to half a layer either way; published, not corrected here.
    pub tip_raster_seat_offset_mm: Vec<f64>,
    /// Tips the law seated at its own smallest term — the DERIVED
    /// rasterisation allowance, one layer in Z plus one pixel in plane —
    /// because neither the flank term nor the ball-cap term asked for more.
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
    /// Members drawn as MORE than one straight span because one straight line
    /// from the tip to the root would have dipped below the area law's profile
    /// somewhere — every trunk under a mid-shaft junction. The path-hit
    /// witness for [`drawn_spans`].
    pub arms_drawn_in_spans: usize,
    /// The `Member` frusta those splits added, chained below each taper.
    pub profile_member_frusta: usize,
    /// Arms handed to the lowering with no radius profile, drawn as a straight
    /// column at the tip radius. Zero on every sweep field; a nonzero count is
    /// a caller defect, published rather than hidden.
    pub arms_without_profile: usize,
    /// **The support resin as EMITTED**, in mm³: the solid volume of every
    /// primitive written — tip balls (less the cap beyond their seat plane),
    /// tapers, chained members and root flares — each measured from its own
    /// record, summed without removing overlaps. This is the number the file
    /// costs; the sweep's ledger volume is the SIZING figure and is published
    /// beside it, labelled as such. It does NOT subtract the part of a first
    /// segment beyond its seat plane, nor what the emitter's model difference
    /// removes: those are measured by the emitter itself
    /// (`plinth_emit::SeatTrimReport::removed_volume_milli_mm3`), so this
    /// figure is an upper bound by exactly that much.
    pub drawn_volume_mm3: f64,
    /// The part of [`Self::drawn_volume_mm3`] in tapers and chained members —
    /// the span the ledger sizes, so the two can be compared like for like.
    pub drawn_shaft_volume_mm3: f64,
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

    /// The same `a ≥ r_tip` comparison on the contact disc the trim actually
    /// leaves at the SEAT FACE ([`Self::tip_contact_disc_radius_mm`]) rather
    /// than on the seat law's own disc at the raster seat. A count, published;
    /// the two differ by the raster's quantisation of where the face is.
    #[must_use]
    pub fn tips_below_area_guarantee_at_face(&self) -> usize {
        self.tip_contact_disc_radius_mm
            .iter()
            .zip(&self.tip_radius_mm)
            .filter(|(disc, tip)| **disc < **tip * (1.0 - Self::AREA_GUARANTEE_TOLERANCE))
            .count()
    }
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
///    `DEFAULT_CONTACT_DEPTH_MM`. Where one straight taper would dip below the
///    area law's own profile (every trunk under a mid-shaft junction) the taper
///    stops at the first break and the rest of the shaft follows as chained
///    `Member` frusta on the same axis ([`drawn_spans`]).
/// 3. `RootFlare` — plate landings only, boss top down to the PLATE, `plate_z`,
///    with its full `FLARE_R_FACTOR × r_root` foot.
///
/// **The plate, not the raft top** (fix/plate-gap, 2026-09-28). The legacy
/// lowering ends its flare at `plate_z + CHITUBOX_RAFT_HEIGHT_MM` because it
/// also emits the raft the flare stands on. This path emits NO raft
/// (`docs/plans/slice-sweep-core-2026-09-02.md` §14 OR-3), and carrying the
/// raft datum over without the raft left every root 0.2 mm — four layers —
/// above the plate, curing its first layer against nothing: the owner saw the
/// bases float in ChituBox. A plate-rooted member's lowest solid IS its plate
/// contact, so it reaches `plate_z`. The same holds for the short-run branch
/// (no boss fits): its straight column also ends on the plate. The boss TOP is
/// untouched — it is the shared [`root_boss_top_at`], which still reads the
/// raft height as its datum — so only the flare's (or short column's) lower
/// end moves.
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
    lower_sweep_arms_of(arms, frame, layers, max_primitives, max_primitive_bytes, None)
}

/// [`lower_sweep_arms`], with the PR-1(a) rasterisation allowance PINNED.
///
/// TEST-ONLY, behind the `seat-minimum-probe` feature, which no production
/// build enables. It exists so a test can re-run the identical lowering with
/// the law's smallest term set to some other number — the retired 0.100 mm, say
/// — and prove that the arms the term does NOT bind emit bit-identical
/// geometry. Without it, "nothing but the allowance moved" is a claim rather
/// than a measurement.
///
/// # Errors
///
/// The same request-shaped errors [`lower_sweep_arms`] returns.
#[cfg(feature = "seat-minimum-probe")]
pub fn lower_sweep_arms_with_pinned_seat_minimum(
    arms: &[SweepArm],
    frame: BuildFrame,
    layers: &LayerPlan,
    max_primitives: usize,
    max_primitive_bytes: usize,
    seat_minimum_mm: f64,
) -> Result<SweepLowering, SweepLoweringError> {
    lower_sweep_arms_of(
        arms,
        frame,
        layers,
        max_primitives,
        max_primitive_bytes,
        Some(seat_minimum_mm),
    )
}

fn lower_sweep_arms_of(
    arms: &[SweepArm],
    frame: BuildFrame,
    layers: &LayerPlan,
    max_primitives: usize,
    max_primitive_bytes: usize,
    pinned_seat_minimum_mm: Option<f64>,
) -> Result<SweepLowering, SweepLoweringError> {
    let mut config = LoweringConfig::current(frame, layers, max_primitives, max_primitive_bytes)?;
    if let Some(minimum_mm) = pinned_seat_minimum_mm {
        // The allowance is `layer_height + pixel_pitch`, so pinning the sum is
        // pinning the pitch. Solved through the same identity the production
        // path adds, never by a second formula.
        config.pixel_pitch_mm = minimum_mm - config.layer_height_mm.get();
    }
    let config = config;
    let plate = config.frame.plate_z_mm().get();
    let mut out = SweepLowering {
        primitives: Vec::with_capacity(arms.len().saturating_mul(3)),
        tip_seat_depth_mm: Vec::with_capacity(arms.len()),
        tip_cut_radius_mm: Vec::with_capacity(arms.len()),
        tip_radius_mm: Vec::with_capacity(arms.len()),
        tip_contact_disc_radius_mm: Vec::with_capacity(arms.len()),
        tip_raster_seat_offset_mm: Vec::with_capacity(arms.len()),
        ..SweepLowering::default()
    };
    for arm in arms {
        let lowered = lower_one_sweep_arm(arm, config, plate, &mut out.primitives);
        match lowered.outcome {
            ArmOutcome::Built => {}
            ArmOutcome::BuiltWithoutFlare => out.arms_without_root_flare += 1,
            ArmOutcome::ZeroLength => out.arms_dropped_zero_length += 1,
            ArmOutcome::InvalidGeometry => out.arms_dropped_invalid_geometry += 1,
        }
        if lowered.spans > 1 {
            out.arms_drawn_in_spans += 1;
            out.profile_member_frusta += lowered.spans - 1;
        }
        out.arms_without_profile += usize::from(lowered.without_profile);
        if let Some(seat) = lowered.seat {
            out.tip_seat_depth_mm.push(seat.depth_mm);
            out.tip_cut_radius_mm.push(seat.cut_radius_mm);
            out.tip_radius_mm.push(seat.tip_radius_mm);
            out.tip_contact_disc_radius_mm
                .push(seat.contact_disc_radius_mm);
            out.tip_raster_seat_offset_mm
                .push(seat.raster_seat_offset_mm);
            out.tips_trimmed_to_seat += 1;
            out.tips_trimmed_without_a_measured_face += usize::from(!seat.face_measured);
            out.tips_at_law_minimum += usize::from(seat.at_law_minimum);
        }
        if out.primitives.len() > max_primitives {
            return Err(SweepLoweringError::PrimitiveLimit {
                limit: max_primitives,
            });
        }
    }
    // THE RESIN AS EMITTED, measured once over the finished primitive batch
    // from each record's own endpoints and radii, never from the ledger.
    for primitive in &out.primitives {
        let volume = emitted_solid_volume_mm3(primitive);
        out.drawn_volume_mm3 += volume;
        if matches!(
            primitive.segment_kind,
            SupportSegmentKind::ContactTaper | SupportSegmentKind::Member
        ) {
            out.drawn_shaft_volume_mm3 += volume;
        }
    }
    Ok(out)
}

/// The solid one emitted record encloses, in mm³, from its own fields.
///
/// A frustum with rings normal to its own axis is `π·L/3·(r₀² + r₀r₁ + r₁²)`
/// over its axial length `L`. A frustum with HORIZONTAL rings (a plate-root
/// column, [`FrustumRingNormal::PlateZ`]) has a circle of the ring radius at
/// every height, so by Cavalieri the same formula holds over its RISE, not its
/// slant. A tip ball is `4/3·π·R³` less the cap beyond its seat plane. A raft
/// record is not a solid of revolution this pass knows how to measure, and the
/// sweep lowering emits none; it counts zero here.
fn emitted_solid_volume_mm3(primitive: &SupportPrimitive) -> f64 {
    use core::f64::consts::PI;
    let r0 = primitive.start_radius_mm.get();
    let r1 = primitive.end_radius_mm.get();
    match primitive.segment_kind {
        SupportSegmentKind::ContactTaper
        | SupportSegmentKind::Member
        | SupportSegmentKind::RootFlare => {
            let length = match primitive.ring_normal {
                FrustumRingNormal::SegmentAxis => distance(primitive.start_mm, primitive.end_mm),
                FrustumRingNormal::PlateZ => {
                    (primitive.start_mm[2].get() - primitive.end_mm[2].get()).abs()
                }
            };
            PI * length / 3.0 * (r0.powi(2) + r0 * r1 + r1.powi(2))
        }
        SupportSegmentKind::ContactSphere | SupportSegmentKind::JointSphere => {
            let whole = 4.0 / 3.0 * PI * r0 * r0 * r0;
            let centre: [f64; 3] = std::array::from_fn(|index| {
                0.5 * (primitive.start_mm[index].get() + primitive.end_mm[index].get())
            });
            // The cap on the MODEL side of the seat plane: a ball whose centre
            // is `d` on the support side loses a cap of height `R − d`.
            let cap = primitive.seat_trim.map_or(0.0, |trim| {
                let height = (r0 - trim.support_side_distance_mm(centre)).clamp(0.0, 2.0 * r0);
                PI * height * height * (3.0 * r0 - height) / 3.0
            });
            whole - cap
        }
        SupportSegmentKind::RaftPad | SupportSegmentKind::RaftBridge => 0.0,
    }
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
    /// The seat trim's plane: the OUTWARD normal the seat law used (the
    /// carried face's, or straight down where none was measured) through the
    /// carried face's own point, or through `seat_mm` where no face was
    /// measured.
    trim: Option<crate::SeatTrim>,
    face_measured: bool,
    /// The contact disc the trim leaves: the ball's section in the seat plane.
    /// Filled by the caller once the ball exists.
    contact_disc_radius_mm: f64,
    /// `seat_mm` measured from the seat plane along the outward normal.
    raster_seat_offset_mm: f64,
    /// The law's smallest term — the registered contact depth — was the
    /// binding one.
    at_law_minimum: bool,
}

struct ArmLowering {
    outcome: ArmOutcome,
    seat: Option<TipSeat>,
    /// Straight spans the shaft was drawn in: 1 is the single taper.
    spans: usize,
    /// The arm arrived with no radius profile and was drawn at its tip radius.
    without_profile: bool,
}

impl ArmLowering {
    const fn refused(outcome: ArmOutcome) -> Self {
        Self {
            outcome,
            seat: None,
            spans: 0,
            without_profile: false,
        }
    }
}

/// One straight span of a drawn member, between two heights on its axis.
#[derive(Clone, Copy, Debug, PartialEq)]
struct DrawnSpan {
    top_z_mm: f64,
    bottom_z_mm: f64,
    top_radius_mm: f64,
    bottom_radius_mm: f64,
}

/// Float noise around an equality the construction designs for — a chord that
/// ends exactly on a ledger value — and nothing more. It is not a physical
/// allowance: a span is split whenever it dips below the ledger by more than
/// one part in a billion.
const PROFILE_RELATIVE_SLACK: f64 = 1.0e-9;

/// **THE DRAWN PROFILE OF ONE MEMBER** (2026-09-28): the fewest straight spans,
/// from `from_z` (the top of the drawn shaft: the tip ball's centre, or the
/// seat of a member with no ball) down to `to_z` (where the shaft ends: the
/// landing, the boss top, or the plate for a plate arm too short for a boss),
/// whose section is at least the area
/// law's own section at EVERY height.
///
/// **The rule** is the area law applied to the emitted geometry rather than
/// only to the ledger: `π·r_drawn(z)² ≥ carried(z) / X_ALLOWED`, which is
/// `r_drawn(z) ≥ profile(z)`. No new number enters — the profile is the
/// down-sweep's, and the construction only decides where the straight spans
/// break.
///
/// **The construction.** The profile is a step function over equal slots
/// between `base_z` and `seat_z`. Cut the drawn range at every slot boundary
/// inside it; each piece has one ledger radius. A candidate span runs between
/// two cuts and is the straight chord from the ledger radius just below its
/// top to the ledger radius just above its bottom (the member's own `start`
/// and `end` radii at the two ends of the whole shaft, where those are wider).
/// A linear chord over a piece with a constant requirement is lowest at one of
/// the piece's ends, so checking every cut inside the span is exhaustive. If
/// the chord dips below the ledger anywhere, the span is split at the cut where
/// it dips furthest and both halves are tried again. One vector pass per span;
/// it terminates because a one-piece span is its own ledger value and cannot
/// dip.
///
/// Under a flat underside the ledger is constant between junctions, so the
/// result is one span per junction-free run and a shoulder at each junction:
/// the span above ends at the thin radius, the span below starts at the fat
/// one. A shoulder faces UP and needs no support. Where the ledger does not
/// step, nothing splits and the member is the single taper it always was.
///
/// This is the fewest spans for THIS chord rule, found greedily; it is not a
/// claim of a global minimum over every possible choice of end radii.
#[expect(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "a slot index bounded by the profile length is exact in f64, and the f64 -> usize \
              cast is reached only for a floored, positive index clamped to the profile length"
)]
fn drawn_spans(
    profile: &[f64],
    base_z: f64,
    seat_z: f64,
    from_z: f64,
    to_z: f64,
    start_radius: f64,
    end_radius: f64,
) -> Vec<DrawnSpan> {
    let single = || {
        vec![DrawnSpan {
            top_z_mm: from_z,
            bottom_z_mm: to_z,
            top_radius_mm: start_radius,
            bottom_radius_mm: end_radius,
        }]
    };
    let slots = profile.len();
    let rise = seat_z - base_z;
    if slots == 0 || !rise.is_finite() || rise <= 0.0 || from_z <= to_z {
        return single();
    }
    let slot_height = rise / slots as f64;
    let slot_of = |z: f64| -> usize {
        let index = ((z - base_z) / slot_height).floor();
        if index <= 0.0 {
            0
        } else {
            (index as usize).min(slots - 1)
        }
    };
    // The cuts, top-down: the shaft's own ends and every slot boundary
    // strictly inside it (one closer than the zero-length tolerance to an end
    // would make a degenerate frustum, and adds nothing the end does not).
    let mut cuts: Vec<f64> = Vec::with_capacity(slots + 2);
    cuts.push(from_z);
    let first = slot_of(from_z);
    for boundary in (1..=first).rev() {
        let z = base_z + boundary as f64 * slot_height;
        if z < from_z - ZERO_LENGTH_MM && z > to_z + ZERO_LENGTH_MM {
            cuts.push(z);
        }
    }
    cuts.push(to_z);
    let pieces = cuts.len() - 1;
    // The ledger requirement on each piece, read at its mid-height.
    let need: Vec<f64> = cuts
        .windows(2)
        .map(|pair| profile[slot_of(0.5 * (pair[0] + pair[1]))])
        .collect();
    let radius_at_top = |cut: usize| {
        if cut == 0 {
            start_radius.max(need[0])
        } else {
            need[cut]
        }
    };
    let radius_at_bottom = |cut: usize| {
        if cut == pieces {
            end_radius.max(need[pieces - 1])
        } else {
            need[cut - 1]
        }
    };
    let mut spans = Vec::new();
    // Depth-first, top half first, so the spans come out top-down.
    let mut pending = vec![(0_usize, pieces)];
    while let Some((top, bottom)) = pending.pop() {
        let top_radius = radius_at_top(top);
        let bottom_radius = radius_at_bottom(bottom);
        let drop = cuts[top] - cuts[bottom];
        let mut worst: Option<(usize, f64)> = None;
        for cut in top + 1..bottom {
            let chord = top_radius + (bottom_radius - top_radius) * (cuts[top] - cuts[cut]) / drop;
            let required = need[cut - 1].max(need[cut]);
            let short = required - chord;
            if short > required * PROFILE_RELATIVE_SLACK
                && worst.is_none_or(|(_, deepest)| short > deepest)
            {
                worst = Some((cut, short));
            }
        }
        match worst {
            Some((cut, _)) => {
                pending.push((cut, bottom));
                pending.push((top, cut));
            }
            None => spans.push(DrawnSpan {
                top_z_mm: cuts[top],
                bottom_z_mm: cuts[bottom],
                top_radius_mm: top_radius,
                bottom_radius_mm: bottom_radius,
            }),
        }
    }
    spans
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
/// **A GRAZING seat cannot get there along its own axis**, and until
/// 2026-09-05 it simply stopped short: the travel is capped at the ball's own
/// diameter (a nearly tangential axis would otherwise ask to slide the ball an
/// unbounded distance along the surface for almost no depth), so the delivered
/// depth fell to `2·R·|cos θ|` and with it the delivered contact area. That was
/// the whole of the residual `a ≥ r_tip` shortfall on the Cylinder and the
/// Morel base — not a thin wall. Under the owner's ruling the tip goes as deep
/// as the AREA needs, so whatever the axis cannot deliver is now made up along
/// the face's own inward normal, exactly the move `contact::seat_endpoint`
/// makes for every legacy seated family. An arm at `|cos θ| ≥ 0.5` never
/// reaches this (its axial travel is inside the diameter cap by construction),
/// so no ordinary seat moves for this reason.
///
/// **The geometry is TRIMMED, never the seat retracted** (owner ruling,
/// 2026-09-05; rule of 2026-09-28). The tip goes to the FULL depth the area
/// term asks for, whatever the skin above it is. What bounds the emitted
/// GEOMETRY is the seat trim returned here: the seat face's own plane, which
/// the emitter cuts the ball and the first shaft segment to, together with the
/// model solid itself. Land-at-strike holds — nothing stands inside the model
/// or past its far side — by cutting the geometry rather than by starving the
/// contact. (Until 2026-09-28 a horizontal far-face plane did this job for the
/// ball's top alone.)
fn seat_tip(
    arm: &SweepArm,
    unit: [f64; 3],
    tip_radius_mm: f64,
    config: LoweringConfig,
) -> ([f64; 3], TipSeat) {
    // AN UNMEASURED NORMAL FALLS BACK TO STRAIGHT DOWN, NOT TO THE ARM'S OWN
    // AXIS — corrected 2026-09-06.
    //
    // The axis fallback said "theta = 0" and so asked the seat law for a depth
    // measured along the ARM. While every arm was vertical the two directions
    // were the same vector and the substitution was exact. A LEANED arm
    // delivers only `depth * cos(tilt)` of DOWNWARD burial for the same axial
    // travel, and the rasterisation allowance the law's smallest term is made
    // of — one layer in Z plus one pixel in plane — is a statement about Z. On
    // the flat 50 mm disc arm 196 leaned 57 degrees off vertical, its nearest
    // mesh face was the vertical rim wall (so the normal went unmeasured, see
    // `tip_landing::tip_seat_normals`), and its pole ended 0.0585 mm inside a
    // model the law had asked for 0.0690 mm of: a starved contact reported as
    // a full one. Falling back to the vertical restores the identity between
    // "the depth the law asked for" and "the depth the model actually gets",
    // and it is a no-op for every vertical arm, whose axis IS this vector.
    const STRAIGHT_DOWN: [f64; 3] = [0.0, 0.0, -1.0];
    // The carried face is down-facing and the arm runs seat -> base, so its
    // OUTWARD normal points the same way as `unit`. A caller handing in the
    // inward normal would otherwise pass unnoticed through `abs()` below and
    // then drive the grazing completion OUT of the model at full magnitude
    // (reviewer finding, 2026-09-05), so the sign is fixed here rather than
    // trusted.
    let measured = geom_kernel::vec3::normalize(arm.seat_normal_mm).unwrap_or(STRAIGHT_DOWN);
    let outward = if geom_kernel::vec3::dot(unit, measured) < 0.0 {
        measured.map(|component| -component)
    } else {
        measured
    };
    let abs_cos = geom_kernel::vec3::dot(unit, outward).min(1.0);
    // PR-1(a) DERIVED, not typed: one layer in Z plus one pixel in plane, the
    // only separation the printer cannot place two solids inside of. The
    // registered `DEFAULT_CONTACT_DEPTH_MM` used to stand here and was retired
    // from this role by the owner's ruling of 2026-09-05 ("'contact depth'
    // 0.1 mm is a fake number ... the important thing is contact surface
    // area"); it survives only as the ChituBox taper record's own field.
    let allowance_mm = crate::contact::rasterisation_allowance_mm(
        config.layer_height_mm.get(),
        config.pixel_pitch_mm,
    );
    let law_depth_mm = crate::contact::seat_depth_of(
        config.pixel_pitch_mm,
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
    // NOTHING BOUNDS THE TRAVEL BUT THE LAW ITSELF. Until 2026-09-05 the
    // available cured thickness was a third `min` here, which RETRACTED the
    // tip out of a thin skin and starved the contact the area law had asked
    // for; the owner's ruling of that date replaced retraction with cutting
    // the emitted geometry, which is now the seat trim built below.
    let axial_mm = axial_for_target_mm.min(2.0 * equator_mm).max(0.0);
    let axial_mm = if axial_mm.is_finite() { axial_mm } else { 0.0 };
    // WHATEVER THE AXIS COULD NOT DELIVER, the normal does. Moving against the
    // outward normal adds depth one for one, so this lands the pole at exactly
    // the depth the area law asked for and the shortfall closes. It is zero for
    // every arm whose axial travel already reached the target, which is every
    // arm at `|cos| >= 0.5`.
    let axial_depth_mm = axial_mm * abs_cos;
    let normal_completion_mm = (target_depth_mm - axial_depth_mm).max(0.0);
    let depth_mm = axial_depth_mm + normal_completion_mm;
    let seated = std::array::from_fn(|index| {
        arm.seat_mm[index] - unit[index] * axial_mm - outward[index] * normal_completion_mm
    });
    // THE DELIVERED MINIMAL SEPARATING CUT at the seat plane — the disc the
    // model surface actually takes through the emitted ball, which is what the
    // area law has to be evaluated on. It is not `r_tip`, and it is measured
    // here rather than assumed anywhere.
    let cut_radius_mm =
        crate::contact::ball_cut_radius_mm(tip_radius_mm, depth_mm, config.tip_ball_factor.get());
    // THE SEAT PLANE (fix/tip-trim, 2026-09-28): the carried face's own point
    // with the very normal the law just used, so the plane the ball is cut to
    // and the face the seat was measured against are one face, never two. The
    // raster seat stands in only where no face was measured.
    let face_measured = arm.seat_face_point_mm.is_some()
        && geom_kernel::vec3::normalize(arm.seat_normal_mm).is_some();
    let plane_point = arm
        .seat_face_point_mm
        .filter(|_| face_measured)
        .unwrap_or(arm.seat_mm);
    let trim = crate::SeatTrim::new(plane_point, outward).ok();
    let raster_seat_offset_mm = trim.map_or(0.0, |trim| trim.support_side_distance_mm(arm.seat_mm));
    (
        seated,
        TipSeat {
            depth_mm,
            cut_radius_mm,
            tip_radius_mm,
            trim,
            face_measured,
            contact_disc_radius_mm: 0.0,
            raster_seat_offset_mm,
            // `law_depth_mm` is a `max` over three terms of which the
            // rasterisation allowance is one, so it can never be below the
            // allowance; `<=` is equality.
            at_law_minimum: law_depth_mm <= allowance_mm,
        },
    )
}

/// One arm's seated tip ball: the seated pole, the ball centre, and the seat
/// record with its contact disc filled in. The ONE derivation both the
/// lowering and [`tip_contact_discs`] read, so the disc the sweep's footprint
/// rule grades is the disc the file gets.
fn seated_ball(
    arm: &SweepArm,
    unit: [f64; 3],
    tip_radius_mm: f64,
    ball_radius_mm: f64,
    config: LoweringConfig,
) -> ([f64; 3], [f64; 3], TipSeat) {
    // THE SEAT. Until 2026-09-05 the ball's pole sat exactly on `arm.seat_mm`
    // — the BOTTOM plane of the first cured model lamina — so the tip touched
    // the model at a single point and carried an area of zero. It is now driven
    // INTO the model by the seat law every other seating family in this crate
    // reads (`contact::seat_depth_of`).
    let (seated, mut tip_seat) = seat_tip(arm, unit, tip_radius_mm, config);
    let center: [f64; 3] =
        std::array::from_fn(|index| seated[index] + ball_radius_mm * unit[index]);
    // TRIM, NEVER RETRACT (owner ruling 2026-09-05, rule of 2026-09-28). The
    // ball is at the full depth the area law asked for; the emitter keeps only
    // its part on the support side of the seat face and outside the model
    // (`SeatTrim`). What that leaves in the seat plane is the contact disc,
    // `sqrt(R² − d²)` for a centre `d` on the support side, measured here from
    // the SAME plane the emitter cuts to.
    //
    // THE FACE MUST BE ONE THE BALL MEETS. The carried face is the nearest
    // down-facing mesh face to the tip's landing point; on a sculpted surface
    // that facet's PLANE can pass clear of the ball (a small facet tilted away
    // from the patch the ball actually seats on). A plane the ball does not
    // cross is not the face this tip is seated against, and trimming to it
    // would keep nothing or everything. Such a tip falls back to the raster
    // seat's own plane with the same normal, and is counted as unmeasured.
    if let Some(trim) = tip_seat.trim {
        if trim.support_side_distance_mm(center).abs() >= ball_radius_mm && tip_seat.face_measured {
            tip_seat.trim = crate::SeatTrim::new(arm.seat_mm, trim.outward_normal()).ok();
            tip_seat.face_measured = false;
            tip_seat.raster_seat_offset_mm = 0.0;
        }
    }
    if let Some(trim) = tip_seat.trim {
        let d = trim.support_side_distance_mm(center);
        tip_seat.contact_disc_radius_mm = (ball_radius_mm * ball_radius_mm - d * d).max(0.0).sqrt();
    }
    (seated, center, tip_seat)
}

/// The contact disc a seated tip's trim leaves in its seat plane.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TipContactDisc {
    /// The disc's centre: the ball centre projected onto the seat plane.
    pub centre_mm: [f64; 3],
    /// The seat face's OUTWARD unit normal (the plane's normal).
    pub outward_normal: [f64; 3],
    /// The disc's radius, `sqrt(R² − d²)`; zero when the ball misses the plane.
    pub radius_mm: f64,
    /// The emitted ball's own radius `R`.
    pub ball_radius_mm: f64,
}

/// **The contact disc every seated arm's tip will deliver**, computed by the
/// lowering's own seat derivation ([`seated_ball`]) without lowering anything
/// (fix/tip-trim, 2026-09-28). `None` for an arm that seats into nothing (a
/// brace or mast), is zero-length, or carries no seat plane.
///
/// This is what the sweep's footprint rule grades against the layer rasters:
/// the disc it tests is, by construction, the disc the file gets.
///
/// # Errors
///
/// Only about the request, as [`lower_sweep_arms`]: an empty or variable
/// layer plan, or an unreadable registry.
pub fn tip_contact_discs(
    arms: &[SweepArm],
    layers: &LayerPlan,
) -> Result<Vec<Option<TipContactDisc>>, SweepLoweringError> {
    let frame = BuildFrame::new(
        Finite::new(layers.z0_mm()).map_err(|_| SweepLoweringError::MissingLayerHeight)?,
        PositiveFinite::new(1.0).map_err(|_| SweepLoweringError::InvalidLimits)?,
    );
    let config = LoweringConfig::current(frame, layers, 1, 1)?;
    Ok(arms
        .iter()
        .map(|arm| {
            if !arm.seats_into_model {
                return None;
            }
            let delta = geom_kernel::vec3::sub(arm.base_mm, arm.seat_mm);
            let length = geom_kernel::vec3::norm(delta);
            if !length.is_finite()
                || length <= ZERO_LENGTH_MM
                || !arm.tip_radius_mm.is_finite()
                || arm.tip_radius_mm <= 0.0
            {
                return None;
            }
            let unit = delta.map(|value| value / length);
            let ball_radius_mm = config.tip_ball_factor.get() * arm.tip_radius_mm;
            let (_, center, tip_seat) =
                seated_ball(arm, unit, arm.tip_radius_mm, ball_radius_mm, config);
            let trim = tip_seat.trim?;
            let normal = trim.outward_normal();
            let d = trim.support_side_distance_mm(center);
            Some(TipContactDisc {
                centre_mm: std::array::from_fn(|index| center[index] - d * normal[index]),
                outward_normal: normal,
                radius_mm: tip_seat.contact_disc_radius_mm,
                ball_radius_mm,
            })
        })
        .collect())
}

fn lower_one_sweep_arm(
    arm: &SweepArm,
    config: LoweringConfig,
    plate_z: f64,
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
    // The ledger's ROOT section — where the load arrives — never narrower than
    // the tip. It is what the plate flare is sized from.
    let ledger_root = arm
        .radius_profile_mm
        .first()
        .copied()
        .unwrap_or(arm.tip_radius_mm);
    let Ok(root_radius) = positive(ledger_root.max(arm.tip_radius_mm)) else {
        return ArmLowering::refused(ArmOutcome::InvalidGeometry);
    };
    let Ok(ball_radius) = positive(config.tip_ball_factor.get() * tip_radius.get()) else {
        return ArmLowering::refused(ArmOutcome::InvalidGeometry);
    };
    // A MEMBER THAT NEVER TOUCHES THE MODEL GETS NO BALL (2026-09-06). A
    // brace's top seats on another arm's shaft and a mast's top stands in free
    // space; there is no model surface for a contact sphere to bear on, and
    // emitting one would put a lump on a member that is not a support tip. The
    // taper then starts at the seat point itself. Every support arm takes the
    // branch below unchanged, so this is the identity on every field the
    // slenderness lane did not touch.
    let mut staged: Vec<SupportPrimitive> = Vec::with_capacity(3);
    let (seated_seat, center, seat) = if arm.seats_into_model {
        // THE SEAT and its trim: `seated_ball`, the one derivation.
        let (seated_seat, center_raw, tip_seat) =
            seated_ball(arm, unit, tip_radius.get(), ball_radius.get(), config);
        let (Ok(center), Ok(unit_finite)) = (finite3(center_raw), finite3(unit)) else {
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
        staged.push(SupportPrimitive {
            segment_kind: SupportSegmentKind::ContactSphere,
            start_mm: sphere_start,
            end_mm: sphere_end,
            start_radius_mm: ball_radius,
            end_radius_mm: ball_radius,
            contact_depth_mm: None,
            ring_normal: FrustumRingNormal::SegmentAxis,
            sphere_geometry: Some(geometry),
            seat_trim: tip_seat.trim,
        });

        (seated_seat, center, Some(tip_seat))
    } else {
        let Ok(center) = finite3(arm.seat_mm) else {
            return ArmLowering::refused(ArmOutcome::InvalidGeometry);
        };
        (arm.seat_mm, center, None)
    };
    let Ok(base) = finite3(arm.base_mm) else {
        return ArmLowering::refused(ArmOutcome::InvalidGeometry);
    };
    // Every shaft below goes through `push_drawn_shaft`, which draws the taper
    // and any chained members the ledger's profile needs; the plate-root chain
    // around it is untouched.
    let shaft = |top: [Finite; 3],
                 bottom: [Finite; 3],
                 ring_normal: FrustumRingNormal,
                 staged: &mut Vec<SupportPrimitive>| {
        push_drawn_shaft(
            arm,
            DrawnShaft {
                top,
                bottom,
                tip_radius,
                root_radius,
                ring_normal,
            },
            config,
            staged,
        )
    };
    let spans: usize;
    let outcome = match arm.landing {
        SweepArmLanding::Landed => {
            if distance(center, base) <= ZERO_LENGTH_MM {
                return ArmLowering::refused(ArmOutcome::ZeroLength);
            }
            let Some(drawn) = shaft(center, base, FrustumRingNormal::SegmentAxis, &mut staged)
            else {
                return ArmLowering::refused(ArmOutcome::InvalidGeometry);
            };
            spans = drawn;
            ArmOutcome::Built
        }
        SweepArmLanding::Plate => {
            // THE FOOT IS ON THE PLATE (fix/plate-gap, 2026-09-28). This path
            // emits no raft, so there is no raft top to stop at: a plate root's
            // lowest solid must be `plate_z` itself, or its first layer cures
            // against the vat film instead of the plate.
            let Ok(plate_z_finite) = finite(plate_z) else {
                return ArmLowering::refused(ArmOutcome::InvalidGeometry);
            };
            let plate_point = [base[0], base[1], plate_z_finite];
            // The SEATED terminal z, exactly as the legacy path reads
            // `member.p0_mm()[2]` — which `initial.rs` already minted embedded.
            let boss_top =
                root_boss_top_at(seated_seat[2], [base[0], base[1]], config).and_then(Result::ok);
            match boss_top {
                Some(top)
                    if distance(center, top) > ZERO_LENGTH_MM
                        && distance(top, plate_point) > ZERO_LENGTH_MM =>
                {
                    let Ok(flare_radius) = positive(root_radius.get() * config.flare_factor.get())
                    else {
                        return ArmLowering::refused(ArmOutcome::InvalidGeometry);
                    };
                    let Some(drawn) = shaft(center, top, FrustumRingNormal::PlateZ, &mut staged)
                    else {
                        return ArmLowering::refused(ArmOutcome::InvalidGeometry);
                    };
                    spans = drawn;
                    staged.push(frustum(
                        SupportSegmentKind::RootFlare,
                        top,
                        plate_point,
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
                    // plate, so no flare fits. Emit the straight column down
                    // to the plate and WARN: refusing here would destroy the
                    // evidence.
                    if distance(center, plate_point) <= ZERO_LENGTH_MM {
                        return ArmLowering::refused(ArmOutcome::ZeroLength);
                    }
                    let Some(drawn) =
                        shaft(center, plate_point, FrustumRingNormal::PlateZ, &mut staged)
                    else {
                        return ArmLowering::refused(ArmOutcome::InvalidGeometry);
                    };
                    spans = drawn;
                    ArmOutcome::BuiltWithoutFlare
                }
            }
        }
    };
    // THE FIRST SHAFT SEGMENT IS TRIMMED TO THE SAME SEAT (fix/tip-trim,
    // 2026-09-28). The taper starts at the ball centre, and on a leaned arm its
    // start cap is tilted with the axis, so it stands through the seat plane
    // and past the face's edge exactly where the ball does ("a chunk of post",
    // tip-clip.md). The same plane and the same model difference apply.
    if let Some(trim) = seat.as_ref().and_then(|seat| seat.trim) {
        if let Some(taper) = staged
            .iter_mut()
            .find(|primitive| primitive.segment_kind == SupportSegmentKind::ContactTaper)
        {
            taper.seat_trim = Some(trim);
        }
    }
    // The emit boundary's own contract, asked HERE, so one bad arm is dropped
    // and counted rather than failing the whole field at
    // `SupportedModel::from_parts` with nothing said about which arm did it.
    if staged.iter().any(|primitive| !primitive.is_semantically_valid()) {
        return ArmLowering::refused(ArmOutcome::InvalidGeometry);
    }
    out.append(&mut staged);
    ArmLowering {
        outcome,
        seat,
        spans,
        without_profile: arm.radius_profile_mm.is_empty(),
    }
}

/// The ends and end radii of one shaft to draw.
struct DrawnShaft {
    top: [Finite; 3],
    bottom: [Finite; 3],
    tip_radius: PositiveFinite,
    root_radius: PositiveFinite,
    ring_normal: FrustumRingNormal,
}

/// Draw one member's shaft from `top` (the ball centre, or the seat of a member
/// with no ball) straight down to `bottom`, as the spans [`drawn_spans`]
/// chooses: the first span is the member's `ContactTaper`, carrying the
/// registered contact depth exactly as before; every further span is a
/// `Member` frustum chained end to end on the same straight axis, sharing its
/// end point bit for bit with the span above. Returns the span count, or
/// `None` when a span will not cross the finite boundary.
fn push_drawn_shaft(
    arm: &SweepArm,
    shaft: DrawnShaft,
    config: LoweringConfig,
    staged: &mut Vec<SupportPrimitive>,
) -> Option<usize> {
    let top_z = shaft.top[2].get();
    let bottom_z = shaft.bottom[2].get();
    let spans = drawn_spans(
        &arm.radius_profile_mm,
        arm.base_mm[2],
        arm.seat_mm[2],
        top_z,
        bottom_z,
        shaft.tip_radius.get(),
        shaft.root_radius.get(),
    );
    // A point on the straight axis at height `z`, by the same parameter the
    // spans were cut at, so a shared end is the same number on both spans.
    let on_axis = |z: f64| -> Option<[Finite; 3]> {
        let t = (top_z - z) / (top_z - bottom_z);
        finite3(std::array::from_fn(|index| {
            let from = shaft.top[index].get();
            from + (shaft.bottom[index].get() - from) * t
        }))
        .ok()
    };
    let last = spans.len().saturating_sub(1);
    for (index, span) in spans.iter().enumerate() {
        let start = if index == 0 {
            shaft.top
        } else {
            on_axis(span.top_z_mm)?
        };
        let end = if index == last {
            shaft.bottom
        } else {
            on_axis(span.bottom_z_mm)?
        };
        let start_radius = positive(span.top_radius_mm).ok()?;
        let end_radius = positive(span.bottom_radius_mm).ok()?;
        let (kind, depth) = if index == 0 {
            (
                SupportSegmentKind::ContactTaper,
                Some(config.contact_depth_mm),
            )
        } else {
            (SupportSegmentKind::Member, None)
        };
        staged.push(frustum(
            kind,
            start,
            end,
            start_radius,
            end_radius,
            depth,
            shaft.ring_normal,
        ));
    }
    Some(spans.len())
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
