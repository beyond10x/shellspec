# AGENTS.md — shellspec

shellspec lifts one shell command string, as an agent's Bash tool sends it, into one IR (lists,
`&&`/`||`, pipelines, simple commands, subshells, redirects, heredocs, substitutions), from bash,
POSIX sh, zsh and fish syntax, and gives each simple command a category (read, search, build, test,
vcs, network, …). Design: `docs/design.md`. Corpus numbers: `research/`.

First consumer: the `sessionspec` repository, which lifts each `ToolCall` of category execute
through shellspec into its sub-commands.

Organization-wide rules live in the workspace `AGENTS.md`; this file adds what is specific to
shellspec.

## Serves

- **O3 — any harness, observed and compared.** A harness's shell calls are recorded in one
  vocabulary: one IR and one category list for every dialect.
- **O6 — self-improvement, built into all of it.** The categories are what a session's record is
  measured by, so a change proposed from that record can be re-measured.

## Phase: implementation

The owner opened implementation on 2026-11-26: "Yeah, I mean, keep going. Just build this
stuff." Specification first (ESS), as in the workspace's other `*spec` repositories: every noun of
`docs/design.md` § 8 is declared in an ESS specification and `ess specify validate` passes before
code uses it; the CLI is an `ess-cli/2` binding.

## Layout

- `spec/`: the ESS specification (`ess/37`), domains `syntax`, `word`, `invocation`,
  `classification`, `cli`; `spec/cli.yaml` is the `ess-cli/2` binding.
- `generated/` (JSON Schema of every type) and `crates/shellspec-cli-contract/` (CLI adapter) are
  generated: `task generate`, never edited. `task check:generated` compares them byte for byte.
- `crates/shellspec/`: the library (`lift`, `invocations`, `dominant`, `shape`, `Ruleset`) and the
  `shellspec` binary. The built-in ruleset is `crates/shellspec/rules/*.yaml`, embedded with its
  digest; `shellspec rules check` refuses ties and wrappers without arity. `workspace.yaml` names
  only public beyond10x tools and generic programs.
- Tests: one binary, `crates/shellspec/tests/main.rs`. `tests/schema.rs` holds lifted trees and
  invocations to the generated JSON Schema, so the Rust types cannot drift from the specification.
- Gate: `slot task check`.
- `README.md` for humans, `LICENSE` (Apache-2.0), `CHANGELOG.md` (Keep a Changelog 1.1.0;
  `## [Unreleased]` takes a line for each change a user of the library or binary sees).
- `website/`: the public documentation site at `https://beyond10x.github.io/shellspec/`
  (Docusaurus on `@beyond10x/docs-system`, pinned by commit in `package.json`). Its pages are
  `docs/design.md` and `research/*.md`, read in place; `docusaurus.config.ts` gives each its
  route, title and header (`pages`). `product.json` is the landing page, drawn by
  `src/pages/index.tsx`. `.npmrc` sets `allow-git=root` for the Git-pinned dependency. A new file
  in `docs/` needs an `include` pattern and a `pages` entry; a new file in `research/` needs a
  `pages` entry. A Markdown link to a file the site does not publish fails the build, so link
  such files by GitHub URL. `task site` builds it as CI does.
- `.github/workflows/pages.yml` (`Documentation validation`): classifies the changed files
  (`website/`, `docs/`, `research/` are the site; `crates/`, `spec/`, `generated/` and every
  other non-Markdown file are code; other `*.md` is planning), builds the site, refuses a machine
  path in it, writes `.well-known/b10x-site.json` and on `main` uploads the artifact
  `b10x-project-site`. Its job `Repository gate` runs `task check` for the class `code` with the
  rustc of `rust-toolchain.toml` and `task`, `cargo-nextest` and `ess` downloaded at a pinned
  version and SHA-256; a new version of one of them is a change of that job.
- `.github/workflows/b10x-docs-site.yml`: publishes that artifact to GitHub Pages through
  `beyond10x/website`'s `project-site.yml`, pinned by commit, for bot-authored pushes to `main`.
- `.github/workflows/shared-gates.yml`: the organization's shared source gates
  (`beyond10x/gates`, pinned by commit), byte-equal to the other repositories' copy.

## Measuring the ruleset

`shellspec corpus stats --dialect zsh --input <jsonl>` counts refusals, categories and `unk` over
a JSON-lines file of commands. The file is extracted into the session scratch directory at run
time and never committed; only the counts go into `research/`.

## Rules

- Command text from real transcripts never enters this repository: not in research, not as a test
  fixture. Research reports counts and generic shapes. A fixture is written by hand or reduced to a
  shape (`cd <dir> && <prog> | tail -n <n>`).
- This repository is public. No person's name, e-mail address, host path, session name, private
  repository name, employer or customer name, token or secret value, in any file or commit.
- The parser never executes, expands or globs anything. Lifting is pure: same input, same IR.

## Commits

Every commit and push is `b10x-bot[bot]`'s through
`b10x-gates bot --repository beyond10x/shellspec -- <git command>`; every other GitHub write goes
through `b10x-gates api` or `b10x-gates gh`, with the organization's gates policy file in
`B10X_GATES_POLICY`. Every commit has a body. A tag and a release are publications and wait for the
owner's order (workspace `AGENTS.md` § "A default proceeds").
