---
title: "API Integrations"
---

kasl talks to three kinds of external service: GitLab and Jira supply candidate tasks, and a corporate reporting API receives the finished day. All three are optional - kasl records the workday without any of them.

## What each integration does

| Service | Direction | What it is for |
| --- | --- | --- |
| GitLab | reads | Today's commits become candidate tasks in `kasl task find` |
| Jira | reads | Resolved issues become candidates; assigned open issues feed `kasl inbox` |
| SiServer | writes | `kasl report --send` and `kasl sum --send` file the day and the month |

Each is configured through the `kasl setup` wizard. Logins and URLs go to
`config.json`; passwords go to the OS keyring. API tokens are the exception -
see the note under GitLab.

## GitLab

Imports the commits you pushed today, so the tasks you log come from what you
actually did rather than from memory.

### Setup

1. In GitLab, open **User Settings → Access Tokens** and create a token with the
   `read_user` and `read_api` scopes.
2. Run `kasl setup` and follow the GitLab prompts.

### Configuration

```json
{
  "gitlab": {
    "access_token": "glpat-XXXXXXXXXXXXXXXXXXXX",
    "api_url": "https://gitlab.com"
  }
}
```

:::caution[The GitLab token is stored in the config file]
Unlike the Jira and SiServer passwords, which go to the OS keyring, the GitLab
personal access token is written to `config.json` in plain text. Give the token
the narrowest scopes that work, and keep the file readable only by you.
:::

### How it finds your commits

kasl reads your user id, asks for your push events of the day, and resolves each
push to the commits it carried. Merge commits and commits already logged as
tasks are filtered out, so `kasl task find` offers each piece of work once.

```bash
kasl task find
```

## Jira

Two separate features read from Jira: `task find` offers issues you resolved
today as completed tasks, and the [inbox](/reference/inbox/) polls issues
assigned to you that are still open.

### Setup

1. Have your Jira login (the username, which is not always the email address)
   and the instance URL at hand.
2. Run `kasl setup` and follow the Jira prompts. The password is asked for once
   and stored in the keyring.

### Configuration

```json
{
  "jira": {
    "login": "john.doe",
    "api_url": "https://jira.company.com"
  }
}
```

Inbox polling has its own block - see [Configuration](/concepts/configuration/)
for `jira_inbox`.

### Authentication

kasl authenticates once and caches the resulting session id in the data
directory as `.jira_session_id`. Subsequent runs reuse it; when the server
rejects it the file is dropped and the login repeats. The password itself is
never written to disk.

## SiServer

Submits the daily report and the monthly summary to a corporate reporting
system. This integration is specific to the deployment kasl was originally
written for; the endpoint paths come from your organization and are configured,
not hard-coded into the docs.

### Setup

Run `kasl setup` and answer the SiServer prompts: login, authentication URL and
API URL. The password goes to the keyring.

### Configuration

```json
{
  "si": {
    "login": "john.doe@company.com",
    "auth_url": "https://auth.company.example",
    "api_url": "https://api.company.example"
  }
}
```

### Usage

```bash
kasl report --send
kasl sum --send
```

Authentication is two-staged: kasl signs in against the LDAP endpoint, exchanges
the result for a bearer token, and caches the session id as `.si_session_id`.
The same session also supplies the company's rest days, which is why `kasl sum`
can tell a holiday from a day you did not work.

## Credentials

Since 1.0, the passwords you type at a prompt live in the operating system
keyring:

- **Windows** - Credential Manager
- **macOS** - Keychain
- **Linux** - Secret Service (GNOME Keyring, KWallet, and compatible)

Nothing is encrypted with a key compiled into the binary any more - that scheme
was removed rather than improved, because a key shipped inside a public release
protects nothing. Installations that predate 1.0 still carry the old AES files
next to the config; they are read once, migrated into the keyring and left alone
afterwards. See [ADR 0001](https://github.com/lacodda/kasl/blob/main/docs/adr/0001-os-keyring.md).

Two values are not passwords and stay in `config.json` as written: the GitLab
`access_token` and the reporting server's `auth_token`.

### What is on disk

The data directory holds the database, the config and the cached session ids:

- **Windows**: `%LOCALAPPDATA%\lacodda\kasl\`
- **macOS**: `~/Library/Application Support/lacodda/kasl/`
- **Linux**: `~/.local/share/lacodda/kasl/`

Session ids are `.jira_session_id` and `.si_session_id`. GitLab has none - it
authenticates with the token on every request.

### When authentication fails

A wrong password is re-prompted up to three times before kasl gives up; a
rejected session id is discarded and the login is retried once. Neither is a
network backoff - a server that is down fails the command rather than being
retried in a loop.

To force a fresh login, delete the cached session id:

```bash
# Linux and macOS
rm ~/.local/share/lacodda/kasl/.jira_session_id
```

To replace a stored password, run `kasl setup` again.

## Debugging

```bash
RUST_LOG=kasl=debug kasl task find
```

Debug logging names the requests kasl makes and the decisions it takes on the
responses. It does not print credentials.

## Related pages

- [`task`](/reference/task/) - `task find` is where GitLab and Jira candidates appear
- [`inbox`](/reference/inbox/) - the Jira inbox and its polling
- [`report`](/reference/report/) - filing the day with `--send`
- [Configuration](/concepts/configuration/) - every configuration block in one place
