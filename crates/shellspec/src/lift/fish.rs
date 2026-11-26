//! fish input through `tree-sitter-fish` (design/design.md § 5.5). The grammar recovers from
//! errors: a statement that holds an error node becomes `Unk`, its siblings stay lifted.

use tree_sitter::Node as TsNode;

use super::{cover, unk_node};
use crate::ir::*;

pub const PARSER: &str = "tree-sitter-fish/3.6.0";

fn parse(text: &str) -> Option<tree_sitter::Tree> {
    let mut parser = tree_sitter::Parser::new();
    parser.set_language(&tree_sitter_fish::language()).ok()?;
    parser.parse(text, None)
}

/// The S-expression of a fish parse tree, for inspection.
pub fn sexp(text: &str) -> String {
    let mut src = text.to_owned();
    src.push('\n');
    parse(&src)
        .map(|t| t.root_node().to_sexp())
        .unwrap_or_default()
}

pub(super) fn lift(text: &str, base: usize, _depth: usize) -> Script {
    let span = Span::new(base, base + text.len());
    // The grammar wants a terminated last statement.
    let mut src = text.to_owned();
    src.push('\n');
    let Some(tree) = parse(&src) else {
        return super::unk_script(
            text,
            span,
            Dialect::Fish,
            UnkReason::Syntax,
            PARSER,
            "parser_unavailable",
        );
    };
    let mut l = Lifter {
        src: &src,
        len: text.len(),
        base,
        diags: Vec::new(),
    };
    let body = l.block(tree.root_node(), &[]);
    Script {
        dialect: Dialect::Fish,
        span,
        body,
        diagnostics: l.diags,
    }
}

const STATEMENTS: &[&str] = &[
    "command",
    "pipe",
    "conditional_execution",
    "negated_statement",
    "redirect_statement",
    "begin_statement",
    "if_statement",
    "while_statement",
    "for_statement",
    "switch_statement",
    "function_definition",
    "break",
    "continue",
    "return",
    "ERROR",
];

struct Lifter<'a> {
    src: &'a str,
    /// Length of the input without the added newline.
    len: usize,
    base: usize,
    diags: Vec<Diagnostic>,
}

impl<'a> Lifter<'a> {
    fn span(&self, n: TsNode) -> Span {
        let s = n.start_byte().min(self.len);
        let e = n.end_byte().min(self.len);
        Span::new(self.base + s, self.base + e)
    }

    fn text(&self, n: TsNode) -> &'a str {
        self.src.get(n.start_byte()..n.end_byte()).unwrap_or("")
    }

    /// The statements among the children of `n`, skipping the children named in `skip` (a
    /// condition, a variable).
    fn block(&mut self, n: TsNode, skip: &[usize]) -> Vec<Statement> {
        let mut out: Vec<Statement> = Vec::new();
        let mut cursor = n.walk();
        let children: Vec<TsNode> = n.children(&mut cursor).collect();
        for child in children {
            if skip.contains(&child.id()) {
                continue;
            }
            let kind = child.kind();
            if !child.is_named() {
                if let Some(last) = out.last_mut().filter(|l| l.sep != Separator::Amp) {
                    match kind {
                        ";" => last.sep = Separator::Semi,
                        "\n" => last.sep = Separator::Newline,
                        "&" => last.sep = Separator::Amp,
                        _ => {}
                    }
                }
                continue;
            }
            if !STATEMENTS.contains(&kind) {
                continue;
            }
            // `a; and b`: a conditional with one statement continues the one before it.
            if kind == "conditional_execution"
                && named(child).len() == 1
                && !child.has_error()
                && let Some(prev) = out.last_mut()
            {
                let op = self.cond_op(child);
                let node = self.node(named(child)[0]);
                let prev_body = std::mem::replace(
                    &mut prev.body,
                    Node::Arith(Arith {
                        span: Span::default(),
                        text: String::new(),
                    }),
                );
                let mut chain = match prev_body {
                    Node::AndOr(a) => a,
                    other => AndOr {
                        span: other.span(),
                        first: Box::new(other),
                        rest: Vec::new(),
                    },
                };
                chain.span = cover([chain.span, node.span()]).unwrap_or(chain.span);
                chain.rest.push(AndOrItem { op, node });
                prev.span = chain.span;
                prev.body = Node::AndOr(chain);
                prev.sep = Separator::End;
                continue;
            }
            let node = self.statement(child);
            // The grammar keeps a trailing `&` inside the statement.
            let mut cursor = child.walk();
            let background = child
                .children(&mut cursor)
                .last()
                .is_some_and(|c| !c.is_named() && c.kind() == "&");
            out.push(Statement {
                span: node.span(),
                body: node,
                sep: if background {
                    Separator::Amp
                } else {
                    Separator::End
                },
            });
        }
        if let Some(last) = out.last_mut()
            && last.sep == Separator::Newline
        {
            last.sep = Separator::End;
        }
        out
    }

    fn statement(&mut self, n: TsNode) -> Node {
        if n.is_error() || n.has_error() || n.is_missing() {
            let span = self.span(n);
            let text = self.text(n);
            self.diags.push(Diagnostic {
                span,
                reason: UnkReason::Syntax,
                parser: PARSER.to_owned(),
                code: "error_node".to_owned(),
            });
            return Node::Unk(unk_node(text, span, UnkReason::Syntax, PARSER));
        }
        self.node(n)
    }

    fn cond_op(&self, n: TsNode) -> AndOrOp {
        let mut cursor = n.walk();
        for c in n.children(&mut cursor) {
            if !c.is_named() {
                match c.kind() {
                    "||" | "or" => return AndOrOp::Or,
                    "&&" | "and" => return AndOrOp::And,
                    _ => {}
                }
            }
        }
        AndOrOp::And
    }

    fn node(&mut self, n: TsNode) -> Node {
        let span = self.span(n);
        match n.kind() {
            "command" => Node::Command(self.command(n)),
            "pipe" => {
                let mut stages = Vec::new();
                let mut links = Vec::new();
                self.pipe(n, &mut stages, &mut links);
                Node::Pipeline(Pipeline {
                    span,
                    stages,
                    links,
                })
            }
            "conditional_execution" => {
                let kids = named(n);
                let op = self.cond_op(n);
                let first = kids.first().map(|k| self.node(*k));
                let second = kids.get(1).map(|k| self.node(*k));
                match (first, second) {
                    (Some(first), Some(second)) => {
                        let mut rest = vec![];
                        match second {
                            Node::AndOr(inner) => {
                                rest.push(AndOrItem {
                                    op,
                                    node: *inner.first,
                                });
                                rest.extend(inner.rest);
                            }
                            other => rest.push(AndOrItem { op, node: other }),
                        }
                        Node::AndOr(AndOr {
                            span,
                            first: Box::new(first),
                            rest,
                        })
                    }
                    (Some(only), None) => only,
                    _ => self.statement_unk(n),
                }
            }
            "negated_statement" => match named(n).first() {
                Some(k) => Node::Not(Not {
                    span,
                    body: Box::new(self.node(*k)),
                }),
                None => self.statement_unk(n),
            },
            "redirect_statement" => {
                let kids = named(n);
                let Some(first) = kids.first() else {
                    return self.statement_unk(n);
                };
                let mut inner = self.node(*first);
                let redirects: Vec<Redirect> =
                    kids[1..].iter().filter_map(|k| self.redirect(*k)).collect();
                match &mut inner {
                    Node::Command(c) => c.redirects.extend(redirects),
                    Node::Group(b) | Node::Subshell(b) => b.redirects.extend(redirects),
                    Node::If(i) => i.redirects.extend(redirects),
                    Node::While(l) | Node::Until(l) => l.redirects.extend(redirects),
                    Node::For(f) => f.redirects.extend(redirects),
                    Node::Case(c) => c.redirects.extend(redirects),
                    _ => {}
                }
                inner
            }
            "begin_statement" => Node::Group(Block {
                span,
                body: self.block(n, &[]),
                redirects: Vec::new(),
            }),
            "if_statement" => {
                let condition = n.child_by_field_name("condition");
                let mut clauses = vec![CondClause {
                    condition: condition.map(|c| vec![self.single(c)]).unwrap_or_default(),
                    body: Vec::new(),
                }];
                let mut otherwise = Vec::new();
                let skip: Vec<usize> = condition.iter().map(|c| c.id()).collect();
                let mut cursor = n.walk();
                for c in n.named_children(&mut cursor) {
                    match c.kind() {
                        "else_if_clause" => {
                            let cond = c.child_by_field_name("condition");
                            let skip: Vec<usize> = cond.iter().map(|x| x.id()).collect();
                            clauses.push(CondClause {
                                condition: cond.map(|x| vec![self.single(x)]).unwrap_or_default(),
                                body: self.block(c, &skip),
                            });
                        }
                        "else_clause" => otherwise = self.block(c, &[]),
                        _ => {}
                    }
                }
                clauses[0].body = self.block_without(n, &skip, &["else_if_clause", "else_clause"]);
                Node::If(If {
                    span,
                    clauses,
                    otherwise,
                    redirects: Vec::new(),
                })
            }
            "while_statement" => {
                let condition = n.child_by_field_name("condition");
                let skip: Vec<usize> = condition.iter().map(|c| c.id()).collect();
                Node::While(Loop {
                    span,
                    condition: condition.map(|c| vec![self.single(c)]).unwrap_or_default(),
                    body: self.block(n, &skip),
                    redirects: Vec::new(),
                })
            }
            "for_statement" => {
                let var = n.child_by_field_name("variable");
                let mut skip: Vec<usize> = var.iter().map(|c| c.id()).collect();
                let mut items = Vec::new();
                let mut cursor = n.walk();
                for v in n.children_by_field_name("value", &mut cursor) {
                    skip.push(v.id());
                    items.push(self.word(v));
                }
                Node::For(For {
                    span,
                    variable: var.map(|v| self.text(v).to_owned()).unwrap_or_default(),
                    items: Some(items),
                    arith: None,
                    body: self.block(n, &skip),
                    redirects: Vec::new(),
                })
            }
            "switch_statement" => {
                let value = n.child_by_field_name("value");
                let word = match value {
                    Some(v) => self.word(v),
                    None => Word::new(span, "", Vec::new()),
                };
                let mut arms = Vec::new();
                let mut cursor = n.walk();
                for c in n.named_children(&mut cursor) {
                    if c.kind() != "case_clause" {
                        continue;
                    }
                    let mut patterns = Vec::new();
                    let mut skip = Vec::new();
                    let mut cc = c.walk();
                    for k in c.named_children(&mut cc) {
                        if STATEMENTS.contains(&k.kind()) {
                            break;
                        }
                        skip.push(k.id());
                        patterns.push(self.word(k));
                    }
                    arms.push(CaseArm {
                        patterns,
                        body: self.block(c, &skip),
                    });
                }
                Node::Case(Case {
                    span,
                    word,
                    arms,
                    redirects: Vec::new(),
                })
            }
            "function_definition" => {
                let name = n.child_by_field_name("name");
                let skip: Vec<usize> = name.iter().map(|c| c.id()).collect();
                let body = self.block(n, &skip);
                Node::Function(Function {
                    span,
                    name: name.map(|v| self.text(v).to_owned()).unwrap_or_default(),
                    body: Box::new(Node::Group(Block {
                        span,
                        body,
                        redirects: Vec::new(),
                    })),
                })
            }
            "break" | "continue" | "return" => {
                let w = Word::new(span, self.text(n), vec![WordPart::lit(n.kind())]);
                Node::Command(Command {
                    span,
                    assignments: Vec::new(),
                    words: vec![w],
                    redirects: Vec::new(),
                })
            }
            _ => self.statement_unk(n),
        }
    }

    fn block_without(&mut self, n: TsNode, skip: &[usize], kinds: &[&str]) -> Vec<Statement> {
        let mut skip = skip.to_vec();
        let mut cursor = n.walk();
        for c in n.named_children(&mut cursor) {
            if kinds.contains(&c.kind()) {
                skip.push(c.id());
            }
        }
        self.block(n, &skip)
    }

    fn single(&mut self, n: TsNode) -> Statement {
        let node = self.statement(n);
        Statement {
            span: node.span(),
            body: node,
            sep: Separator::End,
        }
    }

    fn statement_unk(&mut self, n: TsNode) -> Node {
        let span = self.span(n);
        self.diags.push(Diagnostic {
            span,
            reason: UnkReason::Syntax,
            parser: PARSER.to_owned(),
            code: format!("node_{}", n.kind()),
        });
        Node::Unk(unk_node(self.text(n), span, UnkReason::Syntax, PARSER))
    }

    fn pipe(&mut self, n: TsNode, stages: &mut Vec<Node>, links: &mut Vec<PipeLink>) {
        let mut cursor = n.walk();
        for c in n.children(&mut cursor) {
            if c.is_named() {
                if c.kind() == "pipe" {
                    self.pipe(c, stages, links);
                } else {
                    stages.push(self.node(c));
                }
            } else {
                match c.kind() {
                    "&|" | "2>|" => links.push(PipeLink::StdoutStderr),
                    "|" => links.push(PipeLink::Stdout),
                    _ => {}
                }
            }
        }
        links.truncate(stages.len().saturating_sub(1));
        while links.len() + 1 < stages.len() {
            links.push(PipeLink::Stdout);
        }
    }

    fn command(&mut self, n: TsNode) -> Command {
        let span = self.span(n);
        let mut words = Vec::new();
        let mut redirects = Vec::new();
        let mut cursor = n.walk();
        for c in n.named_children(&mut cursor) {
            match c.kind() {
                "file_redirect" | "stream_redirect" => {
                    if let Some(r) = self.redirect(c) {
                        redirects.push(r);
                    }
                }
                "comment" => {}
                _ => words.push(self.word(c)),
            }
        }
        // fish 3.1+: `X=1 cmd`.
        let mut assignments = Vec::new();
        while words.len() > 1 {
            let Some((name, value)) = words[0]
                .value()
                .and_then(|v| v.split_once('='))
                .map(|(n, v)| (n.to_owned(), v.to_owned()))
            else {
                break;
            };
            if name.is_empty()
                || !name
                    .chars()
                    .all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
            {
                break;
            }
            let w = words.remove(0);
            assignments.push(Assignment {
                span: w.span,
                values: vec![Word::new(w.span, value.clone(), vec![WordPart::lit(value)])],
                name,
                append: false,
            });
        }
        Command {
            span,
            assignments,
            words,
            redirects,
        }
    }

    fn redirect(&mut self, n: TsNode) -> Option<Redirect> {
        let span = self.span(n);
        match n.kind() {
            "file_redirect" => {
                let op_text = n
                    .child_by_field_name("operator")
                    .map(|o| self.text(o))
                    .unwrap_or(">");
                let dest = n.child_by_field_name("destination")?;
                let digits: String = op_text.chars().take_while(char::is_ascii_digit).collect();
                let rest = &op_text[digits.len()..];
                let op = match rest {
                    "<" => RedirectOp::In,
                    ">>" | "^^" => RedirectOp::Append,
                    ">?" => RedirectOp::OutNoclobber,
                    ">|" => RedirectOp::Clobber,
                    "&>" | "&>>" => RedirectOp::Both,
                    "<>" => RedirectOp::Inout,
                    _ => RedirectOp::Out,
                };
                let fd = if rest == "^" || rest == "^^" {
                    Some(2)
                } else {
                    digits.parse().ok()
                };
                Some(Redirect {
                    span,
                    fd,
                    op,
                    target: RedirectTarget::File(self.word(dest)),
                })
            }
            "stream_redirect" => {
                let t = self.text(n);
                let digits: String = t.chars().take_while(char::is_ascii_digit).collect();
                let rest = &t[digits.len()..];
                let (op, target) = match rest.split_once('&') {
                    Some((dir, target)) => (
                        if dir == "<" {
                            RedirectOp::DupIn
                        } else {
                            RedirectOp::DupOut
                        },
                        target,
                    ),
                    None => (RedirectOp::DupOut, rest),
                };
                let target = match target.parse::<i64>() {
                    Ok(fd) => RedirectTarget::Fd(FdTarget { fd }),
                    Err(_) => {
                        RedirectTarget::File(Word::new(span, target, vec![WordPart::lit(target)]))
                    }
                };
                Some(Redirect {
                    span,
                    fd: digits.parse().ok(),
                    op,
                    target,
                })
            }
            _ => None,
        }
    }

    fn word(&mut self, n: TsNode) -> Word {
        let mut parts = Vec::new();
        self.parts(n, &mut parts);
        let mut merged: Vec<WordPart> = Vec::new();
        for p in parts {
            match (merged.last_mut(), p) {
                (Some(WordPart::Lit(prev)), WordPart::Lit(t)) => prev.text.push_str(&t.text),
                (_, p) => merged.push(p),
            }
        }
        Word::new(self.span(n), self.text(n), merged)
    }

    fn parts(&mut self, n: TsNode, out: &mut Vec<WordPart>) {
        let text = self.text(n);
        match n.kind() {
            "word" | "integer" | "float" | "variable_name" => out.push(WordPart::lit(text)),
            "single_quote_string" => {
                let inner = text
                    .strip_prefix('\'')
                    .and_then(|t| t.strip_suffix('\''))
                    .unwrap_or(text);
                out.push(WordPart::SQuoted(Text {
                    text: inner.replace("\\'", "'").replace("\\\\", "\\"),
                }))
            }
            "double_quote_string" => {
                let mut parts = Vec::new();
                let inner_start = n.start_byte() + 1;
                let inner_end = n.end_byte().saturating_sub(1).max(inner_start);
                let mut at = inner_start;
                let mut cursor = n.walk();
                for c in n.named_children(&mut cursor) {
                    if c.start_byte() > at {
                        let gap = self.src.get(at..c.start_byte()).unwrap_or("");
                        parts.push(WordPart::lit(gap));
                    }
                    self.parts(c, &mut parts);
                    at = c.end_byte();
                }
                if inner_end > at {
                    parts.push(WordPart::lit(self.src.get(at..inner_end).unwrap_or("")));
                }
                out.push(WordPart::DQuoted(DQuoted { parts }))
            }
            "escape_sequence" => {
                let t = text.strip_prefix('\\').unwrap_or(text);
                let v = match t {
                    "n" => "\n",
                    "t" => "\t",
                    other => other,
                };
                out.push(WordPart::lit(v))
            }
            "variable_expansion" => {
                let name = named(n)
                    .into_iter()
                    .find(|c| c.kind() == "variable_name")
                    .map(|c| self.text(c).to_owned())
                    .unwrap_or_default();
                let indexed = named(n).iter().any(|c| c.kind() == "list_element_access");
                out.push(WordPart::Param(Param {
                    name,
                    op: indexed.then(|| "index".to_owned()),
                }))
            }
            "command_substitution" => {
                let body = self.block(n, &[]);
                let span = self.span(n);
                let sub = Script {
                    dialect: Dialect::Fish,
                    span,
                    body,
                    diagnostics: Vec::new(),
                };
                out.push(WordPart::CmdSub(CmdSub {
                    body: sub,
                    form: if text.starts_with('$') {
                        SubForm::Dollar
                    } else {
                        SubForm::FishParen
                    },
                }))
            }
            "home_dir_expansion" => out.push(WordPart::Tilde(Text {
                text: text.to_owned(),
            })),
            "glob" => out.push(WordPart::Glob(Text {
                text: text.to_owned(),
            })),
            "brace_expansion" => out.push(WordPart::Brace(Text {
                text: text.to_owned(),
            })),
            "concatenation" => {
                let mut cursor = n.walk();
                for c in n.named_children(&mut cursor) {
                    self.parts(c, out);
                }
            }
            _ => out.push(WordPart::Glob(Text {
                text: text.to_owned(),
            })),
        }
    }
}

fn named(n: TsNode) -> Vec<TsNode> {
    let mut cursor = n.walk();
    n.named_children(&mut cursor).collect()
}
