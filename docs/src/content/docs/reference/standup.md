---
title: "standup"
description: The day or the week as a markdown summary of what was done - for a standup, or written into a daily note.
---

The `standup` command sums up a day or a week as markdown: the hours worked
and the tasks, split into what is done and what is still in progress. Paste it
into a standup thread, or have it written into a daily note.

## Usage

```bash
kasl standup [OPTIONS]
```

| Flag | Short | Description |
| --- | --- | --- |
| `--week` | `-w` | The week, Monday to Sunday, instead of the day |
| `--last` | `-l` | The one before: yesterday, or with `--week` last week |
| `--date <DATE>` | `-d` | The day (`YYYY-MM-DD` or `today`), or with `--week` a day in the week |
| `--append <FILE>` | | Write the summary into a markdown file instead of printing it - see [Into a daily note](#into-a-daily-note) |

`--last` means yesterday, as it does for [`report`](/reference/report/) and
[`server push`](/reference/server/); on a Monday the morning standup is about
Friday, so name the day: `kasl standup --date 2026-11-06`.

## A day

```text
$ kasl standup --last
## Wednesday, November 11, 2026

Worked 7h 37m, 09:12-18:04.

**Done**

- [PROJ-412](https://jira.example.com/browse/PROJ-412) Fix session timeout on the settings page - stale cookie jar
- Review PR #318: pause merging

**In progress**

- PROJ-419 Draft migration for protected pauses (60%) - backfill pending
```

- **The hours** are the day's TOTAL as [`kasl report`](/reference/report/)
  counts it: the intervals between long pauses, short ones left out. The span
  runs from the start of the workday to its end - or, for a day that was never
  closed, to its last counted interval. Today's day, still open, reads
  `Worked 4h 10m so far, since 09:12.`
- **Done** are the day's tasks at 100% (a task with no progress recorded counts
  as done, as the task table shows it); **In progress** the rest, with how far
  each got. A task's comment follows its name.
- **An issue a task was taken from** is linked where the
  [inbox](/reference/inbox/) knows its address: the key at the start of the
  name becomes the link.

Names and comments are escaped, so a `*` or an `_` in a task name stays a
character instead of turning the rest of the line italic.

## A week

```text
$ kasl standup --week
## Week 46, November 9-15, 2026

Worked 30h 12m over 4 days.

- Monday, November 9: 8h 2m
- Tuesday, November 10: 7h 40m
- Wednesday, November 11: 7h 37m
- Thursday, November 12: 6h 53m

**Done**

- PROJ-419 Draft migration for protected pauses - merged
- Review PR #318: pause merging

**In progress**

- PROJ-430 Export the hourly breakdown (40%)
```

A task carried from day to day is one task, not one line per day: the week
lists it once, where it got to on its last day in the week. The hours are the
recorded days' own; company rest days that [`sum`](/reference/sum/) counts as
full days are left out, because nobody worked them.

## Into a daily note

`--append FILE` writes the summary into a markdown file instead of printing
it - a daily note in an Obsidian vault, a journal file, a weekly log:

```bash
kasl standup --append ~/vault/Daily/{date}.md
kasl standup --week --append ~/vault/Weekly/{week}.md
```

- **`{date}`** in the path is the day as `YYYY-MM-DD` (a week's Monday with
  `--week`), the name a daily note usually has; **`{week}`** is the ISO week,
  `2026-W46`. A leading `~` is your home directory, also for shells that pass
  it through unexpanded.
- **The note is created** when it does not exist. Its folder must exist: a
  mistyped folder fails instead of growing a new tree.
- **The summary goes at the end**, after a blank line, between two comments
  that a rendered note does not show:

  ```markdown
  <!-- kasl standup 2026-11-12 -->
  ## Thursday, November 12, 2026
  ...
  <!-- /kasl standup 2026-11-12 -->
  ```

- **Running it again replaces that block** where it is, and leaves the rest of
  the note as it was - so `--append` can run at the end of every day from a
  scheduled task, or after [`kasl end`](/reference/end/), and the note holds
  one summary, the latest. A day's block and a week's block can share a note.
- **The note's line endings are kept**: a note written with CRLF stays CRLF.

A day with nothing recorded - no workday, no task - writes nothing and says so,
and is not a failure: a scheduled run on a holiday leaves the note alone.

## Examples

```bash
# Yesterday, for the morning standup
kasl standup --last

# Last week, for a weekly report
kasl standup --week --last

# Into today's daily note, every evening
kasl standup --append ~/vault/Daily/{date}.md

# Into the clipboard (Windows / macOS)
kasl standup | clip
kasl standup | pbcopy
```

## Related commands

- [`report`](/reference/report/) - The whole day the summary is made from
- [`task`](/reference/task/) - The tasks it lists, and their progress
- [`status`](/reference/status/) - Where today stands, in one line
- [`end`](/reference/end/) - Close the day before summing it up
