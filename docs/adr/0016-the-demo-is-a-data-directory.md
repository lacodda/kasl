# ADR 0016: The demo is a data directory of its own, entered by a word

- Status: accepted
- Date: 2026-11-25

## Context

kasl shows nothing until a watcher has run for a while: an empty report, an
empty month, an empty inbox. A newcomer who installs it sees none of what it
does on the first day, and the documentation's transcripts had to be typed by
hand or taken from somebody's real work.

kasl-server solved the same problem with `KASL_DEMO=true`, which seeds an
empty database (its ADR 0013). The CLI is a different place. Its database is
the one a running watcher writes to, on a machine that already holds, or will
hold, real records: seeded there, invented days would sit beside real ones
with nothing to tell them apart, and "refuse a populated database" would
refuse every machine the demo is wanted on a week after installing.

## Decision

- **The demo is a data directory.** `demo` inside kasl's data directory, with
  its own database, config and files. `libs::demo::enter` switches the one
  function every path is resolved through (`DataStorage::new`), so no command
  knows it runs in a demo, and none can reach the real records from inside
  one. Nothing is added to the schema: the mark that says "this is invented"
  is the directory.
- **It is entered by a word, not a variable.** `kasl demo report` is `kasl
  report` on the demo. The word is taken off the command line before clap
  parses it, so every command, flag and output style works there as it is,
  with nothing to maintain per command. A variable would have made the same
  switch, invisibly: a shell that still has it set shows invented hours
  under the prompt of a real day. The word is in every transcript it
  produces.
- **`kasl demo` builds and introduces it; any command after the word builds
  it too.** The first thing a newcomer types may be `kasl demo report`.
- **The weeks are generated.** Weekdays of the last eight weeks, ending
  today, today's day open, with a day of each kind the reports judge: leave,
  a long day, a pause recorded by hand, a day under the productivity minimum;
  an inbox with an issue in each state the list shows. A day's hours depend
  on its date alone, through a xorshift written out in the module rather than
  a crate whose stream may change with a version.
- **It is made anew when it is not today's.** A file in the directory names
  the day and the version that built it; a demo built on another day, or by
  another version, is rebuilt before the command runs, and kasl says so on
  stderr. What a person added goes with it: the demo is for trying things.
  `kasl demo --reset` does the same on request.
- **History is written by SQL of its own.** The commands' writers stamp the
  present moment, and a history needs its dates. What keeps the generator
  from drifting from them is `tests/demo.rs`, which runs every reading
  command on a built demo.
- **What reaches beyond a data directory is closed separately.** The OS
  keyring answers under `lacodda.kasl.demo`; only the demo's own `plugins`
  directory is searched; `watch`, `autostart`, `server`, `self-update` and
  the toast courier are refused, each naming the command to run instead. A
  watcher on the demo would record real activity into invented days, and a
  server connection would put invented days on a real team's dashboard.
- **No banner.** A line saying "this is a demo" on every output would be in
  every screenshot the demo exists to make. The command line says it.

## Consequences

- A new database prints no migration progress: the twenty lines belonged to
  an upgrade, and on a first run - of an installation or of the demo - they
  buried what the command said. An upgrade of an existing database still
  reports itself, on stderr.
- `kasl demo status` always reads `watch off`: no watcher can run there.
- Shell completion does not complete a command after `demo`; the word is not
  part of the grammar clap describes.
- Changing the backlog, the shapes of the days or the number of weeks changes
  what every later demo shows. The tests pin the properties - weekdays only,
  today open, one of each kind of day, one issue in each state, the same demo
  for the same moment - not the figures.
- The documentation's transcripts can be taken from a release build with
  `kasl demo <command>`.
