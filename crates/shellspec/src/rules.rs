//! The ruleset (`spec/domains/classification.yaml`, docs/design.md § 6): data in `rules/*.yaml`,
//! with a digest every classification reports.

use std::collections::{BTreeMap, HashMap};
use std::path::Path;
use std::sync::LazyLock;

use serde::{Deserialize, Serialize};

/// The format of one rules file.
pub const RULES_FORMAT: &str = "shellspec-rules/1";

/// The rules files built into the crate, by name.
const BUILTIN: &[(&str, &str)] = &[
    ("core.yaml", include_str!("../rules/core.yaml")),
    ("dev.yaml", include_str!("../rules/dev.yaml")),
    ("workspace.yaml", include_str!("../rules/workspace.yaml")),
    ("wrappers.yaml", include_str!("../rules/wrappers.yaml")),
];

/// The names of the built-in rules files, so a test can compare them with `rules/`.
pub fn builtin_files() -> Vec<&'static str> {
    BUILTIN.iter().map(|(n, _)| *n).collect()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Category {
    Navigate,
    Read,
    List,
    Search,
    Filter,
    Output,
    Edit,
    Build,
    Test,
    Lint,
    Run,
    Vcs,
    Forge,
    Network,
    Package,
    Process,
    Environment,
    System,
    Container,
    Tool,
    Unk,
}

impl Category {
    pub const ALL: [Category; 21] = [
        Category::Navigate,
        Category::Read,
        Category::List,
        Category::Search,
        Category::Filter,
        Category::Output,
        Category::Edit,
        Category::Build,
        Category::Test,
        Category::Lint,
        Category::Run,
        Category::Vcs,
        Category::Forge,
        Category::Network,
        Category::Package,
        Category::Process,
        Category::Environment,
        Category::System,
        Category::Container,
        Category::Tool,
        Category::Unk,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Category::Navigate => "navigate",
            Category::Read => "read",
            Category::List => "list",
            Category::Search => "search",
            Category::Filter => "filter",
            Category::Output => "output",
            Category::Edit => "edit",
            Category::Build => "build",
            Category::Test => "test",
            Category::Lint => "lint",
            Category::Run => "run",
            Category::Vcs => "vcs",
            Category::Forge => "forge",
            Category::Network => "network",
            Category::Package => "package",
            Category::Process => "process",
            Category::Environment => "environment",
            Category::System => "system",
            Category::Container => "container",
            Category::Tool => "tool",
            Category::Unk => "unk",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Effect {
    FsRead,
    FsWrite,
    FsDelete,
    NetOut,
    RemoteWrite,
    ProcessSignal,
    Exec,
    Stdin,
    Wait,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NestedKind {
    ShellC,
    Eval,
    Ssh,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WrapperRule {
    pub id: String,
    pub names: Vec<String>,
    #[serde(default)]
    pub flags_with_value: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub positionals: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub until: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub assignments: Option<bool>,
    #[serde(default)]
    pub unless_next: Vec<String>,
    #[serde(default)]
    pub skip_next: Vec<String>,
    #[serde(default)]
    pub implies: Vec<Implied>,
}

/// A wrapper subcommand that names the program its words are for: `b10x-gates bot -- commit` runs
/// `git commit`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Implied {
    pub after: String,
    pub program: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProgramRule {
    pub names: Vec<String>,
    #[serde(default)]
    pub flags_with_value: Vec<String>,
    #[serde(default)]
    pub skip_prefix: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subcommands: Option<usize>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StreamRule {
    pub file_category: Category,
    pub operands_before_files: usize,
    #[serde(default)]
    pub pattern_flags: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScriptCase {
    pub pattern: String,
    pub category: Category,
}

impl ScriptCase {
    pub fn matches(&self, name: &str) -> bool {
        match self.pattern.strip_suffix('*') {
            Some(prefix) => name.starts_with(prefix),
            None => name == self.pattern,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Rule {
    pub id: String,
    pub program: Vec<String>,
    #[serde(default)]
    pub sub: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub flags_any: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub flags_none: Option<Vec<String>>,
    pub category: Category,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub operation: Option<String>,
    #[serde(default)]
    pub effects: Vec<Effect>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stream: Option<StreamRule>,
    #[serde(default)]
    pub scripts: Vec<ScriptCase>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub nested: Option<NestedKind>,
}

impl Rule {
    /// § 6.2 step 6: a subcommand beats a flag condition, a flag condition beats the program.
    pub fn specificity(&self) -> usize {
        self.sub.len() * 4
            + usize::from(self.flags_any.is_some()) * 2
            + usize::from(self.flags_none.is_some())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RulesFile {
    pub format: String,
    #[serde(default)]
    pub wrappers: Vec<WrapperRule>,
    #[serde(default)]
    pub programs: Vec<ProgramRule>,
    #[serde(default)]
    pub rules: Vec<Rule>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RulesetError {
    pub problems: Vec<String>,
}

impl std::fmt::Display for RulesetError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.problems.join("; "))
    }
}

impl std::error::Error for RulesetError {}

/// All rules files of one ruleset, merged, with their digest.
#[derive(Clone, Debug)]
pub struct Ruleset {
    /// `sha256:` and the hex digest over every file name and its bytes, in name order.
    pub digest: String,
    pub files: Vec<String>,
    pub wrappers: Vec<WrapperRule>,
    pub programs: Vec<ProgramRule>,
    pub rules: Vec<Rule>,
    by_wrapper: HashMap<String, usize>,
    by_program: HashMap<String, usize>,
    by_rule_program: HashMap<String, Vec<usize>>,
}

static BUILTIN_RULESET: LazyLock<Ruleset> = LazyLock::new(|| {
    let sources: Vec<(String, String)> = BUILTIN
        .iter()
        .map(|(n, t)| ((*n).to_owned(), (*t).to_owned()))
        .collect();
    match Ruleset::from_sources(&sources) {
        Ok(r) => r,
        Err(e) => panic!("the built-in ruleset is invalid: {e}"),
    }
});

impl Ruleset {
    /// The ruleset built from the crate's `rules/*.yaml`.
    pub fn builtin() -> Ruleset {
        BUILTIN_RULESET.clone()
    }

    /// The built-in ruleset, read once per process.
    pub fn shared() -> &'static Ruleset {
        &BUILTIN_RULESET
    }

    /// A ruleset from a rules file or a directory of `*.yaml` rules files.
    pub fn load(path: &Path) -> Result<Ruleset, RulesetError> {
        let fail = |e: std::io::Error| RulesetError {
            problems: vec![format!("{}: {e}", path.display())],
        };
        let mut sources = Vec::new();
        if path.is_dir() {
            for entry in std::fs::read_dir(path).map_err(fail)? {
                let p = entry.map_err(fail)?.path();
                if p.extension().is_some_and(|e| e == "yaml") {
                    let name = p
                        .file_name()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .into_owned();
                    sources.push((name, std::fs::read_to_string(&p).map_err(fail)?));
                }
            }
        } else {
            let name = path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned();
            sources.push((name, std::fs::read_to_string(path).map_err(fail)?));
        }
        Ruleset::from_sources(&sources)
    }

    /// A ruleset from `(file name, YAML text)` pairs. Refuses what [`Ruleset::check`] finds.
    pub fn from_sources(sources: &[(String, String)]) -> Result<Ruleset, RulesetError> {
        let mut sorted: Vec<&(String, String)> = sources.iter().collect();
        sorted.sort_by(|a, b| a.0.cmp(&b.0));
        let mut problems = Vec::new();
        let mut set = Ruleset {
            digest: digest(&sorted),
            files: sorted.iter().map(|(n, _)| n.clone()).collect(),
            wrappers: Vec::new(),
            programs: Vec::new(),
            rules: Vec::new(),
            by_wrapper: HashMap::new(),
            by_program: HashMap::new(),
            by_rule_program: HashMap::new(),
        };
        for (name, text) in sorted {
            match serde_norway::from_str::<RulesFile>(text) {
                Ok(file) => {
                    if file.format != RULES_FORMAT {
                        problems.push(format!(
                            "{name}: format {} is not {RULES_FORMAT}",
                            file.format
                        ));
                    }
                    set.wrappers.extend(file.wrappers);
                    set.programs.extend(file.programs);
                    set.rules.extend(file.rules);
                }
                Err(e) => problems.push(format!("{name}: {e}")),
            }
        }
        problems.extend(set.check());
        if !problems.is_empty() {
            return Err(RulesetError { problems });
        }
        for (i, w) in set.wrappers.iter().enumerate() {
            for n in &w.names {
                set.by_wrapper.insert(n.clone(), i);
            }
        }
        for (i, p) in set.programs.iter().enumerate() {
            for n in &p.names {
                set.by_program.insert(n.clone(), i);
            }
        }
        for (i, r) in set.rules.iter().enumerate() {
            for n in &r.program {
                set.by_rule_program.entry(n.clone()).or_default().push(i);
            }
        }
        Ok(set)
    }

    /// Ties, duplicate ids and wrappers without arity (§ 8 `rules check`). Unknown categories
    /// and effects are refused when a file is read.
    pub fn check(&self) -> Vec<String> {
        let mut problems = Vec::new();
        let mut ids: BTreeMap<&str, usize> = BTreeMap::new();
        for id in self
            .rules
            .iter()
            .map(|r| r.id.as_str())
            .chain(self.wrappers.iter().map(|w| w.id.as_str()))
        {
            *ids.entry(id).or_default() += 1;
        }
        for (id, n) in ids {
            if n > 1 {
                problems.push(format!("rule id {id} is declared {n} times"));
            }
        }
        for w in &self.wrappers {
            if w.positionals.is_none() && w.until.is_none() {
                problems.push(format!(
                    "wrapper {} declares no arity: neither positionals nor until",
                    w.id
                ));
            }
        }
        let mut names: HashMap<&str, usize> = HashMap::new();
        for w in &self.wrappers {
            for n in &w.names {
                *names.entry(n.as_str()).or_default() += 1;
            }
        }
        for (n, c) in &names {
            if *c > 1 {
                problems.push(format!("wrapper name {n} is declared {c} times"));
            }
        }
        let mut programs: HashMap<&str, usize> = HashMap::new();
        for p in &self.programs {
            for n in &p.names {
                *programs.entry(n.as_str()).or_default() += 1;
            }
        }
        for (n, c) in &programs {
            if *c > 1 {
                problems.push(format!("program {n} is declared {c} times"));
            }
        }
        type Key<'a> = (
            &'a str,
            &'a [String],
            Option<Vec<&'a String>>,
            Option<Vec<&'a String>>,
        );
        let mut keys: HashMap<Key, &str> = HashMap::new();
        fn sorted(v: &Option<Vec<String>>) -> Option<Vec<&String>> {
            v.as_ref().map(|v| {
                let mut s: Vec<&String> = v.iter().collect();
                s.sort();
                s
            })
        }
        for r in &self.rules {
            for p in &r.program {
                let key = (
                    p.as_str(),
                    r.sub.as_slice(),
                    sorted(&r.flags_any),
                    sorted(&r.flags_none),
                );
                if let Some(other) = keys.insert(key, &r.id) {
                    problems.push(format!(
                        "rules {other} and {} tie on program {p}: same subcommand and flag conditions",
                        r.id
                    ));
                }
            }
        }
        problems.sort();
        problems
    }

    pub fn wrapper(&self, name: &str) -> Option<&WrapperRule> {
        self.by_wrapper.get(name).map(|&i| &self.wrappers[i])
    }

    pub fn program(&self, name: &str) -> Option<&ProgramRule> {
        self.by_program.get(name).map(|&i| &self.programs[i])
    }

    pub fn rules_for(&self, name: &str) -> impl Iterator<Item = &Rule> {
        self.by_rule_program
            .get(name)
            .into_iter()
            .flatten()
            .map(|&i| &self.rules[i])
    }

    /// True when some rule names the program.
    pub fn knows(&self, name: &str) -> bool {
        self.by_rule_program.contains_key(name)
    }
}

fn digest(sorted: &[&(String, String)]) -> String {
    let mut bytes = Vec::new();
    for (name, text) in sorted {
        bytes.extend_from_slice(name.as_bytes());
        bytes.push(0);
        bytes.extend_from_slice(text.as_bytes());
        bytes.push(0);
    }
    format!("sha256:{}", crate::lift::digest_hex(&bytes))
}
