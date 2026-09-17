//! Windows only: the shortcut behind a toast button.
//!
//! Every assertion here is about the thing measurement showed matters: a
//! hand-written shortcut is refused by the shell, so what is written must be
//! a real shell link, and the URI handed to the button must be the form the
//! shell actually launches.
#![cfg(windows)]

use kasl::libs::toast_action::{SHORTCUT_DIR, ToastAction};
use kasl::libs::toast_shortcut;
use serial_test::serial;
use tempfile::TempDir;
use test_context::{TestContext, test_context};

struct ShortcutContext {
    temp_dir: TempDir,
}

impl TestContext for ShortcutContext {
    fn setup() -> Self {
        let temp_dir = tempfile::tempdir().unwrap();
        // SAFETY: every test here is #[serial].
        unsafe {
            std::env::set_var("LOCALAPPDATA", temp_dir.path());
        }
        ShortcutContext { temp_dir }
    }
}

impl ShortcutContext {
    /// Where the shortcuts land under the redirected data directory.
    fn dir(&self) -> std::path::PathBuf {
        // Mirrors DataStorage: {LOCALAPPDATA}/{owner}/{app}/{SHORTCUT_DIR}.
        let mut found = None;
        for owner in std::fs::read_dir(self.temp_dir.path()).unwrap() {
            let owner = owner.unwrap().path();
            if !owner.is_dir() {
                continue;
            }
            for app in std::fs::read_dir(&owner).unwrap() {
                let candidate = app.unwrap().path().join(SHORTCUT_DIR);
                if candidate.is_dir() {
                    found = Some(candidate);
                }
            }
        }
        found.expect("the shortcut directory must exist once a shortcut was written")
    }
}

#[test_context(ShortcutContext)]
#[test]
#[serial]
fn a_shortcut_is_written_for_each_action(ctx: &mut ShortcutContext) {
    for action in ToastAction::ALL {
        let uri = toast_shortcut::ensure(action, "KA-1").unwrap();
        assert!(uri.starts_with("file:///"), "the shell launches a file: URI; got {uri}");
        assert!(
            uri.ends_with(&format!("{}-KA-1.lnk", action.as_str())),
            "the URI must name this action and this issue; got {uri}"
        );
        assert!(
            !uri.contains('\\'),
            "a backslash in the URI is the form the shell refused when measured; got {uri}"
        );
    }

    let dir = ctx.dir();
    for action in ToastAction::ALL {
        let path = dir.join(format!("{}-KA-1.lnk", action.as_str()));
        assert!(path.is_file(), "{} must exist", path.display());
    }
}

#[test_context(ShortcutContext)]
#[test]
#[serial]
fn the_written_file_is_a_real_shell_link(ctx: &mut ShortcutContext) {
    toast_shortcut::ensure(ToastAction::Take, "KA-2").unwrap();
    let bytes = std::fs::read(ctx.dir().join("take-KA-2.lnk")).unwrap();

    // The signature and CLSID of MS-SHLLINK. Checked because a file that is
    // merely present proves nothing: the first attempt at this wrote a
    // structurally valid 322-byte shortcut that the shell would not launch.
    assert_eq!(&bytes[0..4], &[0x4C, 0x00, 0x00, 0x00], "a shell link starts with a 0x4C header size");
    assert_eq!(
        &bytes[4..20],
        &[0x01, 0x14, 0x02, 0x00, 0x00, 0x00, 0x00, 0x00, 0xC0, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x46],
        "the link CLSID must be the shell's"
    );

    let flags = u32::from_le_bytes([bytes[20], bytes[21], bytes[22], bytes[23]]);
    const HAS_LINK_TARGET_ID_LIST: u32 = 0x0000_0001;
    const HAS_ARGUMENTS: u32 = 0x0000_0020;
    assert!(
        flags & HAS_LINK_TARGET_ID_LIST != 0,
        "without a LinkTargetIDList the shell refuses the shortcut - this is the flag a hand-written one lacked"
    );
    assert!(
        flags & HAS_ARGUMENTS != 0,
        "the action and the key ride in the arguments, so they must be present"
    );

    // The whole reason a shortcut was chosen over a .cmd: no window in front
    // of the user when a button is pressed. SW_SHOWNORMAL here would put a
    // console on screen for every press, which is what measurement rejected.
    const SW_SHOWMINNOACTIVE: u32 = 7;
    let show_cmd = u32::from_le_bytes([bytes[0x3C], bytes[0x3D], bytes[0x3E], bytes[0x3F]]);
    assert_eq!(
        show_cmd, SW_SHOWMINNOACTIVE,
        "the courier must run without a window in front of the user; ShowCmd {show_cmd} would flash one"
    );

    // The command line is what carries the ask, since the button cannot.
    //
    // Searched for as raw UTF-16 bytes rather than by decoding the file:
    // the arguments sit at an offset that depends on the target path, so
    // decoding from byte zero lands on the wrong parity half the time. That
    // is exactly how the first version of this test passed here and failed
    // on CI, where the shortcut's prefix is one byte longer.
    for needle in ["toast-action", "take", "KA-2"] {
        assert!(contains_utf16(&bytes, needle), "the shortcut must carry {needle:?} on its command line");
    }
}

/// Whether `haystack` contains `needle` encoded as little-endian UTF-16.
fn contains_utf16(haystack: &[u8], needle: &str) -> bool {
    let wide: Vec<u8> = needle.encode_utf16().flat_map(u16::to_le_bytes).collect();
    haystack.windows(wide.len()).any(|window| window == wide.as_slice())
}

#[test_context(ShortcutContext)]
#[test]
#[serial]
fn writing_a_shortcut_twice_refreshes_it(_ctx: &mut ShortcutContext) {
    // A shortcut left by an older build would point at an executable path a
    // self-update has moved, so every toast rewrites the file.
    let first = toast_shortcut::ensure(ToastAction::Snooze, "KA-3").unwrap();
    let second = toast_shortcut::ensure(ToastAction::Snooze, "KA-3").unwrap();
    assert_eq!(first, second, "the same action on the same issue is the same shortcut");
}

#[test_context(ShortcutContext)]
#[test]
#[serial]
fn a_settled_issue_loses_its_shortcuts(ctx: &mut ShortcutContext) {
    for action in ToastAction::ALL {
        toast_shortcut::ensure(action, "KA-4").unwrap();
    }
    toast_shortcut::ensure(ToastAction::Take, "KA-5").unwrap();

    toast_shortcut::forget("KA-4");

    let dir = ctx.dir();
    for action in ToastAction::ALL {
        let path = dir.join(format!("{}-KA-4.lnk", action.as_str()));
        assert!(!path.exists(), "{} is a file that still runs with nothing pointing at it", path.display());
    }
    assert!(dir.join("take-KA-5.lnk").is_file(), "forgetting one issue must not touch another's buttons");
}

#[test_context(ShortcutContext)]
#[test]
#[serial]
fn a_key_that_is_not_a_key_gets_no_shortcut(_ctx: &mut ShortcutContext) {
    // The guard has to hold here and not only in its own unit test: this is
    // the call that puts a key into a file name and onto a command line.
    for bad in ["KA 1", "..\\..\\evil", "KA-1\" && calc", ""] {
        assert!(toast_shortcut::ensure(ToastAction::Take, bad).is_err(), "{bad:?} must not become a shortcut");
    }
}
