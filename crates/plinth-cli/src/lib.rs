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
use plinth_pipeline::OutputProfile;

pub const ROOT_USAGE: &str = "Usage: plinth <subcommand>\n\nSubcommands:\n  prepare  Prepare every source model in a directory\n\nOptions:\n  -h, --help     Show help\n  -V, --version  Show version";

pub const PREPARE_USAGE: &str = "Usage: plinth prepare --input-models-dir DIR --output saturn4-ultra --out-dir DIR [--orient dense|pinned:DIR] [--wall-mm MM] [--no-cache]\n\nOptions:\n  --input-models-dir DIR  Directory containing source models\n  --output PROFILE       Output profile; exactly: saturn4-ultra\n  --out-dir DIR          Destination directory\n  --orient MODE          dense (default) searches every model's build orientation; pinned:DIR replays the rotation DIR/<stem>.pipeline.json records, per model by stem\n  --wall-mm MM           Hollow shell wall thickness in mm, no finer than the frozen hollow voxel pitch; omit for the registered default, raise it for a piece that gets handled\n  --no-cache             Disable cache use for this request\n  -h, --help             Show help";

/// Whether this request may use a `plinth-pipeline` cache.
///
/// **No cache exists yet, so `Disabled` changes no computation.** The spelling
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
    pub output_profile: OutputProfile,
    /// The hollow shell wall the operator selected for every model in this
    /// request, defaulted from `HOLLOW_WALL_DEFAULT_MM` when `--wall-mm` is
    /// absent. Nothing here infers it from the model; the resolver hands this
    /// value straight to `plinth_hollow::HollowRequest::wall_mm`.
    pub wall_mm: PositiveFinite,
    pub cache: CacheMode,
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
    MissingFlagValue(&'static str),
    UnknownFlag(OsString),
    UnexpectedPositional(OsString),
    NonUnicodeOutputProfile(OsString),
    UnknownOutputProfile(String),
    NonUnicodeWallMm(OsString),
    NonUnicodeOrientMode(OsString),
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
            Self::UnknownOrientMode(value) => write!(
                formatter,
                "unknown orientation mode {value:?}; expected dense or pinned:DIR"
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
    let mut cache = CacheMode::Enabled;
    let mut no_cache_seen = false;
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
            Some("--no-cache") => {
                if no_cache_seen {
                    return Err(ParseError::DuplicateFlag("--no-cache"));
                }
                no_cache_seen = true;
                cache = CacheMode::Disabled;
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
        output_profile: output_profile.ok_or(ParseError::MissingRequiredFlag("--output"))?,
        wall_mm: match wall_mm {
            Some(selected) => selected,
            None => registered_wall_default()?,
        },
        cache,
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

    /// The two sanctioned spellings, and nothing between them.
    ///
    /// Parsing opens no file: `pinned:DIR` records the path and the directory
    /// scan happens in `execute`, so this test can name a directory that does
    /// not exist and still describe the parser completely.
    #[test]
    fn orient_accepts_exactly_dense_and_pinned_and_defaults_to_dense() {
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
