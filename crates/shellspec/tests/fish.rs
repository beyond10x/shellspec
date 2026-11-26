use shellspec::ir::{AndOrOp, Node, PipeLink, Separator, UnkReason};
use shellspec::{Dialect, Guard, InvocationContext, lift};

use crate::{invs_in, summary};

#[test]
fn fish_lifts_into_the_same_ir() {
    let list = invs_in("cd x && slot cargo test 2>&1 | tail -20", Dialect::Fish);
    assert_eq!(summary(&list), ["cd:navigate", "cargo:test", "tail:filter"]);
    assert_eq!(list[1].wrappers, ["slot"]);
    assert_eq!(list[1].cwd.as_deref(), Some("x"));
}

#[test]
fn fish_and_or_keywords_chain_the_statement_before() {
    let s = lift("make; and make install; or echo failed", Dialect::Fish);
    assert_eq!(s.body.len(), 1, "{s:#?}");
    let Node::AndOr(a) = &s.body[0].body else {
        panic!("{s:#?}")
    };
    let ops: Vec<AndOrOp> = a.rest.iter().map(|i| i.op).collect();
    assert_eq!(ops, [AndOrOp::And, AndOrOp::Or]);
    let list = invs_in("a && b && c", Dialect::Fish);
    assert_eq!(
        list[2].guard,
        [Guard {
            op: AndOrOp::And,
            after: 1
        }]
    );
}

#[test]
fn fish_blocks_substitutions_and_pipes() {
    let s = lift(
        "begin; ls; end > out; echo (date) &| less; not test -f x; sleep 1 &",
        Dialect::Fish,
    );
    let kinds: Vec<&str> = s
        .body
        .iter()
        .map(|st| match &st.body {
            Node::Group(_) => "group",
            Node::Pipeline(_) => "pipeline",
            Node::Not(_) => "not",
            Node::Command(_) => "command",
            _ => "other",
        })
        .collect();
    assert_eq!(kinds, ["group", "pipeline", "not", "command"]);
    assert_eq!(s.body[3].sep, Separator::Amp);
    let Node::Pipeline(p) = &s.body[1].body else {
        panic!()
    };
    assert_eq!(p.links, [PipeLink::StdoutStderr]);
    let list = invs_in(
        "echo (git rev-parse HEAD); FOO=1 cargo build",
        Dialect::Fish,
    );
    assert_eq!(list[0].context, InvocationContext::Cmdsub);
    assert_eq!(list[2].assignments, ["FOO"]);
    assert_eq!(summary(&list), ["git:vcs", "echo:output", "cargo:build"]);
}

#[test]
fn fish_control_flow() {
    let list = invs_in(
        "if test -f x; cat x; else; echo no; end; for f in *.rs; wc -l $f; end; while true; sleep 1; end; switch $x; case a; ls; end; function g; pwd; end",
        Dialect::Fish,
    );
    assert_eq!(
        summary(&list),
        [
            "test:output",
            "cat:read",
            "echo:output",
            "wc:read",
            "true:output",
            "sleep:process",
            "ls:list",
            "pwd:output"
        ]
    );
    assert_eq!(list[3].context, InvocationContext::LoopBody);
    assert_eq!(list[7].context, InvocationContext::FunctionBody);
}

#[test]
fn fish_errors_become_unk_and_siblings_stay() {
    let s = lift("echo ok; echo (", Dialect::Fish);
    assert!(
        s.body
            .iter()
            .any(|st| matches!(st.body, Node::Unk(ref u) if u.reason == UnkReason::Syntax)),
        "{s:#?}"
    );
    assert!(!s.diagnostics.is_empty());
    let list = invs_in("echo ok; echo (", Dialect::Fish);
    assert_eq!(list[0].program.as_deref(), Some("echo"));
}
