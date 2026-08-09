//! Build script for kasl application
//!
//! This build script handles:
//! - Windows resource embedding (icon)
//! - Loading environment variables from .env file
//! - Extracting metadata from Cargo.toml
//! - Generating encryption keys for secure storage
//! - Creating compile-time constants for application metadata

use dotenv::dotenv;
use std::env;
use std::fs::{self, File};
use std::io::{self, Write};
use std::path::Path;
use toml::Value;

/// Helper struct for generating compile-time application metadata
struct AppMetadata {
    file: std::fs::File,
}

impl AppMetadata {
    /// Creates a new AppMetadata writer that generates a Rust source file
    /// containing compile-time constants for application metadata
    pub fn new() -> io::Result<Self> {
        let out_dir = env::var("OUT_DIR").unwrap();
        let dest_path = Path::new(&out_dir).join("app_metadata.rs");
        let file = File::create(&dest_path).unwrap();
        Ok(Self { file })
    }

    /// Writes a string constant to the metadata file
    ///
    /// # Arguments
    /// * `key` - The constant name (will be prefixed with APP_METADATA_)
    /// * `value` - The string value
    pub fn write(&mut self, key: &str, value: &str) -> io::Result<()> {
        write!(
            self.file,
            "#[allow(unused)]\npub const APP_METADATA_{}: &str = \"{}\";\n",
            key.to_uppercase(),
            value
        )
    }

    /// Writes a byte array constant to the metadata file
    /// Used for embedding encryption keys as compile-time constants
    ///
    /// # Arguments
    /// * `key` - The constant name (will be prefixed with APP_METADATA_)
    /// * `value` - The byte array value
    pub fn write_bytes(&mut self, key: &str, value: &[u8]) -> io::Result<()> {
        write!(
            self.file,
            "#[allow(unused)]\npub const APP_METADATA_{}: &[u8; {}] = &[",
            key.to_uppercase(),
            value.len()
        )?;

        for (i, byte) in value.iter().enumerate() {
            if i > 0 {
                write!(self.file, ", ")?;
            }
            write!(self.file, "{}", byte)?;
        }

        writeln!(self.file, "];")
    }
}

fn main() -> io::Result<()> {
    // Windows-specific: Embed application icon as a resource
    #[cfg(windows)]
    {
        println!("cargo:rerun-if-changed=assets/icon.ico");
        let mut res = winres::WindowsResource::new();
        res.set_icon("assets/icon.ico");
        res.compile().unwrap();
    }

    // Load environment variables from .env file if it exists
    // This allows developers to set encryption keys locally
    let _ = dotenv();

    // Parse Cargo.toml to extract package metadata
    let cargo_toml = fs::read_to_string("Cargo.toml").expect("Failed to read Cargo.toml");
    let cargo_toml: Value = toml::from_str(&cargo_toml).expect("Failed to parse Cargo.toml");

    // The application identity (data directory, self-update asset names) follows
    // the binary name from [[bin]], not the crate name: the crate is published
    // as "kasl-cli" while the binary and on-disk identity stay "kasl".
    //
    // The FIRST [[bin]] wins, and must remain "kasl". The `ka` alias that follows
    // it is the same program under a shorter name; if it ever came first, every
    // user's data directory and update channel would move.
    let bin_name = cargo_toml
        .get("bin")
        .and_then(|bins| bins.as_array())
        .and_then(|bins| bins.first())
        .and_then(|bin| bin.get("name"))
        .and_then(|name| name.as_str())
        .map(str::to_string)
        .unwrap_or_else(|| env::var("CARGO_PKG_NAME").unwrap());

    // Initialize metadata writer and add basic package information
    let mut app_metadata = AppMetadata::new()?;
    app_metadata.write("NAME", &bin_name)?;
    app_metadata.write("VERSION", &env::var("CARGO_PKG_VERSION").unwrap())?;

    // Extract custom metadata from Cargo.toml [package.metadata] section
    if let Some(metadata) = cargo_toml.get("package").and_then(|pkg| pkg.get("metadata")).and_then(|meta| meta.as_table()) {
        for (key, value) in metadata {
            if let Some(value) = value.as_str() {
                app_metadata.write(key, value)?;
            }
        }
    }

    // Legacy AES keys, kept only to read credentials written before 1.0.
    //
    // Credentials now live in the OS keyring; nothing is encrypted with these
    // keys any more. They are fixed rather than taken from the environment
    // because every published binary was built without ENCRYPTION_KEY set and
    // therefore used exactly these values - reproducing them is what lets the
    // one-time migration in libs::secret read what those builds wrote. They are
    // not secret and never were: the derivation was always visible here.
    //
    // Remove once migration support is dropped.
    let mut legacy_key = format!("{}_default_encryption_key_32b", bin_name);
    let mut legacy_iv = format!("{}_iv_16b", bin_name);

    legacy_key.truncate(32);
    while legacy_key.len() < 32 {
        legacy_key.push('!');
    }

    legacy_iv.truncate(16);
    while legacy_iv.len() < 16 {
        legacy_iv.push('!');
    }

    app_metadata.write_bytes("ENCRYPTION_KEY", legacy_key.as_bytes())?;
    app_metadata.write_bytes("ENCRYPTION_IV", legacy_iv.as_bytes())?;

    Ok(())
}
