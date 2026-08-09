//! Interactive prompts that refuse to run without a terminal.
//!
//! Every interactive prompt in kasl goes through this module. `dialoguer` reads
//! from stdin unconditionally: with no terminal attached - under the watch
//! daemon, in CI, from a cron job, behind a pipe - it blocks forever waiting
//! for input that will never arrive. A hung background process is far worse
//! than a failed one, because nothing reports it.
//!
//! `ensure_interactive` turns that hang into an immediate, explanatory error.
//! Call it before any prompt, naming the flags that would have made the command
//! non-interactive, so the message tells the caller how to fix their invocation.
//!
//! ## Usage
//!
//! ```rust,no_run
//! use kasl::libs::prompt::{ensure_interactive, is_interactive};
//!
//! # fn main() -> anyhow::Result<()> {
//! // Refuse to prompt when there is no one to answer.
//! ensure_interactive("task name is required; pass --name outside a terminal")?;
//!
//! // Or branch on it, when a non-interactive fallback exists.
//! if is_interactive() {
//!     // prompt the user
//! }
//! # Ok(())
//! # }
//! ```

use anyhow::{Result, bail};
use std::io::IsTerminal;

/// Returns true when stdin is attached to a terminal.
///
/// This is the signal that a human is present to answer a prompt. It is false
/// under the daemon, in CI, and whenever stdin is a pipe or a file.
pub fn is_interactive() -> bool {
    std::io::stdin().is_terminal()
}

/// Fails with `reason` instead of prompting when there is no terminal.
///
/// `reason` should tell the caller how to supply the missing value without a
/// prompt - which flag to pass, or which argument to provide - because that is
/// exactly what a script hitting this error needs to know.
pub fn ensure_interactive(reason: &str) -> Result<()> {
    if !is_interactive() {
        bail!("{}", reason);
    }
    Ok(())
}
