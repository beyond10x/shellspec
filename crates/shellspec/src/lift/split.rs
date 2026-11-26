//! The lexical fallback of design/design.md § 5.4: a quote-aware pass that splits a refused script
//! at top-level `;`, newline, `&&`, `||` and `&`, so the parts that parse alone are lifted.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Op {
    Semi,
    Newline,
    Amp,
    And,
    Or,
    End,
}

/// One part: the byte range `[start, end)` of the text and the operator after it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Part {
    pub start: usize,
    pub end: usize,
    pub op: Op,
}

pub(crate) fn split(text: &str) -> Vec<Part> {
    let b = text.as_bytes();
    let mut parts = Vec::new();
    let mut start = 0;
    let mut i = 0;
    let mut depth: i32 = 0;
    let mut single = false;
    let mut double = false;
    let mut backtick = false;
    let mut word_start = true;
    while i < b.len() {
        let c = b[i];
        if single {
            if c == b'\'' {
                single = false;
            }
            i += 1;
            continue;
        }
        if c == b'\\' {
            i += 2;
            word_start = false;
            continue;
        }
        if double {
            match c {
                b'"' => double = false,
                b'$' if b.get(i + 1) == Some(&b'(') => depth += 1,
                b')' if depth > 0 => depth -= 1,
                _ => {}
            }
            i += 1;
            continue;
        }
        if backtick {
            if c == b'`' {
                backtick = false;
            }
            i += 1;
            continue;
        }
        match c {
            b'\'' => single = true,
            b'"' => double = true,
            b'`' => backtick = true,
            b'(' | b'{' => depth += 1,
            b')' | b'}' => depth = (depth - 1).max(0),
            b'#' if word_start => {
                while i < b.len() && b[i] != b'\n' {
                    i += 1;
                }
                continue;
            }
            _ => {}
        }
        if depth == 0 && !single && !double && !backtick {
            let next = b.get(i + 1).copied();
            let prev = if i > 0 { Some(b[i - 1]) } else { None };
            let cut = match c {
                b';' if next == Some(b';') => {
                    i += 2;
                    continue;
                }
                b';' => Some((Op::Semi, 1)),
                b'\n' => Some((Op::Newline, 1)),
                b'&' if next == Some(b'&') => Some((Op::And, 2)),
                b'|' if next == Some(b'|') => Some((Op::Or, 2)),
                b'&' if matches!(prev, Some(b'>') | Some(b'<'))
                    || matches!(next, Some(b'>') | Some(b'|') | Some(b'!')) =>
                {
                    None
                }
                b'&' => Some((Op::Amp, 1)),
                _ => None,
            };
            if let Some((op, len)) = cut {
                parts.push(Part { start, end: i, op });
                i += len;
                start = i;
                word_start = true;
                continue;
            }
        }
        word_start = matches!(c, b' ' | b'\t' | b'(' | b'|' | b';' | b'&');
        i += 1;
    }
    parts.push(Part {
        start,
        end: b.len().max(start),
        op: Op::End,
    });
    parts
        .into_iter()
        .filter(|p| !text[p.start..p.end].trim().is_empty())
        .collect()
}
