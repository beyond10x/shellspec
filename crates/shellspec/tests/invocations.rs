use shellspec::ir::{AndOrOp, UnkReason};
use shellspec::{Category, Effect, Guard, InvocationContext, StageRole, dominant};

use crate::{invs, summary};

#[test]
fn the_common_shape_is_three_invocations() {
    let list = invs("cd x && slot cargo test 2>&1 | tail -20");
    assert_eq!(summary(&list), ["cd:navigate", "cargo:test", "tail:filter"]);
    let (cd, test, tail) = (&list[0], &list[1], &list[2]);
    assert_eq!(test.wrappers, ["slot"]);
    assert_eq!(test.subcommand, ["test"]);
    assert_eq!(test.rule.as_deref(), Some("cargo.test"));
    assert_eq!(test.operation.as_deref(), Some("test.cargo"));
    assert_eq!(cd.cwd.as_deref(), Some("."));
    assert_eq!(test.cwd.as_deref(), Some("x"));
    assert_eq!(
        test.guard,
        [Guard {
            op: AndOrOp::And,
            after: 0
        }]
    );
    assert_eq!(tail.guard, test.guard);
    assert_eq!(test.stage.pipeline, tail.stage.pipeline);
    assert_eq!(
        (test.stage.index, test.stage.of, test.stage.role),
        (0, 2, StageRole::Head)
    );
    assert_eq!((tail.stage.index, tail.stage.role), (1, StageRole::Tail));
    assert_eq!(cd.stage.role, StageRole::Alone);
    assert_eq!(tail.flags, ["-20"]);
    assert_eq!(dominant(&list).map(|d| d.index), Some(1));
}

#[test]
fn substitutions_run_before_their_command() {
    let list = invs("echo \"$(git rev-parse HEAD)\" > out.txt");
    assert_eq!(summary(&list), ["git:vcs", "echo:edit"]);
    assert_eq!(list[0].context, InvocationContext::Cmdsub);
    assert_eq!(list[1].context, InvocationContext::Top);
    assert!(list[1].effects.contains(&Effect::FsWrite));
}

#[test]
fn contexts_of_compound_bodies() {
    let list = invs(
        "for f in a b; do wc -l $f; done; (ls); f() { date; }; cat <<EOF\n$(hostname)\nEOF\ndiff <(sort a) b",
    );
    let ctx: Vec<_> = list
        .iter()
        .map(|i| (i.program.clone().unwrap(), i.context))
        .collect();
    assert_eq!(
        ctx,
        [
            ("wc".to_owned(), InvocationContext::LoopBody),
            ("ls".to_owned(), InvocationContext::Subshell),
            ("date".to_owned(), InvocationContext::FunctionBody),
            ("hostname".to_owned(), InvocationContext::Heredoc),
            ("cat".to_owned(), InvocationContext::Top),
            ("sort".to_owned(), InvocationContext::Procsub),
            ("diff".to_owned(), InvocationContext::Top),
        ]
    );
}

#[test]
fn cwd_follows_static_cd_and_a_subshell_restores_it() {
    let list = invs("cd a/b && cd ../c && (cd d; ls) && pwd; cd \"$x\"; ls; cd /abs; ls");
    let cwd: Vec<Option<&str>> = list.iter().map(|i| i.cwd.as_deref()).collect();
    assert_eq!(
        cwd,
        [
            Some("."),
            Some("a/b"),
            Some("a/c"),
            Some("a/c/d"),
            Some("a/c"),
            Some("a/c"),
            None,
            None,
            Some("/abs")
        ]
    );
}

#[test]
fn guards_name_the_invocation_whose_status_decides() {
    let list = invs("a | b && c || d");
    assert_eq!(
        list[2].guard,
        [Guard {
            op: AndOrOp::And,
            after: 1
        }]
    );
    assert_eq!(
        list[3].guard,
        [Guard {
            op: AndOrOp::Or,
            after: 2
        }]
    );
    let list = invs("x && { y; z; }");
    assert_eq!(list[1].guard, list[2].guard);
    assert_eq!(
        list[2].guard,
        [Guard {
            op: AndOrOp::And,
            after: 0
        }]
    );
}

#[test]
fn wrappers_are_unwrapped_and_recorded() {
    let list = invs(
        "timeout -s KILL 300 env -u X RUST_LOG=debug nice -n 5 cargo build; slot status; slot run make; sudo -u root systemctl status x; b10x-gates --repo . bot -- push origin main; xargs -I{} grep -l x {}",
    );
    assert_eq!(
        summary(&list),
        [
            "cargo:build",
            "slot:tool",
            "make:build",
            "systemctl:system",
            "git:vcs",
            "grep:search"
        ]
    );
    assert_eq!(list[0].wrappers, ["timeout", "env", "nice"]);
    assert_eq!(list[0].assignments, ["RUST_LOG"]);
    assert!(list[1].wrappers.is_empty());
    assert_eq!(list[2].wrappers, ["slot"]);
    assert_eq!(list[4].wrappers, ["b10x-gates"]);
    assert_eq!(list[4].subcommand, ["push"]);
    assert!(list[4].effects.contains(&Effect::RemoteWrite));
    assert_eq!(list[5].wrappers, ["xargs"]);
}

#[test]
fn flag_conditions_refine_the_rule() {
    let list = invs(
        "sed -n 1,80p f; sed -i s/a/b/ f; git push --force origin x; git push; curl -sS -X POST https://h/x; curl -s https://h/y; find . -name x -delete; grep -rn todo src",
    );
    assert_eq!(
        summary(&list),
        [
            "sed:read",
            "sed:edit",
            "git:vcs",
            "git:vcs",
            "curl:network",
            "curl:network",
            "find:edit",
            "grep:search"
        ]
    );
    assert_eq!(list[2].operation.as_deref(), Some("vcs.push.force"));
    assert_eq!(list[3].operation.as_deref(), Some("vcs.push"));
    assert!(list[4].effects.contains(&Effect::RemoteWrite));
    assert!(!list[5].effects.contains(&Effect::RemoteWrite));
    assert!(list[6].effects.contains(&Effect::FsDelete));
}

#[test]
fn a_stream_program_is_a_filter_only_in_a_pipe() {
    let list = invs(
        "grep -n x f; cat f | grep -v y | head -n 5; head -n 5 f; wc -l < f; jq .a f.json; echo {} | jq .a",
    );
    assert_eq!(
        summary(&list),
        [
            "grep:search",
            "cat:read",
            "grep:filter",
            "head:filter",
            "head:read",
            "wc:read",
            "jq:read",
            "echo:output",
            "jq:filter"
        ]
    );
    let grep = &list[2];
    assert!(grep.effects.contains(&Effect::Stdin));
}

#[test]
fn script_names_choose_the_category() {
    let list = invs(
        "npm run test:unit; npm run dev; npm test; pnpm install; task check; task deploy; make; make lint; yarn build",
    );
    assert_eq!(
        summary(&list),
        [
            "npm:test",
            "npm:run",
            "npm:test",
            "pnpm:package",
            "task:test",
            "task:run",
            "make:build",
            "make:lint",
            "yarn:build"
        ]
    );
}

#[test]
fn subcommand_paths_skip_global_flags() {
    let list = invs(
        "git -C repo -c a=b log --oneline -5; cargo +nightly nextest run; gh pr create --title t; aep plan artifact validate x",
    );
    let subs: Vec<Vec<String>> = list.iter().map(|i| i.subcommand.clone()).collect();
    assert_eq!(
        subs,
        [
            vec!["log".to_owned()],
            vec!["nextest".to_owned()],
            vec!["pr".to_owned(), "create".to_owned()],
            vec!["plan".to_owned()],
        ]
    );
    assert_eq!(list[1].category, Category::Test);
    assert!(list[2].effects.contains(&Effect::RemoteWrite));
}

#[test]
fn nested_scripts_are_lifted_to_depth_three() {
    let list = invs("bash -c 'cargo build && ls' ; eval \"rm -rf x\"; ssh host 'rm -rf /srv/x'");
    assert_eq!(
        summary(&list),
        [
            "bash:run",
            "cargo:build",
            "ls:list",
            "eval:environment",
            "rm:edit",
            "ssh:network",
            "rm:edit"
        ]
    );
    assert_eq!(list[1].context, InvocationContext::Nested);
    assert_eq!(list[1].depth, 1);
    assert!(!list[4].remote);
    assert!(list[6].remote);
    assert_eq!(list[6].cwd, None);
    assert_eq!(dominant(&list).map(|d| d.index), Some(1));

    let deep = invs("eval eval eval eval ls");
    assert_eq!(deep.last().unwrap().unk, Some(UnkReason::Depth));
    assert_eq!(deep.len(), 5);

    let dynamic = invs("sh -c \"$SCRIPT\"");
    assert_eq!(dynamic[1].unk, Some(UnkReason::Dynamic));
}

#[test]
fn unk_has_its_reason() {
    let list =
        invs("$CC -o x x.c; ./target/debug/tool --x; frobnicate now; f() { :; }; f; echo `x");
    let unk: Vec<_> = list
        .iter()
        .map(|i| (i.category, i.unk, i.rule.clone()))
        .collect();
    assert_eq!(unk[0], (Category::Unk, Some(UnkReason::Dynamic), None));
    assert_eq!(
        unk[1],
        (Category::Run, None, Some("builtin.path".to_owned()))
    );
    assert_eq!(unk[2], (Category::Unk, Some(UnkReason::NoRule), None));
    assert_eq!(
        unk[4],
        (Category::Run, None, Some("builtin.function".to_owned()))
    );
    assert_eq!(unk[5], (Category::Unk, Some(UnkReason::Syntax), None));
}

#[test]
fn bare_redirects_and_assignments() {
    let list = invs("X=1; > out.txt; X=1 Y=2 cargo test");
    assert_eq!(summary(&list), ["-:environment", "-:edit", "cargo:test"]);
    assert_eq!(list[2].assignments, ["X", "Y"]);
}

#[test]
fn dominant_skips_shaping_and_shell_state() {
    let list = invs("cd x && export A=1 && echo start && git log | head");
    assert_eq!(
        dominant(&list).and_then(|d| d.program.as_deref()),
        Some("git")
    );
    assert!(dominant(&invs("cd x && echo hi | tail -1")).is_none());
}
