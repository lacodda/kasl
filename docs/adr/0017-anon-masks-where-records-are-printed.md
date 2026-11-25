# ADR 0017: `--anon` masks where records are printed, with stand-ins that hold still

- Status: accepted
- Date: 2026-11-25

## Context

A transcript for a bug report, a README or a question in a chat is taken from
a live installation, and a live installation prints the names of real work:
task names, comments, issue keys, a tracker's address. People either retype
the output by hand, which is how wrong transcripts get into documentation, or
paste it as it is.

## Decision

- **A flag on the reading commands.** `--anon` gives the usual output - the
  same rows, hours, percentages and statuses - with every word a person or
  their tracker wrote replaced by a stand-in: a task's name and comment, a
  tag's and a template's name, an issue's key, summary, project and address,
  the name of an inbox rule. What kasl counted stays, and so does the
  vocabulary trackers share (`High`, `In Progress`, `Bug`).
- **Stand-ins hold still.** Each is derived from the text and a salt the
  installation keeps (`anon.salt` in the data directory, made on first use):
  `Task 4f2a1c`, `tag-13f41e`, `QXT-1731`, `https://tracker.example/QXT-1731`.
  One task carries one stand-in in every command and on every day, so
  transcripts of `task list` and `report` agree with each other. Without the
  salt nobody can try names against a published stand-in. The hash is written
  out in the module (FNV-1a and a finishing mix): the standard library's
  hasher promises nothing across compiler versions.
- **Shapes are kept where the output relies on them.** A Jira key stays
  letters, a hyphen and a number, and its project part is the project's own
  stand-in; a task name that opens with its issue's key still does, which is
  what the tables and the standup's links look for.
- **Masked where records become output, never where they are read.** The
  table views, the JSON documents and the standup's markdown ask
  `libs::anon` for the record as this run prints it. Filters, orders and sums
  work on the real values, so `task list --tag payments --anon` finds what it
  would have found; and nothing a command writes can carry a stand-in,
  because no record ever holds one.
- **Only commands that show records and change nothing take it.** `report`
  without `--send`, `standup` without `--append`, `task list`, `task show`
  with ids, `inbox list`, `inbox show` with a key and without `--why`,
  `tag list`, `template list`. Anywhere else the flag is refused with the list
  of where it works. A picker has to offer the names as they are, `--why`
  quotes the rules from the config, and a flag that is accepted and does
  nothing is the worst of the three outcomes for one whose job is to hide
  things.

## Consequences

- `tests/anon.rs` seeds one marked word into every place a person or a
  tracker writes and runs every command that takes the flag in every style:
  the word has to be in the output without the flag and nowhere in it with
  the flag. A new field of free text in a view or a document fails it until
  the field is masked.
- A new reading command does not take `--anon` until it is added to the list
  in `Cli::dispatch` - refused by default.
- Not covered: `config` and `server status` print settings - an address, a
  login - rather than records, and `export`, `mcp`, `undo` and `inbox rules`
  are left for when a transcript of them is wanted.
- The salt is one more file in the data directory. Deleting it gives every
  record a new stand-in; nothing else depends on it.
- A stand-in is six hex digits: among some thousands of tasks two may share
  one. It costs a transcript nothing, and no record is addressed by it.
