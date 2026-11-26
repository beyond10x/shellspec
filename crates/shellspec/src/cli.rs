//! The callables of `spec/cli.yaml`, as functions from the validated input to the declared
//! result. The binary hands them to the generated adapter; tests call them the same way.

use std::collections::BTreeMap;
use std::path::Path;

use serde_json::{Value, json};

use crate::ir::{Dialect, FORMAT, UnkReason};
use crate::rules::{Category, Ruleset};
use crate::{dominant, invocations, lift, shape};

/// A declared error: its code and its detail.
pub type Refusal = (String, String);

fn text<'a>(input: &'a Value, field: &str) -> &'a str {
    input.get(field).and_then(Value::as_str).unwrap_or("")
}

fn dialect(input: &Value) -> Dialect {
    Dialect::parse(text(input, "dialect")).unwrap_or(Dialect::Zsh)
}

fn ruleset(input: &Value) -> Result<Ruleset, Refusal> {
    match input.get("ruleset").and_then(Value::as_str) {
        None => Ok(Ruleset::builtin()),
        Some(path) => Ruleset::load(Path::new(path))
            .map_err(|e| ("ruleset_invalid".to_owned(), e.to_string())),
    }
}

/// Runs one callable. `Ok(None)` is a callable this build does not know.
pub fn call(callable: &str, input: &Value) -> Result<Option<Value>, Refusal> {
    let result = match callable {
        "lift" => {
            let d = dialect(input);
            let script = lift(text(input, "command"), d);
            json!({
                "format": FORMAT,
                "parser": crate::lift::parser_for(d),
                "script": serde_json::to_value(&script).unwrap_or(Value::Null),
            })
        }
        "invocations" => {
            let rules = ruleset(input)?;
            let script = lift(text(input, "command"), dialect(input));
            let list = invocations(&script, &rules);
            let mut out = json!({
                "ruleset": rules.digest,
                "invocations": serde_json::to_value(&list).unwrap_or(Value::Null),
            });
            if let Some(d) = dominant(&list) {
                out["dominant"] = json!(d.index);
            }
            out
        }
        "shape" => {
            let script = lift(text(input, "command"), dialect(input));
            json!({ "shape": shape(&script) })
        }
        "rules_check" => {
            let rules = ruleset(input)?;
            json!({
                "ruleset": rules.digest,
                "files": rules.files,
                "wrappers": rules.wrappers.len(),
                "programs": rules.programs.len(),
                "rules": rules.rules.len(),
            })
        }
        "corpus_stats" => {
            let rules = ruleset(input)?;
            let path = text(input, "input");
            let body = std::fs::read_to_string(path)
                .map_err(|e| ("input_unreadable".to_owned(), format!("{path}: {e}")))?;
            let commands =
                body.lines()
                    .filter_map(|line| match serde_json::from_str::<Value>(line).ok()? {
                        Value::String(s) => Some(s),
                        Value::Object(o) => o.get("command")?.as_str().map(str::to_owned),
                        _ => None,
                    });
            serde_json::to_value(corpus_stats(commands, dialect(input), &rules))
                .unwrap_or(Value::Null)
        }
        _ => return Ok(None),
    };
    Ok(Some(result))
}

/// The counts of `shellspec corpus stats` (`shellspec.cli.CorpusStats`).
#[derive(Debug, Clone, serde::Serialize)]
pub struct CorpusStats {
    pub ruleset: String,
    pub commands: usize,
    pub refused: usize,
    pub unk_regions: usize,
    pub invocations: usize,
    pub unk_invocations: usize,
    pub unk_permille: usize,
    pub categories: Vec<Count>,
    pub unk_reasons: Vec<Count>,
    pub unk_programs: Vec<Count>,
    pub dominant: Vec<Count>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct Count {
    pub key: String,
    pub count: usize,
}

fn counts(map: BTreeMap<String, usize>, limit: usize) -> Vec<Count> {
    let mut v: Vec<Count> = map
        .into_iter()
        .map(|(key, count)| Count { key, count })
        .collect();
    v.sort_by(|a, b| b.count.cmp(&a.count).then_with(|| a.key.cmp(&b.key)));
    v.truncate(limit);
    v
}

/// Lifts and classifies every command; keeps only counts and program names.
pub fn corpus_stats(
    commands: impl Iterator<Item = String>,
    dialect: Dialect,
    rules: &Ruleset,
) -> CorpusStats {
    let mut n = 0;
    let mut refused = 0;
    let mut unk_regions = 0;
    let mut total = 0;
    let mut unk = 0;
    let mut categories: BTreeMap<String, usize> = BTreeMap::new();
    let mut reasons: BTreeMap<String, usize> = BTreeMap::new();
    let mut programs: BTreeMap<String, usize> = BTreeMap::new();
    let mut dominants: BTreeMap<String, usize> = BTreeMap::new();
    for command in commands {
        n += 1;
        let script = lift(&command, dialect);
        let syntax = script
            .diagnostics
            .iter()
            .filter(|d| matches!(d.reason, UnkReason::Syntax | UnkReason::Dialect))
            .count();
        unk_regions += syntax;
        if syntax > 0 {
            refused += 1;
        }
        let list = invocations(&script, rules);
        for inv in &list {
            total += 1;
            *categories
                .entry(inv.category.name().to_owned())
                .or_default() += 1;
            if inv.category == Category::Unk {
                unk += 1;
                let reason = inv.unk.map(UnkReason::name).unwrap_or("none");
                *reasons.entry(reason.to_owned()).or_default() += 1;
                if inv.unk == Some(UnkReason::NoRule)
                    && let Some(p) = &inv.program
                {
                    *programs.entry(p.clone()).or_default() += 1;
                }
            }
        }
        let key = dominant(&list).map(|d| d.category.name()).unwrap_or("none");
        *dominants.entry(key.to_owned()).or_default() += 1;
    }
    CorpusStats {
        ruleset: rules.digest.clone(),
        commands: n,
        refused,
        unk_regions,
        invocations: total,
        unk_invocations: unk,
        unk_permille: (unk * 1000).checked_div(total).unwrap_or(0),
        categories: counts(categories, usize::MAX),
        unk_reasons: counts(reasons, usize::MAX),
        unk_programs: counts(programs, 40),
        dominant: counts(dominants, usize::MAX),
    }
}
