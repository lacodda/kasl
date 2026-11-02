//! What an inbox issue's key may be, and how it is written where a key
//! cannot go as it is.
//!
//! A key comes from outside - Jira, or a plugin source - and travels far:
//! into the database, onto command lines (`kasl inbox take KEY`, the
//! courier a toast button runs), into the mailbox's tab-separated lines, and
//! into the file name of a toast button's shortcut. One rule for all of
//! them, checked where a key enters: a source that lists a key outside it is
//! refused whole, rather than half-working - a key whose toast has no
//! buttons, or that no shell can type unquoted.
//!
//! The rule admits what the trackers use: Jira's `PROJ-12`, GitLab's full
//! references `group/app#14` and `group/app!31`.

use anyhow::{Result, bail};

/// The longest key kasl keeps: a GitLab reference several groups deep fits
/// with room to spare; anything longer is not a key.
pub const LONGEST: usize = 200;

/// The punctuation a key may hold besides ASCII letters and digits.
///
/// Nothing a command line, a tab-separated line or a URI reads as structure
/// on its own: no whitespace, no quotes, no `%`, no backslash, no `;`, `&`,
/// `|`, `<`, `>`. `#` and `/` are here for GitLab and are harmless in a key -
/// the one place they would mean something, a shortcut's file name, gets the
/// key written by [`file_stem`].
const PUNCTUATION: &[char] = &['-', '_', '.', '/', '#', '!', ':', '@', '+', '~'];

/// Why `key` cannot be an inbox issue's key, or nothing when it can.
pub fn check(key: &str) -> Result<()> {
    if key.is_empty() || key.len() > LONGEST {
        bail!("an issue key has to be 1 to {LONGEST} characters long: {key:?}");
    }
    if let Some(bad) = key.chars().find(|c| !(c.is_ascii_alphanumeric() || PUNCTUATION.contains(c))) {
        bail!(
            "an issue key is letters, digits and {} only; {key:?} has {bad:?}",
            PUNCTUATION.iter().collect::<String>()
        );
    }
    Ok(())
}

/// The key as a file name's stem: letters, digits and `-` as they are, every
/// other character as `_` and its two hex digits.
///
/// Reversible, so two keys never share a file; and made of the characters a
/// `file:` URI takes verbatim, which is how a toast button reaches it - `#`
/// would start the URI's fragment, and a percent-encoded file name does not
/// launch at all (measured on the shortcuts).
pub fn file_stem(key: &str) -> String {
    let mut stem = String::with_capacity(key.len());
    for c in key.chars() {
        if c.is_ascii_alphanumeric() || c == '-' {
            stem.push(c);
        } else {
            let mut buffer = [0u8; 4];
            for byte in c.encode_utf8(&mut buffer).bytes() {
                stem.push_str(&format!("_{byte:02X}"));
            }
        }
    }
    stem
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_trackers_keys_are_keys() {
        for key in ["PROJ-12", "ABC_DEF-7", "group/app#14", "group/sub/app!31", "me/my.project#2", "gh:owner/repo#5"] {
            assert!(check(key).is_ok(), "{key}");
        }
    }

    #[test]
    fn what_a_command_line_or_a_line_would_read_as_structure_is_refused() {
        for key in [
            "",
            "KA 1",
            "KA-1\"x",
            "KA-1;calc",
            "KA-1&calc",
            "..\\..\\evil",
            "KA-1\ttake",
            "KA-1\nx",
            "50%",
            "ключ-1",
        ] {
            assert!(check(key).is_err(), "{key:?}");
        }
        assert!(check(&"A".repeat(LONGEST + 1)).is_err());
        assert!(check(&"A".repeat(LONGEST)).is_ok());
    }

    #[test]
    fn a_file_stem_keeps_a_jira_key_as_it_is() {
        assert_eq!(file_stem("PROJ-12"), "PROJ-12");
    }

    #[test]
    fn a_file_stem_writes_everything_else_out() {
        assert_eq!(file_stem("group/app#14"), "group_2Fapp_2314");
        assert_eq!(file_stem("group/app!31"), "group_2Fapp_2131");
        assert_eq!(file_stem("../../evil"), "_2E_2E_2F_2E_2E_2Fevil", "no separator survives to climb out");
    }

    #[test]
    fn two_keys_never_share_a_file() {
        // `_` itself is written out, so a key that already looks encoded
        // cannot meet the key it looks like.
        assert_ne!(file_stem("a_2Fb"), file_stem("a/b"));
        assert_ne!(file_stem("ABC_DEF-7"), file_stem("ABC/DEF-7"));
    }
}
