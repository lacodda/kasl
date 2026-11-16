# ADR 0013: Removed rows go to a trash, and `kasl undo` puts them back

- Status: accepted
- Date: 2026-11-16

## Context

Four commands delete records: `task remove`, `pauses remove`, `tag remove`, `template remove`. Each asks before it deletes, and each takes `--yes` so a script can run it - which is also how a person deletes the wrong thing in one keystroke. There was no way back short of restoring the database file.

The schema already hinted at an answer. Migration 4 added `tasks.deleted_at` for a soft delete, and nothing ever set or read it: `task remove` deleted the row outright.

A soft delete - mark the row, leave it in its table - makes every reader of that table responsible for skipping marked rows. The tasks table is read by the report, the summary, the exports, the day upload, the incomplete-task search, the tag queries and the MCP tools; the pauses table by the monitor itself, which looks for "the last open pause". One query that forgets the filter shows a deleted task in a report or closes a deleted pause. It also leaves unique names taken by rows nobody can see: a removed tag `urgent` would refuse a new tag `urgent`.

## Decision

**A removed row leaves its table and its content moves to `trash`.** The table holds the row as a JSON object keyed by column name, the kind of record, the table it came from, when it was removed, and an operation number. The live tables hold only live rows, so no query has anything to skip. `tasks.deleted_at` is dropped (migration 21): the schema names one way of deleting a task.

**One function deletes.** `db::trash::remove` is the only place a row of these four tables is deleted; `Tasks::delete_many`, `Pauses::delete_many`, `Tags::delete` and `Templates::delete` all go through it. A removal `kasl undo` does not know about cannot be written by forgetting a step.

**One command is one operation.** `kasl task remove 4 5 6` writes three rows under one operation number and comes back whole. The rows that went with the removal go into the same operation: a tag's links to its tasks (deleted by cascade), and a task's tag links when its chain's last row went (deleted by the trigger of migration 20). They are found by reading the links before the delete and keeping the ones gone after it, so a link that survived - the chain lives on in another day's row - is not stored, and an undo does not bring back a tag taken off the task since.

**`kasl undo` returns the latest operation; run again, the one before.** A stack, not a picker: the command exists for "I just deleted the wrong thing". `--list` shows the stack.

**A row returns under its own id, unless that id was taken.** SQLite hands the highest id out again once its row is gone, so a task added after a removal can sit on the removed task's id. The newer row keeps it. The returning row gets a new id, the command says which, and what pointed at the old id inside the same operation - later rows of the chain it heads, its tag links - follows. A tag or template whose *name* is in use again is not put back at all: the undo fails, changes nothing and keeps the removal, because there is no honest way to have two tags with one name.

**The row is stored by column name and restored into the columns the table has now.** A removal made before a migration survives it: a dropped column is left out, a new one takes its default.

**A removal is kept for 30 days**, and older ones are dropped when the next removal is written. The trash is for noticing a mistake; a record someone deleted should not stay on disk forever. A constant, not a setting.

**A restored day is owed to the team server again.** A day upload declares its task list complete, so a day pushed between the removal and the undo left kasl-server without the task. The outbox holds a date and rebuilds the day on delivery, so `kasl undo` queues the date of every task or pause it returns, when a server is connected and the date has a workday. Whether the day was in fact pushed in between is not checked: one harmless re-upload costs less than tracking it.

## Consequences

No reader of `tasks`, `pauses`, `tags` or `task_templates` changed, and none can show a deleted record. `--yes` is no longer the last word.

Undo covers removals, not edits: `task edit` and `tag edit` overwrite in place and are not recorded. The trash's shape - a before-image per row, grouped by operation - would hold an edit as well, should that be wanted.

A removal that the id-renumbering cannot resolve cleanly is narrow but real: a row of a chain whose head was removed in an *earlier* operation, whose id a new task then took, would rejoin the wrong chain if restored before its head. It takes two separate removals of one task's rows and a new task in between; the cure - ids that are never handed out twice (`AUTOINCREMENT`) - means rebuilding four tables, one of which cascades into `task_tags` when dropped, and was judged a worse trade than the case it closes.

A removal of the same day's task on a machine connected to a server is not itself queued; the server learns of it with the next push of that day, as before.
