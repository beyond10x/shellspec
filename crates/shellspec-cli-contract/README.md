# `shellspec` CLI contract

Lift shell command strings into one IR and categorise every simple command

This generated package installs an unavailable handler. Application behavior enters through the `Handler` seam; schema-selected calls also require `DynamicValidator`. See `help.txt` for process options and `binding.json` for resolved types and targets.

## Commands

### `corpus stats`

Count parse refusals, categories and unk over a JSON-lines file of commands

Callable: `corpus_stats`. Input: `shellspec.cli.CorpusStatsInput`. Result: `shellspec.cli.CorpusStats`.

Error `input_unreadable`: `shellspec.cli.Refusal`.

Error `ruleset_invalid`: `shellspec.cli.Refusal`.

### `invocations`

Print every simple command of one command string, in execution order, with its category

Callable: `invocations`. Input: `shellspec.cli.InvocationsInput`. Result: `shellspec.cli.InvocationsResult`.

Error `ruleset_invalid`: `shellspec.cli.Refusal`.

### `lift`

Print the shell-ir/1 tree of one command string

Callable: `lift`. Input: `shellspec.cli.LiftInput`. Result: `shellspec.cli.LiftResult`.

### `rules check`

Check a ruleset for ties, unknown categories and wrappers without arity

Callable: `rules_check`. Input: `shellspec.cli.RulesCheckInput`. Result: `shellspec.cli.RulesCheckResult`.

Error `ruleset_invalid`: `shellspec.cli.Refusal`.

### `shape`

Print the redacted shape of one command string

Callable: `shape`. Input: `shellspec.cli.ShapeInput`. Result: `shellspec.cli.ShapeResult`.

## Runtime obligations

- handler:corpus_stats: implement the owner-qualified callable and its declared result/error contract
- handler:invocations: implement the owner-qualified callable and its declared result/error contract
- handler:lift: implement the owner-qualified callable and its declared result/error contract
- handler:rules_check: implement the owner-qualified callable and its declared result/error contract
- handler:shape: implement the owner-qualified callable and its declared result/error contract
