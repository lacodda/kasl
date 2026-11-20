//! Guards the facts that must agree before a version is published, and that
//! are kasl's own.
//!
//! What every program of the line has to agree on - one version in the crate,
//! the npm wrapper and its binary pin, one description, one README whose links
//! work off GitHub and whose commands exist, installers that name the crate -
//! is held by `lyrn check`, in the Shopfronts workflow on every push. What is
//! left here is what only kasl has: the `ka` alias, the plugins in the box, the
//! zip winget and Scoop install, and the installers' care for the machine they
//! run on.
//!
//! These checks run in CI, so a mismatch fails the build instead of shipping.

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::{Path, PathBuf};

    fn repo_root() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
    }

    fn read(path: impl AsRef<Path>) -> String {
        let path = repo_root().join(path);
        fs::read_to_string(&path).unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()))
    }

    #[test]
    fn every_declared_binary_is_packaged_into_the_release() {
        // Field report, 14.08: `ka` was declared in Cargo.toml, built by CI
        // and promised by the README, but the packaging step copied only
        // `kasl` - so `where ka` came up empty on every machine installed
        // from a release archive.
        let manifest = read("Cargo.toml");
        let binaries: Vec<String> = manifest
            .lines()
            .map(str::trim)
            .scan(false, |in_bin, line| {
                if line == "[[bin]]" {
                    *in_bin = true;
                    return Some(None);
                }
                if line.starts_with('[') {
                    *in_bin = false;
                    return Some(None);
                }
                if *in_bin && let Some(value) = line.strip_prefix("name") {
                    return Some(Some(value.trim_start_matches([' ', '=']).trim().trim_matches('"').to_string()));
                }
                Some(None)
            })
            .flatten()
            .collect();

        // One binary, deliberately: the `ka` alias is a link the installers
        // create, not a second executable. Shipping it as its own `[[bin]]`
        // put two identical 15 MB files in every archive and download.
        assert_eq!(binaries, vec!["kasl".to_string()], "expected exactly the `kasl` binary in Cargo.toml");

        let workflow = read(".github/workflows/release.yml");
        for binary in &binaries {
            assert!(
                workflow.contains(&format!("release/{binary}.exe")),
                "release.yml does not package `{binary}.exe`; the Windows archive would ship without it"
            );
            assert!(
                workflow.contains(&format!("release/{binary}\"")),
                "release.yml does not package `{binary}`; the Unix archives would ship without it"
            );
        }
    }

    #[test]
    fn the_npm_package_exposes_every_declared_binary() {
        // npm installs its own shims from `bin`, so a binary missing there is
        // missing for everyone who installed through npm, however well the
        // release archive is packed.
        let npm = read("npm/package.json");
        for name in ["kasl", "ka"] {
            assert!(
                npm.contains(&format!("\"{name}\": \"run.js\"")),
                "npm/package.json does not expose `{name}`; the README promises both names"
            );
        }
    }

    #[test]
    fn the_npm_readme_is_produced_by_the_package_itself() {
        // Copying the README in the publish workflow meant a hand-run
        // `npm publish` shipped a page with no README at all. `prepack` runs
        // for every pack, CI or manual.
        let npm = read("npm/package.json");
        assert!(npm.contains("\"prepack\""), "npm/package.json has no prepack script to bring the README in");
        assert!(npm.contains("README.md"), "README.md is not listed in the npm package files");
        assert!(
            repo_root().join("npm/prepack.js").exists(),
            "npm/prepack.js is missing; the packed tarball would have no README"
        );

        let workflow = read(".github/workflows/publish.yml");
        assert!(
            !workflow.contains("cp README.md npm/README.md"),
            "the publish workflow still copies the README; that step belongs to the package, \
             otherwise a manual publish skips it"
        );
    }

    #[test]
    fn the_unix_installer_redirects_windows_shells() {
        // Field report, 19.08: run in Git Bash on Windows, the script matched
        // no case arm and answered "No prebuilt binary for MINGW64_NT-…",
        // which reads as "unsupported platform" although a Windows release
        // exists - it is just installed by the other script.
        let installer = read("tools/install.sh");

        for shell in ["MINGW*", "MSYS*", "CYGWIN*"] {
            assert!(
                installer.contains(shell),
                "install.sh does not recognise {shell}; Windows shells fall through to the \
                 generic 'no prebuilt binary' message"
            );
        }
        assert!(
            installer.contains("install.ps1"),
            "install.sh does not name the PowerShell installer, leaving Windows users at a dead end"
        );
    }

    /// The Windows installer edits the user PATH in the registry and must keep
    /// its type. `[Environment]::SetEnvironmentVariable` rewrites REG_EXPAND_SZ
    /// as REG_SZ, which turns every `%VAR%` entry into literal text - found on
    /// a sibling's installer (rigger v0.1.0), and this one carried the same code.
    #[test]
    fn the_windows_installer_keeps_the_path_expandable() {
        let installer = read("tools/install.ps1");
        assert!(
            !installer.contains("SetEnvironmentVariable"),
            "install.ps1 uses [Environment]::SetEnvironmentVariable, which downgrades PATH to a plain string"
        );
        assert!(
            installer.contains("-Type ExpandString") && installer.contains("DoNotExpandEnvironmentNames"),
            "install.ps1 must read the raw PATH and write it back as an expandable string"
        );
    }

    /// No Windows test may reconfigure the machine it runs on.
    ///
    /// The Unix autostart implementation is sandboxed by redirecting HOME and
    /// XDG_CONFIG_HOME, so calling it in a test is safe - and the Unix tests
    /// do. The Windows one writes to the Task Scheduler and to the registry,
    /// and neither honours an environment variable: there is nothing to
    /// redirect, so a call lands on the developer's own machine.
    ///
    /// It did. A Windows test asserting only "this does not panic" - which a
    /// function returning `Result` was never going to do - registered the test
    /// binary for startup and then deleted the entry, taking the user's real
    /// kasl autostart with it.
    #[test]
    fn no_windows_test_reconfigures_autostart() {
        let source = read("tests/autostart.rs");
        // Everything from a `#[cfg(windows)]` up to the next `#[cfg(` is the
        // Windows-only region; the calls are only unsafe there.
        let mut region_is_windows = false;
        for line in source.lines() {
            let trimmed = line.trim();
            if trimmed.starts_with("//") {
                continue;
            }
            if let Some(target) = trimmed.strip_prefix("#[cfg(") {
                region_is_windows = target.starts_with("windows");
                continue;
            }
            if region_is_windows && (trimmed.contains("autostart::enable(") || trimmed.contains("autostart::disable(")) {
                panic!("a Windows-only test calls `{trimmed}`, which reconfigures the machine running it - there is no sandbox for that path");
            }
        }
    }

    /// A doctest that touches the machine must never actually run.
    ///
    /// `cargo test` executes every example that is not marked `no_run` or
    /// `ignore` - including, once, the one on `autostart::enable`. It really
    /// registered the doctest's own temporary binary for startup, overwriting
    /// the user's kasl entry with a path that is deleted minutes later. Found
    /// in the field: an owner's Run key held
    /// `...\rustdoctestkS63gI\rust_out.exe watch`.
    ///
    /// So the examples on this handful of functions are checked for the
    /// marker. The list is deliberately explicit rather than a heuristic over
    /// every example: these are the calls that write to the registry, the
    /// scheduler or the filesystem outside the project, and naming them is
    /// what makes the check say something true.
    #[test]
    fn examples_that_touch_the_machine_are_marked_no_run() {
        // (file, the call that makes an example unsafe to execute)
        const SIDE_EFFECTING: &[(&str, &str)] = &[
            ("src/libs/autostart.rs", "autostart::enable()"),
            ("src/libs/autostart.rs", "autostart::disable()"),
        ];

        for (file, call) in SIDE_EFFECTING {
            let source = read(file);
            let mut fence: Option<String> = None;
            for line in source.lines() {
                let trimmed = line.trim().trim_start_matches("///").trim_start_matches("//!").trim();
                if let Some(attributes) = trimmed.strip_prefix("```") {
                    fence = if fence.is_some() { None } else { Some(attributes.to_string()) };
                    continue;
                }
                let Some(attributes) = &fence else { continue };
                if !trimmed.contains(call) {
                    continue;
                }
                assert!(
                    attributes.contains("no_run") || attributes.contains("ignore"),
                    "{file}: an example calling {call} runs during `cargo test` and would change this machine's startup configuration - mark the block ```rust,no_run"
                );
            }
        }
    }

    /// The plugins that ship with kasl ride in the release archive beside
    /// the binary, and every way of installing kasl puts them beside it -
    /// where kasl looks. A path that skipped them would install a kasl whose
    /// GitLab inbox silently is not there.
    #[test]
    fn the_bundled_plugins_reach_every_installation() {
        let workflow = read(".github/workflows/release.yml");
        assert!(
            workflow.contains("GITLAB_PLUGIN: v") && workflow.contains("-R lacodda/kasl-plugin-gitlab"),
            "release.yml no longer fetches a pinned kasl-plugin-gitlab release"
        );
        assert_eq!(
            workflow.matches("bundled/bin/* \"$name\"").count(),
            2,
            "both package steps, Windows and Unix, must pack the bundled plugins"
        );
        assert!(
            workflow.contains("plugin list --json") && workflow.contains("select(.name == \"gitlab\" and .problem == null)"),
            "release.yml no longer checks the packed kasl can use its bundled plugin"
        );
        for installer in ["tools/install.ps1", "tools/install.sh", "npm/download.js"] {
            assert!(
                read(installer).contains("kasl-plugin-"),
                "{installer} does not install the plugins the archive carries"
            );
        }
        assert!(
            read("src/libs/update.rs").contains("is_bundled_plugin(file_name)"),
            "self-update no longer replaces the bundled plugins beside the binary"
        );
        // The list kasl knows its box by is the list the archive is packed with.
        for name in kasl::libs::update::BUNDLED_PLUGINS {
            assert!(
                workflow.contains(&format!("-R lacodda/kasl-plugin-{name} ")),
                "BUNDLED_PLUGINS names `{name}`, which release.yml does not fetch into the archive"
            );
        }
        assert_eq!(
            workflow.matches("gh release download").count(),
            kasl::libs::update::BUNDLED_PLUGINS.len(),
            "release.yml fetches a plugin BUNDLED_PLUGINS does not name; self-update could not tell it is missing"
        );
    }

    /// `KASL_NO_PATH=1` is an install into a directory of its own - the one
    /// `furca release verify --install` proves a release in. It must not stop
    /// the watcher of the kasl on `PATH`, point autostart at a directory about
    /// to be deleted, or edit the `PATH`: the variable is read before any of
    /// that.
    #[test]
    fn the_installers_leave_the_machine_alone_when_asked() {
        let ps1 = read("tools/install.ps1");
        let scratch = ps1.find("$env:KASL_NO_PATH").expect("install.ps1 does not read KASL_NO_PATH");
        let leaves = ps1.find("if ($scratch) {").expect("install.ps1 never stops short for a scratch install");
        for (what, needle) in [
            ("stops the watchers", "Stop-Process"),
            ("edits the PATH", "Set-ItemProperty -Path \"HKCU:\\Environment\""),
            ("rewrites autostart", "Set-ItemProperty -Path $runKey"),
            ("starts a watcher", "& $target watch"),
        ] {
            let at = ps1
                .find(needle)
                .unwrap_or_else(|| panic!("install.ps1 no longer {what} at all - is this check stale?"));
            assert!(scratch < at, "install.ps1 {what} before it reads KASL_NO_PATH");
        }
        assert!(
            ps1.contains("if (-not $scratch) {"),
            "install.ps1 stops the watchers of a scratch install's machine"
        );
        for needle in [
            "Set-ItemProperty -Path \"HKCU:\\Environment\"",
            "Set-ItemProperty -Path $runKey",
            "& $target watch",
        ] {
            assert!(
                ps1.find(needle).unwrap() > leaves,
                "install.ps1 reaches `{needle}` before it stops short for a scratch install"
            );
        }

        let sh = read("tools/install.sh");
        let scratch = sh.find("KASL_NO_PATH").expect("install.sh does not read KASL_NO_PATH");
        for needle in ["systemctl --user restart", "\"$BIN_DIR/kasl\" watch", "Note: add $BIN_DIR to your PATH."] {
            let at = sh.find(needle).unwrap_or_else(|| panic!("install.sh has no `{needle}` - is this check stale?"));
            assert!(scratch < at, "install.sh reaches `{needle}` before it reads KASL_NO_PATH");
        }
    }

    /// Windows refuses to overwrite a running executable, and a watcher is not
    /// the only thing that runs kasl: `kasl mcp`, a prompt's `kasl status`, a
    /// toast's button. The installer moves the file in the way aside first -
    /// a rename is allowed on a running image - for the binary, the alias and
    /// the plugins alike, and sweeps what earlier runs moved aside.
    #[test]
    fn the_windows_installer_moves_a_running_program_aside() {
        let ps1 = read("tools/install.ps1");
        assert!(
            ps1.contains(".old-$([guid]::NewGuid())"),
            "install.ps1 no longer moves a file aside under a name of its own"
        );
        for (what, aside, copy) in [
            ("kasl.exe", "Move-Aside $target", "Copy-Item $binary.FullName $target"),
            (
                "a plugin",
                "Move-Aside (Join-Path $dir $_.Name)",
                "Copy-Item $_.FullName (Join-Path $dir $_.Name)",
            ),
            ("ka.exe", "Move-Aside $alias", "New-Item -ItemType HardLink -Path $alias"),
        ] {
            let aside = ps1
                .find(aside)
                .unwrap_or_else(|| panic!("install.ps1 copies {what} without moving the old one aside"));
            let copy = ps1
                .find(copy)
                .unwrap_or_else(|| panic!("install.ps1 no longer installs {what} - is this check stale?"));
            assert!(aside < copy, "install.ps1 moves {what} aside only after copying over it");
        }
        assert!(ps1.contains("\"*.exe.old-*\""), "install.ps1 never sweeps what it moved aside");
        assert!(
            read("src/libs/update.rs").contains("is_moved_aside(&name)"),
            "kasl no longer sweeps what the installer moved aside, which then waits for the next install"
        );
    }
}
