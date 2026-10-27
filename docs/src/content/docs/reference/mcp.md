---
title: "mcp"
---

The `mcp` command serves kasl's records to a coding assistant over the [Model Context Protocol](https://modelcontextprotocol.io): the day, the week, the month, pauses, tasks, the inbox and the connection to the team server, as tools the assistant calls when it needs them. It only reads. Nothing an assistant does through it adds a task, records a pause or closes a day - the command line does that.

It is not meant to be typed. An assistant starts it, talks JSON-RPC to it on stdin and stdout, and stops it when the session ends. Typed by hand in a terminal, it says on stderr what it is for and waits for a client; Ctrl+C ends it.

## Usage

```bash
kasl mcp
```

The command takes no options.

## Connecting

In Claude Code:

```bash
claude mcp add -s user kasl -- kasl mcp
```

`-s user` makes the server available in every session on this machine, not only in the directory the command was run in. In Git Bash put `MSYS_NO_PATHCONV=1` in front of it: otherwise the shell rewrites the bare `--` into a Windows path and the server is registered under the name of a directory.

Other clients take the same thing: the program `kasl` with the one argument `mcp`.

The installer does not connect it for you, on purpose - see the next section.

## What the assistant sees

Everything a tool returns goes into the assistant's conversation, and from there to whoever runs its model. For kasl that is your working hours and pauses, your task names and comments, the summaries of the Jira issues assigned to you, and the address of your team's server with the names of the person and the agent its token belongs to. If those are your employer's as much as yours, connecting kasl to an assistant is a decision to make with that in mind - which is why it is a command you run rather than something the installer does.

What does not go through it: credentials. The agent token and the passwords stay in the OS keyring, and `server_status` says only whether the token is there. Nor does anything you have not already seen - every tool answers with what a `--json` command prints on your own terminal.

## Tools

| Tool | What it answers | The same document as |
| --- | --- | --- |
| `day` | One day: when the workday began and ended, the intervals between pauses, work time, productivity, the day's tasks, the inbox counts | `kasl report --json` |
| `pauses` | A day's pauses, each with its length and whether it was recorded by hand, and their total | `kasl pauses list --json` |
| `week` | Monday to Sunday: each day's work time and productivity, company rest days, the total and the average | - |
| `month` | A month, the same way | `kasl sum --json` |
| `tasks` | Tasks of a day, every task, the unfinished ones, by tag or by id | `kasl task list --json`, `kasl task show --json` |
| `inbox` | The Jira issues assigned to you, cut and ordered as asked | `kasl inbox list --json` |
| `issue` | One issue in full and, with `why`, where its place in the list comes from | `kasl inbox show --json` |
| `server_status` | Whether this machine's connection to the team server works | `kasl server status --json` |
| `server_queue` | The days still owed to the team server | `kasl server queue --json` |

### Arguments

| Tool | Arguments |
| --- | --- |
| `day` | `date` - `YYYY-MM-DD`; today when left out |
| `pauses` | `date`, and `min_minutes` - the shortest pause listed; the configured minimum when left out |
| `week` | `date` - any day of the week; this week when left out |
| `month` | `month` - `YYYY-MM`; this month when left out |
| `tasks` | At most one of `date`, `all`, `incomplete`, `tag` and `ids`; today's tasks when none is given |
| `inbox` | The flags of [`inbox list`](/reference/inbox/), under the same names: `limit`, `all`, `snoozed`, `dismissed`, `since`, `new`, `changed`, `min_score`, `priority`, `status`, `type`, `project`, `sort`. `status`, `type` and `project` take a list |
| `issue` | `key`, as `KA-12`; `why` |

Arguments are read strictly. One a tool does not take, or a value of the wrong kind, is refused with its name rather than ignored: a misspelt `dat` dropped on the floor would answer about today while the assistant believed it had asked about Monday. Two task selectors at once are refused for the same reason - one of them would have to lose without a word.

### Documents

Each tool answers with the JSON document its command prints, under the same conventions (see [kasl in scripts and CI](/guides/scripting/#json-for-programs)): an object at the top, every key present, moments in local time as `YYYY-MM-DDTHH:MM:SS`, durations in whole seconds in fields ending in `_seconds`, percentages from 0 to 100. The document arrives twice in each answer - as text, and as structured content for a client that reads it.

The week has no command of its own yet. Its document is the month's, with the week's bounds:

```json
{
  "week": "2026-W43",
  "from": "2026-10-19",
  "to": "2026-10-25",
  "days": [
    { "date": "2026-10-19", "work_seconds": 27420, "productivity": 96.1, "rest_day": false },
    { "date": "2026-10-20", "work_seconds": 29040, "productivity": 94.7, "rest_day": false }
  ],
  "total_seconds": 56460,
  "average_seconds": 28230,
  "productivity": 95.4
}
```

A day reads the same in all three: the `work_seconds` a week or a month gives a day is the `work_seconds` its `day` gives it - the intervals between long pauses, short ones left out.

### What stays on the machine

`inbox` and `issue` read the table the watcher keeps in sync and never ask Jira, so they answer at once, as fresh as the last sync. Three tools reach a server: `server_status` asks the team server, and `week` and `month` ask the company calendar for rest days when the reporting API is configured. Each tool says which it is (`openWorldHint` in its annotations), and every tool is marked read-only (`readOnlyHint`).

## Errors

A tool that fails answers with its reason, marked `isError`, so the assistant reads what went wrong and can act on it:

```
Issue 'KA-99' not found in inbox.
```

Asking by name for something that is not there - an issue, tasks by id - is such a failure, as it is on the command line. Only a malformed request is a JSON-RPC error. Either way the server keeps going: one failed call does not end a session, a line that is not JSON is reported on stderr and skipped, and a client that hangs up ends the session quietly.

Anything the server writes on stdout is a protocol message, so nothing else goes there: what kasl would otherwise print - the migrations of a first run, a warning - goes to stderr. If a client reports the server as broken, stderr is where the reason is.

## Related commands

- [`report`](/reference/report/) - The day the `day` tool returns
- [`sum`](/reference/sum/) - The month the `month` tool returns
- [`task`](/reference/task/) - Tasks, and the commands that change them
- [`inbox`](/reference/inbox/) - The inbox's filters, which `inbox` takes by the same names
- [`server`](/reference/server/) - The team server connection behind `server_status` and `server_queue`
