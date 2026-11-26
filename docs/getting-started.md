# Getting started

shellspec is a Rust library with a command-line tool of the same name. There is no release yet:
both build from source.

## Install the command-line tool

```console
git clone https://github.com/beyond10x/shellspec
cd shellspec
cargo build --release -p shellspec
./target/release/shellspec --help
```

## Classify a command

```console
shellspec invocations --dialect zsh \
  --command 'cd app && timeout 300 cargo test --workspace 2>&1 | tail -20'
```

The answer is JSON. Abridged to the fields this example is about:

```json
{
  "dominant": 1,
  "invocations": [
    {"index": 0, "program": "cd", "category": "navigate", "cwd": "."},
    {"index": 1, "program": "cargo", "subcommand": ["test"], "category": "test",
     "operation": "test.cargo", "wrappers": ["timeout"], "cwd": "app",
     "guard": [{"op": "and", "after": 0}]},
    {"index": 2, "program": "tail", "category": "filter", "effects": ["stdin"], "cwd": "app",
     "stage": {"pipeline": 1, "index": 1, "of": 2, "role": "tail"}}
  ],
  "ruleset": "sha256:8e16a52b…"
}
```

- `dominant` is the index of the invocation that labels the whole call. Here it is the test run.
- `guard` says that `cargo test` runs only if invocation 0 succeeded.
- `cwd` follows the `cd` before it.
- `ruleset` is the digest of the rules that produced the categories.

## The commands

| command | prints |
|---|---|
| `shellspec lift --dialect <d> --command <s>` | the full tree of one command string |
| `shellspec invocations --dialect <d> --command <s> [--ruleset <path>]` | every simple command, in execution order, with its category |
| `shellspec shape --dialect <d> --command <s>` | the redacted form |
| `shellspec rules check --ruleset <path>` | the size and digest of a ruleset, or its problems |
| `shellspec corpus stats --dialect <d> --input <file> [--ruleset <path>]` | counts of categories, `unk` and parse refusals over many commands |

`<d>` is one of `bash`, `sh`, `zsh` and `fish`. Without `--ruleset`, the built-in rules apply.

## Use the library

Add the crate by its git URL:

```toml
[dependencies]
shellspec = { git = "https://github.com/beyond10x/shellspec" }
```

```rust
use shellspec::{Category, Dialect, Ruleset, dominant, invocations, lift, shape};

let script = lift("cd app && timeout 300 cargo test 2>&1 | tail -20", Dialect::Zsh);
let list = invocations(&script, Ruleset::shared());
for inv in &list {
    println!("{:?} {:?} {:?}", inv.program, inv.category, inv.wrappers);
}
assert_eq!(dominant(&list).unwrap().category, Category::Test);
println!("{}", shape(&script));
```

Output:

```text
Some("cd") Navigate []
Some("cargo") Test ["timeout"]
Some("tail") Filter []
cd <arg> && timeout <n> cargo test 2>&1 | tail -<n>
```

`lift` never fails. Input that does not parse comes back as `Unk` nodes, and
`script.diagnostics` says where and why.
