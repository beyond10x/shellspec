# Using shellspec from another tool

shellspec is a library first. The command-line tool prints the same records as JSON for tools
that are not written in Rust.

## From Rust

```rust
use shellspec::{Dialect, Ruleset, dominant, invocations, lift};

fn label(command: &str) -> &'static str {
    let script = lift(command, Dialect::Bash);
    let list = invocations(&script, Ruleset::shared());
    dominant(&list).map_or("none", |inv| inv.category.name())
}
```

| function | returns |
|---|---|
| `lift(text, dialect)` | the `Script` tree; never fails |
| `invocations(&script, &ruleset)` | `Vec<Invocation>`, in execution order |
| `dominant(&list)` | the invocation that labels the call, if any |
| `shape(&script)` | the redacted form as a `String` |
| `Ruleset::shared()` | the built-in rules, read once per process |
| `Ruleset::builtin()` | a copy of the built-in rules |
| `Ruleset::load(path)` | rules from a file or a directory of `*.yaml` files |
| `Ruleset::from_sources(&[(name, yaml)])` | rules from text you hold in memory |
| `ruleset.check()` | the problems of a ruleset, empty when it is valid |

`Script`, `Invocation`, `Category` and `Effect` implement `serde::Serialize`, so a tool can store
them as JSON.

## From any language

Call the binary and read standard output:

```console
shellspec invocations --dialect bash --command 'git push --force origin main'
```

Each command prints one JSON document. `--ruleset <path>` selects your rules.

## Counting many commands

`shellspec corpus stats` reads a JSON-lines file. Each line is either a JSON string or an object
with a `command` field:

```json
{"command": "cd app && cargo test | tail -5"}
{"command": "git status"}
{"command": "frobnicate --now"}
```

```console
shellspec corpus stats --dialect zsh --input commands.jsonl
```

```json
{
  "commands": 3, "invocations": 5, "refused": 0,
  "categories": [{"key": "filter", "count": 1}, {"key": "navigate", "count": 1},
                 {"key": "test", "count": 1}, {"key": "unk", "count": 1}, {"key": "vcs", "count": 1}],
  "dominant": [{"key": "test", "count": 1}, {"key": "unk", "count": 1}, {"key": "vcs", "count": 1}],
  "unk_invocations": 1, "unk_permille": 200,
  "unk_programs": [{"key": "frobnicate", "count": 1}],
  "unk_reasons": [{"key": "no_rule", "count": 1}],
  "unk_regions": 0,
  "ruleset": "sha256:8e16a52b…"
}
```

`unk_programs` lists the programs no rule knows, most frequent first. It is the list to write rules
for next.

## Storing what an agent ran

Command strings can hold paths, hostnames and secrets. Two ways to keep the record without them:

- Store the `shape`. It keeps programs, subcommands, flags and structure, and replaces values with
  placeholders.
- Store invocations without their words: `program`, `subcommand`, `flags`, `category`,
  `operation`, `effects` and `stage`, and drop `argv` and `redirects`.

The effects `net_out` and `remote_write` mark the invocations whose arguments leave the machine.
A redaction pass can look there first.

## Mapping to your own categories

A tool with a shorter list maps shellspec's categories down. For example: `read` and `list` to
read, `search` to search, `edit` to edit, `network` and `forge` to network, and every other
category to execute.
