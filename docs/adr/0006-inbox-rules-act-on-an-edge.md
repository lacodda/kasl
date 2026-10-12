# ADR 0006: Inbox rules act on an edge, and a bad rule is skipped

- Status: accepted
- Date: 2026-10-12

## Context

By 1.15 the inbox could be cut, triaged, snoozed and moved in Jira, but every decision was made by hand, every time. Some decisions have the same answer every time: High and above goes on top, Epics are someone else's, work from one project carries one tag. The plan named three settings for them - an auto-pin threshold, a dismiss-by-type list, a project-to-tag map.

Four questions decide whether automatic decisions help or fight the person:

1. **Shape.** Three settings are three vocabularies, and the fourth case ("pin bugs from PAY") fits none of them.
2. **When a rule acts.** A rule applied to every row on every poll re-pins, five minutes later, the issue the person just unpinned.
3. **What a mistake costs.** A dismiss rule with a typo in its condition must not empty the inbox, and a typo anywhere in the config must not stop the watcher, which reads the same file for activity tracking.
4. **What a rule's words mean.** Priority names are the instance's own ("High", «Высокий»), and the order is Jira's priority ids.

## Decision

**One rule shape.** `jira_inbox.rules` is a list of `{name?, when, then}`. `when` speaks the list's filter words - `priority` (`High+`), `min_score`, `status`, `type`, `project` - and compiles to the same `InboxFilter` the CLI cuts with, so a rule pins exactly what the same cuts would list. `then` is `pin`, `urgent`, `dismiss`, `tag`. Type and project are stored on the row (migration 18) in the source's own words, so a future inbox source maps onto the same `when`.

**Edge, not level.** Pin, urgent and dismiss act when an issue arrives matching or a change makes it match: the poll reads the inbox before writing, and a rule counts for an issue only if it did not match before and does now. The person's later unpin or undismiss stands until the issue leaves the rule and comes back into it, which is news. Tags act at the take, on every take path. Issues present before a rule existed are reached by `kasl inbox rules --apply`, a deliberate level pass with no toasts; `kasl inbox rules` without it is the dry run.

**Precedence.** Every matching rule acts. Dismissal beats pin and urgent. A rule never dismisses a taken issue and never touches a dismissed one. The rule behind a pin or dismissal is stored on the row (`ruled_by`); a decision by hand clears it.

**A bad rule is skipped, never guessed at.** Rules are kept in the config as raw JSON and parsed one at a time with unknown keys refused. A misspelt key is refused rather than dropped - dropped, it leaves an empty `when` that matches everything. Also refused: no action, dismiss with pin or urgent, dismiss with no conditions. A refused rule is skipped and named by `kasl inbox rules`, `kasl inbox sync` and a one-line warning over `kasl inbox`; the rest of the config is unaffected.

**Priority names are a vocabulary that only grows.** `jira_priorities` (migration 18) keeps every priority name the inbox has seen with its rank, written on each upsert and never pruned; rules and `--priority` read it. A name nobody has had yet leaves the rule dormant, not broken.

**Urgent is one toast that stays.** It replaces the issue's new or changed toast, uses the Windows reminder scenario (stays on screen; needs buttons, which issue toasts have) or Linux critical urgency, wakes a snoozed issue, and is budgeted in its own hourly lane with the same bound as ordinary toasts. It obeys `notify`, not `notify_changes`.

## Consequences

- Writing a rule changes nothing until an issue arrives or changes, or until `rules --apply`; the dry run says how many issues are waiting for it.
- A dismiss rule cannot hide work in hand, and anything it hid is listed by `list --dismissed` and brought back by `undismiss`.
- Reading the inbox before each poll costs one query over a table of a few hundred rows; it is skipped when there are no rules.
- Tags go on the task's first row, where `task add --tags` puts them. Rows a task gains on later days do not carry tags yet - a limit of tags, not of rules.
- Not the Windows "urgent" scenario: Windows 11 only, and it needs a per-app permission an unpackaged program does not get.
