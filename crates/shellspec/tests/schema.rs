//! The Rust types and the ESS specification describe one wire form: lifted trees and
//! invocations are checked against the JSON Schema `ess generate` projects into `generated/`.

use std::path::PathBuf;

use serde_json::Value;

use crate::{invs, zsh};

fn schema(name: &str) -> Value {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../generated/schema/types")
        .join(format!("{name}.schema.json"));
    serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap()
}

/// The subset of JSON Schema the projection uses: `$ref`, `type`, `properties`, `required`,
/// `additionalProperties: false`, `items`, `oneOf`, `const`, `enum`.
fn check(root: &Value, s: &Value, v: &Value, at: &str, errors: &mut Vec<String>) {
    if let Some(r) = s.get("$ref").and_then(Value::as_str) {
        let name = r.trim_start_matches("#/$defs/");
        return check(root, &root["$defs"][name], v, at, errors);
    }
    if let Some(options) = s.get("oneOf").and_then(Value::as_array) {
        let matching = options
            .iter()
            .filter(|o| {
                let mut e = Vec::new();
                check(root, o, v, at, &mut e);
                e.is_empty()
            })
            .count();
        if matching != 1 {
            errors.push(format!("{at}: {matching} of oneOf match"));
        }
        return;
    }
    if let Some(c) = s.get("const")
        && c != v
    {
        errors.push(format!("{at}: {v} is not {c}"));
    }
    if let Some(e) = s.get("enum").and_then(Value::as_array)
        && !e.contains(v)
    {
        errors.push(format!("{at}: {v} not in enum"));
    }
    let ok = match s.get("type").and_then(Value::as_str) {
        Some("object") => v.is_object(),
        Some("array") => v.is_array(),
        Some("string") => v.is_string(),
        Some("integer") => v.is_i64() || v.is_u64(),
        Some("boolean") => v.is_boolean(),
        _ => true,
    };
    if !ok {
        errors.push(format!("{at}: {v} is not {}", s["type"]));
        return;
    }
    if let (Some(props), Some(obj)) = (
        s.get("properties").and_then(Value::as_object),
        v.as_object(),
    ) {
        for req in s["required"].as_array().into_iter().flatten() {
            let k = req.as_str().unwrap();
            if !obj.contains_key(k) {
                errors.push(format!("{at}: missing {k}"));
            }
        }
        for (k, val) in obj {
            match props.get(k) {
                Some(p) => check(root, p, val, &format!("{at}.{k}"), errors),
                None if s.get("additionalProperties") == Some(&Value::Bool(false)) => {
                    errors.push(format!("{at}: undeclared {k}"))
                }
                None => {}
            }
        }
    }
    if let (Some(items), Some(arr)) = (s.get("items"), v.as_array()) {
        for (i, x) in arr.iter().enumerate() {
            check(root, items, x, &format!("{at}[{i}]"), errors);
        }
    }
}

fn assert_valid(schema_name: &str, v: &Value) {
    let root = schema(schema_name);
    let mut errors = Vec::new();
    check(&root, &root, v, "$", &mut errors);
    assert!(errors.is_empty(), "{schema_name}: {errors:#?}");
}

/// Touches every node kind, word part and redirect kind brush gives.
const RICH: &str = "X=1 a=(1 2) cmd 'q' \"d $x ${y:-z} $(sub)\" `bt` $'\\n' *.rs {a,b} ~/h $((1+2)) <(p) >(q) < in > out >> app 2>&1 &> both <<< hs <<-EOF\n\tbody $(h)\n\tEOF\n\
! a | b |& c && d || e; (s); { g; }; if t; then u; elif v; then w; else x; fi; while y; do z; done; until y; do z; done; \
for i in 1 2; do :; done; for ((i=0;i<2;i++)); do :; done; case $k in a|b) m;; esac; f() { n; }; [[ -f x && $y == z ]]; (( k++ )); sleep 1 &\necho `unterminated";

#[test]
fn a_lifted_script_is_a_shellspec_syntax_script() {
    let v = serde_json::to_value(zsh(RICH)).unwrap();
    assert_valid("shellspec.syntax.Script", &v);
    let fish = shellspec::lift(
        "begin; a; end > o; if b; c; else if d; e; else; f; end; for x in (g) \"$h\" ~/i *.j {k,l}; m 2>&1; end; switch $n; case o; p; end; function q; r; end; not s &| t; u; and v",
        shellspec::Dialect::Fish,
    );
    assert_valid(
        "shellspec.syntax.Script",
        &serde_json::to_value(fish).unwrap(),
    );
}

#[test]
fn invocations_are_shellspec_invocations() {
    for inv in invs(RICH) {
        let v = serde_json::to_value(&inv).unwrap();
        assert_valid("shellspec.invocation.Invocation", &v);
        for w in v["argv"].as_array().unwrap() {
            assert_valid("shellspec.word.Word", w);
        }
        for r in v["redirects"].as_array().unwrap() {
            assert_valid("shellspec.word.Redirect", r);
        }
    }
}

#[test]
fn rules_files_are_shellspec_rules_files() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("rules");
    for name in shellspec::rules::builtin_files() {
        let text = std::fs::read_to_string(dir.join(name)).unwrap();
        let v: Value = serde_norway::from_str(&text).unwrap();
        assert_valid("shellspec.classification.RulesFile", &v);
    }
}
