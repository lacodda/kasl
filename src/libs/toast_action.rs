//! Toast buttons: what a button asks for, and how the ask reaches the daemon.
//!
//! A toast on the desktop is the only part of kasl the user sees without
//! opening a terminal, so the decisions it offers should be settled there.
//! This module holds the three decisions a toast can carry, the mailbox a
//! click drops them into, and the daemon side that performs them.
//!
//! ## Why a mailbox and not a direct write
//!
//! On Windows a toast button can only ask the shell to launch a URI
//! (`activationType="protocol"`): no argument survives the trip, measured
//! three ways in `toast_activation.rs`. So the button points at a shortcut
//! the daemon wrote, one per (action, issue), with the ask baked inside it.
//! The shortcut runs `kasl toast-action` - a courier that appends one line to
//! this mailbox and exits. The daemon, which owns the database and is already
//! running, picks the line up and acts.
//!
//! The courier does not touch the database itself. It would be a second
//! writer racing the poller, and would have to raise the whole config and
//! keyring stack to change one column.
//!
//! ```rust,no_run
//! # fn main() -> anyhow::Result<()> {
//! use kasl::libs::toast_action::{Mailbox, ToastAction, ToastRequest};
//!
//! let mailbox = Mailbox::open()?;
//! mailbox.post(&ToastRequest::new(ToastAction::Take, "PROJ-1"))?;
//! for request in mailbox.collect()? {
//!     println!("{} {}", request.action.as_str(), request.issue_key);
//! }
//! # Ok(())
//! # }
//! ```

use crate::libs::data_storage::DataStorage;
use anyhow::{Result, bail};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::PathBuf;

/// The file clicks are appended to, and the daemon drains.
const MAILBOX_FILE: &str = "toast-actions.jsonl";

/// Where the per-action shortcuts live, one subdirectory of the data dir.
pub const SHORTCUT_DIR: &str = "toast-buttons";

/// The decisions a toast button can carry.
///
/// Deliberately the same three the triage picker offers, and no more. A toast
/// is read in a glance between two other things: a fourth button would make
/// it a menu, and a menu asks to be studied. Everything else stays in
/// `kasl inbox triage`, where there is room to think.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToastAction {
    /// Import the issue into local tasks.
    Take,
    /// Put the issue to sleep for the configured window.
    Snooze,
    /// Hide the issue from the list.
    Dismiss,
}

impl ToastAction {
    /// The wire name, used in the mailbox line and the shortcut file name.
    pub fn as_str(&self) -> &'static str {
        match self {
            ToastAction::Take => "take",
            ToastAction::Snooze => "snooze",
            ToastAction::Dismiss => "dismiss",
        }
    }

    /// The label the button wears.
    pub fn label(&self) -> &'static str {
        match self {
            ToastAction::Take => "Take",
            ToastAction::Snooze => "Snooze",
            ToastAction::Dismiss => "Dismiss",
        }
    }

    /// Parses a wire name back into an action.
    pub fn parse(name: &str) -> Option<Self> {
        match name {
            "take" => Some(ToastAction::Take),
            "snooze" => Some(ToastAction::Snooze),
            "dismiss" => Some(ToastAction::Dismiss),
            _ => None,
        }
    }

    /// The three, in the order they appear on a toast.
    ///
    /// `Take` leads because it is the one that moves work forward; `Dismiss`
    /// is last because it is the only one that cannot be undone by waiting.
    pub const ALL: [ToastAction; 3] = [ToastAction::Take, ToastAction::Snooze, ToastAction::Dismiss];
}

/// One click, on its way to the daemon.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToastRequest {
    pub action: ToastAction,
    pub issue_key: String,
}

impl ToastRequest {
    pub fn new(action: ToastAction, issue_key: &str) -> Self {
        Self {
            action,
            issue_key: issue_key.to_string(),
        }
    }

    /// One line of the mailbox: action, a tab, the key.
    ///
    /// Tab-separated rather than JSON because an issue key cannot contain a
    /// tab and the courier should not carry a serializer to write one line.
    /// A malformed line is skipped rather than failing the drain, so one bad
    /// line cannot wedge every click behind it.
    fn to_line(&self) -> String {
        format!("{}\t{}", self.action.as_str(), self.issue_key)
    }

    fn from_line(line: &str) -> Option<Self> {
        let (action, key) = line.split_once('\t')?;
        let key = key.trim();
        if key.is_empty() {
            return None;
        }
        Some(Self {
            action: ToastAction::parse(action.trim())?,
            issue_key: key.to_string(),
        })
    }
}

/// The file the courier appends to and the daemon drains.
pub struct Mailbox {
    path: PathBuf,
}

impl Mailbox {
    /// Resolves the mailbox path, creating the data directory if needed.
    pub fn open() -> Result<Self> {
        Ok(Self {
            path: DataStorage::new().get_path(MAILBOX_FILE)?,
        })
    }

    /// The mailbox path, for diagnostics.
    pub fn path(&self) -> &PathBuf {
        &self.path
    }

    /// Appends one request.
    ///
    /// Append-only with a single `write_all` so two clicks in the same moment
    /// cannot interleave into one corrupt line: the courier writes one short
    /// line and exits, which is the case append mode handles atomically.
    pub fn post(&self, request: &ToastRequest) -> Result<()> {
        let mut file = OpenOptions::new().create(true).append(true).open(&self.path)?;
        file.write_all(format!("{}\n", request.to_line()).as_bytes())?;
        file.flush()?;
        Ok(())
    }

    /// Takes everything waiting, leaving the mailbox empty.
    ///
    /// The file is removed rather than truncated: a click that lands between
    /// the read and the reset would be truncated away, while a removal loses
    /// only what was already read - the courier recreates the file on its
    /// next append. Missing file means no clicks, which is the common case
    /// and not an error.
    pub fn collect(&self) -> Result<Vec<ToastRequest>> {
        let contents = match fs::read_to_string(&self.path) {
            Ok(contents) => contents,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(e) => return Err(e.into()),
        };
        let requests: Vec<ToastRequest> = contents.lines().filter_map(ToastRequest::from_line).collect();
        let _ = fs::remove_file(&self.path);
        Ok(requests)
    }
}

/// Validates an issue key before it is baked into a file name or a shortcut.
///
/// A key reaches this from Jira, and it ends up inside a file name and a
/// command line. Anything but the shape Jira actually uses is refused rather
/// than escaped: refusing a key we have never seen costs one toast without
/// buttons, while quoting it wrong would put attacker-chosen text on a
/// command line the shell runs.
pub fn validate_issue_key(key: &str) -> Result<()> {
    if key.is_empty() || key.len() > 64 {
        bail!("issue key has an unusable length: {key:?}");
    }
    if !key.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_') {
        bail!("issue key has characters that cannot go in a shortcut: {key:?}");
    }
    Ok(())
}

/// The directory the per-action shortcuts live in.
pub fn shortcut_dir() -> Result<PathBuf> {
    let dir = DataStorage::new().get_path(SHORTCUT_DIR)?;
    fs::create_dir_all(&dir)?;
    Ok(dir)
}
