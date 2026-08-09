//! Main entry point for the kasl application.
//!
//! Thin wrapper over [`kasl::run`], which both this binary and the `ka` alias
//! share so their behaviour cannot drift apart.

fn main() -> anyhow::Result<()> {
    kasl::run()
}
