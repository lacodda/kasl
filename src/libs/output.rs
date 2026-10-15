//! How kasl prints: for a person, for a log or a pipe, or for a program.
//!
//! Three readers, chosen once per run and read everywhere through [`style`]:
//!
//! - **A person at a terminal** - the default: framed tables cut to the
//!   terminal's width, emoji marks on messages, colour where a terminal shows it.
//! - **A log or a pipe** - `--plain`: the same lines without decoration. No
//!   colour, no emoji (a warning says `warning:`), no blank padding, tables as
//!   tab-separated lines with a header row, cells never cut or wrapped, no
//!   spinner. For grep and for CI logs, where a cut-off cell is a lost fact.
//! - **A program** - `--json` on a command that prints data: stdout carries one
//!   JSON document and nothing else; every message goes to stderr, plain.
//!
//! Diagnostics - warnings and errors - go to stderr whoever reads, so a
//! warning never lands between the rows a pipe is reading.
//!
//! Colour is one decision for the whole binary, made by [`colour`], and handed
//! to the crates that paint (`console` for the spinner and the prompts,
//! `tracing` for debug logs) instead of letting each decide: `console` ignores
//! `NO_COLOR` on Windows and lets `CLICOLOR_FORCE` beat it, and `tracing`
//! paints into a pipe. clap colours its own help and errors with the same
//! precedence (see [`clap_colour`]).

use anyhow::Result;
use clap::Args;
use prettytable::{Row, Table};
use serde::Serialize;
use std::fmt::Display;
use std::io::IsTerminal;
use std::sync::OnceLock;

use crate::libs::formatter::{truncate_to_width, wrap_to_width};

/// `--json`, for a command that prints data.
///
/// Flattened into each reading command rather than declared once for all, so
/// the flag appears only where it does something: a command with nothing to
/// print as data refuses `--json` as an unknown argument instead of answering
/// a script with text it cannot parse.
#[derive(Debug, Args, Clone, Copy, Default)]
pub struct Format {
    /// Print JSON instead of text: one document on stdout, for scripts and status bars
    #[arg(long)]
    pub json: bool,
}

/// What a message is, which decides its mark and its stream.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// Output in its own right: a heading, a line of a listing.
    Print,
    Success,
    Info,
    Warning,
    Error,
}

/// How this run prints; see the module header.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Style {
    /// Stdout carries one JSON document and nothing else.
    pub json: bool,
    /// No colour, no emoji, no frames, no truncation, no animation.
    pub plain: bool,
    /// Whether ANSI colour may be written to stdout.
    pub colour_stdout: bool,
    /// Whether ANSI colour may be written to stderr.
    pub colour_stderr: bool,
}

static STYLE: OnceLock<Style> = OnceLock::new();

impl Style {
    /// The style for these flags, against this process's streams and environment.
    ///
    /// `--json` implies `--plain`: what still reaches stderr in JSON mode is
    /// read by the same program that reads the document.
    pub fn detect(json: bool, plain: bool) -> Style {
        let env = |name: &str| std::env::var(name).ok();
        let plain = plain || json;
        Style {
            json,
            plain,
            colour_stdout: colour(plain, std::io::stdout().is_terminal(), &env),
            colour_stderr: colour(plain, std::io::stderr().is_terminal(), &env),
        }
    }

    /// The mark a message of this kind opens with.
    pub fn prefix(self, kind: Kind) -> &'static str {
        match (self.plain, kind) {
            (_, Kind::Print) => "",
            (false, Kind::Success) => "✅ ",
            (false, Kind::Info) => "ℹ️ ",
            (false, Kind::Warning) => "⚠️ ",
            (false, Kind::Error) => "❌ ",
            (true, Kind::Success | Kind::Info) => "",
            (true, Kind::Warning) => "warning: ",
            (true, Kind::Error) => "error: ",
        }
    }

    /// Whether a message of this kind goes to stderr.
    pub fn to_stderr(self, kind: Kind) -> bool {
        self.json || matches!(kind, Kind::Warning | Kind::Error)
    }

    /// Whether a spinner may draw: a person is watching, and asked for decoration.
    pub fn animate(self) -> bool {
        !self.plain && std::io::stderr().is_terminal()
    }
}

/// Fixes this run's style; the first call wins, later ones return it unchanged.
///
/// Called once, right after the command line is parsed, before anything
/// prints. Hands the colour decision to the `console` crate, which paints the
/// spinner and the interactive prompts.
pub fn init(json: bool, plain: bool) -> Style {
    let style = *STYLE.get_or_init(|| Style::detect(json, plain));
    console::set_colors_enabled(style.colour_stdout);
    console::set_colors_enabled_stderr(style.colour_stderr);
    style
}

/// This run's style; the default for a person when nothing set one (tests, the daemon).
pub fn style() -> Style {
    *STYLE.get_or_init(|| Style::detect(false, false))
}

/// Whether to write ANSI colour to a stream.
///
/// The order is anstream's, so kasl and clap's own help agree: `--plain`
/// first, then a non-empty `NO_COLOR` (https://no-color.org), then
/// `CLICOLOR_FORCE` other than `0`, then `CLICOLOR=0`, then whether the
/// stream is a terminal.
pub fn colour(plain: bool, terminal: bool, env: &dyn Fn(&str) -> Option<String>) -> bool {
    let set = |name: &str| env(name).filter(|value| !value.is_empty());
    if plain || set("NO_COLOR").is_some() {
        return false;
    }
    if set("CLICOLOR_FORCE").is_some_and(|value| value != "0") {
        return true;
    }
    if set("CLICOLOR").is_some_and(|value| value == "0") {
        return false;
    }
    terminal
}

/// clap's colour for its own help and usage errors.
///
/// clap prints those before kasl has parsed anything, so `--plain` is looked
/// for in the raw arguments (up to a `--`, after which nothing is a flag).
/// Everything else - `NO_COLOR`, `CLICOLOR*`, the terminal - clap's anstream
/// already decides in the same order as [`colour`].
pub fn clap_colour(args: &[String]) -> clap::ColorChoice {
    let plain = args.iter().skip(1).take_while(|arg| *arg != "--").any(|arg| arg == "--plain");
    if plain { clap::ColorChoice::Never } else { clap::ColorChoice::Auto }
}

/// Prints one message: its mark, on its stream, padded with blank lines when asked.
///
/// What the `msg_*` macros call outside debug mode.
pub fn emit(kind: Kind, message: &dyn Display, padded: bool) {
    let style = style();
    let text = format!("{}{}", style.prefix(kind), message);
    // A blank line is decoration, and a log or a pipe reads it as an empty record.
    let text = if padded && !style.plain { format!("\n{text}\n") } else { text };
    if style.to_stderr(kind) {
        eprintln!("{text}");
    } else {
        println!("{text}");
    }
}

/// Prints the error a command failed with, the last thing a failed run says.
///
/// The whole chain on one line (`outer: cause: cause`), so it greps as one record.
pub fn report_error(error: &anyhow::Error) {
    eprintln!("{}{:#}", style().prefix(Kind::Error), error);
}

/// Prints `value` as the run's one JSON document on stdout.
pub fn print_json(value: &impl Serialize) -> Result<()> {
    println!("{}", serde_json::to_string_pretty(value)?);
    Ok(())
}

/// Prints a table under its titles, framed for a person, or as tab-separated
/// lines for `--plain`.
///
/// The titles come apart from the table because prettytable cannot hand them
/// back once set. The plain form keeps them as a header row - a file of
/// tab-separated lines that names its columns is one a spreadsheet or
/// `column -t` reads as it is - and drops the empty rows tables use as visual
/// separators. Tabs and line breaks inside a cell become spaces, so one row
/// stays one line.
pub fn print_table(titles: Row, mut table: Table) {
    if !style().plain {
        table.set_format(*prettytable::format::consts::FORMAT_NO_LINESEP_WITH_TITLE);
        table.set_titles(titles);
        table.printstd();
        return;
    }
    for line in plain_lines(&titles, &table) {
        println!("{line}");
    }
}

/// The lines `--plain` prints for a table.
pub fn plain_lines(titles: &Row, table: &Table) -> Vec<String> {
    let line = |row: &Row| {
        row.iter()
            .map(|cell| cell.get_content().replace(['\t', '\n', '\r'], " "))
            .collect::<Vec<_>>()
            .join("\t")
    };
    let rows = table.row_iter().filter(|row| row.iter().any(|cell| !cell.get_content().is_empty())).map(line);
    std::iter::once(line(titles)).chain(rows).collect()
}

/// `text` cut to `width` columns for a person; whole for `--plain`.
pub fn fit(text: &str, width: usize) -> String {
    if style().plain { text.to_string() } else { truncate_to_width(text, width) }
}

/// `text` wrapped to `width` columns for a person; on one line for `--plain`.
pub fn wrap(text: &str, width: usize) -> String {
    if style().plain {
        text.split_whitespace().collect::<Vec<_>>().join(" ")
    } else {
        wrap_to_width(text, width)
    }
}

/// Quiets the panic `println!` raises when the reader of stdout has gone.
///
/// `kasl inbox --plain | head -3` closes the pipe after three lines, and the
/// next `println!` panics on it - "failed printing to stdout", os error 232
/// on Windows, a broken pipe elsewhere - turning a command that did its work
/// into code 101. git and coreutils go quiet and exit 0 there, and so does
/// kasl: once, in the panic hook, rather than at every print.
pub fn quiet_on_a_closed_pipe() {
    let report = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let message = info
            .payload()
            .downcast_ref::<String>()
            .map(String::as_str)
            .or_else(|| info.payload().downcast_ref::<&str>().copied())
            .unwrap_or_default();
        if is_a_closed_pipe(message) {
            std::process::exit(0);
        }
        report(info);
    }));
}

/// Whether a panic is the standard library failing to print into a closed pipe.
pub fn is_a_closed_pipe(message: &str) -> bool {
    message.starts_with("failed printing to std") && (message.contains("Broken pipe") || message.contains("os error 232") || message.contains("os error 109"))
}

/// Whether a write failed because the reader of the pipe has gone.
pub fn is_a_closed_pipe_error(error: &std::io::Error) -> bool {
    // 232 and 109 are Windows' "pipe is being closed" and "pipe has been ended".
    error.kind() == std::io::ErrorKind::BrokenPipe || matches!(error.raw_os_error(), Some(232 | 109))
}

/// Serde helpers for the JSON documents: one spelling for every moment.
///
/// Moments are the machine's local time as `YYYY-MM-DDTHH:MM:SS` - whole
/// seconds, no offset - which is how kasl stores them and what the readers on
/// the same machine (a status bar, rigger, the MCP server) compare against.
pub mod moment {
    use chrono::{DateTime, Local, NaiveDateTime, Utc};
    use serde::Serializer;

    const FORMAT: &str = "%Y-%m-%dT%H:%M:%S";

    /// A local moment.
    pub fn local<S: Serializer>(at: &NaiveDateTime, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(&at.format(FORMAT))
    }

    /// A local moment that may be absent.
    pub fn local_opt<S: Serializer>(at: &Option<NaiveDateTime>, serializer: S) -> Result<S::Ok, S::Error> {
        match at {
            Some(at) => local(at, serializer),
            None => serializer.serialize_none(),
        }
    }

    /// A UTC moment, written in local time like every other.
    pub fn utc_opt<S: Serializer>(at: &Option<DateTime<Utc>>, serializer: S) -> Result<S::Ok, S::Error> {
        match at {
            Some(at) => local(&at.with_timezone(&Local).naive_local(), serializer),
            None => serializer.serialize_none(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env_of(pairs: &'static [(&'static str, &'static str)]) -> impl Fn(&str) -> Option<String> {
        move |name| pairs.iter().find(|(key, _)| *key == name).map(|(_, value)| value.to_string())
    }

    #[test]
    fn a_terminal_gets_colour_and_a_pipe_does_not() {
        assert!(colour(false, true, &env_of(&[])));
        assert!(!colour(false, false, &env_of(&[])));
    }

    #[test]
    fn plain_and_no_color_beat_everything() {
        assert!(!colour(true, true, &env_of(&[("CLICOLOR_FORCE", "1")])));
        assert!(!colour(false, true, &env_of(&[("NO_COLOR", "1")])));
        // NO_COLOR over CLICOLOR_FORCE: anstream's order, so clap's help agrees.
        assert!(!colour(false, false, &env_of(&[("NO_COLOR", "1"), ("CLICOLOR_FORCE", "1")])));
    }

    #[test]
    fn an_empty_no_color_is_not_set() {
        // no-color.org: "when present and not an empty string".
        assert!(colour(false, true, &env_of(&[("NO_COLOR", "")])));
    }

    #[test]
    fn clicolor_force_paints_a_pipe_and_clicolor_zero_stops_a_terminal() {
        assert!(colour(false, false, &env_of(&[("CLICOLOR_FORCE", "1")])));
        assert!(!colour(false, false, &env_of(&[("CLICOLOR_FORCE", "0")])));
        assert!(!colour(false, true, &env_of(&[("CLICOLOR", "0")])));
    }

    #[test]
    fn plain_marks_are_words_and_diagnostics_go_to_stderr() {
        let person = Style {
            json: false,
            plain: false,
            colour_stdout: true,
            colour_stderr: true,
        };
        let pipe = Style { plain: true, ..person };
        let program = Style {
            json: true,
            plain: true,
            ..person
        };

        assert_eq!(person.prefix(Kind::Error), "❌ ");
        assert_eq!(pipe.prefix(Kind::Error), "error: ");
        assert_eq!(pipe.prefix(Kind::Warning), "warning: ");
        assert_eq!(pipe.prefix(Kind::Success), "");

        assert!(person.to_stderr(Kind::Warning) && person.to_stderr(Kind::Error));
        assert!(!person.to_stderr(Kind::Info) && !pipe.to_stderr(Kind::Print));
        assert!(program.to_stderr(Kind::Print), "JSON mode keeps stdout for the document");
    }

    #[test]
    fn clap_sees_plain_only_before_a_double_dash() {
        let args = |list: &[&str]| list.iter().map(|arg| arg.to_string()).collect::<Vec<_>>();
        assert_eq!(clap_colour(&args(&["kasl", "inbox", "--plain"])), clap::ColorChoice::Never);
        assert_eq!(clap_colour(&args(&["kasl", "task", "add", "--", "--plain"])), clap::ColorChoice::Auto);
        assert_eq!(clap_colour(&args(&["kasl", "inbox"])), clap::ColorChoice::Auto);
    }

    #[test]
    fn a_plain_table_is_tab_separated_with_its_header_and_no_separator_rows() {
        let mut table = Table::new();
        table.add_row(prettytable::row!["1", "two\twords"]);
        table.add_empty_row();
        table.add_row(prettytable::row!["TOTAL", "line\nbreak"]);
        let titles = prettytable::row!["ID", "NAME"];
        assert_eq!(plain_lines(&titles, &table), vec!["ID\tNAME", "1\ttwo words", "TOTAL\tline break"]);
    }

    #[test]
    fn a_closed_pipe_is_recognised_on_every_platform() {
        assert!(is_a_closed_pipe("failed printing to stdout: Broken pipe (os error 32)"));
        assert!(is_a_closed_pipe("failed printing to stdout: The pipe is being closed. (os error 232)"));
        assert!(!is_a_closed_pipe("index out of bounds"));
    }
}
