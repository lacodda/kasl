//! Guards the facts that must agree before a version is published.
//!
//! kasl ships to three places - GitHub, crates.io and npm - each of which
//! renders its own copy of the metadata. They drift silently: nothing fails
//! when `npm/package.json` still says 1.0.0, or when the npm page describes
//! the product differently from the crate. The drift is only visible after
//! publishing, when it is too late to take back.
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

    /// Extracts a top-level `key = "value"` from Cargo.toml.
    ///
    /// Deliberately naive: it reads only the `[package]` block, which is all
    /// these checks need, and avoids adding a TOML parser as a dev-dependency.
    fn cargo_field(key: &str) -> String {
        let manifest = read("Cargo.toml");
        for line in manifest.lines() {
            let line = line.trim();
            // Stop at the next section: `version` also appears under [lib],
            // [[bin]] and in every dependency.
            if line.starts_with('[') && line != "[package]" {
                break;
            }
            let Some((name, value)) = line.split_once('=') else { continue };
            // Exact match, so `rust-version` cannot answer a lookup for `version`.
            if name.trim() != key {
                continue;
            }
            return value.trim().trim_matches('"').to_string();
        }
        panic!("`{key}` not found in the [package] block of Cargo.toml");
    }

    /// Extracts a `"key": "value"` from a JSON file, without a JSON dependency.
    fn json_field(file: &str, key: &str) -> String {
        let text = read(file);
        let needle = format!("\"{key}\"");
        let start = text.find(&needle).unwrap_or_else(|| panic!("`{key}` not found in {file}"));
        let after = &text[start + needle.len()..];
        let after = after.trim_start().trim_start_matches(':').trim_start();
        let after = after.strip_prefix('"').unwrap_or_else(|| panic!("`{key}` in {file} is not a string"));
        after[..after.find('"').expect("unterminated string")].to_string()
    }

    #[test]
    fn npm_package_version_matches_the_crate() {
        let crate_version = cargo_field("version");
        let npm_version = json_field("npm/package.json", "version");

        assert_eq!(
            npm_version, crate_version,
            "npm/package.json version ({npm_version}) differs from Cargo.toml ({crate_version}); \
             the npm page would advertise a version that was never released"
        );
    }

    #[test]
    fn npm_wrapper_downloads_the_matching_binary() {
        let crate_version = cargo_field("version");
        let binary_tag = json_field("npm/package.json", "binary");

        assert_eq!(
            binary_tag,
            format!("v{crate_version}"),
            "the npm wrapper points at release {binary_tag} while this is {crate_version}; \
             installing from npm would fetch the wrong binary"
        );
    }

    #[test]
    fn the_product_is_described_the_same_way_everywhere() {
        let crate_description = cargo_field("description");
        let npm_description = json_field("npm/package.json", "description");

        assert_eq!(
            npm_description, crate_description,
            "crates.io and npm describe the product differently; \
             Cargo.toml `description` is the single source"
        );
    }

    #[test]
    fn readme_is_shared_rather_than_duplicated() {
        // A second copy under npm/ is what let the two pages drift apart. The
        // npm package takes the root README at publish time instead.
        let duplicate = repo_root().join("npm/README.md");
        assert!(
            !duplicate.exists(),
            "npm/README.md exists again; it will drift from the root README. \
             The publish workflow copies the root one into npm/ instead."
        );
    }

    #[test]
    fn readme_links_resolve_off_github() {
        // The same file is rendered on crates.io and npm, where a relative
        // path has no repository to resolve against: the banner turns into a
        // broken image and the links 404.
        let readme = read("README.md");

        for (line_no, line) in readme.lines().enumerate() {
            for (marker, kind) in [("src=\"", "image"), ("](", "link")] {
                let mut rest = line;
                while let Some(at) = rest.find(marker) {
                    let target = &rest[at + marker.len()..];
                    let end = if marker == "](" { ')' } else { '"' };
                    let target = &target[..target.find(end).unwrap_or(target.len())];

                    let relative = !target.starts_with("http") && !target.starts_with('#') && !target.is_empty();
                    assert!(
                        !relative,
                        "README line {}: relative {kind} `{target}` breaks on crates.io and npm; use an absolute URL",
                        line_no + 1
                    );

                    rest = &rest[at + marker.len()..];
                }
            }
        }
    }

    #[test]
    fn readme_only_shows_commands_that_exist() {
        // The old README documented `kasl adjust` for months after the command
        // was removed. Every `$ kasl <word>` in a console block must name a
        // real subcommand.
        let readme = read("README.md");
        let help = String::from_utf8(
            std::process::Command::new(env!("CARGO_BIN_EXE_kasl"))
                .arg("--help")
                .output()
                .expect("cannot run kasl --help")
                .stdout,
        )
        .expect("help output is not utf-8");

        // Subcommand names are the indented first words in the Commands block.
        let known: Vec<String> = help
            .lines()
            .skip_while(|l| !l.starts_with("Commands:"))
            .skip(1)
            .take_while(|l| l.starts_with("  ") && !l.trim().is_empty())
            .filter_map(|l| l.split_whitespace().next())
            .map(str::to_string)
            .collect();

        assert!(!known.is_empty(), "could not parse subcommands out of --help");

        for line in readme.lines() {
            let line = line.trim();
            let Some(rest) = line.strip_prefix("$ kasl ") else { continue };
            let Some(word) = rest.split_whitespace().next() else { continue };
            if word.starts_with('-') {
                continue; // a flag on the bare binary, e.g. `kasl --version`
            }
            assert!(
                known.contains(&word.to_string()),
                "README shows `kasl {word}`, which is not a command; known: {known:?}"
            );
        }
    }
}
