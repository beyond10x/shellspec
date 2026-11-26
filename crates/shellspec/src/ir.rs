//! The `shell-ir/1` tree (`spec/domains/syntax.yaml`, `spec/domains/word.yaml`). Field names and
//! wire forms follow the specification: a union is `{"kind": …, "value": …}`, an absent optional
//! field is left out.

use serde::{Deserialize, Serialize};

/// The format name of a lifted script document.
pub const FORMAT: &str = "shell-ir/1";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Dialect {
    Bash,
    Sh,
    Zsh,
    Fish,
}

impl Dialect {
    pub fn name(self) -> &'static str {
        match self {
            Dialect::Bash => "bash",
            Dialect::Sh => "sh",
            Dialect::Zsh => "zsh",
            Dialect::Fish => "fish",
        }
    }

    pub fn parse(name: &str) -> Option<Dialect> {
        match name {
            "bash" => Some(Dialect::Bash),
            "sh" | "dash" | "ash" => Some(Dialect::Sh),
            "zsh" => Some(Dialect::Zsh),
            "fish" => Some(Dialect::Fish),
            _ => None,
        }
    }
}

/// A byte range of the input, start inclusive, end exclusive.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Span {
    pub start: usize,
    pub end: usize,
}

impl Span {
    pub fn new(start: usize, end: usize) -> Span {
        Span { start, end }
    }

    pub fn shift(self, by: usize) -> Span {
        Span::new(self.start + by, self.end + by)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UnkReason {
    Syntax,
    Dialect,
    Dynamic,
    NoRule,
    Depth,
}

impl UnkReason {
    pub fn name(self) -> &'static str {
        match self {
            UnkReason::Syntax => "syntax",
            UnkReason::Dialect => "dialect",
            UnkReason::Dynamic => "dynamic",
            UnkReason::NoRule => "no_rule",
            UnkReason::Depth => "depth",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Diagnostic {
    pub span: Span,
    pub reason: UnkReason,
    pub parser: String,
    pub code: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Script {
    pub dialect: Dialect,
    pub span: Span,
    pub body: Vec<Statement>,
    pub diagnostics: Vec<Diagnostic>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Separator {
    Newline,
    Semi,
    Amp,
    End,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Statement {
    pub span: Span,
    pub body: Node,
    pub sep: Separator,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AndOrOp {
    And,
    Or,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AndOrItem {
    pub op: AndOrOp,
    pub node: Node,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AndOr {
    pub span: Span,
    pub first: Box<Node>,
    pub rest: Vec<AndOrItem>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Not {
    pub span: Span,
    pub body: Box<Node>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PipeLink {
    Stdout,
    StdoutStderr,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Pipeline {
    pub span: Span,
    pub stages: Vec<Node>,
    pub links: Vec<PipeLink>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Assignment {
    pub span: Span,
    pub name: String,
    pub values: Vec<Word>,
    pub append: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Command {
    pub span: Span,
    pub assignments: Vec<Assignment>,
    pub words: Vec<Word>,
    pub redirects: Vec<Redirect>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Block {
    pub span: Span,
    pub body: Vec<Statement>,
    pub redirects: Vec<Redirect>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CondClause {
    pub condition: Vec<Statement>,
    pub body: Vec<Statement>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct If {
    pub span: Span,
    pub clauses: Vec<CondClause>,
    pub otherwise: Vec<Statement>,
    pub redirects: Vec<Redirect>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Loop {
    pub span: Span,
    pub condition: Vec<Statement>,
    pub body: Vec<Statement>,
    pub redirects: Vec<Redirect>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct For {
    pub span: Span,
    pub variable: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub items: Option<Vec<Word>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub arith: Option<String>,
    pub body: Vec<Statement>,
    pub redirects: Vec<Redirect>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CaseArm {
    pub patterns: Vec<Word>,
    pub body: Vec<Statement>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Case {
    pub span: Span,
    pub word: Word,
    pub arms: Vec<CaseArm>,
    pub redirects: Vec<Redirect>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Function {
    pub span: Span,
    pub name: String,
    pub body: Box<Node>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Test {
    pub span: Span,
    pub words: Vec<Word>,
    pub redirects: Vec<Redirect>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Arith {
    pub span: Span,
    pub text: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Unk {
    pub span: Span,
    pub text_digest: String,
    pub len: usize,
    pub reason: UnkReason,
    pub parser: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value")]
pub enum Node {
    AndOr(AndOr),
    Not(Not),
    Pipeline(Pipeline),
    Command(Command),
    Subshell(Block),
    Group(Block),
    If(If),
    While(Loop),
    Until(Loop),
    For(For),
    Case(Case),
    Function(Function),
    Test(Test),
    Arith(Arith),
    Unk(Unk),
}

impl Node {
    pub fn span(&self) -> Span {
        match self {
            Node::AndOr(n) => n.span,
            Node::Not(n) => n.span,
            Node::Pipeline(n) => n.span,
            Node::Command(n) => n.span,
            Node::Subshell(n) | Node::Group(n) => n.span,
            Node::If(n) => n.span,
            Node::While(n) | Node::Until(n) => n.span,
            Node::For(n) => n.span,
            Node::Case(n) => n.span,
            Node::Function(n) => n.span,
            Node::Test(n) => n.span,
            Node::Arith(n) => n.span,
            Node::Unk(n) => n.span,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Text {
    pub text: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DQuoted {
    pub parts: Vec<WordPart>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Param {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub op: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SubForm {
    Dollar,
    Backtick,
    FishParen,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CmdSub {
    pub body: Script,
    pub form: SubForm,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProcDir {
    In,
    Out,
    File,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProcSub {
    pub body: Script,
    pub dir: ProcDir,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value")]
pub enum WordPart {
    Lit(Text),
    SQuoted(Text),
    DQuoted(DQuoted),
    AnsiC(Text),
    Param(Param),
    CmdSub(CmdSub),
    ProcSub(ProcSub),
    ArithSub(Text),
    Glob(Text),
    Brace(Text),
    Tilde(Text),
}

impl WordPart {
    pub fn lit(text: impl Into<String>) -> WordPart {
        WordPart::Lit(Text { text: text.into() })
    }

    /// The value of a part that expands nothing.
    fn static_value(&self, out: &mut String) -> bool {
        match self {
            WordPart::Lit(t) | WordPart::SQuoted(t) | WordPart::AnsiC(t) => {
                out.push_str(&t.text);
                true
            }
            WordPart::DQuoted(d) => d.parts.iter().all(|p| p.static_value(out)),
            _ => false,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Word {
    pub span: Span,
    pub raw: String,
    pub parts: Vec<WordPart>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub static_value: Option<String>,
}

impl Word {
    /// A word from its parts; its static value is derived from them.
    pub fn new(span: Span, raw: impl Into<String>, parts: Vec<WordPart>) -> Word {
        let mut value = String::new();
        let is_static = parts.iter().all(|p| p.static_value(&mut value));
        Word {
            span,
            raw: raw.into(),
            parts,
            static_value: is_static.then_some(value),
        }
    }

    pub fn value(&self) -> Option<&str> {
        self.static_value.as_deref()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RedirectOp {
    In,
    Out,
    Append,
    Clobber,
    OutNoclobber,
    Inout,
    DupOut,
    DupIn,
    Both,
    Herestring,
    Heredoc,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Heredoc {
    pub delimiter: String,
    pub quoted: bool,
    pub strip_tabs: bool,
    pub body: Word,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FdTarget {
    pub fd: i64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value")]
pub enum RedirectTarget {
    File(Word),
    Fd(FdTarget),
    Heredoc(Heredoc),
    ProcSub(ProcSub),
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Redirect {
    pub span: Span,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fd: Option<i64>,
    pub op: RedirectOp,
    pub target: RedirectTarget,
}

impl Redirect {
    /// True when the redirect writes standard output to a file that is no device.
    pub fn writes_file(&self) -> bool {
        let writes = matches!(
            self.op,
            RedirectOp::Out
                | RedirectOp::Append
                | RedirectOp::Clobber
                | RedirectOp::OutNoclobber
                | RedirectOp::Both
                | RedirectOp::Inout
        );
        let device = match &self.target {
            RedirectTarget::File(w) => w.value().is_some_and(|v| v.starts_with("/dev/")),
            _ => true,
        };
        writes && !device
    }

    /// True when the redirect feeds the command's standard input.
    pub fn feeds_stdin(&self) -> bool {
        matches!(
            self.op,
            RedirectOp::In | RedirectOp::Herestring | RedirectOp::Heredoc
        ) && self.fd.is_none_or(|fd| fd == 0)
    }
}
