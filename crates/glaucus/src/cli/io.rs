// SPDX-FileCopyrightText: Glaucus contributors
//
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Input reading and crash-safe output.

use anyhow::{Context, Result};
use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};

/// Where a document comes from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Source {
    /// Standard input.
    Stdin,
    /// A path on disk.
    File(PathBuf),
}

impl Source {
    /// The name to show in diagnostics.
    #[must_use]
    pub fn label(&self) -> String {
        match self {
            Self::Stdin => "<stdin>".to_string(),
            Self::File(path) => path.display().to_string(),
        }
    }

    /// The path, when this source is a file.
    #[must_use]
    pub fn path(&self) -> Option<PathBuf> {
        match self {
            Self::Stdin => None,
            Self::File(path) => Some(path.clone()),
        }
    }
}

/// Reads a source to a `String`.
///
/// # Errors
///
/// Returns an error when the file cannot be read or is not valid UTF-8.
pub fn read_input(source: &Source, stdin: &mut dyn BufRead) -> Result<String> {
    match source {
        Source::Stdin => {
            let mut buffer = Vec::new();
            stdin.read_to_end(&mut buffer).context("reading stdin")?;
            String::from_utf8(buffer).context("stdin is not valid UTF-8")
        }
        Source::File(path) => {
            let bytes =
                std::fs::read(path).with_context(|| format!("reading {}", path.display()))?;
            String::from_utf8(bytes)
                .with_context(|| format!("{} is not valid UTF-8", path.display()))
        }
    }
}

/// Writes `contents` to `path` atomically: a temporary file in the same
/// directory, then a rename.
///
/// A crash or a full disk mid-write can never leave a truncated YAML file where
/// a valid one used to be.
///
/// # Errors
///
/// Returns an error when the temporary file cannot be created, written, or
/// renamed over the target.
pub fn write_atomic(path: &Path, contents: &str) -> Result<()> {
    let directory = path.parent().unwrap_or_else(|| Path::new("."));
    let file_name = path.file_name().map_or_else(
        || "out".to_string(),
        |name| name.to_string_lossy().into_owned(),
    );
    let temporary = directory.join(format!(".{file_name}.glaucus-tmp"));

    {
        let mut file = std::fs::File::create(&temporary)
            .with_context(|| format!("creating {}", temporary.display()))?;
        file.write_all(contents.as_bytes())
            .with_context(|| format!("writing {}", temporary.display()))?;
        file.sync_all()
            .with_context(|| format!("syncing {}", temporary.display()))?;
    }

    inherit_permissions(path, &temporary)?;
    std::fs::rename(&temporary, path).with_context(|| format!("replacing {}", path.display()))?;
    Ok(())
}

/// Copies `source`'s permission bits onto `target`.
///
/// Without this, the rename above would replace a `0600` file holding secrets
/// with a fresh `0644` one — silently widening access to every YAML file this
/// tool rewrites. A missing `source` means the target is new, so the process
/// umask is the right answer and there is nothing to copy.
#[cfg(unix)]
fn inherit_permissions(source: &Path, target: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let Ok(metadata) = std::fs::metadata(source) else {
        return Ok(());
    };
    let mode = metadata.permissions().mode();
    std::fs::set_permissions(target, std::fs::Permissions::from_mode(mode))
        .with_context(|| format!("setting mode on {}", target.display()))
}

/// Non-Unix platforms have no mode bits to copy.
#[cfg(not(unix))]
fn inherit_permissions(_source: &Path, _target: &Path) -> Result<()> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn label_names_stdin_and_a_file() {
        assert_eq!(Source::Stdin.label(), "<stdin>");
        let label = Source::File("a/b.yaml".into()).label();
        assert!(label.contains("b.yaml"), "unexpected label: {label}");
    }

    #[test]
    fn path_is_some_only_for_files() {
        assert_eq!(Source::Stdin.path(), None);
        assert_eq!(Source::File("x.yaml".into()).path(), Some("x.yaml".into()));
    }

    #[test]
    fn write_atomic_to_a_path_with_no_file_name_is_an_error() {
        // A path ending in `..` has no file-name component (`file_name()`
        // returns `None`), so this exercises `write_atomic`'s `"out"`
        // fallback. The final rename still fails: the resolved target is the
        // directory itself, not a plain file. Rooted under `temp_dir()`
        // (rather than a bare `Path::new("..")`/`Path::new("/")`) so the
        // stray temp file `write_atomic` leaves behind on this error path
        // stays inside the directory this test removes, not the repository.
        let dir = std::env::temp_dir().join("glaucus-io-no-file-name");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("..");

        assert!(write_atomic(&path, "x").is_err());

        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn reads_stdin_when_source_is_stdin() {
        let mut input = Cursor::new(b"a: 1\n".to_vec());
        let got = read_input(&Source::Stdin, &mut input).unwrap();
        assert_eq!(got, "a: 1\n");
    }

    #[test]
    fn reads_a_file_from_disk() {
        let dir = std::env::temp_dir().join("glaucus-io-read");
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("x.yaml");
        std::fs::write(&p, "b: 2\n").unwrap();
        let mut empty = Cursor::new(Vec::new());
        let got = read_input(&Source::File(p.clone()), &mut empty).unwrap();
        assert_eq!(got, "b: 2\n");
        std::fs::remove_file(p).unwrap();
    }

    #[test]
    fn missing_file_is_an_error() {
        let mut empty = Cursor::new(Vec::new());
        let err = read_input(&Source::File("nope.yaml".into()), &mut empty).unwrap_err();
        assert!(err.to_string().contains("nope.yaml"), "no context: {err}");
    }

    #[test]
    fn non_utf8_input_is_an_error() {
        let dir = std::env::temp_dir().join("glaucus-io-utf8");
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("bad.yaml");
        std::fs::write(&p, [0xff, 0xfe]).unwrap();
        let mut empty = Cursor::new(Vec::new());
        let err = read_input(&Source::File(p.clone()), &mut empty).unwrap_err();
        assert!(err.to_string().contains("UTF-8"), "wrong error: {err}");
        std::fs::remove_file(p).unwrap();
    }

    #[test]
    fn atomic_write_replaces_content() {
        let dir = std::env::temp_dir().join("glaucus-io-write");
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("y.yaml");
        std::fs::write(&p, "old\n").unwrap();
        write_atomic(&p, "new\n").unwrap();
        assert_eq!(std::fs::read_to_string(&p).unwrap(), "new\n");
        std::fs::remove_file(p).unwrap();
    }

    #[test]
    fn atomic_write_to_unwritable_dir_is_an_error() {
        let path = Path::new("/nonexistent-dir-glaucus/z.yaml");
        assert!(write_atomic(path, "x").is_err());
    }

    #[cfg(unix)]
    #[test]
    fn atomic_write_preserves_restrictive_permissions() {
        use std::os::unix::fs::PermissionsExt;
        let dir = std::env::temp_dir().join("glaucus-io-perm");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("secret.yaml");
        std::fs::write(&path, "password: old\n").unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();

        write_atomic(&path, "password: new\n").unwrap();

        let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600, "atomic write widened permissions to {mode:o}");
        std::fs::remove_file(&path).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn atomic_write_to_a_new_path_uses_the_umask() {
        // No source file to inherit from: the helper must not error.
        let dir = std::env::temp_dir().join("glaucus-io-new");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("fresh.yaml");
        let _ = std::fs::remove_file(&path);
        write_atomic(&path, "a: 1\n").unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "a: 1\n");
        std::fs::remove_file(&path).unwrap();
    }
}
