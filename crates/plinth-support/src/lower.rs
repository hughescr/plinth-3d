//! Current-Python-parity lowering from the final live forest to semantic emit primitives.
//!
//! This is deliberately a pure, serial lowering pass: it performs no geometry queries and
//! preserves the frozen member/ball/raft record order. Mesh construction and ChituBox record
//! identities remain owned by `plinth-emit`.

use std::mem::size_of;

use plinth_core::{
    m0_constants, ContentDigest, Finite, LayerPlan, M0Constants, PositiveFinite, TipContactProfiles,
};
use thiserror::Error;

use crate::{
    forest::{Forest, Member, MemberKey, MemberKind},
    joints::{FilteredLiveView, JointConfig, JointError, JointPlan},
    raft::RaftPlan,
    BuildFrame, FrustumRingNormal, SphereGeometry, SupportPrimitive, SupportSegmentKind,
};

const ZERO_LENGTH_MM: f64 = 1e-9;

/// **Placeholder, format parity — how far a sweep foot reaches INTO its raft
/// pad** (fix/sweep-raft, 2026-09-29).
///
/// MEASURED, not chosen: every ChituBox-authored file we hold ends its foot
/// 0.002 mm below the top of the pad it stands on (`raft-diagnosis.md` §3.2:
/// `two-supports` pad top −20.1205, flare end −20.1225;
/// `cylinder-supported-bottom-connected` pad top −17.8, every flare and bare
/// column end −17.802). The overlap makes the foot and the pad SHARE volume,
/// so the slicer's union has no coincident face at the raft top; it is
/// 1/25 of one 0.05 mm layer, far below anything the printer resolves, and
/// far above the binary32 resolution of a coordinate (~1e-6 mm at 20 mm).
///
/// It decides no outcome and prunes nothing: it moves one end point by a
/// sub-pixel amount to match the native bytes. The end-to-end test measures
/// the native value from the committed fixture rather than trusting this one
/// (`crates/plinth-pipeline/tests/it/plate_contact.rs`).
pub const NATIVE_FOOT_PAD_OVERLAP_MM: f64 = 0.002;

/// **The root boss every sweep plate root stands on**: the owner's ChituBox
/// boss (Craig ChituBox profile export 2026-09-30, fields
/// `support_general_para.heightofboss` = 1.0 mm and `.diameterofbossbottom`
/// = 2.2 mm, all four profiles; `CHITUBOX_BOSS_HEIGHT_MM`,
/// `CHITUBOX_BOSS_BOTTOM_DIAMETER_MM`).
///
/// A cone from the shaft's own drawn root section at the boss top down to the
/// boss foot on the raft pad. The foot is the owner's fixed 1.1 mm radius,
/// **grown to the root where the load-sized root is wider** — the boss is
/// grown, the root is never capped (owner ruling 2026-09-29). It replaces the
/// legacy flare (`FLARE_R_FACTOR` × root over `FLARE_LEN_MM`) on the sweep
/// path, and it is the ONE rule for the foot: the lowering draws it, and the
/// sweep's congestion census, mast and thicken clearance and lip-seat root
/// clearance all read [`Self::foot_radius_mm`], so the disc a decision is
/// taken against is the disc the file lays down.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RootBoss {
    height_mm: f64,
    native_foot_radius_mm: f64,
}

impl RootBoss {
    /// The registered boss.
    ///
    /// # Errors
    ///
    /// The registry cannot be read, or holds a non-positive boss.
    pub fn registered() -> Result<Self, String> {
        let constants = m0_constants().map_err(ToString::to_string)?;
        Self::from_constants(constants)
    }

    fn from_constants(constants: &M0Constants) -> Result<Self, String> {
        let evidence = constants.evidence();
        let height_mm = *evidence.chitubox_boss_height_mm().value();
        let native_foot_radius_mm = *evidence.chitubox_boss_bottom_diameter_mm().value() / 2.0;
        if height_mm.is_finite()
            && height_mm > 0.0
            && native_foot_radius_mm.is_finite()
            && native_foot_radius_mm > 0.0
        {
            Ok(Self {
                height_mm,
                native_foot_radius_mm,
            })
        } else {
            Err(format!(
                "the registered root boss is not a solid: height {height_mm} mm, foot radius \
                 {native_foot_radius_mm} mm"
            ))
        }
    }

    /// The boss's height above the raft top, where the run allows it.
    #[must_use]
    pub const fn height_mm(self) -> f64 {
        self.height_mm
    }

    /// The owner's boss foot RADIUS, half of `diameterofbossbottom`.
    #[must_use]
    pub const fn native_foot_radius_mm(self) -> f64 {
        self.native_foot_radius_mm
    }

    /// The RADIUS of the foot a plate root of drawn root radius
    /// `root_radius_mm` stands on: the owner's boss foot, or the root itself
    /// where the root is wider. Never narrower than either.
    #[must_use]
    pub fn foot_radius_mm(self, root_radius_mm: f64) -> f64 {
        root_radius_mm.max(self.native_foot_radius_mm)
    }
}

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
    /// `BALL_FACTOR_STUB`: the LEGACY engine's tip ball ([`lower_stub`]).
    /// The sweep path reads [`Self::sweep_ball_factor`] instead.
    tip_ball_factor: PositiveFinite,
    /// `CHITUBOX_TIP_CONTACT_PROFILES`: the owner's ChituBox contact spheres,
    /// which size every sweep tip ball ([`Self::sweep_ball_factor`]).
    tip_contact_profiles: &'static TipContactProfiles,
    root_layers: u64,
    root_no_bend_mm: PositiveFinite,
    raft_height_mm: PositiveFinite,
    /// `FLARE_LEN_MM` and `FLARE_R_FACTOR`: the LEGACY engine's root flare
    /// ([`lower_member`]). The sweep path builds [`Self::root_boss`] instead.
    flare_length_mm: PositiveFinite,
    flare_factor: PositiveFinite,
    /// The owner's ChituBox root boss, for every sweep plate root.
    root_boss: RootBoss,
    plate_band_mm: PositiveFinite,
    joint: JointConfig,
    max_primitives: usize,
    max_primitive_bytes: usize,
}

impl LoweringConfig {
    pub(crate) const fn joint_config(self) -> JointConfig {
        self.joint
    }

    /// **The contact-sphere factor of a SWEEP tip of `tip_radius_mm` RADIUS**:
    /// read off the owner's ChituBox profiles by tip size
    /// ([`TipContactProfiles::ball_factor`]). The one reader every sweep ball,
    /// seat and trim computation goes through, so the ball the seat law is
    /// solved for and the ball the file gets are one ball.
    fn sweep_ball_factor(self, tip_radius_mm: f64) -> f64 {
        self.tip_contact_profiles.ball_factor(tip_radius_mm)
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
        let (_, raft_height_mm) = crate::raft::registered_pad_and_raft_height(constants)
            .map_err(|error| LoweringError::RegisteredConstants(error.to_string()))?;
        Ok(Self {
            frame,
            layer_height_mm,
            contact_depth_mm: *policy.default_contact_depth_mm().diagnostic(),
            pixel_pitch_mm: crate::contact::registered_pixel_pitch_mm(constants),
            tip_ball_factor: *policy.ball_factor_stub().diagnostic(),
            tip_contact_profiles: constants.evidence().chitubox_tip_contact_profiles().value(),
            root_layers: policy.chitubox_root_layers().diagnostic().get(),
            root_no_bend_mm: *policy.chitubox_root_height_mm().diagnostic(),
            raft_height_mm,
            flare_length_mm: *policy.flare_len_mm().diagnostic(),
            flare_factor: *policy.flare_r_factor().diagnostic(),
            root_boss: RootBoss::from_constants(constants)
                .map_err(LoweringError::RegisteredConstants)?,
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
        config.flare_length_mm.get(),
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
///
/// `boss_height_mm` is the one thing the two engines do not share: the legacy
/// lowering passes its `FLARE_LEN_MM`, the sweep the owner's ChituBox
/// `heightofboss` ([`RootBoss::height_mm`], 2026-09-30).
fn root_boss_top_at(
    terminal_z_mm: f64,
    rootward_xy: [Finite; 2],
    boss_height_mm: f64,
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
    let effective = minimum_boss_height.max(boss_height_mm.min(available_above_raft / 2.0));
    Some(
        finite(raft_top + effective)
            .map(|top| [rootward_xy[0], rootward_xy[1], top]),
    )
}

/// The z of the terminal a plate-rooted sweep member's boss top is measured
/// from: for a member that seats into the model, the SEATED pole
/// [`seat_tip`] drives the tip to, over the same axis (seat to base) and tip
/// radius [`lower_one_sweep_arm`] hands it; for one that does not, its seat.
/// `None` for an arm the lowering refuses (no length, no positive tip).
fn seated_terminal_z(arm: &SweepArm, config: LoweringConfig) -> Option<f64> {
    if !arm.seats_into_model {
        return Some(arm.seat_mm[2]);
    }
    let delta: [f64; 3] = std::array::from_fn(|index| arm.base_mm[index] - arm.seat_mm[index]);
    let length = delta.iter().map(|value| value * value).sum::<f64>().sqrt();
    if !length.is_finite() || length <= ZERO_LENGTH_MM {
        return None;
    }
    let unit = delta.map(|value| value / length);
    let tip_radius = positive(arm.tip_radius_mm).ok()?;
    Some(seat_tip(arm, unit, tip_radius.get(), config).0[2])
}

/// **Where a plate-rooted sweep member's root flare ends, measured up from
/// the plate**: the height of its BOSS TOP, which is the top of the
/// `RootFlare` record [`lower_sweep_arms`] draws for it, and the height of the
/// raft top that flare stands on.
///
/// The slice sweep asks this so that a brace never ties a member inside its
/// own root flare, where the brace adds resin and no stiffness
/// (fix/brace-not-in-flare, 2026-09-30). It answers with
/// [`root_boss_top_at`] itself, over a plate at z = 0, so that the placement
/// and the lowering cannot disagree about where a flare ends: nothing here
/// computes a boss top a second time.
#[derive(Clone, Copy, Debug)]
pub struct SweepRootFlare {
    config: LoweringConfig,
}

impl SweepRootFlare {
    /// The registered flare geometry at one layer height.
    ///
    /// # Errors
    ///
    /// The same request-shaped errors as [`lower_sweep_arms`]: a layer height
    /// that is not positive and finite, or an unreadable registry.
    pub fn registered(layer_height_mm: f64) -> Result<Self, SweepLoweringError> {
        let plate = Finite::new(0.0).map_err(|_| SweepLoweringError::MissingLayerHeight)?;
        // The lift is read only into the legacy forest digest, never into the
        // boss arithmetic; the layer height stands in as a positive value.
        let lift = PositiveFinite::new(layer_height_mm)
            .map_err(|_| SweepLoweringError::MissingLayerHeight)?;
        let layers = LayerPlan::new(0.0, vec![layer_height_mm])
            .map_err(|_| SweepLoweringError::MissingLayerHeight)?;
        let config = LoweringConfig::current(BuildFrame::new(plate, lift), &layers, 1, 1)?;
        Ok(Self { config })
    }

    /// The boss top of the plate-rooted member `arm`, in mm over the plate at
    /// `plate_z_mm`; `None` where the run is shorter than the registered
    /// no-bend height (the lowering draws a straight column and no flare), or
    /// where the arm has no direction or no positive tip, which the lowering
    /// refuses to draw.
    ///
    /// **Read at the SEATED terminal, as [`lower_sweep_arms`] reads it**
    /// (round-6 review, 2026-09-30). A member that seats into the model has
    /// its tip driven in by the seat law ([`seat_tip`], the derivation the
    /// lowering itself calls), and a run shorter than twice the boss height
    /// above the raft puts its boss top half-way up that run, so the seat
    /// depth moves the boss. Until this date the door took the seat SURFACE
    /// and answered for a terminal one seat depth lower: on a 3.05 mm post
    /// with a 0.05 mm tip, a boss top of 1.625 mm where the file draws
    /// 1.6595 mm (under the legacy 1.5 mm flare length), and a brace tie at
    /// 1.65 mm inside the flare. A member that does not seat (a mast's top) is
    /// its own terminal.
    ///
    /// The boss height is the one the sweep lowering draws, the owner's
    /// ChituBox `heightofboss` ([`RootBoss::height_mm`], 2026-09-30).
    #[must_use]
    pub fn top_above_plate_of(&self, arm: &SweepArm, plate_z_mm: f64) -> Option<f64> {
        let terminal_z = seated_terminal_z(arm, self.config)?;
        let zero = Finite::new(0.0).ok()?;
        let top = root_boss_top_at(
            terminal_z - plate_z_mm,
            [zero, zero],
            self.config.root_boss.height_mm(),
            self.config,
        )?
        .ok()?;
        Some(top[2].get())
    }

    /// `CHITUBOX_RAFT_HEIGHT_MM`: the raft top, in mm over the plate.
    #[must_use]
    pub fn raft_top_above_plate_mm(&self) -> f64 {
        self.config.raft_height_mm.get()
    }
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
// (which `root_boss_top` now delegates to). Since 2026-09-30 the sweep's ball
// and boss are the owner's ChituBox geometry, not the legacy placeholders: the
// ball factor from `CHITUBOX_TIP_CONTACT_PROFILES` by tip size
// (`LoweringConfig::sweep_ball_factor`) and the boss from `RootBoss`.
// ---------------------------------------------------------------------------

/// Where one sweep arm's bottom end sits.
///
/// The sweep's own `Landing` carries a layer index and an arm id that this
/// stage has no use for; the only distinction geometry cares about is whether
/// the plate-root chain applies.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SweepArmLanding {
    /// The build plate: the arm gets the legacy plate-root chain — a boss
    /// column down to the boss top, then a `RootFlare` down to the top of the
    /// raft pad the lowering puts under it (see [`lower_sweep_arms`], steps 3
    /// and 4).
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
    /// Every tip's CONTACT-SPHERE FACTOR, in the same order: the owner's
    /// ChituBox ratio at that tip's size (`CHITUBOX_TIP_CONTACT_PROFILES`,
    /// 2026-09-30), which the emitted ball radius is times the tip radius.
    /// The path-hit witness for the profile table: one entry per tip.
    pub tip_ball_factor: Vec<f64>,
    /// Plate roots whose boss stands on the owner's native 1.1 mm foot
    /// ([`RootBoss`], 2026-09-30).
    pub root_bosses_at_native_foot: usize,
    /// Plate roots whose drawn root is wider than the native foot, so the boss
    /// foot is GROWN to the root (never the root capped).
    pub root_bosses_grown_to_root: usize,
    /// Members drawn as MORE than one straight span because one straight line
    /// from the tip to the root would have dipped below the area law's profile
    /// somewhere — every trunk under a mid-shaft junction. The path-hit
    /// witness for [`drawn_spans`].
    pub arms_drawn_in_spans: usize,
    /// The `Member` frusta those splits added, chained below each taper.
    pub profile_member_frusta: usize,
    /// Of those, the `Member` spans that reach the MODEL side of their arm's
    /// seat plane and so carry its seat trim like the taper above them
    /// (review-fixes 2026-09-29). The path-hit witness for trimming every
    /// shaft span a sloped seat plane crosses, not only the first.
    pub shaft_members_trimmed_to_seat: usize,
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
    /// The part of [`Self::drawn_volume_mm3`] in the raft's pads and bridges.
    pub raft_volume_mm3: f64,
    /// THE RAFT (fix/sweep-raft, 2026-09-29): pads emitted, one under every
    /// plate foot (feet within the registered plate mesh tolerance share one).
    /// The path-hit witness for [`crate::raft`]'s sweep planner.
    pub raft_pads: usize,
    /// Minimum-spanning-tree bridges between the pads: `raft_pads − 1` when
    /// there is any pad, so the raft is one piece.
    pub raft_bridges: usize,
    /// Pads grown WIDER than the native `CHITUBOX_PAD_DIAMETER_MM` so that
    /// their emitted polygon covers the foot above (owner ruling 2026-09-29:
    /// grow the pad, never cap the foot).
    pub raft_pads_grown: usize,
    /// Feet that share a pad with another foot within the registered plate
    /// mesh tolerance.
    pub raft_feet_sharing_a_pad: usize,
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
    /// The raft planner could not read its registered pad constants, or built
    /// a pad or bridge the emit boundary's own contract rejects. About the
    /// registry or a defect in the planner, never about one model's arms.
    #[error("the sweep raft could not be planned: {0}")]
    Raft(String),
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
///    [`SphereGeometry::canonical_poles`], never recomputed here; its radius
///    is the owner's ChituBox contact-sphere ratio at this tip's size times the
///    load-sized tip radius (`CHITUBOX_TIP_CONTACT_PROFILES`, 2026-09-30).
/// 2. `ContactTaper` — the ball centre down to the boss top (plate landing) or
///    to the landing point (model/arm landing), carrying the registered
///    `DEFAULT_CONTACT_DEPTH_MM`. Where one straight taper would dip below the
///    area law's own profile (every trunk under a mid-shaft junction) the taper
///    stops at the first break and the rest of the shaft follows as chained
///    `Member` frusta on the same axis ([`drawn_spans`]).
/// 3. `RootFlare` — plate landings only: the owner's ChituBox ROOT BOSS
///    ([`RootBoss`], 2026-09-30), boss top (`CHITUBOX_BOSS_HEIGHT_MM` above
///    the raft where the run allows) down to the RAFT TOP,
///    `plate_z + CHITUBOX_RAFT_HEIGHT_MM − NATIVE_FOOT_PAD_OVERLAP_MM`, from
///    the drawn root out to the boss foot `max(1.1 mm, r_root)`.
/// 4. THE RAFT — after every arm, once for the whole field: a `RaftPad` on the
///    plate under every plate foot, wide enough that its emitted polygon
///    covers the foot, and `RaftBridge`s along the pads' minimum spanning tree
///    ([`crate::raft`]'s sweep planner). Bridges, then pads, after every
///    support record: the native order.
///
/// **The raft, revived** (fix/sweep-raft, 2026-09-29). The owner added
/// ChituBox's own raft to our Round files and it caught only some feet
/// (`raft-diagnosis.md`): our files carried no raft, while every
/// ChituBox-authored file writes a pad under every plate support and ends the
/// foot ON the pad. So the flare ends on the pad top again, as the legacy
/// lowering always did, and the pad under it now exists. The history: until
/// fix/plate-gap (2026-09-28) this path ended the flare at the raft top with
/// NO raft under it, 0.2 mm (four layers) above the plate — the bases the
/// owner saw float; fix/plate-gap moved the foot to the plate; this change
/// puts the raft back under it. The short-run branch (no boss fits) stands its
/// straight column on its pad the same way. The boss TOP is untouched — the
/// shared [`root_boss_top_at`] reads the raft height as its datum.
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
        tip_ball_factor: Vec::with_capacity(arms.len()),
        tip_contact_disc_radius_mm: Vec::with_capacity(arms.len()),
        tip_raster_seat_offset_mm: Vec::with_capacity(arms.len()),
        ..SweepLowering::default()
    };
    let mut feet: Vec<crate::raft::SweepFoot> = Vec::with_capacity(arms.len());
    for arm in arms {
        let lowered = lower_one_sweep_arm(arm, config, plate, &mut out.primitives);
        feet.extend(lowered.foot);
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
        out.shaft_members_trimmed_to_seat += lowered.members_trimmed;
        match lowered.boss_grown {
            Some(true) => out.root_bosses_grown_to_root += 1,
            Some(false) => out.root_bosses_at_native_foot += 1,
            None => {}
        }
        if let Some(seat) = lowered.seat {
            out.tip_seat_depth_mm.push(seat.depth_mm);
            out.tip_cut_radius_mm.push(seat.cut_radius_mm);
            out.tip_radius_mm.push(seat.tip_radius_mm);
            out.tip_ball_factor.push(seat.ball_factor);
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
    // THE RAFT (fix/sweep-raft, 2026-09-29): one pad under every plate foot,
    // MST bridges between the pads, planned ONCE for the whole field from the
    // feet the arms actually emitted, and appended after every support record
    // in the native order (bridges, then pads).
    let raft = crate::raft::plan_sweep_raft(&feet, config.frame.plate_z_mm())
        .map_err(|error| SweepLoweringError::Raft(error.to_string()))?;
    out.raft_pads = raft.pads;
    out.raft_bridges = raft.bridges;
    out.raft_pads_grown = raft.pads_grown;
    out.raft_feet_sharing_a_pad = raft.feet_sharing_a_pad;
    out.primitives.extend(raft.primitives);
    if out.primitives.len() > max_primitives {
        return Err(SweepLoweringError::PrimitiveLimit {
            limit: max_primitives,
        });
    }
    // THE RESIN AS EMITTED, measured once over the finished primitive batch
    // from each record's own endpoints and radii, never from the ledger.
    for primitive in &out.primitives {
        let volume = emitted_solid_volume_mm3(primitive);
        out.drawn_volume_mm3 += volume;
        match primitive.segment_kind {
            SupportSegmentKind::ContactTaper | SupportSegmentKind::Member => {
                out.drawn_shaft_volume_mm3 += volume;
            }
            SupportSegmentKind::RaftPad | SupportSegmentKind::RaftBridge => {
                out.raft_volume_mm3 += volume;
            }
            _ => {}
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
/// pad is a cylinder and a raft bridge a box (fix/sweep-raft, 2026-09-29: the
/// sweep path now emits its raft, and the raft is resin the file costs).
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
        // A pad is a cylinder: `start_radius` holds its DIAMETER and
        // `end_radius` its height. A bridge is a box: `start_radius` holds its
        // width, `end_radius` its height, and its length is the plan-view
        // distance between its ends.
        SupportSegmentKind::RaftPad => PI * (r0 * 0.5).powi(2) * r1,
        SupportSegmentKind::RaftBridge => {
            let plan = |point: [Finite; 3]| [point[0].get(), point[1].get(), 0.0];
            geom_kernel::vec3::dist(plan(primitive.start_mm), plan(primitive.end_mm)) * r0 * r1
        }
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
    /// The contact-sphere factor the ball was drawn and seated with: the
    /// owner's ChituBox ratio at this tip's size.
    ball_factor: f64,
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
    /// The foot a PLATE arm stands on the raft with, for an arm that reached
    /// the file; `None` for a landed arm or a dropped one.
    foot: Option<crate::raft::SweepFoot>,
    /// Straight spans the shaft was drawn in: 1 is the single taper.
    spans: usize,
    /// `Member` spans below the taper that reach the model side of the seat
    /// plane and so carry the seat trim.
    members_trimmed: usize,
    /// The arm arrived with no radius profile and was drawn at its tip radius.
    without_profile: bool,
    /// A plate arm's root boss: `Some(true)` when the boss foot was GROWN to
    /// a root wider than the owner's 1.1 mm, `Some(false)` at his native foot,
    /// `None` for an arm with no boss.
    boss_grown: Option<bool>,
}

impl ArmLowering {
    const fn refused(outcome: ArmOutcome) -> Self {
        Self {
            outcome,
            seat: None,
            foot: None,
            spans: 0,
            members_trimmed: 0,
            without_profile: false,
            boss_grown: None,
        }
    }
}

/// One straight span of a drawn member, between two heights on its axis.
///
/// Public (round-3 review, 2026-09-29) so the sweep's member-against-member
/// clearance test measures the spans the file draws, by this one rule, rather
/// than the ledger's step profile: a span is a chord and may stand wider than
/// the ledger at a slot it bridges.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DrawnSpan {
    pub top_z_mm: f64,
    pub bottom_z_mm: f64,
    pub top_radius_mm: f64,
    pub bottom_radius_mm: f64,
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
#[must_use]
pub fn drawn_spans(
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
    // The owner's ChituBox ball for this tip size (2026-09-30): the seat law's
    // ball-cap term, the equator bound and the delivered cut below all read
    // this one factor, the same one the emitted ball is drawn with.
    let ball_factor = config.sweep_ball_factor(tip_radius_mm);
    let law_depth_mm = crate::contact::seat_depth_of(
        config.pixel_pitch_mm,
        config.layer_height_mm.get(),
        ball_factor,
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
    let equator_mm = ball_factor * tip_radius_mm;
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
    let cut_radius_mm = crate::contact::ball_cut_radius_mm(tip_radius_mm, depth_mm, ball_factor);
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
            ball_factor,
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
    /// The emitted ball's CENTRE, which is also where the drawn shaft starts
    /// (the taper runs from here to the landing). The sweep's head-join rule
    /// (2026-09-29) measures a trunk's shaft from this point, so it reads the
    /// same derivation the lowering draws from.
    pub ball_centre_mm: [f64; 3],
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
            let ball_radius_mm = config.sweep_ball_factor(arm.tip_radius_mm) * arm.tip_radius_mm;
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
                ball_centre_mm: center,
            })
        })
        .collect())
}

/// **How far the seat trim reaches into a seated arm's SHAFT outside its own
/// tip ball** (fix/lip-seat, 2026-09-29), measured on the arm exactly as the
/// lowering draws it.
///
/// The seat trim keeps `primitive ∩ {support side of the seat plane}` of the
/// ball and of every shaft span that reaches the model side
/// ([`crate::SeatTrim`]). Inside the ball that removes only the ball's own
/// cap region: the shaft there is covered by the ball's solid either way. A
/// shaft section OUTSIDE the ball that crosses the plane is different: the
/// trim removes part of the arm's own section, resin that was drawn and is
/// then thrown away, and on an arm that meets a steep face nearly side-on it
/// removes most of the shaft (owner, Round 25, 2026-09-29: "a very fat
/// side-branch is then almost completely shaved off").
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ShaftSeatBite {
    /// The deepest the seat plane reaches into the drawn shaft section at any
    /// station whose ring is not wholly inside the tip ball, in mm; `0` when
    /// the shaft keeps its whole section outside the ball.
    pub bite_mm: f64,
    /// The angle between the arm's axis (seat to base) and the seat face's
    /// OUTWARD normal, in degrees: `0` for an arm that meets its face square.
    pub approach_deg: f64,
    /// **The derived limit angle** for this arm: the largest approach angle at
    /// which the trim keeps the shaft's section, for the taper as drawn at its
    /// top (radius `r`), the emitted ball (radius `R = f·r_tip`) and the seat
    /// law's own depth `δ(φ)` along the normal. It is the root of
    ///
    /// ```text
    /// g(φ) = (R·cos φ − δ(φ)) + √(R² − r²)·cos φ − r·sin φ
    /// ```
    ///
    /// — the ball centre's height over the plane, plus the rise along the axis
    /// to where the shaft's lateral surface leaves the ball, minus the ring's
    /// own drop towards the plane. `g` falls as `φ` grows, so the root is the
    /// one limit. For a constant taper `r = r_tip`, `f = 1.5` and the flank
    /// term `δ = h + r·sin φ` (one layer `h` plus the tilted end disc), this is
    /// `r·(f + √(f² − 1))·cos φ = h + 2·r·sin φ`: about 50° at `r = 0.36 mm`.
    /// Reported beside `approach_deg`; the decision is [`Self::bite_mm`],
    /// which measures the same thing on the spans as they are drawn (a
    /// junction raise below the head, the raster seat's offset from the face).
    pub limit_deg: f64,
}

/// [`ShaftSeatBite`] for every arm, in arm order: `None` for an arm that
/// seats into nothing, carries no seat plane, or cannot be lowered.
///
/// Each arm is lowered by the production [`lower_one_sweep_arm`] into one
/// reused scratch buffer, so the ball, the trim and every span measured are
/// the ones the file gets. One call for the whole field.
///
/// # Errors
///
/// Only about the request, as [`lower_sweep_arms`].
pub fn tip_shaft_seat_bites(
    arms: &[SweepArm],
    layers: &LayerPlan,
) -> Result<Vec<Option<ShaftSeatBite>>, SweepLoweringError> {
    let plate_z = layers.z0_mm();
    let frame = BuildFrame::new(
        Finite::new(plate_z).map_err(|_| SweepLoweringError::MissingLayerHeight)?,
        PositiveFinite::new(1.0).map_err(|_| SweepLoweringError::InvalidLimits)?,
    );
    let config = LoweringConfig::current(frame, layers, 1, 1)?;
    let mut scratch: Vec<SupportPrimitive> = Vec::new();
    Ok(arms
        .iter()
        .map(|arm| {
            if !arm.seats_into_model {
                return None;
            }
            scratch.clear();
            let lowered = lower_one_sweep_arm(arm, config, plate_z, &mut scratch);
            let trim = lowered.seat.as_ref().and_then(|seat| seat.trim)?;
            shaft_seat_bite(arm, &scratch, trim, config)
        })
        .collect())
}

/// One arm's [`ShaftSeatBite`] from its own lowered primitives.
fn shaft_seat_bite(
    arm: &SweepArm,
    primitives: &[SupportPrimitive],
    trim: crate::SeatTrim,
    config: LoweringConfig,
) -> Option<ShaftSeatBite> {
    let ball = primitives
        .iter()
        .find(|primitive| primitive.segment_kind == SupportSegmentKind::ContactSphere)?;
    let centre: [f64; 3] =
        std::array::from_fn(|axis| 0.5 * (ball.start_mm[axis].get() + ball.end_mm[axis].get()));
    let ball_radius = ball.start_radius_mm.get();
    let normal = trim.outward_normal();
    let mut bite = 0.0_f64;
    let mut taper_top_radius = arm.tip_radius_mm;
    for primitive in primitives {
        if !matches!(
            primitive.segment_kind,
            SupportSegmentKind::ContactTaper | SupportSegmentKind::Member
        ) {
            continue;
        }
        if primitive.segment_kind == SupportSegmentKind::ContactTaper {
            taper_top_radius = primitive.start_radius_mm.get();
        }
        bite = bite.max(frustum_bite_outside_ball(
            primitive,
            trim,
            centre,
            ball_radius,
        ));
    }
    let delta = geom_kernel::vec3::sub(arm.base_mm, arm.seat_mm);
    let unit = geom_kernel::vec3::normalize(delta)?;
    let approach_deg = geom_kernel::vec3::dot(unit, normal)
        .clamp(-1.0, 1.0)
        .acos()
        .to_degrees();
    let limit_deg = seat_angle_limit_deg(arm.tip_radius_mm, taper_top_radius, ball_radius, config);
    Some(ShaftSeatBite {
        bite_mm: bite,
        approach_deg,
        limit_deg,
    })
}

/// The deepest the seat plane reaches into one frustum record's section at a
/// station whose ring is not wholly inside the ball `(centre, radius)`.
///
/// Along the record, `P(t) = start + t·(end − start)` and `r(t)` are linear,
/// and the ring's lowest point towards the plane is `d(P(t)) − r(t)·√(1 −
/// (n·m)²)` for ring normal `m` ([`frustum_reaches_model_side`]'s own
/// formula), so the bite over any interval of `t` is largest at one of its
/// ends. The ring leaves the ball where its FARTHEST point from the centre,
/// `√((Δ·m)² + (|Δ_⊥| + r)²)` for `Δ = P − centre`, reaches the ball's
/// radius; that distance grows along a record running away from the ball, so
/// the station is found by bisection and the bite is read at it and at the
/// record's far end. Conservative for a ring that only partly leaves the
/// ball: its whole section is graded there.
fn frustum_bite_outside_ball(
    primitive: &SupportPrimitive,
    trim: crate::SeatTrim,
    centre: [f64; 3],
    ball_radius: f64,
) -> f64 {
    /// Halvings of `t ∈ [0, 1]`: the station to well under a nanometre on any
    /// record this engine draws.
    const BISECTIONS: usize = 48;
    let start = primitive.start_mm.map(Finite::get);
    let end = primitive.end_mm.map(Finite::get);
    let (r0, r1) = (
        primitive.start_radius_mm.get(),
        primitive.end_radius_mm.get(),
    );
    let ring_normal = match primitive.ring_normal {
        FrustumRingNormal::SegmentAxis => {
            geom_kernel::vec3::normalize(geom_kernel::vec3::sub(end, start))
        }
        FrustumRingNormal::PlateZ => Some([0.0, 0.0, 1.0]),
    };
    let Some(ring_normal) = ring_normal else {
        return 0.0;
    };
    let along = geom_kernel::vec3::dot(trim.outward_normal(), ring_normal);
    let spread = (1.0 - along * along).max(0.0).sqrt();
    let at = |t: f64| -> ([f64; 3], f64) {
        (
            std::array::from_fn(|axis| start[axis] + t * (end[axis] - start[axis])),
            r0 + t * (r1 - r0),
        )
    };
    let outside = |t: f64| -> bool {
        let (point, radius) = at(t);
        let offset = geom_kernel::vec3::sub(point, centre);
        let axial = geom_kernel::vec3::dot(offset, ring_normal);
        let in_plane = geom_kernel::vec3::sub(offset, geom_kernel::vec3::scale(ring_normal, axial));
        let far = geom_kernel::vec3::norm(in_plane) + radius;
        axial * axial + far * far > ball_radius * ball_radius
    };
    let lowest = |t: f64| -> f64 {
        let (point, radius) = at(t);
        trim.support_side_distance_mm(point) - radius * spread
    };
    if !outside(1.0) {
        return 0.0;
    }
    let exit = if outside(0.0) {
        0.0
    } else {
        let (mut inside_t, mut outside_t) = (0.0_f64, 1.0_f64);
        for _ in 0..BISECTIONS {
            let mid = 0.5 * (inside_t + outside_t);
            if outside(mid) {
                outside_t = mid;
            } else {
                inside_t = mid;
            }
        }
        outside_t
    };
    let bound = (-lowest(exit)).max(-lowest(1.0)).max(0.0);
    // A ring normal to the axis is centred on the ball's own axis, so every
    // point of it is equally far from the centre and the bound above is exact.
    // A HORIZONTAL ring on a leaned axis (a plate column) is not: it can leave
    // the ball on one side while the part of its section past the plane is
    // still inside the ball, and the bound would call that a cut. Where the
    // bound finds one on such a record, the section itself is sampled.
    if bound <= 0.0 || primitive.ring_normal == FrustumRingNormal::SegmentAxis {
        return bound;
    }
    sampled_bite_outside_ball(&at, ring_normal, trim, centre, ball_radius, exit)
}

/// The deepest point past the plane and outside the ball of a record's
/// sections, sampled: `STATIONS + 1` stations over `t ∈ [exit, 1]`, each
/// section's disc at `RINGS` radii of `ANGLES` points. A resolution of the
/// measurement, not a physical number: the depth is linear on the section, so
/// the sampled maximum sits within one sample spacing of the true one.
fn sampled_bite_outside_ball(
    at: &impl Fn(f64) -> ([f64; 3], f64),
    ring_normal: [f64; 3],
    trim: crate::SeatTrim,
    centre: [f64; 3],
    ball_radius: f64,
    exit: f64,
) -> f64 {
    const STATIONS: usize = 32;
    const RINGS: usize = 4;
    const ANGLES: usize = 64;
    let reference = if ring_normal[0].abs() < 0.9 {
        [1.0, 0.0, 0.0]
    } else {
        [0.0, 1.0, 0.0]
    };
    let Some(e1) = geom_kernel::vec3::normalize(geom_kernel::vec3::cross(ring_normal, reference))
    else {
        return 0.0;
    };
    let e2 = geom_kernel::vec3::cross(ring_normal, e1);
    let mut deepest = 0.0_f64;
    for station in 0..=STATIONS {
        #[expect(
            clippy::cast_precision_loss,
            reason = "sample counts are small integers, exact in f64"
        )]
        let t = exit + (1.0 - exit) * station as f64 / STATIONS as f64;
        let (point, radius) = at(t);
        for ring in 1..=RINGS {
            #[expect(
                clippy::cast_precision_loss,
                reason = "sample counts are small integers, exact in f64"
            )]
            let rho = radius * ring as f64 / RINGS as f64;
            for angle in 0..ANGLES {
                #[expect(
                    clippy::cast_precision_loss,
                    reason = "sample counts are small integers, exact in f64"
                )]
                let (sin, cos) = (std::f64::consts::TAU * angle as f64 / ANGLES as f64).sin_cos();
                let q: [f64; 3] =
                    std::array::from_fn(|i| point[i] + rho * (cos * e1[i] + sin * e2[i]));
                let depth = -trim.support_side_distance_mm(q);
                if depth > deepest
                    && geom_kernel::vec3::norm(geom_kernel::vec3::sub(q, centre)) > ball_radius
                {
                    deepest = depth;
                }
            }
        }
    }
    deepest
}

/// The derived limit angle of [`ShaftSeatBite::limit_deg`], in degrees, by
/// bisection on `g(φ)` over `[0°, 90°]` with the seat law the lowering uses
/// ([`seat_tip`]: `δ = min(law(cos φ), R)`, the law reached along the normal).
fn seat_angle_limit_deg(
    tip_radius_mm: f64,
    shaft_radius_mm: f64,
    ball_radius_mm: f64,
    config: LoweringConfig,
) -> f64 {
    /// Halvings of the 90° interval: far finer than any angle this reports.
    const BISECTIONS: usize = 48;
    let exit_rise = (ball_radius_mm * ball_radius_mm - shaft_radius_mm * shaft_radius_mm)
        .max(0.0)
        .sqrt();
    let clearance = |phi_deg: f64| -> f64 {
        let (sin, cos) = phi_deg.to_radians().sin_cos();
        let law = crate::contact::seat_depth_of(
            config.pixel_pitch_mm,
            config.layer_height_mm.get(),
            config.sweep_ball_factor(tip_radius_mm),
            cos,
            tip_radius_mm,
            true,
        );
        let depth = law.min(ball_radius_mm);
        (ball_radius_mm * cos - depth) + exit_rise * cos - shaft_radius_mm * sin
    };
    if clearance(0.0) < 0.0 {
        return 0.0;
    }
    if clearance(90.0) >= 0.0 {
        return 90.0;
    }
    let (mut holds, mut fails) = (0.0_f64, 90.0_f64);
    for _ in 0..BISECTIONS {
        let mid = 0.5 * (holds + fails);
        if clearance(mid) >= 0.0 {
            holds = mid;
        } else {
            fails = mid;
        }
    }
    holds
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
    let Ok(root_radius) = positive(drawn_root_radius_mm(
        &arm.radius_profile_mm,
        arm.tip_radius_mm,
    )) else {
        return ArmLowering::refused(ArmOutcome::InvalidGeometry);
    };
    // THE OWNER'S CHITUBOX BALL (2026-09-30): the contact-sphere-to-tip ratio
    // of his profiles at this tip's size, on the load-sized tip radius.
    let Ok(ball_radius) = positive(config.sweep_ball_factor(tip_radius.get()) * tip_radius.get())
    else {
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
    let mut boss_grown = None;
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
            // THE FOOT STANDS ON ITS RAFT PAD (fix/sweep-raft, 2026-09-29).
            // Every plate root now gets a pad under it (`plan_sweep_raft`,
            // called once for the whole field by `lower_sweep_arms`), so the
            // foot ends where every ChituBox-authored foot ends: on the pad
            // TOP, `plate_z + CHITUBOX_RAFT_HEIGHT_MM`, reaching
            // `NATIVE_FOOT_PAD_OVERLAP_MM` into the pad so the two solids
            // share volume. The pad itself stands on `plate_z`. (From
            // fix/plate-gap, 2026-09-28, until this change the path emitted
            // no raft and the foot ended on the plate itself.)
            let Ok(foot_z) =
                finite(plate_z + config.raft_height_mm.get() - NATIVE_FOOT_PAD_OVERLAP_MM)
            else {
                return ArmLowering::refused(ArmOutcome::InvalidGeometry);
            };
            let foot_point = [base[0], base[1], foot_z];
            // The SEATED terminal z, exactly as the legacy path reads
            // `member.p0_mm()[2]` — which `initial.rs` already minted embedded.
            // Its boss top is measured from the raft top, as it always was,
            // at the owner's ChituBox boss height (2026-09-30).
            let boss_top = root_boss_top_at(
                seated_seat[2],
                [base[0], base[1]],
                config.root_boss.height_mm(),
                config,
            )
            .and_then(Result::ok);
            match boss_top {
                Some(top)
                    if distance(center, top) > ZERO_LENGTH_MM
                        && distance(top, foot_point) > ZERO_LENGTH_MM =>
                {
                    // THE OWNER'S CHITUBOX BOSS FOOT (2026-09-30): 1.1 mm, or
                    // the drawn root where the root is wider -- grown, never
                    // capped. The `RootFlare` record is the boss cone.
                    let Ok(flare_radius) =
                        positive(config.root_boss.foot_radius_mm(root_radius.get()))
                    else {
                        return ArmLowering::refused(ArmOutcome::InvalidGeometry);
                    };
                    boss_grown =
                        Some(flare_radius.get() > config.root_boss.native_foot_radius_mm());
                    let Some(drawn) = shaft(center, top, FrustumRingNormal::PlateZ, &mut staged)
                    else {
                        return ArmLowering::refused(ArmOutcome::InvalidGeometry);
                    };
                    spans = drawn;
                    staged.push(frustum(
                        SupportSegmentKind::RootFlare,
                        top,
                        foot_point,
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
                    // the raft, so no flare fits. Emit the straight column down
                    // to its pad and WARN: refusing here would destroy the
                    // evidence.
                    if distance(center, foot_point) <= ZERO_LENGTH_MM
                        || center[2].get() <= foot_z.get()
                    {
                        return ArmLowering::refused(ArmOutcome::ZeroLength);
                    }
                    let Some(drawn) =
                        shaft(center, foot_point, FrustumRingNormal::PlateZ, &mut staged)
                    else {
                        return ArmLowering::refused(ArmOutcome::InvalidGeometry);
                    };
                    spans = drawn;
                    ArmOutcome::BuiltWithoutFlare
                }
            }
        }
    };
    // THE SHAFT IS TRIMMED TO THE SAME SEAT (fix/tip-trim, 2026-09-28;
    // every span, review-fixes 2026-09-29). The taper starts at the ball
    // centre, and on a leaned arm or under a sloped face its start cap stands
    // through the seat plane and past the face's edge exactly where the ball
    // does ("a chunk of post", tip-clip.md). The same plane and the same
    // model difference apply.
    //
    // The rule is about the SOLID, not about how it is cut into records: the
    // drawn profile breaks one straight shaft into several spans wherever the
    // ledger steps (`drawn_spans`), and a span boundary can fall a slot below
    // the seat, inside the band where the shaft's ring still reaches the model
    // side of a sloped seat plane. So the taper keeps the trim it always had,
    // and every further `Member` span whose own solid reaches the model side
    // of the plane carries the same trim. A span wholly on the support side is
    // left untouched and emits bit-identically, as before.
    let mut members_trimmed = 0;
    if let Some(trim) = seat.as_ref().and_then(|seat| seat.trim) {
        for primitive in &mut staged {
            match primitive.segment_kind {
                SupportSegmentKind::ContactTaper => primitive.seat_trim = Some(trim),
                SupportSegmentKind::Member if frustum_reaches_model_side(primitive, trim) => {
                    primitive.seat_trim = Some(trim);
                    members_trimmed += 1;
                }
                _ => {}
            }
        }
    }
    // The emit boundary's own contract, asked HERE, so one bad arm is dropped
    // and counted rather than failing the whole field at
    // `SupportedModel::from_parts` with nothing said about which arm did it.
    if staged.iter().any(|primitive| !primitive.is_semantically_valid()) {
        return ArmLowering::refused(ArmOutcome::InvalidGeometry);
    }
    // The foot the raft pad must cover: the LOWEST record of a plate arm — its
    // flare, or the last span of a short column — at its lower end. Its ring
    // is horizontal on both (a flare's axis is vertical; a column carries
    // plate-Z rings), so its end radius is the circle the emitted ring is
    // inscribed in.
    let foot = match arm.landing {
        SweepArmLanding::Plate => staged.last().map(|lowest| crate::raft::SweepFoot {
            xy_mm: [base[0].get(), base[1].get()],
            radius_mm: lowest.end_radius_mm.get(),
        }),
        SweepArmLanding::Landed => None,
    };
    out.append(&mut staged);
    ArmLowering {
        outcome,
        seat,
        foot,
        spans,
        members_trimmed,
        without_profile: arm.radius_profile_mm.is_empty(),
        boss_grown,
    }
}

/// **The root a member is DRAWN with** — the section its plate boss is sized
/// from ([`RootBoss::foot_radius_mm`] at the plate): the first entry of the radius
/// profile the member is drawn at, never narrower than its tip, and the tip
/// itself for an arm with no profile.
///
/// The ONE rule for it (review-fixes 2026-09-29). The lowering flares this
/// root, and the sweep's congestion census and root-clearance decisions size
/// every plate disc from it too, so the disc a decision is taken against is
/// the disc the file lays down.
#[must_use]
pub fn drawn_root_radius_mm(profile: &[f64], tip_radius_mm: f64) -> f64 {
    profile
        .first()
        .copied()
        .unwrap_or(tip_radius_mm)
        .max(tip_radius_mm)
}

/// Whether any point of a frustum record lies strictly on the MODEL side of a
/// seat plane.
///
/// A frustum is the convex hull of its two end rings, so a linear function
/// (the signed distance from the plane) takes its minimum on one of them. On a
/// ring of radius `r` about centre `c` with unit ring normal `m`, the minimum
/// is `d(c) − r·√(1 − (n·m)²)`, `n` the plane's unit normal. The ring normal
/// is the record's own ([`FrustumRingNormal`]): the axis for an ordinary rod,
/// `+Z` for a plate-root column's horizontal rings. Exact for the drawn
/// circle; the emitted polygon is inscribed in it, so a record this calls
/// clear is clear in the file too.
fn frustum_reaches_model_side(primitive: &SupportPrimitive, trim: crate::SeatTrim) -> bool {
    let start = primitive.start_mm.map(Finite::get);
    let end = primitive.end_mm.map(Finite::get);
    let ring_normal = match primitive.ring_normal {
        FrustumRingNormal::SegmentAxis => {
            geom_kernel::vec3::normalize(geom_kernel::vec3::sub(end, start))
        }
        FrustumRingNormal::PlateZ => Some([0.0, 0.0, 1.0]),
    };
    let Some(ring_normal) = ring_normal else {
        return false;
    };
    let along = geom_kernel::vec3::dot(trim.outward_normal(), ring_normal);
    let spread = (1.0 - along * along).max(0.0).sqrt();
    [
        (start, primitive.start_radius_mm.get()),
        (end, primitive.end_radius_mm.get()),
    ]
    .into_iter()
    .any(|(centre, radius)| trim.support_side_distance_mm(centre) - radius * spread < 0.0)
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

/// The seat trim's reach into a shaft outside its ball
/// ([`tip_shaft_seat_bites`], fix/lip-seat 2026-09-29), measured on arms built
/// by hand against a flat and a steep face.
#[cfg(test)]
mod shaft_seat_bite_tests {
    use super::{tip_shaft_seat_bites, ShaftSeatBite, SweepArm, SweepArmLanding};
    use plinth_core::LayerPlan;

    const TIP_MM: f64 = 0.3;
    const LENGTH_MM: f64 = 3.0;

    fn layers() -> LayerPlan {
        LayerPlan::new(0.0, vec![0.05; 200]).expect("a uniform layer plan")
    }

    /// A landed arm seated at `seat` on a face of outward normal `normal`
    /// through `seat`, running `LENGTH_MM` along `axis` (unit, seat to base).
    fn arm(seat: [f64; 3], normal: [f64; 3], axis: [f64; 3]) -> SweepArm {
        SweepArm {
            seat_mm: seat,
            base_mm: std::array::from_fn(|i| seat[i] + LENGTH_MM * axis[i]),
            tip_radius_mm: TIP_MM,
            radius_profile_mm: vec![TIP_MM; 60],
            seat_normal_mm: normal,
            seat_face_point_mm: Some(seat),
            landing: SweepArmLanding::Landed,
            seats_into_model: true,
        }
    }

    fn leaned(deg: f64) -> [f64; 3] {
        let (sin, cos) = deg.to_radians().sin_cos();
        [sin, 0.0, -cos]
    }

    fn measure(arms: &[SweepArm]) -> Vec<ShaftSeatBite> {
        tip_shaft_seat_bites(arms, &layers())
            .expect("a valid request")
            .into_iter()
            .map(|bite| bite.expect("every arm seats into a measured face"))
            .collect()
    }

    /// Under a flat face a vertical arm keeps its whole section, and the
    /// derived limit is the angle the measured cut starts at: one degree
    /// inside it nothing outside the ball is cut, one degree past it the plane
    /// reaches the shaft. The limit and the cut are two readings of one
    /// geometry, so each checks the other.
    #[test]
    fn the_derived_limit_is_where_the_measured_cut_starts() {
        let seat = [0.0, 0.0, 5.0];
        let down = [0.0, 0.0, -1.0];
        let square = measure(&[arm(seat, down, down)])[0];
        assert!(
            square.bite_mm.abs() < 1e-12,
            "square arm cut {}",
            square.bite_mm
        );
        assert!(square.approach_deg.abs() < 1e-9);
        let limit = square.limit_deg;
        assert!(
            (40.0..60.0).contains(&limit),
            "a 0.3 mm tip with its 1.5x ball under a flat face: limit {limit}"
        );
        let pair = measure(&[
            arm(seat, down, leaned(limit - 1.0)),
            arm(seat, down, leaned(limit + 1.0)),
        ]);
        assert!(
            pair[0].bite_mm.abs() < 1e-12,
            "{:.2} deg inside the limit cut {:.5} mm",
            limit - 1.0,
            pair[0].bite_mm
        );
        assert!(
            pair[1].bite_mm > 1e-4,
            "{:.2} deg past the limit cut only {:.6} mm",
            limit + 1.0,
            pair[1].bite_mm
        );
        assert!((pair[1].approach_deg - (limit + 1.0)).abs() < 1e-9);
    }

    /// The owner's case: a face 75° from horizontal (a drafted inner wall),
    /// and an arm leaning 57.5° ALONG it, as a side-branch merging with a
    /// neighbour does. Its axis runs nearly parallel to the plane, and the
    /// trim reaches past the shaft's own axis: most of the section is cut.
    /// The same arm leaning 57.5° straight out of the face keeps its section.
    #[test]
    fn an_arm_running_along_a_steep_face_loses_most_of_its_section() {
        let seat = [0.0, 0.0, 5.0];
        let (sin, cos) = 75.0_f64.to_radians().sin_cos();
        let wall = [-sin, 0.0, -cos];
        let (lean_sin, lean_cos) = 57.5_f64.to_radians().sin_cos();
        let along = [0.0, lean_sin, -lean_cos];
        let out_of = [-lean_sin, 0.0, -lean_cos];
        let bites = measure(&[arm(seat, wall, along), arm(seat, wall, out_of)]);
        assert!(bites[0].approach_deg > bites[0].limit_deg);
        assert!(
            bites[0].bite_mm > TIP_MM,
            "along the wall the plane reaches past the axis: cut {:.4} mm",
            bites[0].bite_mm
        );
        assert!(bites[1].approach_deg < bites[1].limit_deg);
        assert!(
            bites[1].bite_mm.abs() < 1e-12,
            "out of the wall: cut {}",
            bites[1].bite_mm
        );
    }

    /// A member that seats into nothing (a brace, a mast) has no ball and no
    /// seat trim, so nothing is measured.
    #[test]
    fn a_member_that_seats_into_nothing_is_not_measured() {
        let mut brace = arm([0.0, 0.0, 5.0], [0.0, 0.0, -1.0], leaned(60.0));
        brace.seats_into_model = false;
        let bites = tip_shaft_seat_bites(&[brace], &layers()).expect("a valid request");
        assert_eq!(bites, vec![None]);
    }
}

/// [`SweepRootFlare`] answers with the boss top [`lower_sweep_arms`] DRAWS
/// (round-6 review, 2026-09-30): graded against the emitted `RootFlare`
/// record of each arm, lowered on its own, never against the door's own
/// arithmetic.
#[cfg(test)]
mod sweep_root_flare_tests {
    use super::{lower_sweep_arms, SweepArm, SweepArmLanding, SweepRootFlare};
    use crate::{BuildFrame, SupportSegmentKind};
    use plinth_core::{Finite, LayerPlan, PositiveFinite};

    const H: f64 = 0.05;

    /// A plate-rooted arm seated at `seat` (z over a plate at `plate_z`),
    /// standing on the plate at `base_xy`, against a face of outward normal
    /// `normal` (zero: unmeasured).
    fn plate_arm(
        seat: [f64; 3],
        base_xy: [f64; 2],
        plate_z: f64,
        tip: f64,
        normal: [f64; 3],
    ) -> SweepArm {
        SweepArm {
            seat_mm: seat,
            base_mm: [base_xy[0], base_xy[1], plate_z],
            tip_radius_mm: tip,
            radius_profile_mm: Vec::new(),
            seat_normal_mm: normal,
            seat_face_point_mm: None,
            landing: SweepArmLanding::Plate,
            seats_into_model: true,
        }
    }

    /// The `RootFlare` record's top z, over the plate, as the file has it.
    fn drawn_boss_top_mm(arm: &SweepArm, plate_z: f64) -> f64 {
        let layers = LayerPlan::new(plate_z, vec![H; 400]).expect("a uniform plan");
        let frame = BuildFrame::new(
            Finite::new(plate_z).expect("a finite plate"),
            PositiveFinite::new(5.0).expect("a positive lift"),
        );
        let lowering =
            lower_sweep_arms(std::slice::from_ref(arm), frame, &layers, 100_000, 1 << 30)
                .expect("the lowering refuses only about the request");
        let tops: Vec<f64> = lowering
            .primitives
            .iter()
            .filter(|primitive| primitive.segment_kind == SupportSegmentKind::RootFlare)
            .map(|flare| flare.start_mm[2].get() - plate_z)
            .collect();
        assert_eq!(tops.len(), 1, "one plate arm draws one flare: {arm:?}");
        tops[0]
    }

    #[test]
    fn the_door_reads_the_boss_top_the_lowering_draws_at_the_seated_terminal() {
        let door = SweepRootFlare::registered(H).expect("the registry");
        let down = [0.0, 0.0, -1.0];
        let tilted = {
            let (sin, cos) = 35.0_f64.to_radians().sin_cos();
            [sin, 0.0, -cos]
        };
        let cases: Vec<(&str, SweepArm, f64)> = vec![
            // The reviewer's post: 3.05 mm, 0.05 mm tip, normal unmeasured.
            (
                "short vertical, unmeasured",
                plate_arm([0.0, 0.0, 3.05], [0.0, 0.0], 0.0, 0.05, [0.0; 3]),
                0.0,
            ),
            (
                "short vertical, flat face",
                plate_arm([0.0, 0.0, 3.05], [0.0, 0.0], 0.0, 0.05, down),
                0.0,
            ),
            (
                "short vertical, 35 deg face",
                plate_arm([0.0, 0.0, 2.6], [0.0, 0.0], 0.0, 0.3, tilted),
                0.0,
            ),
            (
                "short leaned, 35 deg face",
                plate_arm([0.0, 0.0, 2.6], [0.8, 0.0], 0.0, 0.1, tilted),
                0.0,
            ),
            (
                "short, plate at z = 7.3",
                plate_arm([1.0, 2.0, 10.15], [1.0, 2.0], 7.3, 0.05, down),
                7.3,
            ),
            (
                "tall vertical",
                plate_arm([0.0, 0.0, 8.0], [0.0, 0.0], 0.0, 0.2, down),
                0.0,
            ),
            (
                "short mast (no ball)",
                SweepArm {
                    seats_into_model: false,
                    ..plate_arm([0.0, 0.0, 2.4], [0.0, 0.0], 0.0, 0.1, [0.0; 3])
                },
                0.0,
            ),
        ];
        let mut moved_by_the_seat = 0;
        for (what, arm, plate_z) in &cases {
            let drawn = drawn_boss_top_mm(arm, *plate_z);
            let answered = door.top_above_plate_of(arm, *plate_z).unwrap_or_else(|| {
                panic!("{what}: the door found no flare where the file draws one")
            });
            assert!(
                (answered - drawn).abs() < 1e-9,
                "{what}: the door answers {answered:.6} mm, the file draws {drawn:.6} mm"
            );
            // How far the seat moved the boss off the unseated reading
            // (`seat − plate` itself standing as the terminal).
            let unseated = door
                .top_above_plate_of(
                    &SweepArm {
                        seats_into_model: false,
                        ..arm.clone()
                    },
                    *plate_z,
                )
                .expect("the same run, unseated");
            if (drawn - unseated).abs() > 1e-6 {
                moved_by_the_seat += 1;
            }
        }
        // Anti-vacuity: the short seated arms are the ones the seat depth
        // moves (5 of them); the tall post's boss is the full flare, and the
        // mast does not seat.
        assert_eq!(
            moved_by_the_seat, 5,
            "the seat depth moved {moved_by_the_seat} boss tops"
        );
    }
}
