---
title: "server"
---

The `server` command connects this machine to a [kasl-server](https://github.com/lacodda/kasl-server) - the team server that collects workdays from everyone's agent and gives a manager a dashboard.

Connecting is optional. kasl works entirely on its own, and nothing in this command changes that: the local database stays the record of your day either way.

## Usage

```bash
kasl server <SUBCOMMAND>
```

| Subcommand | Purpose |
| --- | --- |
| `connect` | Connect this machine with an agent token |
| `status` | Show the connection and whether it still works |
| `push` | Send a day's work to the server |
| `disconnect` | Forget the connection and remove the stored token |

## Getting a token

You do not issue your own token. An administrator creates one on the server - under the person it reports for, labelled with the machine it belongs to - and hands it over. The server shows a token once, at issue, and stores only its hash afterwards, so a lost token is replaced rather than recovered.

## `kasl server connect`

```bash
kasl server connect [OPTIONS]
```

- `--url <URL>`: Server address, e.g. `https://kasl.example.com`. Prompted for when omitted.
- `--ca-certificate <PATH>`: PEM file holding the certificate authority that signed the server's certificate.

The token is always asked for at the prompt, never taken from an argument: a token on a command line ends up in shell history.

```console
$ kasl server connect
kasl-server URL: https://kasl.example.com
https://kasl.example.com is kasl-server vX.Y.Z
Agent token (issued by your administrator): ****************
Connected as Kirill Lakhtachev (agent 'laptop')
```

### What it checks, and why in that order

**First, that the address is a kasl-server.** A URL is asked about before the token is, so a typo in the address is reported as a typo rather than as a rejected token - two different problems with two different fixes.

**Then, whose token it is.** The server is asked who it thinks is connecting, and the answer is printed. A token is an opaque string; one pasted from the wrong chat window works perfectly and quietly files this machine's days under a colleague's name. The check costs one request and puts the mistake in front of a person while they are still looking.

Nothing is written until both checks pass. A half-written connection - an address saved with a token the server never accepted - would leave the agent looking configured while every upload failed.

### Where the token is kept

In the operating system's credential store: Credential Manager on Windows, Keychain on macOS, the Secret Service on Linux - the same place as every other kasl credential (see [ADR 0001](https://github.com/lacodda/kasl/blob/main/docs/adr/0001-os-keyring.md)). The config file records the address only, so it stays safe to read, copy between machines, and paste into a bug report.

### Self-hosted certificates

A company kasl-server is routinely behind a private CA or a self-signed certificate, which the system trust store does not know about. Name the CA that signed it:

```bash
kasl server connect --url https://kasl.internal --ca-certificate /etc/ssl/company-ca.pem
```

The certificate is *added* to the trust store rather than replacing it, so public certificates keep working. There is deliberately no option to turn verification off: that would also accept anyone else's certificate, on a connection carrying a bearer token.

The scheme is never guessed. `http://` and `https://` differ by whether the token crosses the network in the clear, which is not a default worth inventing on your behalf.

## `kasl server status`

```bash
kasl server status
```

Reaches the server rather than reading the config back, because what you run this to find out is whether the connection still works:

```console
$ kasl server status
Configured server: https://kasl.example.com
https://kasl.example.com is kasl-server vX.Y.Z
Connected as Kirill Lakhtachev (agent 'laptop')
```

Each part can fail on its own, and each failure names its own fix - the server unreachable, the token no longer accepted (revoked, or the account deactivated), or a configured server with no token stored, which means connecting again.

## `kasl server push`

```bash
kasl server push [OPTIONS]
```

- `--last`, `-l`: Send yesterday instead of today.
- `--date <YYYY-MM-DD>`: Send a specific day.

Sends one day - its start and end, its pauses, and its tasks - to the connected server.

```console
$ kasl server push
Sent 2026-08-31 to the server: 4 pauses, 6 tasks
```

Nothing is sent for a day that was never started, which is the honest answer rather than an error:

```console
$ kasl server push --last
No workday recorded for 2026-08-30 - nothing to send.
```

### Sending the same day twice is safe

The server stores a day as a unit, and the last upload wins. Pushing an unchanged day leaves the server exactly as it was; pushing a day you have since corrected - a task renamed, a break added with [`pauses add`](/reference/pauses/) - replaces the stored copy with the corrected one. There is no separate "update" to remember.

That is also why a failed push costs nothing: whatever went wrong, the day is still here, and sending it again is the whole fix.

### A day is sent whole, including its deletions

Every push says "this is everything I hold for this date". A task you deleted here is therefore deleted there too, and the count is printed when it happens:

```console
$ kasl server push
Sent 2026-08-31 to the server: 4 pauses, 6 tasks
1 task(s) removed on the server - deleted here since the last upload
```

The deletion is scoped to the date being sent. A task carried across several days keeps its copy on the others, so pushing an old day cannot erase work recorded on a different one.

### Time zones

Every timestamp is sent with this machine's UTC offset, and the day carries its own calendar date rather than one derived from the clock. Both matter once a team spans time zones: bare wall-clock time from two countries cannot be compared, and near midnight the date of an instant and the date the work belongs to disagree.

The offset is taken per timestamp, so a day spanning a daylight-saving change keeps both of its halves right. The two wall-clock readings that have no single offset - the hour skipped when clocks go forward, the hour repeated when they go back - are refused by name instead of guessed, because either guess would silently move an hour of work.

### When a push fails

The two kinds of failure need different things from you, so they are reported differently.

**The server refused the day.** The payload will not be accepted as sent, and sending it again will not help. The server's own explanation is printed - which task, which field - so it can be fixed here and pushed again. A revoked token lands here too: the fix is [`kasl server connect`](#kasl-server-connect), not another attempt.

**The server could not answer.** It is down, unreachable, or asking for a pause. The day is unchanged locally and worth sending later.

Retrying is manual for now; an offline queue that holds days and delivers them when the server returns is the next step.

## `kasl server disconnect`

```bash
kasl server disconnect
```

Removes the stored token, then the configured address - in that order, so a failure never leaves a working credential behind with nothing pointing at it. Disconnecting does not revoke the token on the server; ask an administrator to revoke it if the machine is being handed on.

Local data is untouched. Days already uploaded stay on the server, and everything in the local database stays where it is.

## Examples

```bash
# Connect, answering the prompts
kasl server connect

# Connect to a known address without the URL prompt
kasl server connect --url https://kasl.example.com

# Connect to a server behind a company CA
kasl server connect --url https://kasl.internal --ca-certificate /etc/ssl/company-ca.pem

# Check where the connection stands
kasl server status

# Send today's work
kasl server push

# Send yesterday, the morning after
kasl server push --last

# Send one particular day
kasl server push --date 2026-08-24

# Forget the connection on a machine being handed on
kasl server disconnect
```

## Related commands

- **[`report`](/reference/report/)** - The daily report, which the corporate `si` integration submits separately
- **[`setup`](/reference/setup/)** - Configure the rest of kasl, including that separate reporting API
