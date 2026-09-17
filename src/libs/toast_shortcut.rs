//! Windows only: the shortcut a toast button points at.
//!
//! A toast button can only ask the shell to launch a URI, and no argument
//! survives that trip. So the ask has to be baked into the thing being
//! launched: one shortcut per (action, issue), whose command line already
//! says which action and which issue.
//!
//! ## Why a shortcut, and why COM
//!
//! A `.cmd` would be plain text to write, but the shell opens it in a console
//! window - a flash on screen for every button press. A `.vbs` runs silently,
//! but VBScript is a removable feature on Windows 11 and announced for
//! removal, so it is a choice that would have to be made again.
//!
//! A `.lnk` is a first-class shell object and takes `SW_SHOWMINNOACTIVE`, so
//! nothing lands in front of the user. It cannot be written by hand: a
//! header-plus-`LinkInfo` shortcut is structurally valid and 322 bytes, and
//! the shell refuses it, because it wants a `LinkTargetIDList` - the PIDL
//! chain only the shell can build. `IShellLinkW` builds it; `winapi` already
//! ships the interface.

use crate::libs::toast_action::{ToastAction, shortcut_dir, validate_issue_key};
use anyhow::{Context, Result, bail};
use std::ffi::OsStr;
use std::os::windows::ffi::OsStrExt;
use std::path::Path;
use tracing::debug;
use winapi::Interface;
use winapi::shared::guiddef::CLSID;
use winapi::shared::wtypesbase::CLSCTX_INPROC_SERVER;
use winapi::um::combaseapi::{CoCreateInstance, CoInitializeEx, CoUninitialize};
use winapi::um::objbase::COINIT_APARTMENTTHREADED;
use winapi::um::objidl::IPersistFile;
use winapi::um::shobjidl_core::IShellLinkW;
use winapi::um::winuser::SW_SHOWMINNOACTIVE;

/// `CLSID_ShellLink`, the shell's shortcut factory.
///
/// Spelled out here because `winapi` declares the `IShellLinkW` interface but
/// not this class id. It is a documented, frozen Windows constant, so writing
/// it is quoting a number, not reimplementing anything.
const CLSID_SHELL_LINK: CLSID = CLSID {
    Data1: 0x0002_1401,
    Data2: 0x0000,
    Data3: 0x0000,
    Data4: [0xC0, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x46],
};

/// Writes (or refreshes) the shortcut for one action on one issue, and
/// returns the `file:` URI a toast button should launch.
///
/// The shortcut runs `kasl toast-action <action> <key>`, which posts the ask
/// to the mailbox and exits. Every call rewrites the file, so a shortcut left
/// by an older build cannot keep pointing at a stale executable path after a
/// self-update.
pub fn ensure(action: ToastAction, issue_key: &str) -> Result<String> {
    validate_issue_key(issue_key)?;

    let exe = std::env::current_exe().context("cannot find the running kasl executable")?;
    let dir = shortcut_dir()?;
    let path = dir.join(format!("{}-{}.lnk", action.as_str(), issue_key));

    // The key is validated above, so it needs no quoting; the executable path
    // is quoted because it routinely contains spaces.
    let arguments = format!("toast-action {} {}", action.as_str(), issue_key);

    write_shortcut(&path, &exe, &arguments).with_context(|| format!("cannot write the toast shortcut {}", path.display()))?;

    debug!("Toast shortcut ready: {}", path.display());
    Ok(file_uri(&path))
}

/// Deletes the shortcuts of one issue, whatever action they carry.
///
/// Called once the issue is decided: a shortcut whose button is gone is only
/// a file that still runs. Failures are ignored - a leftover shortcut is
/// harmless, and the next `ensure` overwrites it.
pub fn forget(issue_key: &str) {
    let Ok(dir) = shortcut_dir() else { return };
    for action in ToastAction::ALL {
        let path = dir.join(format!("{}-{}.lnk", action.as_str(), issue_key));
        let _ = std::fs::remove_file(path);
    }
}

/// A `file:` URI for a local path, in the form the shell accepts.
///
/// Measured, not assumed: `file:///C:/dir/take-KA-1.lnk` launches, while the
/// same path with a percent-encoded space fails outright. Keys are validated
/// to the characters Jira uses, so nothing here needs escaping; the data
/// directory is the only part that could hold a space, and the shell takes it
/// verbatim.
fn file_uri(path: &Path) -> String {
    format!("file:///{}", path.display().to_string().replace('\\', "/"))
}

/// Null-terminated UTF-16, as every `IShellLinkW` setter wants.
fn wide(value: &str) -> Vec<u16> {
    OsStr::new(value).encode_wide().chain(std::iter::once(0)).collect()
}

/// Creates a shell link at `path` running `target` with `arguments`.
///
/// COM is initialized per call rather than once for the process: shortcuts
/// are written from the poller thread, and a library function that leaves an
/// apartment behind on a thread it does not own would be a side effect the
/// caller cannot see.
fn write_shortcut(path: &Path, target: &Path, arguments: &str) -> Result<()> {
    // SAFETY: the documented COM initialization sequence. A null reserved
    // pointer with an apartment-threaded flag is the documented call, and the
    // matching uninitialize below runs on this same thread.
    let init = unsafe { CoInitializeEx(std::ptr::null_mut(), COINIT_APARTMENTTHREADED) };
    // S_FALSE means the thread was already in an apartment, which works for
    // us, but then the matching uninitialize is the other caller's to make.
    let ours = init == 0;

    let result = write_link(path, target, arguments);

    if ours {
        // SAFETY: balances the successful CoInitializeEx above, same thread.
        unsafe { CoUninitialize() };
    }
    result
}

/// The COM body: create the link, set its three fields, save it.
fn write_link(path: &Path, target: &Path, arguments: &str) -> Result<()> {
    let mut raw: *mut IShellLinkW = std::ptr::null_mut();
    // SAFETY: CLSID and IID are compile-time constants of the right types,
    // and `raw` is a live local the call writes a pointer into.
    let hr = unsafe {
        CoCreateInstance(
            &CLSID_SHELL_LINK,
            std::ptr::null_mut(),
            CLSCTX_INPROC_SERVER,
            &IShellLinkW::uuidof(),
            &mut raw as *mut *mut IShellLinkW as *mut *mut _,
        )
    };
    if hr < 0 || raw.is_null() {
        bail!("CoCreateInstance for ShellLink failed: 0x{hr:08X}");
    }

    // The guard releases the pointer on every way out, including each early
    // return inside `fill_and_save`.
    let link = ComPtr(raw);
    fill_and_save(link.get(), path, target, arguments)
}

/// Owns one COM interface pointer and releases it on drop.
///
/// A guard rather than a `Release` before each `bail!`: the setters below
/// have five failure paths, and a hand-written release on each is a leak
/// waiting for the sixth to be added.
struct ComPtr(*mut IShellLinkW);

impl ComPtr {
    fn get(&self) -> &IShellLinkW {
        // SAFETY: non-null is checked before the guard is constructed, and
        // the pointer stays valid until this guard drops.
        unsafe { &*self.0 }
    }
}

impl Drop for ComPtr {
    fn drop(&mut self) {
        // SAFETY: the pointer came from CoCreateInstance with a reference we
        // own, and this runs exactly once.
        unsafe { (*self.0).Release() };
    }
}

fn fill_and_save(link: &IShellLinkW, path: &Path, target: &Path, arguments: &str) -> Result<()> {
    let target_w = wide(&target.display().to_string());
    // SAFETY: a null-terminated UTF-16 buffer that outlives the call.
    let hr = unsafe { link.SetPath(target_w.as_ptr()) };
    if hr < 0 {
        bail!("IShellLink::SetPath failed: 0x{hr:08X}");
    }

    let args_w = wide(arguments);
    // SAFETY: as above.
    let hr = unsafe { link.SetArguments(args_w.as_ptr()) };
    if hr < 0 {
        bail!("IShellLink::SetArguments failed: 0x{hr:08X}");
    }

    // The courier is a console program that prints nothing and exits at once.
    // Minimized-and-not-activated keeps its window off the screen and out of
    // the way of whatever the user is doing.
    // SAFETY: an integer argument to a method on a live interface.
    let hr = unsafe { link.SetShowCmd(SW_SHOWMINNOACTIVE) };
    if hr < 0 {
        bail!("IShellLink::SetShowCmd failed: 0x{hr:08X}");
    }

    let mut persist: *mut IPersistFile = std::ptr::null_mut();
    // SAFETY: the IID is a compile-time constant and `persist` is a live
    // local the call writes into.
    let hr = unsafe { link.QueryInterface(&IPersistFile::uuidof(), &mut persist as *mut *mut IPersistFile as *mut *mut _) };
    if hr < 0 || persist.is_null() {
        bail!("QueryInterface for IPersistFile failed: 0x{hr:08X}");
    }

    let path_w = wide(&path.display().to_string());
    // SAFETY: a live IPersistFile and a null-terminated path buffer; the
    // release below runs whatever Save returns.
    let hr = unsafe {
        let saved = (*persist).Save(path_w.as_ptr(), 1);
        (*persist).Release();
        saved
    };
    if hr < 0 {
        bail!("IPersistFile::Save failed: 0x{hr:08X}");
    }
    Ok(())
}
