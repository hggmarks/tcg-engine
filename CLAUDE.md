## Agent skills
 
### Project
 
This repository contains a generic, deterministic Trading Card Game (TCG) engine.
 
The long-term goal is to provide a reusable simulation engine that can support:
- Any TCGs through a ruleset
- deterministic simulation
- browser/WASM applications
- desktop and mobile applications
- future graphical clients
- online multiplayer
- AI training and self-play
- replay and debugging
The engine is the source of truth for game execution.
 
### Core Architectural Principles
 
- The game state MUST be explicit and fully contained in the current game state.
- There MUST be no global mutable game state.
- Actions represent player/system intent.
- The engine validates actions before executing them.
- Events represent facts that occurred during execution.
- Effects describe game behavior and are resolved by the engine following the ruleset.
- Rules belong to a Ruleset, not to the generic engine.
- Card definitions are immutable game data.
- Card instances represent runtime state.
- Derived values should be calculated rather than duplicated as mutable state.
- The core engine MUST remain independent of UI, networking, persistence, and AI.
- Deterministic execution is a fundamental requirement.
- Prefer strong types and explicit domain concepts over generic primitives.
- Avoid premature abstractions. Let the domain emerge through incremental development.
### Domain Model
 
The current conceptual model is:
 
Game:
 
→ GameState
→ Players
→ Zones
→ Card Instances
→ Effects / Modifiers
 
Execution follows:
 
Action:
 
→ Validation
→ Execution
→ Events
→ New GameState
 
Card behavior follows:
 
Card:
→ Effects
→ Ruleset / Engine
→ New GameState
 
These are conceptual models, not implementation requirements. Do not assume specific implementation unless stated otherwise.
 
### Development Philosophy
 
Do not attempt to design the entire engine upfront.
 
Prefer:
 
understand → question → decide → document → implement → validate
 
over:
 
design everything → implement everything
 
When a design decision is uncertain, surface the uncertainty and investigate it before implementing.
 
Do not invent requirements merely to make implementation easier.
 
Plan the steps and tasks before development.
 

## Definition of Done and the gate

A ticket is done when all of this holds:

- The gate below passes.
- Every acceptance criterion is covered by a test named in `CONTEXT.md` vocabulary.
- New or changed domain terms are in `CONTEXT.md`.
- Decisions are recorded as short ADRs in `docs/adr/`, and any conflict with an existing ADR is flagged.
- Rule decisions are mirrored in `rulesets/effects-ruleset/effects-ruleset.md` and `.yaml`. The Small Ruleset (`rulesets/small-ruleset/`) is frozen since Phase C: it does not change (ADR-0022).
- Nothing under the ticket's *Out of scope* is introduced.

The gate (toolchain pinned in `rust-toolchain.toml`; `--all`/`--workspace` so later members are covered):

```
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo build -p tcg-engine --target wasm32-unknown-unknown
```

## Determinism rules

| Rule | Enforced by |
|---|---|
| No `HashMap`/`HashSet`; use `Vec`, `BTreeMap`, `BTreeSet` | `clippy.toml` `disallowed-types` |
| No wall-clock time, threads or environment access | `clippy.toml` `disallowed-methods` |
| No floating point arithmetic | `clippy::float_arithmetic` in `[workspace.lints]` |
| No `unsafe` | `unsafe_code = "forbid"` in `[workspace.lints]` |
| Overflow checks on in every profile, release included | `overflow-checks = true` in each `[profile.*]` |
| No I/O in the engine core | documented, enforced in review |
| No panics on any input: checked arithmetic, typed errors | documented, enforced in review |
| The only randomness is the seeded RNG stored in the GameState | documented, enforced in review |
| Anything where order matters is a list (ADR-0004) | documented, enforced in review |

- New crates must set `[lints] workspace = true` and must not add their own `clippy.toml` (it would shadow the root one).
- A local `allow`/`expect` needs `reason = "..."` (`clippy::allow_attributes_without_reason`) and is allowed only in `examples/` and `benches/`.

## Working on roadmap tickets

- The Roadmap is issue #14. Tickets are its sub-issues, taken in `[nn]` order once unblocked.
- One branch `ticket/<n>-<slug>` and one PR containing `Closes #<n>` per ticket.
- Stop and ask: when an ambiguity the ticket does not cover would change game rules or a public contract, do not invent a rule. Comment the question, the options and a recommendation on the ticket, add the `needs-info` label, and stop.

## Where things live

- `CONTEXT.md`: domain vocabulary. Name types, functions and tests with its terms.
- `docs/adr/`: accepted decisions.
- `docs/agents/`: how agents consume the domain docs and the issue tracker.
- `rulesets/`: game rules (`.md` spec and `.yaml` encoding).
