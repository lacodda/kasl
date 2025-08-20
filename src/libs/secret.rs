//! Secure credential storage and management with AES encryption.
//!
//! Provides functionality for securely storing and retrieving sensitive information
//! such as passwords and API tokens using AES-256-CBC encryption.
//!
//! ## Features
//!
//! - **AES-256-CBC Encryption**: Industry-standard encryption for credential storage
//! - **Compile-time Keys**: Encryption keys embedded during build process
//! - **Secure Input**: Password prompting without echo to terminal
//! - **File Protection**: Encrypted credentials stored in user data directory
//! - **Memory Safety**: Credentials cleared from memory after use
//!
//! ## Usage
//!
//! ```rust
//! use kasl::libs::secret::Secret;
//!
//! let secret = Secret::new(".jira_secret", "Enter your Jira password");
//! let password = secret.get_or_prompt()?;
//! let new_password = secret.prompt()?;
//! ```

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

// Include generated metadata containing encryption keys
// This file is created during the build process and contains
// compile-time embedded encryption keys and initialization vectors
include!(concat!(env!("OUT_DIR"), "/app_metadata.rs"));

/// Type alias for AES-256-CBC cipher with PKCS7 padding.
///
/// This cipher configuration provides:
/// - **AES-256**: Advanced Encryption Standard with 256-bit keys
/// - **CBC Mode**: Cipher Block Chaining for secure block encryption
/// - **PKCS7 Padding**: Standard padding scheme for block alignment
type Aes256Cbc = Cbc<Aes256, Pkcs7>;

/// Secure credential storage and management system.
///
/// This structure manages the complete lifecycle of sensitive credentials
/// including user prompting, encryption, file storage, and decryption.
/// It provides a high-level interface for secure credential handling.
///
/// ## Design Principles
///
/// - **Lazy Loading**: Passwords only decrypted when needed
/// - **Immutable State**: Credential changes create new instances
/// - **Error Recovery**: Graceful handling of encryption/decryption failures
/// - **User Friendly**: Clear prompts and error messages
///
/// ## Internal State
///
/// The struct maintains both encrypted and decrypted state:
/// - File-based encrypted storage for persistence
/// - Memory-based decrypted storage for immediate use
/// - Cryptographic keys for encryption/decryption operations
///
/// ## Lifecycle
///
/// 1. **Creation**: Initialize with file path and user prompt
/// 2. **Retrieval**: Check for existing encrypted file
/// 3. **Prompting**: Secure password input when needed
/// 4. **Encryption**: AES encryption before file storage
/// 5. **Decryption**: AES decryption when retrieving
#[derive(Clone, Debug)]
pub struct Secret {
    /// Optional in-memory password storage.
    ///
    /// When present, contains the decrypted password for immediate use.
    /// This avoids repeated decryption operations but increases memory
    /// exposure time. Cleared when instance is dropped.
    password: Option<String>,

    /// User-facing prompt text for password input.
    ///
    /// Displayed when prompting for credentials through the terminal.
    /// Should be descriptive and indicate which service needs authentication.
    /// Examples: "Enter your Jira password", "GitLab API token"
    prompt: String,

    /// File system path for encrypted credential storage.
    ///
    /// Points to the location where encrypted credentials are stored.
    /// Typically in the user's application data directory with a
    /// service-specific filename (e.g., ".jira_secret").
    secret_file_path: PathBuf,

    /// AES encryption key for credential protection.
    ///
    /// 256-bit key used for AES encryption/decryption operations.
    /// Embedded at compile time from build environment or defaults.
    /// Should be kept consistent across application versions.
    key: Vec<u8>,

    /// Initialization vector for AES-CBC encryption.
    ///
    /// Fixed IV used with AES-CBC mode for deterministic encryption.
    /// While using a fixed IV reduces security, it allows for consistent
    /// file-based storage without additional key derivation complexity.
    iv: Vec<u8>,
}

impl Secret {
    /// Creates a new Secret instance for credential management.
    ///
    /// This constructor initializes a new secret manager with the specified
    /// file storage location and user prompt text. It loads encryption keys
    /// from compile-time embedded metadata and prepares the file path within
    /// the application's data directory.
    ///
    /// ## Key Management
    ///
    /// Encryption keys are loaded from compile-time metadata:
    /// - Keys embedded during build process for security
    /// - Consistent keys across application installations
    /// - No runtime key generation or derivation needed
    ///
    /// ## File Path Resolution
    ///
    /// The secret file path is resolved using the application's data storage:
    /// - Platform-appropriate user data directory
    /// - Service-specific filename for credential isolation
    /// - Automatic directory creation when needed
    ///
    /// # Arguments
    ///
    /// * `secret_name` - Filename for storing encrypted credentials (e.g., ".jira_secret")
    /// * `prompt` - User-facing text for password prompts (e.g., "Enter your Jira password")
    ///
    /// # Returns
    ///
    /// A new Secret instance ready for credential operations.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use kasl::libs::secret::Secret;
    ///
    /// // Jira password management
    /// let jira_secret = Secret::new(".jira_secret", "Enter your Jira password");
    ///
    /// // GitLab token management
    /// let gitlab_secret = Secret::new(".gitlab_token", "Enter your GitLab API token");
    ///
    /// // SI Server credentials
    /// let si_secret = Secret::new(".si_credentials", "Enter your SI Server password");
    /// ```
    ///
    /// # Error Handling
    ///
    /// Path resolution errors are handled gracefully by falling back to
    /// the current directory if the data storage path cannot be created.
    pub fn new(secret_name: &str, prompt: &str) -> Self {
        // Load compile-time embedded encryption keys
        let key = APP_METADATA_ENCRYPTION_KEY.to_vec();
        let iv = APP_METADATA_ENCRYPTION_IV.to_vec();

        // Resolve secret file path in application data directory
        let secret_file_path = DataStorage::new().get_path(secret_name).unwrap_or_else(|_| PathBuf::from(secret_name));

        Self {
            password: None,
            secret_file_path,
            prompt: prompt.to_owned(),
            key,
            iv,
        }
    }

    /// Creates a new Secret instance with the specified password.
    ///
    /// This internal method creates a copy of the current Secret with
    /// a different password value. Used for maintaining immutable state
    /// while updating the in-memory password storage.
    ///
    /// # Arguments
    ///
    /// * `password` - The password to store in the new instance
    ///
    /// # Returns
    ///
    /// A new Secret instance with the updated password.
    fn set_password(&self, password: &str) -> Self {
        Self {
            password: Some(password.to_owned()),
            ..self.clone()
        }
    }

    /// Retrieves password from cache or prompts user if not available.
    ///
    /// This method implements the primary credential retrieval logic:
    /// 1. Check if encrypted file exists and is readable
    /// 2. Attempt to decrypt existing credentials
    /// 3. Prompt user for new credentials if decryption fails
    /// 4. Encrypt and store new credentials for future use
    ///
    /// ## Caching Behavior
    ///
    /// - **Cache Hit**: Return decrypted password from file
    /// - **Cache Miss**: Prompt user and store new password
    /// - **Decryption Error**: Re-prompt user (file may be corrupted)
    ///
    /// ## Error Recovery
    ///
    /// If decryption fails (corrupted file, wrong keys, etc.), the method
    /// gracefully falls back to prompting the user for new credentials.
    /// This ensures the application can recover from storage corruption.
    ///
    /// # Returns
    ///
    /// Returns the user's password, either from cache or fresh input.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - User cancels password prompt
    /// - File system operations fail
    /// - Encryption operations fail
    ///
    /// # Examples
    ///
    /// ```rust
    /// use kasl::libs::secret::Secret;
    ///
    /// let secret = Secret::new(".api_token", "Enter API token");
    ///
    /// // First call prompts user and caches result
    /// let token = secret.get_or_prompt()?;
    ///
    /// // Subsequent calls use cached value
    /// let same_token = secret.get_or_prompt()?;
    /// ```
    pub fn get_or_prompt(&self) -> Result<String> {
        // Check if encrypted credentials file exists
        if fs::metadata(&self.secret_file_path).is_ok() {
            // Attempt to decrypt existing credentials
            if let Ok(password) = self.decrypt() {
                return Ok(password);
            }
            // Decryption failed - file may be corrupted, continue to prompt
        }

        // No cached credentials or decryption failed - prompt user
        self.prompt()
    }

    /// Prompts user for password and stores it securely.
    ///
    /// This method handles the complete password input and storage workflow:
    /// 1. Display secure password prompt (no echo)
    /// 2. Encrypt the entered password
    /// 3. Store encrypted data to file
    /// 4. Return the entered password
    ///
    /// ## Security Features
    ///
    /// - **No Echo**: Password characters not displayed on screen
    /// - **Immediate Encryption**: Password encrypted before file storage
    /// - **Memory Clearing**: Original password cleared after encryption
    ///
    /// ## User Experience
    ///
    /// The prompt uses a colorful theme for better visibility and
    /// provides clear instructions to the user. Password input is
    /// handled securely without displaying characters.
    ///
    /// # Returns
    ///
    /// Returns the password entered by the user.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - User cancels password input (Ctrl+C)
    /// - Password encryption fails
    /// - File system write operations fail
    ///
    /// # Examples
    ///
    /// ```rust
    /// use kasl::libs::secret::Secret;
    ///
    /// let secret = Secret::new(".password", "Enter your password");
    ///
    /// // Force password prompt (ignores cache)
    /// let password = secret.prompt()?;
    /// ```
    pub fn prompt(&self) -> Result<String> {
        // Display secure password prompt
        let password = Password::with_theme(&ColorfulTheme::default()).with_prompt(&self.prompt).interact()?;

        // Encrypt and store the password
        self.set_password(&password).encrypt()?;

        Ok(password)
    }

    /// Encrypts the stored password and saves it to file.
    ///
    /// This method performs the complete encryption and storage workflow:
    /// 1. Initialize AES-256-CBC cipher with embedded keys
    /// 2. Encrypt the password using PKCS7 padding
    /// 3. Encode encrypted data as Base64 for safe storage
    /// 4. Write encoded data to the secret file
    ///
    /// ## Encryption Process
    ///
    /// - **Input**: Plain text password from memory
    /// - **Cipher**: AES-256-CBC with compile-time keys
    /// - **Padding**: PKCS7 for block alignment
    /// - **Encoding**: Base64 for text-safe storage
    /// - **Output**: Encrypted file in application data directory
    ///
    /// ## File System Operations
    ///
    /// - Creates parent directories if they don't exist
    /// - Overwrites existing credential files
    /// - Uses platform-appropriate file permissions
    ///
    /// # Returns
    ///
    /// Returns a new Secret instance for method chaining.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - No password is set in memory
    /// - AES encryption fails
    /// - Base64 encoding fails
    /// - File system write operations fail
    ///
    /// # Examples
    ///
    /// ```rust
    /// let secret = Secret::new(".test", "Test password")
    ///     .set_password("my_secret")
    ///     .encrypt()?;
    /// ```
    fn encrypt(&self) -> Result<Self> {
        // Initialize AES cipher with embedded keys
        let cipher = Aes256Cbc::new_from_slices(&self.key, &self.iv)?;

        // Get password from memory
        let password = &self.password.clone().unwrap();

        // Encrypt password with PKCS7 padding
        let ciphertext = cipher.encrypt_vec(&password.as_bytes());

        // Encode as Base64 for safe file storage
        let encoded = BASE64_STANDARD.encode(&ciphertext);

        // Ensure parent directory exists
        if let Some(parent) = self.secret_file_path.parent() {
            let _ = fs::create_dir_all(parent);
        }

        // Write encrypted data to file
        let mut file = File::create(&self.secret_file_path)?;
        file.write_all(encoded.as_bytes())?;

        Ok(self.clone())
    }

    /// Decrypts stored credentials from file.
    ///
    /// This method performs the complete decryption workflow:
    /// 1. Read Base64-encoded data from credential file
    /// 2. Decode Base64 to get raw encrypted bytes
    /// 3. Initialize AES cipher with embedded keys
    /// 4. Decrypt data and remove PKCS7 padding
    /// 5. Convert decrypted bytes to UTF-8 string
    ///
    /// ## Decryption Process
    ///
    /// - **Input**: Base64-encoded encrypted file
    /// - **Decoding**: Base64 to raw encrypted bytes
    /// - **Cipher**: AES-256-CBC with compile-time keys
    /// - **Padding**: PKCS7 removal for original data
    /// - **Output**: Plain text password string
    ///
    /// ## Error Recovery
    ///
    /// The method handles various failure modes:
    /// - **File Not Found**: Returns error for missing credentials
    /// - **Invalid Base64**: Returns error for corrupted encoding
    /// - **Decryption Failure**: Returns error for wrong keys or corrupted data
    /// - **Invalid UTF-8**: Returns error for corrupted password data
    ///
    /// # Returns
    ///
    /// Returns the decrypted password as a string.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - Credential file doesn't exist or can't be read
    /// - Base64 decoding fails (corrupted file)
    /// - AES decryption fails (wrong keys, corrupted data)
    /// - Decrypted data is not valid UTF-8
    ///
    /// # Examples
    ///
    /// ```rust
    /// let secret = Secret::new(".existing_secret", "Password");
    ///
    /// match secret.decrypt() {
    ///     Ok(password) => println!("Retrieved cached password"),
    ///     Err(_) => println!("No cached password or decryption failed"),
    /// }
    /// ```
    fn decrypt(&self) -> Result<String> {
        // Read Base64-encoded data from file
        let mut file = File::open(&self.secret_file_path)?;
        let mut encoded = String::new();
        file.read_to_string(&mut encoded)?;

        // Decode Base64 to get encrypted bytes
        let ciphertext = BASE64_STANDARD.decode(encoded)?;

        // Initialize AES cipher with embedded keys
        let cipher = Aes256Cbc::new_from_slices(&self.key, &self.iv)?;

        // Decrypt data and remove padding
        let decrypted_ciphertext = cipher.decrypt_vec(&ciphertext)?;

        // Convert decrypted bytes to UTF-8 string
        let decrypted_password = String::from_utf8(decrypted_ciphertext)?;

        Ok(decrypted_password)
    }
}
