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
| `flush` | Send every day still waiting to reach the server |
| `queue` | Show the days still waiting, and why |
| `manifest` | Show what this server stores about you |
| `backfill` | Queue every recorded day in a date range and send them |
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
server vX.Y.Z - api v1 - ok
```

Each part can fail on its own, and each failure names its own fix - the server unreachable, the token no longer accepted (revoked, or the account deactivated), or a configured server with no token stored, which means connecting again.

The last line is the pair that decides whether this agent and that server understand each other: the server's own version, and the API version it serves this agent under. Both come from the server rather than being assumed here, so when an upload is refused you can see which two versions are actually talking. Reaching that line at all means they are compatible - the checks above it are what `connect` enforces.

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

**The server could not answer.** It is down, unreachable, or asking for a pause. The day is unchanged locally, and it is queued:

```console
$ kasl server push
Cannot reach kasl-server at https://kasl.example.com: connection refused
The day is unchanged here and stays queued - `kasl server flush` sends it when the server is back.
2026-08-31 is queued and will be sent with the next successful push.
```

Only the second kind is queued. A day the server will never accept as sent is not put in a queue that would retry it forever - it is reported, so you can fix it and push again.

## The queue

A laptop that spends a week off the network still has that week's work. Every day that could not be delivered is remembered, and the next successful push pays the whole debt off - so in normal use the queue is something you never have to think about.

Two properties are worth knowing:

**A queued day is rebuilt when it is sent, not replayed as it was.** The queue holds a date, not a stored copy of the payload. A day you correct while the network is down arrives corrected.

**A backlog goes in one request.** The server takes a batch of days and answers about each one separately, so a single day it will not accept does not strand the rest behind it. Long backlogs are split into batches automatically.

### `kasl server push` carries the backlog

A successful push sends the queue too, on the connection that was just proven to work:

```console
$ kasl server push
Sent 2026-09-03 to the server: 3 pauses, 5 tasks
Sending 4 days...
Sent 2026-08-28 to the server: 2 pauses, 4 tasks
Sent 2026-08-29 to the server: 5 pauses, 6 tasks
Sent 2026-08-30 to the server: 1 pauses, 2 tasks
Sent 2026-08-31 to the server: 4 pauses, 6 tasks
4 sent, 0 refused, 0 still waiting
```

### `kasl server flush`

```bash
kasl server flush
```

Sends what is owed without pushing today - useful from a script, or when you want the backlog gone but today is not finished.

```console
$ kasl server flush
Sending 2 days...
Sent 2026-08-30 to the server: 1 pauses, 2 tasks
2026-08-31 was refused and has been dropped from the queue: tasks[0]: name is empty
1 sent, 1 refused, 0 still waiting
```

A day the server refuses is named and dropped rather than kept: it would be refused identically on every future attempt. A day the server could not answer for stays, and the summary says so.

### `kasl server queue`

```bash
kasl server queue
```

Shows what is still owed, with how many attempts each has taken and what went wrong last time. It does not touch the network, so it answers on a train:

```console
$ kasl server queue
2 days are waiting to be sent:
  2026-08-30 - 3 attempts, last: cannot reach kasl-server at https://kasl.example.com
  2026-08-31 - 1 attempt, last: cannot reach kasl-server at https://kasl.example.com
```

```console
$ kasl server queue
Nothing is waiting to be sent.
```

### `kasl server backfill`

```bash
kasl server backfill [--from <YYYY-MM-DD>] [--to <YYYY-MM-DD>]
```

- `--from <YYYY-MM-DD>`: First date of the range. Without it, the whole history.
- `--to <YYYY-MM-DD>`: Last date; defaults to today.

For history that predates the connection - a machine that tracked locally for months before the team got a server:

```console
$ kasl server backfill --from 2026-08-01 --to 2026-08-31
21 recorded days between 2026-08-01 and 2026-08-31
Sending 21 days...
Sent 2026-08-03 to the server: 2 pauses, 4 tasks
...
21 sent, 0 refused, 0 still waiting
```

With no `--from`, the range opens at the first day this machine ever recorded - which is usually what a machine connecting for the first time actually owes:

```console
$ kasl server backfill
214 recorded days, the whole history from 2025-11-04 to 2026-09-21
Sending 214 days...
...
214 sent, 0 refused, 0 still waiting
```

The first recorded date is named rather than left as "the beginning", because it is the one fact worth reading before a year of your days reaches the server.

Only dates that actually have a workday are queued, so weekends and leave inside the range are skipped rather than queued as days that can never be sent. The days are queued before they are sent, so a run interrupted halfway leaves the rest owed rather than forgotten - running it again, or `flush`, continues where it stopped.

## `kasl server manifest`

```bash
kasl server manifest
```

What this installation stores about you, read from the server itself:

```console
$ kasl server manifest
Privacy level on this server: moderate
This server stores your working hours, when you were interrupted, and the names of tasks you logged - but none of the text you typed about them.
What it stores:
  workdays - the date, when the day started, when it ended, and whether you marked it as leave, sick or a day off
  pauses - each interruption: when it began, how long it lasted, and whether it was a break you entered yourself
  tasks - what you logged: the name and how complete you marked it - not your comment
  account - your email, display name, role, department, and which machines report for you
  live status - whether your agent currently reports you as working, on a break, or not in a day - the latest one only, replaced each time it arrives, never kept as a history
What it never collects:
  keystrokes or what you type
  window titles
  which applications you run
  screenshots or camera images
  web pages you visit
  file names or paths
  your location
Who can see it:
  you, in your own account
  the manager of your department
  administrators of this installation
Retention: Kept for as long as the installation keeps it: there is no automatic deletion. A deactivated account keeps its history rather than losing it.
If the level changes: Changing this setting affects what arrives from now on. Narrowing it does not erase what is already stored, and widening it does not bring back what was dropped.
This level was last set 2026-09-02 11:30.
The level is the installation's, set by an administrator on the server. kasl shows it; it cannot widen or narrow it from here.
```

Every word of that comes from the server. The manifest is generated there from the level the server actually enforces when a day arrives, not written by hand and not kept here - so it describes the installation you report to rather than the one kasl was built against, and it cannot claim a restraint the server does not apply.

### The level is the installation's

kasl shows the level; it does not set it. An administrator chooses it for the whole installation, and there is no personal opt-out - a flag here that appeared to narrow what leaves your machine would be a promise kasl cannot keep, because the filtering happens on the server as the day is written.

What the command is for is the other half of that: if you are asked to run an agent that notices when you stop typing, you can read what it results in from the terminal you already have open, instead of signing into the server that watches you in order to find out what it watches.

### What the levels mean

| Level | What the server keeps |
| --- | --- |
| `full` | Your hours, every interruption with the reason you gave, and your tasks with their comments |
| `moderate` | Your hours, when you were interrupted, and task names - but none of the text you typed |
| `coarse` | Your hours and how much of the day you were away - not when, and not what you worked on |

Narrowing happens at ingest, not on a screen: a field a level excludes is dropped before the day is written, so it never reaches the server's database or its backups.

## Which server this agent needs

kasl and kasl-server ship on their own schedules, so the version of the server
on the other end is not something this machine chooses. Each subcommand below
names the endpoint it calls and the server version that first answered it.

<!-- historical versions -->

| From kasl | Subcommands | Endpoint | Needs kasl-server |
| --- | --- | --- | --- |
| v1.7.0 | `connect`, `status`, `disconnect` | `GET /health`, `GET /api/v1/agent/whoami` | **0.14.1** |
| v1.8.0 | `push` | `POST /api/v1/days` | **0.14.1** |
| v1.9.0 | `queue`, `flush`, `backfill` | `POST /api/v1/days/batch` | **0.14.1** |
| v1.13.0 | `manifest` | `GET /api/v1/privacy/agent` | **0.14.1** |

The floor is the same for all of them, and `whoami` is what sets it. The server
has accepted uploads since 0.3.0, backlogs since 0.4.0 and the agent privacy
manifest since 0.10.0, but `connect` asks whose token it is holding before it
stores anything, and 0.14.1 is where that question could first be answered. A server older than that cannot be connected
to at all, which is the honest outcome: the alternative would be filing this
machine's days under a name nobody checked.

<!-- /historical versions -->

### On a server that is too old

`connect` reports the refusal and stores nothing. The server answers an unknown
`/api` path with `{"error":"no such endpoint"}` and `404`, so the failure names
the endpoint rather than arriving as a hang or as HTML read back as success.

A `404` counts as a rejection, not a temporary fault, so days are not queued
against a server that will never take them - see [when a push
fails](#when-a-push-fails).

### On a newer server

Always safe. `/api/v1` keeps its meaning for as long as agents call it; a change
that would alter it arrives as `/api/v2` with a migration written for agents.
Everything the server has grown since - departments, the audit log, signals, the
month heatmap - is read by people in its web UI and changes nothing this agent
sends.

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

# See what is still owed, without touching the network
kasl server queue

# Send everything that is waiting
kasl server flush

# Read what this server stores about you
kasl server manifest

# Upload history recorded before this machine was connected
kasl server backfill --from 2026-08-01

# Upload everything this machine has ever recorded
kasl server backfill

# Forget the connection on a machine being handed on
kasl server disconnect
```

## Related commands

- **[`report`](/reference/report/)** - The daily report, which the corporate `si` integration submits separately
- **[`setup`](/reference/setup/)** - Configure the rest of kasl, including that separate reporting API
