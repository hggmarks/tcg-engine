//! The generic, deterministic Trading Card Game engine core.
//!
//! The engine executes Actions against an explicit GameState and produces
//! Events. Game rules belong to Rulesets, not to this crate.
//!
//! Determinism is a fundamental requirement. The rules that protect it
//! (no `HashMap`/`HashSet`, floating point, wall-clock time, threads,
//! environment access or I/O; seeded randomness only; no panics) are listed
//! in the "Determinism rules" section of `CLAUDE.md`, and most are enforced
//! by `clippy.toml` and `[workspace.lints]`.

pub mod player;
pub mod ruleset;
pub mod runtime;
pub mod turn;
