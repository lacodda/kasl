---
title: "config"
---

The `config` command reads and changes kasl's settings one at a time. Where
[`kasl setup`](/reference/setup/) walks whole modules with prompts, `config`
changes one threshold without opening `config.json` - and checks the value
against the file's [JSON Schema](#schema---the-files-json-schema) before
anything is written, so a typo is refused instead of being silently ignored.

Every setting is named by a dotted key: the block, then the setting -
`monitor.pause_threshold`, `inbox.toast_snooze_for`, `report.language`.
`kasl config show` lists them all; [Configuration](/concepts/configuration/)
says what each one does.

## Usage

```bash
kasl config [show] [--json]
kasl config get KEY [--json]
kasl config set KEY VALUE
kasl config unset KEY
kasl config check [--json]
kasl config path
kasl config schema
```

None of them asks anything, so all of them work from a script.

## show - every setting

```bash
kasl config show
```

Every setting kasl has, with the value it uses and where that value comes
from:

```
Settings in C:\Users\me\AppData\Local\lacodda\kasl\config.json

+-------------------------------+---------------------------+---------+
| KEY                           | VALUE                     | SOURCE  |
+-------------------------------+---------------------------+---------+
| inbox.enabled                 | true                      | default |
| inbox.poll_interval_secs      | 300                       | default |
| inbox.presets.urgent          | {"priority":"High+",...}  | set     |
| jira                          | —                         | not set |
| monitor.pause_threshold       | 90                        | set     |
| monitor.poll_interval         | 500                       | default |
| report.language               | en                        | default |
...
```

- **set** - written in the file.
- **default** - not in the file; kasl uses its default, shown in the value
  column. A default is not written into the file, so it keeps following kasl
  if a later version changes it.
- **not set** - a block with no defaults that is not set up yet: a connection
  such as `jira` or `si` is one line until it is.

Bare `kasl config` is the same as `kasl config show`. When the file has
problems, a line under the table says how many; [`check`](#check---whats-wrong-with-the-file)
names them. With `--json`, the list is a document: the file's path, whether it
exists, every setting as `{key, value, source}` (`source` is `set`, `default`
or `not_set`), and the file's problems and notices.

## get - one setting

```bash
kasl config get monitor.pause_threshold      # 90
kasl config get monitor                      # the whole block, as JSON
kasl config get report.language --json       # {"key":..., "value": "en", "source": "default"}
```

Prints what kasl uses: a number or a word as it is, a list or a block as JSON.
A setting that is not set and has no default - `jira.api_url` before Jira is
set up - fails with exit code 1, so a script can tell it from an empty value.
A key that is not a setting fails too, naming the one probably meant:

```
❌ `monitor.pause_treshold` is not a setting kasl knows - did you mean `monitor.pause_threshold`?
```

## set - change one setting

```bash
kasl config set monitor.pause_threshold 90
kasl config set report.language ru
kasl config set inbox.notify_gone true
kasl config set task_discovery.ignore_names '["Merge branch ", "chore(deps)"]'
kasl config set inbox.presets.urgent '{"priority": "High+", "since": "7d"}'
```

The value is read the way the setting takes it: `90` is a number where a
number is wanted, `ru` needs no quotes where text is wanted, and a list or a
block is written as JSON. Before anything is written, the value is checked
against the schema - its type, its range, its choices - and the file as a
whole must still read afterwards. Anything that fails is refused and the file
is left as it was:

```
❌ `inbox.poll_interval_secs` takes a whole number of at least 30; '5' is not one: 5 is less than the minimum of 30
❌ `report.language` takes one of en, ru; 'de' is not one: "de" is not one of "en" or "ru"
```

On success it says what changed:

```
✅ monitor.pause_threshold = 90 (was 60 by default)
```

What is written is the one setting, and nothing else: a block that names one
setting takes the rest from its defaults, so setting `monitor.pause_threshold`
does not write out the other five monitor settings. The file also gets
`$schema` (see [schema](#schema---the-files-json-schema)).

A few things happen around the change:

- **The watcher picks it up.** The monitor's thresholds are read when the
  watcher starts, so a change under `monitor` restarts a running watcher, as
  `kasl setup` does. Every other block is read again each time it is used.
- **An older file is brought up to date.** A file written by an older kasl may
  hold `jira_inbox` (the name of `inbox` before 1.19), or blocks nothing reads
  any more (`server`, `jira.completed_statuses`); the first edit moves or drops
  them, and says so.
- **Problems elsewhere do not block it.** A file with a typo somewhere else
  still takes a new setting; a line says how many other problems the file has.
- **The inbox block starts Jira's poll.** With Jira set up, the `inbox` block
  being in the file is what starts the watcher polling Jira for your issues -
  so a first inbox setting, a preset say, starts it, and kasl says so.
  `kasl config set inbox.enabled false` turns the poll off.

Two blocks are not set by hand, because a command does more than write them:

| Block | Written by |
| --- | --- |
| `kasl_server` | [`kasl server connect`](/reference/server/), which checks the server before keeping it, and `kasl server pulse enable` / `disable` |
| `plugins` | [`kasl plugin setup NAME`](/reference/plugin/) - only the plugin knows which of its settings are secret, and those go to the OS keyring, never to this file |

`kasl config get` and `show` read both.

## unset - back to the default

```bash
kasl config unset monitor.pause_threshold
```

Takes the setting out of the file, so kasl goes back to its default:

```
✅ monitor.pause_threshold is unset; kasl uses its default, 60
```

A setting that was not in the file is left alone and said to be. A setting a
block cannot do without - `jira.login`, with the rest of `jira` set - is
refused; `kasl config unset jira` removes the whole block.

## check - what's wrong with the file

```bash
kasl config check
```

Names everything in the file that kasl does not read as written, and fails
(exit code 1) when there is any of it:

```
ℹ️ jira_inbox: the name of `inbox` before 1.19: still read as `inbox`, and the next save writes it under that name
⚠️ monitr: not a setting kasl knows - did you mean `monitor`?
⚠️ productivity.workday_hours: 30 is greater than the maximum of 24
⚠️ inbox.rules.1: this rule is skipped: it does nothing: `then` needs pin, urgent, dismiss or tag
⚠️ inbox.presets.week: `--preset week` would fail: 'a week' is not a window; try 1d, 7d, 12h or 2w
❌ 4 problems in the config file
```

kasl reads past an unknown key rather than failing - the watcher reads this
file too, and a typo must not stop the workday being recorded - which is
exactly why a typo needs saying. `check` reports:

- **problems** - a key kasl does not know, a value of the wrong type or out
  of range, a required field missing, an [inbox rule](/reference/inbox/#rules)
  that is skipped, a [preset](/reference/inbox/#presets) that would fail;
- **notices** - keys an older kasl wrote, read under their new name or
  ignored, and gone from the file at its next save. Notices do not fail the
  check.

With `--json`: `{"file", "problems": [{key, message}], "notices": [...]}`,
printed before the exit code says whether there were problems.

## path - where the file is

```bash
kasl config path
```

Prints the path of `config.json`, whether or not it exists yet:

| System | Path |
| --- | --- |
| Windows | `%LOCALAPPDATA%\lacodda\kasl\config.json` |
| macOS | `~/Library/Application Support/lacodda/kasl/config.json` |
| Linux | `~/.local/share/lacodda/kasl/config.json` |

## schema - the file's JSON Schema

```bash
kasl config schema
```

Prints the JSON Schema the file follows. It is generated from the types kasl
reads the file into, so the schema and kasl cannot disagree about what a
setting is; `set` and `check` validate against this same schema.

It is also published at `https://kasl.lacodda.com/config.schema.json`, and
every file kasl writes starts by naming it:

```json
{
  "$schema": "https://kasl.lacodda.com/config.schema.json",
  "monitor": { "pause_threshold": 90 }
}
```

An editor that knows JSON Schema - VS Code does out of the box - then
completes keys, shows what each setting does on hover, and underlines a
misspelt key or a value out of range as you type. Offline, the editor simply
does not check; nothing else changes.

## Related commands

- [`setup`](/reference/setup/) - the interactive wizard, module by module
- [`inbox`](/reference/inbox/) - what `inbox.rules` and `inbox.presets` do
- [`server`](/reference/server/) - writes the `kasl_server` block
- [`plugin`](/reference/plugin/) - writes the `plugins` block
