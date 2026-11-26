# Dialects

Every call names one dialect: `bash`, `sh`, `zsh` or `fish`. All four lift into the same tree, so a
consumer never branches on the shell.

| dialect | parser | notes |
|---|---|---|
| `bash` | brush-parser | the full bash grammar |
| `sh` | brush-parser in POSIX mode | `<(…)` is refused; `[[` is read as a plain command name |
| `zsh` | brush-parser in bash mode | zsh-only syntax is not parsed |
| `fish` | tree-sitter-fish | fish's own syntax: `; and`, `; or`, `not`, `begin; … end`, `(…)` |

The `lift` output names the parser and its version in its `parser` field, so a stored result says
what produced it.

## The same construct in each shell

| construct | bash | sh | zsh | fish |
|---|---|---|---|---|
| and, or | `&&`, `\|\|` | same | same | `&&`, `\|\|`, `; and`, `; or` |
| negation | `!` | `!` | `!` | `not`, `!` |
| pipe stderr too | `\|&` | none | `\|&`, `&\|` | `&\|` |
| group | `{ …; }` | same | same | `begin; …; end` |
| command substitution | `$(…)`, backticks | same | same | `(…)`, `$(…)` |
| process substitution | `<(…)`, `>(…)` | none | same, `=(…)` | `(… \| psub)` |
| heredoc | `<<`, `<<-` | same | same | none |
| herestring | `<<<` | none | `<<<` | none |

Each row lands in the same node kind. For example, fish `cd x; and cargo build; or echo failed`
gives three invocations, where `cargo build` is guarded by `and` and `echo` by `or`, the same as
bash `cd x && cargo build || echo failed`.

## Nested scripts

A script inside a word is lifted too, when the word is static:

- `bash -c '…'`, `sh -c '…'`, `zsh -c '…'`, `dash -c '…'`, `fish -c '…'` in that shell's dialect;
- `eval '…'` in the current dialect;
- `ssh host '…'`, marked `remote: true`, so `rm` on another host is not counted as a local edit.

Nesting stops at depth 3. A fourth level becomes `unk` with reason `depth`.

```console
shellspec invocations --dialect bash --command 'bash -c "rm -rf build"'
```

This gives two invocations: `bash` (category `run`) and `rm` (category `edit`, effect
`fs_delete`, context `nested`, depth 1).

## What becomes `unk`

`unk` means shellspec could not say. Each `unk` carries one reason:

| reason | when | example |
|---|---|---|
| `syntax` | the parser refused the text | `echo 'unterminated && ls` |
| `dialect` | the text uses syntax of a shell the parser does not cover | zsh `diff =(ls a) =(ls b)` |
| `dynamic` | the program name is not known before the shell runs | `$CC -o x main.c` |
| `no_rule` | the program is known and no rule matches it | `frobnicate --now` |
| `depth` | a nested script beyond depth 3 | four levels of `bash -c` |

A refused region does not hide the rest. When the parser refuses a script, shellspec splits it at
top-level `;`, newline, `&&`, `||` and `&`, and lifts each part that parses alone.

A dynamic word in argument position is not `unk`. `cat "$f"` is still a `read`, and the argument is
kept as a dynamic word.

## Choosing the dialect

Pass the shell that will run the string. A harness that runs `zsh -c <string>` needs `zsh`. A
harness that passes an argv such as `["bash", "-lc", "<string>"]` needs `bash` for the string.
