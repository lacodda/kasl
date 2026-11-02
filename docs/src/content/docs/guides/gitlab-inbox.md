---
title: "Working with the GitLab inbox"
sidebar:
  order: 2
---

GitLab work waits for you in three places: issues assigned to you, merge
requests assigned to you, and merge requests you are asked to review. The
GitLab plugin brings all three into kasl's [inbox](/reference/inbox/), next to
Jira's issues if you have those too, and offers the commits you pushed to
[`kasl task find`](/reference/task/), so the day's tasks come from what you
actually did.

## It comes with kasl

The plugin is [kasl-plugin-gitlab](https://github.com/lacodda/kasl-plugin-gitlab),
and it ships in the box: the release archive carries it beside the kasl binary,
and the installers, the npm package and `kasl self-update` put it there and
keep it at the version the release was built with. kasl finds it beside itself
whether or not that directory is on your `PATH`:

```bash
kasl plugin list
```

```
gitlab X.Y.Z - GitLab issues and merge requests waiting on you, and your commits of the day
  needs `kasl plugin setup gitlab`: url, token
  inbox    a source: list_issues, transition, comment
  tasks    offers tasks to `kasl task find`
  setting  url - GitLab address (required, not set)
  setting  token - Access token (required, secret, not set)
  setting  start_label - Label that marks work started
  at C:\Users\me\AppData\Local\Programs\kasl\kasl-plugin-gitlab.exe
```

Until it is set up, kasl does not call it. A kasl installed with
`cargo install kasl-cli` has no plugin - crates.io carries only the binary -
so take the archive for your system from the
[plugin's releases](https://github.com/lacodda/kasl-plugin-gitlab/releases)
and put the program in kasl's [plugins directory](/reference/plugin/#where-kasl-looks).

## Setting it up

1. In GitLab, open **User Settings → Access Tokens** and create a personal
   access token. With the `api` scope kasl can also start, close and comment;
   `read_api` is enough to read.
2. Give the plugin the address and the token:

```bash
kasl plugin setup gitlab
```

| Setting | What to give it |
| --- | --- |
| `url` | The instance's root: `https://gitlab.com`, or your company's GitLab. A trailing `/` or `/api/v4` is taken off. |
| `token` | The token. It is kept in the OS keyring as `plugin.gitlab.token`, never in `config.json`. |
| `start_label` | Optional: the label your team puts on work in progress, such as `Doing`. See [Starting](#starting). |

A config written by an older kasl, with GitLab in a `gitlab` block of its own,
needs no setup: on the first read kasl moves the address to the plugin's
settings and the token to the keyring, and saves the file without the block.

The next poll of `kasl watch` - or `kasl inbox sync` right away - fills the
inbox.

## What lands in the inbox

```bash
kasl inbox sync
kasl inbox
```

```
[✓] gitlab synced: 4 fetched, 4 new, 0 changed, 0 gone.

Inbox:

+---+--------+-------+----------+--------------+--------+-----------------------------------+
| ★ | CHANGE | SCORE | PRIORITY | KEY          | STATUS | SUMMARY                           |
+---+--------+-------+----------+--------------+--------+-----------------------------------+
|   | NEW    | —     | —        | group/app#14 | Open   | Login fails on an expired session |
|   | NEW    | —     | —        | group/app#15 | Open   | Write the release notes           |
|   | NEW    | —     | —        | group/app!31 | Open   | Fix the expired session           |
|   | NEW    | —     | —        | group/lib!2  | Draft  | Bump the client                   |
+---+--------+-------+----------+--------------+--------+-----------------------------------+
```

- **Keys are GitLab's own references**: `group/app#14` is an issue,
  `group/app!31` a merge request. Pasted into a GitLab comment, they link.
  In bash and zsh, quote a merge request's key - `!31` is history expansion
  there: `kasl inbox take 'group/app!31'`.
- **Type** says which kind it is: the issue's own type (`Issue`, `Incident`,
  `Task`), `Merge request` for one assigned to you, `Review` for one you are
  asked to review. A merge request that is both is listed once, as assigned.
- **Status** is `Open`, `Draft` for a draft merge request, or your
  `start_label` when the work carries it.
- GitLab has no priority or score, so those columns stay empty, and a rule
  on them finds nothing in GitLab's issues.

Everything else is the inbox's and works as it does for Jira: badges, toasts
with their buttons, snoozes, triage, `take`. [Rules](/reference/inbox/#rules)
read `type`, `status` and `project` - to keep review requests out of the way,
dismiss them by type:

```json
{ "name": "no reviews", "when": { "type": "Review" }, "then": { "dismiss": true } }
```

An issue that is closed, or no longer assigned to you, is `gone` at the next
poll.

## Starting

GitLab has no "in progress" of its own; teams mark it with a label. Starting
an issue or a merge request from kasl puts that label on it:

```bash
kasl inbox start 'group/app#14' --to Doing
```

```
[✓] group/app#14 is now Doing in gitlab.
```

Without `--to`, the plugin uses `start_label`; with neither, it refuses and
says so - it does not guess which label your team means. The label has to
exist in the project or its groups: GitLab would create an unknown one on the
spot, so a typo is refused instead of leaving a new label for the whole team.
With `inbox.start_on_take`, taking an issue starts it the same way.

## Closing and commenting

```bash
kasl inbox close 'group/app#14' -m 'Fixed in group/app!31'
kasl inbox comment 'group/app!31' 'Looks good to me'
```

`close` closes an issue, then leaves the comment, if you gave one. A merge
request is refused: it is finished by merging it, and closing it unmerged
would throw the work away. GitLab keeps no resolution, so `--resolution` is
refused too, rather than dropped. `comment` works on both kinds.

## Commits in `task find`

`kasl task find` asks the plugin what you pushed on the day: each push is
opened into its commits, and the ones you wrote that day are offered by their
first line, with the short SHA beside them:

```
Found: 1 incomplete, 2 gitlab
```

```
● Refresh the session before it expires (c1a2b3c)
● Start the release notes (d1e2f3a)
```

Merge commits fall to `task_discovery.ignore_names` like any other name. A push
that could not be read - a project the token cannot see any more - is named in
a warning rather than skipped in silence.

## When it does not work

- `kasl plugin list` shows whether the plugin is found, set up, or why it
  cannot be used.
- **401** - GitLab refused the token: give a new one with `kasl plugin setup gitlab`.
- **403** - the token lacks a scope: `api` to change issues, `read_api` to read.
- A company GitLab behind its own certificate authority is reached the way
  kasl reaches anything else: through the system's trust store.

## Related pages

- [`inbox`](/reference/inbox/) - the list, its commands and its rules
- [`plugin`](/reference/plugin/) - list, set up and run plugins
- [`task`](/reference/task/) - `task find`, where the commits are offered
- [Working with the Jira inbox](/guides/jira-inbox/) - the other source kasl has
- [Writing a plugin](/guides/writing-a-plugin/) - the protocol the GitLab plugin speaks
