# shellspec design notes

Internal design notes. The specification in `spec/` is authoritative where these notes differ. The user documentation is in `docs/`.

## 1. Why

Claude Code's Bash tool sends long pipelined strings, such as
`cd x && foo | grep y; bar || baz 2>&1 | tail`. Each one is a shell script, not one process run.
`research/2026-11-26-corpus.md` measured 331,797 such strings in 72 hours of transcripts:
96.4 % hold at least one of `&&`, `||`, `;` or `|`; the median call runs 5 simple commands, the
90th percentile 10; 61.8 % start with `cd`. A category per call says little; a category per
simple command says what the agent did.

shellspec lifts one such string into one IR, from any of four dialects, and gives every simple
command in it a category. It is a library first; the CLI is for inspection and corpus work.

```text
command string ──parse(dialect)──▶ syntax tree ──lift──▶ shell IR ──classify(ruleset)──▶ invocations
 + dialect                         (parser's own)        (one form)                      (flat list, categorised)
```

First consumer: `sessionspec` (`design/design.md` there). Its `ToolCall` of category execute is
lifted through shellspec into sub-commands, and each sub-command gets a category (§ 6).

## 2. Scope

In: the syntax of one command string as an agent sends it; the four dialects bash, POSIX sh, zsh
and fish; nested scripts (`bash -c '…'`, `eval`, `ssh host '…'`) up to a fixed depth; a
categorisation of every simple command by a declared ruleset.

Out: executing, expanding or globbing anything; the value of a variable; the content of a file a
command runs (`./build.sh` is one invocation of category `run`, not its contents); judging whether
a command is allowed (that stays with its consumers, § 7).

## 3. The IR (`shell-ir/1`, proposal)

A script is a tree of nodes. Every node carries `span` (byte range in the input it was lifted
from) and `origin` (the dialect and, for a nested script, the path to the word it came from).

### Node kinds

| kind | fields | covers |
|---|---|---|
| `Script` | `dialect`, `body: [Statement]`, `diagnostics` | the whole input |
| `Statement` | `body: Node`, `sep: newline \| semi \| amp \| end` | one item of a list; `amp` is `&` (runs in the background) |
| `AndOr` | `first: Node`, `rest: [(op: and \| or, node)]` | `a && b \|\| c`; fish `a; and b`, `a && b` |
| `Not` | `body: Node` | `! a`; fish `not a` |
| `Pipeline` | `stages: [Node]`, `links: [stdout \| stdout_stderr]` | `a \| b`; `\|&`, zsh/fish `&\|` give `stdout_stderr` |
| `Command` | `assignments: [Assignment]`, `words: [Word]`, `redirects: [Redirect]` | a simple command |
| `Subshell` | `body: [Statement]`, `redirects` | `( … )` |
| `Group` | `body: [Statement]`, `redirects` | `{ …; }`; fish `begin; …; end` |
| `If`, `While`, `Until`, `For`, `Case`, `Select` | `condition`/`items`, `body`, `else`, `redirects` | compound commands |
| `Function` | `name`, `body` | `f() { … }`; fish `function f; …; end` |
| `Test` | `words` | `[[ … ]]` |
| `Arith` | `text` | `(( … ))` |
| `Unk` | `text_digest`, `len`, `reason`, `parser` | a region that did not parse (§ 3.3) |

`Statement` keeps `;` and newline apart because a newline-separated multi-line script and a `;`
chain are different habits of an agent; both run in sequence.

### Words

A `Word` is a list of parts. The parts keep what the shell would do with them without doing it:

| part | example |
|---|---|
| `Lit` | `cargo`, `--workspace` (unquoted, escapes removed) |
| `SQuoted`, `DQuoted { parts }`, `AnsiC` | `'x'`, `"a $b"`, `$'\n'` |
| `Param { name, op }` | `$HOME`, `${x:-y}` |
| `CmdSub { body: Script, form: dollar \| backtick \| fish_paren }` | `$(git rev-parse HEAD)`, `` `…` ``, fish `(…)` |
| `ProcSub { body: Script, dir: in \| out \| file }` | `<(…)`, `>(…)`, zsh `=(…)` |
| `ArithSub` | `$(( … ))` |
| `Glob`, `Brace`, `Tilde` | `*.rs`, `{a,b}`, `~/x` |

A word is `static` when every part is `Lit` or quoted text without expansion; its value is then
known (`static_value`). Otherwise the word is `dynamic`. Only static words are matched against a
rule (§ 6).

### Redirects and heredocs

`Redirect { fd: int?, op, target: Word | Fd(int) | Heredoc }`. `op` is one of `in` (`<`), `out`
(`>`), `append` (`>>`), `clobber` (`>|`, fish `>?` is `out_noclobber`), `inout` (`<>`), `dup_out`
(`>&`, `2>&1`), `dup_in` (`<&`), `both` (`&>`, `&>>`), `herestring` (`<<<`), `heredoc` (`<<`,
`<<-`).

`Heredoc { delimiter, quoted: bool, strip_tabs: bool, body: Word }`. A quoted delimiter
(`<<'EOF'`) makes the body literal; an unquoted one keeps substitutions as parts, so a
`$(…)` inside a heredoc body is lifted too. The heredoc is attached to the command that reads it,
in shell order, not where a grammar happens to hang it (§ 5.3).

### 3.1 Dialect mapping

| construct | bash | POSIX sh | zsh | fish |
|---|---|---|---|---|
| sequence | `;`, newline | same | same | same |
| and / or | `&&`, `\|\|` | same | same | `&&`, `\|\|` (3.0+), `; and`, `; or` |
| negation | `!` | `!` | `!` | `not`, `!` |
| pipe stderr too | `\|&` | none | `\|&`, `&\|` | `&\|` |
| background | `&` | `&` | `&`, `&!`, `&\|` (disown) | `&` |
| subshell | `( … )` | same | same | none: `fish -c` is a nested script |
| group | `{ …; }` | same | same, `{ … } always { … }` | `begin; …; end` |
| command substitution | `$(…)`, `` `…` `` | same | same | `(…)`, `$(…)` (3.4+) |
| process substitution | `<(…)`, `>(…)` | none | same, `=(…)` | `(… \| psub)`: a `Command` `psub`, kept as is |
| heredoc | `<<`, `<<-` | same | same | none |
| herestring | `<<<` | none | `<<<` | none |
| redirect both | `&>`, `&>>` | none | `&>`, `>&` | `&>`, `&>>` |
| variables in words | `$x`, `${x…}` | same | same, plus flags `${(f)x}` | `$x`, `$x[1]`, no `${}` |
| assignment prefix | `X=1 cmd` | same | same | `X=1 cmd` (3.1+), `env X=1 cmd` |
| conditionals | `[[ ]]`, `(( ))` | `[ ]` only | same as bash | `test`, `[ ]` |

Every row lands in the same node kind. What has no counterpart is a dialect-only node field, not a
new kind: `Pipeline.links` `stdout_stderr`, `Statement.sep` `amp` with `disown: true`,
`ProcSub.dir` `file` for zsh `=(…)`.

### 3.2 Nested scripts

A static word that is a script for another shell is lifted again and hung under the invocation as
`nested: Script`, with depth at most 3 (the bound `b10x guard bash` uses):

- `bash -c S`, `sh -c S`, `zsh -c S`, `dash -c S` (dialect of the program), `fish -c S` (fish),
  `eval S…` (the current dialect);
- `ssh host S`, `docker exec … sh -c S`, `kubectl exec … -- sh -c S`: lifted and marked
  `remote: true`, so a `rm` on a remote host is not a local edit;
- `xargs P …`, `find … -exec P … ;`, `timeout N P …` and the other wrappers of § 6.2 are not
  nested scripts: their inner program is the invocation, with the wrapper recorded.

A dynamic script word (`bash -c "$x"`) gives `nested: Unk { reason: dynamic }`.

### 3.3 What is `unk`

`unk` means shellspec could not say, and it is never replaced by a guess. Four places, each
counted in `Script.diagnostics`:

1. **Syntax `Unk`**: a region the parser rejected. With a parser that recovers, the smallest
   enclosing statement becomes `Unk` and its siblings stay lifted. With one that does not, the
   whole script is one `Unk`, and the lexical fallback of § 5.4 splits it at top-level
   operators so the parts that do parse are lifted.
2. **Dynamic program**: the first word after wrappers is dynamic (`$CC -o x`, `"$(which cargo)"
   build`). The invocation has `program: unk`.
3. **No rule**: the program is static, no rule of the ruleset matches. Category `unk`.
4. **Depth**: a nested script beyond depth 3: `Unk { reason: depth }`.

A word that is dynamic in argument position is not `unk`: the invocation is classified from its
static words, and the dynamic one is kept as such.

### 3.4 Invocations: the flat view

The tree is for structure; consumers mostly want a list. `invocations(script)` walks the tree in
execution order and returns one `Invocation` per simple command, including those inside
substitutions, subshells, compound bodies, heredoc bodies and nested scripts:

```text
Invocation {
  path: [index]            # position in the tree, so the list maps back to the tree
  program: string | unk    # basename of the first word after wrappers and assignments
  argv: [Word]             # all words after the wrappers
  wrappers: [Wrapper]      # slot, timeout 300, env X=1, nice, nohup, sudo, b10x-gates bot --, …
  assignments: [name]      # VAR=x prefix names; values stay in the tree
  redirects: [Redirect]
  stage: { pipeline: id, index, of, role: alone | head | middle | tail }
  guard: [(op: and | or, after: path)]   # runs only if the named earlier node succeeded / failed
  context: top | cmdsub | procsub | subshell | heredoc | nested | loop_body | function_body
  cwd: relative path | unk # as far as `cd`/`pushd` before it are static
  remote: bool
  category, operation, effects, rule   # § 6
}
```

`cwd` is tracked like `b10x guard bash` does: a static `cd x` changes it for what follows in the
same list; `cd "$d"` or `cd -` makes it `unk`; a subshell restores it.

## 4. Corpus facts the design rests on

From `research/2026-11-26-corpus.md` (331,797 commands, files modified 2026-11-23 to 2026-11-26):

- 96.4 % of calls contain `&&`, `||`, `;`/newline or `|`; 3.5 % are one simple command. The IR
  is a tree, and the flat invocation list (§ 3.4) is what consumers count.
- Simple commands per call: p50 5, p90 10, max 296 including substitutions. Stages per pipeline:
  p50 2, p90 3, max 11.
- 69.4 % of calls pipe. 79.3 % of the 402,870 pipelines end in `head` (47.3 %), `cut` or `tail`:
  output shaping. Hence category `filter` (§ 6.1) and the projection `dominant` (§ 7.1), so a
  `… | tail` call is not counted as a read.
- 61.8 % start with `cd` (sub-agents 67.4 %, main sessions 37.6 %). Hence `cwd` per invocation.
- 36.7 % redirect, 21.6 % copy a descriptor (`2>&1`). Redirects must attach to the right command
  (§ 5.3).
- 8.0 % use `$(…)` or backticks, 11.1 % a compound command, 0.7 % a heredoc, 0.3 % process
  substitution. Rare, but each hides sub-commands, so all are lifted.
- `timeout` (9,211) and `slot` (2,958) are among the top 15 first words, and `sh <path>` is
  fourth (9,639). Wrappers are unwrapped; a script run by path is `run`, its content unknown.
- zsh-only and fish syntax is near absent (text hints: 166 glob-qualifier shapes, 1 `=(…)`,
  2 `fish -c`). A bash parser covers the zsh the Bash tool receives.

## 5. Parser choice

### 5.1 Candidates

Read from crate docs and source, and run over the whole corpus (331,797 commands, one thread,
`research/2026-11-26-corpus.md` § Parsers). Version data from the crates.io API on 2026-11-26.

| crate | version, date | licence | downloads | tree | spans | recovers | refused on corpus | p99 |
|---|---|---|---:|---|---|---|---:|---:|
| `brush-parser` | 0.4.0, 2026-05-03 | MIT | 1.03 M | typed AST; words are strings, re-parsed by `word::parse` | char index, line/col | no | 50 (0.015 %) | 56 µs |
| `rable` | 0.2.2, 2026-10-28 | MIT | 13 k | typed, `$(…)` as nodes | char | no | 43 (0.013 %) | 43 µs |
| `tree-sitter-bash` | 0.25.1, 2025-12-02 | MIT | 25.8 M | CST | byte | yes (ERROR, MISSING) | 300 (0.09 %) | 279 µs |
| `yash-syntax` | 0.25.0, 2026-09-06 | GPL-3.0+ | 64 k | typed, `Rc` | char | no | 2,199 (0.66 %) | 261 µs |
| `conch-parser` | 0.1.1, 2019-05-15 | MIT/Apache | 26 k | typed | byte | no | 3,627 (1.09 %) | 109 µs |
| `tree-sitter-fish` | 3.6.0, 2025-06-13 | MIT | 213 k | CST | byte | yes | (fish, not run) | |
| `tree-sitter-zsh` | 0.64.3, 2026-11-17 | MIT | 127 k | CST, a rework of the bash grammar | byte | yes | (not run) | |

Not dependencies:

- fish's own parser (fish 4.9.3, `ast::parse` with a partial-tree flag) is not on crates.io (the
  name `fish` is an unrelated crate) and is GPL-2.0 inside the whole shell.
- `zshrs` (lib `zsh`, 0.13.85) ports zsh's `parse.c` with global parser state and about 50
  dependencies including tokio and rusqlite.
- `yash-syntax` is GPL-3.0+, POSIX only, and refuses process substitution (1,033 corpus
  commands) and `[[`. `conch-parser` has had no release since 2019 and refuses `$(`/`<(` shapes.

### 5.2 Choice

**`brush-parser` for bash, POSIX sh and zsh input; `tree-sitter-fish` for fish.**

- It refuses the fewest commands of the established crates (50 of 331,797), and its refusals are
  mostly input bash would refuse too: unterminated backquote 14, error at end of input 7 (the
  latter is inference from the error kinds, not checked against bash).
- It gives a typed AST to lift from, has a POSIX mode for `sh`, and is fast enough for a
  PreToolUse hook: 6.1 s for the corpus, p99 56 µs, max 0.9 ms.
- MIT, 1.03 M downloads, synchronous.

Costs: no error recovery (§ 5.4 covers it); words come back as strings and are re-parsed with its
word parser; spans are char indices, converted to bytes in the lift.

`rable` is as accurate and faster with one dependency, but its first release was 2026-03-24 and
it has 13 k downloads. It is the named alternative; the lift is written against shellspec's IR,
so swapping the parser touches only the lift module. `rippy-cli`, a safety hook for agent
shells, is built on it (prior art, not read further).

### 5.3 Why not tree-sitter-bash, which the workspace already uses

It recovers from errors and is in `agentplugins-check`, `metaharness-semantic` and the `b10x`
guard. But it gives the wrong tree, without an error, for the most common shape of the corpus:
`a && b 2>&1 | c` comes out as `(a && b 2>&1) | c`. On commands both parsers accept it disagrees
with brush-parser on 2,014 pipe tails and 1,359 stage counts. The guard already carries two
workarounds for this grammar (guard.rs: "The grammar hangs `a && b <<EOF` on the list; the shell
gives the redirection to `b`" and "The grammar drops a word between the command and the first
redirection").

### 5.4 Fallback for refused input

When brush-parser refuses a script, a quote-aware lexical pass splits it at top-level `;`,
newline, `&&`, `||` and `&`, and each part is parsed alone. Parts that parse are lifted; the rest
become `Unk { reason: syntax }`. On the corpus this decides at most 50 commands (0.015 %). No
second grammar is added for it.

### 5.5 zsh and fish

The Bash tool of the measured corpus ran `zsh -c`, yet zsh-only syntax is close to absent (§ 4), so zsh
input is parsed in brush's bash mode, and a refusal becomes `Unk { reason: dialect }` when the
text has a zsh-only token (`=(`, `&!`, a glob qualifier). `tree-sitter-zsh` is the upgrade path
if that count grows. fish is parsed with `tree-sitter-fish`; with 2 `fish -c` calls in the corpus
it is built last (§ 9).

## 6. Categorisation

### 6.1 The closed list

A category is one value of a closed list. The finer `operation` (`vcs.push`, `test.cargo`) is
free text from the rule. `effects` is a set, orthogonal to the category, and is what sessionspec's
exposure and guardrail KPIs read.

| category | meaning | examples |
|---|---|---|
| `navigate` | changes the working directory | `cd`, `pushd`, `popd` |
| `read` | reads file content | `cat f`, `head f`, `sed -n 1,80p f`, `jq . f`, `wc -l f` |
| `list` | reads file-system metadata | `ls`, `find` (no `-exec`/`-delete`), `stat`, `du`, `tree` |
| `search` | searches content of files | `rg x`, `grep -r x d`, `git grep`, `ast-grep` |
| `filter` | transforms a stream in a pipe | `\| tail`, `\| head`, `\| grep`, `\| jq`, `\| sort`, `\| wc` |
| `output` | writes to its own stdout only | `echo`, `printf`, `true`, `date` |
| `edit` | writes or deletes files | `sed -i`, `tee f`, `cp`, `mv`, `rm`, `mkdir`, `chmod`, `> f` alone |
| `build` | compiles | `cargo build`, `cargo check`, `go build`, `npm run build`, `make` |
| `test` | runs tests | `cargo test`, `cargo nextest run`, `go test`, `vitest`, `playwright test` |
| `lint` | checks or formats code | `cargo clippy`, `cargo fmt`, `shellcheck`, `eslint` |
| `run` | runs a program the agent built or a script | `./target/debug/x`, `cargo run`, `sh ./x.sh`, `python3 x.py` |
| `vcs` | git and other version control | `git status`, `git commit`, `git push` |
| `forge` | a forge API | `gh pr create`, `gh api`, `glab mr` |
| `network` | other network clients | `curl`, `wget`, `ssh`, `scp`, `nc` |
| `package` | installs or changes dependencies | `npm install`, `cargo add`, `cargo install`, `pacman -S` |
| `process` | inspects or controls processes, waits | `ps`, `kill`, `pkill`, `sleep`, `wait`, `pgrep` |
| `environment` | shell state | `export`, `source`, `set`, `unset`, `alias` |
| `system` | services and the host | `systemctl`, `journalctl`, `df`, `free`, `uptime` |
| `container` | containers and clusters | `docker`, `podman`, `kubectl`, `helm` |
| `tool` | a CLI of the workspace or the harness | `aep`, `ess`, `slot status`, `worktree`, `inbox`, `claude` |
| `unk` | no rule matched, or the program is dynamic | |

The category is about the invocation, not the call: `cd x && cargo test 2>&1 | tail -20` is
three invocations, `navigate`, `test`, `filter`.

Effects (a set): `fs.read`, `fs.write`, `fs.delete`, `net.out`, `remote.write` (changes state
outside the host: `git push`, `gh pr create`, `curl -X POST`), `process.signal`, `exec` (runs a
program named in its arguments), `stdin` (reads its stdin), `wait`.

### 6.2 How a rule matches

A ruleset is data (`rules/*.yaml`, a declared ESS noun, § 8), with a version and a digest. Each
classified invocation names its rule id and the ruleset digest, so a change of the rules is
visible in every report that used them.

Matching, in order:

1. **Unwrap.** Leading assignments and wrappers are removed and recorded: `env [-flags]
   [X=y…]`, `timeout [-flags] N`, `slot [--timeout D]`, `nice`, `ionice`, `nohup`, `stdbuf`,
   `time`, `command`, `exec`, `builtin`, `noglob`, `sudo [-flags]`, `b10x-gates bot … --`. Each
   wrapper is itself a rule of kind `wrapper` that declares how many words it takes.
2. **Program.** Basename of the first remaining static word. A path (`./target/debug/x`,
   `/usr/local/bin/y`) keeps its basename and gets `run` unless a rule names the basename.
3. **Subcommand path.** The rule declares the program's global flags that take a value
   (`git -C <dir> -c <k=v>`, `cargo +<toolchain>`, `npm --prefix <dir>`), skips them, and matches
   the next positionals: `git push`, `cargo nextest run`, `gh pr create`, `npm run <script>`.
4. **Flag conditions.** A rule may require or exclude flags: `sed -i` is `edit`, `sed` without is
   `read` or `filter`; `find -delete` adds `fs.delete`; `curl -X POST|-d|--data|-F|-T` adds
   `remote.write`; `git branch -D`, `git push --force` set operation `vcs.push.force`.
5. **Position.** A program whose rule is `stream` (`grep`, `head`, `tail`, `jq`, `sort`, `wc`,
   `sed`, `awk`, `cut`, `tr`, `uniq`) is `filter` when it reads stdin, meaning a pipe stage after
   the head with no file operand, and `read`/`search` when it has a file operand.
6. **Most specific wins**: subcommand and flag match beats program match; ties are an error of
   the ruleset, found by the ruleset check, not at run time.

`npm run <script>` and `task <name>` are classified by the script's name through a table in the
rule (`test*` → `test`, `build` → `build`, `check` → `test`); an unknown script name is `run`.

The workspace's own CLIs (`aep`, `ess`, `slot`, `worktree`, `inbox`, `b10x`) are one rule each,
category `tool`, operation from the subcommand. `slot <cmd>` is a wrapper first: `slot cargo
test` is `test` with wrapper `slot`, `slot status` is `tool`.

### 6.3 What the ruleset is measured against

The corpus gives the order of work: the top first words (§ 4) are covered by the first ruleset,
and a ratchet holds the share of invocations with category `unk` on a pinned corpus sample.

## 7. Handoff to sessionspec and other consumers

### 7.1 sessionspec

```text
ToolCall { tool: "Bash", category: execute, input.command, cwd }
  └── shellspec::lift(command, dialect) ──▶ Script
        └── invocations(script, ruleset) ──▶ [Invocation]   (= sessionspec sub-commands)
```

- **Dialect** is an input, chosen by sessionspec's adapter from what it knows of the harness:
  Claude Code runs the user's shell (`zsh -c …` where that is zsh), so `zsh`. Codex sends an argv
  (`["bash", "-lc", S]`); `lift_argv` takes the argv, and when it is `[shell, -c, S]` lifts `S`
  in that shell's dialect, otherwise it is one invocation.
- **cwd** comes from the `ToolCall`; shellspec returns every invocation's `cwd` relative to it.
- **Effective category of the call.** sessionspec keeps the `ToolCall` category `execute` and adds
  `sub_commands: [Invocation]`. A KPI that wants one label per call uses the projection
  shellspec ships, `dominant(invocations)`: the first invocation that is not `navigate`,
  `filter`, `output` or `environment` (inference: `cd x && cargo test | tail` is a test call).
- **Category mapping** to sessionspec's tool list: `read`, `list` → read; `search` → search;
  `edit` → edit; `network`, `forge` → network; every other category → execute.
- **Exposure.** sessionspec's "sent outward" fact reads `effects ∋ net.out | remote.write` per
  invocation, and the argument words of that invocation are where its redaction pass looks.
- **Redaction.** shellspec's IR carries the raw words. sessionspec's redaction turns each `Word`
  into digest, length and class; shellspec gives it the structure to keep: node kinds, program,
  subcommand path, flag names, word kinds (`flag`, `flag_value`, `positional`, `path_like`),
  categories. `shellspec shape` (§ 8) is that redacted form, so the corpus research of § 4 and
  sessionspec use the same one.
- **Errors.** `lift` never fails on input; a script that does not parse is a `Script` with `Unk`
  nodes and diagnostics, so sessionspec counts it as verdict `unk`.

### 7.2 `b10x guard bash`

Source: `agentplugins` `crates/b10x/src/guard.rs` on an unmerged branch; it is not
on `main`, and an installed `b10x` binary carries
its strings. 1,307 lines; it parses with tree-sitter-bash and has its own walker for what
shellspec's IR does: simple commands inside `$(…)`, wrappers (`env`, `timeout`, `sudo`,
`b10x-gates bot --`), `bash -c` to depth 3, `cd` tracking, redirect targets, heredoc and
herestring feeds, and two workarounds for tree-sitter-bash (§ 5.3).

It should become a shellspec consumer: its six checks are predicates over `Invocation`s (program,
flags, redirects, `stage`, `context: loop_body`, `cwd`), and its walker is the part shellspec
replaces. Conditions: shellspec must decide within the hook's latency (measured in § 5), and the
guard keeps its own rule "a parse failure allows". Move it after shellspec's IR is stable, not
before; until then nothing changes in agentplugins.

### 7.3 metaharness-semantic

`metaharness/crates/metaharness-semantic/src/extract.rs` (2,549 lines, MIT-licensed prior art)
also lifts shell calls with tree-sitter-bash into semantic kinds (`file.read`,
`stream.select`, `vcs.status`, `validation.test`, `network.fetch`, `process.run`). Its kinds are
the closest prior art for § 6 and were read for it. It is a candidate consumer; no change is
proposed there.

## 8. Specification first (ESS)

shellspec is specified first, like the other `*spec` and tool repositories: every noun below is declared in an ESS specification and
`ess specify validate` passes before code uses it; the CLI is an `ess-cli/2` binding
(`ess/docs/design/cli-presentation-binding.md`), and its parser comes from `ess generate cli`.

Nouns, by domain (names are proposals):

- **syntax**: `Script`, `Dialect`, `Span`, `Origin`, `Statement`, `Separator`, `AndOr`, `Not`,
  `Pipeline`, `PipeLink`, `Command`, `Assignment`, `Subshell`, `Group`, `If`, `While`, `Until`,
  `For`, `Case`, `Select`, `Function`, `Test`, `Arith`, `Unk`, `UnkReason`, `Diagnostic`.
- **word**: `Word`, `WordPart` (`Lit`, `SQuoted`, `DQuoted`, `AnsiC`, `Param`, `CmdSub`,
  `ProcSub`, `ArithSub`, `Glob`, `Brace`, `Tilde`), `Redirect`, `RedirectOp`, `Heredoc`.
- **invocation**: `Invocation`, `Wrapper`, `Stage`, `StageRole`, `Guard`, `InvocationContext`,
  `WordRole`.
- **classification**: `Category` (closed enum, § 6.1), `Effect`, `Operation`, `Rule`,
  `WrapperRule`, `Ruleset`, `RulesetDigest`, `Classification`.
- **parser**: `ParserId` (parser and version that produced a script), `LiftResult`.
- **shape**: `Shape` (the redacted form of a script, § 7.1), `CorpusStats` (the numbers of
  `research/`).

CLI (`ess-cli/2`, binary `shellspec`, proposal):

| command | does |
|---|---|
| `lift --dialect <d> --command <s>` | prints the `shell-ir/1` document |
| `invocations --dialect <d> --command <s> --ruleset <path>` | prints the classified flat list |
| `shape --dialect <d> --command <s>` | prints the redacted shape |
| `rules check --ruleset <path>` | refuses ties, unknown categories, a wrapper without arity |
| `corpus stats --input <jsonl> --dialect <d>` | the numbers of `research/` from a JSON-lines file of commands |

## 9. Build order (proposal)

1. ESS domains of § 8, `ess specify validate` first; `shell-ir/1` JSON Schema projected from it.
2. bash/zsh lifting with the chosen parser (§ 5), the fallback of § 5.4, and `corpus stats`
   reproducing `research/2026-11-26-corpus.md` within its stated error.
3. The first ruleset, covering the top first words of § 4; `unk` share measured.
4. sessionspec's Claude Code adapter calls `invocations`.
5. fish lifting.
6. `b10x guard bash` on shellspec (§ 7.2), when the agentplugins maintainers agree.

## 10. Choices and their alternatives

- Parser for bash, sh and zsh: `brush-parser`. Alternatives: `rable` (equal accuracy, young),
  `tree-sitter-bash` (recovers, mis-nests the common shape).
- zsh: parsed with brush's bash mode, zsh-only refusals as `Unk`. Alternative: `tree-sitter-zsh`.
- fish: `tree-sitter-fish`. Alternative: fish's own parser as a git dependency (GPL-2.0).
- Ruleset: YAML data in the repository, declared as an ESS noun, with a digest. Alternative: a
  Rust table.
- Category list: the 21 categories of § 6.1, mapped down to sessionspec's tool list (§ 7.1).
  Alternative: sessionspec's list only.
- `b10x guard bash`: becomes a consumer once `shell-ir/1` is stable; no change in agentplugins
  now.
- sessionspec uses shellspec as a Rust library dependency; the CLI is for inspection and corpus
  runs.
