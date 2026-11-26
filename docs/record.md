# The record

shellspec gives two views of one command string: the tree, which keeps its structure, and the
invocation list, which most tools count.

## The tree

`shellspec lift` and the library's `lift` return a script:

```console
shellspec lift --dialect zsh --command 'echo $(git rev-parse HEAD)'
```

```json
{
  "format": "shell-ir/1",
  "parser": "brush-parser/0.4.0",
  "script": {
    "dialect": "zsh",
    "diagnostics": [],
    "body": [{"sep": "end", "body": {"kind": "Command", "value": {
      "words": [
        {"raw": "echo", "static_value": "echo", "parts": [{"kind": "Lit", "value": {"text": "echo"}}]},
        {"raw": "$(git rev-parse HEAD)", "parts": [{"kind": "CmdSub", "value": {"form": "dollar",
          "body": {"body": ["…the script git rev-parse HEAD…"]}}}]}
      ],
      "assignments": [], "redirects": []}}}]
  }
}
```

Abridged: the substitution's body is a full script of its own, and every node carries a `span`.

| node | what it holds |
|---|---|
| `Statement` | one item of a list and its separator: newline, `;`, `&` or the end |
| `AndOr` | `a && b \|\| c` |
| `Not` | `! a`, fish `not a` |
| `Pipeline` | its stages, and for each link whether stderr is piped too |
| `Command` | a simple command: assignments, words, redirects |
| `Subshell`, `Group` | `( … )`, `{ …; }`, fish `begin; …; end` |
| `If`, `While`, `Until`, `For`, `Case`, `Function` | compound commands |
| `Test`, `Arith` | `[[ … ]]`, `(( … ))` |
| `Unk` | a region that did not parse: its length, a digest of its text, the reason and the parser |

An `Unk` node keeps a digest of its text and never the text itself.

## Words

A word is a list of parts. Each part records what the shell would do with it, without doing it.

| part | example |
|---|---|
| `Lit` | `cargo`, `--workspace` |
| `SQuoted`, `DQuoted`, `AnsiC` | `'x'`, `"a $b"`, `$'\n'` |
| `Param` | `$HOME`, `${x:-y}` |
| `CmdSub` | `$(git rev-parse HEAD)`, backticks, fish `(…)` |
| `ProcSub` | `<(…)`, `>(…)`, zsh `=(…)` |
| `ArithSub` | `$(( … ))` |
| `Glob`, `Brace`, `Tilde` | `*.rs`, `{a,b}`, `~/x` |

A word whose parts are all literal or quoted text has a `static_value`. Only static words are
matched against rules. Substitutions inside words, heredoc bodies and redirect targets are lifted
too.

## Invocations

`shellspec invocations` and the library's `invocations` walk the tree in execution order. They
return one entry per simple command, wherever it sits.

| field | meaning |
|---|---|
| `index` | position in the list |
| `path` | position in the tree, so an entry maps back to its node |
| `program` | basename of the first word after wrappers and assignments; absent when dynamic |
| `subcommand` | the subcommand words a rule matched: `["test"]` for `cargo test` |
| `wrappers` | the wrappers removed before the program: `timeout`, `env`, `nice`, `sudo` and more |
| `assignments` | names of `VAR=x` prefixes; the values stay in the tree |
| `flags` | the flag words |
| `argv` | every word after the wrappers |
| `redirects` | its redirects, attached to the command the shell gives them to |
| `stage` | pipeline number, position, length and role: `alone`, `head`, `middle`, `tail` |
| `guard` | runs only if an earlier invocation succeeded (`and`) or failed (`or`) |
| `context` | where it sits: `top`, `cmdsub`, `procsub`, `subshell`, `heredoc`, `nested`, `loop_body`, `function_body` |
| `cwd` | working directory relative to the start, as far as earlier `cd` and `pushd` are static |
| `remote` | `true` inside a script run on another host through `ssh` |
| `depth` | nesting depth of `bash -c`, `eval` and `ssh` scripts |
| `category`, `operation`, `effects`, `rule` | the classification (see [Rules and categories](rules.md)) |
| `unk` | the reason, when the category is `unk` |

`cwd` becomes unknown after a dynamic `cd "$d"` or `cd -`. A subshell restores it on exit.

## The dominant invocation

A call often starts with `cd` and ends in `| tail`. `dominant` picks the invocation that labels the
call: the first one whose category is not `navigate`, `filter`, `output` or `environment`. A
`bash -c` that only carries a nested script is skipped for the script inside it.

`cd app && cargo test | tail -5` is a test call. `bash -c "rm -rf build"` is an edit.

## The shape

`shellspec shape` and the library's `shape` keep the structure and drop the values. Programs,
subcommands and flags stay. Paths, arguments and numbers become placeholders:

```console
shellspec shape --dialect zsh --command 'cd app && timeout 300 cargo test --workspace 2>&1 | tail -20'
```

```json
{"shape": "cd <arg> && timeout <n> cargo test --workspace 2>&1 | tail -<n>"}
```

Use it to store or share what an agent ran without storing what it ran on.
