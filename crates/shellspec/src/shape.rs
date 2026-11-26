//! The redacted shape of a script (design/design.md § 7.1): node kinds, operators, program names
//! the built-in ruleset knows, subcommand paths, flag names and word kinds. It holds no value
//! of the input: an unknown program is `<prog>`, an operand `<arg>`, `<path>` or `<n>`, a flag's
//! value `<v>`, a dynamic word `<$>`.

use crate::invocation::{best_rule, is_ident, read_args, unwrap};
use crate::ir::*;
use crate::rules::Ruleset;

/// Redirect targets that are no value of the input.
const DEVICES: [&str; 5] = [
    "/dev/null",
    "/dev/stdout",
    "/dev/stderr",
    "/dev/stdin",
    "/dev/tty",
];

/// The redacted shape of a script, judged with the built-in ruleset.
pub fn shape(script: &Script) -> String {
    let rules = Ruleset::shared();
    let mut out = String::new();
    list(rules, &script.body, &mut out);
    out
}

fn list(rules: &Ruleset, body: &[Statement], out: &mut String) {
    for (i, s) in body.iter().enumerate() {
        node(rules, &s.body, out);
        match s.sep {
            Separator::Semi => out.push(';'),
            Separator::Amp => out.push_str(" &"),
            Separator::Newline => out.push('\n'),
            Separator::End => {}
        }
        if i + 1 < body.len() && s.sep != Separator::Newline {
            out.push(' ');
        }
    }
}

fn block(rules: &Ruleset, body: &[Statement], out: &mut String) {
    let mut inner = String::new();
    list(rules, body, &mut inner);
    out.push_str(inner.trim_end_matches([';', '\n']));
}

fn node(rules: &Ruleset, n: &Node, out: &mut String) {
    match n {
        Node::Command(c) => command(rules, c, out),
        Node::Pipeline(p) => {
            for (i, s) in p.stages.iter().enumerate() {
                if i > 0 {
                    out.push_str(match p.links[i - 1] {
                        PipeLink::Stdout => " | ",
                        PipeLink::StdoutStderr => " |& ",
                    });
                }
                node(rules, s, out);
            }
        }
        Node::AndOr(a) => {
            node(rules, &a.first, out);
            for item in &a.rest {
                out.push_str(match item.op {
                    AndOrOp::And => " && ",
                    AndOrOp::Or => " || ",
                });
                node(rules, &item.node, out);
            }
        }
        Node::Not(x) => {
            out.push_str("! ");
            node(rules, &x.body, out);
        }
        Node::Subshell(b) => {
            out.push('(');
            block(rules, &b.body, out);
            out.push(')');
            redirects(&b.redirects, out);
        }
        Node::Group(b) => {
            out.push_str("{ ");
            block(rules, &b.body, out);
            out.push_str("; }");
            redirects(&b.redirects, out);
        }
        Node::If(i) => {
            for (n, c) in i.clauses.iter().enumerate() {
                out.push_str(if n == 0 { "if " } else { "; elif " });
                block(rules, &c.condition, out);
                out.push_str("; then ");
                block(rules, &c.body, out);
            }
            if !i.otherwise.is_empty() {
                out.push_str("; else ");
                block(rules, &i.otherwise, out);
            }
            out.push_str("; fi");
            redirects(&i.redirects, out);
        }
        Node::While(l) | Node::Until(l) => {
            out.push_str(if matches!(n, Node::While(_)) {
                "while "
            } else {
                "until "
            });
            block(rules, &l.condition, out);
            out.push_str("; do ");
            block(rules, &l.body, out);
            out.push_str("; done");
            redirects(&l.redirects, out);
        }
        Node::For(f) => {
            out.push_str("for <var>");
            if let Some(items) = &f.items {
                out.push_str(" in");
                for w in items {
                    out.push(' ');
                    operand(w, out);
                }
            }
            out.push_str("; do ");
            block(rules, &f.body, out);
            out.push_str("; done");
            redirects(&f.redirects, out);
        }
        Node::Case(k) => {
            out.push_str("case ");
            operand(&k.word, out);
            out.push_str(" in");
            for arm in &k.arms {
                out.push_str(" <pattern>) ");
                block(rules, &arm.body, out);
                out.push_str(";;");
            }
            out.push_str(" esac");
        }
        Node::Function(f) => {
            out.push_str("<fn>() ");
            node(rules, &f.body, out);
        }
        Node::Test(t) => {
            out.push_str("[[");
            for w in &t.words {
                out.push(' ');
                operand(w, out);
            }
            out.push_str(" ]]");
        }
        Node::Arith(_) => out.push_str("(( <expr> ))"),
        Node::Unk(u) => {
            out.push_str("<unk:");
            out.push_str(u.reason.name());
            out.push('>');
        }
    }
}

fn command(rules: &Ruleset, c: &Command, out: &mut String) {
    let mut first = true;
    let mut sep = |out: &mut String| {
        if !first {
            out.push(' ');
        }
        first = false;
    };
    for _ in &c.assignments {
        sep(out);
        out.push_str("<var>=<v>");
    }
    let u = unwrap(rules, &c.words);
    for (name, from, to) in &u.wrappers {
        sep(out);
        out.push_str(name);
        let rule = rules.wrapper(name);
        for w in &c.words[from + 1..*to] {
            out.push(' ');
            let keyword = |v: &str| {
                rule.is_some_and(|r| {
                    r.skip_next.iter().any(|s| s == v)
                        || r.implies.iter().any(|i| i.after == v)
                        || r.until.as_deref() == Some(v)
                })
            };
            match w.value() {
                Some(v) if keyword(v) => out.push_str(v),
                Some(v) if v.starts_with('-') && v.len() > 1 => flag(v, out),
                _ => operand(w, out),
            }
        }
    }
    let head = if u.implied.is_some() {
        None
    } else {
        c.words.get(u.start)
    };
    if u.implied.is_some() || head.is_some() {
        let args = &c.words[(u.start + usize::from(head.is_some())).min(c.words.len())..];
        let name = match (&u.implied, head) {
            (Some(p), _) => Some(p.as_str()),
            (None, Some(p)) => p.value().map(|v| v.rsplit('/').next().unwrap_or(v)),
            (None, None) => None,
        };
        let program = name.and_then(|n| rules.program(n));
        let read = read_args(args, program);
        let rule_sub = name
            .and_then(|n| best_rule(rules, n, &read))
            .map(|r| r.sub.len())
            .unwrap_or(0);
        let ident_sub = read
            .positionals
            .iter()
            .take(program.and_then(|p| p.subcommands).unwrap_or(0))
            .take_while(|p| p.as_deref().is_some_and(is_ident))
            .count();
        let sub_len = rule_sub.max(ident_sub);
        if let Some(p) = head {
            sep(out);
            match (p.value(), name) {
                (Some(v), Some(n))
                    if !v.contains('/') && (rules.knows(n) || rules.wrapper(n).is_some()) =>
                {
                    out.push_str(n)
                }
                (Some(v), _) if v.contains('/') => out.push_str("<path>"),
                (Some(_), _) => out.push_str("<prog>"),
                (None, _) => dynamic(p, out),
            }
        }
        let mut positional = 0;
        let mut i = 0;
        let mut options_end = false;
        while i < args.len() {
            let w = &args[i];
            i += 1;
            out.push(' ');
            match w.value() {
                Some("--") if !options_end => {
                    options_end = true;
                    out.push_str("--");
                }
                Some(v) if !options_end && v.starts_with('-') && v.len() > 1 => {
                    flag(v, out);
                    let takes = !v.contains('=')
                        && program.is_some_and(|p| p.flags_with_value.iter().any(|f| f == v));
                    if takes && i < args.len() {
                        out.push_str(" <v>");
                        i += 1;
                    }
                }
                Some(v) if positional < sub_len => {
                    out.push_str(v);
                    positional += 1;
                }
                _ => {
                    positional += 1;
                    operand(w, out);
                }
            }
        }
    }
    redirects(&c.redirects, out);
}

/// A flag by name; digits and a value after `=` are redacted.
fn flag(v: &str, out: &mut String) {
    let (name, value) = match v.split_once('=') {
        Some((n, _)) => (n, true),
        None => (v, false),
    };
    let mut digits = false;
    for ch in name.chars() {
        if ch.is_ascii_digit() {
            if !digits {
                out.push_str("<n>");
            }
            digits = true;
        } else if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
            digits = false;
            out.push(ch);
        } else {
            out.push_str("<v>");
            return;
        }
    }
    if value {
        out.push_str("=<v>");
    }
}

fn operand(w: &Word, out: &mut String) {
    match w.value() {
        Some("]") => out.push(']'),
        Some(v) if !v.is_empty() && v.chars().all(|c| c.is_ascii_digit()) => out.push_str("<n>"),
        Some(v) if v.contains('/') || v.contains('.') => out.push_str("<path>"),
        Some(_) => out.push_str("<arg>"),
        None => dynamic(w, out),
    }
}

fn dynamic(w: &Word, out: &mut String) {
    let mut subs = String::new();
    parts_subs(&w.parts, &mut subs);
    if subs.is_empty() {
        out.push_str("<$>");
    } else {
        out.push_str(&subs);
    }
}

fn parts_subs(parts: &[WordPart], out: &mut String) {
    let rules = Ruleset::shared();
    for p in parts {
        match p {
            WordPart::CmdSub(s) => {
                out.push_str("$(");
                block(rules, &s.body.body, out);
                out.push(')');
            }
            WordPart::ProcSub(s) => {
                out.push_str(match s.dir {
                    ProcDir::In => "<(",
                    ProcDir::Out => ">(",
                    ProcDir::File => "=(",
                });
                block(rules, &s.body.body, out);
                out.push(')');
            }
            WordPart::DQuoted(d) => parts_subs(&d.parts, out),
            _ => {}
        }
    }
}

fn redirects(list: &[Redirect], out: &mut String) {
    for r in list {
        out.push(' ');
        if let Some(fd) = r.fd {
            out.push_str(&fd.to_string());
        }
        out.push_str(match r.op {
            RedirectOp::In => "<",
            RedirectOp::Out => ">",
            RedirectOp::Append => ">>",
            RedirectOp::Clobber => ">|",
            RedirectOp::OutNoclobber => ">?",
            RedirectOp::Inout => "<>",
            RedirectOp::DupOut => ">&",
            RedirectOp::DupIn => "<&",
            RedirectOp::Both => "&>",
            RedirectOp::Herestring => "<<<",
            RedirectOp::Heredoc => "<<",
        });
        match &r.target {
            RedirectTarget::Fd(f) => out.push_str(&f.fd.to_string()),
            RedirectTarget::File(w) => match w.value() {
                Some("-") => out.push('-'),
                Some(v) if DEVICES.contains(&v) => {
                    out.push(' ');
                    out.push_str(v)
                }
                _ => {
                    out.push(' ');
                    operand(w, out)
                }
            },
            RedirectTarget::Heredoc(h) => {
                out.push_str(if h.quoted { "'EOF'" } else { "EOF" });
                let mut subs = String::new();
                parts_subs(&h.body.parts, &mut subs);
                if !subs.is_empty() {
                    out.push_str(" [");
                    out.push_str(&subs);
                    out.push(']');
                }
            }
            RedirectTarget::ProcSub(p) => {
                let mut s = String::new();
                parts_subs(&[WordPart::ProcSub(p.clone())], &mut s);
                out.push_str(&s);
            }
        }
    }
}
