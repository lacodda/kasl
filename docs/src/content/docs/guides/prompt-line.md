---
title: "The day in your prompt"
description: Put today's work, a pause in progress, a watcher that is not running and the inbox into a shell prompt or a status bar with kasl status.
sidebar:
  order: 3
---

How long have I worked today? Is kasl still recording? Did something new land
in the inbox? `kasl status` answers all three in one line, made for a prompt:

```text
6h 40m | paused 12m | inbox 9 (3 new)
```

`watch off` takes the place of the pause when no watcher is running - the
segment that matters most, because then nothing is being recorded. The whole
format is on the [`status`](/reference/status/#the-line) page.

The command reads the local database and the watcher's PID file only - never
the network, never an update check - and prints nothing at all when there is
nothing to say, so the segment simply disappears.

## starship

In `~/.config/starship.toml`:

```toml
[custom.kasl]
command = "kasl status"
when = true
format = "([$output]($style) )"
style = "bold green"
```

The parentheses make the group conditional: with nothing printed, not even the
space after it is left. On Windows add `shell = ["cmd", "/C"]`, so starship does
not start a PowerShell for every prompt just to run one command.

## tmux

In `~/.tmux.conf`:

```bash
set -g status-right '#(kasl status) '
set -g status-interval 30
```

## PowerShell

In your profile (`$PROFILE`), around your existing prompt:

```powershell
$KaslPrompt = $function:prompt
function prompt {
    $line = kasl status
    if ($line) { Write-Host "[$line] " -NoNewline -ForegroundColor DarkGreen }
    & $KaslPrompt
}
```

The line is plain ASCII, so it reads the same whatever code page the console
is in.

## bash and zsh

```bash
# ~/.bashrc
PS1='$(s=$(kasl status); [ -n "$s" ] && printf "[%s] " "$s")'"$PS1"
```

```zsh
# ~/.zshrc
setopt PROMPT_SUBST
PROMPT='$(s=$(kasl status); [[ -n $s ]] && print -n "[$s] ")'"$PROMPT"
```

## A status bar that wants more

A bar that draws its own pieces - waybar, polybar, a Rainmeter skin - reads
`kasl status --json` instead and picks the fields it needs: `work_seconds`,
`watcher`, `state`, `paused_since`, `inbox.fresh`. A bar that only shows text
can take `line` from the same document. See
[JSON output](/reference/status/#json-output).

```bash
# waybar: a custom module's exec
kasl status --json | jq -c '{text: .line, class: (.state // "off")}'
```

## Related pages

- [`status`](/reference/status/) - The line, field by field
- [`watch`](/reference/watch/) - The watcher the line keeps an eye on
- [kasl in scripts and CI](/guides/scripting/) - The `--json` conventions
