---
title: "Working with the Jira inbox"
sidebar:
  order: 2
---

Issues get assigned to you in Jira whether or not you have Jira open. Normally
you find out by checking - or by someone pinging you. The inbox turns that
around: kasl polls Jira for you in the background and tells you when something
lands, so checking Jira for "did anything new show up" stops being a thing you
have to remember to do.

## Turning it on

Two config sections are involved: `jira`, which holds the connection, and
`jira_inbox`, which holds the polling settings. Run `kasl setup` and pick
**Jira** and **Jira inbox** from the module checklist; the wizard walks
through both.

Polling itself happens inside `kasl watch` - the same daemon that records your
workday. There is no separate inbox process. If the daemon is not running, the
inbox does not fill up, no matter how the config looks:

```bash
kasl watch
```

`kasl inbox sync` fetches immediately if you don't want to wait for the next
scheduled poll or the daemon isn't running yet.

## What happens to an issue over time

**It appears.** The next poll after Jira assigns you an issue, it lands in the
local inbox with a `NEW` badge and - if `notify` is on, which it is by default
- a desktop toast. Each issue toasts on arrival only once.

```bash
kasl inbox
```

```
KEY        SUMMARY                          STATUS       PRIORITY  CHANGE
PROJ-412   Fix export timeout on large...    To Do        High      NEW
```

**It changes.** If its status, priority, or ranking score moves before you've
acted on it, the row keeps its key but the `CHANGE` column now shows what
moved - `status→In Progress`, `↑prio High`, `score 5→8` - and, with
`notify_changes` (also on by default), a second toast. Both the `NEW` badge
and a change badge fade after 24 hours; the issue is still there, just without
the highlight.

**It disappears.** When an issue stops coming back from Jira - closed,
resolved, or reassigned to someone else - the next sync marks it `gone` and it
drops off the plain list. It isn't deleted: `kasl inbox --all` still shows it,
tagged `gone`, sorted below everything present. If it comes back later
(reopened, reassigned back to you), the next poll clears the `gone` mark and
it reappears in the normal list like nothing happened.

**You act on it.** Three ways, and they don't overlap:

```bash
kasl inbox pin PROJ-412       # keep it at the top while you decide
kasl inbox dismiss PROJ-412   # not now - stop showing it
kasl inbox take PROJ-412      # turn it into a task
```

`take` creates a local task named `PROJ-412 <summary>` and dismisses the inbox
entry in the same step - the issue leaves the inbox because the work it
represents now lives in your task list. There is no stored link back from the
task to the Jira issue: once taken, the two are separate records, so the
inbox row does not track what became of the task afterward. If you need the
issue again, `kasl inbox open PROJ-412` still works from its browse URL.

## Sort order

`kasl inbox` doesn't list issues in the order Jira returned them. Pinned
issues always come first. Within a group, if you've configured a ranking
field (see below) the one with the highest value goes on top; then issues are
ordered by Jira priority; then by how recently the issue was seen. Gone issues
- visible only with `--all` - always sort last, regardless of pin or priority.

## Ranking by a custom field

Plain priority is often too coarse - "High" doesn't say which High issue to
pick up first. If your Jira has a numeric field for that (a Scoring field, a
custom weight, anything comparable), point the inbox at it:

```json
{
  "jira_inbox": {
    "custom_fields": [{ "id": "customfield_12345", "label": "Scoring" }],
    "sort_by_field": "customfield_12345"
  }
}
```

`custom_fields` is what gets fetched and shown; `sort_by_field` is which of
those (by id) decides the order. Setting `sort_by_field` without listing the
field in `custom_fields` still works - kasl fetches it either way - but then
its value has no label to display with. Issues without a value for that field
sort after ones that have it, in the priority/freshness order described
above.

## Toasts, and where they take you

Clicking the toast opens the issue in your browser - on Windows and Linux.
That's a real click-to-open action wired to the notification.

On macOS it doesn't: the underlying notification API there has no way to
report that a notification was clicked, so the toast is informational only -
it tells you something happened, but clicking it does nothing. Use the CLI to
open the issue instead:

```bash
kasl inbox open PROJ-412
```

A third toast, off by default (`notify_gone: false`), fires when an issue
leaves the inbox. Most people don't want to be interrupted for that; turn it
on if you do want to know the moment something closes out from under you.

## A morning with the inbox

Before diving into tasks, clear out what accumulated overnight:

```bash
kasl inbox
```

Work through the list: `take` what you're picking up today, `dismiss` what
isn't yours to worry about right now, `pin` anything you want to keep visible
without committing to it yet. From there the day runs as usual - see
[A day with kasl](/guides/a-day-with-kasl/).

## Related pages

- [`inbox`](/reference/inbox/) - full command and flag reference
- [A day with kasl](/guides/a-day-with-kasl/) - where the inbox fits into the daily flow
- [Configuration](/concepts/configuration/) - the `jira` and `jira_inbox` blocks in full
- [API Integrations](/concepts/api-integrations/) - the Jira connection itself
