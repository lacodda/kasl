---
title: "inbox"
---

The `inbox` command manages a local inbox of open Jira issues assigned to you. The watcher polls Jira in the background, stores discovered issues locally, and shows a desktop toast when a new issue appears or an existing one visibly changes. From the inbox you can pin, snooze, dismiss, open in the browser, or take an issue into your task list - one at a time, or the whole pile in one sitting with `triage`.

Every sync reconciles the list against Jira: issues that stop appearing in the poll (closed or reassigned) are marked gone and leave the list instead of lingering forever. They stay inspectable with `--all`.

Decisions that are the same every time - anything High or above goes on top, Epics are never yours, work from one project carries one tag - are written once as [rules](#rules), and the inbox applies them by itself.

The inbox also writes to Jira, in two places and no more: `start` moves an issue into progress, `close` resolves it, and `comment` leaves a note. Those are the two moments a local task and its issue meet - taking the issue and finishing the work - and [Closing the loop](#closing-the-loop) is how they connect.

## Usage

```bash
kasl inbox [OPTIONS] [COMMAND]
```

Running `kasl inbox` without a subcommand lists the active (non-dismissed) issues.

## Options

- `-n, --limit <N>`: Show only the top N issues after filtering and sorting
- `--all`: Include issues gone from Jira (closed or reassigned); they sort below the present ones
- `--snoozed`: Include issues that are still asleep, each showing the date it is due back
- `--dismissed`: Include dismissed issues, each saying whether you or a rule dismissed it
- `--json`: Print the list as one JSON document - see [JSON output](#json-output). It belongs to the list: `kasl inbox --json pin PROJ-412` is refused before anything is pinned; give `--json` to the subcommand that prints data

### Filters

An inbox of two hundred open issues is a pile, not a list. These cuts turn it into one; they combine (every set filter must hold) and apply to the bare `kasl inbox`, to `list`, and to every picker (`take`, `open`, `pin`, `unpin`, `dismiss` without a key), so a `take --since 7d` offers only this week's issues.

- `--since <WINDOW>`: Only issues first seen within the window - `1d`, `7d`, `12h`, `2w`; a bare number is days
- `--new`: Only issues discovered in the last day (the same window as the `NEW` badge)
- `--changed <WINDOW>`: Only issues that visibly changed (status, priority, score) within the window; issues that never changed are out
- `--min-score <N>`: Only issues whose ranking field (e.g. Scoring) is at least N; issues without a score are out
- `--priority <NAME[+]>`: Only issues of that priority, named as your Jira names it; add `+` for that priority and everything more urgent (`High+`). An unknown name lists the ones the inbox knows
- `--status <NAME>`: Only issues in that status, by name or id, regardless of case; repeat it for any of several
- `--type <NAME>`: Only issues of that type, as your Jira names it (`Bug`, `Story`, `Epic`), regardless of case; repeat it for any of several
- `--project <KEY>`: Only issues of that project - the prefix of the key; repeat it for any of several
- `--sort <score|priority|new|changed>`: Order of the list; `score` is the inbox's own order (ranking field, then priority). Pinned issues lead and gone issues trail whatever the order

Priority names are read against every priority the inbox has ever seen, not only the ones on the list right now. The same words are what a [rule](#rules)'s `when` is written in, so a rule pins exactly what the same cuts would list.

A cut list says so in its header - `Jira inbox: 12 of 200 issues (since 7d, score ≥ 5):` - so a filtered view never reads as the whole inbox. When nothing matches, the message names the cuts and the size of the whole.

## Commands

### `sync` - Sync inbox from Jira

```bash
kasl inbox sync
```

Polls Jira immediately instead of waiting for the background cadence. The summary counts fetched, new, changed, and gone issues:

```
[✓] Jira inbox synced: 7 fetched, 1 new, 2 changed, 1 gone.
```

### `list` - List active inbox issues

```bash
kasl inbox list [OPTIONS]
```

**Options:**
- `-n, --limit <N>`: Show only the top N issues
- `--all`: Include issues gone from Jira
- `--snoozed`: Include issues that are still asleep
- `--dismissed`: Include dismissed issues
- The [filters](#filters) above: `--since`, `--new`, `--changed`, `--min-score`, `--priority`, `--status`, `--type`, `--project`, `--sort`
- `--json`: Print the same slice, in the same order, as one JSON document - see [JSON output](#json-output)

The `CHANGE` column carries freshness badges for about a day: `NEW` for freshly discovered issues, a change summary such as `status→In Progress`, `↑prio High`, or `score 5→8` for existing ones, and `gone` for issues no longer returned by Jira (visible only with `--all`). `taken` marks an issue you have already started; unlike the others it does not fade, and it outranks `NEW` and change summaries. `zzz Mar 4` is a sleeping issue and the date it is due back, shown only under `--snoozed`; `back` marks one whose snooze has just run out. `gone` outranks everything - and for an issue you took, it reads `resolved` once Jira has said that is why it left. Under `--dismissed`, a dismissed issue reads `dismissed`, or `ruled out: NAME` when a [rule](#rules) hid it.

### `triage` - Triage the inbox issue by issue

```bash
kasl inbox triage [--snooze-for <FOR>] [FILTERS]
```

**Options:**
- `--snooze-for <FOR>`: How long `snooze` sleeps during this run - `3d`, `12h`, `2w`. Defaults to `1d`.
- The [filters](#filters) above, so `triage --since 7d --min-score 5` walks exactly that slice

Walks the inbox one issue at a time and asks what to do with each. Two hundred issues are not triaged by running `take`, `snooze` and `dismiss` two hundred times, each re-reading the list to find the next row.

Each issue is shown with its position, badge, score and priority, and offers:

- **take** - make it a task and start
- **snooze** - not now, bring it back later (for `--snooze-for`)
- **dismiss** - not mine, hide it for good
- **open** - look at it in the browser, then decide
- **skip** - leave it and move on (the default)
- **quit** - stop here; everything not yet decided is left untouched

`open` is a look, not a decision: the issue is asked about again once the browser is up. `skip` leads because the common answer in a long run is "not this one", and it is the only choice that changes nothing if the finger slips. A run ends with what it came to:

```
[✓] Triaged: 3 taken, 5 snoozed, 2 dismissed, 12 skipped.
```

Triage is a conversation, so it needs a terminal. In a script, use `inbox take`, `inbox snooze` and `inbox dismiss` with explicit keys.

### `show` - Show one inbox issue

```bash
kasl inbox show [KEY] [--why] [--json] [FILTERS]
```

**Arguments:**
- `KEY`: Issue key, e.g. `PROJ-123`. Omit it on a terminal to pick from the inbox; the [filters](#filters) narrow what the picker offers.

**Options:**
- `--why`: Explain where the ranking comes from and what each mark means
- `--json`: Print the issue - and with `--why` the explanation - as one JSON document; see [JSON output](#json-output)

One issue in full: the whole summary, its type and project, the URL, and the dates a six-column table row has no room for. A key the inbox does not hold is an error (exit code 1).

`--why` answers the question a list sorted by an invisible field invites - why is this issue *here*:

```
+----------+--------------+---------------------------------------------------+
| WHAT     | VALUE        | WHY                                               |
+----------+--------------+---------------------------------------------------+
| score    | 8            | read from Scoring in Jira; the list is ordered by  |
|          |              | it, highest first                                 |
| priority | High         | Jira priority id 2, which breaks ties on equal     |
|          |              | scores - lower is more urgent                     |
| pinned   | yes          | pinned issues lead the list whatever the order     |
+----------+--------------+---------------------------------------------------+
```

A pin says who made it - `pinned by hand`, or `pinned by the rule 'on fire'` - and every [rule](#rules) the issue matches now gets a line of its own saying what that rule does.

Note what this is not: a score broken into points. kasl does not compute importance. The score is a number read straight out of the Jira field you named in [configuration](#configuration), and the priority rank is Jira's own priority id - so `--why` names where each part came from rather than inventing a local formula that would disagree with Jira about which issue matters.

### `pin` - Pin an inbox issue

```bash
kasl inbox pin [KEY] [FILTERS]
```

**Arguments:**
- `KEY`: Issue key, e.g. `PROJ-123`. Omit it on a terminal to pick from the inbox; the [filters](#filters) narrow what the picker offers.

Pinned issues stay on top of the list.

### `unpin` - Unpin an inbox issue

```bash
kasl inbox unpin [KEY] [FILTERS]
```

**Arguments:**
- `KEY`: Issue key, e.g. `PROJ-123`. Omit it on a terminal to pick from the inbox; the [filters](#filters) narrow what the picker offers.

### `dismiss` - Dismiss an inbox issue

```bash
kasl inbox dismiss [KEY] [FILTERS]
```

**Arguments:**
- `KEY`: Issue key, e.g. `PROJ-123`. Omit it on a terminal to pick from the inbox; the [filters](#filters) narrow what the picker offers.

Hides an issue from the list until you bring it back with
[`undismiss`](#undismiss---undismiss-an-inbox-issue). Dismissal is the answer to
"this is not mine"; for "not now", see [`snooze`](#snooze---snooze-an-inbox-issue).

### `undismiss` - Undismiss an inbox issue

```bash
kasl inbox undismiss [KEY]
```

**Arguments:**
- `KEY`: Issue key, e.g. `PROJ-123`. Omit it on a terminal to pick from the dismissed issues.

Brings a dismissed issue back into the list, whoever dismissed it. This is the
way back from a [rule](#rules) that hid the wrong issues: correct the rule in
the config, then bring back what it hid - `kasl inbox list --dismissed` shows
each one with the rule that hid it. Bringing an issue back by hand takes the
decision from the rule, which will not hide it again unless the issue leaves
the rule and comes back into it.

```
[✓] PROJ-123 is back in the inbox.
```

### `snooze` - Snooze an inbox issue

```bash
kasl inbox snooze [KEY] [FOR] [FILTERS]
```

**Arguments:**
- `KEY`: Issue key, e.g. `PROJ-123`. Omit it on a terminal to pick from the inbox; the [filters](#filters) narrow what the picker offers.
- `FOR`: How long to sleep - `3d`, `12h`, `2w`; a bare number is days. Defaults to `1d`.

Puts an issue down until the moment passes. It leaves the list, stops counting
among what is waiting, and comes back on its own when its time is up - with a
toast and a `back` badge saying why it has returned.

The difference from `dismiss` is the returning. Dismissal means "never" and is
right for an issue that is not yours; snoozing means "not now" and is right for
the issue you will deal with on Monday. Without it the only way to defer an
issue was to keep reading past it, which is how an inbox stops being read.

Sleeping issues come back whether or not Jira is reachable: the due date is
local bookkeeping, so a VPN outage cannot hold an issue past its moment.

```
[✓] Snoozed PROJ-123 until Mar 4 09:30.
```

### `unsnooze` - Wake a snoozed inbox issue

```bash
kasl inbox unsnooze [KEY]
```

**Arguments:**
- `KEY`: Issue key, e.g. `PROJ-123`. Omit it on a terminal to pick from the sleeping issues - only those, since waking an awake issue does nothing.

Brings a sleeping issue back before its time.

### `open` - Open issue URL in browser

```bash
kasl inbox open [KEY] [FILTERS]
```

**Arguments:**
- `KEY`: Issue key, e.g. `PROJ-123`. Omit it on a terminal to pick from the inbox; the [filters](#filters) narrow what the picker offers.

### `take` - Start working on an issue

```bash
kasl inbox take [KEY] [FILTERS]
```

**Arguments:**
- `KEY`: Issue key, e.g. `PROJ-123`. Omit it on a terminal to pick from the inbox; the [filters](#filters) narrow what the picker offers.

Creates a task named `KEY summary` and records that the issue is in hand: the
task stores the issue key, and the issue stays in the inbox wearing a `taken`
badge. Dismissing is still a separate act - `take` means "I started this", not
"this is not mine".

Tags come with the take when a [rule](#rules) says so - `"when": {"project":
"PAY"}, "then": {"tag": "payments"}` tags every task taken from PAY, from the
command line, from triage and from a toast button alike.

Taking the same issue twice does not create a second task; the command says
which task it already became. The stored key survives renaming the task, so the
link outlives the summary it started with.

With [`start_on_take`](#configuration) on, the take also starts the issue in
Jira, exactly as [`start`](#start---start-an-issue-in-jira) would. The task is
made first and stands whatever Jira answers: a VPN that is down costs the move,
not the task, and the answer says so and names `kasl inbox start KEY` to try
again.

```
[✓] Imported PROJ-123 into tasks.
[✓] PROJ-123 is now In Progress in Jira.
```

### `start` - Start an issue in Jira

```bash
kasl inbox start [KEY] [--to <NAME>] [FILTERS]
```

**Arguments:**
- `KEY`: Issue key, e.g. `PROJ-123`. Omit it on a terminal to pick from the inbox; the [filters](#filters) narrow what the picker offers.

**Options:**
- `--to <NAME>`: Which way, when Jira offers several - the target status (`In Review`) or the transition (`Start Review`), either spelling, any case

Moves the issue into progress: through the transition whose target status is in
Jira's *in progress* category. kasl never moves an issue by status name - names
are your workflow's own and come translated ("In Progress", "В работе"), while
the category is the same on every Jira, so this works on any workflow without
setup. An issue already in progress is left as it is and the command says so.

When more than one transition leads into progress, kasl asks on a terminal and
refuses to guess anywhere else; `--to` settles it in a script. A move that does
not happen - no way from the current status, several ways and nobody to ask,
an issue Jira does not show you - fails the command, so a script can tell.

### `close` - Close an issue in Jira

```bash
kasl inbox close [KEY] [--comment <TEXT>] [--to <NAME>] [--resolution <NAME>] [FILTERS]
```

**Arguments:**
- `KEY`: Issue key, e.g. `PROJ-123`. Omit it on a terminal to pick from the inbox; the [filters](#filters) narrow what the picker offers.

**Options:**
- `-m, --comment <TEXT>`: A comment to add before closing
- `--to <NAME>`: Which way, when Jira offers several (`Resolved`, `Closed`, `Resolve Issue`)
- `--resolution <NAME>`: The resolution, when closing asks for one (`Done`, `Fixed`, `Won't Fix`)

Moves the issue into Jira's *done* category, the same way `start` moves it into
progress. When the close screen requires a resolution, kasl does not assume
one - `Fixed`, `Done` and `Won't Do` all close an issue and mean different
things on the team's reports. A single allowed value is no choice at all; on a
terminal several are asked about; in a script `--resolution` names one, and
without it the command lists them and fails.

The comment goes first. A closed issue is often no longer editable, and Jira
then refuses comments on it too, so closing first could leave the note with
nowhere to go. A comment Jira refuses stops the close.

Other required fields on the close screen are Jira's to explain: its own words
come back as the error - `customfield_10100: Story points are required.` -
and the issue is finished in the browser.

### `comment` - Comment on an issue in Jira

```bash
kasl inbox comment [KEY] [TEXT] [FILTERS]
```

**Arguments:**
- `KEY`: Issue key, e.g. `PROJ-123`. Omit it on a terminal to pick from the inbox; the [filters](#filters) narrow what the picker offers.
- `TEXT`: The comment. Omit it on a terminal to type it; elsewhere it is required.

## Closing the loop

A task taken from the inbox and the issue it came from are one piece of work
in two places, and each side hears when the other finishes.

**Finished here, so Jira is asked.** When a task taken from an issue reaches
100% - in `kasl task edit`, or when `kasl task find` carries it to 100% - kasl
asks what to do in Jira:

```
? PROJ-123 is done here. In Jira ›
❯ Leave PROJ-123 as it is
  Close PROJ-123
  Comment on PROJ-123
```

Leaving it is the default, so Enter on a question about a shared tracker
changes nothing in it. The question is only asked on a terminal, and not when
the inbox already knows Jira resolved the issue.

**Resolved in Jira, so the task is offered.** When an issue you took leaves the
inbox, kasl asks Jira why - one request per departed issue, and only for issues
you took. Resolved, the row is marked `resolved`; unresolved (reassigned, moved
out of reach) it stays `gone`. If the task is still open, the offer is made
once, wherever the departure was seen:

- the watcher shows a toast with a single **Mark done** button;
- `kasl inbox sync` on a terminal asks, with every such task ticked:

```
? Resolved in Jira - mark these tasks done (space toggles, enter confirms) ›
✔ PROJ-123 Fix login redirect (Done)
```

- `kasl inbox sync` in a script says it instead:

```
[i] PROJ-123 was resolved in Jira (Done); its task is still open - `kasl task find` offers to finish it.
```

The offer does not repeat on every poll. What lasts is
[`kasl task find`](/reference/task/#find---find-incomplete-tasks-and-import-from-gitlabjira): an unfinished
task whose issue Jira resolved is marked `resolved in Jira`, ticked, and
proposed at 100%.

Marking a task done moves its newest day to 100%: today's row when there is
one, otherwise a row for today that continues the task - the same thing
`task find` does when work carries over, so today's report shows it finished
today. An answer Jira did not give (VPN down) is asked for again on the next
poll.

## Rules

Rules are written in the config, under `jira_inbox.rules`. Each has a `when` and a `then`, and may have a `name`:

```json
{
  "jira_inbox": {
    "rules": [
      { "name": "on fire", "when": { "priority": "High+" }, "then": { "pin": true, "urgent": true } },
      { "when": { "min_score": 8 }, "then": { "pin": true } },
      { "when": { "type": ["Epic", "Sub-task"] }, "then": { "dismiss": true } },
      { "when": { "project": "PAY" }, "then": { "tag": "payments" } }
    ]
  }
}
```

**`when`** uses the words of the [filters](#filters): `priority` (a name as your Jira writes it, `+` for that priority and everything more urgent), `min_score`, `status`, `type` and `project`. Every condition given must hold; a list means any of its names; names match whatever the case. A priority nobody has had yet is not an error: the rule waits until the inbox has seen that name, and `kasl inbox rules` says it is waiting.

**`then`** is one or more of:

- `pin` - pin the issue, as `kasl inbox pin` does
- `urgent` - announce it with a toast that stays until you act on it: a reminder toast on Windows, a critical notification on Linux; macOS has only the ordinary toast. It takes the place of the issue's new or changed toast rather than adding a second one, counts in an hourly [budget](#background-polling) of its own so a busy afternoon cannot crowd it out, and wakes the issue if it was snoozed. `notify: false` silences it; `notify_changes: false` does not
- `dismiss` - hide the issue, as `kasl inbox dismiss` does
- `tag` - one tag or a list, put on the task when the issue is [taken](#take---start-working-on-an-issue)

Without a `name`, a rule is called by its conditions - `priority High+` - in messages, toasts and `show --why`.

**When a rule acts.** Pin, urgent and dismiss act when an issue arrives already matching, or when a change makes it match: an issue raised to High is pinned on that poll. They do not act again while it keeps matching, so unpinning an issue a rule pinned sticks. If it drops below High and is raised again, that is news, and the rule acts again. Tags act at the take. Issues already in the inbox when you write a rule are left alone until you apply the rules to them with [`kasl inbox rules --apply`](#rules---show-and-apply-inbox-rules).

**When rules disagree.** Every matching rule acts. Dismissal wins over pin and urgent - an issue a rule hides is not also pinned and announced. A rule never hides an issue you have taken, and never touches one that is already dismissed: those are your decisions.

**A rule that cannot be read is skipped, never guessed at.** A misspelt key (`prority`) is refused rather than ignored, because ignored it would leave a rule without conditions, which matches every issue. So is a rule that does nothing, one that both dismisses and pins, and a dismiss with no conditions at all. The rest of the config and the other rules carry on: a typo in a rule never stops the watcher recording your day. `kasl inbox` warns in one line, and `kasl inbox rules` says which rule and why:

```
[!] Inbox rule 4 is skipped: unknown field `prority`, expected one of `priority`, `min_score`, `status`, `type`, `project`.
```

What a rule pinned or dismissed carries its name: `show --why` says `pinned by the rule 'on fire'`, and `list --dismissed` shows what each rule hid. Pinning, unpinning, dismissing or bringing back by hand takes the decision back from the rule.

### `rules` - Show and apply inbox rules

```bash
kasl inbox rules [--apply] [--json]
```

**Options:**
- `--apply`: Apply the rules to the issues already in the inbox, as if each had just arrived
- `--json`: Print the rules, the ones skipped and what `--apply` changed as one JSON document; see [JSON output](#json-output)

Lists the rules with what each matches in the inbox now and what its pin or dismissal has not reached yet. Without `--apply` this is a dry run - the way to check a dismiss rule before it hides anything:

```
+---+---------+-----------------------+-------------------+------------------------------+
| # | NAME    | WHEN                  | THEN              | NOW                          |
+---+---------+-----------------------+-------------------+------------------------------+
| 1 | on fire | priority High+        | pin, urgent toast | 1 match, 1 not yet pinned    |
| 2 | —       | type Epic or Sub-task | dismiss           | 1 match, 1 not yet dismissed |
| 3 | —       | project PAY           | tag payments      | 1 match, 1 taken             |
+---+---------+-----------------------+-------------------+------------------------------+
[i] 2 issue(s) match a rule that has not acted on them: rules act when an issue arrives or changes. `kasl inbox rules --apply` acts on them now.
```

`--apply` pins and dismisses what the rules say, and tags the tasks of issues already taken. It shows no toasts - none of it is news - and running it twice changes nothing the second time:

```
[i] PROJ-412: pinned by 'on fire'.
[i] PROJ-418: dismissed by 'type Epic or Sub-task'.
[i] PAY-77: its task tagged payments.
[✓] The rules changed 3 issue(s).
+---+---------+-----------------------+-------------------+----------------------+
| # | NAME    | WHEN                  | THEN              | NOW                  |
+---+---------+-----------------------+-------------------+----------------------+
| 1 | on fire | priority High+        | pin, urgent toast | 1 match, 1 pinned    |
| 2 | —       | type Epic or Sub-task | dismiss           | 1 match, 1 dismissed |
| 3 | —       | project PAY           | tag payments      | 1 match, 1 taken     |
+---+---------+-----------------------+-------------------+----------------------+
```

## Toast buttons

A toast about one issue carries the three decisions worth making about it, so the pile can be triaged without opening a terminal:

- **Take** - imports the issue into your tasks, exactly as `kasl inbox take` does - including the move in Jira when `start_on_take` is on
- **Snooze** - puts it to sleep for `toast_snooze_for` (a day by default)
- **Dismiss** - hides it from the list

The toast saying a taken issue was [resolved in Jira](#closing-the-loop) has one button of its own, **Mark done**, which finishes the task.

A button has nobody to ask, so a take from a toast moves the issue only when Jira offers exactly one way into progress, and only with a session kasl already holds (a cached login or the password in the keyring). Otherwise the answer toast says what stopped it and names `kasl inbox start KEY`.

Clicking the toast *body*, as before, opens the issue in the browser.

The press is carried out by the running `kasl watch` daemon, which answers with a second toast saying what happened - `KA-1 is now a task`, `KA-1 sleeps until Sep 18 09:30`. An answer within a couple of seconds is the point: a button that changes something in silence leaves you unsure the press registered.

Pressing a button for an issue that Jira has since closed or reassigned says so (`KA-1 is no longer in the inbox`) instead of failing quietly. Pressing **Take** twice reports the task that already exists rather than making a second copy - which matters, because a toast can be pressed from the notification centre long after it appeared.

Stopping the watcher does not leave a dead button: with no daemon running, the press is carried out on the spot instead.

### What each platform can do

| Platform | Toast body | Buttons |
| --- | --- | --- |
| Windows | Opens the issue | **Take**, **Snooze**, **Dismiss**; **Mark done** on a resolved issue |
| Linux (and other XDG desktops) | Opens the issue | **Take**, **Snooze**, **Dismiss**; **Mark done** on a resolved issue |
| macOS | Display only | None - use `kasl inbox triage` |

On macOS the notification API cannot report a click at all, so the buttons are not drawn rather than drawn dead: `kasl inbox triage` walks the same three decisions with room to think.

Windows needs no registry entry and no protocol of its own. A toast button there can only ask the shell to launch a URI, and no argument survives that trip, so the watcher writes one shortcut per (action, issue) under `toast-buttons/` in the data directory and the button launches that. The shortcuts of a decided issue are removed once it is settled.

## Background Polling

Polling runs inside `kasl watch` (both daemon and `--foreground` modes). New issues trigger a desktop notification; clicking the toast opens the issue in the browser on Windows and Linux, and its buttons decide it. On macOS the toast is display-only - the notification API cannot report a click - so opening stays on `kasl inbox open`. Each issue is notified about only once. Status and priority changes on existing issues also toast (a score change or a return after a missed poll only sets the badge), and issues leaving the inbox can toast too when `notify_gone` is enabled. Snoozed issues whose time is up are woken at the start of each poll and toast their return; that step is local and runs even when the Jira poll itself is skipped or fails.

When an issue you took leaves the inbox, the poll also asks Jira why, and a resolved one with an open task gets the **Mark done** toast described in [Closing the loop](#closing-the-loop). That toast counts against the same hourly budget as the rest, and is not held back by `notify_gone`: it is a question about your task, not news that an issue left.

A poll that fails - VPN down, Jira unreachable, a session that could not be renewed - changes nothing: the list is reconciled only against an answer Jira actually gave, so a bad night does not mark every issue `gone` and bring all of them `back` in the morning. Two answers that look like answers are refused the same way: an expired session that Jira serves as the anonymous user (an empty page with status 200), and a list shorter than the total Jira counted, which happens when an issue is created or resolved while the pages are read.

Toasts are budgeted over time, not per poll. Within any hour at most five single toasts show; past that, one summary toast says how many issues were new, changed or left and opens your open-issues list in Jira, and the rest of the hour stays quiet. A trickle of three changes every five minutes is held to the same bound as two hundred at once.

Urgent toasts - the ones a [rule](#rules) asks for - are budgeted the same way in a lane of their own: an afternoon of ordinary changes does not spend the room an escalation needs, and fifty issues turning urgent at once still collapse into one summary.

Each poll also runs the [rules](#rules) over what it brought. Pins and dismissals by rule happen with or without toasts; `kasl inbox sync` says what each rule did:

```
[✓] Jira inbox synced: 6 fetched, 2 new, 1 changed, 0 gone.
[i] PROJ-420: pinned by 'on fire'; urgent by 'on fire'.
[i] PROJ-431: pinned by 'on fire'; urgent by 'on fire'.
[i] PROJ-433: dismissed by 'type Epic or Sub-task'.
```

## Configuration

Polling is enabled by adding the `jira_inbox` section to the config; the `jira` section must be configured as well.

```json
{
  "jira_inbox": {
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
      { "when": { "type": "Epic" }, "then": { "dismiss": true } }
    ]
  }
}
```

- `enabled`: Whether the watcher polls Jira (default `true` when the section is present)
- `poll_interval_secs`: Seconds between polls (default `300`)
- `notify`: Show desktop toasts for new issues (default `true`); when `false`, all inbox toasts are off
- `notify_changes`: Toast when an existing issue changes status or priority (default `true`); score changes only set the badge
- `notify_gone`: Toast when an issue leaves the inbox — closed or reassigned (default `false`)
- `toast_snooze_for`: How long a toast's **Snooze** button sleeps — `3d`, `12h`, `2w` (default `1d`), the same spelling as `triage --snooze-for`
- `start_on_take`: Every take - command, triage, toast button - also starts the issue in Jira (default `false`). Off by default because it writes to a tracker the whole team reads, and in some teams "In Progress" means more than "on my list"
- `custom_fields`: Extra Jira fields to fetch and display, such as a Scoring field
- `sort_by_field`: Field id used to rank the list in descending order
- `rules`: What the inbox does by itself - see [Rules](#rules) (default: none)

## Examples

```bash
# Show the inbox
kasl inbox

# Top five issues by ranking
kasl inbox -n 5

# This week's arrivals, most urgent first
kasl inbox --since 7d --sort priority

# What is worth a look: High or above, scoring at least 5
kasl inbox --priority High+ --min-score 5

# Pick something to start from what changed today
kasl inbox take --changed 1d

# Sync now and show the result
kasl inbox sync
kasl inbox list

# Check what left the inbox
kasl inbox list --all

# Clear the whole pile in one sitting
kasl inbox triage

# Triage only this week's serious ones, sleeping the rest for three days
kasl inbox triage --since 7d --min-score 5 --snooze-for 3d

# Put one issue down until Thursday, and see what is asleep
kasl inbox snooze PROJ-123 3d
kasl inbox --snoozed

# Why is this issue at the top?
kasl inbox show PROJ-123 --why

# Only bugs from two projects
kasl inbox --type Bug --project PAY --project OPS

# Check the rules, then apply them to the issues already here
kasl inbox rules
kasl inbox rules --apply

# See what was dismissed - by you or by a rule - and bring one back
kasl inbox list --dismissed
kasl inbox undismiss PROJ-456

# Work with a specific issue
kasl inbox pin PROJ-123
kasl inbox open PROJ-123
kasl inbox take PROJ-123

# Move it in Jira by hand
kasl inbox start PROJ-123
kasl inbox close PROJ-123 --comment "Fixed in 2.3" --resolution Done
kasl inbox comment PROJ-123 "Waiting on the API team"
```

## Scripting

Every subcommand takes its issue key as an argument, so the inbox can be driven from scripts without any interactive prompt:

```bash
kasl inbox sync
kasl inbox take PROJ-123
kasl inbox dismiss PROJ-456
kasl inbox close PROJ-123 --to Closed --resolution Done
```

`start` and `close` exit non-zero when the issue did not end up where they were sent, and name the flag that settles it.

## JSON output

`list`, `show` and `rules` take `--json` and print one document each; the
conventions every kasl document keeps are in
[kasl in scripts and CI](/guides/scripting/#json-for-programs). They read the
local inbox table only - never Jira - so they answer at once, whatever the
network is doing.

`kasl inbox list --json` - the same slice in the same order the table shows,
the [filters](#filters), `--sort` and `--limit` included:

```json
{
  "issues": [
    {
      "key": "PROJ-431",
      "summary": "Checkout fails on saved cards",
      "status": "In Progress",
      "status_id": "3",
      "priority": "High",
      "score": 8.5,
      "type": "Bug",
      "project": "PROJ",
      "url": "https://jira.example.com/browse/PROJ-431",
      "pinned": true,
      "dismissed": false,
      "ruled_by": null,
      "badge": "NEW",
      "first_seen": "2026-08-08T08:41:07",
      "last_seen": "2026-08-08T18:05:00",
      "changed_at": null,
      "taken_at": null,
      "snoozed_until": null,
      "gone_at": null,
      "left": null,
      "resolution": null
    }
  ],
  "total": 3,
  "filter": ["since 7d", "priority High+"]
}
```

| Field | Meaning |
| --- | --- |
| `total` | How many issues the inbox holds before the filters and the limit; fewer `issues` than this is a slice. |
| `filter` | The filters in the words the list's header uses; `[]` for the whole inbox. |
| `status` | The status's name; `null` until kasl has learnt the name for `status_id`. |
| `score` | The ranking field's value, when one is [configured](#configuration). |
| `ruled_by` | The [rule](#rules) behind the pin or the dismissal; `null` when a hand set it, or nothing did. |
| `badge` | What the `CHANGE` column shows: `NEW`, `taken`, `status→In Progress`, `zzz Mar 4`, `gone`. |
| `left` | Why a gone issue left: `resolved` or `moved`; `null` while it is present, or not yet known. |
| `resolution` | The resolution Jira gave, when it left resolved. |

[rigger](https://github.com/lacodda/rigger) reads the inbox this way, beside
its own tasks:

```bash
kasl inbox list --all --snoozed --json
```

`kasl inbox show KEY --json` prints `{"issue": {...}, "why": null}`; with
`--why`, `why` lists what the table under `--why` says, one object per line:

```json
{
  "issue": { "key": "PROJ-431", "...": "..." },
  "why": [
    { "what": "score", "value": "8.5", "because": "read from the ranking field in Jira; the list is ordered by it, highest first" },
    { "what": "pinned", "value": "yes", "because": "pinned by hand; pinned issues lead the list whatever the order" }
  ]
}
```

`kasl inbox rules --json` - the table, the skipped rules and, with `--apply`,
what changed (`applied` is `null` on a dry run):

```json
{
  "rules": [
    { "number": 1, "name": "on fire", "when": "priority High+", "then": "pin, urgent toast", "matches": "1 match, 1 not yet pinned", "waiting": 1 }
  ],
  "skipped": [
    { "number": 2, "reason": "unknown field `prority`, expected one of `priority`, `min_score`, `status`, `type`, `project`" }
  ],
  "applied": null
}
```

## Related commands

- [`watch`](/reference/watch/) - the daemon that polls the inbox
- [`task`](/reference/task/) - where `take` puts the issue
- [`setup`](/reference/setup/) - configuring the Jira connection
- [Configuration](/concepts/configuration/) - the `jira_inbox` block in full
