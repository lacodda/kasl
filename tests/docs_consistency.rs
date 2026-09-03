//! Guards the facts the documentation site repeats from the code.
//!
//! The docs drift the same way the registry metadata does, only quieter: the
//! site keeps building, every page renders, and the wrong sentence sits there
//! until a user follows it. Two rounds of release doc-review missed
//! `getting-started.md` asking for "Rust 1.70 or higher" - four MSRV bumps out
//! of date - because the review looked at the pages that had changed.
//!
//! These checks read the same facts out of the binary and the manifest, so a
//! command added without a page, a page kept after the command went away, or an
//! MSRV bumped in `Cargo.toml` alone fails the build instead of shipping.

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

    const REFERENCE_DIR: &str = "docs/src/content/docs/reference";

    /// Commands compiled into debug builds only, which users never receive.
    ///
    /// The test binary is a debug build, so `--help` lists these too; a page is
    /// only owed for what ships.
    const DEBUG_ONLY: [&str; 1] = ["migrations"];

    /// Subcommand names as the shipped binary lists them.
    ///
    /// Parsed from `--help` rather than from the source: the help text is what
    /// a user actually sees, and parsing it keeps the check honest about
    /// aliases and hidden variants.
    fn shipped_commands() -> Vec<String> {
        let help = String::from_utf8(
            std::process::Command::new(env!("CARGO_BIN_EXE_kasl"))
                .arg("--help")
                .output()
                .expect("cannot run kasl --help")
                .stdout,
        )
        .expect("help output is not utf-8");

        let commands: Vec<String> = help
            .lines()
            .skip_while(|l| !l.starts_with("Commands:"))
            .skip(1)
            .take_while(|l| l.starts_with("  ") && !l.trim().is_empty())
            .filter_map(|l| l.split_whitespace().next())
            .filter(|name| *name != "help") // clap's own, not ours
            .filter(|name| !DEBUG_ONLY.contains(name))
            .map(str::to_string)
            .collect();

        assert!(!commands.is_empty(), "could not parse subcommands out of --help");
        commands
    }

    /// Reference page slugs currently on the site.
    fn reference_pages() -> Vec<String> {
        let dir = repo_root().join(REFERENCE_DIR);
        let mut pages: Vec<String> = fs::read_dir(&dir)
            .unwrap_or_else(|e| panic!("cannot list {}: {e}", dir.display()))
            .filter_map(|entry| {
                let path = entry.ok()?.path();
                (path.extension()? == "md").then(|| path.file_stem()?.to_str().map(str::to_string))?
            })
            .collect();
        pages.sort();
        pages
    }

    #[test]
    fn every_shipped_command_has_a_reference_page() {
        let pages = reference_pages();
        for command in shipped_commands() {
            assert!(
                pages.contains(&command),
                "`kasl {command}` ships but has no page in {REFERENCE_DIR}; known pages: {pages:?}"
            );
        }
    }

    /// The subcommands `kasl <command> --help` lists.
    fn shipped_subcommands(command: &str) -> Vec<String> {
        let help = String::from_utf8(
            std::process::Command::new(env!("CARGO_BIN_EXE_kasl"))
                .args([command, "--help"])
                .output()
                .unwrap_or_else(|error| panic!("cannot run kasl {command} --help: {error}"))
                .stdout,
        )
        .expect("help output is not utf-8");

        help.lines()
            .skip_while(|l| !l.starts_with("Commands:"))
            .skip(1)
            .take_while(|l| l.starts_with("  ") && !l.trim().is_empty())
            .filter_map(|l| l.split_whitespace().next())
            .filter(|name| *name != "help")
            .map(str::to_string)
            .collect()
    }

    #[test]
    fn the_overview_guide_lists_every_shipped_command() {
        // `server` shipped in v1.7 and never reached this guide - the same
        // omission `inbox` had before v1.5. A reader who starts at the
        // overview simply never learns the command exists, and the page
        // coverage gate cannot see it: the reference page was there all along.
        let overview = read("docs/src/content/docs/guides/command-overview.md");
        for command in shipped_commands() {
            assert!(
                overview.contains(&format!("`{command}`")),
                "`kasl {command}` ships but is not listed in the command overview guide"
            );
        }
    }

    #[test]
    fn every_shipped_subcommand_is_documented_on_its_page() {
        // The coverage gate above only sees top-level commands, so a new
        // `kasl server flush` could ship with nothing on the page and nothing
        // complaining - the same gap that let `command-overview.md` go without
        // `inbox`. A subcommand is documented where its parent lives, so that
        // is where it is looked for.
        for command in shipped_commands() {
            let subcommands = shipped_subcommands(&command);
            if subcommands.is_empty() {
                continue;
            }

            let page = read(format!("{REFERENCE_DIR}/{command}.md"));
            for subcommand in subcommands {
                assert!(
                    page.contains(&format!("{command} {subcommand}")),
                    "`kasl {command} {subcommand}` ships but is not mentioned in {REFERENCE_DIR}/{command}.md"
                );
            }
        }
    }

    #[test]
    fn every_reference_page_documents_a_shipped_command() {
        // `breaks` kept its page for a while after the command became
        // `pauses`. A page for a command that no longer exists reads as a
        // feature that is merely broken.
        let commands = shipped_commands();
        for page in reference_pages() {
            assert!(
                commands.contains(&page),
                "{REFERENCE_DIR}/{page}.md documents `kasl {page}`, which is not a command; shipped: {commands:?}"
            );
        }
    }

    #[test]
    fn reference_titles_are_the_bare_command_name() {
        // Line-standard: the product name already sits in the site header and
        // in every sidebar entry, so `title: kasl export` only adds noise.
        for page in reference_pages() {
            let text = read(format!("{REFERENCE_DIR}/{page}.md"));
            let title = text
                .lines()
                .find_map(|line| line.trim().strip_prefix("title:").map(|v| v.trim().trim_matches('"').to_string()))
                .unwrap_or_else(|| panic!("{page}.md has no title in its frontmatter"));
            assert_eq!(title, page, "{page}.md is titled {title:?}; reference titles are the bare command name");
        }
    }

    #[test]
    fn every_reference_page_points_at_its_neighbours() {
        // Line-standard: a reference page ends by naming the commands next to
        // it, so the reader is never left at a dead end. The section drifted
        // in twice as "Integration with Other Commands" duplicating it, and
        // was missing outright from two pages.
        for page in reference_pages() {
            let text = read(format!("{REFERENCE_DIR}/{page}.md"));
            assert!(text.contains("## Related commands"), "{page}.md has no `## Related commands` section");
            assert!(
                !text.contains("## Integration with Other Commands"),
                "{page}.md still carries `Integration with Other Commands`, which duplicates `Related commands`"
            );
        }
    }

    #[test]
    fn the_documented_msrv_matches_the_manifest() {
        // `getting-started.md` asked for "Rust 1.70 or higher" through four
        // MSRV bumps: the number lives in prose, so nothing but a reader
        // noticed. rust-version is the promise; the page must repeat it.
        let manifest = read("Cargo.toml");
        let msrv = manifest
            .lines()
            .find_map(|line| {
                line.trim()
                    .strip_prefix("rust-version")
                    .map(|v| v.trim_start_matches([' ', '=']).trim().trim_matches('"').to_string())
            })
            .expect("`rust-version` not found in Cargo.toml");

        let page = "docs/src/content/docs/getting-started.md";
        let text = read(page);
        let expected = format!("Rust {msrv} or higher");
        assert!(
            text.contains(&expected),
            "Cargo.toml pins rust-version = {msrv:?}, but {page} does not say {expected:?}"
        );
    }

    #[test]
    fn no_page_pins_a_release_version() {
        // Release numbers written into prose go stale on the next tag and
        // nothing notices: `self-update.md` illustrated its output with a
        // version the binary could no longer offer, and both installers told
        // users to "set KASL_VERSION to a tag like v1.4.0" long after 1.4.0
        // stopped being the newest. Examples use the shape (vX.Y.Z), not a
        // number; the real version belongs in Cargo.toml alone.
        let mut files: Vec<PathBuf> = walk_markdown(&repo_root().join("docs/src/content/docs"));
        files.push(repo_root().join("README.md"));
        files.push(repo_root().join("tools/install.sh"));
        files.push(repo_root().join("tools/install.ps1"));

        for path in files {
            let text = fs::read_to_string(&path).expect("cannot read a documented file");
            let relative = path.strip_prefix(repo_root()).unwrap_or(&path).display().to_string();
            for (line_no, line) in text.lines().enumerate() {
                // A release version: v?N.N.N. Dependency pins and dates do not
                // live in these files, so any such triple is a release number.
                let mut chars = line.char_indices().peekable();
                while let Some((idx, c)) = chars.next() {
                    if !c.is_ascii_digit() {
                        continue;
                    }
                    // Only look at the start of a run of digits.
                    if idx > 0 && line[..idx].ends_with(|p: char| p.is_ascii_digit() || p == '.') {
                        continue;
                    }
                    let tail = &line[idx..];
                    let digits_dots: String = tail.chars().take_while(|c| c.is_ascii_digit() || *c == '.').collect();
                    let parts: Vec<&str> = digits_dots.split('.').collect();
                    if parts.len() == 3 && parts.iter().all(|p| !p.is_empty() && p.chars().all(|c| c.is_ascii_digit())) {
                        panic!(
                            "{relative}:{} pins the release version {digits_dots}; use the vX.Y.Z shape instead",
                            line_no + 1
                        );
                    }
                    // Skip the rest of this run.
                    while chars.peek().is_some_and(|(_, c)| c.is_ascii_digit() || *c == '.') {
                        chars.next();
                    }
                }
            }
        }
    }

    #[test]
    fn docs_only_show_commands_that_exist() {
        // Same guard the README carries, over every page: a `kasl <word>` in a
        // code block must name a real subcommand. Catches the renames
        // (`init` -> `setup`, `update` -> `self-update`, `breaks` -> `pauses`)
        // that leave working prose behind on pages nobody reopened.
        let commands = shipped_commands();
        // Aliases stay accepted until 2.0, so pages may still mention them -
        // deliberately, when documenting the rename itself. The debug-only
        // commands are fair game too: `database.md` explains the schema to
        // someone building from source, who does have them.
        let also_valid = ["init", "update"];

        let docs_dir = repo_root().join("docs/src/content/docs");
        let mut checked = 0;
        for path in walk_markdown(&docs_dir) {
            let text = fs::read_to_string(&path).expect("cannot read a docs page");
            let relative = path.strip_prefix(repo_root()).unwrap_or(&path).display().to_string();
            // Only inside fenced code blocks: prose says things like "kasl
            // supports integration with...", which is a sentence, not a call.
            let mut in_code = false;
            for (line_no, line) in text.lines().enumerate() {
                let line = line.trim();
                if line.starts_with("```") {
                    in_code = !in_code;
                    continue;
                }
                if !in_code {
                    continue;
                }
                let line = line.trim_start_matches("$ ");
                let Some(rest) = line.strip_prefix("kasl ") else { continue };
                let Some(word) = rest.split_whitespace().next() else { continue };
                // `kasl <command> --help` is a placeholder, not a call.
                if word.starts_with('-') || word.starts_with('<') || also_valid.contains(&word) || DEBUG_ONLY.contains(&word) {
                    continue;
                }
                assert!(
                    commands.contains(&word.to_string()),
                    "{relative}:{} shows `kasl {word}`, which is not a command; shipped: {commands:?}",
                    line_no + 1
                );
                checked += 1;
            }
        }
        assert!(checked > 50, "only {checked} command invocations found in the docs - is the walk working?");
    }

    #[test]
    fn internal_links_resolve_to_a_page() {
        // Starlight builds a broken internal link without complaining, so a
        // page renamed or removed leaves a 404 behind that only a reader
        // finds. Every `](/some/path/)` must have a file behind it.
        let docs_dir = repo_root().join("docs/src/content/docs");
        let mut checked = 0;
        for path in walk_markdown(&docs_dir) {
            let text = fs::read_to_string(&path).expect("cannot read a docs page");
            let relative = path.strip_prefix(repo_root()).unwrap_or(&path).display().to_string();
            for (line_no, line) in text.lines().enumerate() {
                let mut rest = line;
                while let Some(at) = rest.find("](/") {
                    rest = &rest[at + 2..];
                    let Some(close) = rest.find(')') else { break };
                    let target = &rest[..close];
                    rest = &rest[close..];

                    // Site-absolute page links only: anchors and files served
                    // from public/ are not content pages.
                    if !target.ends_with('/') || target.contains('#') {
                        continue;
                    }
                    let slug = target.trim_matches('/');
                    let exists = ["md", "mdx"].iter().any(|ext| docs_dir.join(format!("{slug}.{ext}")).exists())
                        || docs_dir.join(slug).join("index.md").exists()
                        || docs_dir.join(slug).join("index.mdx").exists();
                    assert!(exists, "{relative}:{} links to /{slug}/, which has no page", line_no + 1);
                    checked += 1;
                }
            }
        }
        assert!(checked > 20, "only {checked} internal links found - is the walk working?");
    }

    /// Every markdown/MDX file under a docs directory, recursively.
    fn walk_markdown(dir: &Path) -> Vec<PathBuf> {
        let mut found = Vec::new();
        let entries = fs::read_dir(dir).unwrap_or_else(|e| panic!("cannot list {}: {e}", dir.display()));
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                found.extend(walk_markdown(&path));
            } else if matches!(path.extension().and_then(|e| e.to_str()), Some("md" | "mdx")) {
                found.push(path);
            }
        }
        found
    }
}
