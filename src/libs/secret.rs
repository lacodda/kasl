use super::data_storage::DataStorage;
use aes::Aes256;
use anyhow::Result;
use base64::prelude::*;
use block_modes::block_padding::Pkcs7;
use block_modes::{BlockMode, Cbc};
use dialoguer::{theme::ColorfulTheme, Password};
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::PathBuf;

// Include generated metadata with encryption keys
include!(concat!(env!("OUT_DIR"), "/app_metadata.rs"));

type Aes256Cbc = Cbc<Aes256, Pkcs7>;

#[derive(Clone, Debug)]
pub struct Secret {
    password: Option<String>,
    prompt: String,
    secret_file_path: PathBuf,
    key: Vec<u8>,
    iv: Vec<u8>,
}

impl Secret {
    pub fn new(secret_name: &str, prompt: &str) -> Self {
        // Use compile-time embedded keys
        let key = APP_METADATA_ENCRYPTION_KEY.to_vec();
        let iv = APP_METADATA_ENCRYPTION_IV.to_vec();

        let secret_file_path = DataStorage::new().get_path(secret_name).unwrap_or_else(|_| PathBuf::from(secret_name));

        Self {
            password: None,
            secret_file_path,
            prompt: prompt.to_owned(),
            key,
            iv,
        }
    }

    fn set_password(&self, password: &str) -> Self {
        Self {
            password: Some(password.to_owned()),
            ..self.clone()
        }
    }

    pub fn get_or_prompt(&self) -> Result<String> {
        if fs::metadata(&self.secret_file_path).is_ok() {
            if let Ok(password) = self.decrypt() {
                return Ok(password);
            }
        }
        self.prompt()
    }

    pub fn prompt(&self) -> Result<String> {
        let password = Password::with_theme(&ColorfulTheme::default()).with_prompt(&self.prompt).interact().unwrap();
        self.set_password(&password).encrypt()?;
        Ok(password)
    }

    fn encrypt(&self) -> Result<Self> {
        let cipher = Aes256Cbc::new_from_slices(&self.key, &self.iv)?;
        let password = &self.password.clone().unwrap();
        let ciphertext = cipher.encrypt_vec(&password.as_bytes());
        let encoded = BASE64_STANDARD.encode(&ciphertext);

        // Create directory if it doesn't exist
        if let Some(parent) = self.secret_file_path.parent() {
            let _ = fs::create_dir_all(parent);
        }

        let mut file = File::create(&self.secret_file_path)?;
        file.write_all(encoded.as_bytes())?;

        Ok(self.clone())
    }

    fn decrypt(&self) -> Result<String> {
        let mut file = File::open(&self.secret_file_path)?;
        let mut encoded = String::new();
        file.read_to_string(&mut encoded)?;
        let ciphertext = BASE64_STANDARD.decode(encoded)?;
        let cipher = Aes256Cbc::new_from_slices(&self.key, &self.iv)?;
        let decrypted_ciphertext = cipher.decrypt_vec(&ciphertext)?;
        let decrypted_password = String::from_utf8(decrypted_ciphertext)?;

        Ok(decrypted_password)
    }
}
