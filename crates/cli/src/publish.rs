//! Whole-or-nothing file output (PRD FR-42, FR-37): bytes go to a hidden temp file on the
//! output volume, are closed and synced, and only then take their final name. A final name
//! never holds a partial file, and an existing file is never replaced: there is no
//! overwrite mode.

use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

/// A simulated failure, for the interrupted-write, full-disk and crash tests.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Fault {
    /// No fault: production runs.
    #[default]
    None,
    /// The write stops with an `Interrupted` error after this many bytes.
    Interrupt {
        /// Bytes written before the error.
        after: usize,
    },
    /// The disk is full after this many bytes (the OS error ENOSPC).
    DiskFull {
        /// Bytes written before the error.
        after: usize,
    },
    /// The process dies after the checker, before publishing: the temp file stays behind.
    CrashBeforePublish,
}

/// Which model gets which fault. Production runs use [`Faults::default`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Faults {
    /// Index of the model (in sorted input order) the fault hits.
    pub model: usize,
    /// The fault.
    pub fault: Fault,
}

impl Faults {
    /// The fault for the model at `model` (in sorted input order).
    #[must_use]
    pub fn for_model(&self, model: usize) -> Fault {
        if self.model == model {
            self.fault
        } else {
            Fault::None
        }
    }
}

/// The OS error number for "no space left on device".
pub const ENOSPC: i32 = 28;

/// Temp name in `dir`: hidden, never a final name, and different on every call (process id,
/// clock and a counter), so a file left by a crashed run is never reused. The name is
/// bounded (about 60 bytes with a short `tag`) and does not contain the final name, so any
/// final name that fits the filesystem also has a temp name that fits.
#[must_use]
pub fn temp_path(dir: &Path, tag: &str) -> PathBuf {
    static CALLS: AtomicU64 = AtomicU64::new(0);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos());
    let n = CALLS.fetch_add(1, Ordering::Relaxed);
    dir.join(format!(
        ".plinth-{tag}.tmp-{}-{nanos}-{n}",
        std::process::id()
    ))
}

/// Write `bytes` to a new temp file at `path` (it must not exist), then sync it.
///
/// # Errors
/// Any I/O error, or the injected fault. On error the temp file is removed.
pub fn write_temp(path: &Path, bytes: &[u8], fault: Fault) -> io::Result<()> {
    // Opening fails if the name exists, before anything is removed: a file this call did
    // not create is never deleted.
    let mut f: File = OpenOptions::new().write(true).create_new(true).open(path)?;
    let result = (|| {
        match fault {
            Fault::Interrupt { after } => {
                f.write_all(&bytes[..after.min(bytes.len())])?;
                return Err(io::Error::new(
                    io::ErrorKind::Interrupted,
                    "write interrupted",
                ));
            }
            Fault::DiskFull { after } => {
                f.write_all(&bytes[..after.min(bytes.len())])?;
                return Err(io::Error::from_raw_os_error(ENOSPC));
            }
            Fault::None | Fault::CrashBeforePublish => {}
        }
        f.write_all(bytes)?;
        f.sync_all()
    })();
    drop(f);
    if result.is_err() {
        let _ = fs::remove_file(path);
    }
    result
}

/// Give `temp` the name `dest`, never replacing an existing file (a hard link is atomic and
/// fails if the name exists). The temp name is removed either way.
///
/// # Errors
/// `AlreadyExists` when `dest` exists (it is left untouched); any other I/O error,
/// including a volume that cannot hard link.
pub fn publish(temp: &Path, dest: &Path) -> io::Result<()> {
    let result = fs::hard_link(temp, dest);
    let _ = fs::remove_file(temp);
    result?;
    // Best effort: make the new name durable. A failure here does not undo the publish.
    if let Some(dir) = dest.parent().filter(|d| !d.as_os_str().is_empty())
        && let Ok(d) = File::open(dir)
    {
        let _ = d.sync_all();
    }
    Ok(())
}

/// Publish a portable bundle with the model as its commit record. All files are
/// synced in staging first; reports and pictures are linked into a newly reserved
/// real directory, and the model is linked LAST. Thus a visible model always has
/// its complete review set. No symlink or temporary backing store survives success.
/// A crash before the model link can leave review files, never a published model.
/// Existing directories and files are never replaced.
pub fn publish_bundle(temp: &Path, dest: &Path) -> io::Result<()> {
    fn files(dir: &Path, relative: &Path, out: &mut Vec<PathBuf>) -> io::Result<()> {
        for entry in fs::read_dir(dir.join(relative))? {
            let entry = entry?;
            let path = relative.join(entry.file_name());
            if entry.file_type()?.is_dir() {
                files(dir, &path, out)?;
            } else if entry.file_type()?.is_file() {
                File::open(entry.path())?.sync_all()?;
                out.push(path);
            } else {
                return Err(io::Error::other("bundle contains a non-regular file"));
            }
        }
        File::open(dir.join(relative))?.sync_all()
    }
    let mut paths = Vec::new();
    files(temp, Path::new(""), &mut paths)?;
    let is_model = |p: &PathBuf| p.extension().is_some_and(|x| x == "chitubox");
    if paths.iter().filter(|p| is_model(p)).count() != 1 {
        return Err(io::Error::other(
            "bundle needs exactly one model commit record",
        ));
    }
    paths.sort_by_key(|p| (is_model(p), p.clone()));
    fs::create_dir(dest)?; // Atomic no-replace reservation; owned only on success.
    let result = (|| {
        for path in paths {
            let final_path = dest.join(&path);
            let parent = final_path
                .parent()
                .ok_or_else(|| io::Error::other("no parent"))?;
            fs::create_dir_all(parent)?;
            fs::hard_link(temp.join(&path), &final_path)?;
            File::open(parent)?.sync_all()?;
        }
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_dir_all(dest);
    } else {
        // Cleanup is not part of acceptance: all final files have independent names.
        let _ = fs::remove_dir_all(temp);
        if let Some(parent) = dest.parent() {
            let _ = File::open(parent).and_then(|f| f.sync_all());
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dir(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("plinth-publish-{tag}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        d
    }

    fn names(d: &Path) -> Vec<String> {
        let mut v: Vec<String> = fs::read_dir(d)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        v.sort();
        v
    }

    #[test]
    fn publish_moves_the_file_and_leaves_no_temp() {
        let d = dir("ok");
        let t = temp_path(&d, "model");
        write_temp(&t, b"hello", Fault::None).unwrap();
        publish(&t, &d.join("a.chitubox")).unwrap();
        assert_eq!(names(&d), ["a.chitubox"]);
        assert_eq!(fs::read(d.join("a.chitubox")).unwrap(), b"hello");
        fs::remove_dir_all(&d).unwrap();
    }

    #[test]
    fn interrupted_and_full_disk_writes_leave_nothing() {
        let d = dir("fault");
        for fault in [
            Fault::Interrupt { after: 3 },
            Fault::DiskFull { after: 3 },
            Fault::DiskFull { after: 0 },
        ] {
            let t = temp_path(&d, "model");
            let e = write_temp(&t, b"hello world", fault).unwrap_err();
            match fault {
                Fault::Interrupt { .. } => assert_eq!(e.kind(), io::ErrorKind::Interrupted),
                _ => assert_eq!(e.raw_os_error(), Some(ENOSPC)),
            }
            assert!(names(&d).is_empty(), "{fault:?}: {:?}", names(&d));
        }
        fs::remove_dir_all(&d).unwrap();
    }

    #[test]
    fn an_existing_name_is_never_replaced() {
        let d = dir("clash");
        fs::write(d.join("a.chitubox"), b"first").unwrap();
        let t = temp_path(&d, "model");
        write_temp(&t, b"second", Fault::None).unwrap();
        let e = publish(&t, &d.join("a.chitubox")).unwrap_err();
        assert_eq!(e.kind(), io::ErrorKind::AlreadyExists);
        assert_eq!(fs::read(d.join("a.chitubox")).unwrap(), b"first");
        assert_eq!(names(&d), ["a.chitubox"], "temp removed");
        fs::remove_dir_all(&d).unwrap();
    }

    #[test]
    fn a_temp_name_is_short_and_independent_of_the_final_name() {
        let t = temp_path(Path::new("out"), "model");
        let name = t.file_name().unwrap().to_string_lossy().into_owned();
        assert!(name.len() < 80, "{name}");
        assert!(name.starts_with(".plinth-model.tmp-"), "{name}");
        assert_ne!(t, temp_path(Path::new("out"), "model"));
    }

    #[test]
    fn supported_bundle_is_portable_and_never_replaces_a_directory() {
        let d = dir("bundle");
        let staged = d.join("stage");
        let dest = d.join("ready");
        fs::create_dir_all(staged.join("pictures")).unwrap();
        fs::write(staged.join("pictures/01.png"), b"picture").unwrap();
        fs::write(staged.join("model.fr21.json"), b"report").unwrap();
        fs::write(staged.join("model.chitubox"), b"model").unwrap();
        fs::create_dir(&dest).unwrap();
        assert_eq!(
            publish_bundle(&staged, &dest).unwrap_err().kind(),
            io::ErrorKind::AlreadyExists
        );
        assert!(names(&dest).is_empty());
        fs::remove_dir(&dest).unwrap();
        publish_bundle(&staged, &dest).unwrap();
        assert!(
            !fs::symlink_metadata(&dest)
                .unwrap()
                .file_type()
                .is_symlink()
        );
        assert!(!staged.exists(), "no temporary backing directory");
        // Relocating the whole visible directory cannot break any file link.
        let moved = d.join("moved");
        fs::rename(&dest, &moved).unwrap();
        assert_eq!(fs::read(moved.join("model.chitubox")).unwrap(), b"model");
        assert_eq!(fs::read(moved.join("pictures/01.png")).unwrap(), b"picture");
        fs::remove_dir_all(d).unwrap();
    }

    #[test]
    fn invalid_bundle_never_exposes_a_model() {
        let d = dir("invalid-bundle");
        let staged = d.join("stage");
        fs::create_dir(&staged).unwrap();
        fs::write(staged.join("model.chitubox"), b"model").unwrap();
        std::os::unix::fs::symlink("missing-picture", staged.join("01.png")).unwrap();
        assert!(publish_bundle(&staged, &d.join("ready")).is_err());
        assert!(!d.join("ready").exists());
        fs::remove_dir_all(d).unwrap();
    }

    #[test]
    fn a_temp_name_is_never_reused_silently() {
        let d = dir("temp-clash");
        let t = temp_path(&d, "model");
        fs::write(&t, b"left by a crashed run").unwrap();
        let e = write_temp(&t, b"new", Fault::None).unwrap_err();
        assert_eq!(e.kind(), io::ErrorKind::AlreadyExists);
        // A failed attempt must not delete a file it did not create.
        assert_eq!(fs::read(&t).unwrap(), b"left by a crashed run");
        fs::remove_dir_all(&d).unwrap();
    }
}
