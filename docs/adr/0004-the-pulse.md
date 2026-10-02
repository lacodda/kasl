# ADR 0004: The pulse - one sender, consent per connection, one row

- Status: accepted
- Date: 2026-10-02

## Context

kasl-server 0.17.0 added `POST /api/v1/agent/heartbeat` (its ADR 0014): an agent claims `working`, `paused` or `idle`, the server ages the claim by its own clock and shows the team who is working right now. The claim has to come from kasl - only the watcher knows whether this is a pause and whether today's workday is open - and nothing in kasl sent it.

Three questions had to be settled before the code:

1. **Who agrees to it.** Connecting to a server already agrees to send finished days. A live signal of whether someone is at their keyboard this minute is a different thing to agree to, and the server's privacy level governs days, not the pulse.
2. **Who sends it.** The watcher knows the pause; a CLI command does not. A command that sent a pulse of its own - say, to check the route works when the pulse is turned on - would read the pause from the pauses table, where a pause left open by a watcher killed mid-break stays open long after the person came back, and the server would hear two versions of the present.
3. **How a different process learns how it is going.** `kasl server status` runs in a different process from the watcher and has to say when the last pulse went, and why the last one did not.

## Decision

**Opt-in, stored with the connection.** `KaslServerConfig.pulse` defaults to `false` and only `kasl server pulse enable` sets it. Being part of the connection, it goes with `disconnect`, and `connect` carries it over only when reconnecting to the same URL - consent given to one installation is not given to the next.

**The watcher is the only sender.** `libs::pulse::run` is a sibling task of the monitor in both daemon and `--foreground` modes, like the Jira inbox poller. The monitor exposes `pause_flag()`, an `Arc<AtomicBool>` it keeps in step with its own state; the pulse combines it with today's workday (open, closed, absent) into the claim. Every five seconds it re-reads the config and the claim, and sends when the server's interval is up or the claim changed. A failed attempt holds changes back until its wait is over: the next interval when the server could not answer, five minutes when it refused. `enable` sends nothing - it clears the record, which the watcher takes as its cue to send at once, and reads back what the watcher wrote.

**The server owns the cadence.** Every answer carries `interval_seconds` and `stale_after_seconds`; kasl takes both, the interval bounded to 10 s - 15 min so a wrong answer cannot become a busy loop. There is no setting for it.

**One row, never a log.** Migration 16 adds `server_pulse` with `CHECK (id = 1)`: the latest attempt, its next one, the last accepted pulse and its state, the server's staleness and the clock skew, and the last error. Times are UTC, because the only question asked of them is "how long ago". `next_at` is what lets `status` tell a quiet watcher from a missing one - a watcher that has stopped leaves it behind - without depending on the PID file, which a `--foreground` watcher does not write.

**The body is the state and the moment.** `Pulse { state, at }` and nothing else. The agent's label is the token's, so it is not repeated in the body.

## Consequences

- Turning the pulse on or off takes effect within a tick of the running watcher, with no restart and no second channel into it: the config file is the channel.
- Turning it off sends nothing, because the server has no "stop" - it goes by silence and shows the person offline once the last pulse is older than `stale_after_seconds`. `disable` says so rather than letting the dashboard surprise anyone.
- A server between 0.14.1 and 0.17.0 accepts the connection and answers the pulse with `404`, which is reported as a server too old for the pulse and asked again every five minutes. Days are unaffected.
- The pulse needs the agent token from the keyring at every attempt (`Secret::try_get_cached`), so a missing token is a recorded refusal rather than a prompt, like every other unattended path (ADR 0003).
- The pulse's answer also carries `notifications`, the count the server's notices feature uses. It is not read here; showing those notices is a later milestone, and an unread field costs nothing.
