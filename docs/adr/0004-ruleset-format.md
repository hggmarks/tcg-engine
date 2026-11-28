# ADR-0004: Ruleset format and separation between ruleset and cards

- Status: accepted
- Decision 3's typed-model names are amended by ADR-0007 (`Ruleset` becomes `RulesetDefinition`).
- Amended by ADR-0028: the recorded open item on hidden information is decided, and the three Visibility values get their exact meaning (`public` every Viewer, `owner` its owner only, `hidden` nobody, its owner included).
- Amended by ADR-0041: the ruleset also declares Player-State Schemas, a top-level `player_state` list (marked in decision 1).
- Amended by ADR-0046: decision 2's "YAML" is YAML 1.2, read by `serde-saphyr`, in which the unquoted key `on` of a triggered Card Effect is the string `on` (YAML 1.1 reads it as `true`); the catalogs keep it unquoted, and a test pins it (marked in decision 2).
- Date: 2026-10-23

## Context

The engine must support any TCG through a ruleset, with deterministic
execution. The first ruleset will be a minimal game, to validate the design.
Online multiplayer is a future goal, not a current requirement.

## Decisions

1. **Ruleset and cards are separate artifacts.**
   - Ruleset: number of players, zones and visibility, turn phases, victory
     conditions, setup rules, enums, and the card types with the schema of
     their fields (name, data type, constraints) and whether they allow effects.
     *(Amended by ADR-0041: and the Player-State Schemas, the values every
     Player holds, `player_state: [{ id, type, init }]`, a top-level key
     beside the card types.)*
   - Cards: catalog with identity and field values, validated against the
     schema of their type, plus effects.
   - Field data types are a closed set defined by the engine
     (int, bool, text, enum, enum set).
   - Card fields are immutable definition data; values that change during
     a match belong to the instance or are derived.
   - Decks are match input, validated against the ruleset and cards.
   - Dependency: cards depend on the ruleset, never the other way around.
2. **Ruleset and cards are declarative data, not code.** Initial format:
   YAML, in a restricted subset (no anchors, no implicit typing).
   *(Amended by ADR-0046: the YAML is version 1.2, as `serde-saphyr` reads it.
   One spelling means something else in YAML 1.1 and is accepted on purpose:
   the unquoted `on:` key of a triggered Card Effect, a string in 1.2 and the
   boolean `true` in 1.1.)*
3. **Files are never executed.** On load they are validated and converted
   into a typed model (`Ruleset`, `CardCatalog`, `DeckList`). The engine only
   knows this model. The file format is interchangeable.
4. **Effects use a closed set of engine primitives.** Files compose them, and
   new primitives are added only when a concrete ruleset requires them.
5. **Two-stage loading:** first the ruleset, then the catalog validated
   against it.
6. **The number of players is defined by the ruleset.** The core does not
   assume 1v1.

## Consequences

- Determinism: every structure where order matters is a list, never a map.
- Full validation on load, with errors surfaced before the match exists.
- A new frontend (code or script) can produce the same typed model without
  changing the core.
- For future multiplayer, ruleset and catalog must be identifiable by version
  or hash, so client and server can guarantee identical data. Nothing beyond
  that is decided now.

## Review criteria

Reopen this decision if files start needing variables, deeply nested
conditionals or repetition, or if a card needs to change structural rules.
Options: a new engine primitive or an extension mechanism in code/script.

## Recorded open items

- Validate item 6 with a second ruleset that has a different number of players.
- Effect resolution model and hidden information are not yet decided. *Amended by ADR-0028: hidden information is decided (Viewers, Observers and Observed GameStates); the effect resolution model is ADR-0023's.*
- State size (performance): ADR-0021 decided that the GameState shares what a
  transition does not change (ids and Card Instance state are `Arc`s, copied on
  write) and measured the cost of Legal Action listing. Still open: a smaller
  GameState than one list of Card Instances per Game, a listing without one
  transition per candidate, and the per-transition end-of-Game scan, which still
  visits every card.
