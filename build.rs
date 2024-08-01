use std::env;
use std::fs::{self, File};
use std::io::{self, Write};
use std::path::Path;
use toml::Value;

struct AppMetadata {
    file: std::fs::File,
}

impl AppMetadata {
    pub fn new() -> io::Result<Self> {
        let out_dir = env::var("OUT_DIR").unwrap();
        let dest_path = Path::new(&out_dir).join("app_metadata.rs");
        let file = File::create(&dest_path).unwrap();
        Ok(Self { file })
    }

    pub fn write(&mut self, key: &str, value: &str) -> io::Result<()> {
        write!(
            self.file,
            "#[allow(unused)]\npub const APP_METADATA_{}: &str = \"{}\";\n",
            &key.to_uppercase(),
            &value
        )
    }
}

fn main() -> io::Result<()> {
    let cargo_toml = fs::read_to_string("Cargo.toml").expect("Failed to read Cargo.toml");
    let cargo_toml: Value = toml::from_str(&cargo_toml).expect("Failed to parse Cargo.toml");

    let mut app_metadata = AppMetadata::new()?;
    app_metadata.write("NAME", &env::var("CARGO_PKG_NAME").unwrap())?;
    app_metadata.write("VERSION", &env::var("CARGO_PKG_VERSION").unwrap())?;

    if let Some(metadata) = cargo_toml.get("package").and_then(|pkg| pkg.get("metadata")).and_then(|meta| meta.as_table()) {
        for (key, value) in metadata {
            if let Some(value) = value.as_str() {
                app_metadata.write(key, value)?;
            }
        }
    }
    Ok(())
}
