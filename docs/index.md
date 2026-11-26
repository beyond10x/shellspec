# shellspec

A coding agent's shell tool sends a whole script in one string:

```sh
cd app && timeout 300 cargo test --workspace 2>&1 | tail -20
```

That string runs three programs. shellspec lifts it into a tree, lists every simple command in it
in the order the shell runs them, and gives each one a category:

| program | category | wrappers | runs only if |
|---|---|---|---|
| `cd app` | `navigate` | | |
| `cargo test --workspace` | `test` | `timeout` | `cd` succeeded |
| `tail -20` | `filter` | | `cd` succeeded |

The call as a whole is a test run. A tool that counts calls by their first word would have
counted it as `cd`.

## What it does

- **Lifts** bash, POSIX sh, zsh and fish into one tree: lists, `&&` and `||`, pipelines, simple
  commands, subshells, groups, compound commands, redirects, heredocs and substitutions.
- **Lists invocations**: one entry per simple command, including those inside `$(…)`, `<(…)`,
  `bash -c '…'`, `eval` and `ssh host '…'`. Each entry carries its wrappers, pipe stage, guard and
  working directory.
- **Classifies** every invocation with a ruleset you can read and extend: 21 categories such as
  `read`, `search`, `edit`, `build`, `test`, `vcs` and `network`, plus effects such as
  `fs_delete` and `remote_write`.
- **Redacts**: `shape` keeps the structure, program names, subcommands and flags, and drops every
  value.

## What it does not do

shellspec never executes, expands or globs anything. It does not know the value of a variable or
the content of a script it runs by path. It does not decide whether a command is allowed. That is
the job of the tool that uses it.

When it cannot say something, it says `unk` and gives the reason. It never guesses.

## Where to go next

- [Getting started](getting-started.md): install, the first commands, the library.
- [Dialects](dialects.md): what each shell supports and what becomes `unk`.
- [The record](record.md): the tree, the words and the invocation list.
- [Rules and categories](rules.md): how classification works and how to write your own rules.
- [Using shellspec from another tool](integrating.md): the library, JSON output and redaction.
