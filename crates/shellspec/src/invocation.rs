//! The flat view (docs/design.md § 3.4, `spec/domains/invocation.yaml`): one [`Invocation`] per
//! simple command, in execution order, classified by a [`Ruleset`] (§ 6.2).

use std::collections::HashSet;

use serde::{Deserialize, Serialize};

use crate::ir::*;
use crate::rules::{Category, Effect, NestedKind, ProgramRule, Rule, Ruleset};

/// Nested scripts (`bash -c`, `eval`, `ssh host …`) are lifted to this depth (§ 3.2).
pub const MAX_NESTED_DEPTH: usize = 3;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StageRole {
    Alone,
    Head,
    Middle,
    Tail,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Stage {
    pub pipeline: usize,
    pub index: usize,
    pub of: usize,
    pub role: StageRole,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Guard {
    pub op: AndOrOp,
    pub after: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InvocationContext {
    Top,
    Cmdsub,
    Procsub,
    Subshell,
    Heredoc,
    Nested,
    LoopBody,
    FunctionBody,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Invocation {
    pub index: usize,
    pub path: Vec<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub program: Option<String>,
    pub subcommand: Vec<String>,
    pub wrappers: Vec<String>,
    pub assignments: Vec<String>,
    pub flags: Vec<String>,
    pub argv: Vec<Word>,
    pub redirects: Vec<Redirect>,
    pub stage: Stage,
    pub guard: Vec<Guard>,
    pub context: InvocationContext,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cwd: Option<String>,
    pub remote: bool,
    pub depth: usize,
    pub category: Category,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub operation: Option<String>,
    pub effects: Vec<Effect>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rule: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unk: Option<UnkReason>,
}

/// Every simple command of the script, in execution order, classified.
pub fn invocations(script: &Script, rules: &Ruleset) -> Vec<Invocation> {
    let mut functions = HashSet::new();
    collect_functions(&script.body, &mut functions);
    let mut w = Walk {
        rules,
        out: Vec::new(),
        pipelines: 0,
        functions,
    };
    let ctx = Ctx {
        context: InvocationContext::Top,
        guard: Vec::new(),
        depth: 0,
        remote: false,
        dialect: script.dialect,
        path: Vec::new(),
    };
    let mut cwd = Some(".".to_owned());
    w.list(&script.body, &ctx, &mut cwd);
    w.out
}

/// § 7.1: the first invocation that is not navigate, filter, output or environment, and that
/// does not only carry a nested script.
pub fn dominant(list: &[Invocation]) -> Option<&Invocation> {
    list.iter().find(|inv| {
        !matches!(
            inv.category,
            Category::Navigate | Category::Filter | Category::Output | Category::Environment
        ) && !list.iter().any(|other| {
            other.index > inv.index
                && other.depth == inv.depth + 1
                && other.context == InvocationContext::Nested
                && other.path.starts_with(&inv.path)
        })
    })
}

#[derive(Clone)]
struct Ctx {
    context: InvocationContext,
    guard: Vec<Guard>,
    depth: usize,
    remote: bool,
    dialect: Dialect,
    path: Vec<usize>,
}

impl Ctx {
    fn at(&self, i: usize) -> Ctx {
        let mut c = self.clone();
        c.path.push(i);
        c
    }

    fn within(&self, context: InvocationContext) -> Ctx {
        let mut c = self.clone();
        c.context = context;
        c
    }
}

struct Walk<'r> {
    rules: &'r Ruleset,
    out: Vec<Invocation>,
    pipelines: usize,
    /// Names of the functions the script defines; a call of one is `run`.
    functions: HashSet<String>,
}

fn collect_functions(list: &[Statement], out: &mut HashSet<String>) {
    for s in list {
        collect_in(&s.body, out);
    }
}

fn collect_in(n: &Node, out: &mut HashSet<String>) {
    match n {
        Node::Function(f) => {
            out.insert(f.name.clone());
            collect_in(&f.body, out);
        }
        Node::AndOr(a) => {
            collect_in(&a.first, out);
            for i in &a.rest {
                collect_in(&i.node, out);
            }
        }
        Node::Not(x) => collect_in(&x.body, out),
        Node::Pipeline(p) => p.stages.iter().for_each(|s| collect_in(s, out)),
        Node::Subshell(b) | Node::Group(b) => collect_functions(&b.body, out),
        Node::If(i) => {
            for c in &i.clauses {
                collect_functions(&c.condition, out);
                collect_functions(&c.body, out);
            }
            collect_functions(&i.otherwise, out);
        }
        Node::While(l) | Node::Until(l) => {
            collect_functions(&l.condition, out);
            collect_functions(&l.body, out);
        }
        Node::For(f) => collect_functions(&f.body, out),
        Node::Case(c) => c.arms.iter().for_each(|a| collect_functions(&a.body, out)),
        Node::Command(_) | Node::Test(_) | Node::Arith(_) | Node::Unk(_) => {}
    }
}

const ALONE: StageRole = StageRole::Alone;

impl Walk<'_> {
    fn new_stage(&mut self) -> Stage {
        let pipeline = self.pipelines;
        self.pipelines += 1;
        Stage {
            pipeline,
            index: 0,
            of: 1,
            role: ALONE,
        }
    }

    /// Returns the index of the last invocation, the one whose status the list ends with.
    fn list(&mut self, list: &[Statement], ctx: &Ctx, cwd: &mut Option<String>) -> Option<usize> {
        let mut last = None;
        for (i, s) in list.iter().enumerate() {
            let c = ctx.at(i);
            if let Some(l) = self.node(&s.body, &c, cwd, None) {
                last = Some(l);
            }
        }
        last
    }

    fn node(
        &mut self,
        node: &Node,
        ctx: &Ctx,
        cwd: &mut Option<String>,
        stage: Option<Stage>,
    ) -> Option<usize> {
        match node {
            Node::Command(c) => {
                let stage = stage.unwrap_or_else(|| self.new_stage());
                Some(self.command(c, ctx, cwd, stage))
            }
            Node::Pipeline(p) => {
                let pipeline = self.pipelines;
                self.pipelines += 1;
                let of = p.stages.len();
                let mut last = None;
                for (i, s) in p.stages.iter().enumerate() {
                    let role = match i {
                        0 => StageRole::Head,
                        i if i + 1 == of => StageRole::Tail,
                        _ => StageRole::Middle,
                    };
                    let stage = Stage {
                        pipeline,
                        index: i,
                        of,
                        role,
                    };
                    let mut own = cwd.clone();
                    last = self.node(s, &ctx.at(i), &mut own, Some(stage)).or(last);
                }
                last
            }
            Node::AndOr(a) => {
                let mut last = self.node(&a.first, &ctx.at(0), cwd, stage);
                for (i, item) in a.rest.iter().enumerate() {
                    let mut c = ctx.at(i + 1);
                    if let Some(after) = last {
                        c.guard.push(Guard { op: item.op, after });
                    }
                    last = self.node(&item.node, &c, cwd, None).or(last);
                }
                last
            }
            Node::Not(n) => self.node(&n.body, ctx, cwd, stage),
            Node::Subshell(b) => {
                let mut own = cwd.clone();
                self.redirect_subs(&b.redirects, ctx, cwd);
                self.list(&b.body, &ctx.within(InvocationContext::Subshell), &mut own)
            }
            Node::Group(b) => {
                self.redirect_subs(&b.redirects, ctx, cwd);
                self.list(&b.body, ctx, cwd)
            }
            Node::If(i) => {
                self.redirect_subs(&i.redirects, ctx, cwd);
                let mut last = None;
                for (n, clause) in i.clauses.iter().enumerate() {
                    let c = ctx.at(n);
                    last = self.list(&clause.condition, &c.at(0), cwd).or(last);
                    last = self.list(&clause.body, &c.at(1), cwd).or(last);
                }
                self.list(&i.otherwise, &ctx.at(i.clauses.len()), cwd)
                    .or(last)
            }
            Node::While(l) | Node::Until(l) => {
                self.redirect_subs(&l.redirects, ctx, cwd);
                let c = ctx.within(InvocationContext::LoopBody);
                let last = self.list(&l.condition, &c.at(0), cwd);
                self.list(&l.body, &c.at(1), cwd).or(last)
            }
            Node::For(f) => {
                self.redirect_subs(&f.redirects, ctx, cwd);
                for w in f.items.iter().flatten() {
                    self.word_subs(w, ctx, cwd);
                }
                let c = ctx.within(InvocationContext::LoopBody);
                self.list(&f.body, &c, cwd)
            }
            Node::Case(k) => {
                self.redirect_subs(&k.redirects, ctx, cwd);
                self.word_subs(&k.word, ctx, cwd);
                let mut last = None;
                for (n, arm) in k.arms.iter().enumerate() {
                    last = self.list(&arm.body, &ctx.at(n), cwd).or(last);
                }
                last
            }
            Node::Function(f) => {
                let mut own = cwd.clone();
                let c = ctx.within(InvocationContext::FunctionBody);
                self.node(&f.body, &c, &mut own, None)
            }
            Node::Test(t) => {
                for w in &t.words {
                    self.word_subs(w, ctx, cwd);
                }
                self.redirect_subs(&t.redirects, ctx, cwd);
                None
            }
            Node::Arith(_) => None,
            Node::Unk(u) => {
                let stage = stage.unwrap_or_else(|| self.new_stage());
                Some(self.push_unk(ctx, cwd, stage, u.reason))
            }
        }
    }

    fn push_unk(
        &mut self,
        ctx: &Ctx,
        cwd: &Option<String>,
        stage: Stage,
        reason: UnkReason,
    ) -> usize {
        self.push(Invocation {
            index: 0,
            path: ctx.path.clone(),
            program: None,
            subcommand: Vec::new(),
            wrappers: Vec::new(),
            assignments: Vec::new(),
            flags: Vec::new(),
            argv: Vec::new(),
            redirects: Vec::new(),
            stage,
            guard: ctx.guard.clone(),
            context: ctx.context,
            cwd: cwd.clone(),
            remote: ctx.remote,
            depth: ctx.depth,
            category: Category::Unk,
            operation: None,
            effects: Vec::new(),
            rule: None,
            unk: Some(reason),
        })
    }

    fn push(&mut self, mut inv: Invocation) -> usize {
        inv.index = self.out.len();
        self.out.push(inv);
        self.out.len() - 1
    }

    fn script(&mut self, s: &Script, ctx: &Ctx, cwd: &Option<String>) {
        let mut own = cwd.clone();
        let mut c = ctx.clone();
        c.dialect = s.dialect;
        self.list(&s.body, &c, &mut own);
    }

    /// Substitutions inside a word run before the command that holds it.
    fn word_subs(&mut self, w: &Word, ctx: &Ctx, cwd: &Option<String>) {
        self.parts_subs(&w.parts, ctx, cwd);
    }

    fn parts_subs(&mut self, parts: &[WordPart], ctx: &Ctx, cwd: &Option<String>) {
        for p in parts {
            match p {
                WordPart::CmdSub(s) => {
                    // A substitution in a heredoc body keeps the heredoc context.
                    let c = if ctx.context == InvocationContext::Heredoc {
                        ctx.clone()
                    } else {
                        ctx.within(InvocationContext::Cmdsub)
                    };
                    self.script(&s.body, &c, cwd)
                }
                WordPart::ProcSub(s) => {
                    self.script(&s.body, &ctx.within(InvocationContext::Procsub), cwd)
                }
                WordPart::DQuoted(d) => self.parts_subs(&d.parts, ctx, cwd),
                _ => {}
            }
        }
    }

    fn redirect_subs(&mut self, redirects: &[Redirect], ctx: &Ctx, cwd: &Option<String>) {
        for r in redirects {
            match &r.target {
                RedirectTarget::File(w) => self.word_subs(w, ctx, cwd),
                RedirectTarget::Heredoc(h) => {
                    let c = ctx.within(InvocationContext::Heredoc);
                    self.parts_subs(&h.body.parts, &c, cwd)
                }
                RedirectTarget::ProcSub(p) => {
                    self.script(&p.body, &ctx.within(InvocationContext::Procsub), cwd)
                }
                RedirectTarget::Fd(_) => {}
            }
        }
    }

    fn command(&mut self, c: &Command, ctx: &Ctx, cwd: &mut Option<String>, stage: Stage) -> usize {
        for a in &c.assignments {
            for v in &a.values {
                self.word_subs(v, ctx, cwd);
            }
        }
        for w in &c.words {
            self.word_subs(w, ctx, cwd);
        }
        self.redirect_subs(&c.redirects, ctx, cwd);

        let mut inv = Invocation {
            index: 0,
            path: ctx.path.clone(),
            program: None,
            subcommand: Vec::new(),
            wrappers: Vec::new(),
            assignments: c.assignments.iter().map(|a| a.name.clone()).collect(),
            flags: Vec::new(),
            argv: Vec::new(),
            redirects: c.redirects.clone(),
            stage,
            guard: ctx.guard.clone(),
            context: ctx.context,
            cwd: cwd.clone(),
            remote: ctx.remote,
            depth: ctx.depth,
            category: Category::Unk,
            operation: None,
            effects: Vec::new(),
            rule: None,
            unk: None,
        };

        if c.words.is_empty() {
            if c.redirects.iter().any(Redirect::writes_file) {
                inv.category = Category::Edit;
                inv.rule = Some("builtin.redirect".to_owned());
                inv.effects.push(Effect::FsWrite);
            } else {
                inv.category = Category::Environment;
                inv.rule = Some("builtin.assignment".to_owned());
            }
            return self.push(inv);
        }

        let unwrapped = unwrap(self.rules, &c.words);
        let start = unwrapped.start;
        inv.wrappers = unwrapped.wrappers.into_iter().map(|w| w.0).collect();
        inv.assignments.extend(unwrapped.assignments);
        inv.argv = c.words[start..].to_vec();
        let (name, path_like, args) = match unwrapped.implied {
            Some(p) => (p, false, &c.words[start..]),
            None => {
                let Some(first) = c.words.get(start) else {
                    inv.category = Category::Unk;
                    inv.unk = Some(UnkReason::NoRule);
                    return self.push(inv);
                };
                let Some(value) = first.value() else {
                    inv.unk = Some(UnkReason::Dynamic);
                    return self.push(inv);
                };
                let name = value.rsplit('/').next().unwrap_or(value).to_owned();
                (name, value.contains('/'), &c.words[start + 1..])
            }
        };
        inv.program = Some(name.clone());
        let program = self.rules.program(&name);
        let read = read_args(args, program);
        inv.flags = read.flags.clone();

        let rule = if self.functions.contains(&name) && !path_like {
            None
        } else {
            best_rule(self.rules, &name, &read)
        };
        let nested = match rule {
            None if self.functions.contains(&name) && !path_like => {
                inv.category = Category::Run;
                inv.rule = Some("builtin.function".to_owned());
                inv.effects.push(Effect::Exec);
                None
            }
            Some(r) => {
                classify(r, &read, &c.redirects, stage, &mut inv);
                inv.subcommand = if r.sub.is_empty() {
                    read.positionals
                        .iter()
                        .take(program.and_then(|p| p.subcommands).unwrap_or(0))
                        .map_while(|p| p.clone().filter(|v| is_ident(v)))
                        .collect()
                } else {
                    r.sub.clone()
                };
                r.nested
            }
            None if path_like => {
                inv.category = Category::Run;
                inv.rule = Some("builtin.path".to_owned());
                inv.effects.push(Effect::Exec);
                None
            }
            None => {
                inv.unk = Some(UnkReason::NoRule);
                None
            }
        };
        redirect_effects(&c.redirects, stage, &mut inv);
        inv.effects.sort();
        inv.effects.dedup();

        if matches!(name.as_str(), "cd" | "pushd") {
            *cwd = change_dir(cwd.as_deref(), &read.positionals);
        } else if name == "popd" {
            *cwd = None;
        }

        let index = self.push(inv);
        if let Some(kind) = nested {
            self.nested(kind, &name, args, &read, ctx, cwd);
        }
        index
    }

    /// § 3.2: a script word for another shell, lifted again under the invocation.
    fn nested(
        &mut self,
        kind: NestedKind,
        program: &str,
        args: &[Word],
        read: &Args,
        ctx: &Ctx,
        cwd: &Option<String>,
    ) {
        let parent = self.out.len() - 1;
        let mut c = ctx.within(InvocationContext::Nested);
        c.depth += 1;
        c.path.push(0);
        c.guard = vec![Guard {
            op: AndOrOp::And,
            after: parent,
        }];
        let (dialect, script, own_cwd) = match kind {
            NestedKind::ShellC => {
                let Some(pos) = args.iter().position(|w| {
                    w.value().is_some_and(|v| {
                        v.starts_with('-') && !v.starts_with("--") && v.contains('c')
                    })
                }) else {
                    return;
                };
                let Some(word) = args.get(pos + 1) else {
                    return;
                };
                let dialect = Dialect::parse(program).unwrap_or(ctx.dialect);
                (dialect, word.value().map(str::to_owned), cwd.clone())
            }
            NestedKind::Eval => {
                let values: Option<Vec<&str>> = args.iter().map(Word::value).collect();
                (ctx.dialect, values.map(|v| v.join(" ")), cwd.clone())
            }
            NestedKind::Ssh => {
                // The first positional is the host; the rest is the remote command.
                let rest: Vec<&Option<String>> = read.positionals.iter().skip(1).collect();
                if rest.is_empty() {
                    return;
                }
                c.remote = true;
                let values: Option<Vec<&str>> = rest.iter().map(|p| p.as_deref()).collect();
                (Dialect::Bash, values.map(|v| v.join(" ")), None)
            }
        };
        let stage = self.new_stage();
        if c.depth > MAX_NESTED_DEPTH {
            self.push_unk(&c, &own_cwd, stage, UnkReason::Depth);
            return;
        }
        let Some(text) = script else {
            self.push_unk(&c, &own_cwd, stage, UnkReason::Dynamic);
            return;
        };
        let lifted = crate::lift::lift(&text, dialect);
        c.dialect = dialect;
        let mut own = own_cwd;
        self.list(&lifted.body, &c, &mut own);
    }
}

/// The words of a command after its program, read with the program's declarations.
pub(crate) struct Args {
    /// Static value of each positional; `None` for a dynamic one.
    pub positionals: Vec<Option<String>>,
    /// Flag names, without values.
    pub flags: Vec<String>,
}

pub(crate) fn read_args(args: &[Word], program: Option<&ProgramRule>) -> Args {
    let mut positionals = Vec::new();
    let mut flags = Vec::new();
    let mut i = 0;
    let mut options_end = false;
    while i < args.len() {
        let w = &args[i];
        i += 1;
        let Some(v) = w.value() else {
            positionals.push(None);
            continue;
        };
        if options_end {
            positionals.push(Some(v.to_owned()));
            continue;
        }
        if v == "--" {
            options_end = true;
            continue;
        }
        if let Some(p) = program
            && p.skip_prefix.iter().any(|pre| v.starts_with(pre.as_str()))
            && positionals.is_empty()
        {
            continue;
        }
        if v.len() > 1 && v.starts_with('-') {
            let name = v.split('=').next().unwrap_or(v).to_owned();
            let takes_value = !v.contains('=')
                && program.is_some_and(|p| p.flags_with_value.iter().any(|f| f == v));
            flags.push(name);
            if takes_value {
                i += 1;
            }
            continue;
        }
        positionals.push(Some(v.to_owned()));
    }
    Args { positionals, flags }
}

/// § 6.2 step 4: `-x` matches `-x` and a cluster `-yxz`; a long flag matches by name.
pub(crate) fn flag_matches(cond: &str, flag: &str) -> bool {
    if cond == flag {
        return true;
    }
    let short = cond.len() == 2 && cond.starts_with('-') && !cond.starts_with("--");
    if !short || flag.starts_with("--") || !flag.starts_with('-') {
        return false;
    }
    let letter = cond.as_bytes()[1];
    flag.as_bytes()[1..]
        .iter()
        .take_while(|c| c.is_ascii_alphabetic())
        .any(|&c| c == letter)
}

pub(crate) fn best_rule<'r>(rules: &'r Ruleset, name: &str, args: &Args) -> Option<&'r Rule> {
    let mut best: Option<&Rule> = None;
    for r in rules.rules_for(name) {
        let sub_ok = r.sub.len() <= args.positionals.len()
            && r.sub
                .iter()
                .zip(&args.positionals)
                .all(|(s, p)| p.as_deref() == Some(s.as_str()));
        let any_ok = r.flags_any.as_ref().is_none_or(|conds| {
            conds
                .iter()
                .any(|c| args.flags.iter().any(|f| flag_matches(c, f)))
        });
        let none_ok = r.flags_none.as_ref().is_none_or(|conds| {
            !conds
                .iter()
                .any(|c| args.flags.iter().any(|f| flag_matches(c, f)))
        });
        if sub_ok && any_ok && none_ok && best.is_none_or(|b| r.specificity() > b.specificity()) {
            best = Some(r);
        }
    }
    best
}

/// § 6.2 steps 4-5 for the rule that matched. A stream program with a file operand reads it;
/// without one it is a filter when it is a pipe stage after the head, reads a file when `<`
/// feeds it, and otherwise keeps the rule's own category.
fn classify(r: &Rule, args: &Args, redirects: &[Redirect], stage: Stage, inv: &mut Invocation) {
    inv.rule = Some(r.id.clone());
    inv.operation = r.operation.clone();
    inv.effects = r.effects.clone();
    inv.category = r.category;
    if let Some(Some(script)) = args.positionals.get(r.sub.len())
        && let Some(case) = r.scripts.iter().find(|c| c.matches(script))
    {
        inv.category = case.category;
    }
    if let Some(s) = &r.stream {
        let pattern_given = args
            .flags
            .iter()
            .any(|f| s.pattern_flags.iter().any(|c| flag_matches(c, f)));
        let before = if pattern_given {
            0
        } else {
            s.operands_before_files
        };
        let files = args.positionals.len().saturating_sub(r.sub.len());
        let file_fed = redirects
            .iter()
            .any(|r| r.op == RedirectOp::In && r.fd.is_none_or(|fd| fd == 0));
        if files > before || (stage.index == 0 && file_fed) {
            inv.category = s.file_category;
            inv.effects.push(Effect::FsRead);
        } else if stage.index > 0 {
            inv.category = Category::Filter;
            inv.effects.push(Effect::Stdin);
        } else if redirects.iter().any(Redirect::feeds_stdin) {
            inv.effects.push(Effect::Stdin);
        }
    }
}

fn redirect_effects(redirects: &[Redirect], stage: Stage, inv: &mut Invocation) {
    if redirects.iter().any(Redirect::writes_file) {
        inv.effects.push(Effect::FsWrite);
        let last = matches!(stage.role, StageRole::Alone | StageRole::Tail);
        if last && matches!(inv.category, Category::Output | Category::Filter) {
            inv.category = Category::Edit;
        }
    }
    if redirects
        .iter()
        .any(|r| r.op == RedirectOp::In && r.fd.is_none_or(|fd| fd == 0))
    {
        inv.effects.push(Effect::FsRead);
    }
}

/// The leading wrappers of a command's words.
pub(crate) struct Unwrapped {
    /// Each wrapper: its name and the range of words it took, its own name first.
    pub wrappers: Vec<(String, usize, usize)>,
    /// `X=y` names a wrapper (`env`) took.
    pub assignments: Vec<String>,
    /// Index of the first word after the wrappers.
    pub start: usize,
    /// The program a wrapper's subcommand names; the words from `start` are its arguments.
    pub implied: Option<String>,
}

/// § 6.2 step 1: leading wrappers are removed and recorded.
pub(crate) fn unwrap(rules: &Ruleset, words: &[Word]) -> Unwrapped {
    let mut out = Unwrapped {
        wrappers: Vec::new(),
        assignments: Vec::new(),
        start: 0,
        implied: None,
    };
    let mut i = 0;
    while let Some(v) = words.get(i).and_then(Word::value) {
        let name = v.rsplit('/').next().unwrap_or(v);
        let Some(w) = rules.wrapper(name) else {
            break;
        };
        let Some(next) = words.get(i + 1) else {
            break;
        };
        if next
            .value()
            .is_some_and(|n| w.unless_next.iter().any(|u| u == n))
        {
            break;
        }
        let mut j = i + 1;
        if next
            .value()
            .is_some_and(|n| w.skip_next.iter().any(|u| u == n))
        {
            j += 1;
        }
        if let Some(until) = &w.until {
            match words[j..]
                .iter()
                .position(|x| x.value() == Some(until.as_str()))
            {
                Some(p) => j += p + 1,
                None => break,
            }
        }
        let mut assignments = Vec::new();
        if w.until.is_none() {
            while let Some(x) = words.get(j) {
                let Some(xv) = x.value() else { break };
                if xv == "--" {
                    j += 1;
                    break;
                }
                if xv.len() > 1 && xv.starts_with('-') {
                    let takes = !xv.contains('=') && w.flags_with_value.iter().any(|f| f == xv);
                    j += if takes { 2 } else { 1 };
                    continue;
                }
                if w.assignments == Some(true) && is_assignment(xv) {
                    assignments.push(xv.split('=').next().unwrap_or("").to_owned());
                    j += 1;
                    continue;
                }
                break;
            }
            j += w.positionals.unwrap_or(0);
        }
        if j >= words.len() {
            break;
        }
        out.wrappers.push((name.to_owned(), i, j));
        out.assignments.extend(assignments);
        let implied = words[i + 1..j].iter().find_map(|x| {
            let v = x.value()?;
            w.implies.iter().find(|im| im.after == v)
        });
        i = j;
        if let Some(im) = implied {
            out.implied = Some(im.program.clone());
            break;
        }
    }
    out.start = i;
    out
}

fn is_assignment(v: &str) -> bool {
    match v.split_once('=') {
        Some((name, _)) => {
            !name.is_empty()
                && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
                && !name.starts_with(|c: char| c.is_ascii_digit())
        }
        None => false,
    }
}

pub(crate) fn is_ident(v: &str) -> bool {
    !v.is_empty()
        && v.len() <= 32
        && v.starts_with(|c: char| c.is_ascii_lowercase())
        && v.chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_')
}

/// `cd` as `b10x guard bash` tracks it: a static path moves the directory, anything else makes
/// it unknown.
fn change_dir(cwd: Option<&str>, positionals: &[Option<String>]) -> Option<String> {
    let target = positionals.first()?.as_deref()?;
    if target == "-" || target.starts_with('~') {
        return None;
    }
    if target.starts_with('/') {
        return Some(normalize("", target));
    }
    Some(normalize(cwd?, target))
}

fn normalize(base: &str, rel: &str) -> String {
    let absolute = base.starts_with('/') || (base.is_empty() && rel.starts_with('/'));
    let mut parts: Vec<&str> = Vec::new();
    for p in base.split('/').chain(rel.split('/')) {
        match p {
            "" | "." => {}
            ".." => {
                if parts.last().is_some_and(|l| *l != "..") {
                    parts.pop();
                } else if !absolute {
                    parts.push("..");
                }
            }
            p => parts.push(p),
        }
    }
    let joined = parts.join("/");
    match (absolute, joined.is_empty()) {
        (true, _) => format!("/{joined}"),
        (false, true) => ".".to_owned(),
        (false, false) => joined,
    }
}
