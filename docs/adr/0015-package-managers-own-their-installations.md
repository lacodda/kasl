# ADR 0015: Package managers own their installations, and Windows ships a zip as well

- Status: accepted
- Date: 2026-11-20

## Context

Until 1.24 kasl reached a machine five ways: the two one-line installers,
npm, cargo and the release archive by hand. Each of them leaves a kasl that
`kasl self-update` looks after: it downloads the next release's tar.gz and
swaps the binary - and the plugins that ship with it - in place.

1.24 adds Scoop, Homebrew and winget. They are different in kind: each keeps
a record of what it installed and at which version, and updates by that
record. Three things follow from that, and each was a defect waiting to
happen.

- **The record.** A `self-update` that swaps the file behind Scoop's back
  leaves Scoop saying the old version is installed; the next `scoop update`
  works from a lie, and Homebrew's Cellar keeps a directory named after a
  version that is no longer in it.
- **The link.** winget starts a portable package through a symbolic link in
  `WinGet\Links`, Homebrew through one in its `bin`. `std::env::current_exe`
  returns that link, not the file - checked on Windows 11 with a link to a
  probe program; macOS does not resolve it either - so "beside the running
  executable", where kasl looks for its plugins, is the link's directory, and
  the plugin unpacked beside the real file is not found there.
- **The version in the path.** Linux reports the running executable resolved,
  and under Homebrew that is `Cellar/kasl/<version>/bin/kasl`. Autostart wrote
  that path into the systemd unit, and the next `brew upgrade` deletes it.

And one constraint: winget installs from no archive but a zip, and kasl has
shipped tar.gz on every platform. The tar.gz cannot go: the self-update and
the npm wrapper of every released version ask for it.

## Decision

- **A package manager's installation is updated by the package manager.**
  `libs::install` reads the manager from the path kasl was started by and the
  file it resolves to - `scoop/apps/kasl/`, `Cellar/kasl/`, `WinGet/Packages`
  or `WinGet/Links`. There `kasl self-update` names the manager's command and
  changes nothing, and the notice of a newer release names the same command.
  The installers', npm's and cargo's installations are kasl's to update, as
  before.
- **"Beside kasl" means beside both.** Plugin discovery searches the directory
  of the path kasl was started by and the directory of the file it resolves
  to.
- **Autostart names a path that outlives an upgrade.** A path through the
  Cellar becomes the one through Homebrew's `opt/kasl` link.
- **Windows ships a zip as well as the tar.gz**, packed from the same
  `kasl-<tag>-<target>/` directory. winget and the Scoop bucket install the
  zip; everything else goes on taking the tar.gz.
- **Each manager installs the whole box.** Scoop unpacks the archive into the
  app's directory, so the plugin is beside `kasl.exe`; the Homebrew tap's
  generator learnt a `bundled` list and installs `kasl-plugin-gitlab` into
  `bin` with kasl; the winget manifest names `kasl.exe` as the portable
  command, and the plugin stays beside it in the package directory.

## Consequences

- `scoop update kasl`, `brew upgrade kasl` and `winget upgrade lacodda.kasl`
  are the only way an installation of theirs moves; `self-update` says so
  instead of half-doing it.
- The release page carries a second Windows archive of the same size.
- A manager added later is one more arm in `Manager::detect`, with a test of
  its paths; an unrecognised path stays kasl's own, which is the old
  behaviour, not a new failure.
