---
title: "plugin"
---

The `plugin` command shows the plugins installed, tells one what it needs, and
runs one of its commands by hand.

A plugin is a program named `kasl-plugin-NAME` - in any language - that kasl
runs for the length of one call: to hear an event, to list issues for the
inbox, or to do something a person asks for. There is nothing to enable: a
plugin is used because it is installed, and once its required settings are
set. How to write one is in [Writing a plugin](/guides/writing-a-plugin/).

## Usage

```bash
kasl plugin list [--json]
kasl plugin setup NAME
kasl plugin run NAME COMMAND [KEY] [--date DATE] [--json]
```

`NAME` is the part after the prefix: `gitlab` for `kasl-plugin-gitlab`.

## Where kasl looks

First in its own plugins directory, then along your `PATH`. The first copy of a
name wins, as with any other command.

| System | kasl's plugins directory |
| --- | --- |
| Windows | `%LOCALAPPDATA%\lacodda\kasl\plugins` |
| macOS | `~/Library/Application Support/lacodda/kasl/plugins` |
| Linux | `~/.local/share/lacodda/kasl/plugins` |

On Windows only `.exe`, `.cmd` and `.bat` files count; a stray
`kasl-plugin-notes.txt` is not a plugin.

## list - what is installed

```bash
kasl plugin list
```

Every plugin found, with what it said about itself and where it stands:

```
gitlab 1.4 - Assigned issues and merge requests from GitLab
  needs `kasl plugin setup gitlab`: url, token
  inbox    a source: list_issues, transition, comment
  setting  url - GitLab address (required, not set)
  setting  token - Access token (required, secret, not set)
  at C:\Users\me\AppData\Local\lacodda\kasl\plugins\kasl-plugin-gitlab.exe

outlook 2.1 - Mails the day when it is sent
  ready
  command  mail (on a day) - Mail today's report now
  hears    report_ready
  setting  to - Recipient (required)
  at C:\Users\me\AppData\Local\lacodda\kasl\plugins\kasl-plugin-outlook.cmd
```

A plugin stands one of three ways:

- **ready** - kasl calls it: for the events it hears, for inbox polls if it is
  a source, and for `plugin run`.
- **needs setup** - a required setting has no value. kasl does not call it
  until `kasl plugin setup NAME` gives it one, and `plugin run` says what is
  missing. A plugin that ships with kasl but is not set up stays quiet this way.
- **cannot be used** - it did not describe itself, speaks another protocol
  version, or contradicts itself. It is listed anyway, with the reason: an
  integration that silently does not appear is indistinguishable from one
  that was never installed.

With `--json`, one document:

```json
{
  "directory": "C:\\Users\\me\\AppData\\Local\\lacodda\\kasl\\plugins",
  "plugins": [
    {
      "name": "outlook",
      "executable": "kasl-plugin-outlook",
      "path": "C:\\Users\\me\\AppData\\Local\\lacodda\\kasl\\plugins\\kasl-plugin-outlook.cmd",
      "state": "ready",
      "problem": null,
      "missing": [],
      "manifest": {
        "protocol_version": 1,
        "name": "outlook",
        "version": "2.1",
        "description": "Mails the day when it is sent",
        "commands": [{ "key": "mail", "label": "Mail the day", "description": "Mail today's report now", "target": "day" }],
        "events": ["report_ready"],
        "requests": [],
        "settings": [{ "key": "to", "label": "Recipient", "secret": false, "required": true }]
      }
    }
  ]
}
```

| Field | Meaning |
| --- | --- |
| `state` | `ready`, `needs_setup` or `unusable`. |
| `problem` | Why it cannot be used; `null` when it can. |
| `missing` | The required settings without a value, by key. |
| `manifest` | What the plugin printed for `--manifest`; `null` when it could not say. |

## setup - tell a plugin what it needs

```bash
kasl plugin setup gitlab
```

Asks for every setting the plugin declares, in its order. The current value is
offered, so Enter keeps it. A setting the plugin marks **secret** - a token, a
password - is typed without echo and kept in the OS keyring under
`plugin.NAME.KEY` (`plugin.gitlab.token`), never in the config file; Enter on
it keeps the stored one. The rest go to the config file:

```json
{
  "plugins": {
    "gitlab": {
      "settings": { "url": "https://gitlab.example.com" }
    }
  }
}
```

It needs a terminal, as `kasl setup` does. A plugin that declares no settings
needs no setup, and `setup` says so.

## run - a plugin's command, by hand

```bash
kasl plugin run outlook mail
kasl plugin run outlook mail --date 2026-10-26
kasl plugin run gitlab copy-link app#14
```

Runs one of the commands `plugin list` shows. A command runs on a **day** or on
an **issue**:

- On a day: the day `--date` names, today by default. The plugin is handed the
  day as [`kasl report --json`](/reference/report/#json) prints it.
- On an issue: the inbox issue `KEY` names - picked from the inbox on a
  terminal when it is left out. The plugin is handed the issue as
  [`kasl inbox show --json`](/reference/inbox/) prints it.

What the plugin answers is said on one line, and the fields it found are shown
under it. Shown, not kept: a plugin never writes to kasl's records.

```
✅ outlook: Mailed the day to me@example.com
  sent_at: 18:02
```

A command has two minutes unless the plugin's manifest gives it more (an hour
at most); one that runs past its time is stopped, with whatever it started,
and the command fails saying so. A plugin that fails says why: its own error,
or what it wrote on stderr.

With `--json`, the plugin's reply as it printed it:

```json
{ "plugin": "outlook", "command": "mail", "reply": { "message": "Mailed the day to me@example.com", "meta": { "sent_at": "18:02" } } }
```

## What plugins hear

Without being run by hand, a ready plugin hears the events it lists:

| Event | When | Handed |
| --- | --- | --- |
| `workday_end` | A day's end is written: by [`kasl end`](/reference/end/), or by the [`report --send`](/reference/report/#sending) that closes the day it sends | The day, its end included |
| `report_ready` | `kasl report --send` finds the day ready: its tasks are there and its productivity clears the minimum | The day |
| `new_inbox_issue` | An issue arrives in the [inbox](/reference/inbox/) and no rule hides it - not on the first poll of a source, which is where kasl starts watching it | The issue |

A plugin that fails an event changes nothing in kasl - the moment has happened
- and is named in a warning; the others still hear. An event is delivered once,
at the moment, and not kept for a plugin that was not there.

## Related commands

- **[`inbox`](/reference/inbox/)** - Plugin sources fill it beside Jira; `start`, `close` and `comment` go to the issue's source
- **[`report`](/reference/report/)** - `--send` hands the day to `report_ready`; `--send --show` names who would get it
- **[`end`](/reference/end/)** - Closes the day and tells `workday_end`
- **[`setup`](/reference/setup/)** - kasl's own settings; a plugin's are `plugin setup`
