# ADR 0001: Credentials live in the OS keyring

- Status: accepted
- Date: 2026-08-08

## Context

kasl authenticates against Jira and a corporate reporting API, so it must keep two passwords between runs. Until 1.0 they were stored as AES-256-CBC files under the data directory, encrypted with a key compiled into the binary by `build.rs` from `ENCRYPTION_KEY`/`ENCRYPTION_IV`.

That key had to come from somewhere. Release builds set neither variable, so every published binary fell back to a deterministic default derived from the binary name - a value anyone can reconstruct from the public source. The ciphertext was therefore obfuscation, not encryption: possession of a `.jira_secret` file plus the repository is enough to recover the password.

The scheme had a second problem independent of the first. Because the key is baked in, it must stay byte-identical forever or previously stored credentials become unreadable. That rules out rotation, and it means a build with a custom `.env` produces a binary that cannot read files written by the official one.

## Decision

Store credentials in the operating system keyring through the `keyring` crate: Credential Manager on Windows, Keychain on macOS, Secret Service on Linux. The service name is `lacodda.kasl`; the account name is derived from the old file name (`.jira_secret` becomes `jira`).

Legacy AES files are migrated transparently on first use: a lookup that misses the keyring decrypts the file with the old compiled-in key, stores the value, and deletes the file. `build.rs` still emits those key constants, now fixed to the documented defaults rather than read from the environment, because their only remaining job is reading what earlier builds wrote.

A file that fails to decrypt is left on disk rather than deleted, and the user is prompted instead - that file may have been written by a differently-keyed build and is the user's only copy.

## Consequences

- No key management: access is guarded by the user's OS login session.
- Platform-specific behaviour; CI must cover all three systems, and headless Linux needs a Secret Service provider (or the credential prompt fails, which is preferable to hanging).
- Credentials no longer travel with a copy of the data directory. Re-entered per machine, by design.
- `ENCRYPTION_KEY`/`ENCRYPTION_IV` are no longer read at build time; the `.env` entries and the hub's build-secrets note are obsolete.
- The migration path, and the AES dependencies it needs, can be removed once installs predating 1.0 are gone.
