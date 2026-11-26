# Changelog

All notable changes to this project are documented in this file.
The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

## [Unreleased]

### Added

- `lift` turns one command string of bash, POSIX sh, zsh or fish into the `shell-ir/1` tree; input that does not parse becomes `Unk` nodes with diagnostics.
- `invocations` lists every simple command with its wrappers, pipe stage, guard, `cwd`, effects and a category from the built-in ruleset; `dominant` picks the one that labels the call.
- `shape` gives the redacted form of a script, with no value of the input.
- A ruleset in the `shellspec-rules/1` YAML format with a digest; `shellspec rules check` refuses ties and wrappers without arity.
- The `shellspec` binary: `lift`, `invocations`, `shape`, `rules check` and `corpus stats`.
- A documentation site with the design and the corpus research.
