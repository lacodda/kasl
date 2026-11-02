# ADR 0010: GitLab leaves the core for a plugin that ships with kasl

- Status: accepted
- Date: 2026-11-02

## Context

Before v1.20 GitLab lived inside kasl twice over in waiting: `task find` read the day's commits through kasl's own client (`api::gitlab`, a `gitlab` block in `config.json` with the token in plain text), and the plan called for GitLab's assigned issues as the first inbox source of the plugin protocol (ADR 0009). Building the source as a plugin while the commit search stayed in the core would have left two GitLab integrations with two copies of the same address and token - one in the config file, one in the keyring - that could disagree.

Being a plugin also had to cost the person nothing: an integration that has to be found, downloaded and placed by hand is one most people never get to. And GitLab's keys are not Jira's: `group/app#14` and `group/app!31` carry `#`, `!` and `/`, which kasl's toast buttons could not put in a shortcut's file name.

## Decision

**GitLab is a plugin, whole.** [kasl-plugin-gitlab](https://github.com/lacodda/kasl-plugin-gitlab) is an inbox source (`list_issues`, `transition`, `comment`) and answers a new request, `find_tasks`, with the day's commits. `api::gitlab`, the `gitlab` config block and the GitLab step of `kasl setup` are gone; there is one GitLab address and one token, the plugin's.

**`find_tasks` is a request of its own.** The subject is the day, as `kasl report --json` prints it; the reply is `tasks` (each a `name` and an optional `ref`) and an optional `message`. `tasks` is required - a plugin that found nothing says `[]` - and a task without a name is refused. It does not make a plugin an inbox source: only `transition` and `comment` need `list_issues` beside them. `task find` asks every plugin that answers it, side by side, and one that fails costs its own suggestions only.

**The plugin ships in the box.** The release archive carries the plugin beside the binary, at a version pinned in `release.yml` and checked there against the packed kasl (`plugin list --json`, no problem reported). The installers, the npm package and `self-update` put it beside kasl, and kasl looks for plugins in its plugins directory, then **beside its own executable**, then on `PATH` - so the bundled plugin works however kasl was installed, and one a person put in the plugins directory wins over it. Until `kasl plugin setup gitlab` gives it an address and a token it is not called, as ADR 0009 already promised for a bundled plugin.

**The old block moves by itself.** `Config::read` takes a `gitlab` block from an older file, stores its token in the keyring as `plugin.gitlab.token` and its address as `plugins.gitlab.settings.url`, and saves the file without it. What the plugin was already told wins; a keyring that refuses the token leaves the file as it was, to move on a later read, so the token is never lost.

**One rule for a key, checked where it enters.** A key is ASCII letters, digits and `- _ . / # ! : @ + ~`, at most 200 characters (`libs::issue_key`): nothing a command line, a tab-separated mailbox line or a URI reads as structure. A source listing anything else is refused whole, like any listing kasl cannot read. A toast's shortcut writes the key out in its file name (`_` and two hex digits for every character but letters, digits and `-`), since `#` would end the `file:` URI the button launches, and passes it as it is on the courier's command line.

## Consequences

A person who used `task find` with GitLab before v1.20 keeps it with no step of their own, provided the plugin came with their kasl (the archive, the installers, npm). One who installed with `cargo install kasl-cli` gets no plugin, since crates.io carries only the binary; the plugin's own release is where it comes from then.

GitLab's code now has its own repository, releases and tests, and a kasl release names the plugin release it carries. A protocol change that a plugin release has not caught up with fails kasl's release, not a person's machine.

The same path is open for Jira (v2.11): `find_tasks` is where its resolved issues would go, and the key rule already admits its keys.
