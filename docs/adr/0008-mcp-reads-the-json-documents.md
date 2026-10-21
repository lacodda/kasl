# ADR 0008: The MCP server reads, and answers with the `--json` documents

- Status: accepted
- Date: 2026-10-21

## Context

The line works with Claude Code beside it, and rigger and kilna already serve their records over MCP. An assistant asked "how did my week go" or "what is waiting in my inbox" could only shell out to kasl and parse tables. 1.17 gave every reading command a `--json` document under one contract (ADR 0007), so the data had a shape before it had a second reader.

Five questions decide what the server is:

1. **What it may do.** Writing - add a task, record a pause, close the day - changes what goes to payroll and to the team server.
2. **What it answers with.** A second set of shapes for the assistant would drift from the first.
3. **What counts as a day.** `kasl report` counted a day as its intervals between long pauses with short ones dropped; `kasl sum` counted gross time minus long pauses and kept the short ones, though the docs said both dropped them. An assistant reading `day` and `week` side by side would see two lengths for one Monday. And the two ended an unclosed past day in different places: `sum` at the last pause of any length, the report and the productivity figure at the last long pause - on a day with a lunch and then an evening between short breaks, the report lost the evening. On the owner's machine most past days are never closed, so this was the common case: October read 17.5 hours apart.
4. **What can end a session.** A tool call that panics takes the whole server down, and every later call with it. Reading a workday row with a fraction of a second in it did panic.
5. **Who connects it.** rigger's installer registers rigger's server with Claude Code by itself.

## Decision

**Reading only.** Nine tools: `day`, `pauses`, `week`, `month`, `tasks`, `inbox`, `issue`, `server_status`, `server_queue`. Every tool is annotated `readOnlyHint`; the three that reach a server - the team server, the company calendar - are annotated `openWorldHint`. Writing is a later stage, and nothing of it is present in a disabled form: a tool an assistant can see is a tool it will try.

**The `--json` documents, from the same functions.** Each tool answers with the document its command prints - `day` with `kasl report --json`, `inbox` with `kasl inbox list --json`, and so on - built by the function the command calls (`DayReport::read`, `Summary`, `DayPauses::read`, `inbox::list_document`, `server::status_document`). The document goes back both as text and as `structuredContent`. A process test compares each tool's document with its command's, field for field. The week has no command yet (`kasl week` is planned with the Friday summary); its document is the month's with the week's bounds and ISO week name, and the command will render the same reading.

**One arithmetic for a day.** `DayWork::of` works a day out - ended at the last activity any pause saw, sliced at long pauses (manual ones always counted), intervals under `min_work_interval` dropped, productivity from `Productivity` - and the daily report, its `--send` and `--send --show`, the week and the month all call it. The day's end is an argument of `calculate_work_intervals` rather than something it derives from the pauses it slices with: the pauses a day is sliced at (the long ones) are not the ones its end is seen in (all of them), and the hidden derivation is what let three callers cut evenings off. `Productivity` measures to the end over its short and long pauses together; the hourly export fetches every pause for its end. Weeks and months are one reading over a date range (`Summary::read(from, to)`), with rest days asked of the company calendar year by year and kept to the range.

**Nothing ends a session but the client.** Each tool call runs on its own task, so a panic fails that call with `isError` and a sentence saying it is a defect in kasl, and the next call is served. Workday and pause rows are read through rusqlite's own date conversion, which takes a fraction of a second and a `T` separator and returns an error for anything else instead of panicking. A failing tool answers the model (`isError`), not the client; a malformed request is a JSON-RPC error; a line that is not JSON is reported on stderr and skipped; a closed stdout ends the session with exit 0.

**Arguments are read strictly.** The schemas say `additionalProperties: false`, and the server refuses by name an argument a tool does not take or a value of the wrong kind. A misspelt `dat` silently dropped would answer about today while the model believes it asked about another day. Two task selectors at once are refused rather than ranked.

**stdout is the protocol.** `kasl mcp` runs in the JSON output style: every message - a first run's migrations, a warning - goes to stderr, and the update check is skipped. A person who types the command gets one line on stderr saying what it is for.

**Connected by the person.** The installer does not register the server with any assistant. Whatever a tool returns goes to the assistant's model provider, and kasl's records - Jira issue summaries, work hours, the team server's identity - are an employer's as much as the person's. The reference page gives the `claude mcp add -s user kasl -- kasl mcp` line and says plainly what crosses.

**The protocol is spoken directly**, as rigger does: JSON-RPC 2.0 over newline-delimited stdio, protocol version 2025-06-18, tools capability only. The loop is async because two reads reach a server.

## Consequences

- An assistant answers "how long did I work this week" and "what is pinned in my inbox" from the same documents a script reads; a field added to one is there for both.
- `kasl sum` shows less time on days with short intervals than it did in 1.17 - the report's figure. `kasl report` shows more on an unclosed past day with work after its last long pause, and its productivity is measured over that time too. Nothing is stored differently; only the arithmetic is shared.
- `inbox` answers from the local table, as fresh as the watcher's last sync; the server does not ask Jira on an assistant's behalf.
- Writing tools (v2.13) join the same server and the same argument rules; they will not need a second process.
- The `export summary` sheet still counts a day as gross presence (end minus start); bringing it onto `DayWork` is left as a wish, since it is a file format someone may rely on.
