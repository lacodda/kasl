//! The config file's JSON Schema, as published, and the config examples the
//! docs show against it.
//!
//! The schema is generated from the config types, and the docs site serves
//! the copy in `docs/public/config.schema.json` - the URL every saved config
//! names in `$schema`. A copy that drifted from the types would have editors
//! underline settings that work and pass ones that do not, so the copy is held
//! to the generated document here.
//!
//! Every config example the docs show - a ```json block titled
//! `config.json` - is checked by the same function as `kasl config check`, so
//! a page cannot teach a key that was renamed, retired or never existed.

use kasl::libs::config::schema;
use serde_json::Value;
use std::path::{Path, PathBuf};

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

#[test]
fn the_published_schema_is_the_one_kasl_generates() {
    let path = repo_root().join("docs/public/config.schema.json");
    let published: Value = serde_json::from_str(&std::fs::read_to_string(&path).expect("docs/public/config.schema.json exists")).unwrap();
    assert!(
        &published == schema::document(),
        "docs/public/config.schema.json is not the schema kasl generates; rewrite it with\n  cargo run -- config schema > docs/public/config.schema.json"
    );
}

/// The ```json blocks titled `config.json` in one page, with their line.
fn config_examples(page: &Path) -> Vec<(usize, String)> {
    let text = std::fs::read_to_string(page).unwrap();
    let mut examples = Vec::new();
    let mut open: Option<(usize, String)> = None;
    for (index, line) in text.lines().enumerate() {
        let trimmed = line.trim_start();
        match &mut open {
            None if trimmed.starts_with("```json") && trimmed.contains(r#"title="config.json""#) => open = Some((index + 1, String::new())),
            Some(_) if trimmed.starts_with("```") => examples.extend(open.take()),
            Some((_, body)) => {
                body.push_str(line);
                body.push('\n');
            }
            None => {}
        }
    }
    examples
}

fn markdown_pages(dir: &Path, found: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            markdown_pages(&path, found);
        } else if path.extension().is_some_and(|ext| ext == "md" || ext == "mdx") {
            found.push(path);
        }
    }
}

#[test]
fn every_config_example_in_the_docs_reads_as_written() {
    let mut pages = Vec::new();
    markdown_pages(&repo_root().join("docs/src/content/docs"), &mut pages);
    let mut checked = 0;
    let mut wrong = Vec::new();
    for page in &pages {
        for (line, body) in config_examples(page) {
            checked += 1;
            let at = format!("{}:{line}", page.strip_prefix(repo_root()).unwrap_or(page).display());
            let doc: Value = match serde_json::from_str(&body) {
                Ok(doc) => doc,
                Err(error) => {
                    wrong.push(format!("{at}: not JSON: {error}"));
                    continue;
                }
            };
            let findings = schema::check(&doc);
            for problem in findings.problems.iter().chain(&findings.notices) {
                wrong.push(format!("{at}: {problem}"));
            }
        }
    }
    // The gate must not pass by finding nothing: the configuration page
    // alone shows several examples.
    assert!(checked >= 6, "only {checked} config examples found - are they still titled config.json?");
    assert!(
        wrong.is_empty(),
        "config examples the docs show that kasl would not read as written:\n{}",
        wrong.join("\n")
    );
}

#[test]
fn a_misspelt_example_would_be_caught() {
    // The gate above is only worth having if a wrong example fails it.
    let findings = schema::check(&serde_json::json!({ "monitor": { "pause_treshold": 90 } }));
    assert_eq!(findings.problems.len(), 1);
}
