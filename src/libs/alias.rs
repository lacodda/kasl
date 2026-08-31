//! The short `ka` alias, as a link rather than a second binary.
//!
//! Until v1.8.1 `ka` was its own `[[bin]]`: a second, byte-identical 15 MB
//! executable built, packed into every archive and downloaded by every user, to
//! give one program a second name. A link costs nothing and cannot go stale,
//! because there is only ever one set of bytes:
//!
//! - **Unix:** a symlink, which `install.sh` already created.
//! - **Windows:** a *hard* link. Symlinks there need elevation or developer
//!   mode, which an installer has no business demanding; hard links do not, as
//!   long as both names are on the same volume - and they are, since the alias
//!   lands beside the binary.
//!
//! The one seam is [`self_update`](crate::libs::update): replacing the binary
//! renames the old file aside and moves a new one in, which breaks the link, so
//! an update re-links afterwards - that is what [`refresh`] is for.
//!
//! Which name needs repairing depends on which one was typed. An update
//! replaces the file it is *running from*, so `ka self-update` replaces `ka`
//! and leaves `kasl` behind, mirroring the usual case exactly. Both are handled
//! by asking for the [`counterpart`] rather than for "the alias" - asking the
//! wrong question there cost turnout a vanishing binary (its ADR 0015), and the
//! same mistake is available here.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

/// The alias name, without any platform extension.
pub const ALIAS: &str = "ka";

/// The primary name, without any platform extension.
pub const PRIMARY: &str = "kasl";

/// The *other* name of this install: the alias when running as `kasl`, and
/// `kasl` when running as the alias.
///
/// An update replaces the file it is running from, whatever it is called, so
/// "which name did the swap *not* touch" is the question that is right either
/// way. A binary the user renamed has no counterpart, and nothing is invented
/// beside it.
pub fn counterpart(exe: &Path) -> Option<PathBuf> {
    let stem = exe.file_stem()?.to_str()?;
    let other = match stem {
        ALIAS => PRIMARY,
        PRIMARY => ALIAS,
        _ => return None,
    };
    Some(sibling_named(exe, other))
}

/// A path beside `exe` carrying `name` and the same extension.
fn sibling_named(exe: &Path, name: &str) -> PathBuf {
    let mut sibling = exe.with_file_name(name);
    if let Some(extension) = exe.extension() {
        sibling.set_extension(extension);
    }
    sibling
}

/// Point `alias` at `exe`, replacing whatever is already there.
///
/// Both paths must be on the same volume on Windows, which they are whenever
/// the alias is created beside the binary.
pub fn link(exe: &Path, alias: &Path) -> Result<()> {
    // Linking a name to itself would destroy it: the removal below takes the
    // only copy and there is then nothing left to link from. No caller should
    // ask - `counterpart` exists so none does - but the consequence is a
    // binary that disappears, which is too expensive to leave to callers.
    if alias == exe {
        anyhow::bail!("refusing to link {} to itself", alias.display());
    }

    // A link cannot be created over an existing name; the file being replaced
    // is not the running image, so removing it is allowed.
    let _ = std::fs::remove_file(alias);

    #[cfg(windows)]
    let result = std::fs::hard_link(exe, alias);
    // Relative, matching what `install.sh` writes: a symlink holding just the
    // file name survives the install directory being moved or renamed, where
    // one holding an absolute path would dangle.
    #[cfg(not(windows))]
    let result = {
        let target = exe.file_name().unwrap_or(exe.as_os_str());
        std::os::unix::fs::symlink(target, alias)
    };

    result.with_context(|| format!("cannot link {} to {}", alias.display(), exe.display()))
}

/// Re-point this install's *other* name at the binary an update just replaced.
///
/// Best-effort by design: the second name is a convenience, and an install that
/// never had one must not grow one behind the user's back - `KASL_NO_ALIAS` is
/// their choice to keep. So this only acts when the counterpart is already
/// there, and reports what it did for the caller to print.
pub fn refresh(exe: &Path) -> Outcome {
    let Some(other) = counterpart(exe) else {
        return Outcome::Absent;
    };
    if !other.exists() {
        return Outcome::Absent;
    }
    match link(exe, &other) {
        Ok(()) => Outcome::Relinked(other),
        Err(err) => Outcome::Failed(other, err.to_string()),
    }
}

/// What [`refresh`] found and did.
#[derive(Debug, PartialEq)]
pub enum Outcome {
    /// This install has only the one name - nothing to refresh.
    Absent,
    /// The second name was re-pointed at the new binary.
    Relinked(PathBuf),
    /// The second name is installed but could not be re-pointed; it is stale
    /// and the user has to be told, since it still answers under its own name.
    Failed(PathBuf, String),
}

impl Outcome {
    /// The line to print after an update, if any.
    ///
    /// Phrased around the path rather than the word "alias": an update run as
    /// `ka` repairs `kasl`, and calling that the alias would name the wrong
    /// file for whoever is reading.
    pub fn message(&self) -> Option<String> {
        match self {
            Self::Absent => None,
            Self::Relinked(other) => Some(format!("{} updated too.", other.display())),
            Self::Failed(other, err) => Some(format!(
                "Warning: {} still points at the previous version and could not be relinked ({err}).\n\
                 Re-run the installer to fix it.",
                other.display()
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The question an update has to ask is "which name did I *not* replace",
    /// and the answer depends on how it was launched.
    #[test]
    fn the_counterpart_is_whichever_name_is_not_running() {
        let dir = tempfile::tempdir().unwrap();
        let primary = dir.path().join(if cfg!(windows) { "kasl.exe" } else { "kasl" });
        let alias = sibling_named(&primary, ALIAS);

        assert_eq!(counterpart(&primary).unwrap(), alias);
        assert_eq!(counterpart(&alias).unwrap(), primary);
    }

    /// A binary the user renamed is not one of ours to pair up.
    #[test]
    fn a_renamed_binary_has_no_counterpart() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(counterpart(&dir.path().join("my-kasl")), None);
    }

    /// The whole point of the link: one set of bytes answers to both names, so
    /// replacing the content through one name shows through the other.
    #[test]
    fn a_linked_alias_shares_the_binary_it_points_at() {
        let dir = tempfile::tempdir().unwrap();
        let exe = dir.path().join("kasl");
        std::fs::write(&exe, b"version one").unwrap();
        let alias = sibling_named(&exe, ALIAS);

        link(&exe, &alias).unwrap();

        // Identity, observed rather than asked about: writing through one name
        // shows through the other only if there is one file behind both.
        std::fs::write(&exe, b"version two").unwrap();
        assert_eq!(
            std::fs::read(&alias).unwrap(),
            b"version two",
            "the alias must be the same file as the binary, not a copy of it"
        );
    }

    /// turnout's field report, guarded here before it can happen: an update
    /// launched as the alias asked to link that name to itself, and `link`
    /// removes the destination before creating it - so the file was deleted
    /// with nothing left to link from.
    #[test]
    fn linking_a_name_to_itself_is_refused_rather_than_destroying_it() {
        let dir = tempfile::tempdir().unwrap();
        let exe = dir.path().join("ka");
        std::fs::write(&exe, b"the only copy").unwrap();

        assert!(link(&exe, &exe).is_err());
        assert!(exe.exists(), "the binary must survive a self-link attempt");
        assert_eq!(std::fs::read(&exe).unwrap(), b"the only copy");
    }

    /// An update run under the alias repairs the primary name, which is the
    /// one the swap left on the outgoing release.
    #[test]
    fn refreshing_from_the_alias_repairs_the_primary_name() {
        let dir = tempfile::tempdir().unwrap();
        let primary = dir.path().join("kasl");
        let alias = dir.path().join("ka");
        // After the swap: the alias is the new binary, the primary is stale.
        std::fs::write(&alias, b"new version").unwrap();
        std::fs::write(&primary, b"old version").unwrap();

        assert!(matches!(refresh(&alias), Outcome::Relinked(_)));

        assert!(alias.exists(), "the running name must not be removed");
        assert_eq!(std::fs::read(&primary).unwrap(), b"new version");
    }

    /// The usual direction: an update run as `kasl` repairs `ka`.
    #[test]
    fn refreshing_from_the_primary_repairs_the_alias() {
        let dir = tempfile::tempdir().unwrap();
        let primary = dir.path().join("kasl");
        let alias = dir.path().join("ka");
        std::fs::write(&primary, b"new version").unwrap();
        std::fs::write(&alias, b"old version").unwrap();

        assert!(matches!(refresh(&primary), Outcome::Relinked(_)));

        assert_eq!(std::fs::read(&alias).unwrap(), b"new version");
    }

    /// An install without an alias must not grow one: `KASL_NO_ALIAS` is a
    /// choice the user made, and an update is no place to overrule it.
    #[test]
    fn refresh_leaves_an_aliasless_install_alone() {
        let dir = tempfile::tempdir().unwrap();
        let exe = dir.path().join("kasl");
        std::fs::write(&exe, b"binary").unwrap();

        assert_eq!(refresh(&exe), Outcome::Absent);
        assert!(!sibling_named(&exe, ALIAS).exists());
    }

    /// A stale alias must never fail quietly: it keeps answering to its own
    /// name, so the user has to hear about it.
    #[test]
    fn a_failed_relink_names_the_file_and_a_way_out() {
        let outcome = Outcome::Failed(PathBuf::from("/home/dev/.local/bin/ka"), "permission denied".to_string());
        let message = outcome.message().expect("a failure has to be reported");
        assert!(message.contains("/home/dev/.local/bin/ka"));
        assert!(message.contains("previous version"));
        assert!(message.contains("installer"));
    }
}
