---
title: "completions"
---

Prints a shell completion script for kasl on stdout. Source it from your shell profile to get tab-completion for commands, subcommands, and flags.

## Usage

```bash
kasl completions <SHELL>
```

`<SHELL>` is one of `bash`, `zsh`, `fish`, `powershell`, or `elvish`.

## Setup

### Bash

```bash
kasl completions bash > ~/.local/share/bash-completion/completions/kasl
```

Or source it directly from `~/.bashrc`:

```bash
eval "$(kasl completions bash)"
```

### Zsh

```bash
kasl completions zsh > ~/.zfunc/_kasl
```

Make sure `~/.zfunc` is on your `fpath` before `compinit` runs in `~/.zshrc`:

```bash
fpath=(~/.zfunc $fpath)
autoload -Uz compinit && compinit
```

### Fish

```bash
kasl completions fish > ~/.config/fish/completions/kasl.fish
```

### PowerShell

Append to your profile (`$PROFILE`):

```powershell
kasl completions powershell | Out-String | Invoke-Expression
```

To make it permanent:

```powershell
kasl completions powershell >> $PROFILE
```

### Elvish

```bash
kasl completions elvish > ~/.elvish/lib/kasl.elv
```

## Notes

The script is generated from kasl's own command definitions, so it always matches the installed version. Regenerate it after updating kasl if you wrote the output to a file rather than sourcing it dynamically.
