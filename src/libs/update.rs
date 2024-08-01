use crate::libs::data_storage::DataStorage;
use chrono::{DateTime, Duration, Utc};
use flate2::read::GzDecoder;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::env;
use std::fs::{self, File};
use std::io::copy;
use std::path::PathBuf;
use tar::Archive;
include!(concat!(env!("OUT_DIR"), "/package.rs"));

const LAST_CHECK_FILE: &str = ".last_update_check";

#[derive(Serialize, Deserialize, Debug)]
struct Release {
    tag_name: String,
    assets: Vec<Asset>,
}

#[derive(Serialize, Deserialize, Debug)]
struct Asset {
    browser_download_url: String,
    name: String,
}

#[derive(Debug)]
pub struct Update {
    pub client: Client,
    pub owner: String,
    pub name: String,
    pub version: String,
    pub latest_version: Option<String>,
    pub download_url: Option<String>,
    pub releases_url: String,
    pub last_check_file: PathBuf,
}

impl Update {
    pub fn new() -> Self {
        Self {
            client: Client::new(),
            owner: PKG_OWNER.to_owned(),
            name: PKG_NAME.to_owned(),
            version: PKG_VERSION.to_owned(),
            latest_version: None,
            download_url: None,
            last_check_file: DataStorage::new().get_path(LAST_CHECK_FILE).expect("DataStorage get_path error"),
            releases_url: format!("https://api.github.com/repos/{}/{}/releases/latest", PKG_OWNER, PKG_NAME),
        }
    }

    pub async fn show_msg() {
        match Self::new().check() {
            Some(update) => match update.update_release().await {
                Ok(updated) => {
                    let name = updated.name;
                    println!(
                        "\nA new version of {} is available: v{}\nUpgrade now by running: {} update\n",
                        &name,
                        &updated.latest_version.unwrap(),
                        &name
                    );
                }
                Err(e) => {
                    println!("Error during update: {}", e);
                }
            },
            None => (),
        }
    }

    pub async fn update(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let resp = self.client.get(&self.download_url.clone().unwrap()).send().await?;
        let tar_gz_path = format!("{}.tar.gz", &self.name);
        let mut out = File::create(&tar_gz_path)?;
        let content = resp.bytes().await?;
        copy(&mut content.as_ref(), &mut out)?;
        self.extract_and_replace_binary(&tar_gz_path)?;

        println!(
            "The {} application has been successfully updated to version {}!",
            &self.name,
            &self.latest_version.clone().unwrap()
        );

        Ok(())
    }

    pub async fn update_release(mut self) -> Result<Self, Box<dyn std::error::Error>> {
        let release = self
            .client
            .get(&self.releases_url)
            .header("User-Agent", &self.name)
            .send()
            .await?
            .json::<Release>()
            .await?;
        let latest_version = release.tag_name[1..].to_owned();
        self.update_last_check_time();

        if latest_version > self.version {
            self.latest_version = Some(latest_version);
            self.download_url = release
                .assets
                .iter()
                .find(|asset| asset.name.contains(&self.get_platform_name()))
                .map(|asset| asset.browser_download_url.clone());
        }

        Ok(self)
    }

    fn update_last_check_time(&self) {
        let now = Utc::now().to_rfc3339();
        fs::write(&self.last_check_file, now).expect("Unable to write last check time");
    }

    fn check(self) -> Option<Self> {
        match fs::read_to_string(&self.last_check_file) {
            Ok(content) => {
                let last_check: DateTime<Utc> = content.parse().unwrap_or(Utc::now() - Duration::days(2));
                if Utc::now().signed_duration_since(last_check) > Duration::days(1) {
                    return Some(self);
                }
                return None;
            }
            Err(_) => Some(self),
        }
    }

    fn extract_and_replace_binary(&self, tar_gz_path: &str) -> Result<(), Box<dyn std::error::Error>> {
        let tar_gz = File::open(tar_gz_path)?;
        let tar = GzDecoder::new(tar_gz);
        let mut archive = Archive::new(tar);

        let current_exe = env::current_exe()?;
        let current_exe_backup = current_exe.with_extension("bak");

        for entry in archive.entries()? {
            let mut entry = entry?;
            let entry_path = entry.path()?;
            if entry_path.ends_with(current_exe.file_name().unwrap()) {
                // Backup current executable
                fs::rename(&current_exe, &current_exe_backup)?;
                // Extract new executable to the current executable location
                entry.unpack(&current_exe)?;
            } else {
                // Extract other files to the same directory as the executable
                let dest_path = current_exe.parent().unwrap().join(&entry_path);
                entry.unpack(dest_path)?;
            }
        }

        Ok(())
    }

    fn get_platform_name(&self) -> String {
        let arch = env::consts::ARCH;
        let os = match env::consts::OS {
            "windows" => "pc-windows-msvc",
            "macos" => "apple-darwin",
            _ => "unknown-linux-musl",
        };
        format!("{}-v{}-{}-{}", &self.name, &self.latest_version.clone().unwrap(), arch, os)
    }
}
