---
title: "undo"
description: Put back what the last remove command took away - a task, a pause, a tag or a template, with the tags that went with it.
---

The `undo` command puts back what the last remove command took away.

## Usage

```bash
kasl undo [--list] [--json]
```

- `--list`: Show what can be put back, the latest removal first, and change nothing.
- `--json`: Print the answer as one JSON document - see [JSON output](#json-output).

```text
$ kasl task remove 1 2 --yes
✅ Deleted 2 task(s) successfully. `kasl undo` puts them back.

$ kasl undo
✅ Put back 2 tasks that `kasl task remove` removed 2 min ago.
  1  Review the release notes  (2026-11-16)
  2  Update the changelog  (2026-11-16)
```

## What can be undone

Four commands delete records, and each can be undone:

- [`kasl task remove`](/reference/task/#remove---remove-tasks-by-id-or-all-of-todays) -
  the tasks come back with their ids, their progress and their tags.
- [`kasl pauses remove`](/reference/pauses/#kasl-pauses-remove) - the pause
  comes back with its times, its reason and whether it was kept from filtering.
- [`kasl tag remove`](/reference/tag/#kasl-tag-remove) - the tag comes back
  with its color, on every task that carried it.
- [`kasl template remove`](/reference/template/#kasl-template-remove) - the
  template comes back as it was.

One command is one removal: `kasl task remove 4 5 6` and
`kasl task remove --today` are each put back whole. `kasl undo` returns the
latest removal; run again, it returns the one before. There is nothing to
confirm - putting a record back deletes nothing.

A removal is kept for 30 days. After that it is dropped the next time
something is removed, and cannot be put back.

`undo` is for removals only. An edit - `kasl task edit`, `kasl tag edit`,
a dismissed inbox issue - is not a removal and is not undone by it; a
dismissed issue comes back with
[`kasl inbox undismiss`](/reference/inbox/#undismiss---undismiss-an-inbox-issue).

## When something took its place

A record comes back as it was, unless something added since stands where it
stood:

- **An id handed out again.** A task added after the removal can hold the
  removed task's id. The newer task keeps it; the returning one gets a new
  id, and `undo` says so - `7  Fix the build  - was 5, an id taken since` -
  because a script or a note may still hold the old one. Its tags, and the
  rest of its chain removed with it, follow it to the new id.
- **A name in use again.** A tag or a template is known by its name. If a
  new one carries the removed one's name, `undo` refuses and changes
  nothing: rename or remove the newer one, then run `kasl undo` again.
- **A tag removed for good since.** A task returns with the tags that still
  exist.

## With a team server

A day sent to [kasl-server](/reference/server/) tells it "these are all the
tasks of this day", so a day pushed after the removal left the server
without the task. When this machine is connected to a server, `undo` puts
the day of every task or pause it brings back into the
[queue](/reference/server/#kasl-server-queue), and the next
`kasl server push` or `kasl server flush` sends it as it now stands:

```text
$ kasl undo
✅ Put back 1 task that `kasl task remove` removed 3 h 12 min ago.
  12  Reconcile the invoices  (2026-11-13)
ℹ️ 2026-11-13 is queued and will be sent with the next successful push.
```

A day with no workday on record has nothing to send and is not queued.

## Listing

```text
$ kasl undo --list
4 min ago, `kasl pauses remove` removed 1 pause:
  3  13:00-13:45  (2026-11-16)
2 h 10 min ago, `kasl task remove` removed 2 tasks:
  1  Review the release notes  (2026-11-16)
  2  Update the changelog  (2026-11-16)
```

The first entry is what the next `kasl undo` puts back.

## JSON output

`kasl undo --json` prints what was put back (conventions:
[kasl in scripts and CI](/guides/scripting/#json-for-programs)):

```json
{
  "restored": {
    "kind": "task",
    "command": "task remove",
    "removed_at": "2026-11-16T10:42:07",
    "items": [
      { "id": 1, "renumbered_from": null, "label": "Review the release notes", "date": "2026-11-16" },
      { "id": 2, "renumbered_from": null, "label": "Update the changelog", "date": "2026-11-16" }
    ]
  },
  "queued": []
}
```

- `restored` is `null` when there was nothing to undo; the command still
  exits 0.
- `kind` is `task`, `pause`, `tag` or `template`; `command` is the command
  that removed them.
- `id` is the id the record has now, and `renumbered_from` the one it had
  when that id was taken since - `null` otherwise.
- `label` is a task's, tag's or template's name, and a pause's hours
  (`13:00-13:45`).
- `date` is the day a task or a pause belongs to, `null` for a tag or a
  template.
- `queued` lists the days queued for the team server.

`kasl undo --list --json` prints every removal still kept, the latest first,
each in the shape of `restored`:

```json
{
  "kept_days": 30,
  "removals": [
    {
      "kind": "pause",
      "command": "pauses remove",
      "removed_at": "2026-11-16T12:48:30",
      "items": [{ "id": 3, "renumbered_from": null, "label": "13:00-13:45", "date": "2026-11-16" }]
    }
  ]
}
```

## Examples

```bash
# Put back what the last remove command took away
kasl undo

# See what the next undo would return
kasl undo --list

# In a script: remove, and take it back if the next step fails
kasl task remove 3 --yes && ./sync.sh || kasl undo
```

## Related commands

- [`task`](/reference/task/) - Removing tasks, by id or all of today's
- [`pauses`](/reference/pauses/) - Removing a pause
- [`tag`](/reference/tag/) - Removing a tag
- [`template`](/reference/template/) - Removing a template
- [`server`](/reference/server/) - The queue a restored day waits in
