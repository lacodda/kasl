//! Credential storage backed by the operating system keyring.
//!
//! Passwords are kept in the platform credential store - Credential Manager on
//! Windows, Keychain on macOS, the Secret Service on Linux - so they are
//! protected by the user's login session rather than by this program.
//!
//! ## Usage
//!
//! ```rust,no_run
//! use kasl::libs::secret::Secret;
//!
//! # fn main() -> anyhow::Result<()> {
//! let secret = Secret::new(".jira_secret", "Enter your Jira password");
//! let password = secret.get_or_prompt()?;
//! # Ok(())
//! # }
//! ```

use super::data_storage::DataStorage;
use aes::Aes256;
use aes::cipher::block_padding::Pkcs7;
use aes::cipher::{BlockModeDecrypt, KeyIvInit};
use anyhow::{Context, Result};
use base64::prelude::*;
use dialoguer::{Password, theme::ColorfulTheme};
use keyring::Entry;
use std::fs;
use std::io::Read;
use std::path::PathBuf;

// Include generated metadata containing the legacy encryption keys.
// Still needed to read credentials written before the keyring migration.
include!(concat!(env!("OUT_DIR"), "/app_metadata.rs"));

/// Type alias for the legacy AES-256-CBC decryptor.
type Aes256CbcDec = cbc::Decryptor<Aes256>;

/// Service name under which kasl registers its credentials with the OS.
///
/// Appears verbatim in Credential Manager, Keychain and Secret Service, so it
/// must stay stable: changing it orphans every stored credential.
const KEYRING_SERVICE: &str = "lacodda.kasl";

/// The keyring service this run's credentials live under.
///
/// The OS keyring knows nothing of kasl's data directory: moving `HOME` or
/// `LOCALAPPDATA` gives a run a database and a config of its own, and still
/// the one keyring of the machine. Every test runs that way, and a test that
/// ran `kasl server disconnect` deleted the real agent token of whoever ran
/// the suite - a connected machine silently stopped sending its days. Keeping
/// tests off the keyring by discipline is a rule the next test forgets.
///
/// So a data directory under the system's temporary directory - where every
/// throwaway one lives, and no real installation does - keeps its secrets
/// apart: under a service named for that directory. A real installation is
/// never under it and keeps [`KEYRING_SERVICE`].
fn keyring_service() -> String {
    service_for(DataStorage::new().base(), &std::env::temp_dir())
}

/// [`keyring_service`] for a given data directory and temporary directory.
fn service_for(data_dir: &std::path::Path, temp_dir: &std::path::Path) -> String {
    let under_temp =
        data_dir.starts_with(temp_dir) || matches!((data_dir.canonicalize(), temp_dir.canonicalize()), (Ok(data), Ok(temp)) if data.starts_with(&temp));
    if !under_temp {
        return KEYRING_SERVICE.to_string();
    }
    // FNV-1a: stable across builds, so a test and the binary it runs agree.
    let hash = data_dir
        .to_string_lossy()
        .bytes()
        .fold(0xcbf2_9ce4_8422_2325_u64, |hash, byte| (hash ^ u64::from(byte)).wrapping_mul(0x0100_0000_01b3));
    format!("{KEYRING_SERVICE}.scratch-{hash:016x}")
}

/// Credential manager backed by the OS keyring.
///
/// Each instance addresses one credential, identified by the account name
/// derived from the legacy file name (`.jira_secret` becomes `jira`), and knows
/// how to ask the user for it when the store has nothing.
#[derive(Clone, Debug)]
pub struct Secret {
    /// Account name for this credential inside the keyring service.
    account: String,

    /// User-facing prompt text for password input.
    ///
    /// Displayed when prompting for credentials through the terminal.
    /// Should be descriptive and indicate which service needs authentication.
    prompt: String,

    /// Location of the pre-1.0 encrypted file, if one was ever written.
    ///
    /// Consulted only during migration and removed once its contents reach the
    /// keyring.
    legacy_file_path: PathBuf,
}

impl Secret {
    /// Creates a credential handle for the given secret.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use kasl::libs::secret::Secret;
    ///
    /// let jira_secret = Secret::new(".jira_secret", "Enter your Jira password");
    /// ```
    pub fn new(secret_name: &str, prompt: &str) -> Self {
        // `.jira_secret` -> `jira`: a readable account name in the OS UI.
        let account = secret_name.trim_start_matches('.').trim_end_matches("_secret").to_string();

        let legacy_file_path = DataStorage::new().get_path(secret_name).unwrap_or_else(|_| PathBuf::from(secret_name));

        Self {
            account,
            prompt: prompt.to_owned(),
            legacy_file_path,
        }
    }

    /// Opens the keyring entry for this credential.
    fn entry(&self) -> Result<Entry> {
        Entry::new(&keyring_service(), &self.account).with_context(|| format!("cannot open keyring entry for '{}'", self.account))
    }

    /// Retrieves the password from the keyring, prompting when absent.
    ///
    /// Order of resolution: keyring, then a legacy AES file (migrated on the
    /// spot), then the user. A value obtained from either of the last two is
    /// written to the keyring, so this is the only time it is asked for.
    ///
    /// # Errors
    ///
    /// Returns an error when the keyring is unavailable, or when there is no
    /// terminal to prompt at and nothing stored to fall back on.
    pub fn get_or_prompt(&self) -> Result<String> {
        if let Some(password) = self.try_get_cached() {
            return Ok(password);
        }

        self.prompt()
    }

    /// Returns a stored password without prompting the user.
    ///
    /// Used by the background daemon, which has no terminal and must never
    /// block. Migrates a legacy file if one is found, so an unattended run
    /// benefits from the migration too.
    ///
    pub fn try_get_cached(&self) -> Option<String> {
        if let Ok(entry) = self.entry()
            && let Ok(password) = entry.get_password()
        {
            return Some(password);
        }

        // Nothing in the keyring - a pre-1.0 install may still have the file.
        self.migrate_legacy_file()
    }

    /// Prompts for the password and stores it in the keyring.
    ///
    /// # Errors
    ///
    /// Fails when stdin is not a terminal, rather than blocking on input that
    /// cannot arrive - this path is reachable from a scheduled `report --send`.
    pub fn prompt(&self) -> Result<String> {
        // Reached whenever a stored credential is missing or stale, including
        // from a scheduled run. Fail loudly rather than hang there.
        crate::libs::prompt::ensure_interactive(&format!(
            "{} - but there is no terminal to ask; run `kasl setup` interactively first",
            self.prompt
        ))?;

        let password = Password::with_theme(&ColorfulTheme::default()).with_prompt(&self.prompt).interact()?;

        self.store(&password)?;

        Ok(password)
    }

    /// Writes the password into the OS keyring.
    pub fn store(&self, password: &str) -> Result<()> {
        self.entry()?
            .set_password(password)
            .with_context(|| format!("cannot store credential '{}' in the OS keyring", self.account))
    }

    /// Removes the credential from the keyring, if present.
    ///
    /// Absence is not an error: the goal is that nothing remains afterwards.
    pub fn delete(&self) -> Result<()> {
        match self.entry()?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(e).with_context(|| format!("cannot remove credential '{}' from the OS keyring", self.account)),
        }
    }

    /// Moves a pre-1.0 AES file into the keyring and deletes it.
    ///
    /// Returns the recovered password, or `None` when there is no readable
    /// legacy file. A file that cannot be decrypted is left untouched: it may
    /// have been written by a build with different key material, and destroying
    /// it would remove the user's only chance of recovering it by other means.
    fn migrate_legacy_file(&self) -> Option<String> {
        let password = self.decrypt_legacy_file().ok()?;

        // Keep the file if the keyring rejects the value, so nothing is lost.
        if self.store(&password).is_err() {
            return Some(password);
        }

        let _ = fs::remove_file(&self.legacy_file_path);

        Some(password)
    }

    /// Decrypts the legacy AES-256-CBC credential file.
    fn decrypt_legacy_file(&self) -> Result<String> {
        let mut file = fs::File::open(&self.legacy_file_path)?;
        let mut encoded = String::new();
        file.read_to_string(&mut encoded)?;

        let ciphertext = BASE64_STANDARD.decode(encoded.trim())?;
        let cipher = Aes256CbcDec::new_from_slices(APP_METADATA_ENCRYPTION_KEY, APP_METADATA_ENCRYPTION_IV)?;
        let plaintext = cipher
            .decrypt_padded_vec::<Pkcs7>(&ciphertext)
            .map_err(|e| anyhow::anyhow!("failed to decrypt stored secret: {e}"))?;

        Ok(String::from_utf8(plaintext)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn a_data_directory_under_temp_keeps_its_secrets_apart() {
        let temp = Path::new("/tmp");
        let scratch = service_for(Path::new("/tmp/.tmpA1b2/lacodda/kasl"), temp);
        assert_ne!(scratch, KEYRING_SERVICE, "a throwaway data directory must not reach the real credentials");
        assert!(scratch.starts_with("lacodda.kasl.scratch-"), "{scratch}");
        assert_eq!(
            scratch,
            service_for(Path::new("/tmp/.tmpA1b2/lacodda/kasl"), temp),
            "the same directory, the same service"
        );
        assert_ne!(
            scratch,
            service_for(Path::new("/tmp/.tmpZ9y8/lacodda/kasl"), temp),
            "two sandboxes do not share secrets"
        );
    }

    #[test]
    fn a_real_installation_keeps_the_service_it_always_had() {
        assert_eq!(service_for(Path::new("/home/me/.local/share/lacodda/kasl"), Path::new("/tmp")), KEYRING_SERVICE);
        assert_eq!(
            service_for(
                Path::new(r"C:\Users\me\AppData\Local\lacodda\kasl"),
                Path::new(r"C:\Users\me\AppData\Local\Temp")
            ),
            KEYRING_SERVICE,
            "LOCALAPPDATA holds Temp, not the other way round"
        );
    }
}
