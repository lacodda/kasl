//! `ka` - short alias for the `kasl` binary.
//!
//! Same program under the brand's two-letter code, so `ka report` and
//! `kasl report` are interchangeable. Application identity - data directory,
//! self-update channel, keyring service - is fixed to "kasl" regardless of
//! which name was typed, so installing the alias never moves a user's data.

fn main() -> anyhow::Result<()> {
    kasl::run()
}
