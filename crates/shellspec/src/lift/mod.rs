//! Lifting: one command string and its dialect into one [`Script`]. Lifting never fails: a region
//! no parser accepts becomes an [`Unk`] node and a [`Diagnostic`] (design/design.md § 3.3, § 5.4).

mod brush;
mod fish;
mod split;

use sha2::{Digest, Sha256};

use crate::ir::{Diagnostic, Dialect, Node, Script, Span, Statement, Unk, UnkReason};

pub use brush::PARSER as BRUSH_PARSER;
pub use fish::PARSER as FISH_PARSER;
pub use fish::sexp as fish_sexp;

/// Substitutions nested deeper than this inside one command string are not lifted.
const MAX_SUB_DEPTH: usize = 32;

/// Lift one command string in one dialect. Pure: same input, same tree.
pub fn lift(text: &str, dialect: Dialect) -> Script {
    lift_at(text, 0, dialect, 0)
}

/// The parser that reads a dialect.
pub fn parser_for(dialect: Dialect) -> &'static str {
    match dialect {
        Dialect::Fish => fish::PARSER,
        _ => brush::PARSER,
    }
}

/// Lift `text`, which starts at byte `base` of the outermost input.
pub(crate) fn lift_at(text: &str, base: usize, dialect: Dialect, depth: usize) -> Script {
    let span = Span::new(base, base + text.len());
    if depth > MAX_SUB_DEPTH {
        return unk_script(
            text,
            span,
            dialect,
            UnkReason::Depth,
            parser_for(dialect),
            "depth",
        );
    }
    match dialect {
        Dialect::Fish => fish::lift(text, base, depth),
        _ => brush::lift(text, base, dialect, depth),
    }
}

pub(crate) fn digest_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut out = String::with_capacity(64);
    for b in digest {
        out.push_str(&format!("{b:02x}"));
    }
    out
}

pub(crate) fn unk_node(text: &str, span: Span, reason: UnkReason, parser: &str) -> Unk {
    Unk {
        span,
        text_digest: digest_hex(text.as_bytes()),
        len: text.len(),
        reason,
        parser: parser.to_owned(),
    }
}

/// A script that is one `Unk` statement.
pub(crate) fn unk_script(
    text: &str,
    span: Span,
    dialect: Dialect,
    reason: UnkReason,
    parser: &str,
    code: &str,
) -> Script {
    let body = if text.trim().is_empty() {
        Vec::new()
    } else {
        vec![Statement {
            span,
            body: Node::Unk(unk_node(text, span, reason, parser)),
            sep: crate::ir::Separator::End,
        }]
    };
    Script {
        dialect,
        span,
        diagnostics: if body.is_empty() {
            Vec::new()
        } else {
            vec![Diagnostic {
                span,
                reason,
                parser: parser.to_owned(),
                code: code.to_owned(),
            }]
        },
        body,
    }
}

/// True when the text holds a token only zsh reads (§ 5.5): `=(…)`, `&!`, a glob qualifier.
pub(crate) fn has_zsh_token(text: &str) -> bool {
    if text.contains("=(") || text.contains("&!") {
        return true;
    }
    let bytes = text.as_bytes();
    for (i, w) in bytes.windows(2).enumerate() {
        if w == b"*(" {
            let rest = &bytes[i + 2..];
            if let Some(close) = rest.iter().position(|&c| c == b')') {
                let inner = &rest[..close];
                if !inner.is_empty()
                    && inner.len() <= 8
                    && inner
                        .iter()
                        .all(|c| b"./@=pN-^omnLDrwxRWX0123456789[]:".contains(c))
                {
                    return true;
                }
            }
        }
    }
    false
}

/// The smallest span that covers all given spans.
pub(crate) fn cover(spans: impl IntoIterator<Item = Span>) -> Option<Span> {
    spans
        .into_iter()
        .filter(|s| s.end >= s.start)
        .fold(None, |acc: Option<Span>, s| match acc {
            None => Some(s),
            Some(a) => Some(Span::new(a.start.min(s.start), a.end.max(s.end))),
        })
}
