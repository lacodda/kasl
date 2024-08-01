use std::env;
use std::fs;
use std::fs::File;
use std::io::Write;
use std::path::Path;
use toml::Value;

fn main() {
    let out_dir = env::var("OUT_DIR").unwrap();
    let dest_path = Path::new(&out_dir).join("package.rs");
    let mut f = File::create(&dest_path).unwrap();

    let cargo_toml = fs::read_to_string("Cargo.toml").expect("Failed to read Cargo.toml");
    let cargo_toml: Value = toml::from_str(&cargo_toml).expect("Failed to parse Cargo.toml");

    let name = env::var("CARGO_PKG_NAME").unwrap();
    write!(f, "pub const PKG_NAME: &str = \"{}\";\n", name).unwrap();

    let version = env::var("CARGO_PKG_VERSION").unwrap();
    write!(f, "pub const PKG_VERSION: &str = \"{}\";\n", version).unwrap();

    if let Some(metadata) = cargo_toml.get("package").and_then(|pkg| pkg.get("metadata")).and_then(|meta| meta.as_table()) {
        for (key, value) in metadata {
            if let Some(value) = value.as_str() {
                write!(f, "pub const PKG_{}: &str = \"{}\";\n", key.to_uppercase(), value).unwrap();
            }
        }
    }
}
