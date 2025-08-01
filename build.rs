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
            &key.to_uppercase(),
            &value
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
            &key.to_uppercase(),
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
        let mut res = winres::WindowsResource::new();
        res.set_icon("icon.ico");
        res.compile().unwrap();
    }

    // Load environment variables from .env file if it exists
    // This allows developers to set encryption keys locally
    let _ = dotenv();

    // Parse Cargo.toml to extract package metadata
    let cargo_toml = fs::read_to_string("Cargo.toml").expect("Failed to read Cargo.toml");
    let cargo_toml: Value = toml::from_str(&cargo_toml).expect("Failed to parse Cargo.toml");

    // Initialize metadata writer and add basic package information
    let mut app_metadata = AppMetadata::new()?;
    app_metadata.write("NAME", &env::var("CARGO_PKG_NAME").unwrap())?;
    app_metadata.write("VERSION", &env::var("CARGO_PKG_VERSION").unwrap())?;

    // Extract custom metadata from Cargo.toml [package.metadata] section
    if let Some(metadata) = cargo_toml.get("package").and_then(|pkg| pkg.get("metadata")).and_then(|meta| meta.as_table()) {
        for (key, value) in metadata {
            if let Some(value) = value.as_str() {
                app_metadata.write(key, value)?;
            }
        }
    }

    // Handle encryption keys for secure storage
    // Keys can be provided via environment variables or generated deterministically
    let (encryption_key, encryption_iv) = match (env::var("ENCRYPTION_KEY"), env::var("ENCRYPTION_IV")) {
        (Ok(key), Ok(iv)) => {
            // Validate that provided keys have correct lengths for AES-256
            let key_bytes = key.as_bytes();
            let iv_bytes = iv.as_bytes();

            if key_bytes.len() != 32 {
                panic!("ENCRYPTION_KEY must be exactly 32 bytes long, got {} bytes", key_bytes.len());
            }
            if iv_bytes.len() != 16 {
                panic!("ENCRYPTION_IV must be exactly 16 bytes long, got {} bytes", iv_bytes.len());
            }

            (key_bytes.to_vec(), iv_bytes.to_vec())
        }
        _ => {
            // Generate deterministic default keys based on package name
            // This ensures consistent keys across builds while maintaining security
            let package_name = env::var("CARGO_PKG_NAME").unwrap_or_else(|_| "kasl".to_string());

            // Create deterministic but unique keys based on package name
            let mut default_key = format!("{}_default_encryption_key_32b", package_name);
            let mut default_iv = format!("{}_iv_16b", package_name);

            // Ensure exact lengths required by AES-256
            default_key.truncate(32);
            while default_key.len() < 32 {
                default_key.push('!');
            }

            default_iv.truncate(16);
            while default_iv.len() < 16 {
                default_iv.push('!');
            }

            // Warn developer about using default keys
            println!("cargo:warning=ENCRYPTION_KEY or ENCRYPTION_IV not found in environment.");
            println!("cargo:warning=Using default keys. For production, create a .env file with:");
            println!("cargo:warning=ENCRYPTION_KEY=your_32_byte_key_here!!!!!!!!!");
            println!("cargo:warning=ENCRYPTION_IV=your_16_byte_iv!");

            (default_key.into_bytes(), default_iv.into_bytes())
        }
    };

    // Embed encryption keys as compile-time byte arrays
    app_metadata.write_bytes("ENCRYPTION_KEY", &encryption_key)?;
    app_metadata.write_bytes("ENCRYPTION_IV", &encryption_iv)?;

    Ok(())
}
