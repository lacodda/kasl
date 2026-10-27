---
title: Writing a plugin
description: The plugin protocol - --manifest, run with one JSON object on stdin, the three kinds of call (a command, an event, a request to an inbox source), settings, replies and time limits.
---

A kasl plugin is an ordinary program, not code kasl loads into itself. It is
any executable named `kasl-plugin-NAME`, found in kasl's plugins directory or
on your `PATH`, written in whatever language you like - Rust, Python, a
PowerShell script behind a `.cmd`. kasl runs it for one call and waits for it
to exit; nothing is started as a service, and nothing is kept warm between
calls.

This is the protocol kasl speaks, version 1. It is the convention of the
lacodda line: kilna's plugins are run the same way, with `--manifest` and
`run`, and differ only in what they are handed.

## Two arguments

A plugin is run with exactly one argument:

| Argument | kasl expects |
| --- | --- |
| `--manifest` | One JSON object on stdout describing the plugin, and exit 0. Within 10 seconds: a manifest is printed, not computed. |
| `run` | One JSON object on stdin (the **invocation**), and one JSON object on stdout (the **reply**) before exiting. |

Anything else may be refused; a plugin that does not understand its argument
should say so on stderr and exit non-zero.

## The manifest

```json
{
  "protocol_version": 1,
  "name": "gitlab",
  "version": "1.4",
  "description": "Assigned issues and merge requests from GitLab",
  "commands": [
    { "key": "copy-link", "label": "Copy the link", "description": "Puts the issue's link on the clipboard", "target": "issue" }
  ],
  "events": ["workday_end"],
  "requests": ["list_issues", "transition", "comment"],
  "settings": [
    { "key": "url", "label": "GitLab address", "required": true },
    { "key": "token", "label": "Access token", "secret": true, "required": true }
  ]
}
```

Only `protocol_version`, `name` and `version` are required; every list may be
left out.

- **`protocol_version`** must be `1`. A plugin declaring another version is
  listed but not used, with the mismatch shown.
- **`commands`** - what a person can run with
  [`kasl plugin run NAME KEY`](/reference/plugin/). Each has a `key`, a
  `label`, an optional `description`, a `target` - `day` or `issue`, what it
  runs on - and an optional `timeout` in seconds.
- **`events`** - the moments the plugin wants to hear about:
  `report_ready`, `workday_end`, `new_inbox_issue`.
- **`requests`** - what the plugin answers as an inbox source. One that
  answers `list_issues` is a source; `transition` and `comment` are what it
  can do to an issue it listed. `transition` or `comment` without
  `list_issues` is refused.
- **`settings`** - what it needs to be told. Each has a `key`, a `label`, an
  optional `description`, and two flags: `secret` (kept in the OS keyring,
  typed without echo) and `required` (without it the plugin is not called).

Every list is closed. An event, a request or a target kasl does not know makes
the manifest unreadable, and the plugin is listed with that reason rather than
half-used; so are a command or a setting declared twice.

The plugin is known by its executable, not by `name`: `kasl-plugin-gitlab` is
`gitlab`, whatever its manifest says.

## The invocation

Every invocation is one JSON object on one line, with the parts every call
shares - the protocol version and the plugin's settings - and exactly one of
`command`, `event` or `request` saying what kind of call it is.

`settings` holds every declared setting that has a value: the plain ones from
kasl's config, the secret ones from the keyring. A setting never set is
absent, not `null`, and a key the config still holds that the manifest no
longer declares is not passed on. A plugin never has to read kasl's files or
its keyring on its own.

### A command

```json
{
  "protocol_version": 1,
  "command": "mail",
  "target": "day",
  "subject": { "date": "2026-10-27", "workday": { "start": "2026-10-27T09:02:11+03:00", "end": null }, "intervals": [], "tasks": [], "...": "..." },
  "settings": { "to": "me@example.com" }
}
```

The subject is the whole thing the command runs on, so the first thing a
plugin does is never ask for data back:

| Target | Subject |
| --- | --- |
| `day` | The day as [`kasl report --json`](/reference/report/) prints it |
| `issue` | The issue as `kasl inbox show --json` prints it (the `issue` object) |

### An event

```json
{ "protocol_version": 1, "event": "workday_end", "subject": { "date": "2026-10-27", "...": "..." }, "settings": {} }
```

| Event | When | Subject |
| --- | --- | --- |
| `workday_end` | A day's end is written - by `kasl end`, or by the `kasl report --send` that closes the day it sends. Ending today again later fires it again, with the later end. | The day, its end in place |
| `report_ready` | `kasl report --send` finds the day ready: its tasks are there and its productivity clears the minimum. It goes to plugins whether or not SiServer is configured - each is a channel of its own. | The day |
| `new_inbox_issue` | An issue arrives in the inbox and no rule hides it. Not on the first poll of a source: that is where kasl starts watching it, not a moment its issues arrived. | The issue |

The moment has already happened when a plugin hears of it, so a plugin that
fails an event changes nothing in kasl; it is named in a warning and the other
plugins still hear. An event is delivered once and not queued for a plugin
that was not there.

### A request to an inbox source

```json
{ "protocol_version": 1, "request": "list_issues", "settings": { "url": "https://gitlab.example.com", "token": "…" } }
```

**`list_issues`** asks for every issue assigned now, whole, on every poll -
by the watcher every `inbox.poll_interval_secs`, and by `kasl inbox sync`. The
reply:

```json
{
  "list_url": "https://gitlab.example.com/dashboard/issues?assignee_username=me",
  "issues": [
    {
      "key": "app#14",
      "summary": "Login fails on an expired session",
      "url": "https://gitlab.example.com/group/app/-/issues/14",
      "id": "90114",
      "status": "Opened",
      "priority": "High",
      "priority_rank": 2,
      "score": 8,
      "type": "Bug",
      "project": "group/app",
      "updated": "2026-10-27T08:40:00Z"
    }
  ]
}
```

`key`, `summary` and `url` are required: what the issue is called, what it
says, and what opening it opens - a toast's click and `kasl inbox open` both
open `url`. The rest is optional, and a rule or a filter that reads a field
the source does not give finds nothing there. `priority_rank` ranks a
priority (lower is more urgent, as Jira's 1 is Highest); a priority named
without one takes the rank the inbox already knows for that name. `list_url`
is what a summary toast opens.

The reply decides which of the source's issues are gone, so it is read whole
or refused whole: a reply without `issues`, an issue without a key, summary or
url, or a key listed twice fails the sync and marks nothing gone. Silence is
refused too - a source with nothing assigned answers `"issues": []`.

Each source is reconciled on its own. A poll marks gone only its own issues
that it no longer lists, and a key another source already holds is left with
that source, with a warning.

**`transition`** moves an issue the source listed:

```json
{
  "protocol_version": 1,
  "request": "transition",
  "subject": { "key": "app#14", "summary": "…", "source": "gitlab", "...": "..." },
  "to": "start",
  "status": null,
  "resolution": null,
  "comment": null,
  "settings": {}
}
```

`to` is `start` (`kasl inbox start`, and `kasl inbox take` with
`inbox.start_on_take`) or `close` (`kasl inbox close`). `status` and
`resolution` are what the person named with `--to` and `--resolution`, and
`comment` is the text of `close -m`. The reply says where the issue is now and
whether this request is what put it there:

```json
{ "status": "Doing", "moved": true }
```

Both are required. `"moved": false` means it was already there. kasl keeps
the status as the issue's own until the next poll says otherwise.

**`comment`** leaves a comment on an issue the source listed - `kasl inbox
comment`, and the offer after marking a taken task done:

```json
{ "protocol_version": 1, "request": "comment", "subject": { "key": "app#14", "...": "..." }, "comment": "Fixed in !31", "settings": {} }
```

A source that does not list `transition` or `comment` in its manifest is not
asked: the command says the source cannot do it.

## The reply

One JSON object on stdout, then exit 0:

```json
{ "message": "Mailed the day to me@example.com", "meta": { "sent_at": "18:02" } }
```

- **`message`** - one line kasl shows, prefixed with the plugin's name.
- **`meta`** - fields a command found, shown under the message by
  `kasl plugin run`. Shown, not stored: a plugin never writes to kasl's
  records - every change to them goes through kasl.
- **`error`** - set this instead to report a failure; kasl shows it as the
  plugin's own words.

Printing nothing is success with nothing to say - except for `list_issues`,
where it is refused, as above. A non-zero exit is a failure, and what the
plugin wrote on stderr is shown as its reason; stderr is otherwise not shown.

## Time

A command may take its manifest's `timeout`; everything else - events,
requests, a command with no `timeout` - has two minutes. An hour is the most a
manifest can claim. A plugin that runs past its time is stopped together with
everything it started (a job object on Windows, a process group elsewhere), so
a `.cmd` that runs PowerShell is stopped whole; the call fails saying so.

## Settings and secrets

[`kasl plugin setup NAME`](/reference/plugin/#setup---tell-a-plugin-what-it-needs)
asks for each declared setting. Plain values go to the config under
`plugins.NAME.settings`; secret ones go to the OS keyring under
`plugin.NAME.KEY`. A plugin whose required settings are not all set is not
called for events or polls, and `kasl plugin list` says what it needs - which
is how a plugin that ships with kasl stays quiet until it is set up.

## A plugin in a few lines

A PowerShell plugin that hears `report_ready`, as a `.cmd` beside a `.ps1`:

```bat
@echo off
powershell -NoProfile -ExecutionPolicy Bypass -File "%~dp0kasl-plugin-hello.ps1" %*
```

```powershell
if ($args[0] -eq '--manifest') {
    '{"protocol_version":1,"name":"hello","version":"1.0","events":["report_ready"]}'
    exit 0
}
$call = [Console]::In.ReadToEnd() | ConvertFrom-Json
$minutes = [math]::Round($call.subject.work_seconds / 60)
'{"message":"' + $call.subject.date + ': ' + $minutes + ' minutes"}'
```

Put both in kasl's plugins directory and `kasl plugin list` shows `hello` as
ready; the next `kasl report --send` hands it the day.

## What a plugin should not assume

- **No persistent process.** It is started per call and expected to exit.
- **No order between plugins.** Listeners of one event are called one after
  another, in name order, but a plugin must not rely on another having run.
- **No second delivery.** A missed event is not offered again.
- **The subject is a document, not a promise of every field.** Fields are
  added between versions without changing `protocol_version` - read the ones
  you need and ignore the rest, as kasl ignores fields of a reply it does not
  read.

## Related pages

- [`plugin`](/reference/plugin/) - list, set up and run plugins
- [`inbox`](/reference/inbox/) - where a source's issues land, and what `start`, `close` and `comment` do with them
- [Configuration](/concepts/configuration/) - the `plugins` block
