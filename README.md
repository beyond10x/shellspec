# shellspec

An agent's Bash tool sends a whole script in one string: `cd x && slot cargo test 2>&1 | tail -20`.
A category per call says little about it. shellspec lifts that string into one IR and gives every
simple command in it a category.

- **Dialects:** bash, POSIX sh, zsh and fish.
- **IR (`shell-ir/1`):** lists, `&&`/`||`, pipelines, simple commands, subshells, groups,
  compound commands, redirects, heredocs and substitutions. Nested scripts (`bash -c '…'`,
  `eval`, `ssh host '…'`) are lifted to a fixed depth.
- **Pure:** nothing is executed, expanded or globbed. Input that does not parse becomes `Unk`
  nodes with diagnostics, never an error.
- **Categories:** a closed list (`read`, `search`, `build`, `test`, `vcs`, `network`, `filter`,
  `navigate`, …) from a declared YAML ruleset with a digest, so a change of the rules is visible
  in every result.

Documentation: <https://beyond10x.github.io/shellspec/> (getting started, dialects, the record,
rules and categories, use from another tool).

## Library

```rust
use shellspec::{Category, Dialect, Ruleset, invocations, lift};

let script = lift("cd x && slot cargo test 2>&1 | tail -20", Dialect::Zsh);
let list = invocations(&script, &Ruleset::builtin());
let cats: Vec<Category> = list.iter().map(|i| i.category).collect();
assert_eq!(cats, [Category::Navigate, Category::Test, Category::Filter]);
```

- `lift(text, dialect) -> Script`: the IR of one command string.
- `invocations(&script, &ruleset) -> Vec<Invocation>`: every simple command in execution order,
  with its wrappers (`slot`, `timeout`, `env`, `sudo`, …), pipe stage, guard, `cwd`, effects and
  category.
- `dominant(&invocations)`: the one invocation that labels the call, skipping `navigate`,
  `filter`, `output` and `environment`.
- `shape(&script)`: the redacted form; node kinds, known programs, subcommands and
  flag names, and no value of the input.
- `Ruleset::builtin()`, `Ruleset::load(path)`, `Ruleset::check()`: the rules in
  `crates/shellspec/rules/*.yaml`, or your own file in the `shellspec-rules/1` format.

## CLI

```console
shellspec lift --dialect zsh --command 'cd x && cargo test | tail'
shellspec invocations --dialect zsh --command 'cd x && cargo test | tail'
shellspec shape --dialect zsh --command 'cd x && cargo test | tail'
shellspec rules check --ruleset my-rules.yaml
shellspec corpus stats --dialect zsh --input commands.jsonl
```

## Building

The specification in `spec/` (ESS) declares every type; `generated/` and
`crates/shellspec-cli-contract/` are generated from it. The gate is `task check` (needs `ess`,
`cargo-nextest` and `task`); `task site` builds the documentation site.

## License

Apache-2.0, see `LICENSE`.
