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

mod junction;

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
/// boss, at his height and SCALED TO THE ROOT by his HEAVY profile's ratio
/// (Craig ChituBox profile export 2026-09-30, field
/// `support_general_para.heightofboss` = 1.0 mm in all four profiles,
/// `CHITUBOX_BOSS_HEIGHT_MM`; and profile 2 "Heavy"'s `diameterofbossbottom`
/// 2.2 over its `middle_support.supportmiddlediameter` 1.5,
/// `CHITUBOX_ROOT_BOSS_PROFILE`).
///
/// A cone from the shaft's own drawn root section at the boss top down to the
/// boss foot on the raft pad. **The foot is the root times 22/15, about 1.47,
/// for every support** (Craig ruling 2026-09-30: Heavy profile ratio). It is
/// NOT a fixed 2.2 mm boss under every support (that raised resin 4-15 % on
/// our 2-4 mm dense fields), and NOT a ratio read off his four profiles by
/// root size (1.47 to 4.4; that raised resin 5-26 % on the lane models). The
/// ratio is above one, so the foot is always wider than its root and never
/// caps it; a foot still stands on its raft pad, which grows only where its
/// polygon cannot cover the foot. A root at Heavy's own 1.5 mm shaft gets the
/// native 2.2 mm foot.
///
/// It replaces the legacy flare (`FLARE_R_FACTOR` × root over `FLARE_LEN_MM`)
/// on the sweep path. The sweep's placement decisions at the plate (the
/// congestion census, the mast and thicken root clearance, the lip-seat
/// new-root clearance) keep their crowding disc, `FLARE_R_FACTOR` × the drawn
/// root (`plinth_sweep::SweepPolicy::flare_r_factor`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RootBoss {
    height_mm: f64,
    /// The owner's Heavy boss-to-shaft ratio, read once off the registry.
    factor: f64,
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

    fn from_constants(constants: &'static M0Constants) -> Result<Self, String> {
        let evidence = constants.evidence();
        let height_mm = *evidence.chitubox_boss_height_mm().value();
        // `RootBossProfile::new` refuses a foot no wider than its shaft, so
        // the registered factor is above one; checked again here so a boss
        // that would cap its root can never be built.
        let factor = evidence.chitubox_root_boss_profile().value().boss_factor();
        if height_mm.is_finite() && height_mm > 0.0 && factor.is_finite() && factor > 1.0 {
            Ok(Self { height_mm, factor })
        } else {
            Err(format!(
                "the registered root boss is not a solid wider than its root: height \
                 {height_mm} mm, foot-to-root factor {factor}"
            ))
        }
    }

    /// The boss's height above the raft top, where the run allows it.
    #[must_use]
    pub const fn height_mm(self) -> f64 {
        self.height_mm
    }

    /// The boss-foot-to-root ratio of EVERY plate root: the owner's Heavy
    /// profile's `diameterofbossbottom` / `supportmiddlediameter`
    /// ([`plinth_core::RootBossProfile::boss_factor`]), whatever the root's size.
    #[must_use]
    pub const fn factor(self) -> f64 {
        self.factor
    }

    /// The RADIUS of the foot a plate root of drawn root radius
    /// `root_radius_mm` stands on: [`Self::factor`] times the root. Always
    /// wider than the root.
    #[must_use]
    pub fn foot_radius_mm(self, root_radius_mm: f64) -> f64 {
        self.factor * root_radius_mm
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
    Some(finite(raft_top + effective).map(|top| [rootward_xy[0], rootward_xy[1], top]))
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
    // A headed arm's post starts at the carried joint.
    if let Some(head) = &arm.head {
        if headed_tip(arm, config).is_some() {
            return Some(head.joint_mm[2]);
        }
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

    /// **The straight line [`lower_sweep_arms`] draws a plate-rooted member's
    /// SHAFT along** (fix/morel-long-posts, 2026-10-02): from the shaft's top
    /// (the seated ball centre of a member that seats into the model, its
    /// seat otherwise) down to its BOSS TOP, which stands vertically over its
    /// foot; or down to its foot on the pad where no boss fits. In the same
    /// frame as `arm`, over a plate at `plate_z_mm`.
    ///
    /// The placement plans a member as the straight segment from its seat to
    /// its base ON THE PLATE. For a member that leans, the boss is not on that
    /// segment: it stands over the foot, so the file draws the shaft from the
    /// seat to a point `boss top` above the foot, and the drawn axis leaves
    /// the planned one by `lateral × boss_top / (seat_height − boss_top)` at
    /// the plate's end. On the Morel base (for-craig-v10) a post leaning 6.15
    /// mm over 10.46 mm drew its shaft 0.58 mm away from the planned axis at
    /// 1.1 mm up, and the two `|X|` diagonals tied to it there ended in air.
    /// A rule that puts anything ON a member's drawn shaft reads it here: the
    /// same seat law ([`seated_ball`]) and boss arithmetic
    /// ([`root_boss_top_at`]) the lowering calls, so the two cannot disagree.
    ///
    /// `None` for a member that does not stand on the plate, or that the
    /// lowering refuses to draw (no length, no positive tip).
    #[must_use]
    pub fn plate_shaft_of(&self, arm: &SweepArm, plate_z_mm: f64) -> Option<([f64; 3], [f64; 3])> {
        if arm.landing != SweepArmLanding::Plate {
            return None;
        }
        // This door's config stands on a plate at z = 0; the arm is read in
        // that frame and the answer moved back.
        let lift = |point: [f64; 3], by: f64| [point[0], point[1], point[2] + by];
        let local = SweepArm {
            seat_mm: lift(arm.seat_mm, -plate_z_mm),
            base_mm: lift(arm.base_mm, -plate_z_mm),
            seat_face_point_mm: arm.seat_face_point_mm.map(|point| lift(point, -plate_z_mm)),
            head: arm.head.as_ref().map(|head| SweepHead {
                contact_mm: lift(head.contact_mm, -plate_z_mm),
                joint_mm: lift(head.joint_mm, -plate_z_mm),
                ..head.clone()
            }),
            ..arm.clone()
        };
        let delta: [f64; 3] =
            std::array::from_fn(|index| local.base_mm[index] - local.seat_mm[index]);
        let length = delta.iter().map(|value| value * value).sum::<f64>().sqrt();
        if !length.is_finite() || length <= ZERO_LENGTH_MM {
            return None;
        }
        let unit = delta.map(|value| value / length);
        let tip_radius = positive(local.tip_radius_mm).ok()?.get();
        let (seated_seat, top) = if let Some(head) = headed_tip(&local, self.config) {
            // A headed arm's post starts at the carried joint.
            (head.head.joint_mm, head.head.joint_mm)
        } else if local.seats_into_model {
            let ball_radius = self.config.sweep_ball_factor(tip_radius) * tip_radius;
            let (seated_seat, center, _) =
                seated_ball(&local, unit, tip_radius, ball_radius, self.config);
            (seated_seat, center)
        } else {
            (local.seat_mm, local.seat_mm)
        };
        let foot = [
            local.base_mm[0],
            local.base_mm[1],
            self.config.raft_height_mm.get() - NATIVE_FOOT_PAD_OVERLAP_MM,
        ];
        let zero_length = |a: [f64; 3], b: [f64; 3]| {
            let d: f64 = (0..3).map(|i| (a[i] - b[i]).powi(2)).sum::<f64>().sqrt();
            d <= ZERO_LENGTH_MM
        };
        let boss = root_boss_top_at(
            seated_seat[2],
            [
                finite(local.base_mm[0]).ok()?,
                finite(local.base_mm[1]).ok()?,
            ],
            self.config.root_boss.height_mm(),
            self.config,
        )
        .and_then(Result::ok)
        .map(|top| [top[0].get(), top[1].get(), top[2].get()])
        .filter(|boss| !zero_length(top, *boss) && !zero_length(*boss, foot));
        let bottom = boss.unwrap_or(foot);
        Some((lift(top, plate_z_mm), lift(bottom, plate_z_mm)))
    }

    /// The printer's rasterisation allowance, one layer in Z plus one pixel in
    /// plane (the seat law's own smallest separation,
    /// `contact::rasterisation_allowance_mm`), in mm.
    #[must_use]
    pub fn rasterisation_allowance_mm(&self) -> f64 {
        crate::contact::rasterisation_allowance_mm(
            self.config.layer_height_mm.get(),
            self.config.pixel_pitch_mm,
        )
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
        match primitive.ring_normal {
            FrustumRingNormal::SegmentAxis => digest.byte(0),
            FrustumRingNormal::PlateZ => digest.byte(1),
            FrustumRingNormal::Junction(rings) => {
                digest.byte(2);
                for ring in rings.rings() {
                    for coordinate in ring.frustum_start_mm.into_iter().chain(ring.frustum_end_mm) {
                        digest.u64(coordinate.get().to_bits());
                    }
                    digest.u64(ring.radius_mm.get().to_bits());
                    digest.byte(u8::from(ring.at_start));
                    digest.byte(u8::from(ring.plate_z));
                }
            }
        }
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
/// stage has no use for; the distinctions geometry cares about are whether
/// the plate-root chain applies, and whether the end is a CONTACT on the
/// model (a foot) or a junction on another arm.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SweepArmLanding {
    /// The build plate: the arm gets the legacy plate-root chain — a boss
    /// column down to the boss top, then a `RootFlare` down to the top of the
    /// raft pad the lowering puts under it (see [`lower_sweep_arms`], steps 3
    /// and 4).
    Plate,
    /// Another arm, or (where the sweep measured no foot) cured model
    /// material. The arm ends there. v1 emits no joint sphere at an
    /// arm-onto-arm junction; that is disclosed in the run report rather than
    /// hidden.
    Landed,
    /// **Cured model material, with a CONTACT FOOT** (lane 2, 2026-10-12,
    /// `MODEL-FEET.md` F1): the shaft narrows in a cone to a ball seated into
    /// the landed face by the same seat law as a top tip ([`seated_ball`]) and
    /// cut to the model mesh by the same [`crate::SeatTrim`]. The contact is
    /// sized by the area law for the load the foot passes, not by the shaft's
    /// widest section ([`ModelFoot`]).
    Model(ModelFoot),
}

/// **The foot of an arm that lands on the model** ([`SweepArmLanding::Model`]),
/// as the sweep measured it at the finished landing.
///
/// *UNITS TRAP*: radii, as everywhere in this stage.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ModelFoot {
    /// A point ON the landed face: the closest point of the nearest UP-facing
    /// mesh face to the landing. `None` where no face was measured; the plane
    /// through the raster surface then stands in, as for a top tip.
    pub face_point_mm: Option<[f64; 3]>,
    /// OUTWARD unit normal of the landed face (away from the material, so
    /// upward for a face a foot stands on). Zero or non-finite means
    /// unmeasured, and straight up stands in.
    pub face_normal_mm: [f64; 3],
    /// **The contact the foot must deliver**: the area law for the load the
    /// foot passes, `π r² ≥ carried / X_ALLOWED`, read from the down-sweep's
    /// load ledger at the landing layer BEFORE any slenderness or head minimum
    /// widened the section there. It is the foot's tip radius: the ball is
    /// the owner's ChituBox ball for this size and is seated by the seat law
    /// exactly as a top tip of this radius is.
    pub contact_radius_mm: f64,
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
    /// With the outward normal it is the SEAT PLANE ([`crate::SeatTrim`]): the
    /// plane the PLANNED contact disc lies in, and the one that says which
    /// shaft spans reach the model side and are cut to the model. Nothing is
    /// cut to this plane since 2026-10-12 (the emitter cuts to the model mesh).
    /// It is the model's own surface, not `seat_mm`: `seat_mm` is the bottom of
    /// the first cured LAMINA, which the layer raster puts up to half a layer
    /// above or below the true face. `None` falls back to the plane through
    /// `seat_mm`.
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
    /// **The articulated head** (Craig 2026-10-06): the ball is seated along
    /// [`SweepHead::unit`] (so the contact arrives square), a short straight
    /// head runs to a joint sphere at the carried [`SweepHead::joint_mm`], and
    /// the post continues from there to the base. `None` is the straight arm,
    /// drawn exactly as before this field existed. For a headed arm
    /// [`Self::radius_profile_mm`] spans `base_mm` -> `joint_mm` (not the seat).
    /// An arm whose head cannot be drawn is drawn straight and counted
    /// ([`SweepLowering::heads_drawn_straight`]).
    pub head: Option<SweepHead>,
    /// **The tip ball grown so its REAL contact reaches plan** (Craig
    /// 2026-10-12: cut to the mesh, area governs; orchestrator ruling of the
    /// same date: keep the seat depth, grow the ball). `None` draws the
    /// owner's ChituBox ball for this tip size (`ball factor × tip radius`).
    /// A value above that ball draws a larger ball with the SAME deepest
    /// point along the seat normal, so the seat depth does not change: the
    /// new ball contains the old one and is tangent to it there. A value at or
    /// below it is ignored. The sweep measures the model surface inside the
    /// ball and sets this (`plinth_sweep::contact_growth`); the planned
    /// contact ([`TipContactDisc::radius_mm`]) is always the ungrown ball's.
    pub contact_ball_radius_mm: Option<f64>,
}

/// What the sweep decided about one arm's head ([`SweepArm::head`]). Every
/// point is in the frame of [`SweepArm::seat_mm`]. `K` is CARRIED: the
/// lowering never re-derives it from the seated ball.
#[derive(Clone, Debug, PartialEq)]
pub struct SweepHead {
    /// `D`: the contact centre, on the seat plane. The seat trim's plane is
    /// `(contact_mm, unit)`.
    pub contact_mm: [f64; 3],
    /// `u`: the outward unit normal of the face, `z < 0`.
    pub unit: [f64; 3],
    /// `K`: the joint sphere's centre.
    pub joint_mm: [f64; 3],
    /// `r_K`: the joint sphere's radius.
    pub joint_radius_mm: f64,
    /// `g`: the registered final clearance gap.
    pub gap_mm: f64,
    /// The head's section as a polyline of `(s, r)` knots, `s` the distance
    /// along `u` from `D` (the first knot has `s = 0`, the last `s = |K - D|`,
    /// strictly rising), `r` the drawn radius there. One straight frustum is
    /// drawn between consecutive knots, so a polyline on or above the
    /// bending law stays on or above it. The last radius is the joint's.
    ///
    /// With a link ([`Self::link_mm`] above zero) these are the CONE's knots
    /// only, the first at the link's planned far end with the link's radius;
    /// the lowering draws the link itself from the seated ball.
    pub profile_knots_mm: Vec<[f64; 2]>,
    /// `L_link`: the length of the bare LINK, mm (Craig 2026-10-08: "neck
    /// 0.3-0.6, short is fine"): a cylinder of [`Self::link_radius_mm`] that
    /// runs this far along `u` from where it leaves the contact ball, then the
    /// cone ([`Self::profile_knots_mm`]). `0` draws the head from the ball
    /// centre along the knots, as before the link existed.
    pub link_mm: f64,
    /// `r_L`: the link's radius, mm. Read only when [`Self::link_mm`] is
    /// above zero.
    pub link_radius_mm: f64,
    /// The head is sized by the joint certificate (Craig 2026-10-11): the cone
    /// knots and the joint radius are ONE profile, certified on every frustum
    /// and at `K`. The lowering draws the chain verbatim, with no pixel merge
    /// and no widening of the joint sphere past the last knot.
    pub certified: bool,
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
    /// Every plate root boss's FOOT-TO-ROOT FACTOR, in arm order: the owner's
    /// Heavy profile's boss-to-shaft ratio, the same for every root
    /// (`CHITUBOX_ROOT_BOSS_PROFILE`, [`RootBoss`], Craig ruling 2026-09-30:
    /// Heavy profile ratio), which the emitted foot radius is times the drawn
    /// root. The path-hit witness for the boss: one entry per boss drawn.
    pub root_boss_factor: Vec<f64>,
    /// Every plate root boss's drawn ROOT radius, in the same order.
    pub root_boss_root_radius_mm: Vec<f64>,
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
    /// Tips drawn as a head ([`SweepArm::head`]): ball along the face normal,
    /// head to a joint sphere, post. The path-hit witness for heads.
    pub tips_headed: usize,
    /// Arms that carried a head the lowering could not draw (non-finite, no
    /// length, a ball already past the joint, no measured seat plane) and drew
    /// straight instead.
    pub heads_drawn_straight: usize,
    /// Headed arms whose neck, where it leaves the ball, is wider than the
    /// ball's own solid there (allowed: the law asks for it).
    pub head_necks_wider_than_ball: usize,
    /// Per headed arm, in arm order: the neck's first ring radius over the
    /// ball's solid radius where the neck leaves it (above 1 when wider).
    pub head_neck_ratio: Vec<f64>,
    /// Per headed arm, in arm order: the DRAWN LINK, measured from the head's
    /// drawn frusta (Craig 2026-10-07, redefined 2026-10-08): the bare axial
    /// length of the leading constant-radius run, from where its ring leaves
    /// the contact ball's surface to where the radius first rises ([`drawn_link_mm`]).
    /// `0` when the first frustum widens (a cone from the ball). (Until
    /// 2026-10-08 this held the PLANNED gap `|K - C| - R - r_K`, which the
    /// drawn file did not have: median 0.96 mm reported, 0 mm drawn.) The field
    /// keeps its name because the sweep adapter copies it by name.
    pub head_stalk_mm: Vec<f64>,
    /// Per headed arm, in arm order: the joint's clearance from the seat
    /// plane, `(K - D)·u - r_K`. It equals `g` when the joint has not grown.
    pub head_standoff_mm: Vec<f64>,
    /// **Model feet drawn** ([`SweepArmLanding::Model`]): arms that land on
    /// the model and end in a seated, trimmed contact ball. The path-hit
    /// witness for the foot.
    pub model_feet_drawn: usize,
    /// Of those, feet whose seat plane is the plane through the raster surface
    /// because no landed face was measured (or the measured face's plane
    /// missed the ball).
    pub model_feet_without_measured_face: usize,
    /// Of those, feet whose shaft narrows to the foot in a cone (the shaft's
    /// root is wider than the foot's load-sized contact by more than one
    /// pixel of cone). The others run the shaft straight into the ball.
    pub model_foot_cones: usize,
    /// Of the coned feet, those whose arm is too short for the cone at the
    /// lean law's slope: the shaft runs into the ball at its own root radius
    /// instead, and the trim cuts it to the model (emit and warn).
    pub model_foot_cones_too_short: usize,
    /// Model-landed arms the sweep handed no usable foot (a non-finite or zero
    /// contact radius): the shaft ends flat on the model as before 2026-10-12.
    /// Zero on every sweep field; a nonzero count is a caller defect.
    pub model_feet_flat: usize,
    /// Per drawn foot, in arm order: the load-sized contact radius.
    pub model_foot_radius_mm: Vec<f64>,
    /// Per drawn foot, in arm order: the PLANNED contact disc radius (the
    /// ball's section in the seat plane, as for a top tip).
    pub model_foot_contact_disc_radius_mm: Vec<f64>,
    /// Per drawn foot, in arm order: the foot ball's radius.
    pub model_foot_ball_radius_mm: Vec<f64>,
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
///    the drawn root out to the boss foot, the root times the owner's Heavy
///    boss-to-shaft ratio, 2.2 / 1.5, for every root (`CHITUBOX_ROOT_BOSS_PROFILE`).
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
    lower_sweep_arms_of(
        arms,
        None,
        frame,
        layers,
        max_primitives,
        max_primitive_bytes,
        None,
    )
}

/// [`lower_sweep_arms`], with the junction hulls PLANNED on another field of
/// arms and KEPT on this one (R8, Craig 2026-10-11).
///
/// `hull_reference` is the field as an earlier sizing drew it (same members,
/// same order, same ends); `None` is [`lower_sweep_arms`]. The junction pass
/// ([`junction::hull_junctions`]) decides its hosts, its pull-backs, its cut
/// arcs and its rings from the shafts it is given, so a member whose radii
/// moved would move every ring on it. Here the pass runs on the reference
/// field's shafts, and the hulls it makes are the hulls the file gets, record
/// for record. The shafts of the arms they join are the arms' own, cut at the
/// reference's arcs with the reference's radii there
/// ([`junction::keep_planned_hulls`]): the hull ring and the shaft that meets
/// it agree, and nothing a hull covers is drawn twice. A reference whose shafts
/// do not line up with the arms' is not used: the pass runs on the arms and the
/// lowering says so.
///
/// # Errors
///
/// The same request-shaped errors [`lower_sweep_arms`] returns.
pub fn lower_sweep_arms_with_hull_reference(
    arms: &[SweepArm],
    hull_reference: Option<&[SweepArm]>,
    frame: BuildFrame,
    layers: &LayerPlan,
    max_primitives: usize,
    max_primitive_bytes: usize,
) -> Result<SweepLowering, SweepLoweringError> {
    lower_sweep_arms_of(
        arms,
        hull_reference,
        frame,
        layers,
        max_primitives,
        max_primitive_bytes,
        None,
    )
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
        None,
        frame,
        layers,
        max_primitives,
        max_primitive_bytes,
        Some(seat_minimum_mm),
    )
}

fn lower_sweep_arms_of(
    arms: &[SweepArm],
    hull_reference: Option<&[SweepArm]>,
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
    // Where each arm's records stand, for the junction pass below.
    let mut arm_ends: Vec<usize> = Vec::with_capacity(arms.len());
    let mut junction_arms: Vec<junction::JunctionArm> = Vec::with_capacity(arms.len());
    for arm in arms {
        let arm_start = out.primitives.len();
        let lowered = lower_one_sweep_arm(arm, config, plate, &mut out.primitives);
        let emitted = !matches!(
            lowered.outcome,
            ArmOutcome::ZeroLength | ArmOutcome::InvalidGeometry
        );
        junction_arms.push(junction::JunctionArm {
            landed: emitted && arm.landing == SweepArmLanding::Landed,
            shaft: (emitted && lowered.shaft.1 > 0)
                .then_some((arm_start + lowered.shaft.0, lowered.shaft.1)),
        });
        arm_ends.push(out.primitives.len());
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
        out.heads_drawn_straight += usize::from(lowered.head_drawn_straight);
        if let Some(head) = lowered.head {
            out.tips_headed += 1;
            out.head_necks_wider_than_ball += usize::from(head.neck_wider_than_ball);
            out.head_neck_ratio.push(head.neck_ratio);
            out.head_stalk_mm.push(head.stalk_mm);
            out.head_standoff_mm.push(head.standoff_mm);
        }
        out.shaft_members_trimmed_to_seat += lowered.members_trimmed;
        if let Some((root_radius_mm, factor)) = lowered.boss {
            out.root_boss_root_radius_mm.push(root_radius_mm);
            out.root_boss_factor.push(factor);
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
        if let Some(foot) = lowered.model_foot {
            out.model_feet_drawn += 1;
            out.model_feet_without_measured_face += usize::from(!foot.seat.face_measured);
            out.model_foot_cones += usize::from(foot.cone);
            out.model_foot_cones_too_short += usize::from(foot.cone_too_short);
            out.model_foot_radius_mm.push(foot.seat.tip_radius_mm);
            out.model_foot_contact_disc_radius_mm
                .push(foot.seat.contact_disc_radius_mm);
            out.model_foot_ball_radius_mm
                .push(foot.seat.drawn_ball_radius_mm);
        }
        out.model_feet_flat += usize::from(lowered.model_foot_flat);
        if out.primitives.len() > max_primitives {
            return Err(SweepLoweringError::PrimitiveLimit {
                limit: max_primitives,
            });
        }
    }
    // LANDED JUNCTIONS AS HULLS (heads v19 fixes, L2): each landed branch
    // whose end ring is not already inside its host meets it in the convex
    // hull of the end rings that meet there. The pass edits the shafts of the
    // arms it joins and adds one hull record per junction, after the branch.
    let sides = registered_emit_sides();
    if sides >= 3 {
        let rules = junction::JunctionRules {
            pixel_pitch_mm: config.pixel_pitch_mm,
            layer_height_mm: config.layer_height_mm.get(),
            sides,
            cone_slope: drawn_span_rules().map_or(0.0, |rules| rules.cone_slope),
        };
        // R8: the hulls are planned on the reference field when one is given,
        // and kept on this one; else the pass runs on the arms as they are.
        let kept = hull_reference
            .filter(|reference| reference.len() == arms.len())
            .and_then(|reference| {
                let (reference_primitives, reference_arms) = lower_hull_reference(
                    arms,
                    reference,
                    &out.primitives,
                    &arm_ends,
                    &junction_arms,
                    config,
                    plate,
                );
                junction::keep_planned_hulls(
                    &out.primitives,
                    &junction_arms,
                    &reference_primitives,
                    &reference_arms,
                    rules,
                )
            });
        if hull_reference.is_some() && kept.is_none() {
            eprintln!(
                "WARNING junction hulls: the reference field's shafts do not line up with the \
                 arms'; the hulls were planned on the arms as they are (emit and warn)"
            );
        }
        let edits = kept.unwrap_or_else(|| {
            junction::hull_junctions(&out.primitives, &junction_arms, rules)
        });
        let rings = edits.rings;
        if rings.exact + rings.thicker + rings.thinner + rings.unmet > 0 {
            eprintln!(
                "junction rings: {} kept hull rings met by the shaft on the ring's own radius \
                 ({} already equal, {} thicker than the chord and tapered down to it, {} \
                 thinner and tapered up to it; largest difference closed {:.6} mm, largest dip \
                 under the chord {:.6} mm), {} not met",
                rings.exact + rings.thicker + rings.thinner,
                rings.exact,
                rings.thicker,
                rings.thinner,
                rings.max_gap_mm,
                rings.max_under_chord_mm,
                rings.unmet,
            );
        }
        if rings.unmet > 0 {
            eprintln!(
                "WARNING junction rings: {} kept hull rings could not be met on their own \
                 radius ({} with no piece of the arm at the cut, {} at a piece a taper may not \
                 change, {} where the pieces ran out before the difference closed); the shaft \
                 meets them on its own radius (emit and warn)",
                rings.unmet,
                rings.unmet_no_piece,
                rings.unmet_not_adjustable,
                rings.unmet_ran_out,
            );
        }
        // Emit and warn: a landed junction that could be neither cut into its
        // host nor filled keeps the old flat cap, and says so.
        if edits.crowded + edits.undrawable > 0 {
            eprintln!(
                "WARNING junction hulls: {} landed junctions kept the old flat cap ({} crowded, \
                 {} undrawable); {} hulled, {} filled with an added hull, {} buried in the host, \
                 {} with no host",
                edits.crowded + edits.undrawable,
                edits.crowded,
                edits.undrawable,
                edits.hulled,
                edits.filled,
                edits.buried,
                edits.no_host,
            );
        }
        if edits.hulled + edits.filled > 0 {
            let mut rebuilt: Vec<SupportPrimitive> =
                Vec::with_capacity(out.primitives.len() + edits.hulled + edits.filled);
            let mut from = 0;
            for (index, &end) in arm_ends.iter().enumerate() {
                let slice = &out.primitives[from..end];
                match (&edits.shafts[index], junction_arms[index].shaft) {
                    (Some(shaft), Some((first, len))) => {
                        let offset = first - from;
                        rebuilt.extend_from_slice(&slice[..offset]);
                        rebuilt.extend(shaft.iter().cloned());
                        rebuilt.extend_from_slice(&slice[offset + len..]);
                    }
                    _ => rebuilt.extend_from_slice(slice),
                }
                rebuilt.extend(edits.hulls[index].iter().cloned());
                from = end;
            }
            out.primitives = rebuilt;
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

/// **The reference field, lowered** (R8): the records and the junction arms of
/// `reference`, arm for arm. An arm the reference leaves as it is shares its
/// records with the field's (no second lowering of it); the rest are lowered
/// into a scratch list, with no counter, foot or seat kept.
fn lower_hull_reference(
    arms: &[SweepArm],
    reference: &[SweepArm],
    primitives: &[SupportPrimitive],
    arm_ends: &[usize],
    junction_arms: &[junction::JunctionArm],
    config: LoweringConfig,
    plate: f64,
) -> (Vec<SupportPrimitive>, Vec<junction::JunctionArm>) {
    let mut out: Vec<SupportPrimitive> = Vec::with_capacity(primitives.len());
    let mut reference_arms: Vec<junction::JunctionArm> = Vec::with_capacity(arms.len());
    let mut from = 0;
    for (index, arm) in arms.iter().enumerate() {
        let end = arm_ends[index];
        let start = out.len();
        if reference[index] == *arm {
            out.extend_from_slice(&primitives[from..end]);
            reference_arms.push(junction::JunctionArm {
                landed: junction_arms[index].landed,
                shaft: junction_arms[index]
                    .shaft
                    .map(|(first, len)| (start + (first - from), len)),
            });
        } else {
            let lowered = lower_one_sweep_arm(&reference[index], config, plate, &mut out);
            let emitted = !matches!(
                lowered.outcome,
                ArmOutcome::ZeroLength | ArmOutcome::InvalidGeometry
            );
            reference_arms.push(junction::JunctionArm {
                landed: emitted && reference[index].landing == SweepArmLanding::Landed,
                shaft: (emitted && lowered.shaft.1 > 0)
                    .then_some((start + lowered.shaft.0, lowered.shaft.1)),
            });
        }
        from = end;
    }
    (out, reference_arms)
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
                // A junction hull is not a frustum: the hull of its rings.
                FrustumRingNormal::Junction(rings) => return junction_hull_volume_mm3(&rings),
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

/// A drawn frustum counts as a constant-radius run (a cylinder) when its two
/// end radii differ by no more than this: the file stores coordinates as
/// `f32`, so a radius is only known to about a tenth of a micron.
const LINK_FLAT_TOLERANCE_MM: f64 = 1.0e-4;

/// **The drawn link between the contact ball and the head's cone**, in mm
/// along the head axis, from the DRAWN frusta (Craig 2026-10-08: the link is
/// the bare cylinder the ball hinges on; one definition for the log and the
/// audit).
///
/// `frusta` are the head's drawn cones as `([a, r] at the start, [a, r] at the
/// end)`, `a` the distance along the head axis from the ball's centre, `r` the
/// drawn radius there, in order. The link is the LEADING constant-radius run:
/// the first frustum and every frustum after it that keeps the same radius (to
/// [`LINK_FLAT_TOLERANCE_MM`]). It is measured from where that radius leaves
/// the ball's surface, `a = sqrt(R² − r²)` (the run's start where the radius
/// is the ball's or more), to where the radius first rises. It is `0` when the
/// first frustum widens (a cone from the ball), and when the run ends inside
/// the ball.
fn drawn_link_mm(frusta: &[([f64; 2], [f64; 2])], ball_radius: f64) -> f64 {
    let flat = |&([_, r0], [_, r1]): &([f64; 2], [f64; 2])| (r1 - r0).abs() <= LINK_FLAT_TOLERANCE_MM;
    let Some(&([start_at, radius], _)) = frusta.first().filter(|first| flat(first)) else {
        return 0.0;
    };
    let run_end = frusta
        .iter()
        .take_while(|frustum| {
            flat(frustum) && (frustum.0[1] - radius).abs() <= LINK_FLAT_TOLERANCE_MM
        })
        .last()
        .map_or(start_at, |last| last.1[0]);
    let exit = if radius < ball_radius {
        (ball_radius * ball_radius - radius * radius)
            .sqrt()
            .max(start_at)
    } else {
        start_at
    };
    (run_end - exit).max(0.0)
}

/// `chitubox_emit_n_sides`: the polygon every frustum ring and hull ring is
/// drawn with, read once. `0` when the registry cannot be read.
fn registered_emit_sides() -> usize {
    static SIDES: std::sync::OnceLock<usize> = std::sync::OnceLock::new();
    *SIDES.get_or_init(|| {
        m0_constants().map_or(0, |constants| {
            usize::try_from(
                constants
                    .placeholders()
                    .chitubox_emit_n_sides()
                    .diagnostic()
                    .get(),
            )
            .unwrap_or(0)
        })
    })
}

/// The solid one junction hull encloses, in mm³: the hull of its rings, made
/// with the emitter's own ring routine and polygon.
fn junction_hull_volume_mm3(rings: &crate::JunctionRings) -> f64 {
    let sides = registered_emit_sides();
    let mut points: Vec<[f64; 3]> = Vec::with_capacity(3 * sides);
    for ring in rings.rings() {
        let Some(corners) = crate::hull::frustum_ring(
            ring.frustum_start_mm.map(Finite::get),
            ring.frustum_end_mm.map(Finite::get),
            ring.radius_mm.get(),
            ring.at_start,
            ring.plate_z,
            sides,
        ) else {
            return 0.0;
        };
        points.extend(corners);
    }
    crate::hull::convex_hull(&points).map_or(0.0, |hull| hull.volume().max(0.0))
}

enum ArmOutcome {
    Built,
    BuiltWithoutFlare,
    ZeroLength,
    InvalidGeometry,
}

/// What one arm's tip seat actually delivered, recorded only for an arm that
/// reached the file.
#[derive(Clone, Copy)]
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
    /// The PLANNED contact disc: the ungrown ball's section in the seat plane,
    /// the area the seat was sized for. Filled by the caller once the ball
    /// exists.
    contact_disc_radius_mm: f64,
    /// The radius the ball is DRAWN with: the ungrown ChituBox ball, or the
    /// larger ball [`SweepArm::contact_ball_radius_mm`] asks for.
    drawn_ball_radius_mm: f64,
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
    /// A plate arm's root boss: its drawn root radius and the foot-to-root
    /// factor it was drawn with; `None` for an arm with no boss.
    boss: Option<(f64, f64)>,
    /// What a headed arm drew ([`SweepArm::head`]); `None` for a straight one.
    head: Option<HeadDrawn>,
    /// The arm carried a head that could not be drawn and was drawn straight.
    head_drawn_straight: bool,
    /// Where the drawn shaft frusta stand among the arm's own records:
    /// `(offset, length)`. The junction pass edits exactly these.
    shaft: (usize, usize),
    /// What a model-landed arm's foot drew ([`SweepArmLanding::Model`]).
    model_foot: Option<FootDrawn>,
    /// A model-landed arm whose foot could not be built and that ends flat.
    model_foot_flat: bool,
}

/// The measurements of one drawn model foot.
#[derive(Clone, Copy)]
struct FootDrawn {
    seat: TipSeat,
    cone: bool,
    cone_too_short: bool,
}

/// The measurements of one drawn head.
#[derive(Clone, Copy)]
struct HeadDrawn {
    stalk_mm: f64,
    standoff_mm: f64,
    neck_wider_than_ball: bool,
    neck_ratio: f64,
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
            boss: None,
            head: None,
            head_drawn_straight: false,
            shaft: (0, 0),
            model_foot: None,
            model_foot_flat: false,
        }
    }
}

/// One arm's head, derived from its carried [`SweepHead`] and its seated ball
/// ([`headed_tip`]). `K` is the carried joint, never derived from the ball.
struct HeadedTip<'a> {
    head: &'a SweepHead,
    /// The ball's axis: the head's outward normal.
    unit: [f64; 3],
    /// The seated pole, the ball centre and the seat record, as
    /// [`seated_ball`] gives them along [`Self::unit`].
    seated: [f64; 3],
    centre: [f64; 3],
    tip_seat: TipSeat,
    /// Where the head starts: the ball centre, or, where the centre stands
    /// under the seat plane, the contact disc's centre on it.
    start: [f64; 3],
    /// The start's height over the seat plane, `max(d, 0)`.
    start_height_mm: f64,
    /// The ball's own radius `R` at its centre, the contact disc's radius
    /// `sqrt(R² - d²)` where the start is on the plane.
    start_cap_radius_mm: f64,
    /// The ball's radius `R`.
    ball_radius_mm: f64,
    /// The ball centre's distance along [`Self::unit`] from the contact centre
    /// `D` (negative where the centre stands under the seat plane).
    centre_s_mm: f64,
}

/// **The head of an articulated arm**, or `None` where the arm cannot be drawn
/// as one (it has no [`SweepArm::head`], seats into nothing, the head is not
/// finite, the seated ball does not meet the head's seat plane, or the ball
/// already stands past the joint). Every reader of a headed arm's geometry goes
/// through this one derivation, so the placement, the census and the file see
/// one head.
fn headed_tip<'a>(arm: &'a SweepArm, config: LoweringConfig) -> Option<HeadedTip<'a>> {
    let head = arm.head.as_ref()?;
    if !arm.seats_into_model {
        return None;
    }
    let finite_point = |point: [f64; 3]| point.iter().all(|value| value.is_finite());
    if !(finite_point(head.contact_mm)
        && finite_point(head.joint_mm)
        && head.joint_radius_mm.is_finite()
        && head.joint_radius_mm > 0.0
        && head.profile_knots_mm.len() >= 2
        && head
            .profile_knots_mm
            .iter()
            .all(|[s, radius]| s.is_finite() && *s >= 0.0 && radius.is_finite() && *radius > 0.0)
        && head
            .profile_knots_mm
            .windows(2)
            .all(|pair| pair[0][0] < pair[1][0]))
    {
        return None;
    }
    let unit = geom_kernel::vec3::normalize(head.unit)?;
    if unit[2] >= 0.0 {
        return None;
    }
    let tip_radius = positive(arm.tip_radius_mm).ok()?.get();
    let ball_radius = config.sweep_ball_factor(tip_radius) * tip_radius;
    // The ball is seated against the head's own seat plane `(D, u)`: the seat
    // trim of a headed arm. The raster seat still says how deep the ball goes.
    let seating = SweepArm {
        radius_profile_mm: Vec::new(),
        seat_normal_mm: unit,
        seat_face_point_mm: Some(head.contact_mm),
        head: None,
        ..arm.clone()
    };
    let (seated, centre, tip_seat) = seated_ball(&seating, unit, tip_radius, ball_radius, config);
    // The ball as drawn: grown where the sweep asked for more contact.
    let ball_radius = tip_seat.drawn_ball_radius_mm;
    let trim = tip_seat.trim.filter(|_| tip_seat.face_measured)?;
    let normal = trim.outward_normal();
    let centre_height_mm = trim.support_side_distance_mm(centre);
    // Where the ball centre stands under the plane (the raster seat inside the
    // face by more than the ball's height), the head starts on the plane, at
    // the contact disc's centre.
    let (start, start_height_mm, start_cap_radius_mm) = if centre_height_mm < 0.0 {
        (
            std::array::from_fn(|axis| centre[axis] - centre_height_mm * normal[axis]),
            0.0,
            (ball_radius * ball_radius - centre_height_mm * centre_height_mm)
                .max(0.0)
                .sqrt(),
        )
    } else {
        (centre, centre_height_mm, ball_radius)
    };
    let to_joint = geom_kernel::vec3::dot(unit, geom_kernel::vec3::sub(head.joint_mm, start));
    let rise = start[2] - head.joint_mm[2];
    if !(to_joint.is_finite()
        && to_joint > ZERO_LENGTH_MM
        && rise.is_finite()
        && rise > ZERO_LENGTH_MM)
    {
        return None;
    }
    Some(HeadedTip {
        head,
        unit,
        seated,
        centre,
        tip_seat,
        start,
        start_height_mm,
        start_cap_radius_mm,
        ball_radius_mm: ball_radius,
        centre_s_mm: geom_kernel::vec3::dot(unit, geom_kernel::vec3::sub(centre, head.contact_mm)),
    })
}

/// What [`push_head_segment`] drew.
struct HeadSegment {
    spans: usize,
    /// The neck's first ring is wider than the ball's solid at the start.
    neck_wider_than_ball: bool,
    /// The neck's first ring radius over the ball's solid radius at the start.
    neck_ratio: f64,
}

/// Draw the head from its start to the carried joint `K`, and the joint sphere
/// at `K`. The head's section is the carried polyline of `(s, r)` knots (the
/// bending law's tangent polyline, from the contact centre to the joint): the
/// polyline is clipped at the start's own distance `s_start` along `u`, and
/// one straight frustum is drawn between consecutive knots, so consecutive
/// frusta share their end radius, no step is drawn, and the last ring is the
/// joint's. The first is the contact taper. A neck ring wider than the ball is
/// allowed, and reported. `None` where a frustum will not cross the finite
/// boundary, or the start is not before the joint.
fn push_head_segment(
    headed: &HeadedTip<'_>,
    config: LoweringConfig,
    staged: &mut Vec<SupportPrimitive>,
) -> Option<HeadSegment> {
    let head = headed.head;
    let start = headed.start;
    let knots = &head.profile_knots_mm;
    let length = knots.last()?[0];
    // `start` stands `s_start` along `u` from the contact centre `D`: on the
    // plane (0) or the ball centre above it.
    let s_start =
        geom_kernel::vec3::dot(headed.unit, geom_kernel::vec3::sub(start, head.contact_mm))
            .max(0.0);
    // A NaN start or length is not before the joint either.
    if s_start.partial_cmp(&length) != Some(std::cmp::Ordering::Less) {
        return None;
    }
    // THE LINK (Craig 2026-10-08): the head is the ball, a bare cylinder of
    // the link's radius that leaves the ball's surface and runs `link_mm`
    // further along `u`, then the cone. The cylinder is the first frustum
    // (the contact taper) and the carried knots are the cone's: the first is
    // the planned far end of the link and is replaced by the drawn one, which
    // follows the ball as it is seated. A cone knot within one XY pixel of the
    // link's end would be a frustum shorter than a pixel and is merged into the
    // next; the cone starts at the link's radius, never wider.
    //
    // A CERTIFIED head (Craig 2026-10-11) is drawn verbatim: the link, then the
    // certified knots as they stand (the first is the link's far end), with no
    // pixel merge, because every frustum and the joint were certified as drawn.
    if head.certified && head.link_mm > 0.0 && head.link_radius_mm > 0.0 {
        if let Some(cone_start) = knots.first().filter(|knot| knot[0] > s_start + LINK_MIN_LENGTH_MM) {
            let chain: Vec<[f64; 2]> = std::iter::once([s_start, head.link_radius_mm])
                .chain(knots.iter().copied())
                .collect();
            debug_assert!(
                (cone_start[1] - head.link_radius_mm).abs() <= 1.0e-12,
                "a certified cone starts at the link's radius"
            );
            let ball = headed.ball_radius_mm;
            return push_head_chain(
                headed,
                config,
                &chain,
                staged,
                head.link_radius_mm >= ball,
                head.link_radius_mm / ball,
            );
        }
    }
    if head.link_mm > 0.0 && head.link_radius_mm > 0.0 {
        let link_radius = head.link_radius_mm;
        let ball = headed.ball_radius_mm;
        let exit = (ball * ball - link_radius * link_radius).max(0.0).sqrt();
        let link_end = (headed.centre_s_mm + exit + head.link_mm)
            .max(s_start + LINK_MIN_LENGTH_MM)
            .min(length - LINK_MIN_LENGTH_MM);
        let cone = knots.get(1..).filter(|cone| !cone.is_empty())?;
        let first = cone
            .partition_point(|knot| knot[0] < link_end + config.pixel_pitch_mm)
            .min(cone.len() - 1);
        let chain: Vec<[f64; 2]> = [[s_start, link_radius], [link_end, link_radius]]
            .into_iter()
            .chain(cone[first..].iter().copied())
            .collect();
        return push_head_chain(
            headed,
            config,
            &chain,
            staged,
            link_radius >= ball,
            link_radius / ball,
        );
    }
    // The clipped polyline: the law's radius at `s_start`, then every knot
    // past it. A knot within a micron of the start would be a degenerate
    // frustum and is skipped.
    let past = knots
        .partition_point(|knot| knot[0] <= s_start + ZERO_LENGTH_MM)
        .min(knots.len() - 1);
    let first_radius = match past.checked_sub(1).map(|index| knots[index]) {
        Some(below) => {
            let above = knots.get(past).copied().unwrap_or(below);
            let t = (s_start - below[0]) / (above[0] - below[0]).max(f64::MIN_POSITIVE);
            below[1] + (above[1] - below[1]) * t.clamp(0.0, 1.0)
        }
        None => knots[0][1],
    };
    // A first frustum shorter than one registered XY pixel is merged into the
    // next (heads v19 fixes, L2): the knots within a pixel of the start are
    // dropped and the first frustum starts at the widest radius among them. The
    // head widens toward the joint, so the new chord stands on or above the
    // polyline it replaces at every station.
    let merged = knots
        .partition_point(|knot| knot[0] < s_start + config.pixel_pitch_mm)
        .min(knots.len() - 1)
        .max(past);
    let first_radius = knots[past..merged]
        .iter()
        .fold(first_radius, |widest, knot| widest.max(knot[1]));
    let clipped: Vec<[f64; 2]> = std::iter::once([s_start, first_radius])
        .chain(knots[merged..].iter().copied())
        .collect();
    push_head_chain(
        headed,
        config,
        &clipped,
        staged,
        first_radius > headed.start_cap_radius_mm,
        first_radius / headed.start_cap_radius_mm,
    )
}

/// The shortest link or cone span the lowering will draw, mm: a micron, the
/// same degenerate-frustum guard the head's other spans use.
const LINK_MIN_LENGTH_MM: f64 = 1.0e-6;

/// Draw the head's chain of `(s, r)` knots as straight frusta (the first the
/// contact taper, the rest members, the last ending exactly on `K`) and the
/// joint sphere at `K`. `chain[0]` stands at the head's start and the knots
/// rise strictly in `s`.
fn push_head_chain(
    headed: &HeadedTip<'_>,
    config: LoweringConfig,
    clipped: &[[f64; 2]],
    staged: &mut Vec<SupportPrimitive>,
    neck_wider_than_ball: bool,
    neck_ratio: f64,
) -> Option<HeadSegment> {
    let head = headed.head;
    let (start, joint) = (headed.start, head.joint_mm);
    let length = head.profile_knots_mm.last()?[0];
    let s_start = clipped.first()?[0];
    let on_axis = |s: f64| -> Option<[Finite; 3]> {
        let t = (s - s_start) / (length - s_start);
        finite3(std::array::from_fn(|axis| {
            start[axis] + (joint[axis] - start[axis]) * t
        }))
        .ok()
    };
    let last = clipped.len().saturating_sub(2);
    for (index, pair) in clipped.windows(2).enumerate() {
        let from = if index == 0 {
            finite3(start).ok()?
        } else {
            on_axis(pair[0][0])?
        };
        let to = if index == last {
            finite3(joint).ok()?
        } else {
            on_axis(pair[1][0])?
        };
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
            from,
            to,
            positive(pair[0][1]).ok()?,
            positive(pair[1][1]).ok()?,
            depth,
            FrustumRingNormal::SegmentAxis,
        ));
    }
    let spans = clipped.len() - 1;
    let end_radius = clipped.last()?[1];
    let geometry = SphereGeometry::joint(finite3(joint).ok()?);
    // A certified head's joint is the certified radius, the last knot's own;
    // the old law keeps the wider of the two.
    let radius = if head.certified {
        debug_assert!(
            (head.joint_radius_mm - end_radius).abs() <= 1.0e-9,
            "the certified joint is the cone's last radius"
        );
        positive(head.joint_radius_mm).ok()?
    } else {
        positive(head.joint_radius_mm.max(end_radius)).ok()?
    };
    let (start_mm, end_mm) = geometry.canonical_poles(radius).ok()?;
    staged.push(SupportPrimitive {
        segment_kind: SupportSegmentKind::JointSphere,
        start_mm,
        end_mm,
        start_radius_mm: radius,
        end_radius_mm: radius,
        contact_depth_mm: None,
        ring_normal: FrustumRingNormal::SegmentAxis,
        sphere_geometry: Some(geometry),
        seat_trim: None,
    });
    Some(HeadSegment {
        spans,
        neck_wider_than_ball,
        neck_ratio,
    })
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

/// The two registered readings [`drawn_spans`] finishes the chords with.
#[derive(Clone, Copy, Debug, PartialEq)]
struct DrawnSpanRules {
    /// The radius a cone may widen per mm of height, going down: the lean
    /// law's own `cot(theta_sat)`, `theta_sat = centre - transition / 2` from
    /// the registered overhang angle and weight transition (32.5 degrees at
    /// 45 and 25: 1.5697, which is 0.0785 mm of radius per 0.05 mm layer). The
    /// adapter's load-step cone reads the same two numbers
    /// (`plinth_sweep::lean::LeanLaw::max_step_mm`, `h * cot(theta_sat)`): the
    /// layer height cancels, so the slope needs none. Two readers of one law.
    cone_slope: f64,
    /// One registered XY pixel pitch: a frustum shorter than this is below
    /// what the printer resolves and is merged into its neighbour.
    min_span_mm: f64,
}

fn drawn_span_rules() -> Option<DrawnSpanRules> {
    static RULES: std::sync::OnceLock<Option<DrawnSpanRules>> = std::sync::OnceLock::new();
    *RULES.get_or_init(|| {
        let constants = m0_constants().ok()?;
        let placeholders = constants.placeholders();
        let saturated_deg = placeholders.overhang_angle_deg().diagnostic().get()
            - 0.5
                * placeholders
                    .overhang_weight_transition_deg()
                    .diagnostic()
                    .get();
        (saturated_deg > 0.0 && saturated_deg < 90.0).then_some(())?;
        Some(DrawnSpanRules {
            cone_slope: 1.0 / saturated_deg.to_radians().tan(),
            min_span_mm: crate::contact::registered_pixel_pitch_mm(constants),
        })
    })
}

/// Merge every span shorter than `min_len` into a neighbour (the one below, or
/// the one above for the last) as ONE straight span that stays on or above both
/// originals over their whole height.
///
/// The originals are straight between three heights: the top of the upper span,
/// the junction, and the bottom of the lower span. One line is on or above both
/// exactly when it is on or above the three values there (the junction's value
/// is the wider of the two radii that meet). Where the line between the outer
/// radii dips below the junction's, it is raised by turning it about the end
/// that must not move: the bottom end when the pair is not at the member's top
/// (the raised top then stands wider than the span above, a face that looks up
/// and is drawn as a cone by [`cone_the_shoulders`]), the top end when the pair
/// is at the member's top (the lower neighbour's top is raised to match, so no
/// face looks down). A pair that is the whole member moves both ends together.
fn merge_short_spans(mut spans: Vec<DrawnSpan>, min_len: f64) -> Vec<DrawnSpan> {
    let length = |span: &DrawnSpan| span.top_z_mm - span.bottom_z_mm;
    while spans.len() > 1 {
        let Some(short) = (0..spans.len())
            .filter(|&index| length(&spans[index]) < min_len)
            .min_by(|&a, &b| length(&spans[a]).total_cmp(&length(&spans[b])))
        else {
            break;
        };
        let upper_index = if short + 1 < spans.len() {
            short
        } else {
            short - 1
        };
        let (upper, lower) = (spans[upper_index], spans[upper_index + 1]);
        let (top_z, bottom_z, junction_z) = (upper.top_z_mm, lower.bottom_z_mm, upper.bottom_z_mm);
        let (mut top_r, mut bottom_r) = (upper.top_radius_mm, lower.bottom_radius_mm);
        let needed = upper.bottom_radius_mm.max(lower.top_radius_mm);
        let total = top_z - bottom_z;
        let above_junction = top_z - junction_z;
        let below_junction = junction_z - bottom_z;
        if total > 0.0 {
            let line_at_junction = top_r + (bottom_r - top_r) * (above_junction / total);
            let dip = needed - line_at_junction;
            if dip > needed * PROFILE_RELATIVE_SLACK {
                let first = upper_index == 0;
                let last = upper_index + 2 == spans.len();
                // A short span at the end that must not move has almost no lever:
                // turning the line about that end by `total / short` raises the
                // other end by thousands of times the dip (a 0.4 micron span at
                // the top of a 1.4 mm post drew a 41 mm radius, Cylinder with the
                // link, 2026-10-08). Under one `min_len` the pair is raised by
                // the dip instead, both ends together.
                if (first && last)
                    || (first && above_junction < min_len)
                    || (!first && below_junction < min_len)
                {
                    top_r += dip;
                    bottom_r += dip;
                } else if first {
                    bottom_r = top_r + (needed - top_r) * (total / above_junction);
                    if let Some(next) = spans.get_mut(upper_index + 2) {
                        next.top_radius_mm = next.top_radius_mm.max(bottom_r);
                    }
                } else {
                    top_r = bottom_r + (needed - bottom_r) * (total / below_junction);
                }
            }
        } else {
            top_r = top_r.max(bottom_r);
            bottom_r = top_r;
        }
        spans[upper_index] = DrawnSpan {
            top_z_mm: top_z,
            bottom_z_mm: bottom_z,
            top_radius_mm: top_r,
            bottom_radius_mm: bottom_r,
        };
        spans.remove(upper_index + 1);
    }
    spans
}

/// Draw every bare radius step, going down, as a cone: a span ending narrower
/// than the next one starts is cut `h` above the step, and a cone of at most
/// `slope` mm of radius per mm of height (and at least `min_len` long) runs from
/// the chord there to the wide radius. The cone sits ABOVE the step, where the
/// ledger asks for the thin value and the cone only adds section; below the
/// step the ledger asks for the wide value, which the cone has reached.
///
/// Steps are taken from the bottom up, so a span that has to give way to a
/// cone and so widens at its top passes the step on to the span above it.
/// Three cases, for the span above a step of height `length` and a cone that
/// needs `height` of it:
///
/// * the cone fits and leaves at least `min_len` of the span: cut there;
/// * it fits but the rest would be shorter than `min_len`, or it needs the whole
///   span: the whole span is the cone (from its own top radius to the wide one,
///   at the law's slope or shallower);
/// * it needs more than the span has: the span becomes the law's cone exactly
///   and its top, now wider, is a step for the span above. A span at the top of
///   the member keeps its own top radius and is drawn steeper than the law. The
///   profile is continuous in every case, never a flat shoulder.
fn cone_the_shoulders(mut spans: Vec<DrawnSpan>, slope: f64, min_len: f64) -> Vec<DrawnSpan> {
    let mut k = spans.len().saturating_sub(1);
    while k > 0 {
        k -= 1;
        let (above, below) = (spans[k], spans[k + 1]);
        let (thin, wide) = (above.bottom_radius_mm, below.top_radius_mm);
        let length = above.top_z_mm - above.bottom_z_mm;
        let continuous = above.bottom_z_mm.to_bits() == below.top_z_mm.to_bits();
        if !(continuous && wide > thin * (1.0 + PROFILE_RELATIVE_SLACK) && length > 0.0) {
            continue;
        }
        // dr/dh of the span above going up; the cone's radius at height h is
        // `thin + g h` on the chord, and must meet `wide` at the step.
        let g = (above.top_radius_mm - above.bottom_radius_mm) / length;
        let denominator = slope + g;
        let height = if denominator > 0.0 {
            ((wide - thin) / denominator).max(min_len)
        } else {
            f64::INFINITY
        };
        if height < length - min_len {
            let cut_z = above.bottom_z_mm + height;
            let chord = thin + g * height;
            spans[k].bottom_z_mm = cut_z;
            spans[k].bottom_radius_mm = chord;
            spans.insert(
                k + 1,
                DrawnSpan {
                    top_z_mm: cut_z,
                    bottom_z_mm: above.bottom_z_mm,
                    top_radius_mm: chord,
                    bottom_radius_mm: wide,
                },
            );
        } else if height <= length * (1.0 + PROFILE_RELATIVE_SLACK) || k == 0 {
            spans[k].bottom_radius_mm = wide;
        } else {
            spans[k].top_radius_mm = wide - slope * length;
            spans[k].bottom_radius_mm = wide;
            // The span's top is wider now: the step above it is the next one.
        }
    }
    spans
}

/// **THE DRAWN PROFILE OF ONE MEMBER**: the fewest straight spans, from
/// `from_z` down to `to_z`, whose section is at least the area law's own
/// section at every height ([`drawn_chords`]), finished by the two rules of the
/// 2026-10-08 junction round:
///
/// * **A bare radius step is a cone, not a shoulder.** Where the profile steps
///   wider going down and nothing lands there, the step is drawn as a cone at
///   the lean law's slope instead of a flat annulus (15 on the Cylinder at
///   v19).
/// * **A frustum shorter than one registered XY pixel is merged into its
///   neighbour** (5 on the Cylinder at v19, the shortest 0.001 mm).
///
/// The registry is read once. Unreadable, the chords are returned as they are.
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
    let spans = drawn_chords(
        profile,
        base_z,
        seat_z,
        from_z,
        to_z,
        start_radius,
        end_radius,
    );
    match drawn_span_rules() {
        Some(rules) => cone_the_shoulders(
            merge_short_spans(spans, rules.min_span_mm),
            rules.cone_slope,
            rules.min_span_mm,
        ),
        None => spans,
    }
}

/// The chords of [`drawn_spans`] before the cone and merge rules: the fewest
/// straight spans, from `from_z` (the top of the drawn shaft: the tip ball's centre, or the
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
/// **Ramps** (2026-10-07): a run of two or more consecutive widenings, which
/// is how the adapter writes a load-step collar (one radius per layer slot),
/// is drawn as one frustum from the thin value above it to the wide value at
/// its last rise, not as a disc per slot. See the comment at `ramp_cut`.
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
fn drawn_chords(
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
    // RAMPS (2026-10-07). A rise is an interior cut where the member widens
    // going down. A ramp is a maximal run of at least two consecutive rises:
    // the adapter's load-step collar, written as one radius per layer slot.
    // Left to the greedy alone, every slot of a ramp becomes its own disc,
    // because each shorter chord ends at the thin value above its cut. A
    // single rise is a flat-underside junction and keeps its shoulder.
    //
    // The shaft is split into regions at each ramp's top boundary (the top of
    // the last piece before its first rise, which keeps its own thin value)
    // and at its last rise; a region with no ramp is the plain shaft and runs
    // exactly the code it always did. Inside a ramp region a span that ends at
    // a ramp cut takes the wider of the two ledger values there, so the
    // surface stays continuous with the span below, which starts at it.
    let mut ramp_cut = vec![false; pieces + 1];
    let mut region_starts: Vec<usize> = vec![0];
    let mut cut = 1;
    while cut < pieces {
        let widens = |c: usize| need[c] > need[c - 1] * (1.0 + PROFILE_RELATIVE_SLACK);
        if !widens(cut) {
            cut += 1;
            continue;
        }
        let first_rise = cut;
        while cut < pieces && widens(cut) {
            cut += 1;
        }
        let last_rise = cut - 1;
        if last_rise > first_rise {
            ramp_cut[first_rise..=last_rise].fill(true);
            for boundary in [first_rise - 1, last_rise] {
                if region_starts.last() != Some(&boundary) {
                    region_starts.push(boundary);
                }
            }
        }
    }
    if region_starts.last() != Some(&pieces) {
        region_starts.push(pieces);
    }
    let radius_at_bottom = |cut: usize| {
        if cut == pieces {
            end_radius.max(need[pieces - 1])
        } else if ramp_cut[cut] {
            need[cut - 1].max(need[cut])
        } else {
            need[cut - 1]
        }
    };
    let mut spans = Vec::new();
    // Depth-first, top half first, so the spans come out top-down.
    let mut pending: Vec<(usize, usize)> = region_starts
        .windows(2)
        .map(|pair| (pair[0], pair[1]))
        .rev()
        .collect();
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
/// **The geometry is CUT, never the seat retracted** (owner ruling,
/// 2026-09-05; cut to the model mesh, Craig 2026-10-12). The tip goes to the
/// FULL depth the area term asks for, whatever the skin above it is. What
/// bounds the emitted GEOMETRY is the model solid itself: the emitter keeps
/// `ball ∖ model` and the piece of it that holds the shaft end
/// ([`crate::SeatTrim`]). Land-at-strike holds — nothing stands inside the
/// model or past its far side — by cutting the geometry rather than by
/// starving the contact. The seat plane returned here is data: the normal the
/// law used, the planned disc, and which shaft spans are cut at all.
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
            drawn_ball_radius_mm: 0.0,
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
    // CUT, NEVER RETRACT (owner ruling 2026-09-05; cut to the model mesh,
    // 2026-10-12). The ball is at the full depth the area law asked for; the
    // emitter keeps its part outside the model (`SeatTrim`). The PLANNED
    // contact is the ball's section in the seat plane, `sqrt(R² − d²)` for a
    // centre `d` on the support side: the disc a flat face takes. The real
    // contact is the model surface inside the ball, which the sweep measures
    // (`plinth_sweep::contact_growth`).
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
    // THE GROWN BALL (Craig 2026-10-12, area governs; the seat depth stays).
    // The ball the sweep asks for keeps the ungrown ball's DEEPEST point along
    // the seat's outward normal, so it reaches no deeper into the model on the
    // seat plane: it contains the ungrown ball and is tangent to it there. The
    // planned disc above stays the ungrown ball's: it is the area the seat was
    // sized for, and the sweep grows the ball until the real contact reaches it.
    tip_seat.drawn_ball_radius_mm = ball_radius_mm;
    let grown = arm
        .contact_ball_radius_mm
        .filter(|grown| grown.is_finite() && *grown > ball_radius_mm);
    let center = match grown {
        Some(grown) => {
            let normal = tip_seat.trim.map_or(unit, |trim| trim.outward_normal());
            tip_seat.drawn_ball_radius_mm = grown;
            std::array::from_fn(|index| center[index] + (grown - ball_radius_mm) * normal[index])
        }
        None => center,
    };
    (seated, center, tip_seat)
}

/// One model-landed arm's FOOT BALL ([`SweepArmLanding::Model`]): its centre,
/// the unit from it up the shaft, and the seat record (its trim's shaft
/// direction set to that unit).
struct FootBall {
    centre: [f64; 3],
    up: [f64; 3],
    seat: TipSeat,
}

/// **The foot ball of a model-landed arm, by the top tip's own derivation**
/// (lane 2, 2026-10-12, `MODEL-FEET.md` F1).
///
/// The foot IS a tip turned over: a ball of the owner's ChituBox size for the
/// foot's load-sized contact radius, driven INTO the landed face by the seat
/// law ([`seated_ball`]) and cut to the model mesh by the same
/// [`crate::SeatTrim`]. It is derived by handing [`seated_ball`] the arm with
/// its two ends swapped: the "seat" is where the axis crosses the top of the
/// strike lamina (the raster surface the landing stands on: the arm's base is
/// that lamina's bottom plane, one layer into the model), the "base" is the
/// post's top, and the carried face is the landed face. No second seat law,
/// ball factor or trim is written.
///
/// `None` where the arm has no length or the foot no usable radius.
fn model_foot_ball(arm: &SweepArm, foot: &ModelFoot, config: LoweringConfig) -> Option<FootBall> {
    let radius = foot.contact_radius_mm;
    if !(radius.is_finite() && radius > 0.0) {
        return None;
    }
    // The post's top: a headed arm's post starts at its carried joint.
    let top = arm
        .head
        .as_ref()
        .filter(|head| finite3(head.joint_mm).is_ok())
        .map_or(arm.seat_mm, |head| head.joint_mm);
    let along = geom_kernel::vec3::sub(top, arm.base_mm);
    let length = geom_kernel::vec3::norm(along);
    if !(length.is_finite() && length > ZERO_LENGTH_MM) {
        return None;
    }
    let up = along.map(|value| value / length);
    // Where the axis leaves the strike lamina: one layer up from the base, read
    // along the axis (a leaned arm crosses the lamina obliquely), never past
    // the post's top.
    let rise = if up[2] > 0.0 {
        (config.layer_height_mm.get() / up[2]).min(length)
    } else {
        0.0
    };
    let surface: [f64; 3] = std::array::from_fn(|axis| arm.base_mm[axis] + up[axis] * rise);
    let mirror = SweepArm {
        seat_mm: surface,
        base_mm: top,
        tip_radius_mm: radius,
        radius_profile_mm: Vec::new(),
        seat_normal_mm: foot.face_normal_mm,
        seat_face_point_mm: foot.face_point_mm,
        landing: SweepArmLanding::Landed,
        seats_into_model: true,
        head: None,
        contact_ball_radius_mm: None,
    };
    let ball_radius = config.sweep_ball_factor(radius) * radius;
    let (_, centre, mut seat) = seated_ball(&mirror, up, radius, ball_radius, config);
    // The shaft leaves the foot ball UP the arm: the emitter keeps the piece
    // of every cut record that holds that end.
    seat.trim = seat.trim.map(|trim| trim.with_shaft_unit(up));
    Some(FootBall { centre, up, seat })
}

/// The contact disc a seated tip's trim leaves in its seat plane.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TipContactDisc {
    /// The disc's centre: the ball centre projected onto the seat plane.
    pub centre_mm: [f64; 3],
    /// The seat face's OUTWARD unit normal (the plane's normal).
    pub outward_normal: [f64; 3],
    /// The PLANNED disc's radius, `sqrt(R₀² − d₀²)` for the UNGROWN ball
    /// (`R₀`, centre `d₀` on the support side); zero when that ball misses the
    /// plane. Its area is the contact the seat was sized for. The file's real
    /// contact is the model surface inside the drawn ball (the emitter cuts
    /// to the mesh, not to this plane), which the sweep measures and grows the
    /// ball for ([`SweepArm::contact_ball_radius_mm`]).
    pub radius_mm: f64,
    /// The emitted ball's own radius `R`: the ungrown ball, or the grown one.
    pub ball_radius_mm: f64,
    /// The UNGROWN ball's radius `R₀` (the owner's ChituBox ball for the tip
    /// size): the size every rule that is about the tip's own feature reads
    /// (the footprint rule's search radius), whatever the ball grew to.
    pub planned_ball_radius_mm: f64,
    /// The emitted ball's CENTRE, which is also where the drawn shaft starts
    /// (the taper runs from here to the landing). The sweep's head-join rule
    /// (2026-09-29) measures a trunk's shaft from this point, so it reads the
    /// same derivation the lowering draws from.
    pub ball_centre_mm: [f64; 3],
    /// Where the arm's POST starts: the ball centre for a straight arm, the
    /// carried joint `K` for a headed one ([`SweepArm::head`]). A reader that
    /// measures the post (the clearance census, the head-join zone) starts
    /// here, not at the ball.
    pub post_top_mm: [f64; 3],
    /// The head's joint radius, `None` for a straight arm.
    pub head_joint_radius_mm: Option<f64>,
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
            // A head's ball is seated along its own normal, not the axis.
            let headed = headed_tip(arm, config);
            let (center, tip_seat) = match &headed {
                Some(head) => (head.centre, head.tip_seat),
                None => {
                    let (_, center, tip_seat) =
                        seated_ball(arm, unit, arm.tip_radius_mm, ball_radius_mm, config);
                    (center, tip_seat)
                }
            };
            let trim = tip_seat.trim?;
            let normal = trim.outward_normal();
            let d = trim.support_side_distance_mm(center);
            Some(TipContactDisc {
                centre_mm: std::array::from_fn(|index| center[index] - d * normal[index]),
                outward_normal: normal,
                radius_mm: tip_seat.contact_disc_radius_mm,
                ball_radius_mm: tip_seat.drawn_ball_radius_mm,
                planned_ball_radius_mm: ball_radius_mm,
                ball_centre_mm: center,
                post_top_mm: headed.as_ref().map_or(center, |head| head.head.joint_mm),
                head_joint_radius_mm: headed.as_ref().map(|head| head.head.joint_radius_mm),
            })
        })
        .collect())
}

/// How far inside its own drawn shaft a seated tip's ball's LOWEST point
/// stands, mm (negative outside), on the arm exactly as the lowering draws it:
/// the ball the arm asks for ([`SweepArm::contact_ball_radius_mm`]), centred
/// where [`seated_ball`] or [`headed_tip`] puts it, against the shaft the ball
/// hands its load to (a straight arm's taper from the ball centre toward its
/// base at the tip's radius; a head's span from its start toward the joint
/// `K` at the link's radius, or the head's narrowest knot where no link is
/// drawn). The narrowest section that shaft has near the ball is used, so a
/// shaft that widens below can only carry more. `None` for an arm with no
/// seated ball.
fn ball_pole_margin_mm(arm: &SweepArm, config: LoweringConfig) -> Option<f64> {
    if !arm.seats_into_model {
        return None;
    }
    let delta = geom_kernel::vec3::sub(arm.base_mm, arm.seat_mm);
    let length = geom_kernel::vec3::norm(delta);
    let tip_radius = arm.tip_radius_mm;
    if !(length.is_finite() && length > ZERO_LENGTH_MM && tip_radius.is_finite() && tip_radius > 0.0)
    {
        return None;
    }
    let unit = delta.map(|value| value / length);
    let (centre, ball_radius, start, end, near_radius) = match headed_tip(arm, config) {
        Some(head) => {
            let link = head.head.link_mm > 0.0 && head.head.link_radius_mm > 0.0;
            let near = if link {
                head.head.link_radius_mm
            } else {
                head.head
                    .profile_knots_mm
                    .iter()
                    .map(|knot| knot[1])
                    .fold(f64::INFINITY, f64::min)
            };
            (
                head.centre,
                head.ball_radius_mm,
                head.start,
                head.head.joint_mm,
                near,
            )
        }
        None => {
            let ball_radius = config.sweep_ball_factor(tip_radius) * tip_radius;
            let (_, centre, seat) = seated_ball(arm, unit, tip_radius, ball_radius, config);
            (
                centre,
                seat.drawn_ball_radius_mm,
                centre,
                arm.base_mm,
                tip_radius,
            )
        }
    };
    let axis = geom_kernel::vec3::sub(end, start);
    let span = geom_kernel::vec3::norm(axis);
    if !(span.is_finite() && span > ZERO_LENGTH_MM && near_radius.is_finite()) {
        return None;
    }
    let along = axis.map(|value| value / span);
    let pole = [centre[0], centre[1], centre[2] - ball_radius];
    let reach = geom_kernel::vec3::dot(geom_kernel::vec3::sub(pole, start), along).clamp(0.0, span);
    let foot = geom_kernel::vec3::add(start, geom_kernel::vec3::scale(along, reach));
    let margin = near_radius - geom_kernel::vec3::dist(pole, foot);
    margin.is_finite().then_some(margin)
}

/// **The largest ball each seated tip may be drawn at and still print from its
/// own shaft** (joint review P1, 2026-10-12), in arm order, for a growth
/// ceiling per arm.
///
/// A ball's horizontal sections below its equator grow upward, each holding
/// the one below it, so the ONLY part of a ball that starts a new layer island
/// is its lowest point. That point prints from the solid under it when it lies
/// inside the shaft the ball stands on ([`ball_pole_margin_mm`]); outside it,
/// the slicer cures a patch with nothing beneath it, however well the finished
/// solid is connected higher up. A ball grown for contact moves out along its
/// seat normal, and on an oblique shaft its lowest point leaves the shaft:
/// Bat_Cat_Owl's 0.270 mm ball on a 0.097 mm taper, Deinonychus's 0.533 mm ball
/// on a 0.078 mm link (`joint/out-fix/final`, a new 97-pixel and a 634-pixel
/// island).
///
/// For each arm: the ungrown ball's radius `R₀` where `R₀`'s own lowest point
/// is already outside its shaft (growth would only move it further out), else
/// the largest radius in `[R₀, ceiling]` whose lowest point is inside it, by
/// bisection (a search resolution, it decides nothing the geometry does not).
/// `None` for an arm with no seated ball. Pure arithmetic per arm, one call
/// for the whole field.
///
/// # Errors
///
/// Only about the request, as [`lower_sweep_arms`].
pub fn carried_ball_radii_mm(
    arms: &[SweepArm],
    ceilings_mm: &[f64],
    layers: &LayerPlan,
) -> Result<Vec<Option<f64>>, SweepLoweringError> {
    /// Halvings of `[R₀, ceiling]`: far finer than one pixel.
    const BISECTIONS: usize = 48;
    let frame = BuildFrame::new(
        Finite::new(layers.z0_mm()).map_err(|_| SweepLoweringError::MissingLayerHeight)?,
        PositiveFinite::new(1.0).map_err(|_| SweepLoweringError::InvalidLimits)?,
    );
    let config = LoweringConfig::current(frame, layers, 1, 1)?;
    Ok(arms
        .iter()
        .zip(ceilings_mm)
        .map(|(arm, &ceiling)| {
            // The arm without its profile (no reader below needs it), its
            // ball set per probe.
            let mut probe = SweepArm {
                radius_profile_mm: Vec::new(),
                contact_ball_radius_mm: None,
                ..arm.clone()
            };
            let ungrown = config.sweep_ball_factor(arm.tip_radius_mm) * arm.tip_radius_mm;
            let mut carried = |radius: f64| -> Option<bool> {
                probe.contact_ball_radius_mm = (radius > ungrown).then_some(radius);
                ball_pole_margin_mm(&probe, config).map(|margin| margin >= 0.0)
            };
            if !carried(ungrown)? || !(ceiling.is_finite() && ceiling > ungrown) {
                return Some(ungrown);
            }
            if carried(ceiling)? {
                return Some(ceiling);
            }
            let (mut holds, mut fails) = (ungrown, ceiling);
            for _ in 0..BISECTIONS {
                let mid = 0.5 * (holds + fails);
                if carried(mid)? {
                    holds = mid;
                } else {
                    fails = mid;
                }
            }
            Some(holds)
        })
        .collect())
}

/// **The contact disc every model-landed arm's FOOT will deliver**
/// ([`SweepArmLanding::Model`]), by the lowering's own derivation
/// ([`model_foot_ball`]) without lowering anything (lane 2, 2026-10-12). In
/// arm order; `None` for an arm that does not land on the model with a foot.
///
/// The sweep measures the model surface inside each ball from this (the real
/// contact the file's foot gets, which prices its scar), so the ball it
/// measures is, by construction, the ball the file gets. `post_top_mm` is the
/// ball centre and `head_joint_radius_mm` is `None`: a foot has no head.
///
/// # Errors
///
/// Only about the request, as [`lower_sweep_arms`].
pub fn model_foot_discs(
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
            let SweepArmLanding::Model(foot) = arm.landing else {
                return None;
            };
            let ball = model_foot_ball(arm, &foot, config)?;
            let trim = ball.seat.trim?;
            let normal = trim.outward_normal();
            let d = trim.support_side_distance_mm(ball.centre);
            Some(TipContactDisc {
                centre_mm: std::array::from_fn(|index| ball.centre[index] - d * normal[index]),
                outward_normal: normal,
                radius_mm: ball.seat.contact_disc_radius_mm,
                ball_radius_mm: ball.seat.drawn_ball_radius_mm,
                planned_ball_radius_mm: ball.seat.drawn_ball_radius_mm,
                ball_centre_mm: ball.centre,
                post_top_mm: ball.centre,
                head_joint_radius_mm: None,
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
        // A junction hull carries no seat trim and is never graded against one.
        FrustumRingNormal::Junction(_) => None,
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
    // THE ARTICULATED HEAD: the ball is seated along the face normal, the head
    // runs to the carried joint, and the post starts there. The head's pieces
    // are staged apart first, so a head that cannot be drawn leaves the arm
    // straight and counted. `None` is the straight arm.
    let mut head_pieces: Vec<SupportPrimitive> = Vec::new();
    let headed = headed_tip(arm, config).and_then(|headed| {
        push_head_segment(&headed, config, &mut head_pieces).map(|segment| (headed, segment))
    });
    let head_drawn_straight = arm.head.is_some() && headed.is_none();
    let (seated_seat, center, seat) = if arm.seats_into_model {
        // THE SEAT and its trim: `seated_ball`, the one derivation.
        let (seated_seat, center_raw, mut tip_seat) = match &headed {
            Some((head, _)) => (head.seated, head.centre, head.tip_seat),
            None => seated_ball(arm, unit, tip_radius.get(), ball_radius.get(), config),
        };
        let ball_unit = headed.as_ref().map_or(unit, |(head, _)| head.unit);
        // The ball as drawn (grown where the sweep asked for more contact),
        // and the direction its shaft leaves it in: the emitter keeps the
        // piece of every cut record that holds that end.
        let Ok(ball_radius) = positive(tip_seat.drawn_ball_radius_mm) else {
            return ArmLowering::refused(ArmOutcome::InvalidGeometry);
        };
        tip_seat.trim = tip_seat.trim.map(|trim| trim.with_shaft_unit(ball_unit));
        let (Ok(center), Ok(unit_finite)) = (finite3(center_raw), finite3(ball_unit)) else {
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
    // The shaft's top: the ball centre, or for a head the carried joint `K`,
    // with the head drawn first and then the joint sphere. The post then starts
    // at `K` with the joint's radius, over a profile that spans base -> `K`.
    let (shaft_top, post_start_radius, profile_top_z, head_drawn) = match &headed {
        Some((head, segment)) => {
            let (Ok(joint), Ok(joint_radius)) = (
                finite3(head.head.joint_mm),
                positive(head.head.joint_radius_mm),
            ) else {
                return ArmLowering::refused(ArmOutcome::InvalidGeometry);
            };
            // THE LINK, MEASURED FROM THE DRAWN FRUSTA (Craig 2026-10-07): the
            // bare length where the drawn head is thinner than the contact
            // ball. v19 reported the planned gap between the two spheres
            // (median 0.96 mm) while the file drew the head wider than the
            // ball from the ball's surface on: a drawn link of 0.
            let head_frusta: Vec<([f64; 2], [f64; 2])> = head_pieces
                .iter()
                .filter(|piece| {
                    matches!(
                        piece.segment_kind,
                        SupportSegmentKind::ContactTaper | SupportSegmentKind::Member
                    )
                })
                .map(|piece| {
                    let along = |point: [Finite; 3]| {
                        geom_kernel::vec3::dot(
                            head.unit,
                            geom_kernel::vec3::sub(point.map(Finite::get), head.centre),
                        )
                    };
                    (
                        [along(piece.start_mm), piece.start_radius_mm.get()],
                        [along(piece.end_mm), piece.end_radius_mm.get()],
                    )
                })
                .collect();
            let stalk_mm = drawn_link_mm(&head_frusta, head.ball_radius_mm);
            staged.append(&mut head_pieces);
            let standoff_mm = geom_kernel::vec3::dot(
                head.unit,
                geom_kernel::vec3::sub(head.head.joint_mm, head.head.contact_mm),
            ) - head.head.joint_radius_mm;
            // The post's profile holds one slot per LAYER from the base up to
            // and including the joint's layer, so its top is that layer's top
            // boundary, not `K` (which stands inside it). Mapping the slots
            // onto base -> `K` squeezed every slot under a layer high on the
            // post: on the Cylinder at `149452e` a one-step cone read as two
            // steps in one layer (round-18 milestone, 2026-10-07). The shaft
            // still ends at `K`; `drawn_spans` reads the joint slot there.
            #[expect(
                clippy::cast_precision_loss,
                reason = "a post's slot count is bounded by the plan's layer count, exact in f64"
            )]
            let slots_top_z =
                base[2].get() + arm.radius_profile_mm.len() as f64 * config.layer_height_mm.get();
            (
                joint,
                joint_radius,
                slots_top_z.max(head.head.joint_mm[2]),
                Some(HeadDrawn {
                    stalk_mm,
                    standoff_mm,
                    neck_wider_than_ball: segment.neck_wider_than_ball,
                    neck_ratio: segment.neck_ratio,
                }),
            )
        }
        None => (center, tip_radius, arm.seat_mm[2], None),
    };
    let first_kind = if headed.is_some() {
        SupportSegmentKind::Member
    } else {
        SupportSegmentKind::ContactTaper
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
                tip_radius: post_start_radius,
                root_radius,
                ring_normal,
                first_kind,
                profile_top_z_mm: profile_top_z,
            },
            config,
            staged,
        )
    };
    let spans: usize;
    let mut boss = None;
    // Where the shaft frusta begin among this arm's records: after the ball,
    // the head and the joint sphere, and before the plate flare.
    let shaft_offset = staged.len();
    // A MODEL FOOT (lane 2, 2026-10-12): the ball the shaft narrows to, by the
    // top tip's own seat derivation. An arm whose foot cannot be built ends
    // flat on the model as before, and is counted.
    let model_foot = match arm.landing {
        SweepArmLanding::Model(foot) => model_foot_ball(arm, &foot, config),
        _ => None,
    };
    let model_foot_flat = matches!(arm.landing, SweepArmLanding::Model(_)) && model_foot.is_none();
    // The foot's records (cone, ball), appended after the shaft so the
    // junction pass's shaft window is untouched, and their drawn shape.
    let mut foot_pieces: Vec<SupportPrimitive> = Vec::new();
    let mut foot_drawn: Option<FootDrawn> = None;
    let outcome = match (arm.landing, &model_foot) {
        (SweepArmLanding::Model(_), Some(foot)) => {
            let (Ok(centre), Ok(up)) = (finite3(foot.centre), finite3(foot.up)) else {
                return ArmLowering::refused(ArmOutcome::InvalidGeometry);
            };
            let available = distance(shaft_top, centre);
            if available <= ZERO_LENGTH_MM {
                return ArmLowering::refused(ArmOutcome::ZeroLength);
            }
            let Ok(foot_radius) = positive(foot.seat.tip_radius_mm) else {
                return ArmLowering::refused(ArmOutcome::InvalidGeometry);
            };
            let Ok(foot_ball_radius) = positive(foot.seat.drawn_ball_radius_mm) else {
                return ArmLowering::refused(ArmOutcome::InvalidGeometry);
            };
            // THE CONE, AT THE LEAN LAW'S SLOPE. The shaft keeps its whole
            // drawn section (the area law plus the slenderness width) down to
            // where the cone starts, and narrows from its root to the foot's
            // load-sized contact over `(r_root - r_foot) / cot(theta_sat)`: the
            // cone's underside faces down, and the lean law's saturation angle
            // is the steepest slope it admits for an arm's own underside
            // (`drawn_span_rules`, one law with its other readers). A step under
            // one pixel of cone is no cone.
            let rules = drawn_span_rules();
            let min_span_mm = rules.map_or(ZERO_LENGTH_MM, |rules| rules.min_span_mm);
            let cone_mm = rules.map_or(0.0, |rules| {
                (root_radius.get() - foot_radius.get()).max(0.0) / rules.cone_slope
            });
            let wants_cone = cone_mm >= min_span_mm;
            let fits_cone = wants_cone && cone_mm + min_span_mm < available;
            let toward_top = geom_kernel::vec3::normalize(geom_kernel::vec3::sub(
                shaft_top.map(Finite::get),
                foot.centre,
            ))
            .unwrap_or(foot.up);
            let cone_top = if fits_cone {
                let Ok(point) = finite3(std::array::from_fn(|axis| {
                    foot.centre[axis] + toward_top[axis] * cone_mm
                })) else {
                    return ArmLowering::refused(ArmOutcome::InvalidGeometry);
                };
                point
            } else {
                centre
            };
            let Some(drawn) = shaft(shaft_top, cone_top, FrustumRingNormal::SegmentAxis, &mut staged)
            else {
                return ArmLowering::refused(ArmOutcome::InvalidGeometry);
            };
            spans = drawn;
            if fits_cone {
                foot_pieces.push(frustum(
                    SupportSegmentKind::Member,
                    cone_top,
                    centre,
                    root_radius,
                    foot_radius,
                    None,
                    FrustumRingNormal::SegmentAxis,
                ));
            }
            let Ok(geometry) = SphereGeometry::tip(centre, up) else {
                return ArmLowering::refused(ArmOutcome::InvalidGeometry);
            };
            let Ok((sphere_start, sphere_end)) = geometry.canonical_poles(foot_ball_radius) else {
                return ArmLowering::refused(ArmOutcome::InvalidGeometry);
            };
            foot_pieces.push(SupportPrimitive {
                segment_kind: SupportSegmentKind::ContactSphere,
                start_mm: sphere_start,
                end_mm: sphere_end,
                start_radius_mm: foot_ball_radius,
                end_radius_mm: foot_ball_radius,
                contact_depth_mm: None,
                ring_normal: FrustumRingNormal::SegmentAxis,
                sphere_geometry: Some(geometry),
                seat_trim: foot.seat.trim,
            });
            foot_drawn = Some(FootDrawn {
                seat: foot.seat,
                cone: fits_cone,
                cone_too_short: wants_cone && !fits_cone,
            });
            ArmOutcome::Built
        }
        (SweepArmLanding::Landed | SweepArmLanding::Model(_), _) => {
            if distance(shaft_top, base) <= ZERO_LENGTH_MM {
                return ArmLowering::refused(ArmOutcome::ZeroLength);
            }
            let Some(drawn) = shaft(shaft_top, base, FrustumRingNormal::SegmentAxis, &mut staged)
            else {
                return ArmLowering::refused(ArmOutcome::InvalidGeometry);
            };
            spans = drawn;
            ArmOutcome::Built
        }
        (SweepArmLanding::Plate, _) => {
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
            // A headed arm's post starts at the joint, so its boss is measured
            // from there.
            let terminal_z = if headed.is_some() {
                profile_top_z
            } else {
                seated_seat[2]
            };
            let boss_top = root_boss_top_at(
                terminal_z,
                [base[0], base[1]],
                config.root_boss.height_mm(),
                config,
            )
            .and_then(Result::ok);
            match boss_top {
                Some(top)
                    if distance(shaft_top, top) > ZERO_LENGTH_MM
                        && distance(top, foot_point) > ZERO_LENGTH_MM =>
                {
                    // THE OWNER'S CHITUBOX BOSS FOOT, SCALED TO THE ROOT
                    // (2026-09-30): the drawn root times his Heavy profile's
                    // boss-to-shaft ratio. The `RootFlare` record is the boss
                    // cone.
                    let factor = config.root_boss.factor();
                    let Ok(flare_radius) =
                        positive(config.root_boss.foot_radius_mm(root_radius.get()))
                    else {
                        return ArmLowering::refused(ArmOutcome::InvalidGeometry);
                    };
                    boss = Some((root_radius.get(), factor));
                    let Some(drawn) = shaft(shaft_top, top, FrustumRingNormal::PlateZ, &mut staged)
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
                    if distance(shaft_top, foot_point) <= ZERO_LENGTH_MM
                        || shaft_top[2].get() <= foot_z.get()
                    {
                        return ArmLowering::refused(ArmOutcome::ZeroLength);
                    }
                    let Some(drawn) = shaft(
                        shaft_top,
                        foot_point,
                        FrustumRingNormal::PlateZ,
                        &mut staged,
                    ) else {
                        return ArmLowering::refused(ArmOutcome::InvalidGeometry);
                    };
                    spans = drawn;
                    ArmOutcome::BuiltWithoutFlare
                }
            }
        }
    };
    // THE SHAFT IS CUT TO THE SAME MODEL (fix/tip-trim, 2026-09-28; every
    // span, review-fixes 2026-09-29; the mesh, not a plane, 2026-10-12). The
    // taper starts at the ball centre, and on a leaned arm or under a sloped
    // face its start cap can reach into the model where the ball does ("a
    // chunk of post", tip-clip.md). The same model difference and the same
    // shaft-end piece rule apply.
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
    // THE FOOT IS CUT TO THE MODEL MESH like the tip (lane 2, 2026-10-12): its
    // ball and cone carry the foot's trim, and so does every shaft span below
    // the tip's own trimmed ones whose solid reaches the model side of the
    // landed face's plane (a leaned shaft's last span under a sloped face). A
    // span already cut for the tip keeps the tip's trim: the model difference
    // is the same solid either way, and only the kept piece differs.
    if let Some(trim) = foot_drawn.and_then(|foot| foot.seat.trim) {
        for primitive in &mut staged[shaft_offset..] {
            let shaft_span = matches!(
                primitive.segment_kind,
                SupportSegmentKind::ContactTaper | SupportSegmentKind::Member
            );
            if shaft_span
                && primitive.seat_trim.is_none()
                && frustum_reaches_model_side(primitive, trim)
            {
                primitive.seat_trim = Some(trim);
                members_trimmed += 1;
            }
        }
        for piece in &mut foot_pieces {
            piece.seat_trim = Some(trim);
        }
    }
    staged.append(&mut foot_pieces);
    // The emit boundary's own contract, asked HERE, so one bad arm is dropped
    // and counted rather than failing the whole field at
    // `SupportedModel::from_parts` with nothing said about which arm did it.
    if staged
        .iter()
        .any(|primitive| !primitive.is_semantically_valid())
    {
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
        SweepArmLanding::Landed | SweepArmLanding::Model(_) => None,
    };
    out.append(&mut staged);
    ArmLowering {
        outcome,
        seat,
        foot,
        spans,
        members_trimmed,
        without_profile: arm.radius_profile_mm.is_empty(),
        boss,
        head: head_drawn,
        head_drawn_straight,
        shaft: (shaft_offset, spans),
        model_foot: foot_drawn,
        model_foot_flat,
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
        FrustumRingNormal::Junction(_) => None,
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
    /// The radius the top span starts at: the tip's, or a head joint's.
    tip_radius: PositiveFinite,
    root_radius: PositiveFinite,
    ring_normal: FrustumRingNormal,
    /// The first span's kind: the arm's `ContactTaper`, or a `Member` for the
    /// post below a head (the taper is the head's segment above it).
    first_kind: SupportSegmentKind,
    /// The height the radius profile's top slot ends at: the seat for a
    /// straight arm, the joint for a headed one.
    profile_top_z_mm: f64,
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
        shaft.profile_top_z_mm,
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
        let (kind, depth) = if index == 0 && shaft.first_kind == SupportSegmentKind::ContactTaper {
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
            head: None,
            contact_ball_radius_mm: None,
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
            head: None,
            contact_ball_radius_mm: None,
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
        // "Short" means a run above the raft under twice the boss height, so
        // the boss top sits half-way up it and the seat depth moves it. The
        // sweep's boss height is the owner's 1.0 mm (`CHITUBOX_BOSS_HEIGHT_MM`,
        // 2026-09-30), so every short post below stands 1.0 mm lower than it
        // did under the legacy 1.5 mm flare length (the reviewer's 3.05 mm
        // post is 2.05 mm here); at the old heights the boss top is the full
        // 1.0 mm and no seat moves it.
        let cases: Vec<(&str, SweepArm, f64)> = vec![
            // The reviewer's post, 1.0 mm shorter: 0.05 mm tip, normal unmeasured.
            (
                "short vertical, unmeasured",
                plate_arm([0.0, 0.0, 2.05], [0.0, 0.0], 0.0, 0.05, [0.0; 3]),
                0.0,
            ),
            (
                "short vertical, flat face",
                plate_arm([0.0, 0.0, 2.05], [0.0, 0.0], 0.0, 0.05, down),
                0.0,
            ),
            (
                "short vertical, 35 deg face",
                plate_arm([0.0, 0.0, 2.0], [0.0, 0.0], 0.0, 0.3, tilted),
                0.0,
            ),
            (
                "short leaned, 35 deg face",
                plate_arm([0.0, 0.0, 2.0], [0.8, 0.0], 0.0, 0.1, tilted),
                0.0,
            ),
            (
                "short, plate at z = 7.3",
                plate_arm([1.0, 2.0, 9.35], [1.0, 2.0], 7.3, 0.05, down),
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
                    ..plate_arm([0.0, 0.0, 1.9], [0.0, 0.0], 0.0, 0.1, [0.0; 3])
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

    /// The shaft's two ends as the file has them: the start of its first
    /// shaft record (the taper, or the first chained member) and the end of
    /// its last one, read from the lowered primitives of the arm alone.
    fn drawn_shaft_ends(arm: &SweepArm, plate_z: f64) -> ([f64; 3], [f64; 3]) {
        let layers = LayerPlan::new(plate_z, vec![H; 400]).expect("a uniform plan");
        let frame = BuildFrame::new(
            Finite::new(plate_z).expect("a finite plate"),
            PositiveFinite::new(5.0).expect("a positive lift"),
        );
        let lowering =
            lower_sweep_arms(std::slice::from_ref(arm), frame, &layers, 100_000, 1 << 30)
                .expect("the lowering refuses only about the request");
        let shaft: Vec<_> = lowering
            .primitives
            .iter()
            .filter(|primitive| {
                matches!(
                    primitive.segment_kind,
                    SupportSegmentKind::ContactTaper | SupportSegmentKind::Member
                )
            })
            .collect();
        let point = |p: &[Finite; 3]| [p[0].get(), p[1].get(), p[2].get()];
        let first = shaft.first().expect("a drawn shaft");
        let last = shaft.last().expect("a drawn shaft");
        (point(&first.start_mm), point(&last.end_mm))
    }

    /// The door's shaft line IS the drawn shaft, for a leaning post above all
    /// (fix/morel-long-posts, 2026-10-02): its bottom is the boss top over the
    /// FOOT, not the planned axis at that height, and so a member tied to the
    /// post meets the solid the file draws only if it reads this line.
    #[test]
    fn the_door_reads_the_shaft_line_the_lowering_draws() {
        let door = SweepRootFlare::registered(H).expect("the registry");
        let down = [0.0, 0.0, -1.0];
        let tilted = {
            let (sin, cos) = 30.0_f64.to_radians().sin_cos();
            [-sin, 0.0, -cos]
        };
        let cases: Vec<(&str, SweepArm, f64)> = vec![
            // The Morel base's post 116 in for-craig-v10: 6.15 mm out over
            // 10.46 mm, a 0.086 mm tip.
            (
                "tall leaned (Morel post 116)",
                plate_arm([-2.69, -10.54, 10.7], [-7.94, -13.79], 0.0, 0.086, tilted),
                0.0,
            ),
            (
                "tall leaned, plate at z = 7.3",
                plate_arm([0.0, 0.0, 15.3], [3.0, 1.0], 7.3, 0.12, down),
                7.3,
            ),
            (
                "tall vertical",
                plate_arm([0.0, 0.0, 8.0], [0.0, 0.0], 0.0, 0.2, down),
                0.0,
            ),
            (
                "short leaned",
                plate_arm([0.0, 0.0, 2.0], [0.8, 0.0], 0.0, 0.1, tilted),
                0.0,
            ),
            (
                "tall leaned mast-like (no ball)",
                SweepArm {
                    seats_into_model: false,
                    ..plate_arm([0.0, 0.0, 6.0], [1.5, 0.0], 0.0, 0.1, [0.0; 3])
                },
                0.0,
            ),
        ];
        let mut off_the_planned_axis = 0;
        for (what, arm, plate_z) in &cases {
            let (top, bottom) = drawn_shaft_ends(arm, *plate_z);
            let (door_top, door_bottom) = door
                .plate_shaft_of(arm, *plate_z)
                .unwrap_or_else(|| panic!("{what}: the door found no shaft"));
            for (name, drawn, answered) in [("top", top, door_top), ("bottom", bottom, door_bottom)]
            {
                let gap = (0..3)
                    .map(|i| (drawn[i] - answered[i]).powi(2))
                    .sum::<f64>()
                    .sqrt();
                assert!(
                    gap < 1e-6,
                    "{what}: the door's shaft {name} {answered:?} is {gap:.3e} mm from the \
                     file's {drawn:?}"
                );
            }
            // Where the drawn bottom leaves the PLANNED axis (seat to base on
            // the plate) at the same height: the defect this door exists for.
            let t = (arm.seat_mm[2] - door_bottom[2]) / (arm.seat_mm[2] - arm.base_mm[2]);
            let planned: [f64; 3] =
                std::array::from_fn(|i| arm.seat_mm[i] + t * (arm.base_mm[i] - arm.seat_mm[i]));
            let off = geom_kernel::vec3::dist(
                [planned[0], planned[1], 0.0],
                [door_bottom[0], door_bottom[1], 0.0],
            );
            if off > 0.05 {
                off_the_planned_axis += 1;
            }
        }
        // Anti-vacuity: the four leaning posts draw their shaft off the
        // planned axis; the vertical post does not.
        assert_eq!(
            off_the_planned_axis, 4,
            "{off_the_planned_axis} shafts left the planned axis"
        );
        assert!(door
            .plate_shaft_of(
                &SweepArm {
                    landing: SweepArmLanding::Landed,
                    ..plate_arm([0.0, 0.0, 8.0], [0.0, 0.0], 0.0, 0.2, down)
                },
                0.0
            )
            .is_none());
    }
}

/// AN ARTICULATED HEAD (Craig 2026-10-06): the lowering draws a ball seated
/// along the head's normal, a head to the CARRIED joint, the joint sphere and
/// the post from it. Built by hand on a 41.3 degree face with the head's
/// section from the law (`r(s)^3 = (r_t^2/c)(r + 4s)`, c = cos 57.5 deg).
#[cfg(test)]
mod headed_arm_tests {
    use super::{
        lower_one_sweep_arm, lower_sweep_arms, tip_shaft_seat_bites, BuildFrame, Finite,
        LoweringConfig, PositiveFinite, SupportPrimitive, SupportSegmentKind, SweepArm,
        SweepArmLanding, SweepHead,
    };
    use plinth_core::LayerPlan;

    const FACE_DEG: f64 = 41.3;
    const GAP_MM: f64 = 0.3;
    const TIP_MM: f64 = 0.05;

    fn cos_lean() -> f64 {
        57.5_f64.to_radians().cos()
    }

    /// The root `r >= ledger` of `r^3 = ledger^2 r + 4 tip^2 lever`, by
    /// bisection (plinth-sweep's `heads` module owns the production law; this
    /// crate sits below it).
    fn section(ledger: f64, tip: f64, lever: f64) -> f64 {
        let excess = |r: f64| r * (r * r - ledger * ledger) - 4.0 * tip * tip * lever;
        let (mut low, mut high) = (ledger, ledger + (4.0 * tip * tip * lever).cbrt());
        for _ in 0..100 {
            let mid = 0.5 * (low + high);
            if excess(mid) >= 0.0 {
                high = mid;
            } else {
                low = mid;
            }
        }
        high
    }

    fn unit() -> [f64; 3] {
        let (sin, cos) = FACE_DEG.to_radians().sin_cos();
        [sin, 0.0, -cos]
    }

    /// `(r_K, |DK|)`: the joint's cubic `r_K^3 = (r_t^2/c)(r_K + 4(g + r_K))`.
    fn joint() -> (f64, f64) {
        let ledger = TIP_MM / cos_lean().sqrt();
        let (mut low, mut high) = (ledger, 1.0);
        let excess = |r: f64| r * r * r - ledger * ledger * (r + 4.0 * (GAP_MM + r));
        for _ in 0..100 {
            let mid = 0.5 * (low + high);
            if excess(mid) >= 0.0 {
                high = mid;
            } else {
                low = mid;
            }
        }
        (high, GAP_MM + high)
    }

    /// The head's law at the final tip `tip` and distance `s` from `D`.
    fn law(tip: f64, s: f64) -> f64 {
        let ledger = tip / cos_lean().sqrt();
        section(ledger, ledger, s)
    }

    /// The knots of a head of length `length`: the law's tangents at
    /// `s_i = L (i/3)^3` meeting each other, between the exact ends. A fixture
    /// input only: the production polyline is `plinth_sweep::heads::
    /// head_polyline_mm`, tested against the law and the plan's numbers there.
    fn knots(tip: f64, length: f64) -> Vec<[f64; 2]> {
        let a = tip * tip / cos_lean();
        let tangents: Vec<[f64; 3]> = (0..4_i32)
            .map(|index| {
                let s = length * (f64::from(index) / 3.0).powi(3);
                let r = law(tip, s);
                [s, r, 4.0 * a / (3.0 * r * r - a)]
            })
            .collect();
        let mut out = vec![[0.0, law(tip, 0.0)]];
        for pair in tangents.windows(2) {
            // Each tangent is (s, r, slope); the two lines meet where
            // r_lo + m_lo (s - s_lo) = r_hi + m_hi (s - s_hi).
            let [s_lo, r_lo, m_lo] = pair[0];
            let [s_hi, r_hi, m_hi] = pair[1];
            let s = (r_hi - r_lo + m_lo * s_lo - m_hi * s_hi) / (m_lo - m_hi);
            out.push([s, r_lo + m_lo * (s - s_lo)]);
        }
        out.push([length, law(tip, length)]);
        out
    }

    fn head(contact: [f64; 3]) -> SweepHead {
        let (_, length) = joint();
        head_with(TIP_MM, length, contact)
    }

    /// A head of the final tip `tip` and length `|DK| = length`, its joint the
    /// law's own radius there.
    fn head_with(tip: f64, length: f64, contact: [f64; 3]) -> SweepHead {
        let u = unit();
        SweepHead {
            contact_mm: contact,
            unit: u,
            joint_mm: std::array::from_fn(|axis| contact[axis] + length * u[axis]),
            joint_radius_mm: law(tip, length),
            gap_mm: GAP_MM,
            profile_knots_mm: knots(tip, length),
            link_mm: 0.0,
            link_radius_mm: 0.0,
            certified: false,
        }
    }

    /// A landed arm: the seat at `seat`, the head's contact centre at
    /// `contact`, a vertical post from the carried joint down to z = 2.
    fn arm(seat: [f64; 3], contact: [f64; 3]) -> SweepArm {
        let (_, length) = joint();
        arm_with(TIP_MM, length, seat, contact)
    }

    fn arm_with(tip: f64, length: f64, seat: [f64; 3], contact: [f64; 3]) -> SweepArm {
        let head = head_with(tip, length, contact);
        let base = [head.joint_mm[0], head.joint_mm[1], 2.0];
        SweepArm {
            seat_mm: seat,
            base_mm: base,
            tip_radius_mm: tip,
            radius_profile_mm: vec![head.joint_radius_mm + 0.05; 150],
            seat_normal_mm: unit(),
            seat_face_point_mm: Some(contact),
            landing: SweepArmLanding::Landed,
            seats_into_model: true,
            head: Some(head),
            contact_ball_radius_mm: None,
        }
    }

    fn layers() -> LayerPlan {
        LayerPlan::new(0.0, vec![0.05; 400]).expect("a uniform layer plan")
    }

    fn frame() -> BuildFrame {
        BuildFrame::new(
            Finite::new(0.0).expect("finite"),
            PositiveFinite::new(1.0).expect("positive"),
        )
    }

    fn lower(arm: &SweepArm) -> Vec<SupportPrimitive> {
        let layers = layers();
        let config = LoweringConfig::current(frame(), &layers, 1, 1).expect("config");
        let mut out = Vec::new();
        let lowered = lower_one_sweep_arm(arm, config, 0.0, &mut out);
        assert!(lowered.head.is_some(), "the head was drawn as a head");
        out
    }

    fn point(p: [Finite; 3]) -> [f64; 3] {
        p.map(Finite::get)
    }

    #[test]
    #[expect(
        clippy::float_cmp,
        reason = "the joint and the post ends are CARRIED points, never re-derived, so they are \
                  pinned bit for bit"
    )]
    fn a_headed_arm_is_drawn_ball_head_joint_post_to_the_carried_joint() {
        let seat = [0.0, 0.0, 10.0];
        // The seat is moved 0.01 mm off the contact centre: the carried joint
        // must not follow the seated ball.
        let moved = [0.01, 0.0, 10.0];
        for (arm, what) in [
            (arm(seat, seat), "seat on the contact centre"),
            (arm(moved, seat), "seat moved 0.01 mm"),
        ] {
            let drawn = lower(&arm);
            let head = arm.head.as_ref().expect("a head");
            let k = head.joint_mm;
            let n = unit();
            // The ball first, its axis parallel to u.
            let ball = &drawn[0];
            assert_eq!(
                ball.segment_kind,
                SupportSegmentKind::ContactSphere,
                "{what}"
            );
            let (centre, axis) = ball
                .sphere_geometry
                .as_ref()
                .and_then(crate::SphereGeometry::tip_authority)
                .expect("a tip ball");
            for i in 0..3 {
                assert!(
                    (axis[i].get() - n[i]).abs() < 1e-12,
                    "{what}: ball axis {axis:?}"
                );
            }
            let centre = point(centre);
            // The joint sphere sits AT the carried K, radius r_K.
            let joint_at = drawn
                .iter()
                .position(|p| p.segment_kind == SupportSegmentKind::JointSphere)
                .expect("a joint sphere");
            let joint = &drawn[joint_at];
            assert!((joint.start_radius_mm.get() - head.joint_radius_mm).abs() < 1e-12);
            let drawn_k = point(
                joint
                    .sphere_geometry
                    .as_ref()
                    .and_then(crate::SphereGeometry::joint_center)
                    .expect("a joint centre"),
            );
            assert_eq!(drawn_k, k, "{what}: the joint is the carried K");
            // The head's taper and member spans end exactly at K.
            let neck = &drawn[1..joint_at];
            assert!(!neck.is_empty());
            assert_eq!(neck[0].segment_kind, SupportSegmentKind::ContactTaper);
            // (On the ball centre, or on the plane through it where the raster
            // seat puts the centre a rounding under the plane.)
            let start = point(neck[0].start_mm);
            for (axis, centre_at) in centre.iter().enumerate() {
                assert!(
                    (start[axis] - centre_at).abs() < 1e-9,
                    "{what}: the head starts off the ball"
                );
            }
            assert!(neck[1..]
                .iter()
                .all(|p| p.segment_kind == SupportSegmentKind::Member));
            assert_eq!(point(neck.last().expect("a neck").end_mm), k, "{what}");
            // The post: Member records from K to the base.
            let post = &drawn[joint_at + 1..];
            assert!(post
                .iter()
                .all(|p| p.segment_kind == SupportSegmentKind::Member));
            assert_eq!(point(post[0].start_mm), k, "{what}");
            assert_eq!(
                point(post.last().expect("a post").end_mm),
                arm.base_mm,
                "{what}"
            );
            // The seat trim cuts no head piece.
            let bite = tip_shaft_seat_bites(std::slice::from_ref(&arm), &layers())
                .expect("a valid request")[0]
                .expect("measured")
                .bite_mm;
            assert!(
                bite.abs() < 1e-12,
                "{what}: the trim bites the head: {bite}"
            );
            // The counters: one head, stand-off g (the joint has not grown),
            // none drawn straight.
            let lowering = lower_sweep_arms(
                std::slice::from_ref(&arm),
                frame(),
                &layers(),
                100_000,
                1 << 30,
            )
            .expect("a valid request");
            assert_eq!(lowering.tips_headed, 1, "{what}");
            assert_eq!(lowering.heads_drawn_straight, 0, "{what}");
            assert_eq!(lowering.head_standoff_mm.len(), 1);
            assert!(
                (lowering.head_standoff_mm[0] - GAP_MM).abs() < 1e-9,
                "{what}: stand-off {}",
                lowering.head_standoff_mm[0]
            );
            assert_eq!(lowering.head_stalk_mm.len(), 1);
            assert!(lowering.head_stalk_mm[0].is_finite());
        }
    }

    /// THE LINK (Craig 2026-10-08): the head is the ball, a bare cylinder of
    /// the link's radius that is `link_mm` long from where it leaves the ball,
    /// then a cone that starts at the link's radius (no step) and ends at the
    /// joint's. The measured link is the carried one, the cylinder is the
    /// contact taper, and the link is thinner than the ball.
    #[test]
    fn a_linked_head_is_the_ball_a_bare_cylinder_of_the_link_length_and_a_cone() {
        let (tip, length, link) = (0.1, 1.2, 0.3);
        let contact = [0.0, 0.0, 10.0];
        let mut arm = arm_with(tip, length, contact, contact);
        let head = arm.head.as_mut().expect("a head");
        head.link_mm = link;
        head.link_radius_mm = tip;
        // The cone: from the link's planned far end at the link's radius to
        // the joint's own radius.
        head.profile_knots_mm = vec![[0.5, tip], [length, head.joint_radius_mm]];
        let layers = layers();
        let config = LoweringConfig::current(frame(), &layers, 1, 1).expect("config");
        let mut out = Vec::new();
        let lowered = lower_one_sweep_arm(&arm, config, 0.0, &mut out);
        let drawn = lowered.head.expect("the head was drawn as a head");
        let joint_at = out
            .iter()
            .position(|p| p.segment_kind == SupportSegmentKind::JointSphere)
            .expect("a joint sphere");
        let neck = &out[1..joint_at];
        assert_eq!(neck.len(), 2, "the link, then one cone frustum");
        assert_eq!(neck[0].segment_kind, SupportSegmentKind::ContactTaper);
        let radius = |p: &SupportPrimitive| (p.start_radius_mm.get(), p.end_radius_mm.get());
        assert_eq!(radius(&neck[0]), (tip, tip), "the link is a cylinder of the link radius");
        assert!(
            (neck[1].start_radius_mm.get() - tip).abs() < 1e-12,
            "the cone starts at the link radius, no step"
        );
        assert!((neck[1].end_radius_mm.get() - arm.head.as_ref().expect("head").joint_radius_mm).abs() < 1e-12);
        assert!((drawn.stalk_mm - link).abs() < 1e-6, "measured link {}", drawn.stalk_mm);
        assert!(drawn.neck_ratio < 1.0 && !drawn.neck_wider_than_ball, "link thinner than the ball");
    }

    /// THE SMOOTH HEAD (v18 feedback B, 2026-10-07): the head is at most four
    /// frusta that share their end radii, never go under the law, hold at most
    /// 1.04 of the law's volume, and end at the joint's own radius. The v18 head
    /// was about fourteen one-layer discs that jumped in radius at every layer.
    #[test]
    fn the_head_is_a_few_smooth_frusta_on_or_above_the_law_ending_at_the_joint() {
        let length = 1.344;
        let u = unit();
        for tip in [0.05, 0.115] {
            let contact = [0.0, 0.0, 10.0];
            let arm = arm_with(tip, length, contact, contact);
            let drawn = lower(&arm);
            let joint_at = drawn
                .iter()
                .position(|p| p.segment_kind == SupportSegmentKind::JointSphere)
                .expect("a joint sphere");
            let neck = &drawn[1..joint_at];
            assert!(
                (1..=4).contains(&neck.len()),
                "tip {tip}: {} head frusta, at most four",
                neck.len()
            );
            for pair in neck.windows(2) {
                assert!(
                    (pair[0].end_radius_mm.get() - pair[1].start_radius_mm.get()).abs() < 1e-12,
                    "tip {tip}: a step from {} to {}",
                    pair[0].end_radius_mm.get(),
                    pair[1].start_radius_mm.get()
                );
            }
            let head = arm.head.as_ref().expect("a head");
            assert!(
                (neck.last().expect("a neck").end_radius_mm.get() - head.joint_radius_mm).abs()
                    < 1e-9,
                "tip {tip}: the last ring is the joint's radius"
            );
            // The drawn radius at s along u from D, by the frusta's own ends.
            let along = |p: [Finite; 3]| -> f64 {
                let p = point(p);
                (0..3).map(|axis| (p[axis] - contact[axis]) * u[axis]).sum()
            };
            let ends: Vec<(f64, f64)> =
                std::iter::once((along(neck[0].start_mm), neck[0].start_radius_mm.get()))
                    .chain(
                        neck.iter()
                            .map(|p| (along(p.end_mm), p.end_radius_mm.get())),
                    )
                    .collect();
            let (s_start, s_end) = (ends[0].0, ends[ends.len() - 1].0);
            assert!(
                (s_end - length).abs() < 1e-9,
                "tip {tip}: the head ends at K"
            );
            let drawn_at = |s: f64| -> f64 {
                let pair = ends
                    .windows(2)
                    .find(|w| s <= w[1].0)
                    .expect("s lies in the head");
                pair[0].1 + (pair[1].1 - pair[0].1) * (s - pair[0].0) / (pair[1].0 - pair[0].0)
            };
            for step in 0..=200 {
                let s = s_start + (s_end - s_start) * f64::from(step) / 200.0;
                let (got, want) = (drawn_at(s), law(tip, s));
                assert!(
                    got >= want - 1e-9,
                    "tip {tip}, s {s}: drawn {got} under the law {want}"
                );
            }
            // Volumes over [s_start, L]: the frusta against the law (Simpson).
            let drawn_volume: f64 = neck
                .iter()
                .map(|p| {
                    let (r0, r1) = (p.start_radius_mm.get(), p.end_radius_mm.get());
                    let h: f64 = (0..3)
                        .map(|axis| (point(p.end_mm)[axis] - point(p.start_mm)[axis]).powi(2))
                        .sum::<f64>()
                        .sqrt();
                    // The frustum volume, pi h (r0^2 + r0 r1 + r1^2) / 3.
                    std::f64::consts::PI * h * (r0 * (r0 + r1) + r1 * r1) / 3.0
                })
                .sum();
            let steps = 2000;
            let dz = (s_end - s_start) / f64::from(steps);
            let law_volume: f64 = (0..steps)
                .map(|i| {
                    let s = s_start + dz * (f64::from(i) + 0.5);
                    std::f64::consts::PI * law(tip, s).powi(2) * dz
                })
                .sum();
            assert!(
                drawn_volume <= 1.04 * law_volume && drawn_volume >= law_volume - 1e-9,
                "tip {tip}: drawn {drawn_volume} mm3 against the law's {law_volume}"
            );
        }
    }

    #[test]
    fn a_head_that_cannot_be_drawn_is_drawn_straight_and_counted() {
        let seat = [0.0, 0.0, 10.0];
        let mut broken = arm(seat, seat);
        broken.head.as_mut().expect("a head").joint_mm[0] = f64::NAN;
        let lowering = lower_sweep_arms(&[broken], frame(), &layers(), 100_000, 1 << 30)
            .expect("a valid request");
        assert_eq!(lowering.tips_headed, 0);
        assert_eq!(lowering.heads_drawn_straight, 1);
    }
}

#[cfg(test)]
mod drawn_link_tests {
    use super::drawn_link_mm;

    const BALL: f64 = 0.15;

    #[test]
    fn a_head_as_wide_as_the_ball_from_its_surface_on_has_no_link() {
        // v19's heads: the first frustum starts at the ball's centre at a
        // radius at or over the ball's own.
        for first in [BALL, 1.2 * BALL, 3.0 * BALL] {
            let frusta = [
                ([0.0, first], [1.0, first + 0.2]),
                ([1.0, first + 0.2], [1.5, 0.9]),
            ];
            assert!(
                drawn_link_mm(&frusta, BALL).abs() < 1e-12,
                "first radius {first}"
            );
        }
    }

    #[test]
    fn the_link_of_a_ball_a_cylinder_and_a_cone_is_the_bare_length_from_the_ball() {
        // A cylinder of radius 0.05 from the ball's centre to 1.0, then a cone
        // widening to 0.40 at 1.5. The cylinder leaves the ball where
        // r² + a² = R², at sqrt(R² - r²); the link runs to where the radius
        // first rises, at 1.0.
        let r = 0.05;
        let frusta = [([0.0, r], [1.0, r]), ([1.0, r], [1.5, 0.40])];
        let exit = (BALL * BALL - r * r).sqrt();
        let link = drawn_link_mm(&frusta, BALL);
        assert!((link - (1.0 - exit)).abs() < 1e-12, "{link} vs {}", 1.0 - exit);
        // A step up to the ball's own radius where the cone starts changes
        // nothing: the link is the leading constant-radius run.
        let abrupt = [([0.0, r], [1.0, r]), ([1.0, BALL], [1.5, 0.40])];
        let link = drawn_link_mm(&abrupt, BALL);
        assert!((link - (1.0 - exit)).abs() < 1e-12, "{link} vs {}", 1.0 - exit);
    }

    #[test]
    fn a_link_as_wide_as_the_ball_or_wider_is_still_a_link_and_a_cone_from_the_ball_is_not() {
        // A cylinder of the ball's radius from the centre: it never sits inside
        // the ball's surface, so the whole run is link.
        let wide = [([0.0, BALL], [0.4, BALL]), ([0.4, BALL], [1.0, 0.5])];
        assert!((drawn_link_mm(&wide, BALL) - 0.4).abs() < 1e-12);
        let wider = [([0.0, 2.0 * BALL], [0.4, 2.0 * BALL])];
        assert!((drawn_link_mm(&wider, BALL) - 0.4).abs() < 1e-12);
        // A cone from the ball is no link, f32 noise in a radius is not a cone.
        let cone = [([0.0, 0.05], [1.0, 0.2])];
        assert!(drawn_link_mm(&cone, BALL).abs() < 1e-12);
        let noise = [([0.0, 0.05], [0.5, 0.050_05]), ([0.5, 0.050_05], [1.0, 0.3])];
        let exit = (BALL * BALL - 0.05_f64 * 0.05).sqrt();
        assert!((drawn_link_mm(&noise, BALL) - (0.5 - exit)).abs() < 1e-12);
    }

    #[test]
    fn a_head_that_never_leaves_the_ball_or_never_widens_is_measured_without_panic() {
        assert!(drawn_link_mm(&[], BALL).abs() < 1e-12);
        let thin = [([0.0, 0.05], [2.0, 0.05])];
        assert!(
            (drawn_link_mm(&thin, BALL) - (2.0 - (BALL * BALL - 0.0025_f64).sqrt())).abs() < 1e-12
        );
    }
}

#[cfg(test)]
mod merged_short_span_tests {
    /// A post whose top stands a micron over a slot boundary (the Cylinder's
    /// arm 113 once the link moved its joint, 2026-10-08): the top span is
    /// 0.4 micron long, and turning the merged line about the top end by
    /// `total / short` drew a 41.7 mm radius (55 000 mm3 of resin on a 1 000 mm3
    /// model). No drawn radius may exceed the widest ledger radius.
    #[test]
    fn a_micron_span_at_the_top_of_a_post_is_merged_without_a_lever() {
        let mut profile = vec![0.857_588_436_496_121_9; 2];
        profile.extend(std::iter::repeat_n(0.797_348_443_985_570_3, 26));
        profile.push(0.783_887_290_440_891_4);
        let spans = super::drawn_spans(
            &profile,
            -1.0,
            0.450_000_000_000_000_2,
            0.400_427_588_615_468_7,
            -1.0,
            0.783_887_290_440_891_4,
            0.857_588_436_496_121_9,
        );
        let widest = 0.857_588_436_496_121_9 * 1.05;
        for span in &spans {
            assert!(
                span.top_radius_mm <= widest && span.bottom_radius_mm <= widest,
                "a radius over the widest ledger value: {spans:?}"
            );
        }
        // The drawn section is never under the ledger: the top is at least the
        // thin radius, the bottom reaches the root.
        assert!(spans[0].top_radius_mm >= 0.783_887_290_440_891_4 - 1e-12);
        assert!((spans[spans.len() - 1].bottom_radius_mm - 0.857_588_436_496_121_9).abs() < 1e-9);
    }
}

#[cfg(test)]
mod drawn_span_ramp_tests {
    use super::{drawn_chords, drawn_spans, DrawnSpan};

    /// The lean law's radius change per 0.05 mm layer (the adapter's collar).
    const D: f64 = 0.078_485;

    /// Spans of a shaft whose pieces are one unit tall, given TOP-DOWN per
    /// piece requirements; the shaft runs from the top of the first piece to
    /// the bottom of the last.
    #[expect(
        clippy::cast_precision_loss,
        reason = "a small piece count is exact in f64"
    )]
    fn spans_of(top_down: &[f64], start: f64, end: f64) -> Vec<DrawnSpan> {
        let n = top_down.len();
        let bottom_first: Vec<f64> = top_down.iter().rev().copied().collect();
        drawn_spans(&bottom_first, 0.0, n as f64, n as f64, 0.0, start, end)
    }

    fn repeat(value: f64, count: usize) -> Vec<f64> {
        vec![value; count]
    }

    fn assert_continuous(spans: &[DrawnSpan], total: f64) {
        assert!((spans[0].top_z_mm - total).abs() < 1e-12);
        assert!(spans.last().unwrap().bottom_z_mm.abs() < 1e-12);
        for pair in spans.windows(2) {
            assert!((pair[0].bottom_z_mm - pair[1].top_z_mm).abs() < 1e-12);
        }
    }

    #[test]
    fn a_ramp_of_seven_steps_is_one_frustum() {
        let thin = 0.3;
        let mut profile = repeat(thin, 10);
        profile.extend((1..=7).map(|k| thin + f64::from(k) * D));
        let wide = thin + 8.0 * D;
        profile.extend(repeat(wide, 5));
        let spans = spans_of(&profile, thin, wide);
        assert_continuous(&spans, 22.0);
        assert_eq!(spans.len(), 3, "{spans:?}");
        assert_eq!(
            (spans[0].top_radius_mm, spans[0].bottom_radius_mm),
            (thin, thin)
        );
        // The frustum starts at the top of the last thin layer (z = 13).
        assert!((spans[0].bottom_z_mm - 13.0).abs() < 1e-12, "{spans:?}");
        assert!((spans[1].top_radius_mm - thin).abs() < 1e-12);
        assert!((spans[1].bottom_radius_mm - wide).abs() < 1e-12);
        assert!((spans[1].bottom_z_mm - 5.0).abs() < 1e-12, "{spans:?}");
        assert!((spans[2].top_radius_mm - wide).abs() < 1e-12);
        assert!((spans[2].bottom_radius_mm - wide).abs() < 1e-12);
    }

    #[test]
    fn the_adapters_collar_cone_is_at_most_two_frusta() {
        // Top-down: the post's ledger 0.25, the cone from 0.272 widening by
        // D per layer down to the branch radius 0.90, then the raised post.
        let mut profile = repeat(0.25, 3);
        profile.extend((1..=8).rev().map(|k| 0.90 - f64::from(k) * D));
        profile.extend(repeat(0.90, 3));
        let spans = spans_of(&profile, 0.25, 0.90);
        assert_continuous(&spans, 14.0);
        let in_ramp: Vec<&DrawnSpan> = spans
            .iter()
            .filter(|s| s.top_z_mm <= 12.0 && s.bottom_z_mm >= 3.0)
            .collect();
        assert!(
            (1..=2).contains(&in_ramp.len()),
            "{} spans across the ramp: {spans:?}",
            in_ramp.len()
        );
        assert!(spans.len() <= 4, "{spans:?}");
        for pair in spans.windows(2) {
            assert!((pair[0].bottom_radius_mm - pair[1].top_radius_mm).abs() < 1e-12);
        }
    }

    #[test]
    fn a_single_step_is_a_cone_at_the_lean_law_not_a_shoulder() {
        // The lean law's slope: cot(45 - 25 / 2 = 32.5 deg) mm of radius per mm.
        let slope = 1.0 / 32.5_f64.to_radians().tan();
        let mut profile = repeat(0.3, 4);
        profile.extend(repeat(0.6, 4));
        let spans = spans_of(&profile, 0.3, 0.6);
        assert_continuous(&spans, 8.0);
        assert_eq!(spans.len(), 3, "{spans:?}");
        // No shoulder: every cut joins equal radii.
        for pair in spans.windows(2) {
            assert!(
                (pair[0].bottom_radius_mm - pair[1].top_radius_mm).abs() < 1e-12,
                "a flat shoulder at z {}: {spans:?}",
                pair[0].bottom_z_mm
            );
        }
        // The cone sits ABOVE the step (the ledger asks 0.3 there), ends on the
        // step at the wide radius, and is no flatter than the lean law.
        let cone = spans[1];
        assert!((cone.bottom_z_mm - 4.0).abs() < 1e-12);
        assert!((cone.bottom_radius_mm - 0.6).abs() < 1e-12);
        let height = cone.top_z_mm - cone.bottom_z_mm;
        assert!(height > 0.0 && cone.top_radius_mm >= 0.3 - 1e-12);
        let drawn_slope = (cone.bottom_radius_mm - cone.top_radius_mm) / height;
        assert!(
            drawn_slope <= slope + 1e-9,
            "{drawn_slope} vs the law's {slope}"
        );
        assert!(
            (drawn_slope - slope).abs() < 1e-9,
            "a step this size uses the law's own slope: {drawn_slope} vs {slope}"
        );
        // And the section never drops under the ledger.
        assert!(spans[0].bottom_radius_mm >= 0.3 - 1e-12);
    }

    #[test]
    fn a_frustum_shorter_than_one_pixel_is_merged_into_its_neighbour() {
        // The shaft's top stands 0.001 mm over a slot boundary (z = 2), and the
        // ledger steps there: the chords alone leave a 0.001 mm first frustum
        // (the Cylinder's 437).
        let profile = [0.5, 0.5, 0.3, 0.3];
        let chords = drawn_chords(&profile, 0.0, 4.0, 2.001, 0.0, 0.3, 0.5);
        assert!(
            chords.iter().any(|s| s.top_z_mm - s.bottom_z_mm < 0.0015),
            "red before: the chords hold a 0.001 mm frustum: {chords:?}"
        );
        let spans = drawn_spans(&profile, 0.0, 4.0, 2.001, 0.0, 0.3, 0.5);
        assert!(
            spans.iter().all(|s| s.top_z_mm - s.bottom_z_mm >= 0.019),
            "no span under one registered pixel pitch remains: {spans:?}"
        );
        assert_continuous(&spans, 2.001);
        assert!(spans[0].top_radius_mm >= 0.3 && spans.last().unwrap().bottom_radius_mm >= 0.5);
    }

    /// A shaft with a thin slot over a wide slot (0.05 mm each, as in the review
    /// traces): the ledger asks 0.30 above z = 0.05 and 0.37 below it. The
    /// drawn spans from `from_z` to `to_z` must be continuous, no flat shoulder,
    /// none shorter than one registered pixel pitch, and on or above the ledger
    /// at every height.
    fn assert_sound_over_a_thin_slot_on_a_wide_one(from_z: f64, to_z: f64) {
        let spans = drawn_spans(&[0.37, 0.30], 0.0, 0.10, from_z, to_z, 0.30, 0.37);
        assert!((spans[0].top_z_mm - from_z).abs() < 1e-12, "{spans:?}");
        assert!(
            (spans.last().unwrap().bottom_z_mm - to_z).abs() < 1e-12,
            "{spans:?}"
        );
        for pair in spans.windows(2) {
            assert!(
                (pair[0].bottom_z_mm - pair[1].top_z_mm).abs() < 1e-12,
                "{spans:?}"
            );
            assert!(
                (pair[0].bottom_radius_mm - pair[1].top_radius_mm).abs() < 1e-9,
                "a flat shoulder at z {}: {spans:?}",
                pair[0].bottom_z_mm
            );
        }
        for span in &spans {
            assert!(
                span.top_z_mm - span.bottom_z_mm >= 0.019 - 1e-12,
                "a frustum under one pixel pitch: {spans:?}"
            );
        }
        for step in 0..=200_u32 {
            let z = to_z + (from_z - to_z) * (f64::from(step) + 0.37) / 200.37;
            let ledger = if z < 0.05 { 0.37 } else { 0.30 };
            let drawn = spans
                .iter()
                .filter(|s| s.bottom_z_mm - 1e-12 <= z && z <= s.top_z_mm + 1e-12)
                .map(|s| {
                    let t = (s.top_z_mm - z) / (s.top_z_mm - s.bottom_z_mm);
                    s.top_radius_mm + (s.bottom_radius_mm - s.top_radius_mm) * t
                })
                .fold(0.0_f64, f64::max);
            assert!(
                drawn >= ledger - 1e-9,
                "section {drawn} under the ledger {ledger} at z {z}: {spans:?}"
            );
        }
    }

    #[test]
    fn a_short_last_span_merged_keeps_the_ledger_over_its_whole_height() {
        // Chords: 0.05 mm at 0.30, then 0.01 mm at 0.37. A straight 0.30 -> 0.37
        // line over the 0.06 mm reaches 0.37 only at its foot (red before).
        assert_sound_over_a_thin_slot_on_a_wide_one(0.10, 0.04);
    }

    #[test]
    fn a_cone_never_leaves_a_sliver_of_the_span_it_cuts() {
        // Two 0.05 mm spans: the law's cone takes 0.0446 mm of the upper one and
        // would leave 0.0054 mm (red before).
        assert_sound_over_a_thin_slot_on_a_wide_one(0.10, 0.0);
    }

    #[test]
    fn a_step_near_the_shaft_top_is_a_continuous_cone_not_a_shoulder() {
        // The upper span is 0.04 mm, shorter than the cone: v19-fix review kept the
        // flat 0.30 -> 0.37 annulus here (red before).
        assert_sound_over_a_thin_slot_on_a_wide_one(0.09, 0.0);
    }

    #[test]
    fn a_ramp_next_to_a_single_step_changes_only_the_ramp() {
        // thin, a lone step up to mid, flat, then a two-rise ramp to wide.
        let mut profile = repeat(0.3, 3);
        profile.extend(repeat(0.5, 3));
        profile.extend([0.6, 0.7]);
        profile.extend(repeat(0.8, 3));
        let spans = spans_of(&profile, 0.3, 0.8);
        assert_continuous(&spans, 11.0);
        // The lone step is a cone just above z = 8 (the span below it starts at
        // 0.5 there, and the cone reaches 0.5 exactly on it): no shoulder.
        let below = spans
            .iter()
            .find(|s| (s.top_z_mm - 8.0).abs() < 1e-12)
            .expect("a span starts at the step");
        assert!((below.top_radius_mm - 0.5).abs() < 1e-12, "{spans:?}");
        let cone = spans
            .iter()
            .find(|s| (s.bottom_z_mm - 8.0).abs() < 1e-12)
            .expect("a cone ends on the step");
        assert!((cone.bottom_radius_mm - 0.5).abs() < 1e-12, "{spans:?}");
        assert!(
            cone.top_radius_mm < 0.5 && cone.top_radius_mm >= 0.3 - 1e-12,
            "{spans:?}"
        );
        // The ramp (cuts at z = 5, 4, 3) is continuous.
        for z in [5.0, 4.0, 3.0] {
            for pair in spans
                .windows(2)
                .filter(|p| (p[0].bottom_z_mm - z).abs() < 1e-12)
            {
                assert!((pair[0].bottom_radius_mm - pair[1].top_radius_mm).abs() < 1e-12);
            }
        }
    }

    #[test]
    fn a_constant_profile_is_the_single_taper() {
        let spans = spans_of(&repeat(0.4, 6), 0.3, 0.5);
        assert_eq!(
            spans,
            vec![DrawnSpan {
                top_z_mm: 6.0,
                bottom_z_mm: 0.0,
                top_radius_mm: 0.4,
                bottom_radius_mm: 0.5
            }]
        );
    }

    struct Lcg(u64);
    impl Lcg {
        #[expect(clippy::cast_precision_loss, reason = "53 bits are exact in f64")]
        fn next(&mut self) -> f64 {
            self.0 = self
                .0
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            ((self.0 >> 11) as f64) / ((1_u64 << 53) as f64)
        }
        #[expect(
            clippy::cast_precision_loss,
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "a small index range in a test generator"
        )]
        fn count(&mut self, lo: usize, hi: usize) -> usize {
            lo + (self.next() * (hi - lo + 1) as f64) as usize % (hi - lo + 1)
        }
    }

    #[test]
    #[expect(
        clippy::cast_precision_loss,
        reason = "small cut indices are exact in f64"
    )]
    fn random_stepped_profiles_stay_above_the_ledger_and_ramps_are_continuous() {
        for seed in 0..400_u64 {
            let mut rng = Lcg(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ 0xD1B5_4A32);
            let mut need = Vec::new();
            let mut value = 0.15 + 0.3 * rng.next();
            while need.len() < 40 {
                need.extend(repeat(value, rng.count(1, 4)));
                match rng.count(0, 2) {
                    0 => value += 0.02 + 0.2 * rng.next(),
                    1 => {
                        // A ramp: 2-6 consecutive widenings.
                        for _ in 0..rng.count(2, 6) {
                            value += 0.02 + 0.07 * rng.next();
                            need.push(value);
                        }
                    }
                    _ => {}
                }
            }
            let n = need.len();
            let spans = spans_of(&need, need[0] * 0.9, need[n - 1] * 1.1);
            assert_continuous(&spans, n as f64);
            let radius_at = |z: f64| -> f64 {
                let s = spans
                    .iter()
                    .find(|s| z <= s.top_z_mm + 1e-12 && z >= s.bottom_z_mm - 1e-12)
                    .expect("a span covers z");
                let t = (s.top_z_mm - z) / (s.top_z_mm - s.bottom_z_mm);
                s.top_radius_mm + (s.bottom_radius_mm - s.top_radius_mm) * t
            };
            for (piece, required) in need.iter().enumerate() {
                let z = n as f64 - piece as f64 - 0.5;
                assert!(
                    radius_at(z) >= required * (1.0 - 1e-9),
                    "seed {seed} piece {piece}"
                );
            }
            // Cuts strictly inside a span carry the larger neighbour.
            for cut in 1..n {
                let z = (n - cut) as f64;
                let boundary = spans.iter().any(|s| (s.bottom_z_mm - z).abs() < 1e-12);
                if !boundary {
                    let required = need[cut - 1].max(need[cut]);
                    assert!(
                        radius_at(z) >= required * (1.0 - 1e-9),
                        "seed {seed} cut {cut}"
                    );
                }
            }
            // Ramps: runs of >= 2 consecutive rises.
            let rise: Vec<bool> = (0..n)
                .map(|c| c >= 1 && need[c] > need[c - 1] * (1.0 + 1e-9))
                .collect();
            let mut c = 1;
            while c < n {
                if !rise[c] {
                    c += 1;
                    continue;
                }
                let a = c;
                while c < n && rise[c] {
                    c += 1;
                }
                let b = c - 1;
                if b == a {
                    continue;
                }
                // The region is split at its top (cut a - 1) and its bottom (cut b).
                for cut in [a - 1, b] {
                    if cut == 0 {
                        continue;
                    }
                    let z = (n - cut) as f64;
                    assert!(
                        spans.iter().any(|s| (s.bottom_z_mm - z).abs() < 1e-12),
                        "seed {seed}: no span boundary at ramp edge cut {cut}"
                    );
                }
                for cut in a..=b {
                    let z = (n - cut) as f64;
                    for pair in spans
                        .windows(2)
                        .filter(|p| (p[0].bottom_z_mm - z).abs() < 1e-12)
                    {
                        assert!(
                            (pair[0].bottom_radius_mm - pair[1].top_radius_mm).abs() < 1e-12,
                            "seed {seed}: radii differ at ramp cut {cut}"
                        );
                    }
                }
            }
        }
    }
}

/// **A model-landed arm ends in a FOOT** (lane 2, 2026-10-12, `MODEL-FEET.md`
/// F1), graded on the lowering's own records.
#[cfg(test)]
mod model_foot_tests {
    use super::{drawn_span_rules, lower_sweep_arms, ModelFoot, SweepArm, SweepArmLanding};
    use crate::{BuildFrame, SupportPrimitive, SupportSegmentKind, SweepLowering};
    use plinth_core::{Finite, LayerPlan, PositiveFinite};

    const H: f64 = 0.05;
    const PLATE_Z: f64 = -5.0;
    /// The slenderness width the post keeps in its span.
    const ROOT: f64 = 0.5;
    /// The load-sized contact: one small tip's load.
    const FOOT: f64 = 0.05;
    /// The landed face: the top of the strike lamina over the base at z = 1.
    const FACE_Z: f64 = 1.0 + H;

    fn lower(arms: &[SweepArm]) -> SweepLowering {
        let layers = LayerPlan::new(PLATE_Z, vec![H; 400]).expect("a uniform plan");
        let frame = BuildFrame::new(
            Finite::new(PLATE_Z).expect("a finite plate"),
            PositiveFinite::new(5.0).expect("a positive lift"),
        );
        lower_sweep_arms(arms, frame, &layers, 100_000, 1 << 30)
            .expect("the lowering refuses only about the request")
    }

    /// A vertical post, seat at z = 6 under a flat face, landing 5 mm lower on
    /// a flat up-facing face; its ledger is the slenderness width all the way
    /// down, as a long unbraced post's is.
    fn landed_arm(landing: SweepArmLanding) -> SweepArm {
        SweepArm {
            seat_mm: [0.0, 0.0, 6.0],
            base_mm: [0.0, 0.0, 1.0],
            tip_radius_mm: 0.1,
            radius_profile_mm: vec![ROOT; 100],
            seat_normal_mm: [0.0, 0.0, -1.0],
            seat_face_point_mm: Some([0.0, 0.0, 6.0]),
            landing,
            seats_into_model: true,
            head: None,
            contact_ball_radius_mm: None,
        }
    }

    fn foot() -> SweepArmLanding {
        SweepArmLanding::Model(ModelFoot {
            face_point_mm: Some([0.0, 0.0, FACE_Z]),
            face_normal_mm: [0.0, 0.0, 1.0],
            contact_radius_mm: FOOT,
        })
    }

    fn centre(primitive: &SupportPrimitive) -> [f64; 3] {
        std::array::from_fn(|axis| {
            0.5 * (primitive.start_mm[axis].get() + primitive.end_mm[axis].get())
        })
    }

    /// The lowest record of the arm: what touches the landed face.
    fn lowest(lowered: &SweepLowering) -> &SupportPrimitive {
        lowered
            .primitives
            .iter()
            .min_by(|a, b| {
                let low = |p: &SupportPrimitive| p.start_mm[2].get().min(p.end_mm[2].get());
                low(a).total_cmp(&low(b))
            })
            .expect("records")
    }

    /// THE FOOT, AND ITS SABOTAGE. The landed arm ends in a ball of the owner's
    /// ChituBox size for the LOAD-sized contact (not the 0.5 mm slenderness
    /// width), seated into the face at least a layer deep, cut to the model
    /// by a trim whose plane is the landed face and whose shaft direction is
    /// up the arm. The shaft keeps its whole width down to a cone at the lean
    /// law's slope, and the cone narrows to the foot. SABOTAGE: the same arm
    /// as a plain `Landed` landing (the drawing before 2026-10-12) ends in its
    /// own flat 0.5 mm section and carries no ball and no trim at its foot;
    /// every assertion below that names the foot fails on it.
    #[test]
    fn a_model_landing_ends_in_a_seated_trimmed_ball_sized_by_its_load() {
        let lowered = lower(&[landed_arm(foot())]);
        assert_eq!(lowered.model_feet_drawn, 1, "{lowered:?}");
        assert_eq!(lowered.model_foot_cones, 1);
        assert_eq!(lowered.model_feet_without_measured_face, 0);
        assert_eq!(lowered.model_foot_radius_mm, vec![FOOT]);

        // The ball's size is the owner's ball for a tip of the foot's radius:
        // the same factor a top tip of that radius is drawn with.
        let tip_of_foot_size = lower(&[SweepArm {
            tip_radius_mm: FOOT,
            radius_profile_mm: Vec::new(),
            landing: SweepArmLanding::Plate,
            ..landed_arm(SweepArmLanding::Plate)
        }]);
        let factor = tip_of_foot_size.tip_ball_factor[0];
        let ball = lowest(&lowered);
        assert_eq!(ball.segment_kind, SupportSegmentKind::ContactSphere);
        let ball_radius = ball.start_radius_mm.get();
        assert!((ball_radius - factor * FOOT).abs() < 1e-12, "{ball_radius} vs {factor}");
        assert!(ball_radius < ROOT, "the foot is narrower than the flat end it replaces");
        // Seated INTO the face, at least one layer (the law's own minimum is a
        // layer plus a pixel), and never past its own equator.
        let ball_centre = centre(ball);
        let pole_depth = FACE_Z - (ball_centre[2] - ball_radius);
        assert!(pole_depth >= H - 1e-12, "seated {pole_depth} mm");
        assert!(pole_depth <= ball_radius + 1e-12, "past the equator: {pole_depth}");
        // Cut to the model by the landed face's trim, shaft end up the arm.
        let trim = ball.seat_trim.expect("the foot ball is cut to the model");
        let up = |v: [f64; 3]| v[0].abs() < 1e-12 && v[1].abs() < 1e-12 && (v[2] - 1.0).abs() < 1e-12;
        assert!(up(trim.outward_normal()), "{:?}", trim.outward_normal());
        assert!(up(trim.shaft_unit()), "{:?}", trim.shaft_unit());
        assert!((trim.point_mm()[2] - FACE_Z).abs() < 1e-12);
        // The contact the plane cuts through the ball covers the load's own
        // area: a >= r_foot.
        let disc = lowered.model_foot_contact_disc_radius_mm[0];
        assert!(disc >= FOOT * (1.0 - 1e-9), "planned disc {disc} under the foot {FOOT}");

        // The cone: from the shaft's full width to the foot, at the lean law's
        // slope, ending at the ball's centre, cut by the same trim.
        let cone = lowered
            .primitives
            .iter()
            .find(|p| {
                p.segment_kind == SupportSegmentKind::Member
                    && (p.end_radius_mm.get() - FOOT).abs() < 1e-12
            })
            .expect("a cone narrows to the foot");
        assert!((cone.start_radius_mm.get() - ROOT).abs() < 1e-12);
        let cone_end: [f64; 3] = cone.end_mm.map(Finite::get);
        assert!(
            (0..3).all(|axis| (cone_end[axis] - ball_centre[axis]).abs() < 1e-9),
            "the cone ends at the ball's centre"
        );
        let slope = drawn_span_rules().expect("the registry").cone_slope;
        let length = cone.start_mm[2].get() - cone.end_mm[2].get();
        assert!(
            (length - (ROOT - FOOT) / slope).abs() < 1e-9,
            "cone {length} mm, the law's {}",
            (ROOT - FOOT) / slope
        );
        assert_eq!(cone.seat_trim, Some(trim));
        // The slenderness width stays in the span: the shaft reaches the
        // cone's top at its full width.
        let into_cone = lowered
            .primitives
            .iter()
            .find(|p| {
                p.segment_kind != SupportSegmentKind::ContactSphere
                    && (p.end_mm[2].get() - cone.start_mm[2].get()).abs() < 1e-12
            })
            .expect("a shaft span ends where the cone starts");
        assert!((into_cone.end_radius_mm.get() - ROOT).abs() < 1e-12);

        // SABOTAGE: the pre-2026-10-12 drawing.
        let flat = lower(&[landed_arm(SweepArmLanding::Landed)]);
        assert_eq!(flat.model_feet_drawn, 0);
        let end = lowest(&flat);
        assert_ne!(end.segment_kind, SupportSegmentKind::ContactSphere);
        assert!((end.end_radius_mm.get() - ROOT).abs() < 1e-12, "a flat end of the full width");
        assert!(
            end.seat_trim
                .is_none_or(|trim| !up(trim.outward_normal())),
            "nothing cuts the flat end to the landed face (only the tip's own trim, if any)"
        );
    }

    /// A foot whose load needs the shaft's own width draws no cone: the shaft
    /// runs into the ball. A foot handed no usable radius ends flat and is
    /// counted, never dropped (emit and warn).
    #[test]
    fn a_foot_as_wide_as_its_shaft_draws_no_cone_and_a_foot_with_no_radius_is_counted_flat() {
        let wide = lower(&[landed_arm(SweepArmLanding::Model(ModelFoot {
            face_point_mm: Some([0.0, 0.0, FACE_Z]),
            face_normal_mm: [0.0, 0.0, 1.0],
            contact_radius_mm: ROOT,
        }))]);
        assert_eq!(wide.model_feet_drawn, 1);
        assert_eq!(wide.model_foot_cones, 0);
        assert_eq!(lowest(&wide).segment_kind, SupportSegmentKind::ContactSphere);

        let none = lower(&[landed_arm(SweepArmLanding::Model(ModelFoot {
            face_point_mm: None,
            face_normal_mm: [0.0; 3],
            contact_radius_mm: 0.0,
        }))]);
        assert_eq!(none.model_feet_drawn, 0);
        assert_eq!(none.model_feet_flat, 1);
        assert_eq!(none.arms_dropped_invalid_geometry, 0, "the arm still emits");
    }
}
