//! Typed parser for the stable `plinth` command-line surface.
//!
//! Parsing deliberately stops before execution policy is assembled. The later
//! pipeline resolver consumes [`PrepareSelection`] and owns construction of a
//! complete `plinth_pipeline::PrepareRequest`, including all resource and
//! physics values which are not CLI arguments.

#![forbid(unsafe_code)]

use std::ffi::OsString;
use std::path::PathBuf;

use plinth_pipeline::{OrientationPolicy, OutputProfile};

pub const ROOT_USAGE: &str = "Usage: plinth <subcommand>\n\nSubcommands:\n  prepare  Prepare every source model in a directory\n\nOptions:\n  -h, --help     Show help\n  -V, --version  Show version";

pub const PREPARE_USAGE: &str = "Usage: plinth prepare --input-models-dir DIR --output saturn4-ultra --out-dir DIR [--no-cache]\n\nOptions:\n  --input-models-dir DIR  Directory containing source models\n  --output PROFILE       Output profile; exactly: saturn4-ultra\n  --out-dir DIR          Destination directory\n  --no-cache             Disable cache use for this request\n  -h, --help             Show help";

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

/// The complete user-selected CLI state and the compile-time handoff seam for
/// a later resource resolver. It is intentionally not a partial or fabricated
/// `PrepareRequest`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PrepareSelection {
    pub input_models_dir: PathBuf,
    pub output_dir: PathBuf,
    pub orientation: OrientationPolicy,
    pub output_profile: OutputProfile,
    pub cache: CacheMode,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CliAction {
    RootHelp,
    PrepareHelp,
    Version,
    Prepare(PrepareSelection),
}

#[derive(Clone, Debug, PartialEq, Eq)]
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
        orientation: OrientationPolicy::LegacyCoarse,
        output_profile: output_profile.ok_or(ParseError::MissingRequiredFlag("--output"))?,
        cache,
    }))
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
        assert_eq!(parsed.orientation, OrientationPolicy::LegacyCoarse);
        assert_eq!(parsed.output_profile, OutputProfile::Saturn4Ultra);
        assert_eq!(parsed.cache, CacheMode::Enabled);
        assert!(!parsed.cache.no_cache());

        // The later resolver takes this type directly; no tuple or stringly
        // compatibility representation sits between CLI and policy assembly.
        fn later_prepare_request_resolver(_: PrepareSelection) {}
        later_prepare_request_resolver(parsed);
    }

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
