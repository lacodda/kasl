# ADR 0005: Jira moves by status category, and says why an issue left

- Status: accepted
- Date: 2026-10-08

## Context

Until 1.15 kasl only read Jira: the inbox polled assigned open issues, `task find` asked what was resolved today. A taken issue and its task were one piece of work living in two places that never heard from each other - the issue stayed To Do while the task was being worked, and the task stayed open after Jira resolved the issue.

Writing to Jira raises three questions:

1. **Which transition.** Jira moves an issue through its workflow's transitions, and a workflow is the team's own: names are custom ("Start Progress", "Doing") and come translated ("В работе"). The completed-issue search already failed once on a name - an English "Done" in its JQL broke the whole query against a Russian Jira.
2. **What to do with a choice.** Many workflows have more than one transition into progress or done (Start Progress / Start Review; Resolve / Close). A toast button and a script have nobody to ask.
3. **Why an issue left the inbox.** The poll sees absence. A resolved issue and a reassigned one are both absent, and they ask for opposite things: one is a task to mark done, the other is not.

## Decision

**Moves are chosen by status category.** Every Jira status belongs to one of three fixed categories - `new`, `indeterminate`, `done` - whose keys are the same on every instance in every language. "Start" is the transition whose target is in `indeterminate`; "close" is one into `done`. An issue already in the target category is not moved. `--to NAME` narrows by target status or transition name for the case below; it never widens a move outside its category.

**No guessing.** With several candidate transitions, a terminal asks; anywhere else the move does not happen and the answer names `kasl inbox start|close KEY --to NAME`. The same holds for a close screen that requires a resolution: one allowed value is used, several are asked about or named with `--resolution`. Fields kasl does not fill are left for Jira to refuse in its own words.

**The comment goes before the close.** A final status is often not editable, and Jira then refuses comments too.

**The core speaks start and close, not transition ids.** `libs::jira_bridge` holds the two moves; commands and the toast never see a transition id. A future inbox source other than Jira answers the same two questions.

**Starting on take is opt-in** (`jira_inbox.start_on_take`, default off), applies to every take - command, triage, toast - and runs after the local take, which stands whatever Jira answers.

**Departures are explained for taken issues only.** When a taken issue leaves the poll, kasl asks Jira (`GET issue?fields=status,resolution`, at most 20 per poll) and stores `left_reason` = `resolved | moved` with the resolution name (migration 17, `CHECK` on the two values). NULL means not asked yet, so a question that failed is asked on the next poll. Both clear when the issue reappears. A resolved issue whose task is open is offered once - a toast with one **Mark done** button from the watcher, a ticked list from `kasl inbox sync` on a terminal - and lastingly in `kasl task find`, which marks and pre-ticks the task.

## Consequences

- Any workflow works without a mapping file, including translated and custom ones; the cost is that a workflow with two ways in needs one `--to` or one answer on a terminal.
- A toast's Take can leave the issue unmoved (ambiguity, no cached session) while the task is made; the answer says why and how to finish.
- One extra GET per departed taken issue. Issues never taken cost nothing.
- Every Jira request now has a 30-second timeout, since the watcher drains toast buttons and polls on the same runtime.
- Migration 17 also gives every task left on `task_id = 0` a chain of its own: the incomplete-task query that `task find` and this loop rely on groups by chain, and chain 0 had been merging every task not made by `task add`.
