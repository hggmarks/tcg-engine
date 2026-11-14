# Ruleset: Effects

The ruleset of Phase C. It is **derived from the Small Ruleset** (`../small-ruleset/small-ruleset.md`), which is frozen as the canonical ruleset of Phase B: from now on a rule decision is made here, and the Small Ruleset files do not change (ADR-0022).

Every rule not stated on this page is the Small Ruleset's, word for word: parameters, zones, decks, setup, the turn, combat and win conditions. A Game of the Effects Ruleset with the same Seed, decks and Actions plays identically to a Game of the Small Ruleset, Event for Event (its golden game, `crates/tcg-loader/tests/golden/effects-leader-kill.trace`, is the Small Ruleset's Leader-kill game).

## What differs

| | Small Ruleset | Effects Ruleset |
|---|---|---|
| `meta` | `id: small`, `version: 1` | `id: effects`, `version: 1` |
| **Creature** `effects` | `false` | `true`: a creature definition may carry Card Effects |
| **Leader** and **Mana** `effects` | `false` | `false`: Card Effects act only while the card is on the Field (roadmap #48); a Leader never reaches the Field, and a mana card never does either and has no int field to modify |

The files are `effects-ruleset.yaml` (the encoding), `effects-catalog.yaml` (the cards of the Small catalog, with `meta: { ruleset: effects, ruleset_version: 1 }`) and `decks/deck-a.yaml` and `decks/deck-b.yaml` (the Small sample decks). **No card of the catalog carries a Card Effect yet.**

## Card Effects

A card definition's `effects` is a list of **Card Effects**, allowed only on a card type declared `effects: true`. A Card Effect is one of two shapes, told apart by which shape keys appear (`on`, `if`, `do` or `while`, `modify`; any key of both shapes is a mixed shape):

```yaml
effects:
  - on: { enter: field }     # triggered: a Trigger Matcher over Events
    if: { ... }              # optional Expression, evaluated when it fires
    do: [ <action effects> ] # the existing Action Effects; at least one
  - while: { ... }           # continuous: an Expression
    modify: { field: power, amount: 1 }   # an additive int on the carrying card's fields
```

- **Triggered**: `on` (required), `if` (optional) and `do` (required, at least one effect).
  - `on` is a single-key map. `enter: <zone>` matches the card entering that zone (a declared zone); `turn_start: {}` matches the controller's Turn beginning (the semantics land in ticket [17], #50). Any other key is an unknown matcher.
  - `if` must be a bool Expression.
  - `do` is a list of Action Effects (`move`, `set_state`, `add_state`, `if`; ADR-0015, ADR-0016), except that **`end_turn` is rejected anywhere in it**, including inside an `if` effect: a trigger that ends the Turn is not covered yet.
- **Continuous**: `while` (a bool Expression) and `modify: { field, amount }` (required). `field` must be a field the card's card type declares, of type int (an instance state is not a field); `amount` is an unquoted integer. The modifier is meant to add `amount` to that field of the carrying card while the condition holds.
- **`self`** is the one name an Expression or an effect of a Card Effect may use as a ref: the carrying card, of the card's own card type. `{ ref: self }` is a card and `{ ref: self.power }` is a value (a field or an instance state of the card's type). Any other name is a dangling reference to a param.
- An element has the keys of one shape only: `on` or `if` or `do` together with `while` or `modify` is a mixed shape, and an element with neither `on` nor `while` has no shape.

### Validation

The loader reads the syntax and rejects an unknown key of the envelope or of `modify`, and an `amount` that is not an unquoted integer. The engine checks the rest, the first error winning, in this order: the list against the card type's `effects` flag, then each element in order: its shape, then for a triggered one its `on`, `if` and `do` (and `end_turn` in `do`), for a continuous one its `while` and `modify`.

**No Card Effect executes yet.** A well-formed triggered Card Effect is rejected as an unsupported triggered Card Effect and a well-formed continuous one as an unsupported continuous Card Effect, each at `cards[<id>].effects[<i>]`. The later tickets of Phase C lift this, one kind at a time (ADR-0022).

## Glossary: Effects Ruleset

| Term | Definition |
|---|---|
| **Card Effect** | An ability a card definition carries, in the `effects` list of a card of a type with `effects: true`. Triggered or continuous. |
| **Triggered Card Effect** | `on`, optional `if` and `do`: when an Event matches the Trigger Matcher and the condition holds, the Action Effects run. |
| **Continuous Card Effect** | `while` and `modify`: while the condition holds, the Field Modifier applies to the carrying card. |
| **Trigger Matcher** | What a triggered Card Effect matches among Events: `enter` (the card enters a zone) or `turn_start` (the controller's Turn began). |
| **Field Modifier** | `modify: { field, amount }`: an additive int on a field of the carrying card's definition. |
| **Self** | `self`, the carrying card, the only name in a Card Effect's scope. |
