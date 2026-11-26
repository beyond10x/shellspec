# Rules and categories

Every invocation gets one category from a closed list, an optional finer `operation`, and a set of
effects. A ruleset decides them. The built-in ruleset ships with the crate, and you can load your
own.

## Categories

| category | meaning | examples |
|---|---|---|
| `navigate` | changes the working directory | `cd`, `pushd`, `popd` |
| `read` | reads file content | `cat f`, `head f`, `jq . f`, `wc -l f` |
| `list` | reads file-system metadata | `ls`, `find`, `stat`, `du`, `tree` |
| `search` | searches the content of files | `rg x`, `grep -r x src`, `git grep` |
| `filter` | transforms a stream in a pipe | `\| tail`, `\| head`, `\| grep`, `\| jq`, `\| sort`, `\| wc` |
| `output` | writes to its own stdout only | `echo`, `printf`, `true`, `date` |
| `edit` | writes or deletes files | `sed -i`, `tee f`, `cp`, `mv`, `rm`, `mkdir`, `chmod` |
| `build` | compiles | `cargo build`, `go build`, `npm run build`, `make` |
| `test` | runs tests | `cargo test`, `cargo nextest run`, `go test`, `vitest` |
| `lint` | checks or formats code | `cargo clippy`, `cargo fmt`, `shellcheck`, `eslint` |
| `run` | runs a program or script | `./target/debug/app`, `cargo run`, `sh ./x.sh`, `python3 x.py` |
| `vcs` | version control | `git status`, `git commit`, `git push` |
| `forge` | a forge API | `gh pr create`, `gh api`, `glab mr` |
| `network` | other network clients | `curl`, `wget`, `ssh`, `scp` |
| `package` | installs or changes dependencies | `npm install`, `cargo add`, `pacman -S` |
| `process` | inspects or controls processes, or waits | `ps`, `kill`, `sleep`, `wait` |
| `environment` | changes shell state | `export`, `source`, `set`, `unset`, `X=1` alone |
| `system` | services and the host | `systemctl`, `journalctl`, `df`, `free` |
| `container` | containers and clusters | `docker`, `podman`, `kubectl`, `helm` |
| `tool` | a developer tool's own CLI | `slot status`, `worktree`, `aep`, `ess` |
| `unk` | no rule matched, or the program is dynamic | |

Effects are independent of the category: `fs_read`, `fs_write`, `fs_delete`, `net_out`,
`remote_write` (changes state outside the host, such as `git push` or `curl -X POST`),
`process_signal`, `exec`, `stdin` and `wait`.

## How a rule matches

For each invocation, in this order:

1. **Unwrap.** Leading assignments and wrappers are removed and recorded: `timeout 300`, `env
   X=1`, `nice`, `nohup`, `sudo`, `xargs`, `npx` and the rest of the wrapper rules. Each wrapper
   rule declares how many words it takes.
2. **Program.** The basename of the first static word that remains. A path such as
   `./target/debug/app` is `run` unless a rule names its basename.
3. **Subcommand.** Global flags that take a value are skipped: `git -C dir`, `cargo +nightly`,
   `npm --prefix dir`. The next words are matched as the subcommand: `git push`,
   `cargo nextest run`, `gh pr create`.
4. **Flags.** A rule may require a flag (`flags_any`) or exclude one (`flags_none`): `sed -i` is
   `edit`, plain `sed` is `filter`.
5. **Position.** A stream program such as `grep`, `head`, `tail`, `jq`, `sort` or `wc` is `filter`
   when it reads a pipe, and `read` or `search` when it has a file operand.
6. **Most specific wins.** A subcommand beats a flag condition, and a flag condition beats a plain
   program match.

`npm run <script>`, `task <name>` and similar runners are classified by the script name:
`npm run test:unit` is `test`.

## The rules format

A rules file is YAML with `format: shellspec-rules/1` and up to three lists:

| list | holds |
|---|---|
| `wrappers` | programs that run another program: their names, the flags that take a value, and how many words they take (`positionals`, or `until` a marker word) |
| `programs` | how to read a program's command line: `flags_with_value` before the subcommand, `skip_prefix` such as `+`, and how many words form its `subcommands` |
| `rules` | `id`, `program`, optional `sub`, `flags_any`, `flags_none`, then `category`, optional `operation`, `effects`, `stream` and `scripts` |

## Write your own rules

Say your team deploys with a tool called `deployctl`. Without a rule it is `unk` with reason
`no_rule`. Add a file `local.yaml`:

```yaml
format: shellspec-rules/1

programs:
  - names: [deployctl]
    flags_with_value: [--env]
    subcommands: 1

rules:
  - {id: deployctl, program: [deployctl], category: tool, operation: deploy}
  - {id: deployctl.status, program: [deployctl], sub: [status], category: tool, operation: deploy.status}
  - id: deployctl.push
    program: [deployctl]
    sub: [push]
    category: network
    operation: deploy.push
    effects: [net_out, remote_write]
```

`--ruleset` takes one file or a directory of `*.yaml` files, and that set replaces the built-in
one. To extend the built-in rules, copy them from `crates/shellspec/rules/` into a directory and
add your file beside them:

```console
mkdir my-rules
cp shellspec/crates/shellspec/rules/*.yaml my-rules/
cp local.yaml my-rules/
shellspec rules check --ruleset my-rules
```

```json
{"files":["core.yaml","dev.yaml","local.yaml","workspace.yaml","wrappers.yaml"],"programs":57,"rules":201,"ruleset":"sha256:4013de15…","wrappers":20}
```

Then classify with it:

```console
shellspec invocations --dialect bash --ruleset my-rules \
  --command 'deployctl --env prod push && deployctl status'
```

The two invocations come back as `network` with operation `deploy.push` and effects `net_out`
and `remote_write`, then `tool` with operation `deploy.status`. The `--env prod` flag is skipped
before the subcommand is read.

## Checking a ruleset

`shellspec rules check` exits 1 and lists every problem:

- two rules with the same program, subcommand and flag conditions;
- the same rule id twice;
- a wrapper without `positionals` or `until`;
- a wrapper name or program declared twice;
- an unknown category or effect, or a field the format does not have.

```text
ruleset_invalid: {"detail":"rules deployctl.push and deployctl.push2 tie on program deployctl: same subcommand and flag conditions"}
```

The library refuses the same problems: `Ruleset::load` and `Ruleset::from_sources` return a
`RulesetError` with every problem listed.

## The digest

Every ruleset has a digest over its file names and bytes. `invocations` and `corpus stats` print
it as `ruleset`, so a stored result says which rules produced it. A change to any rules file
changes the digest.
