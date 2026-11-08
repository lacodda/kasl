---
title: "Configuration"
---

kasl keeps its settings in one JSON file. This page says what every setting
does; [`kasl config`](/reference/config/) reads and changes them one at a time,
and [`kasl setup`](/reference/setup/) walks whole modules with prompts.

## Where the file is

| System | Path |
| --- | --- |
| Windows | `%LOCALAPPDATA%\lacodda\kasl\config.json` |
| macOS | `~/Library/Application Support/lacodda/kasl/config.json` |
| Linux | `~/.local/share/lacodda/kasl/config.json` |

`kasl config path` prints it. There is no file until something writes one;
until then every setting is its default.

## Changing a setting

```bash
kasl config show                              # every setting, set or default
kasl config set monitor.pause_threshold 90    # checked before it is written
kasl config unset monitor.pause_threshold     # back to the default
kasl config check                             # what the file says that kasl does not read
```

A setting is named by its block and its name: `monitor.pause_threshold`.
`kasl config set` checks the value against the file's schema - its type, its
range, its choices - and refuses it otherwise, leaving the file as it was. See
[`config`](/reference/config/).

Editing the file by hand works as well. Every file kasl writes starts with
`$schema`, so an editor that knows JSON Schema - VS Code does out of the box -
completes keys, explains them on hover, and underlines a misspelt key as you
type. The schema is published at `https://kasl.lacodda.com/config.schema.json`
and printed by `kasl config schema`.

### Every block may be partial

A block names only the settings you changed; the rest are its defaults. This
is a whole, valid file:

```json title="config.json"
{
  "$schema": "https://kasl.lacodda.com/config.schema.json",
  "monitor": { "pause_threshold": 90 },
  "report": { "language": "ru" }
}
```

Defaults are not written into the file, so a setting you never changed keeps
following kasl if a later version changes its default. Blocks without defaults
- the connections, `si` and `jira` - need their required fields: a login and
the addresses.

### What kasl does with a mistake

kasl reads past what it does not understand rather than failing: the watcher
reads this file too, and a typo must not stop the workday being recorded. An
unknown key is ignored, a value of the wrong type fails only the command that
reads it, and an [inbox rule](/reference/inbox/#rules) that cannot be read is
skipped and named. Which is why a mistake needs saying: `kasl config check`
names each one, with the key probably meant.

## Monitor

How the watcher turns keyboard and mouse activity into a workday and its
pauses.

| Setting | Default | What it does |
| --- | --- | --- |
| `min_pause_duration` | `20` | Minutes. A pause shorter than this is noise, not a break, and is not kept |
| `pause_threshold` | `60` | Seconds without input before a pause begins |
| `poll_interval` | `500` | Milliseconds between activity checks; lower is more responsive and costs more CPU |
| `activity_threshold` | `30` | Seconds of steady activity needed to start a workday, so a stray nudge of the mouse does not open the day |
| `min_work_interval` | `10` | Minutes. Work intervals shorter than this are left out of reports and of what is sent |
| `pause_merge_gap` | `30` | Seconds. A stray keypress in the middle of a break splits it in two; pauses no further apart than this count as one, so neither half is dropped for being short. Keep it small, or genuine short work between pauses is swallowed into the break |

The watcher reads these when it starts. `kasl config set monitor.…` restarts a
running watcher so they take effect; after editing the file by hand, restart
it with `kasl watch`, which stops the running watcher and starts a new one.

## Productivity

| Setting | Default | Range | What it does |
| --- | --- | --- | --- |
| `min_productivity_threshold` | `75.0` | 0 - 100 | Productivity, in percent, below which a report is held back. If an absence is missing from the day, record it with `kasl pauses add` |
| `workday_hours` | `8.0` | 1 - 24 | The expected workday, in hours |
| `min_workday_fraction_before_suggest` | `0.5` | 0 - 1 | How much of the workday must pass before the low-productivity warning appears: early in the day one pause swings the ratio, and warning then would be noise |

```json title="config.json"
{
  "productivity": {
    "min_productivity_threshold": 75.0,
    "workday_hours": 8.0,
    "min_workday_fraction_before_suggest": 0.5
  }
}
```

## Report

Defaults for generated report files. Every setting is optional.

| Setting | Default | What it does |
| --- | --- | --- |
| `output_dir` | none | Directory for exports made without `--output`; created if missing. Without it, a timestamped file in the current directory |
| `filename_template` | `daily_report_{date}{seq}` | File name, without extension. `{date}` is the report date as `YYYY-MM-DD`; `{seq}` is empty for the day's first report, then `_2`, `_3` and so on |
| `language` | `en` | `en` or `ru`: the language of the labels inside hourly Excel reports. Russian was the default before 1.0; this opts back into it |
| `template` | `siserver` | Design template name, read from `<data>/report_templates/<name>.json`. A missing file falls back to the built-in look |

```json title="config.json"
{
  "report": {
    "output_dir": "~/reports",
    "filename_template": "daily_report_{date}{seq}",
    "language": "en"
  }
}
```

## Task discovery

What [`kasl task find`](/reference/task/) leaves out.

| Setting | Default | What it does |
| --- | --- | --- |
| `ignore_names` | `["Merge remote-tracking branch", "Merge branch ", "update webui"]` | Task and commit names, or their beginnings, that discovery skips. Matched without regard to case; `kasl task find` can add to it |

## SiServer

The corporate reporting API: `kasl report --send` and the company's rest days.
The password is kept in the OS keyring, not here.

| Setting | What it is |
| --- | --- |
| `login` | Corporate user name for LDAP |
| `auth_url` | The login endpoint |
| `api_url` | The reporting API |

All three are required once the block is there.

## Jira

Issue discovery and the [inbox](/reference/inbox/). The password is kept in the
OS keyring, not here.

| Setting | What it is |
| --- | --- |
| `login` | Jira user name - an email address on Atlassian Cloud; ask your administrator on Server or Data Center |
| `api_url` | The instance's root URL, without `/rest/api/`: `https://company.atlassian.net`, `https://jira.company.com` |

```json title="config.json"
{
  "jira": {
    "login": "john.doe",
    "api_url": "https://company.atlassian.net"
  }
}
```

## GitLab

GitLab is a [plugin](/reference/plugin/), so it has no block of its own. Its
settings (`url`, `token`, `start_label`) live under `plugins.gitlab` - see
[Plugins](#plugins) below - and are written by `kasl plugin setup gitlab`. The
token is kept in the OS keyring, not in this file; the
[GitLab guide](/guides/gitlab-inbox/) has the details. A `gitlab` block written
by an older kasl is moved automatically on the first read.

## Inbox

The background poll behind [`kasl inbox`](/reference/inbox/), for every
[source](/reference/inbox/#sources). Jira's poll needs the `jira` block and
starts once this block is in the file; a plugin source is polled once it is set
up, with these settings or their defaults. Written `jira_inbox` before 1.19,
when Jira was the only source: a file with the old key is read as it is and
saved under `inbox`.

| Setting | Default | What it does |
| --- | --- | --- |
| `enabled` | `true` | Whether the watcher polls the inbox's sources |
| `poll_interval_secs` | `300` | Seconds between polls; at least 30 |
| `notify` | `true` | A desktop toast when a new issue lands on you |
| `notify_changes` | `true` | A toast when an issue already in the inbox changes status or priority. A score change shows as a badge and never toasts |
| `notify_gone` | `false` | A toast when an issue leaves the inbox, closed or reassigned. Off by default: a departure is rarely worth interrupting you for |
| `toast_snooze_for` | `1d` | How long a toast's **Snooze** button sleeps - `3d`, `12h`, `2w`. Spelt as `inbox triage --snooze-for` is, so a button and the picker cannot put an issue to sleep for different lengths |
| `start_on_take` | `false` | Every take - `kasl inbox take`, triage, a toast's **Take** button - also starts the issue at its source. Off by default: it writes to a tracker the whole team reads, and in some teams "In Progress" means more than "on my list". See [`inbox start`](/reference/inbox/#start---start-an-issue-at-its-source) |
| `custom_fields` | `[]` | Extra Jira fields to fetch and show, such as a scoring field: `{ "id": "customfield_12345", "label": "Scoring" }` |
| `sort_by_field` | none | The field id the inbox is ranked by, highest first - typically the scoring custom field |
| `rules` | `[]` | What the inbox does by itself - see [Rules](/reference/inbox/#rules) |
| `presets` | `{}` | Named cuts of the list - see [Presets](/reference/inbox/#presets) |

**Rules.** `when` takes `priority` (`"High+"`), `min_score`, `status`, `type`
and `project` - a name or a list of names; `then` takes `pin`, `urgent`,
`dismiss` and `tag`. Pin, urgent and dismiss act when an issue arrives
matching, or a change makes it match; tags act when the issue is taken. A rule
that cannot be read - a misspelt key, no action, dismiss with pin - is skipped
and named by `kasl inbox rules` and `kasl config check`, and never makes the
rest of the file unreadable.

**Presets.** A preset is a name for the cuts of `kasl inbox list`, in the same
words: `since`, `new`, `changed`, `min_score`, `priority`, `status`, `type`,
`project`, `sort`. `kasl inbox --preset urgent` lists what the preset keeps;
a flag given beside it wins over the preset's field.

```json title="config.json"
{
  "inbox": {
    "enabled": true,
    "poll_interval_secs": 300,
    "notify": true,
    "notify_changes": true,
    "notify_gone": false,
    "toast_snooze_for": "1d",
    "start_on_take": false,
    "custom_fields": [{ "id": "customfield_12345", "label": "Scoring" }],
    "sort_by_field": "customfield_12345",
    "rules": [
      { "name": "on fire", "when": { "priority": "High+" }, "then": { "pin": true, "urgent": true } },
      { "when": { "type": "Epic" }, "then": { "dismiss": true } },
      { "when": { "project": "PAY" }, "then": { "tag": "payments" } }
    ],
    "presets": {
      "urgent": { "priority": "High+", "since": "7d" },
      "fresh-bugs": { "type": "Bug", "new": true, "sort": "priority" }
    }
  }
}
```

## kasl-server

The team server this machine reports to: its `url`, a `ca_certificate` when
the server's certificate is signed by a company CA, and `pulse` - whether the
watcher tells the server, once a minute, whether you are working. Written by
[`kasl server connect`](/reference/server/) and `kasl server pulse enable` /
`disable`, not by hand: connecting checks the server before keeping it, and the
pulse is a consent. The agent token is kept in the OS keyring.

## Plugins

What each [plugin](/reference/plugin/) has been told, by its name - `gitlab` for
`kasl-plugin-gitlab`. [`kasl plugin setup NAME`](/reference/plugin/#setup---tell-a-plugin-what-it-needs)
writes it; a plugin is used because it is installed, not because it is listed
here.

```json title="config.json"
{
  "plugins": {
    "gitlab": {
      "settings": { "url": "https://gitlab.example.com" }
    }
  }
}
```

`settings` holds the plugin's settings that are not secret, by the keys its
manifest declares. A secret one - a token, a password - is kept in the OS
keyring as `plugin.NAME.KEY`, never here, which is why `kasl config set` leaves
this block to `kasl plugin setup`.

## Keys an older kasl wrote

A file written by an older kasl may hold keys this one reads differently.
None of them stops anything working, `kasl config check` lists them as notices,
and each leaves the file the next time kasl saves it:

| Key | What became of it |
| --- | --- |
| `jira_inbox` | The name of `inbox` before 1.19; read as `inbox` |
| `gitlab` | GitLab's own block before 1.20; moved to the GitLab plugin on the first read |
| `server` | An API URL and a token in plain text, offered by `kasl setup` until 1.21 and read by nothing - SiServer reports go through `si`. Ignored |
| `jira.completed_statuses` | Kept for compatibility until 1.21 and read by nothing. Ignored |

## Starting over

```bash
kasl setup --delete
```

Deletes the file, so every setting is back to its default. The database and
the OS keyring are left as they are.

## Credentials are not in this file

Passwords and tokens are never written to `config.json`; they live in the OS
keyring - see [API Integrations](/concepts/api-integrations/). So do a plugin's
secret settings, as `plugin.NAME.KEY` - the GitLab token is
`plugin.gitlab.token`. There is no environment-variable override: kasl reads
its settings from this file only.

## Related pages

- [`config`](/reference/config/) - read and change one setting at a time
- [`setup`](/reference/setup/) - the wizard, module by module
- [API Integrations](/concepts/api-integrations/) - Jira, SiServer and the GitLab plugin in detail
- [`watch`](/reference/watch/) - what the monitor settings govern
