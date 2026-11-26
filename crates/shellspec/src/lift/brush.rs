//! bash, POSIX sh and zsh input through `brush-parser` (design/design.md § 5.2). brush gives char
//! indices; they are converted to bytes here. Words come back as strings and are re-parsed with
//! brush's word parser.

use std::panic::{AssertUnwindSafe, catch_unwind};

use brush_parser::ast::{self, SourceLocation};
use brush_parser::word::{self, ParameterExpr, WordPiece, WordPieceWithSource};
use brush_parser::{ParseError, ParserOptions, SourceSpan};

use super::split::{self, Op};
use super::{cover, has_zsh_token, lift_at, unk_node};
use crate::ir::*;

pub const PARSER: &str = "brush-parser/0.4.0";

fn options(dialect: Dialect) -> ParserOptions {
    let mut o = ParserOptions::default();
    if dialect == Dialect::Sh {
        o.sh_mode = true;
        o.posix_mode = true;
    }
    o
}

fn error_code(e: &ParseError) -> String {
    match e {
        ParseError::ParsingNear(_) => "parse_near".to_owned(),
        ParseError::ParsingAtEndOfInput => "parse_at_end".to_owned(),
        ParseError::Tokenizing { inner, .. } => {
            let debug = format!("{inner:?}");
            let name = debug
                .split(|c: char| !c.is_ascii_alphanumeric())
                .next()
                .unwrap_or("");
            format!("tokenize_{}", snake(name))
        }
    }
}

fn snake(name: &str) -> String {
    let mut out = String::new();
    for (i, c) in name.chars().enumerate() {
        if c.is_ascii_uppercase() {
            if i > 0 {
                out.push('_');
            }
            out.push(c.to_ascii_lowercase());
        } else {
            out.push(c);
        }
    }
    out
}

fn parse(text: &str, opts: &ParserOptions) -> Result<ast::Program, String> {
    let run = catch_unwind(AssertUnwindSafe(|| {
        brush_parser::Parser::new(text.as_bytes(), opts).parse_program()
    }));
    match run {
        Ok(Ok(p)) => Ok(p),
        Ok(Err(e)) => Err(error_code(&e)),
        Err(_) => Err("parser_panic".to_owned()),
    }
}

/// zsh's `&!` and `&|` run a command in the background and disown it; bash reads `&` and then
/// `!` or `|`. Each becomes `& ` of the same length, so spans stay byte-true.
fn zsh_disown(text: &str) -> Option<String> {
    let b = text.as_bytes();
    let mut out: Option<Vec<u8>> = None;
    let (mut single, mut double) = (false, false);
    let mut i = 0;
    while i < b.len() {
        let c = b[i];
        match c {
            b'\\' if !single => i += 1,
            b'\'' if !double => single = !single,
            b'"' if !single => double = !double,
            b'&' if !single && !double => {
                let next = b.get(i + 1).copied();
                let prev = if i > 0 { b[i - 1] } else { b' ' };
                let after = b.get(i + 2).copied();
                if matches!(next, Some(b'!') | Some(b'|'))
                    && prev != b'&'
                    && prev != b'>'
                    && after.is_none_or(|a| a.is_ascii_whitespace() || a == b';' || a == b')')
                {
                    out.get_or_insert_with(|| b.to_vec())[i + 1] = b' ';
                }
            }
            _ => {}
        }
        i += 1;
    }
    out.and_then(|v| String::from_utf8(v).ok())
}

pub(super) fn lift(text: &str, base: usize, dialect: Dialect, depth: usize) -> Script {
    let opts = options(dialect);
    let span = Span::new(base, base + text.len());
    let rewritten = if dialect == Dialect::Zsh {
        zsh_disown(text)
    } else {
        None
    };
    let text = rewritten.as_deref().unwrap_or(text);
    match parse(text, &opts) {
        Ok(program) => {
            let mut l = Lifter::new(text, base, dialect, depth, opts);
            let body = l.program(&program);
            Script {
                dialect,
                span,
                body,
                diagnostics: l.diags,
            }
        }
        Err(code) => fallback(text, base, dialect, depth, opts, &code),
    }
}

/// § 5.4: split at top-level list operators and lift each part alone.
fn fallback(
    text: &str,
    base: usize,
    dialect: Dialect,
    depth: usize,
    opts: ParserOptions,
    whole_code: &str,
) -> Script {
    let parts = split::split(text);
    let mut diags = Vec::new();
    let mut nodes: Vec<(Node, Op)> = Vec::new();
    for part in &parts {
        let piece = &text[part.start..part.end];
        let at = base + part.start;
        let node = if parts.len() == 1 {
            None
        } else {
            match parse(piece, &opts) {
                Ok(program) => {
                    let mut l = Lifter::new(piece, at, dialect, depth, opts.clone());
                    let mut body = l.program(&program);
                    diags.append(&mut l.diags);
                    match body.len() {
                        0 => continue,
                        1 => Some(body.remove(0).body),
                        _ => {
                            let span = cover(body.iter().map(|s| s.span)).unwrap_or_default();
                            Some(Node::Group(Block {
                                span,
                                body,
                                redirects: Vec::new(),
                            }))
                        }
                    }
                }
                Err(_) => None,
            }
        };
        let node = node.unwrap_or_else(|| {
            let trimmed_start = piece.len() - piece.trim_start().len();
            let t = piece.trim();
            let span = Span::new(at + trimmed_start, at + trimmed_start + t.len());
            let reason = if dialect == Dialect::Zsh && has_zsh_token(t) {
                UnkReason::Dialect
            } else {
                UnkReason::Syntax
            };
            let code = if parts.len() == 1 {
                whole_code.to_owned()
            } else {
                parse(piece, &opts).err().unwrap_or_default()
            };
            diags.push(Diagnostic {
                span,
                reason,
                parser: PARSER.to_owned(),
                code,
            });
            Node::Unk(unk_node(t, span, reason, PARSER))
        });
        nodes.push((node, part.op));
    }
    let mut body = Vec::new();
    let mut chain: Option<AndOr> = None;
    let mut pending: Option<AndOrOp> = None;
    for (node, op) in nodes {
        let current = match (chain.take(), pending.take()) {
            (Some(mut c), Some(prev)) => {
                c.span = cover([c.span, node.span()]).unwrap_or(c.span);
                c.rest.push(AndOrItem { op: prev, node });
                c
            }
            _ => AndOr {
                span: node.span(),
                first: Box::new(node),
                rest: Vec::new(),
            },
        };
        match op {
            Op::And | Op::Or => {
                pending = Some(if op == Op::And {
                    AndOrOp::And
                } else {
                    AndOrOp::Or
                });
                chain = Some(current);
            }
            _ => {
                let sep = match op {
                    Op::Semi => Separator::Semi,
                    Op::Newline => Separator::Newline,
                    Op::Amp => Separator::Amp,
                    _ => Separator::End,
                };
                body.push(statement(current, sep));
            }
        }
    }
    if let Some(c) = chain {
        body.push(statement(c, Separator::End));
    }
    Script {
        dialect,
        span: Span::new(base, base + text.len()),
        body,
        diagnostics: diags,
    }
}

fn statement(c: AndOr, sep: Separator) -> Statement {
    let span = c.span;
    let body = if c.rest.is_empty() {
        *c.first
    } else {
        Node::AndOr(c)
    };
    Statement { span, body, sep }
}

struct Lifter<'a> {
    src: &'a str,
    base: usize,
    /// Byte offset of each char index, when the text is not ASCII.
    c2b: Option<Vec<usize>>,
    dialect: Dialect,
    depth: usize,
    opts: ParserOptions,
    diags: Vec<Diagnostic>,
}

impl<'a> Lifter<'a> {
    fn new(
        src: &'a str,
        base: usize,
        dialect: Dialect,
        depth: usize,
        opts: ParserOptions,
    ) -> Lifter<'a> {
        let c2b = if src.is_ascii() {
            None
        } else {
            let mut v: Vec<usize> = src.char_indices().map(|(b, _)| b).collect();
            v.push(src.len());
            Some(v)
        };
        Lifter {
            src,
            base,
            c2b,
            dialect,
            depth,
            opts,
            diags: Vec::new(),
        }
    }

    fn byte(&self, char_index: usize) -> usize {
        let b = match &self.c2b {
            None => char_index.min(self.src.len()),
            Some(v) => v
                .get(char_index)
                .copied()
                .unwrap_or_else(|| *v.last().unwrap_or(&0)),
        };
        self.base + b
    }

    fn span(&self, loc: Option<SourceSpan>) -> Option<Span> {
        loc.map(|l| Span::new(self.byte(l.start.index), self.byte(l.end.index)))
    }

    /// The input text of an absolute span, when it lies in this lifter's text.
    fn text(&self, span: Span) -> &'a str {
        let s = span.start.saturating_sub(self.base);
        let e = span.end.saturating_sub(self.base);
        self.src.get(s..e).unwrap_or("")
    }

    fn program(&mut self, p: &ast::Program) -> Vec<Statement> {
        let mut out = Vec::new();
        for cc in &p.complete_commands {
            out.extend(self.list(cc));
        }
        self.fix_separators(&mut out);
        out
    }

    fn list(&mut self, list: &ast::CompoundList) -> Vec<Statement> {
        let mut out: Vec<Statement> = list
            .0
            .iter()
            .map(|item| {
                let body = self.and_or(&item.0);
                let sep = match item.1 {
                    ast::SeparatorOperator::Async => Separator::Amp,
                    ast::SeparatorOperator::Sequence => Separator::End,
                };
                Statement {
                    span: body.span(),
                    body,
                    sep,
                }
            })
            .collect();
        self.fix_separators(&mut out);
        out
    }

    /// A sequence separator is `;` or a newline, read from the text between two statements.
    fn fix_separators(&self, list: &mut [Statement]) {
        for i in 0..list.len() {
            if list[i].sep == Separator::Amp {
                continue;
            }
            list[i].sep = Separator::End;
            let Some(next) = list.get(i + 1).map(|s| s.span.start) else {
                continue;
            };
            let end = list[i].span.end;
            let gap = self.text(Span::new(end, next.max(end)));
            for c in gap.chars() {
                match c {
                    ';' => {
                        list[i].sep = Separator::Semi;
                        break;
                    }
                    '\n' => {
                        list[i].sep = Separator::Newline;
                        break;
                    }
                    _ => {}
                }
            }
        }
    }

    fn and_or(&mut self, l: &ast::AndOrList) -> Node {
        let first = self.pipeline(&l.first);
        if l.additional.is_empty() {
            return first;
        }
        let mut spans = vec![first.span()];
        let rest: Vec<AndOrItem> = l
            .additional
            .iter()
            .map(|a| {
                let (op, p) = match a {
                    ast::AndOr::And(p) => (AndOrOp::And, p),
                    ast::AndOr::Or(p) => (AndOrOp::Or, p),
                };
                let node = self.pipeline(p);
                spans.push(node.span());
                AndOrItem { op, node }
            })
            .collect();
        Node::AndOr(AndOr {
            span: cover(spans).unwrap_or_default(),
            first: Box::new(first),
            rest,
        })
    }

    fn pipeline(&mut self, p: &ast::Pipeline) -> Node {
        let mut stages: Vec<Node> = Vec::new();
        let mut links = Vec::new();
        for (i, c) in p.seq.iter().enumerate() {
            let piped_both = if i + 1 < p.seq.len() {
                let here = self.command_span(c);
                let next = self.command_span(&p.seq[i + 1]);
                match (here, next) {
                    (Some(a), Some(b)) => {
                        let gap = self.text(Span::new(a.end, b.start.max(a.end)));
                        gap.contains("|&")
                    }
                    _ => false,
                }
            } else {
                false
            };
            let mut node = self.command(c);
            if piped_both {
                drop_pipe_redirect(&mut node);
            }
            stages.push(node);
            if i + 1 < p.seq.len() {
                links.push(if piped_both {
                    PipeLink::StdoutStderr
                } else {
                    PipeLink::Stdout
                });
            }
        }
        let node = if stages.len() == 1 {
            stages.remove(0)
        } else {
            Node::Pipeline(Pipeline {
                span: cover(stages.iter().map(Node::span)).unwrap_or_default(),
                stages,
                links,
            })
        };
        if p.bang {
            let start = self
                .span(p.location())
                .map(|s| s.start)
                .unwrap_or(node.span().start);
            Node::Not(Not {
                span: Span::new(start.min(node.span().start), node.span().end),
                body: Box::new(node),
            })
        } else {
            node
        }
    }

    fn command_span(&self, c: &ast::Command) -> Option<Span> {
        self.span(c.location())
    }

    fn command(&mut self, c: &ast::Command) -> Node {
        match c {
            ast::Command::Simple(sc) => Node::Command(self.simple(sc)),
            ast::Command::Compound(cc, r) => self.compound(cc, r.as_ref()),
            ast::Command::Function(f) => {
                let body = self.compound(&f.body.0, f.body.1.as_ref());
                let span = cover(
                    [self.span(f.location()), Some(body.span())]
                        .into_iter()
                        .flatten(),
                )
                .unwrap_or_default();
                Node::Function(Function {
                    span,
                    name: f.fname.value.clone(),
                    body: Box::new(body),
                })
            }
            ast::Command::ExtendedTest(t, r) => {
                let mut words = Vec::new();
                self.test_words(&t.expr, &mut words);
                let redirects = self.redirect_list(r.as_ref());
                let span = self
                    .span(t.location())
                    .or_else(|| cover(words.iter().map(|w| w.span)))
                    .unwrap_or_default();
                Node::Test(Test {
                    span,
                    words,
                    redirects,
                })
            }
        }
    }

    fn test_words(&mut self, e: &ast::ExtendedTestExpr, out: &mut Vec<Word>) {
        match e {
            ast::ExtendedTestExpr::And(a, b) | ast::ExtendedTestExpr::Or(a, b) => {
                self.test_words(a, out);
                self.test_words(b, out);
            }
            ast::ExtendedTestExpr::Not(a) | ast::ExtendedTestExpr::Parenthesized(a) => {
                self.test_words(a, out)
            }
            ast::ExtendedTestExpr::UnaryTest(_, w) => out.push(self.word(w)),
            ast::ExtendedTestExpr::BinaryTest(_, a, b) => {
                out.push(self.word(a));
                out.push(self.word(b));
            }
        }
    }

    fn simple(&mut self, sc: &ast::SimpleCommand) -> Command {
        let mut assignments = Vec::new();
        let mut words = Vec::new();
        let mut redirects = Vec::new();
        if let Some(prefix) = &sc.prefix {
            for item in &prefix.0 {
                match item {
                    ast::CommandPrefixOrSuffixItem::AssignmentWord(a, w) => {
                        assignments.push(self.assignment(a, w))
                    }
                    other => self.item(other, &mut words, &mut redirects),
                }
            }
        }
        if let Some(w) = &sc.word_or_name {
            words.push(self.word(w));
        }
        if let Some(suffix) = &sc.suffix {
            for item in &suffix.0 {
                self.item(item, &mut words, &mut redirects);
            }
        }
        let span = cover(
            self.span(sc.location())
                .into_iter()
                .chain(assignments.iter().map(|a| a.span))
                .chain(words.iter().map(|w| w.span))
                .chain(redirects.iter().map(|r| r.span)),
        )
        .unwrap_or_default();
        Command {
            span,
            assignments,
            words,
            redirects,
        }
    }

    fn item(
        &mut self,
        item: &ast::CommandPrefixOrSuffixItem,
        words: &mut Vec<Word>,
        redirects: &mut Vec<Redirect>,
    ) {
        match item {
            ast::CommandPrefixOrSuffixItem::IoRedirect(r) => redirects.push(self.redirect(r)),
            ast::CommandPrefixOrSuffixItem::Word(w) => words.push(self.word(w)),
            ast::CommandPrefixOrSuffixItem::AssignmentWord(_, w) => words.push(self.word(w)),
            ast::CommandPrefixOrSuffixItem::ProcessSubstitution(kind, s) => {
                words.push(self.procsub_word(kind, s))
            }
        }
    }

    fn assignment(&mut self, a: &ast::Assignment, w: &ast::Word) -> Assignment {
        let name = match &a.name {
            ast::AssignmentName::VariableName(n) => n.clone(),
            ast::AssignmentName::ArrayElementName(n, _) => n.clone(),
        };
        let values = match &a.value {
            ast::AssignmentValue::Scalar(v) => vec![self.word(v)],
            ast::AssignmentValue::Array(items) => items.iter().map(|(_, v)| self.word(v)).collect(),
        };
        let span = self
            .span(a.location())
            .or_else(|| self.span(w.location()))
            .unwrap_or_default();
        Assignment {
            span,
            name,
            values,
            append: a.append,
        }
    }

    fn procsub_word(
        &mut self,
        kind: &ast::ProcessSubstitutionKind,
        s: &ast::SubshellCommand,
    ) -> Word {
        let body = self.list(&s.list);
        let span = self
            .span(s.location())
            .or_else(|| cover(body.iter().map(|b| b.span)))
            .unwrap_or_default();
        let dir = match kind {
            ast::ProcessSubstitutionKind::Read => ProcDir::In,
            ast::ProcessSubstitutionKind::Write => ProcDir::Out,
        };
        let raw = self.text(Span::new(span.start.saturating_sub(1), span.end));
        Word::new(
            span,
            raw,
            vec![WordPart::ProcSub(ProcSub {
                body: Script {
                    dialect: self.dialect,
                    span,
                    body,
                    diagnostics: Vec::new(),
                },
                dir,
            })],
        )
    }

    fn word(&mut self, w: &ast::Word) -> Word {
        let span = self
            .span(w.location())
            .unwrap_or_else(|| Span::new(self.base, self.base));
        let parts = match catch_unwind(AssertUnwindSafe(|| word::parse(&w.value, &self.opts))) {
            Ok(Ok(pieces)) => self.pieces(&pieces, &w.value, span.start),
            _ if w.value.contains(['$', '`']) => vec![unparsed()],
            _ => vec![WordPart::lit(w.value.clone())],
        };
        Word::new(span, w.value.clone(), parts)
    }

    fn pieces(&mut self, pieces: &[WordPieceWithSource], raw: &str, at: usize) -> Vec<WordPart> {
        let mut out: Vec<WordPart> = Vec::new();
        for p in pieces {
            let here = at + p.start_index;
            let part = match &p.piece {
                WordPiece::Text(t) => text_part(t),
                WordPiece::SingleQuotedText(t) => WordPart::SQuoted(Text { text: t.clone() }),
                WordPiece::AnsiCQuotedText(t) => WordPart::AnsiC(Text { text: t.clone() }),
                WordPiece::DoubleQuotedSequence(inner)
                | WordPiece::GettextDoubleQuotedSequence(inner) => WordPart::DQuoted(DQuoted {
                    parts: self.pieces(inner, raw, at),
                }),
                WordPiece::TildeExpansion(_) => WordPart::Tilde(Text {
                    text: raw
                        .get(p.start_index..p.end_index)
                        .unwrap_or("~")
                        .to_owned(),
                }),
                WordPiece::ParameterExpansion(e) => WordPart::Param(param(e)),
                WordPiece::CommandSubstitution(s) => WordPart::CmdSub(CmdSub {
                    body: self.sub_script(s, here + 2),
                    form: SubForm::Dollar,
                }),
                WordPiece::BackquotedCommandSubstitution(s) => WordPart::CmdSub(CmdSub {
                    body: self.sub_script(s, here + 1),
                    form: SubForm::Backtick,
                }),
                WordPiece::EscapeSequence(s) => {
                    let t = s.strip_prefix('\\').unwrap_or(s);
                    WordPart::lit(if t == "\n" { "" } else { t })
                }
                WordPiece::ArithmeticExpression(e) => WordPart::ArithSub(Text {
                    text: e.value.clone(),
                }),
            };
            match (out.last_mut(), part) {
                (Some(WordPart::Lit(prev)), WordPart::Lit(t)) => prev.text.push_str(&t.text),
                (_, part) => out.push(part),
            }
        }
        for part in &mut out {
            if let WordPart::Lit(t) = part
                && (t.text.contains("${") || t.text.contains("$("))
            {
                *part = unparsed();
            }
        }
        out
    }

    fn sub_script(&mut self, text: &str, at: usize) -> Script {
        let script = lift_at(text, at, self.dialect, self.depth + 1);
        self.diags.extend(script.diagnostics.iter().cloned());
        script
    }

    fn redirect_list(&mut self, r: Option<&ast::RedirectList>) -> Vec<Redirect> {
        r.map(|l| l.0.iter().map(|r| self.redirect(r)).collect())
            .unwrap_or_default()
    }

    fn redirect(&mut self, r: &ast::IoRedirect) -> Redirect {
        let (fd, op, target) = match r {
            ast::IoRedirect::File(fd, kind, target) => {
                let op = match kind {
                    ast::IoFileRedirectKind::Read => RedirectOp::In,
                    ast::IoFileRedirectKind::Write => RedirectOp::Out,
                    ast::IoFileRedirectKind::Append => RedirectOp::Append,
                    ast::IoFileRedirectKind::ReadAndWrite => RedirectOp::Inout,
                    ast::IoFileRedirectKind::Clobber => RedirectOp::Clobber,
                    ast::IoFileRedirectKind::DuplicateInput => RedirectOp::DupIn,
                    ast::IoFileRedirectKind::DuplicateOutput => RedirectOp::DupOut,
                };
                let target = match target {
                    ast::IoFileRedirectTarget::Filename(w) => RedirectTarget::File(self.word(w)),
                    ast::IoFileRedirectTarget::Fd(n) => {
                        RedirectTarget::Fd(FdTarget { fd: *n as i64 })
                    }
                    ast::IoFileRedirectTarget::ProcessSubstitution(kind, s) => {
                        let w = self.procsub_word(kind, s);
                        match w.parts.into_iter().next() {
                            Some(WordPart::ProcSub(p)) => RedirectTarget::ProcSub(p),
                            _ => RedirectTarget::Fd(FdTarget { fd: -1 }),
                        }
                    }
                    ast::IoFileRedirectTarget::Duplicate(w) => {
                        let word = self.word(w);
                        match word.value().and_then(|v| v.parse::<i64>().ok()) {
                            Some(n) => RedirectTarget::Fd(FdTarget { fd: n }),
                            None => RedirectTarget::File(word),
                        }
                    }
                };
                (fd.map(|n| n as i64), op, target)
            }
            ast::IoRedirect::HereDocument(fd, doc) => {
                let body = self.heredoc_body(doc);
                let delimiter: String = doc
                    .here_end
                    .value
                    .chars()
                    .filter(|c| !matches!(c, '\'' | '"' | '\\'))
                    .collect();
                (
                    fd.map(|n| n as i64),
                    RedirectOp::Heredoc,
                    RedirectTarget::Heredoc(Heredoc {
                        delimiter,
                        quoted: !doc.requires_expansion,
                        strip_tabs: doc.remove_tabs,
                        body,
                    }),
                )
            }
            ast::IoRedirect::HereString(fd, w) => (
                fd.map(|n| n as i64),
                RedirectOp::Herestring,
                RedirectTarget::File(self.word(w)),
            ),
            ast::IoRedirect::OutputAndError(w, _append) => {
                (None, RedirectOp::Both, RedirectTarget::File(self.word(w)))
            }
        };
        let span = match &target {
            RedirectTarget::File(w) => w.span,
            RedirectTarget::Heredoc(h) => h.body.span,
            RedirectTarget::ProcSub(p) => p.body.span,
            RedirectTarget::Fd(_) => Span::new(self.base, self.base),
        };
        Redirect {
            span,
            fd,
            op,
            target,
        }
    }

    fn heredoc_body(&mut self, doc: &ast::IoHereDocument) -> Word {
        let text = &doc.doc.value;
        let span = self.span(doc.doc.location()).unwrap_or_else(|| {
            match self.src.find(text.as_str()).filter(|_| !text.is_empty()) {
                Some(i) => Span::new(self.base + i, self.base + i + text.len()),
                None => Span::new(self.base, self.base),
            }
        });
        let parts = if doc.requires_expansion {
            match catch_unwind(AssertUnwindSafe(|| word::parse_heredoc(text, &self.opts))) {
                Ok(Ok(pieces)) => self.pieces(&pieces, text, span.start),
                _ => vec![WordPart::lit(text.clone())],
            }
        } else {
            vec![WordPart::lit(text.clone())]
        };
        Word::new(span, text.clone(), parts)
    }

    fn compound(&mut self, cc: &ast::CompoundCommand, r: Option<&ast::RedirectList>) -> Node {
        let redirects = self.redirect_list(r);
        let loc = self.span(cc.location());
        let with = |body_spans: Vec<Span>| {
            cover(
                loc.into_iter()
                    .chain(body_spans)
                    .chain(redirects.iter().map(|r| r.span)),
            )
            .unwrap_or_default()
        };
        match cc {
            ast::CompoundCommand::Arithmetic(a) => Node::Arith(Arith {
                span: with(Vec::new()),
                text: a.expr.value.clone(),
            }),
            ast::CompoundCommand::ArithmeticForClause(f) => {
                let body = self.list(&f.body.list);
                let header = [&f.initializer, &f.condition, &f.updater]
                    .iter()
                    .map(|e| e.as_ref().map(|e| e.value.clone()).unwrap_or_default())
                    .collect::<Vec<_>>()
                    .join("; ");
                let span = with(body.iter().map(|s| s.span).collect());
                Node::For(For {
                    span,
                    variable: String::new(),
                    items: None,
                    arith: Some(header),
                    body,
                    redirects,
                })
            }
            ast::CompoundCommand::BraceGroup(g) => {
                let body = self.list(&g.list);
                let span = with(body.iter().map(|s| s.span).collect());
                Node::Group(Block {
                    span,
                    body,
                    redirects,
                })
            }
            ast::CompoundCommand::Subshell(s) => {
                let body = self.list(&s.list);
                let span = with(body.iter().map(|s| s.span).collect());
                Node::Subshell(Block {
                    span,
                    body,
                    redirects,
                })
            }
            ast::CompoundCommand::ForClause(f) => {
                let items = f
                    .values
                    .as_ref()
                    .map(|v| v.iter().map(|w| self.word(w)).collect::<Vec<_>>());
                let body = self.list(&f.body.list);
                let span = with(body.iter().map(|s| s.span).collect());
                Node::For(For {
                    span,
                    variable: f.variable_name.clone(),
                    items,
                    arith: None,
                    body,
                    redirects,
                })
            }
            ast::CompoundCommand::CaseClause(c) => {
                let word = self.word(&c.value);
                let arms: Vec<CaseArm> = c
                    .cases
                    .iter()
                    .map(|item| CaseArm {
                        patterns: item.patterns.iter().map(|w| self.word(w)).collect(),
                        body: item.cmd.as_ref().map(|l| self.list(l)).unwrap_or_default(),
                    })
                    .collect();
                let span = with(vec![word.span]);
                Node::Case(Case {
                    span,
                    word,
                    arms,
                    redirects,
                })
            }
            ast::CompoundCommand::IfClause(i) => {
                let mut clauses = vec![CondClause {
                    condition: self.list(&i.condition),
                    body: self.list(&i.then),
                }];
                let mut otherwise = Vec::new();
                for e in i.elses.iter().flatten() {
                    match &e.condition {
                        Some(c) => clauses.push(CondClause {
                            condition: self.list(c),
                            body: self.list(&e.body),
                        }),
                        None => otherwise = self.list(&e.body),
                    }
                }
                let span = with(Vec::new());
                Node::If(If {
                    span,
                    clauses,
                    otherwise,
                    redirects,
                })
            }
            ast::CompoundCommand::WhileClause(w) | ast::CompoundCommand::UntilClause(w) => {
                let condition = self.list(&w.0);
                let body = self.list(&w.1.list);
                let span = with(
                    condition
                        .iter()
                        .chain(body.iter())
                        .map(|s| s.span)
                        .collect(),
                );
                let l = Loop {
                    span,
                    condition,
                    body,
                    redirects,
                };
                if matches!(cc, ast::CompoundCommand::WhileClause(_)) {
                    Node::While(l)
                } else {
                    Node::Until(l)
                }
            }
            ast::CompoundCommand::Coprocess(c) => {
                let inner = self.command(&c.body);
                let span = with(vec![inner.span()]);
                Node::Group(Block {
                    span,
                    body: vec![Statement {
                        span: inner.span(),
                        body: inner,
                        sep: Separator::End,
                    }],
                    redirects,
                })
            }
        }
    }
}

/// brush turns `a |& b` into `a 2>&1 | b`; the link records it, so the added redirect goes.
fn drop_pipe_redirect(node: &mut Node) {
    let redirects = match node {
        Node::Command(c) => &mut c.redirects,
        Node::Subshell(b) | Node::Group(b) => &mut b.redirects,
        Node::If(i) => &mut i.redirects,
        Node::While(l) | Node::Until(l) => &mut l.redirects,
        Node::For(f) => &mut f.redirects,
        Node::Case(c) => &mut c.redirects,
        _ => return,
    };
    if let Some(last) = redirects.last()
        && last.fd == Some(2)
        && last.op == RedirectOp::DupOut
        && matches!(last.target, RedirectTarget::Fd(FdTarget { fd: 1 }))
    {
        redirects.pop();
    }
}

/// An expansion brush left as text (zsh `${=x}`, `${(f)x}`): dynamic, never a value.
fn unparsed() -> WordPart {
    WordPart::Param(Param {
        name: String::new(),
        op: Some("unparsed".to_owned()),
    })
}

fn text_part(t: &str) -> WordPart {
    let text = Text { text: t.to_owned() };
    let bracket = t.find('[').is_some_and(|i| t[i + 1..].contains(']'));
    if t.contains(['*', '?']) || bracket {
        WordPart::Glob(text)
    } else if t.contains('{') && t.contains('}') && (t.contains(',') || t.contains("..")) {
        WordPart::Brace(text)
    } else {
        WordPart::Lit(text)
    }
}

fn param(e: &ParameterExpr) -> Param {
    use ParameterExpr as P;
    let (p, op) = match e {
        P::Parameter { parameter, .. } => (parameter, None),
        P::UseDefaultValues { parameter, .. } => (parameter, Some("use_default_values")),
        P::AssignDefaultValues { parameter, .. } => (parameter, Some("assign_default_values")),
        P::IndicateErrorIfNullOrUnset { parameter, .. } => {
            (parameter, Some("indicate_error_if_null_or_unset"))
        }
        P::UseAlternativeValue { parameter, .. } => (parameter, Some("use_alternative_value")),
        P::ParameterLength { parameter, .. } => (parameter, Some("parameter_length")),
        P::RemoveSmallestSuffixPattern { parameter, .. } => {
            (parameter, Some("remove_smallest_suffix_pattern"))
        }
        P::RemoveLargestSuffixPattern { parameter, .. } => {
            (parameter, Some("remove_largest_suffix_pattern"))
        }
        P::RemoveSmallestPrefixPattern { parameter, .. } => {
            (parameter, Some("remove_smallest_prefix_pattern"))
        }
        P::RemoveLargestPrefixPattern { parameter, .. } => {
            (parameter, Some("remove_largest_prefix_pattern"))
        }
        P::Substring { parameter, .. } => (parameter, Some("substring")),
        P::Transform { parameter, .. } => (parameter, Some("transform")),
        P::UppercaseFirstChar { parameter, .. } => (parameter, Some("uppercase_first_char")),
        P::UppercasePattern { parameter, .. } => (parameter, Some("uppercase_pattern")),
        P::LowercaseFirstChar { parameter, .. } => (parameter, Some("lowercase_first_char")),
        P::LowercasePattern { parameter, .. } => (parameter, Some("lowercase_pattern")),
        P::ReplaceSubstring { parameter, .. } => (parameter, Some("replace_substring")),
        P::VariableNames { prefix, .. } => {
            return Param {
                name: prefix.clone(),
                op: Some("variable_names".to_owned()),
            };
        }
        P::MemberKeys { variable_name, .. } => {
            return Param {
                name: variable_name.clone(),
                op: Some("member_keys".to_owned()),
            };
        }
    };
    let name = match p {
        word::Parameter::Positional(n) => n.to_string(),
        word::Parameter::Special(s) => s.to_string().trim_start_matches('$').to_owned(),
        word::Parameter::Named(n) => n.clone(),
        word::Parameter::NamedWithIndex { name, .. } => name.clone(),
        word::Parameter::NamedWithAllIndices { name, .. } => name.clone(),
    };
    Param {
        name,
        op: op.map(str::to_owned),
    }
}
