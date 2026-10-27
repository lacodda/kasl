# ADR 0009: Plugins are programs kasl runs for one call, and the inbox has sources

- Status: accepted
- Date: 2026-10-27

## Context

kasl's integrations had all grown inside it: Jira's inbox, GitLab's commits in `task find`, SiServer's reports. The plan moves them out - GitLab's assigned issues as the first plugin source (v1.20), Jira and SiServer into plugins later (v2.11) - and adds ones that cannot live in a public crate: a company's own tracker, Outlook. The line already chose how its products take plugins (decision of 2026-08-07, first built in kilna v0.8): an executable found by name, `--manifest` and `run`, JSON over stdio. kasl is the second host, and the first that is a command-line program with a background watcher, events and an inbox fed by several sources.

The questions this stage had to settle:

1. **How a plugin is called**, and how it says what it is.
2. **What it is told**: settings, secrets, and the thing it acts on.
3. **When it is called by itself**: which events, at which moment, with what promise of delivery.
4. **What an inbox source may decide.** The inbox table, snoozes, triage and rules are kasl's; a source only knows what is assigned.
5. **What the inbox is called** once it is not Jira's.

## Decision

**The line's convention, unchanged.** `kasl-plugin-NAME` in kasl's plugins directory, then on `PATH`; the first copy of a name wins; on Windows only `.exe`, `.cmd`, `.bat`. `--manifest` prints one object (10 seconds); `run` reads one invocation on stdin and prints one reply. No dynamic libraries (no stable Rust ABI), no sandbox (the integrations plugins are for - COM, a company's network - are what a sandbox forbids). A plugin that cannot describe itself, speaks another protocol, or contradicts itself is listed with the reason, never hidden. The executable names the plugin, not the manifest.

**Three kinds of call, one invocation.** Every invocation carries `protocol_version`, the plugin's `settings`, and exactly one of `command` (with its `target`, `day` or `issue`), `event`, or `request`. The subject is the document the matching `--json` command prints - the day of `kasl report --json`, the issue of `kasl inbox show --json` - so a plugin never asks for data back, and a field added to a document reaches scripts, the MCP server and plugins at once. The reply's `meta` is shown, never stored: kasl keeps no field a plugin writes, so every change to kasl's records goes through kasl.

**Closed vocabularies.** `events`, `requests` and `targets` are fixed lists; an unknown name makes the manifest unreadable rather than half-used. Everything is spelt snake_case - `list_issues`, not the plan's `list-issues` - so one protocol has one spelling.

**Settings belong to kasl, secrets to the keyring.** A manifest declares settings (`secret`, `required`); `kasl plugin setup` asks for them; plain values go to `plugins.NAME.settings` in the config, secret ones to the OS keyring as `plugin.NAME.KEY` (ADR 0001). Only declared settings that have a value are passed. A plugin with a required setting missing is not called - which is what lets a plugin bundled with kasl sit quietly until it is set up. There is no enable switch: installing it and setting it up is the switch.

**Events at the moment, once.** `workday_end` when a day's end is written, `report_ready` when `report --send` finds the day ready, `new_inbox_issue` when an issue arrives and no rule hides it. Listeners run one after another, each with its time limit; a failure is a warning and changes nothing, because the moment has happened. Nothing is queued: keeping what a channel missed is one queue for every channel - kasl-server's days, SiServer, plugins - and that is the plan's own stage (v1.34), not a third copy of the outbox. A source's first poll announces nothing: it is where kasl starts watching that source, and two hundred issues "arriving" at install would make `new_inbox_issue` unusable for anything that mails or posts.

**SiServer and plugins are channels side by side.** `report --send` looks for its channels before it writes anything: with neither SiServer nor a `report_ready` listener it refuses and closes nothing. Each channel then gets the ready day on its own; one failing does not stop the other. `--send --show` names the plugins that would get it.

**A day's end has one rule**, because `workday_end` hands the end to others. Today ends now, and ending it again moves the end; a past day ends at its last observed activity - the end of its last pause, the rule the report already counted an unclosed day by - and a past day already closed keeps its end. `kasl end` and `report --send` both go through it; the old "stamp now on any date" write is gone, so a send of yesterday this morning no longer stretches yesterday to this morning.

**The inbox has sources; the source answers three requests.** `list_issues` returns every issue assigned now; `transition` moves one (`to`: `start` or `close`) and answers the status it is in and whether it moved; `comment` leaves a note. The plan's fourth, `open`, became the issue's `url`: a toast's click, `kasl inbox open` and triage all open it without running anything, and a plugin that needs another way to open an issue gives a URL of a scheme it registers. A reply to `list_issues` is read whole or refused whole - silence, a missing `issues`, a broken issue or a repeated key fail the sync and mark nothing gone - because the reply decides which rows are gone. Every row carries its `source`; a poll reconciles only its own rows, a key another source holds is refused with a warning, a source's statuses are kept as `SOURCE:NAME` so they never meet a Jira id, and only Jira is asked why a taken issue left. `start`, `close`, `comment`, the take that starts an issue and the offer after a task is done go to the row's source; a key the inbox has never seen goes to Jira, the one source kasl can ask about an issue it did not list. A plugin named `jira` is refused as a source while kasl's own Jira is configured: one name, one owner.

**The inbox stops being Jira's in name too.** Migration 19 renames `jira_inbox`, `jira_statuses`, `jira_priorities` to `inbox`, `inbox_statuses`, `inbox_priorities`, adds `source` (existing rows are Jira's) and renames `tasks.jira_key` to `issue_key`; the task document's `jira_key` is `issue_key`. The config section is `inbox`, still read under `jira_inbox` and saved under the new name.

## Consequences

- v1.20 writes GitLab's assigned issues as a plugin against this protocol and touches no inbox code; v2.11 can do the same for Jira, and a plugin named `jira` takes over the rows Jira's built-in poll left once the `jira` block is removed.
- A plugin's events are best-effort. A mail plugin that was broken on Friday does not mail Friday on Monday; when the channel queue arrives it will.
- Every discovery runs every plugin's `--manifest`: each poll and each `kasl end`. With a handful of plugins this is milliseconds; a plugin that hangs on `--manifest` costs ten seconds and is listed as stuck.
- `kasl end` on a day already ended today moves its end and fires `workday_end` again; `report --send` of a closed past day fires nothing.
- `lyrn new NAME --form plugin --host kasl` can generate a plugin from this table: the protocol version, the targets `day` and `issue`, the subject, and where kasl looks.
