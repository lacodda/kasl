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
it reappears in the normal list like nothing happened. A poll that fails
(VPN down, Jira unreachable) is not "nothing came back": it changes nothing,
so an overnight outage does not turn the whole inbox `gone` and then `back`.

**You act on it.** Four ways, and they don't overlap:

```bash
kasl inbox pin PROJ-412        # keep it at the top while you decide
kasl inbox take PROJ-412       # turn it into a task
kasl inbox snooze PROJ-412 3d  # not now - put it down until Thursday
kasl inbox dismiss PROJ-412    # not mine - stop showing it for good
```

`take` creates a local task named `PROJ-412 <summary>` and marks the issue as
taken. The issue **stays** in the inbox, wearing a `taken` badge: what you have
picked up is as much a part of the picture as what you have not.

`snooze` and `dismiss` are both ways out of the list, and the difference is
whether the issue comes back. Dismissal means "never" and is right for an
issue that is not yours. Snoozing means "not now": the issue leaves the list,
stops counting among what is waiting, and returns by itself when its time is
up, with a toast and a `back` badge. The due date is local, so an issue
deferred to Monday returns on Monday whether or not Jira is reachable.
`kasl inbox --snoozed` shows what is asleep, and `unsnooze` wakes one early.

The task stores the issue key, so the two stay connected even after you rename
the task to something that reads better. Taking the same issue twice does not
create a second task - the command tells you which task it already became.

## Clearing the pile

One issue at a time is right when one issue arrives. When the inbox has drifted
into two hundred, `kasl inbox triage` walks them and asks about each in turn -
take, snooze, dismiss, open to look first, skip, or quit - so the pile is
decided in one sitting instead of two hundred commands. The filters apply, so
you can triage just the part worth deciding now:

```bash
kasl inbox triage --since 7d --min-score 5 --snooze-for 3d
```

When you wonder why an issue sits where it does, `kasl inbox show PROJ-412
--why` says what its place is made of - which Jira field the score came from,
what the priority rank is, and what pinning, sleeping or taking did to it.

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

## Toasts, and what you can do from them

A toast about an issue is not just a notice - it carries the three decisions
worth making about that issue, so most of the pile never needs a terminal:

- **Take** - the issue becomes a task, exactly as `kasl inbox take` would
- **Snooze** - it sleeps for `toast_snooze_for`, a day by default
- **Dismiss** - it leaves the list

Clicking the toast *body* still opens the issue in your browser.

The press is carried out by the `kasl watch` daemon, which answers within a
couple of seconds with a second toast: `PROJ-412 is now a task`, or
`PROJ-412 sleeps until Sep 18 09:30`. That answer is the point - a button
that changes something in silence leaves you wondering whether it registered,
and pressing again is the natural response to that doubt.

Two things that would otherwise surprise you:

- Pressing **Take** twice reports the task you already have instead of making
  a second one. A toast can be pressed from the notification centre long
  after it appeared, so this is common rather than exotic.
- Pressing anything for an issue Jira has since closed or reassigned says
  `PROJ-412 is no longer in the inbox` rather than failing quietly.

On macOS the buttons are not there. The notification API on that platform has
no way to report that a notification was clicked at all, so the toast is
informational only, and the buttons are left out rather than drawn dead. The
same three decisions are one command away:

```bash
kasl inbox triage      # walk the pile, deciding each
kasl inbox open PROJ-412
```

| Platform | Toast body | Buttons |
| --- | --- | --- |
| Windows | Opens the issue | **Take**, **Snooze**, **Dismiss** |
| Linux (and other XDG desktops) | Opens the issue | **Take**, **Snooze**, **Dismiss** |
| macOS | Display only | None - use `kasl inbox triage` |

Stopping the watcher does not leave a dead button behind: with no daemon
running, a press is carried out on the spot instead.

A third toast, off by default (`notify_gone: false`), fires when an issue
leaves the inbox. Most people don't want to be interrupted for that; turn it
on if you do want to know the moment something closes out from under you.

Whatever the kind, more than five toasts in one poll become one: "12 new
issues - see `kasl inbox`", clicking through to your open-issues list in
Jira. A first sync of a long backlog or a mass re-scoring is one event, and
gets one toast.

## A morning with the inbox

Before diving into tasks, clear out what accumulated overnight:

```bash
kasl inbox --new
```

With two hundred open issues, the whole inbox is not a morning read - the
cuts are. `--new` is what arrived since yesterday; `--changed 7d` is what
moved this week; `--priority High+ --min-score 5` is what deserves a look
regardless of age. They combine, and the header says how much of the whole
you are looking at (`12 of 200 issues`). The same cuts narrow every picker,
so `kasl inbox take --since 7d` offers this week's issues and nothing else.

Work through the list: `take` what you're picking up today, `dismiss` what
isn't yours to worry about right now, `pin` anything you want to keep visible
without committing to it yet. From there the day runs as usual - see
[A day with kasl](/guides/a-day-with-kasl/).

At the end of the day, `kasl report` closes with a line about what is still
waiting, so the inbox does not quietly grow while you look only at what you
finished:

```
3 in the inbox (1 new, 1 taken)
```

## Related pages

- [`inbox`](/reference/inbox/) - full command and flag reference
- [A day with kasl](/guides/a-day-with-kasl/) - where the inbox fits into the daily flow
- [Configuration](/concepts/configuration/) - the `jira` and `jira_inbox` blocks in full
- [API Integrations](/concepts/api-integrations/) - the Jira connection itself
