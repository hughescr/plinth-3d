//! Small helpers shared by the subcommands: an exit value, git calls, path lookup.

use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};

/// Why a subcommand stops: the exit code and the message to print.
#[derive(Debug)]
pub struct Exit {
    pub code: i32,
    pub msg: String,
}

impl Exit {
    /// Exit code 4: setup error or refusal.
    pub fn setup(msg: impl Into<String>) -> Self {
        Self {
            code: 4,
            msg: msg.into(),
        }
    }
}

/// Run a program and return its trimmed stdout, or its stderr as the error.
pub fn out(prog: &str, args: &[&str], dir: &Path) -> Result<String, String> {
    let o = Command::new(prog)
        .args(args)
        .current_dir(dir)
        .stdin(Stdio::null())
        .output()
        .map_err(|e| format!("cannot run {prog}: {e}"))?;
    if o.status.success() {
        Ok(String::from_utf8_lossy(&o.stdout).trim().to_owned())
    } else {
        Err(format!(
            "{prog} {}: {}",
            args.join(" "),
            String::from_utf8_lossy(&o.stderr).trim()
        ))
    }
}

/// `git -C dir args...` with trimmed stdout.
pub fn git(dir: &Path, args: &[&str]) -> Result<String, String> {
    out("git", args, dir)
}

/// True if the git command succeeds (its output is ignored).
#[must_use]
pub fn git_ok(dir: &Path, args: &[&str]) -> bool {
    Command::new("git")
        .args(args)
        .current_dir(dir)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}

/// The first 12 characters of the git object hash of `bytes` (a content fingerprint).
#[must_use]
pub fn fingerprint(dir: &Path, bytes: &[u8]) -> String {
    let child = Command::new("git")
        .args(["hash-object", "--stdin"])
        .current_dir(dir)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn();
    let Ok(mut child) = child else {
        return "unknown".into();
    };
    if let Some(mut stdin) = child.stdin.take() {
        let _ = stdin.write_all(bytes);
    }
    let text = child
        .wait_with_output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_owned());
    text.map_or_else(|_| "unknown".into(), |t| t.chars().take(12).collect())
}

/// The local time as `2026-10-30T15:03:04` and the Unix time in seconds.
#[must_use]
pub fn now_stamp() -> (String, u64) {
    let text = out("date", &["+%Y-%m-%dT%H:%M:%S"], Path::new("/")).unwrap_or_default();
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    (text, secs)
}

fn is_exe(p: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    std::fs::metadata(p).is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
}

/// True if `name` is an executable file in some PATH directory.
#[must_use]
pub fn on_path(name: &str) -> bool {
    let Some(path) = std::env::var_os("PATH") else {
        return false;
    };
    std::env::split_paths(&path).any(|d| is_exe(&d.join(name)))
}

/// True if `name` is on PATH or in Cargo's bin directory. Cargo finds its `cargo-*` subcommands
/// there even when the directory is not on PATH.
#[must_use]
pub fn has_tool(name: &str) -> bool {
    let cargo_home = std::env::var_os("CARGO_HOME")
        .map(std::path::PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| std::path::PathBuf::from(h).join(".cargo")));
    on_path(name) || cargo_home.is_some_and(|h| is_exe(&h.join("bin").join(name)))
}

/// A positive integer from the environment, or `default`. A bad value is an error, not a default.
pub fn env_count(name: &str, default: u32) -> Result<u32, String> {
    match std::env::var(name) {
        Err(_) => Ok(default),
        Ok(v) => v
            .parse::<u32>()
            .ok()
            .filter(|n| *n > 0)
            .ok_or_else(|| format!("{name} must be a positive integer, got '{v}'")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn out_returns_trimmed_stdout_and_errors_on_failure() {
        assert_eq!(out("echo", &["hi"], Path::new("/")).as_deref(), Ok("hi"));
        assert!(out("false", &[], Path::new("/")).is_err());
        assert!(out("/nonexistent/prog", &[], Path::new("/")).is_err());
    }

    #[test]
    fn on_path_finds_sh_and_not_nonsense() {
        assert!(on_path("sh"));
        assert!(!on_path("definitely-not-a-program-devloop"));
    }

    #[test]
    fn fingerprint_is_stable_and_content_sensitive() {
        let a = fingerprint(Path::new("/"), b"abc");
        assert_eq!(a.len(), 12);
        assert_eq!(a, fingerprint(Path::new("/"), b"abc"));
        assert_ne!(a, fingerprint(Path::new("/"), b"abd"));
    }
}
