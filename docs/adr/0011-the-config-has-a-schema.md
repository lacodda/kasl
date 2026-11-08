# ADR 0011: The config has a schema, generated from its types, and is edited one key at a time

- Status: accepted
- Date: 2026-11-08

## Context

Before v1.21 the config file had two ways in: `kasl setup`, which walks whole modules with prompts and needs a terminal, and an editor. Changing one threshold meant either answering every prompt of its module or opening the JSON by hand - and by hand, a misspelt key was simply ignored: the file is read leniently on purpose, because the watcher reads it too and a typo must not stop the workday being recorded. A lenient reader is exactly what lets a mistake go unsaid.

The file also carried things nothing read: a `server` block (an API URL and a token in plain text, offered by the wizard and used by no code) and `jira.completed_statuses`. And a module block had to be whole - a `monitor` block naming one setting failed to parse, which failed every command, the watcher included.

## Decision

**The schema is generated from the config types.** `schemars` derives a JSON Schema from the same structs serde reads the file into, and one transform shapes it into the schema the file is held to: no `null` (an unset setting is a missing key), no unknown keys anywhere (one rule, so a new struct cannot forget it), unit enums as `enum`. Inbox rules and presets are kept as raw JSON in the struct - a bad one is skipped, not fatal - and given their real types in the schema only. The schema is printed by `kasl config schema`, published at `https://kasl.lacodda.com/config.schema.json` from `docs/public/config.schema.json` (a test holds that copy equal to the generated one), and named in `$schema` by every file kasl writes, so an editor checks the file as it is typed.

**One validator, the standard one.** `jsonschema` validates against that schema; it is what hilvan uses, and writing a second subset validator would be a second opinion about the same schema. What the schema cannot express - a rule that does nothing, a window that is not `7d` - is checked by the same functions that read those values at run time, on the parts the schema passed, so one mistake is said once.

**Every block may be partial.** The defaultable modules take `#[serde(default)]` at the struct level, so a block names only what was changed and takes the rest from the module's defaults.

**`kasl config` edits the JSON, not the struct.** `set` and `unset` change the file's JSON at one key and write it back with `$schema`, rather than round-tripping through `Config`, which would write out every default of every block it touched - a default written into the file reads as "set" and stops following kasl when the default changes. An edit is refused when it would bring in a problem the file did not have; problems already elsewhere do not block it. The blocks a command owns are refused: `kasl_server` (connecting checks the server first; the pulse is a consent) and `plugins` (only the plugin's manifest knows which of its settings are secret).

**Retired keys are notices, then gone.** `server` and `jira.completed_statuses` left the types. A file that has them still reads; `kasl config check` names them as notices, not problems, and the next save drops them. `jira_inbox` (the name of `inbox` before 1.19) is moved by the first `kasl config` edit.

## Consequences

A typo in the config is said - by `kasl config check`, by `kasl config set` before it is written, by an editor as it is typed - while the watcher still reads past it. Docs examples of the config are titled `config.json` and checked against the schema by a test, so a page cannot teach a key that does not exist.

Two dependencies came in: `schemars` (generation) and `jsonschema` without its default features (no HTTP retrieval; the schema's references are all internal). The schema's `enumDescriptions` keyword is VS Code's, an annotation other validators ignore.

A setting's documentation now lives in three places that cannot drift apart silently: the doc comment on its field (which becomes its description in the schema and in the editor's hover), the Configuration page, and the published schema - the last held to the first by a test.
