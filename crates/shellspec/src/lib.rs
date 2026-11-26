//! shellspec lifts one shell command string, as an agent's Bash tool sends it, into one IR
//! (`shell-ir/1`) from bash, POSIX sh, zsh or fish syntax, and gives each simple command in it a
//! category (docs/design.md). Lifting is pure: it never executes, expands or globs anything.
//!
//! ```text
//! use shellspec::{Dialect, Ruleset, Category, invocations, lift};
//! let script = lift("cd x && slot cargo test 2>&1 | tail -20", Dialect::Zsh);
//! let list = invocations(&script, &Ruleset::builtin());
//! let cats: Vec<Category> = list.iter().map(|i| i.category).collect();
//! assert_eq!(cats, [Category::Navigate, Category::Test, Category::Filter]);
//! ```

pub mod cli;
pub mod invocation;
pub mod ir;
pub mod lift;
pub mod rules;
pub mod shape;

pub use invocation::{
    Guard, Invocation, InvocationContext, Stage, StageRole, dominant, invocations,
};
pub use ir::{Dialect, FORMAT, Script, UnkReason};
pub use lift::lift;
pub use rules::{Category, Effect, Ruleset, RulesetError};
pub use shape::shape;
