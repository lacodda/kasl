//! How kasl got onto this machine, when a package manager put it there.
//!
//! Scoop, Homebrew and winget each keep a record of what they installed and
//! at which version. Writing over their file behind their back - which is all
//! `self-update` knows how to do - leaves that record saying the old version
//! is still there, and the next `scoop update` or `brew upgrade` works from a
//! lie. So an installation one of them owns is updated by it, and kasl says
//! the command instead of running its own.
//!
//! Each of them also starts kasl through a name of its own: winget through a
//! symbolic link in `WinGet\Links`, Homebrew through a link in its `bin`. The
//! running executable's path (`std::env::current_exe`) is that link, not the
//! file - on Windows and macOS it is not resolved - so whatever lives beside
//! the real file, such as the plugins that ship with kasl, is not beside the
//! path kasl is told. [`own_dirs`] gives both directories.
//!
//! ```rust,no_run
//! use kasl::libs::install::Manager;
//!
//! if let Some(manager) = Manager::running() {
//!     println!("update with: {}", manager.update_command());
//! }
//! ```

use std::path::{Path, PathBuf};

/// The name the package managers know kasl by.
const PACKAGE: &str = "kasl";

/// A package manager that owns an installation of kasl.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Manager {
    Scoop,
    Homebrew,
    Winget,
}

impl Manager {
    /// The manager that installed the running kasl, if one did.
    pub fn running() -> Option<Self> {
        let (raw, resolved) = running_exe()?;
        Self::detect(&raw, &resolved)
    }

    /// Reads the manager from the path kasl was started by and the file it
    /// resolves to. Only a confident match counts: anything else is an
    /// installer's, npm's, cargo's or a hand-made install, which
    /// `self-update` serves as it always has.
    pub fn detect(raw: &Path, resolved: &Path) -> Option<Self> {
        let paths = [normalize(raw), normalize(resolved)];
        let any = |needle: &str| paths.iter().any(|path| path.contains(needle));
        // Scoop runs the binary from its app directory, behind a shim.
        if any(&format!("/scoop/apps/{PACKAGE}/")) {
            return Some(Self::Scoop);
        }
        // winget unpacks a portable package under `WinGet\Packages` and puts a
        // link to it in `WinGet\Links` - per user under %LOCALAPPDATA%\Microsoft,
        // machine-wide under Program Files.
        if any("/winget/packages/") || any("/winget/links/") {
            return Some(Self::Winget);
        }
        // Homebrew keeps every version under `Cellar/<name>/<version>`, on
        // macOS and Linux alike, and links it into its prefix.
        if any(&format!("/cellar/{PACKAGE}/")) {
            return Some(Self::Homebrew);
        }
        None
    }

    /// What the manager is called in a sentence.
    pub fn name(self) -> &'static str {
        match self {
            Self::Scoop => "Scoop",
            Self::Homebrew => "Homebrew",
            Self::Winget => "winget",
        }
    }

    /// The command that updates kasl through this manager.
    pub fn update_command(self) -> String {
        match self {
            Self::Scoop => format!("scoop update {PACKAGE}"),
            Self::Homebrew => format!("brew upgrade {PACKAGE}"),
            Self::Winget => "winget upgrade lacodda.kasl".to_string(),
        }
    }
}

/// The command that updates the running kasl: its package manager's, or
/// `kasl self-update` for an installation kasl looks after itself.
pub fn update_command() -> String {
    Manager::running().map_or_else(|| format!("{PACKAGE} self-update"), Manager::update_command)
}

/// The directories the running kasl lives in: the one it was started from,
/// then the one its file really is in, when a link stands between them.
///
/// The plugins that ship with kasl sit beside the real file. When winget or
/// Homebrew start kasl through a link, only the second directory has them.
pub fn own_dirs() -> Vec<PathBuf> {
    let Some((raw, resolved)) = running_exe() else {
        return Vec::new();
    };
    let mut dirs: Vec<PathBuf> = Vec::new();
    for exe in [raw, resolved] {
        if let Some(dir) = exe.parent().map(Path::to_path_buf)
            && !dirs.contains(&dir)
        {
            dirs.push(dir);
        }
    }
    dirs
}

/// The path autostart should start kasl by: one that outlives an upgrade.
///
/// Linux reports the running executable resolved, and under Homebrew that is
/// the versioned `Cellar/kasl/<version>/bin/kasl`, which the next `brew
/// upgrade` deletes. Homebrew's own `opt/kasl` link always points at the
/// installed version, so a path through the Cellar is turned into one
/// through it. Every other path is kept as it is.
pub fn stable_exe(exe: &Path) -> PathBuf {
    let text = exe.to_string_lossy();
    let marker = format!("/Cellar/{PACKAGE}/");
    let Some(at) = text.find(&marker) else {
        return exe.to_path_buf();
    };
    let prefix = &text[..at];
    let Some(file) = exe.file_name() else {
        return exe.to_path_buf();
    };
    PathBuf::from(prefix).join("opt").join(PACKAGE).join("bin").join(file)
}

/// The path kasl was started by, and the file it resolves to.
fn running_exe() -> Option<(PathBuf, PathBuf)> {
    let raw = std::env::current_exe().ok()?;
    let resolved = std::fs::canonicalize(&raw).map(strip_verbatim).unwrap_or_else(|_| raw.clone());
    Some((raw, resolved))
}

/// Drops the `\\?\` prefix Windows puts on a canonical path, so the path
/// reads - and joins, and prints - like the one it came from.
fn strip_verbatim(path: PathBuf) -> PathBuf {
    let text = path.to_string_lossy();
    match text.strip_prefix(r"\\?\") {
        Some(rest) if !rest.starts_with("UNC\\") => PathBuf::from(rest),
        _ => path,
    }
}

/// A path as one lower-case string with forward slashes, for matching.
fn normalize(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/").to_ascii_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn detect(raw: &str, resolved: &str) -> Option<Manager> {
        Manager::detect(Path::new(raw), Path::new(resolved))
    }

    #[test]
    fn scoop_is_recognised_by_its_app_directory() {
        let exe = r"C:\Users\dev\scoop\apps\kasl\current\kasl.exe";
        assert_eq!(detect(exe, exe), Some(Manager::Scoop));
        // A global install, and the version directory the junction leads to.
        let global = r"C:\ProgramData\scoop\apps\kasl\1.24.0\kasl.exe";
        assert_eq!(detect(global, global), Some(Manager::Scoop));
    }

    /// winget starts kasl through the link in `Links`; the file is under
    /// `Packages`. Either is enough, since one of the two may not resolve.
    #[test]
    fn winget_is_recognised_by_its_link_or_its_package() {
        let link = r"C:\Users\dev\AppData\Local\Microsoft\WinGet\Links\kasl.exe";
        let file = r"C:\Users\dev\AppData\Local\Microsoft\WinGet\Packages\lacodda.kasl_Microsoft.Winget.Source_8wekyb3d8bbwe\kasl-v1.24.0-x86_64-pc-windows-msvc\kasl.exe";
        assert_eq!(detect(link, file), Some(Manager::Winget));
        assert_eq!(detect(link, link), Some(Manager::Winget));
        assert_eq!(detect(file, file), Some(Manager::Winget));
        let machine = r"C:\Program Files\WinGet\Packages\lacodda.kasl_Microsoft.Winget.Source_8wekyb3d8bbwe\kasl-v1.24.0-x86_64-pc-windows-msvc\kasl.exe";
        assert_eq!(detect(machine, machine), Some(Manager::Winget));
    }

    /// On macOS kasl is started through the link in Homebrew's `bin`, which
    /// does not name Homebrew; the file it resolves to is in the Cellar.
    #[test]
    fn homebrew_is_recognised_by_its_cellar() {
        assert_eq!(
            detect("/opt/homebrew/bin/kasl", "/opt/homebrew/Cellar/kasl/1.24.0/bin/kasl"),
            Some(Manager::Homebrew)
        );
        let linux = "/home/linuxbrew/.linuxbrew/Cellar/kasl/1.24.0/bin/kasl";
        assert_eq!(detect(linux, linux), Some(Manager::Homebrew));
        assert_eq!(detect("/usr/local/bin/kasl", "/usr/local/Cellar/kasl/1.24.0/bin/kasl"), Some(Manager::Homebrew));
    }

    /// The installers', npm's, cargo's and a hand-made install stay kasl's
    /// own to update - including one in a directory that merely mentions a
    /// manager, or another package of it.
    #[test]
    fn every_other_installation_is_left_to_self_update() {
        for exe in [
            r"C:\Users\dev\AppData\Local\Programs\kasl\kasl.exe",
            r"C:\Users\dev\AppData\Roaming\npm\node_modules\kasl-cli\kasl.exe",
            "/home/dev/.cargo/bin/kasl",
            "/home/dev/.local/bin/kasl",
            r"C:\Users\dev\scoop\apps\kasl-server\current\kasl.exe",
            "/opt/homebrew/Cellar/kasl-server/0.9.0/bin/kasl",
            "/usr/local/bin/kasl",
        ] {
            assert_eq!(detect(exe, exe), None, "{exe}");
        }
    }

    #[test]
    fn each_manager_names_its_own_update() {
        assert_eq!(Manager::Scoop.update_command(), "scoop update kasl");
        assert_eq!(Manager::Homebrew.update_command(), "brew upgrade kasl");
        assert_eq!(Manager::Winget.update_command(), "winget upgrade lacodda.kasl");
    }

    #[test]
    fn a_cellar_path_becomes_the_one_through_opt() {
        assert_eq!(
            stable_exe(Path::new("/home/linuxbrew/.linuxbrew/Cellar/kasl/1.24.0/bin/kasl")),
            PathBuf::from("/home/linuxbrew/.linuxbrew/opt/kasl/bin/kasl")
        );
        assert_eq!(
            stable_exe(Path::new("/opt/homebrew/Cellar/kasl/1.24.0/bin/kasl")),
            PathBuf::from("/opt/homebrew/opt/kasl/bin/kasl")
        );
    }

    #[test]
    fn any_other_path_is_kept_for_autostart() {
        for exe in [
            "/home/dev/.local/bin/kasl",
            "/opt/homebrew/bin/kasl",
            "/opt/homebrew/Cellar/kasl-server/0.9.0/bin/kasl",
        ] {
            assert_eq!(stable_exe(Path::new(exe)), PathBuf::from(exe), "{exe}");
        }
    }

    #[cfg(windows)]
    #[test]
    fn a_verbatim_prefix_is_dropped_but_a_share_is_kept() {
        assert_eq!(strip_verbatim(PathBuf::from(r"\\?\C:\kasl\kasl.exe")), PathBuf::from(r"C:\kasl\kasl.exe"));
        assert_eq!(
            strip_verbatim(PathBuf::from(r"\\?\UNC\server\share\kasl.exe")),
            PathBuf::from(r"\\?\UNC\server\share\kasl.exe")
        );
    }

    /// The running test binary is no package manager's, and its directory is
    /// where it lives.
    #[test]
    fn the_running_binary_has_its_own_directory() {
        let dirs = own_dirs();
        assert!(!dirs.is_empty());
        let exe = std::env::current_exe().unwrap();
        assert_eq!(dirs[0], exe.parent().unwrap());
    }
}
