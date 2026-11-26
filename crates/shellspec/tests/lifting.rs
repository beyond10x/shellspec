use shellspec::ir::*;
use shellspec::{Dialect, lift};

use crate::zsh;

fn only(script: &Script) -> &Node {
    assert_eq!(script.body.len(), 1, "{script:#?}");
    &script.body[0].body
}

fn text(input: &str, span: Span) -> &str {
    &input[span.start..span.end]
}

#[test]
fn and_binds_looser_than_pipe() {
    // tree-sitter-bash nests this as `(a && b 2>&1) | c` (docs/design.md § 5.3).
    let s = zsh("a && b 2>&1 | c");
    let Node::AndOr(a) = only(&s) else {
        panic!("{s:#?}")
    };
    assert!(matches!(*a.first, Node::Command(_)));
    assert_eq!(a.rest.len(), 1);
    assert_eq!(a.rest[0].op, AndOrOp::And);
    let Node::Pipeline(p) = &a.rest[0].node else {
        panic!("{a:#?}")
    };
    assert_eq!(p.stages.len(), 2);
    let Node::Command(b) = &p.stages[0] else {
        panic!()
    };
    assert_eq!(b.redirects.len(), 1);
    assert_eq!(b.redirects[0].fd, Some(2));
    assert_eq!(b.redirects[0].op, RedirectOp::DupOut);
    assert_eq!(
        b.redirects[0].target,
        RedirectTarget::Fd(FdTarget { fd: 1 })
    );
}

#[test]
fn separators_are_kept_apart() {
    let s = zsh("a; b\nc & d");
    let seps: Vec<Separator> = s.body.iter().map(|st| st.sep).collect();
    assert_eq!(
        seps,
        [
            Separator::Semi,
            Separator::Newline,
            Separator::Amp,
            Separator::End
        ]
    );
}

#[test]
fn pipe_with_stderr_is_a_link_not_a_redirect() {
    let s = zsh("make |& tee log");
    let Node::Pipeline(p) = only(&s) else {
        panic!()
    };
    assert_eq!(p.links, [PipeLink::StdoutStderr]);
    let Node::Command(c) = &p.stages[0] else {
        panic!()
    };
    assert!(c.redirects.is_empty(), "{c:#?}");
}

#[test]
fn heredoc_goes_to_the_command_that_reads_it() {
    let input = "cat <<EOF && echo done\nhello $(whoami)\nEOF\n";
    let s = zsh(input);
    let Node::AndOr(a) = only(&s) else {
        panic!("{s:#?}")
    };
    let Node::Command(cat) = &*a.first else {
        panic!()
    };
    let RedirectTarget::Heredoc(h) = &cat.redirects[0].target else {
        panic!("{cat:#?}")
    };
    assert_eq!(h.delimiter, "EOF");
    assert!(!h.quoted);
    assert!(
        h.body
            .parts
            .iter()
            .any(|p| matches!(p, WordPart::CmdSub(_)))
    );
    let Node::Command(echo) = &a.rest[0].node else {
        panic!()
    };
    assert!(echo.redirects.is_empty());

    let quoted = zsh("cat <<'EOF'\n$(not lifted)\nEOF\n");
    let Node::Command(c) = only(&quoted) else {
        panic!()
    };
    let RedirectTarget::Heredoc(h) = &c.redirects[0].target else {
        panic!()
    };
    assert!(h.quoted);
    assert!(h.body.parts.iter().all(|p| matches!(p, WordPart::Lit(_))));
}

#[test]
fn substitutions_hold_scripts() {
    let input = "echo \"head: $(git rev-parse HEAD)\" `date` <(sort a)";
    let s = zsh(input);
    let Node::Command(c) = only(&s) else { panic!() };
    let Some(WordPart::DQuoted(d)) = c.words[1].parts.first() else {
        panic!("{:#?}", c.words[1])
    };
    let Some(WordPart::CmdSub(sub)) = d.parts.iter().find(|p| matches!(p, WordPart::CmdSub(_)))
    else {
        panic!()
    };
    assert_eq!(sub.form, SubForm::Dollar);
    let Node::Command(git) = &sub.body.body[0].body else {
        panic!()
    };
    assert_eq!(text(input, git.words[0].span), "git");
    assert!(matches!(
        c.words[2].parts[0],
        WordPart::CmdSub(CmdSub {
            form: SubForm::Backtick,
            ..
        })
    ));
    assert!(matches!(
        c.words[3].parts[0],
        WordPart::ProcSub(ProcSub {
            dir: ProcDir::In,
            ..
        })
    ));
    assert_eq!(c.words[1].static_value, None);
}

#[test]
fn spans_are_bytes() {
    let input = "echo 'grüße' && ls -la";
    let s = zsh(input);
    let Node::AndOr(a) = only(&s) else { panic!() };
    let Node::Command(ls) = &a.rest[0].node else {
        panic!()
    };
    assert_eq!(text(input, ls.span), "ls -la");
    assert_eq!(text(input, ls.words[1].span), "-la");
}

#[test]
fn words_know_whether_they_are_static() {
    let s = zsh("cmd plain 'single' \"dq\" \"$x\" *.rs ~/y a\\ b");
    let Node::Command(c) = only(&s) else { panic!() };
    let values: Vec<Option<&str>> = c.words.iter().map(Word::value).collect();
    assert_eq!(
        values,
        [
            Some("cmd"),
            Some("plain"),
            Some("single"),
            Some("dq"),
            None,
            None,
            None,
            Some("a b")
        ]
    );
    assert!(matches!(c.words[5].parts[0], WordPart::Glob(_)));
    assert!(matches!(c.words[6].parts[0], WordPart::Tilde(_)));
}

#[test]
fn compound_commands_are_lifted() {
    let s = zsh(
        "if [ -f a ]; then cat a; elif true; then :; else echo no; fi; for f in a b; do wc -l $f; done; while read l; do echo $l; done < in; case $x in a) ls;; *) pwd;; esac; f() { echo; }; (cd sub && ls); { a; b; } > out; [[ -n $x ]]; (( i++ ))",
    );
    let kinds: Vec<&str> = s
        .body
        .iter()
        .map(|st| match &st.body {
            Node::If(_) => "if",
            Node::For(_) => "for",
            Node::While(_) => "while",
            Node::Case(_) => "case",
            Node::Function(_) => "function",
            Node::Subshell(_) => "subshell",
            Node::Group(_) => "group",
            Node::Test(_) => "test",
            Node::Arith(_) => "arith",
            _ => "other",
        })
        .collect();
    assert_eq!(
        kinds,
        [
            "if", "for", "while", "case", "function", "subshell", "group", "test", "arith"
        ]
    );
    let Node::If(i) = &s.body[0].body else {
        panic!()
    };
    assert_eq!(i.clauses.len(), 2);
    assert_eq!(i.otherwise.len(), 1);
    let Node::While(w) = &s.body[2].body else {
        panic!()
    };
    assert_eq!(w.redirects[0].op, RedirectOp::In);
    let Node::Group(g) = &s.body[6].body else {
        panic!()
    };
    assert_eq!(g.redirects[0].op, RedirectOp::Out);
}

#[test]
fn refused_input_keeps_the_parts_that_parse() {
    let input = "echo ok; echo `unterminated";
    let s = zsh(input);
    assert_eq!(s.body.len(), 2, "{s:#?}");
    assert!(matches!(s.body[0].body, Node::Command(_)));
    let Node::Unk(u) = &s.body[1].body else {
        panic!("{s:#?}")
    };
    assert_eq!(u.reason, UnkReason::Syntax);
    assert_eq!(u.len, "echo `unterminated".len());
    assert_eq!(u.text_digest.len(), 64);
    assert_eq!(s.diagnostics.len(), 1);
    assert_eq!(s.diagnostics[0].code, "tokenize_unterminated_backquote");
    assert_eq!(s.diagnostics[0].parser, shellspec::lift::BRUSH_PARSER);
}

#[test]
fn zsh_only_syntax_is_unk_dialect_or_read_as_zsh() {
    let s = zsh("diff =(ls a) b");
    let Node::Unk(u) = only(&s) else {
        panic!("{s:#?}")
    };
    assert_eq!(u.reason, UnkReason::Dialect);

    // `&!` runs in the background and disowns; it is no `!` negation.
    let s = zsh("sleep 5 &! echo next");
    assert_eq!(s.body.len(), 2, "{s:#?}");
    assert_eq!(s.body[0].sep, Separator::Amp);
    assert!(matches!(s.body[1].body, Node::Command(_)));
}

#[test]
fn sh_dialect_refuses_bash_only_syntax() {
    let s = lift("diff <(a) <(b)", Dialect::Sh);
    assert!(s.diagnostics.iter().all(|d| d.reason == UnkReason::Syntax));
    assert!(matches!(only(&s), Node::Unk(_)));
    assert!(matches!(
        only(&lift("diff <(a) <(b)", Dialect::Bash)),
        Node::Command(_)
    ));
}

#[test]
fn lifting_is_pure_and_never_fails() {
    for input in [
        "",
        "   ",
        "(((",
        "'",
        "\"$(",
        "a |",
        "&& b",
        "}{",
        "case x in",
        "\u{0}",
    ] {
        let a = zsh(input);
        let b = zsh(input);
        assert_eq!(a, b, "{input:?}");
        assert_eq!(a.span, Span::new(0, input.len()));
    }
}

#[test]
fn wire_form_is_tagged_and_leaves_absent_fields_out() {
    let s = zsh("echo $(x)");
    let v = serde_json::to_value(&s).unwrap();
    assert_eq!(v["dialect"], "zsh");
    let node = &v["body"][0]["body"];
    assert_eq!(node["kind"], "Command");
    let word = &node["value"]["words"][1];
    assert!(word.get("static_value").is_none(), "{word}");
    assert_eq!(word["parts"][0]["kind"], "CmdSub");
    assert_eq!(word["parts"][0]["value"]["form"], "dollar");
    let back: Script = serde_json::from_value(v).unwrap();
    assert_eq!(back, s);
}
