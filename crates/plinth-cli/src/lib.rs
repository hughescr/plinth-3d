//! Typed parser for the stable `plinth` command-line surface, and the resolver
//! that turns one parsed selection into an executed run.
//!
//! Parsing still deliberately stops before execution policy is assembled. The
//! resolver in [`execute`] consumes [`PrepareSelection`] and owns construction
//! of a complete `plinth_pipeline::PrepareRequest`, including all resource and
//! physics values which are not CLI arguments — every one of them read from the
//! ratified registry rather than restated here.
//!
//! The one thing parsing reads beyond its arguments is the registered default
//! for an omitted optional flag — today only `--wall-mm`. That is the M0
//! registry, a pure publication, and reading it here is what keeps the CLI
//! default and the hollow stage's default the same number. No environment
//! variable is read, and no file or model is opened during parsing.

#![forbid(unsafe_code)]

pub mod execute;

use std::ffi::OsString;
use std::path::PathBuf;

use plinth_core::{M0ConstantsError, PositiveFinite};
use plinth_hollow::{checked_wall_mm, registered_wall, HollowError, WallSelection};
use plinth_orient::as_drawn::NoOrientList;
use plinth_pipeline::{OutputProfile, Placement};

pub const ROOT_USAGE: &str = "Usage: plinth <subcommand>\n\nSubcommands:\n  prepare  Prepare every source model in a directory\n\nOptions:\n  -h, --help     Show help\n  -V, --version  Show version";

pub const PREPARE_USAGE: &str = "Usage: plinth prepare --input-models-dir DIR --output saturn4-ultra --out-dir DIR [--orient dense|none|pinned:DIR] [--no-orient=LIST] [--placement legacy|sweep] [--wall-mm MM] [--no-cache] [--exact-delicacy] [--hollow-first|--no-hollow-first] [--perf DIR]\n\nOptions:\n  --input-models-dir DIR  Directory containing source models\n  --output PROFILE       Output profile; exactly: saturn4-ultra\n  --out-dir DIR          Destination directory\n  --orient MODE          dense (default) searches every model's build orientation; none uses every model exactly as drawn (no rotation; still centred and lifted off the plate); pinned:DIR replays the rotation DIR/<stem>.pipeline.json records, per model by stem.\n                         One model is used as drawn under any mode when it is named in --no-orient, carries the Finder tag plinth-no-rotate (macOS), has plinth-no-rotate in its user.xdg.tags attribute (Linux), or has a marker file <stem>.asdrawn beside it (any platform). The report says, per model, \"as drawn (<reason>)\" or \"orientation searched\"\n  --no-orient=LIST       Use these models exactly as drawn, under any --orient mode: a comma-separated list of file names in the input directory, with or without .stl, and simple globs (* any run of characters, ? one character), e.g. --no-orient=Round_*,make_overhang_table. Also --no-orient LIST; may be given more than once. A name that matches no model prints a WARNING and the run continues\n  --placement MODE       legacy (default) is the sealed support run and its pre-write gates; sweep is the EXPERIMENTAL slice-sweep core, which runs NO pre-write gate; it leans and merges arms onto other shafts, ties long members with |X| braces, thickened roots and masts, and keeps each brace's and mast's solid clear of the model and of other members, but draws no joint spheres and does not clearance-check an arm's own shaft against the model\n  --wall-mm MM           Hollow shell wall thickness in mm, no finer than the frozen hollow voxel pitch; omit for the registered default, raise it for a piece that gets handled\n  --no-cache             Disable cache use for this request\n  --exact-delicacy       Evaluate the orientation table's delicacy curvature on the full mesh (the reference; slower on a dense model). Omitted, a protected proxy is used: low-curvature vertices are merged into small patches, every sharp vertex is still spread exactly. The proxy can pick a different pose on a dense model\n  --hollow-first         On by default (accepted for scripts that give it): score the orientation finalists on the hollowed, drilled shell each pose would print as (drains at its bottom, vents at its top), not on the solid; the winner's plan is then emitted without a second sweep. Changes the output of hollowed models only. Wins over PLINTH_HOLLOW_FIRST\n  --no-hollow-first      Score the orientation finalists on the solid, as before 2026-10-16. PLINTH_HOLLOW_FIRST=0 does the same when neither flag is given\n  --perf DIR            Write per-model work counters and phase times to DIR/<model>.perf.json (outside the output folder; the emitted file is unchanged). PLINTH_PERF_DIR=DIR does the same when --perf is absent\n  -h, --help             Show help";

/// Whether this request may use a `plinth-pipeline` cache.
///
/// **No cache persists between runs. The one cache there is lives inside a run:
/// under `--placement sweep`, `Disabled` makes the seal plan the winning pose
/// again instead of emitting the plan the orientation search measured for it
/// (`r1-winner-reuse`); the emitted bytes are the same either way. Under the
/// legacy engine `Disabled` changes no computation.** The spelling
/// is kept anyway, and is not dead surface: `06-conventions.md` § 6 makes
/// cache-disabled equality CI-mandatory and names *this exact flag* as the one
/// and only cache-off mechanism — "one spelling, defined here, used verbatim in
/// M8's acceptance commands. There is no environment-variable form ... a second
/// way to disable the cache is a second thing that can silently stop working."
/// `04-migration-plan.md`'s determinism runs invoke it verbatim.
///
/// It was briefly deleted on 2026-08-26 as an unread flag and restored the same
/// day: "no production code reads this" was true of the compiler and false of
/// the procedure. Keep it accepted and keep it reported honestly as a no-op
/// until a cache exists to bypass; do not add a second spelling.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CacheMode {
    #[default]
    Enabled,
    Disabled,
}

impl CacheMode {
    #[must_use]
    pub const fn no_cache(self) -> bool {
        matches!(self, Self::Disabled)
    }
}

/// The orientation authority the operator selected.
///
/// **`--orient dense` is the default and is what an omitted flag means** — the
/// recorded M8 default (`04-migration-plan.md:1721`). Until this flag existed
/// the CLI hardcoded a policy with no implementation, so `plinth prepare`
/// refused every model it was ever handed; flipping the default is what closes
/// that standing product defect.
///
/// **Parsing opens no file.** `pinned:DIR` records a path and nothing else; the
/// directory scan that turns it into a per-stem policy lives in
/// [`execute`](crate::execute), because a parser that reads the filesystem
/// cannot be tested without one.
///
/// There is deliberately **no `--orient-scar-weight`**: the scar weight is a
/// registered constant, not an operator input, and a flag for it would be a
/// second authority over a number the registry already owns.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum OrientSelection {
    /// Search every model's orientation.
    #[default]
    Dense,
    /// Replay `DIR/<stem>.pipeline.json` for each model, by stem. A model the
    /// directory does not name is refused per model, never given a neighbour's
    /// rotation.
    Pinned(PathBuf),
    /// `--orient none`: use every model exactly as drawn (identity rotation,
    /// still centred and lifted off the plate). GitHub issue #1.
    ///
    /// Under `dense` or `pinned:DIR` one model can still be marked as drawn by
    /// a `plinth-no-rotate` tag or a `<stem>.asdrawn` marker; that per-model
    /// check reads the filesystem, so it lives in the pipeline, not here.
    AsDrawn,
}

/// The support engine the operator selected.
///
/// **`--placement legacy` is the default and is what an omitted flag means.**
/// The legacy path is untouched by this flag existing; `sweep` selects the
/// experimental slice-sweep core, which runs **no pre-write gate at all** and
/// skips branching, bracing, joint spheres, raft and every clearance check.
/// Those absences are printed on every sweep run rather than left to be
/// discovered in ChituBox.
///
/// There is deliberately no `--placement-kappa` or any other numeric knob: the
/// reach is a registered constant computed from its measured parents at run
/// time, and a flag for it would be a second authority over a number the
/// registry already owns.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PlacementSelection {
    #[default]
    Legacy,
    Sweep,
}

impl From<PlacementSelection> for Placement {
    fn from(selection: PlacementSelection) -> Self {
        match selection {
            PlacementSelection::Legacy => Self::Legacy,
            PlacementSelection::Sweep => Self::Sweep,
        }
    }
}

/// The complete user-selected CLI state and the compile-time handoff seam for
/// a later resource resolver. It is intentionally not a partial or fabricated
/// `PrepareRequest`.
///
/// `Eq` is deliberately absent: `wall_mm` is a millimetre quantity, and a
/// selection is compared for equality in tests, never keyed on.
#[derive(Clone, Debug, PartialEq)]
pub struct PrepareSelection {
    pub input_models_dir: PathBuf,
    pub output_dir: PathBuf,
    pub orientation: OrientSelection,
    /// `--no-orient=LIST`: models to use exactly as drawn under any
    /// `orientation`, by file name or glob (owner spec 2026-10-01). Parsing
    /// only splits the list; matching it against the folder, and the WARNING
    /// for a name that matches no model, happen when the folder is read.
    pub no_orient: NoOrientList,
    /// Which support engine this run uses. See [`PlacementSelection`].
    pub placement: PlacementSelection,
    pub output_profile: OutputProfile,
    /// The hollow shell wall the operator selected for every model in this
    /// request, defaulted from `HOLLOW_WALL_DEFAULT_MM` when `--wall-mm` is
    /// absent. Nothing here infers it from the model; the resolver hands this
    /// value straight to `plinth_hollow::HollowRequest::wall_mm`.
    pub wall_mm: PositiveFinite,
    pub cache: CacheMode,
    /// `--exact-delicacy`: the orientation table's delicacy curvature is
    /// evaluated on the full mesh, the slower reference. Absent (the default),
    /// a protected proxy is used (2026-10-15): the same walk over merged
    /// low-curvature vertices, with every sharp vertex still spread exactly.
    /// The proxy can move the chosen pose on a dense model.
    pub exact_delicacy: bool,
    /// Whether the orientation finalists are scored on the drilled shell each
    /// pose prints as (`docs/plans/hollow-first-2026-10-16.md`). `Some(true)`
    /// is `--hollow-first`, `Some(false)` is `--no-hollow-first`. `None`
    /// (neither flag): `PLINTH_HOLLOW_FIRST=0` turns it off, else it is on.
    pub hollow_first: Option<bool>,
    /// `--perf DIR`: where the write-only per-model counter sidecars go. It
    /// reaches no decision and no digested output.
    pub perf_dir: Option<PathBuf>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum CliAction {
    RootHelp,
    PrepareHelp,
    Version,
    Prepare(PrepareSelection),
}

#[derive(Clone, Debug, PartialEq)]
pub enum ParseError {
    MissingSubcommand,
    UnknownSubcommand(OsString),
    MissingRequiredFlag(&'static str),
    DuplicateFlag(&'static str),
    /// Two flags that ask for opposite things were both given.
    ConflictingFlags(&'static str, &'static str),
    MissingFlagValue(&'static str),
    UnknownFlag(OsString),
    UnexpectedPositional(OsString),
    NonUnicodeOutputProfile(OsString),
    UnknownOutputProfile(String),
    NonUnicodeWallMm(OsString),
    NonUnicodeOrientMode(OsString),
    /// `--no-orient` was given a list that is not valid Unicode.
    NonUnicodeNoOrient(OsString),
    NonUnicodePlacementMode(OsString),
    /// `--placement` was given a mode that is not one of the two sanctioned
    /// spellings. One spelling per concept, so a near-miss is refused rather
    /// than guessed at — the same rule `--orient` follows.
    UnknownPlacementMode(String),
    /// `--orient` was given a mode that is not one of the two sanctioned
    /// spellings. There is exactly one spelling per concept, so a near-miss is
    /// refused rather than guessed at.
    UnknownOrientMode(String),
    /// `--wall-mm` was given a wall the hollow stage will not accept: not a
    /// positive finite number of millimetres, or finer than the frozen voxel
    /// pitch that stage selects cavities on.
    InvalidWallMm(String),
    /// The registered wall default could not be read, so no default exists to
    /// fall back to. The parser refuses rather than substituting a literal.
    RegisteredWallUnavailable(&'static M0ConstantsError),
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingSubcommand => formatter.write_str("missing subcommand"),
            Self::UnknownSubcommand(value) => {
                write!(
                    formatter,
                    "unknown subcommand {:?}",
                    value.to_string_lossy()
                )
            }
            Self::MissingRequiredFlag(flag) => write!(formatter, "missing required flag {flag}"),
            Self::DuplicateFlag(flag) => write!(formatter, "duplicate flag {flag}"),
            Self::ConflictingFlags(first, second) => {
                write!(formatter, "{first} and {second} cannot be given together")
            }
            Self::MissingFlagValue(flag) => write!(formatter, "missing value for flag {flag}"),
            Self::UnknownFlag(flag) => {
                write!(formatter, "unknown flag {:?}", flag.to_string_lossy())
            }
            Self::UnexpectedPositional(value) => write!(
                formatter,
                "unexpected positional argument {:?}",
                value.to_string_lossy()
            ),
            Self::NonUnicodeOrientMode(value) => write!(
                formatter,
                "orientation mode is not valid Unicode: {:?}",
                value.to_string_lossy()
            ),
            Self::NonUnicodeNoOrient(value) => write!(
                formatter,
                "--no-orient list is not valid Unicode: {:?}",
                value.to_string_lossy()
            ),
            Self::UnknownOrientMode(value) => write!(
                formatter,
                "unknown orientation mode {value:?}; expected dense, none or pinned:DIR"
            ),
            Self::NonUnicodePlacementMode(value) => write!(
                formatter,
                "placement mode is not valid Unicode: {:?}",
                value.to_string_lossy()
            ),
            Self::UnknownPlacementMode(value) => write!(
                formatter,
                "unknown placement mode {value:?}; expected legacy or sweep"
            ),
            Self::NonUnicodeOutputProfile(value) => write!(
                formatter,
                "output profile is not valid Unicode: {:?}",
                value.to_string_lossy()
            ),
            Self::UnknownOutputProfile(value) => write!(
                formatter,
                "unknown output profile {value:?}; expected {:?}",
                OutputProfile::SATURN4_ULTRA_CLI_VALUE
            ),
            Self::NonUnicodeWallMm(value) => write!(
                formatter,
                "wall thickness is not valid Unicode: {:?}",
                value.to_string_lossy()
            ),
            Self::InvalidWallMm(value) => write!(
                formatter,
                "wall thickness {value:?} is not a wall this stage can produce: expected a positive finite number of millimetres, no finer than the frozen hollow voxel pitch"
            ),
            Self::RegisteredWallUnavailable(error) => {
                write!(formatter, "registered wall default is unavailable: {error}")
            }
        }
    }
}

impl std::error::Error for ParseError {}

/// Parse arguments after the binary name. No environment variable is read.
pub fn parse_args<I, T>(arguments: I) -> Result<CliAction, ParseError>
where
    I: IntoIterator<Item = T>,
    T: Into<OsString>,
{
    let arguments = arguments.into_iter().map(Into::into).collect::<Vec<_>>();
    let Some(subcommand) = arguments.first() else {
        return Err(ParseError::MissingSubcommand);
    };
    match subcommand.to_str() {
        Some("--help" | "-h") if arguments.len() == 1 => Ok(CliAction::RootHelp),
        Some("--version" | "-V") if arguments.len() == 1 => Ok(CliAction::Version),
        Some("prepare") => parse_prepare(&arguments[1..]),
        _ => Err(ParseError::UnknownSubcommand(subcommand.clone())),
    }
}

fn parse_prepare(arguments: &[OsString]) -> Result<CliAction, ParseError> {
    if arguments.len() == 1 && matches!(arguments[0].to_str(), Some("--help" | "-h")) {
        return Ok(CliAction::PrepareHelp);
    }

    let mut input_models_dir = None;
    let mut output_dir = None;
    let mut output_profile = None;
    let mut wall_mm = None;
    let mut orientation = None;
    let mut placement = None;
    let mut no_orient = NoOrientList::default();
    let mut cache = CacheMode::Enabled;
    let mut no_cache_seen = false;
    let mut exact_delicacy = false;
    let mut hollow_first: Option<bool> = None;
    let mut perf_dir = None;
    let mut index = 0;
    while index < arguments.len() {
        let argument = &arguments[index];
        match argument.to_str() {
            Some("--input-models-dir") => {
                set_value(
                    &mut input_models_dir,
                    "--input-models-dir",
                    arguments,
                    &mut index,
                    PathBuf::from,
                )?;
            }
            Some("--out-dir") => {
                set_value(
                    &mut output_dir,
                    "--out-dir",
                    arguments,
                    &mut index,
                    PathBuf::from,
                )?;
            }
            Some("--perf") => {
                set_value(&mut perf_dir, "--perf", arguments, &mut index, PathBuf::from)?;
            }
            Some("--output") => {
                let raw = take_value("--output", arguments, &mut index)?;
                if output_profile.is_some() {
                    return Err(ParseError::DuplicateFlag("--output"));
                }
                let value = raw
                    .to_str()
                    .ok_or_else(|| ParseError::NonUnicodeOutputProfile(raw.clone()))?;
                output_profile = Some(
                    OutputProfile::try_from(value)
                        .map_err(|_| ParseError::UnknownOutputProfile(value.to_owned()))?,
                );
            }
            Some("--wall-mm") => {
                let raw = take_value("--wall-mm", arguments, &mut index)?;
                if wall_mm.is_some() {
                    return Err(ParseError::DuplicateFlag("--wall-mm"));
                }
                let value = raw
                    .to_str()
                    .ok_or_else(|| ParseError::NonUnicodeWallMm(raw.clone()))?;
                let millimetres = value
                    .parse::<f64>()
                    .map_err(|_| ParseError::InvalidWallMm(value.to_owned()))?;
                let positive = PositiveFinite::new(millimetres)
                    .map_err(|_| ParseError::InvalidWallMm(value.to_owned()))?;
                // The hollow stage owns the admissibility rule; the parser
                // refuses the same values it does rather than restating one.
                wall_mm = Some(match checked_wall_mm(positive) {
                    Ok(checked) => checked,
                    Err(HollowError::Constants(error)) => {
                        return Err(ParseError::RegisteredWallUnavailable(error))
                    }
                    Err(_) => return Err(ParseError::InvalidWallMm(value.to_owned())),
                });
            }
            Some("--orient") => {
                let raw = take_value("--orient", arguments, &mut index)?;
                if orientation.is_some() {
                    return Err(ParseError::DuplicateFlag("--orient"));
                }
                let value = raw
                    .to_str()
                    .ok_or_else(|| ParseError::NonUnicodeOrientMode(raw.clone()))?;
                orientation = Some(match value {
                    "dense" => OrientSelection::Dense,
                    "none" => OrientSelection::AsDrawn,
                    _ => match value.strip_prefix("pinned:") {
                        // An empty directory is refused here rather than
                        // resolving to the process working directory, which is
                        // not a thing the operator named.
                        Some(directory) if !directory.is_empty() => {
                            OrientSelection::Pinned(PathBuf::from(directory))
                        }
                        _ => return Err(ParseError::UnknownOrientMode(value.to_owned())),
                    },
                });
            }
            Some("--placement") => {
                let raw = take_value("--placement", arguments, &mut index)?;
                if placement.is_some() {
                    return Err(ParseError::DuplicateFlag("--placement"));
                }
                let value = raw
                    .to_str()
                    .ok_or_else(|| ParseError::NonUnicodePlacementMode(raw.clone()))?;
                placement = Some(match value {
                    "legacy" => PlacementSelection::Legacy,
                    "sweep" => PlacementSelection::Sweep,
                    _ => return Err(ParseError::UnknownPlacementMode(value.to_owned())),
                });
            }
            // `--no-orient=LIST` (the spelling the owner used) or
            // `--no-orient LIST`, like every other flag here. A list flag
            // accumulates: given twice, both lists apply. An empty list names
            // nothing and is refused as a missing value; a name that matches
            // no model is NOT a parse error (the parser opens no folder) — the
            // run warns about it and goes on.
            Some(flag) if flag == "--no-orient" || flag.starts_with("--no-orient=") => {
                let raw = match flag.strip_prefix("--no-orient=") {
                    Some(inline) => OsString::from(inline),
                    None => take_value("--no-orient", arguments, &mut index)?,
                };
                let value = raw
                    .to_str()
                    .ok_or_else(|| ParseError::NonUnicodeNoOrient(raw.clone()))?;
                let listed = NoOrientList::parse(value);
                if listed.is_empty() {
                    return Err(ParseError::MissingFlagValue("--no-orient"));
                }
                no_orient.extend(listed.entries().iter().cloned());
            }
            Some("--no-cache") => {
                if no_cache_seen {
                    return Err(ParseError::DuplicateFlag("--no-cache"));
                }
                no_cache_seen = true;
                cache = CacheMode::Disabled;
            }
            Some("--exact-delicacy") => {
                if exact_delicacy {
                    return Err(ParseError::DuplicateFlag("--exact-delicacy"));
                }
                exact_delicacy = true;
            }
            Some(flag @ ("--hollow-first" | "--no-hollow-first")) => {
                let flag: &'static str = if flag == "--hollow-first" {
                    "--hollow-first"
                } else {
                    "--no-hollow-first"
                };
                match hollow_first {
                    Some(earlier) if earlier == (flag == "--hollow-first") => {
                        return Err(ParseError::DuplicateFlag(flag));
                    }
                    Some(_) => {
                        return Err(ParseError::ConflictingFlags(
                            "--hollow-first",
                            "--no-hollow-first",
                        ));
                    }
                    None => hollow_first = Some(flag == "--hollow-first"),
                }
            }
            Some(value) if value.starts_with('-') => {
                return Err(ParseError::UnknownFlag(argument.clone()));
            }
            _ => return Err(ParseError::UnexpectedPositional(argument.clone())),
        }
        index += 1;
    }

    Ok(CliAction::Prepare(PrepareSelection {
        input_models_dir: input_models_dir
            .ok_or(ParseError::MissingRequiredFlag("--input-models-dir"))?,
        output_dir: output_dir.ok_or(ParseError::MissingRequiredFlag("--out-dir"))?,
        orientation: orientation.unwrap_or_default(),
        no_orient,
        placement: placement.unwrap_or_default(),
        output_profile: output_profile.ok_or(ParseError::MissingRequiredFlag("--output"))?,
        wall_mm: match wall_mm {
            Some(selected) => selected,
            None => registered_wall_default()?,
        },
        cache,
        exact_delicacy,
        hollow_first,
        perf_dir,
    }))
}

/// `HOLLOW_WALL_DEFAULT_MM`, read through the one hollow-stage accessor so the
/// CLI default and the stage default cannot be two different numbers.
fn registered_wall_default() -> Result<PositiveFinite, ParseError> {
    registered_wall(WallSelection::Default).map_err(ParseError::RegisteredWallUnavailable)
}

fn set_value<T>(
    slot: &mut Option<T>,
    flag: &'static str,
    arguments: &[OsString],
    index: &mut usize,
    convert: impl FnOnce(OsString) -> T,
) -> Result<(), ParseError> {
    if slot.is_some() {
        return Err(ParseError::DuplicateFlag(flag));
    }
    let value = take_value(flag, arguments, index)?;
    *slot = Some(convert(value));
    Ok(())
}

fn take_value(
    flag: &'static str,
    arguments: &[OsString],
    index: &mut usize,
) -> Result<OsString, ParseError> {
    let Some(value) = arguments.get(*index + 1) else {
        return Err(ParseError::MissingFlagValue(flag));
    };
    if value.as_os_str().as_encoded_bytes().starts_with(b"--") {
        return Err(ParseError::MissingFlagValue(flag));
    }
    *index += 1;
    Ok(value.clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stable_arguments() -> [&'static str; 7] {
        [
            "prepare",
            "--input-models-dir",
            "input models",
            "--output",
            "saturn4-ultra",
            "--out-dir",
            "prepared output",
        ]
    }

    fn selection(action: CliAction) -> PrepareSelection {
        let CliAction::Prepare(selection) = action else {
            panic!("expected prepare selection");
        };
        selection
    }

    #[test]
    fn stable_command_constructs_the_reviewed_typed_selection() {
        let parsed = selection(parse_args(stable_arguments()).unwrap());
        assert_eq!(parsed.input_models_dir, PathBuf::from("input models"));
        assert_eq!(parsed.output_dir, PathBuf::from("prepared output"));
        assert_eq!(parsed.orientation, OrientSelection::Dense);
        assert_eq!(parsed.output_profile, OutputProfile::Saturn4Ultra);
        assert_eq!(parsed.cache, CacheMode::Enabled);
        assert!(!parsed.cache.no_cache());
        assert_eq!(
            parsed.wall_mm,
            registered_wall(WallSelection::Default).expect("registered wall default")
        );

        // The later resolver takes this type directly; no tuple or stringly
        // compatibility representation sits between CLI and policy assembly.
        fn later_prepare_request_resolver(_: PrepareSelection) {}
        later_prepare_request_resolver(parsed);
    }

    /// `--exact-delicacy` opts out of the protected proxy; absent, the proxy is
    /// used, and the flag can be given once.
    #[test]
    fn exact_delicacy_is_an_opt_out_given_at_most_once() {
        assert!(!selection(parse_args(stable_arguments()).unwrap()).exact_delicacy);
        let mut arguments = stable_arguments().to_vec();
        arguments.push("--exact-delicacy");
        assert!(selection(parse_args(arguments.clone()).unwrap()).exact_delicacy);
        arguments.push("--exact-delicacy");
        assert!(matches!(
            parse_args(arguments),
            Err(ParseError::DuplicateFlag("--exact-delicacy"))
        ));
    }

    /// Hollow first is on by default (the parser records no choice), is still
    /// accepted as `--hollow-first`, is opted out of with `--no-hollow-first`,
    /// each can be given once, and the two cannot be given together.
    #[test]
    fn hollow_first_is_on_by_default_with_an_opt_out_given_at_most_once() {
        assert_eq!(selection(parse_args(stable_arguments()).unwrap()).hollow_first, None);
        let mut on = stable_arguments().to_vec();
        on.push("--hollow-first");
        assert_eq!(selection(parse_args(on.clone()).unwrap()).hollow_first, Some(true));
        let mut off = stable_arguments().to_vec();
        off.push("--no-hollow-first");
        assert_eq!(selection(parse_args(off.clone()).unwrap()).hollow_first, Some(false));
        on.push("--hollow-first");
        assert!(matches!(
            parse_args(on),
            Err(ParseError::DuplicateFlag("--hollow-first"))
        ));
        off.push("--no-hollow-first");
        assert!(matches!(
            parse_args(off),
            Err(ParseError::DuplicateFlag("--no-hollow-first"))
        ));
        let mut both = stable_arguments().to_vec();
        both.extend(["--hollow-first", "--no-hollow-first"]);
        assert!(matches!(
            parse_args(both),
            Err(ParseError::ConflictingFlags("--hollow-first", "--no-hollow-first"))
        ));
    }

    /// `--perf DIR` only names where the write-only counters go; absent, no
    /// counting happens, and it can be given once.
    #[test]
    fn perf_names_a_sidecar_folder_and_defaults_to_off() {
        assert_eq!(selection(parse_args(stable_arguments()).unwrap()).perf_dir, None);
        let mut arguments = stable_arguments().to_vec();
        arguments.extend(["--perf", "counters"]);
        assert_eq!(
            selection(parse_args(arguments.clone()).unwrap()).perf_dir,
            Some(PathBuf::from("counters"))
        );
        arguments.extend(["--perf", "again"]);
        assert!(matches!(
            parse_args(arguments),
            Err(ParseError::DuplicateFlag("--perf"))
        ));
    }

    /// The three sanctioned spellings, and nothing between them.
    ///
    /// Parsing opens no file: `pinned:DIR` records the path and the directory
    /// scan happens in `execute`, so this test can name a directory that does
    /// not exist and still describe the parser completely.
    #[test]
    fn orient_accepts_exactly_dense_none_and_pinned_and_defaults_to_dense() {
        let with = |mode: &str| {
            let mut arguments = stable_arguments().to_vec();
            arguments.extend(["--orient", mode]);
            parse_args(arguments)
        };

        assert_eq!(
            selection(with("dense").unwrap()).orientation,
            OrientSelection::Dense
        );
        assert_eq!(
            selection(with("pinned:corpus/baseline").unwrap()).orientation,
            OrientSelection::Pinned(PathBuf::from("corpus/baseline"))
        );
        // GitHub issue #1: every model exactly as drawn.
        assert_eq!(
            selection(with("none").unwrap()).orientation,
            OrientSelection::AsDrawn
        );
        // An omitted flag is the dense sweep, which is the recorded M8 default.
        assert_eq!(
            selection(parse_args(stable_arguments()).unwrap()).orientation,
            OrientSelection::Dense
        );

        for rejected in [
            "",
            "pinned",
            "pinned:",
            "Dense",
            "legacy_coarse",
            "legacy-13",
            "None",
            "as-drawn",
            "identity",
        ] {
            assert_eq!(
                with(rejected),
                Err(ParseError::UnknownOrientMode(rejected.to_owned())),
                "{rejected:?} must be refused rather than guessed at"
            );
        }

        let mut twice = stable_arguments().to_vec();
        twice.extend(["--orient", "dense", "--orient", "dense"]);
        assert_eq!(
            parse_args(twice),
            Err(ParseError::DuplicateFlag("--orient"))
        );
    }

    /// `--no-orient=LIST` (owner spec 2026-10-01): the inline form the owner
    /// wrote and the separate-value form every other flag takes; the list is
    /// split, trimmed and kept in order; a second flag adds to the first; an
    /// empty list is a missing value. Parsing opens no folder, so a name that
    /// matches nothing is NOT refused here (the run warns about it).
    #[test]
    fn no_orient_takes_an_inline_or_separate_list_and_accumulates() {
        let with = |extra: &[&str]| {
            let mut arguments = stable_arguments().to_vec();
            arguments.extend(extra);
            parse_args(arguments)
        };
        assert!(selection(parse_args(stable_arguments()).unwrap())
            .no_orient
            .is_empty());
        assert_eq!(
            selection(with(&["--no-orient=Round_*, make_overhang_table.stl"]).unwrap())
                .no_orient
                .entries(),
            ["Round_*", "make_overhang_table.stl"]
        );
        assert_eq!(
            selection(with(&["--no-orient", "Morel base,Sphere"]).unwrap())
                .no_orient
                .entries(),
            ["Morel base", "Sphere"]
        );
        let both = selection(
            with(&[
                "--no-orient=Round_25",
                "--orient",
                "dense",
                "--no-orient",
                "Round_50,Round_25",
            ])
            .unwrap(),
        );
        assert_eq!(both.no_orient.entries(), ["Round_25", "Round_50"]);
        assert_eq!(both.orientation, OrientSelection::Dense);
        for empty in [&["--no-orient="][..], &["--no-orient=, ,"], &["--no-orient"]] {
            assert_eq!(
                with(empty),
                Err(ParseError::MissingFlagValue("--no-orient")),
                "{empty:?}"
            );
        }
        assert_eq!(
            with(&["--no-orientation=x"]),
            Err(ParseError::UnknownFlag("--no-orientation=x".into()))
        );
    }

    /// The two sanctioned placement spellings, and nothing between them.
    ///
    /// An omitted flag is the LEGACY engine. That matters more than the usual
    /// default-flag pin: the sweep path runs no pre-write gate at all, so a
    /// default that drifted to `sweep` would silently un-gate every run in
    /// this repository.
    #[test]
    fn placement_accepts_exactly_legacy_and_sweep_and_defaults_to_legacy() {
        let with = |mode: &str| {
            let mut arguments = stable_arguments().to_vec();
            arguments.extend(["--placement", mode]);
            parse_args(arguments)
        };

        assert_eq!(
            selection(with("legacy").unwrap()).placement,
            PlacementSelection::Legacy
        );
        assert_eq!(
            selection(with("sweep").unwrap()).placement,
            PlacementSelection::Sweep
        );
        assert_eq!(
            selection(parse_args(stable_arguments()).unwrap()).placement,
            PlacementSelection::Legacy,
            "an omitted --placement must be the gated legacy engine"
        );

        for rejected in ["", "Sweep", "slice-sweep", "slice_sweep", "new", "Legacy"] {
            assert_eq!(
                with(rejected),
                Err(ParseError::UnknownPlacementMode(rejected.to_owned())),
                "{rejected:?} must be refused rather than guessed at"
            );
        }

        let mut twice = stable_arguments().to_vec();
        twice.extend(["--placement", "sweep", "--placement", "legacy"]);
        assert_eq!(
            parse_args(twice),
            Err(ParseError::DuplicateFlag("--placement"))
        );
    }

    /// The selector reaches the pipeline as the pipeline's own enum, with no
    /// stringly step in between.
    #[test]
    fn the_placement_selection_maps_onto_the_pipeline_enum_both_ways() {
        assert_eq!(Placement::from(PlacementSelection::Legacy), Placement::Legacy);
        assert_eq!(Placement::from(PlacementSelection::Sweep), Placement::Sweep);
        // The pipeline's own default is the gated engine too, so a caller that
        // builds a `PrepareRequest` with `..Default::default()` cannot land on
        // the ungated path by omission.
        assert_eq!(Placement::default(), Placement::Legacy);
        assert_eq!(PlacementSelection::default(), PlacementSelection::Legacy);
    }

    /// `--no-cache` is the ONE cache-off spelling `06-conventions.md` § 6
    /// defines, and cache-disabled equality is CI-mandatory, so this pins two
    /// things at once: that the mandated spelling is accepted, and that no
    /// second spelling is. A second way to disable the cache is a second thing
    /// that can silently stop working.
    ///
    /// It stays accepted while it is still a no-op — no cache exists to bypass
    /// yet. Deleting the flag for having no reader is what this test now
    /// guards against; that deletion was made on 2026-08-26 and reverted the
    /// same day.
    #[test]
    fn no_cache_is_the_only_cache_off_form() {
        let mut arguments = stable_arguments().map(OsString::from).to_vec();
        arguments.push("--no-cache".into());
        let parsed = selection(parse_args(arguments).unwrap());
        assert_eq!(parsed.cache, CacheMode::Disabled);
        assert!(parsed.cache.no_cache());
        for alias in ["--cache=false", "--no_cache", "--disable-cache"] {
            let mut arguments = stable_arguments().map(OsString::from).to_vec();
            arguments.push(alias.into());
            assert_eq!(
                parse_args(arguments),
                Err(ParseError::UnknownFlag(alias.into()))
            );
        }
    }

    #[test]
    #[expect(
        clippy::float_cmp,
        reason = "test-local arithmetic and exact pins over hand-written, bounded fixture values. See \
              docs/evidence/remediation-2026-08-26/census/cast-audit.md"
    )]
    fn wall_mm_is_optional_and_defaults_to_the_registered_constant() {
        let registered =
            registered_wall(WallSelection::Default).expect("registered wall default must publish");
        assert_eq!(registered.get(), 2.5, "the registered default moved");
        assert_eq!(
            selection(parse_args(stable_arguments()).unwrap()).wall_mm,
            registered,
            "an omitted --wall-mm must take the registry value, not a CLI literal"
        );

        // The handling-heavy value is a registered constant an operator may
        // spell out; the parser has no classifier that picks it.
        let heavy = registered_wall(WallSelection::HandlingHeavy)
            .expect("registered handling-heavy wall must publish");
        assert_eq!(heavy.get(), 3.0);
        let mut arguments = stable_arguments().map(OsString::from).to_vec();
        arguments.extend(["--wall-mm".into(), OsString::from("3.0")]);
        assert_eq!(selection(parse_args(arguments).unwrap()).wall_mm, heavy);
    }

    #[test]
    #[expect(
        clippy::float_cmp,
        reason = "test-local arithmetic and exact pins over hand-written, bounded fixture values. See \
              docs/evidence/remediation-2026-08-26/census/cast-audit.md"
    )]
    fn wall_mm_refuses_a_wall_finer_than_the_frozen_voxel_pitch() {
        let pitch = plinth_hollow::minimum_representable_wall_mm().expect("frozen pitch publishes");
        assert_eq!(pitch.get(), 0.5, "the frozen voxel pitch moved");

        // Sub-pitch walls parse as positive finite numbers and would have
        // threaded straight to a cavity identical to every other sub-pitch
        // wall's. The parser refuses them at the same bound the stage does.
        for value in ["1e-9", "0.001", "0.25", "0.4", "0.499999999"] {
            let mut arguments = stable_arguments().map(OsString::from).to_vec();
            arguments.extend(["--wall-mm".into(), OsString::from(value)]);
            assert_eq!(
                parse_args(arguments),
                Err(ParseError::InvalidWallMm(value.to_owned())),
                "sub-pitch wall {value:?} must be refused by name"
            );
        }

        // The pitch itself is admissible, and so is anything above it.
        for value in ["0.5", "0.500000001", "0.75", "12"] {
            let mut arguments = stable_arguments().map(OsString::from).to_vec();
            arguments.extend(["--wall-mm".into(), OsString::from(value)]);
            assert_eq!(
                selection(parse_args(arguments).unwrap()).wall_mm.get(),
                value.parse::<f64>().unwrap(),
                "wall {value:?} is at or above the pitch and must be accepted verbatim"
            );
        }
    }

    #[test]
    fn wall_mm_accepts_only_a_positive_finite_millimetre_value() {
        for value in ["0", "-1.5", "nan", "inf", "2.5mm", "", "thick"] {
            let mut arguments = stable_arguments().map(OsString::from).to_vec();
            arguments.extend(["--wall-mm".into(), OsString::from(value)]);
            assert_eq!(
                parse_args(arguments),
                Err(ParseError::InvalidWallMm(value.to_owned())),
                "value {value:?} must be refused by name"
            );
        }

        let mut arguments = stable_arguments().map(OsString::from).to_vec();
        arguments.extend([
            "--wall-mm".into(),
            OsString::from("3.0"),
            "--wall-mm".into(),
            OsString::from("2.5"),
        ]);
        assert_eq!(
            parse_args(arguments),
            Err(ParseError::DuplicateFlag("--wall-mm"))
        );

        let mut arguments = stable_arguments().map(OsString::from).to_vec();
        arguments.push("--wall-mm".into());
        assert_eq!(
            parse_args(arguments),
            Err(ParseError::MissingFlagValue("--wall-mm"))
        );

        for alias in ["--wall", "--wall_mm", "--wall-thickness-mm"] {
            let mut arguments = stable_arguments().map(OsString::from).to_vec();
            arguments.extend([alias.into(), OsString::from("3.0")]);
            assert_eq!(
                parse_args(arguments),
                Err(ParseError::UnknownFlag(alias.into()))
            );
        }
    }

    #[test]
    fn required_flags_accept_all_argument_order_permutations() {
        let groups = [
            ["--input-models-dir", "input models"],
            ["--output", "saturn4-ultra"],
            ["--out-dir", "prepared output"],
        ];
        for order in [
            [0, 1, 2],
            [0, 2, 1],
            [1, 0, 2],
            [1, 2, 0],
            [2, 0, 1],
            [2, 1, 0],
        ] {
            let mut arguments = vec![OsString::from("prepare")];
            for group in order {
                arguments.extend(groups[group].map(OsString::from));
            }
            assert_eq!(
                selection(parse_args(arguments).unwrap()).output_profile,
                OutputProfile::Saturn4Ultra
            );
        }
    }

    #[test]
    fn missing_duplicate_unknown_and_positional_inputs_are_named() {
        assert_eq!(
            parse_args(Vec::<OsString>::new()),
            Err(ParseError::MissingSubcommand)
        );
        assert_eq!(
            parse_args(["score"]),
            Err(ParseError::UnknownSubcommand("score".into()))
        );
        assert_eq!(
            parse_args(["prepare", "--output", "saturn4-ultra"]),
            Err(ParseError::MissingRequiredFlag("--input-models-dir"))
        );
        assert_eq!(
            parse_args([
                "prepare",
                "--input-models-dir",
                "one",
                "--input-models-dir",
                "two",
            ]),
            Err(ParseError::DuplicateFlag("--input-models-dir"))
        );
        assert_eq!(
            parse_args(["prepare", "--output", "--out-dir", "out"]),
            Err(ParseError::MissingFlagValue("--output"))
        );
        assert_eq!(
            parse_args(["prepare", "--mystery"]),
            Err(ParseError::UnknownFlag("--mystery".into()))
        );
        assert_eq!(
            parse_args(["prepare", "models"]),
            Err(ParseError::UnexpectedPositional("models".into()))
        );
    }

    #[test]
    fn output_profile_spelling_is_exact() {
        for value in ["Saturn4-Ultra", "saturn4_ultra", "saturn4-ultra "] {
            let mut arguments = stable_arguments().map(OsString::from).to_vec();
            arguments[4] = value.into();
            assert_eq!(
                parse_args(arguments),
                Err(ParseError::UnknownOutputProfile(value.to_owned()))
            );
        }
    }
}
