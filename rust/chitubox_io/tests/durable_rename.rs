//! T3.3-8: `Project::save` must not silently return `Ok` when the directory
//! fsync that makes its rename durable cannot happen.
//!
//! `save` writes to a same-directory temp file, `fsync`s the bytes, and
//! renames onto the target -- that makes the *content* durable. It also
//! needs to `fsync` the directory afterwards, because a `rename` can return
//! `Ok` while the directory entry it just repointed is still only in the
//! filesystem's in-memory page cache; on a filesystem that requires an
//! explicit directory `fsync` for that entry to survive a crash, a power
//! loss right after `save` returns `Ok` could leave the target name absent
//! or still pointing at whatever occupied it before, even though the rename
//! syscall itself succeeded.

#[cfg(unix)]
mod unix_only {
    use chitubox_io::Project;
    use std::fs;
    use std::os::unix::fs::PermissionsExt;

    /// A directory with `--wx` permission (write + search, no read) still
    /// permits `save`'s own temp-file create and rename -- POSIX only
    /// requires search and write on the containing directory for those --
    /// but `File::open` on the directory itself, which is what `fsync`ing
    /// it requires, fails with `EACCES`. That gives a deterministic way to
    /// make the rename succeed and the durability sync fail, without
    /// touching the filesystem's actual crash behaviour.
    ///
    /// Before this fix, `save` never attempted the directory sync at all
    /// and unconditionally returned `Ok` once the rename succeeded, so this
    /// test fails against that code: `result` comes back `Ok`.
    #[test]
    fn save_fails_loudly_when_the_target_directory_cannot_be_synced() {
        let dir = std::env::temp_dir().join(format!(
            "chitubox_io_dir_sync_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&dir).expect("create isolated test directory");
        let target = dir.join("out.chitubox");

        fs::set_permissions(&dir, fs::Permissions::from_mode(0o300))
            .expect("restrict directory to write+search, no read");

        // This test's premise is that restricting read permission on `dir`
        // makes `File::open(dir)` fail with EACCES -- root (and some CI
        // containers/filesystems that don't enforce POSIX permission bits)
        // can open it anyway, which would make the assertions below fail
        // for a reason that has nothing to do with `save`'s own behaviour.
        // Detect that directly, on the exact operation the fix performs,
        // rather than guessing from the effective uid.
        if fs::File::open(&dir).is_ok() {
            fs::set_permissions(&dir, fs::Permissions::from_mode(0o700)).ok();
            fs::remove_dir_all(&dir).ok();
            eprintln!(
                "skipping save_fails_loudly_when_the_target_directory_cannot_be_synced: \
                 this process can open a --wx directory for reading (running as root, or a \
                 filesystem/container that does not enforce POSIX permission bits), so the \
                 EACCES this test relies on cannot be produced here"
            );
            return;
        }

        let project = Project::new_empty();
        let expected_bytes = project.to_bytes().expect("serialize fixture project");
        let result = project.save(&target);

        // Restore permissions before any assertion can panic and skip cleanup.
        fs::set_permissions(&dir, fs::Permissions::from_mode(0o700)).ok();
        let bytes_present = fs::read(&target).ok();
        fs::remove_dir_all(&dir).ok();

        assert!(
            result.is_err(),
            "save() returned Ok even though the target directory could not \
             be fsynced after the rename -- the rename's durability was \
             never verified"
        );
        assert_eq!(
            bytes_present.as_deref(),
            Some(expected_bytes.as_slice()),
            "the rename itself should still have succeeded (only the \
             directory's durability sync should have failed), so the \
             correct bytes should still be on disk at the renamed name"
        );
    }

    /// The mirror of the test above: under NORMAL permissions the directory
    /// sync this fix adds must not turn a previously-successful save into a
    /// failure. Without this, a fix that made the sync overly strict (or
    /// applied it to the wrong path) could pass the EACCES test above by
    /// accident while breaking the ordinary case.
    #[test]
    fn save_still_succeeds_when_the_target_directory_is_fully_accessible() {
        let dir = std::env::temp_dir().join(format!(
            "chitubox_io_dir_sync_ok_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&dir).expect("create isolated test directory");
        let target = dir.join("out.chitubox");

        let project = Project::new_empty();
        let expected_bytes = project.to_bytes().expect("serialize fixture project");
        let result = project.save(&target);
        let bytes_present = fs::read(&target).ok();
        fs::remove_dir_all(&dir).ok();

        assert!(
            result.is_ok(),
            "save() must still succeed once its directory fsync can too: {result:?}"
        );
        assert_eq!(bytes_present.as_deref(), Some(expected_bytes.as_slice()));
    }
}
