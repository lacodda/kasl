//! Renamed commands keep their old names working, and say they are old.
//!
//! `init` became `setup` and `update` became `self-update` in 1.2. The old
//! spellings stay as aliases until 2.0, so anything already written against
//! them keeps running. These spawn the real binary: the notice depends on
//! which word was typed on the command line, which only a real invocation
//! reproduces.

#[cfg(test)]
mod tests {
    use std::path::Path;
    use std::process::{Command, Stdio};
    use tempfile::TempDir;

    /// Builds a kasl command bound to a private data directory, with no stdin.
    fn kasl_cmd(dir: &Path) -> Command {
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_kasl"));
        cmd.env("HOME", dir).env("LOCALAPPDATA", dir).stdin(Stdio::null());
        cmd
    }

    /// Runs a command and returns stdout and stderr joined.
    ///
    /// The notice goes to one of the two depending on the message macro, and
    /// the test cares that the user sees it, not which stream carried it.
    fn output_of(dir: &Path, args: &[&str]) -> String {
        let out = kasl_cmd(dir).args(args).output().unwrap();
        format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr))
    }

    #[test]
    fn help_lists_only_the_current_names() {
        let dir = TempDir::new().unwrap();
        let help = output_of(dir.path(), &["--help"]);

        assert!(help.contains("setup"), "setup missing from help:\n{help}");
        assert!(help.contains("self-update"), "self-update missing from help:\n{help}");

        // The aliases resolve but must not be advertised as separate commands.
        for line in help.lines() {
            let listed = line.trim_start();
            assert!(!listed.starts_with("init "), "deprecated `init` is listed in help:\n{help}");
            assert!(!listed.starts_with("update "), "deprecated `update` is listed in help:\n{help}");
        }
    }

    #[test]
    fn the_old_names_still_resolve() {
        let dir = TempDir::new().unwrap();

        // `--help` on the alias reaches the same command, which proves the
        // alias resolves without running the wizard or touching the network.
        let via_alias = output_of(dir.path(), &["init", "--help"]);
        assert!(via_alias.contains("--delete"), "`init` did not resolve to setup:\n{via_alias}");

        let via_alias = output_of(dir.path(), &["update", "--help"]);
        assert!(
            via_alias.to_lowercase().contains("update"),
            "`update` did not resolve to self-update:\n{via_alias}"
        );
    }

    #[test]
    fn the_old_names_announce_the_new_ones() {
        let dir = TempDir::new().unwrap();

        // --delete is the one setup path that needs no terminal.
        let deprecated = output_of(dir.path(), &["init", "--delete"]);
        assert!(
            deprecated.contains("`kasl init` is now `kasl setup`"),
            "no rename notice for `init`:\n{deprecated}"
        );

        let current = output_of(dir.path(), &["setup", "--delete"]);
        assert!(!current.contains("is now"), "`setup` must not warn about itself:\n{current}");
    }

    #[test]
    fn completions_offer_the_current_names_only() {
        let dir = TempDir::new().unwrap();
        let script = output_of(dir.path(), &["completions", "bash"]);

        // The generated script lists candidates in `opts=` lines; the aliases
        // must not be among them, or Tab would keep teaching the old spelling.
        let opts: Vec<&str> = script.lines().filter(|l| l.trim_start().starts_with("opts=")).collect();
        assert!(!opts.is_empty(), "no opts lines in the completion script:\n{script}");

        let top_level = opts.iter().find(|l| l.contains("autostart")).expect("no top-level opts line");
        assert!(top_level.contains("setup"), "setup missing from completions:\n{top_level}");
        assert!(top_level.contains("self-update"), "self-update missing from completions:\n{top_level}");

        for word in top_level.split_whitespace() {
            let word = word.trim_matches(['"', '='].as_ref());
            assert_ne!(word, "init", "deprecated `init` is offered by completions:\n{top_level}");
            assert_ne!(word, "update", "deprecated `update` is offered by completions:\n{top_level}");
        }
    }
}
