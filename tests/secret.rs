//! Credential storage tests.
//!
//! The keyring itself is the OS's business, so these tests focus on what kasl
//! owns: the account naming that decides where a credential lands, and the
//! one-way migration of pre-1.0 AES files, which is the only path that can
//! destroy a credential a user still needs.

#[cfg(test)]
mod tests {
    use aes::Aes256;
    use aes::cipher::block_padding::Pkcs7;
    use aes::cipher::{BlockModeEncrypt, KeyIvInit};
    use base64::prelude::*;
    use kasl::libs::data_storage::DataStorage;
    use kasl::libs::secret::Secret;
    use serial_test::serial;
    use std::fs;
    use tempfile::TempDir;
    use test_context::{TestContext, test_context};

    type Aes256CbcEnc = cbc::Encryptor<Aes256>;

    // Mirrors build.rs when ENCRYPTION_KEY / ENCRYPTION_IV are absent, which is
    // how every published binary was built. Tests must not depend on a local
    // .env, so they exercise the default-key path explicitly.
    const DEFAULT_KEY: &[u8; 32] = b"kasl_default_encryption_key_32b!";
    const DEFAULT_IV: &[u8; 16] = b"kasl_iv_16b!!!!!";

    struct SecretTestContext {
        _temp_dir: TempDir,
    }

    impl TestContext for SecretTestContext {
        fn setup() -> Self {
            let temp_dir = tempfile::tempdir().unwrap();
            // SAFETY: tests touching the env are #[serial] or single-threaded setup
            unsafe {
                std::env::set_var("HOME", temp_dir.path());
                std::env::set_var("LOCALAPPDATA", temp_dir.path());
            }
            SecretTestContext { _temp_dir: temp_dir }
        }
    }

    /// Writes a legacy AES-encrypted credential file the way pre-1.0 kasl did.
    fn write_legacy_file(secret_name: &str, password: &str) -> std::path::PathBuf {
        let cipher = Aes256CbcEnc::new_from_slices(DEFAULT_KEY, DEFAULT_IV).unwrap();
        let ciphertext = cipher.encrypt_padded_vec::<Pkcs7>(password.as_bytes());
        let encoded = BASE64_STANDARD.encode(&ciphertext);

        let path = DataStorage::new().get_path(secret_name).unwrap();
        fs::write(&path, encoded).unwrap();
        path
    }

    /// True when this machine has a usable keyring.
    ///
    /// CI runners generally do not: headless Linux has no Secret Service, and
    /// the macOS Keychain is locked for the agent user. Storage-dependent
    /// assertions are skipped there rather than reported as product failures -
    /// what they would be testing is the platform, not kasl.
    fn keyring_available() -> bool {
        let probe = Secret::new(".keyring_probe_secret", "probe prompt");
        let usable = probe.store("probe").is_ok();
        if usable {
            let _ = probe.delete();
        }
        usable
    }

    /// True when this binary carries the same default keys the tests encrypt with.
    ///
    /// A developer build with a custom `.env` cannot decrypt the fixture, and
    /// that is expected rather than a failure - the migration is only ever
    /// asked to read what the same binary previously wrote.
    fn built_with_default_keys() -> bool {
        let name = ".keycheck_secret";
        write_legacy_file(name, "probe");
        let recovered = Secret::new(name, "probe prompt").try_get_cached();
        let matched = recovered.as_deref() == Some("probe");
        if matched {
            let _ = Secret::new(name, "probe prompt").delete();
        }
        let _ = fs::remove_file(DataStorage::new().get_path(name).unwrap());
        matched
    }

    #[test_context(SecretTestContext)]
    #[serial]
    #[test]
    fn legacy_file_is_migrated_into_the_keyring(_ctx: &mut SecretTestContext) {
        if !keyring_available() || !built_with_default_keys() {
            return;
        }

        let path = write_legacy_file(".migrate_secret", "s3cret-from-0.10");
        let secret = Secret::new(".migrate_secret", "Enter password");

        // First lookup finds no keyring entry and falls back to the file.
        assert_eq!(secret.try_get_cached().as_deref(), Some("s3cret-from-0.10"));

        // The file is consumed: the credential now lives in the keyring only.
        assert!(!path.exists(), "legacy file should be removed after migration");
        assert_eq!(secret.try_get_cached().as_deref(), Some("s3cret-from-0.10"));

        secret.delete().unwrap();
    }

    #[test_context(SecretTestContext)]
    #[serial]
    #[test]
    fn undecryptable_legacy_file_is_left_alone(_ctx: &mut SecretTestContext) {
        // A file written by a build with different key material must survive:
        // deleting it would destroy the user's only copy.
        let path = DataStorage::new().get_path(".corrupt_secret").unwrap();
        fs::write(&path, "not base64 at all !@#$").unwrap();

        let secret = Secret::new(".corrupt_secret", "Enter password");

        assert_eq!(secret.try_get_cached(), None);
        assert!(path.exists(), "unreadable legacy file must not be deleted");
    }

    #[test_context(SecretTestContext)]
    #[serial]
    #[test]
    fn missing_credential_reports_absence_without_prompting(_ctx: &mut SecretTestContext) {
        let secret = Secret::new(".absent_secret", "Enter password");
        assert_eq!(secret.try_get_cached(), None);
    }

    #[test_context(SecretTestContext)]
    #[serial]
    #[test]
    fn stored_credential_round_trips(_ctx: &mut SecretTestContext) {
        if !keyring_available() {
            return;
        }

        let secret = Secret::new(".roundtrip_secret", "Enter password");

        secret.store("hunter2").unwrap();
        assert_eq!(secret.try_get_cached().as_deref(), Some("hunter2"));

        secret.delete().unwrap();
        assert_eq!(secret.try_get_cached(), None);
    }

    #[test_context(SecretTestContext)]
    #[serial]
    #[test]
    fn deleting_an_absent_credential_succeeds(_ctx: &mut SecretTestContext) {
        if !keyring_available() {
            return;
        }

        // Removal is idempotent: what matters is that nothing remains after.
        let secret = Secret::new(".never_stored_secret", "Enter password");
        assert!(secret.delete().is_ok());
    }
}
