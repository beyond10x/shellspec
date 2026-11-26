use std::path::PathBuf;

use shellspec::Ruleset;
use shellspec::rules::builtin_files;

fn rules_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("rules")
}

#[test]
fn the_builtin_ruleset_is_every_rules_file_and_checks_clean() {
    let mut on_disk: Vec<String> = std::fs::read_dir(rules_dir())
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .filter(|n| n.ends_with(".yaml"))
        .collect();
    on_disk.sort();
    assert_eq!(builtin_files(), on_disk);
    let builtin = Ruleset::builtin();
    assert!(builtin.check().is_empty(), "{:?}", builtin.check());
    let loaded = Ruleset::load(&rules_dir()).unwrap();
    assert_eq!(loaded.digest, builtin.digest);
    assert!(builtin.digest.starts_with("sha256:"));
    assert_eq!(builtin.digest.len(), "sha256:".len() + 64);
}

#[test]
fn the_top_first_words_of_the_corpus_have_a_rule() {
    // research/2026-11-26-corpus.md § First words: first programs and top-level programs.
    let rules = Ruleset::shared();
    for p in [
        "cd",
        "grep",
        "cat",
        "sh",
        "sed",
        "tail",
        "ls",
        "date",
        "[",
        "printf",
        "echo",
        "sleep",
        "mkdir",
        "git",
        "head",
        "cut",
        "aep",
        "worktree",
        "cargo",
        "b10x-gates",
        "slot",
        "jq",
        "wc",
        "find",
        "rg",
        "env",
        "timeout",
    ] {
        assert!(
            rules.knows(p) || rules.wrapper(p).is_some(),
            "no rule or wrapper for {p}"
        );
    }
}

fn from(yaml: &str) -> Result<Ruleset, shellspec::RulesetError> {
    Ruleset::from_sources(&[("t.yaml".to_owned(), yaml.to_owned())])
}

#[test]
fn a_tie_is_refused() {
    let err = from(
        "format: shellspec-rules/1\nrules:\n  - {id: a, program: [x], category: read}\n  - {id: b, program: [x], category: list}\n",
    )
    .unwrap_err();
    assert!(err.to_string().contains("tie on program x"), "{err}");
}

#[test]
fn a_wrapper_without_arity_is_refused() {
    let err = from("format: shellspec-rules/1\nwrappers:\n  - {id: w, names: [w]}\n").unwrap_err();
    assert!(err.to_string().contains("declares no arity"), "{err}");
}

#[test]
fn an_unknown_category_or_field_is_refused() {
    assert!(
        from("format: shellspec-rules/1\nrules:\n  - {id: a, program: [x], category: reading}\n")
            .is_err()
    );
    assert!(from("format: shellspec-rules/1\nrules:\n  - {id: a, program: [x], category: read, colour: red}\n").is_err());
    assert!(from("format: shellspec-rules/9\n").is_err());
}

#[test]
fn the_digest_changes_with_the_rules() {
    let a = from("format: shellspec-rules/1\nrules:\n  - {id: a, program: [x], category: read}\n")
        .unwrap();
    let b = from("format: shellspec-rules/1\nrules:\n  - {id: a, program: [x], category: list}\n")
        .unwrap();
    assert_ne!(a.digest, b.digest);
}
