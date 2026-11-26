//! One integration-test binary (workspace rule "Tests stay fast"). Every command string here is
//! written by hand; none comes from a transcript (AGENTS.md § Rules).

mod cli;
mod fish;
mod invocations;
mod lifting;

mod rules;
mod schema;
mod shape;

use shellspec::{Dialect, Invocation, Ruleset, Script, invocations};

pub fn zsh(command: &str) -> Script {
    shellspec::lift(command, Dialect::Zsh)
}

pub fn invs(command: &str) -> Vec<Invocation> {
    invocations(&zsh(command), Ruleset::shared())
}

pub fn invs_in(command: &str, dialect: Dialect) -> Vec<Invocation> {
    invocations(&shellspec::lift(command, dialect), Ruleset::shared())
}

/// `program:category` of each invocation.
pub fn summary(list: &[Invocation]) -> Vec<String> {
    list.iter()
        .map(|i| {
            format!(
                "{}:{}",
                i.program.as_deref().unwrap_or("-"),
                i.category.name()
            )
        })
        .collect()
}
