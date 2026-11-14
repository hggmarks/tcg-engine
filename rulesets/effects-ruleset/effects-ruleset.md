# Ruleset: Effects

The ruleset of Phase C. It is **derived from the Small Ruleset** (`../small-ruleset/small-ruleset.md`), which is frozen as the canonical ruleset of Phase B: from now on a rule decision is made here, and the Small Ruleset files do not change (ADR-0022).

Every rule not stated on this page is the Small Ruleset's, word for word: parameters, zones, decks, setup, the turn, combat and win conditions. A Game of the Effects Ruleset with the same Seed, decks and Actions plays identically to a Game of the Small Ruleset, Event for Event (its golden game, `crates/tcg-loader/tests/golden/effects-leader-kill.trace`, is the Small Ruleset's Leader-kill game).

## What differs

| | Small Ruleset | Effects Ruleset |
|---|---|---|
| `meta` | `id: small`, `version: 1` | `id: effects`, `version: 1` |
| **Creature** `effects` | `false` | `true`: a creature definition may carry Card Effects |
| **Leader** and **Mana** `effects` | `false` | `false`: Card Effects act only while the card is on the Field (roadmap #48); a Leader never reaches the Field, and a mana card never does either and has no int field to modify |
| `effects.active_zone` | none | `field`: the zone where Card Effects act (below) |

The files are `effects-ruleset.yaml` (the encoding), `effects-catalog.yaml` (the cards of the Small catalog, with `meta: { ruleset: effects, ruleset_version: 1 }`) and `decks/deck-a.yaml` and `decks/deck-b.yaml` (the Small sample decks). The catalog is the Small catalog plus two creatures that carry a Continuous Card Effect (see below); the sample decks do not use them.

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
- **Continuous**: `while` (a bool Expression) and `modify: { field, amount }` (required). `field` must be a field the card's card type declares, of type int (an instance state is not a field); `amount` is an unquoted integer. The modifier adds `amount` to that field of the carrying card while it is active (see Continuous Card Effects).
- **`self`** is the one name an Expression or an effect of a Card Effect may use as a ref: the carrying card, of the card's own card type. `{ ref: self }` is a card and `{ ref: self.power }` is a value (a field or an instance state of the card's type). Any other name is a dangling reference to a param.
- An element has the keys of one shape only: `on` or `if` or `do` together with `while` or `modify` is a mixed shape, and an element with neither `on` nor `while` has no shape.

### Validation

The loader reads the syntax and rejects an unknown key of the envelope or of `modify`, and an `amount` that is not an unquoted integer. The engine checks the rest, the first error winning, in this order: the list against the card type's `effects` flag, then each element in order: its shape, then for a triggered one its `on`, `if` and `do` (and `end_turn` in `do`), for a continuous one its `while` and `modify`.

**A triggered Card Effect does not execute yet.** A well-formed one is rejected as an unsupported triggered Card Effect, at `cards[<id>].effects[<i>]`; ticket [17] lifts this (ADR-0022). A continuous one is supported (ADR-0024).

## Continuous Card Effects

A Continuous Card Effect is a **derived read**: nothing is written to the Game and no Event is emitted for it. Every time a rule reads an int field of a card through a ref (a param `where`, an effect `if`, a cost's `count`, the combat comparison, a win condition `when`), the value is the field's base value plus the amounts of the Field Modifiers that are active at that moment (ADR-0024).

- **Active zone.** The top-level `effects: { active_zone: field }` block names the zone where Card Effects act. A Field Modifier is active only while its carrying card is in that zone, and its `while` holds. A card anywhere else, a false `while`, or a ruleset without the block reads its base values.
- **`while` reads base values only.** The condition of a Field Modifier is evaluated on base values, never on modified ones, whatever field it reads. So `while: { gte: [ { ref: self.power }, 3 ] }` with `modify: { field: power, amount: -1 }` on a power-3 card holds, and the card is power 2 (a derived read of a `while` that read derived values would be circular).
- **Additive and commuting.** Several active modifiers on the same field add up, in any order. An int that would overflow, the sum of modifiers included, is the rejection `DerivedValueOverflow`, never a wrap.
- **Fields only.** Only card fields (int) are modified; instance states (`tapped`, `current_life`) are read as stored.
- **The carrying card only.** A modifier changes a field of the card that carries it. A modifier that reaches other cards is not in the format yet (ticket [19]).

The catalog carries two creatures that show it. A creature taps when it attacks, before the combat compares powers, so a modifier that holds `while` the creature is tapped is the one a Game flips:

| Card | Cost | Power | Continuous Card Effect |
|---|---|---|---|
| `battle_veteran` | 1 | 2 | `while: { eq: [ { ref: self.tapped }, true ] }`, `modify: { field: power, amount: 1 }`: power 3 once tapped |
| `weary_ox` | 2 | 3 | the same `while`, `amount: -1`: power 2 once tapped |

## Glossary: Effects Ruleset

| Term | Definition |
|---|---|
| **Card Effect** | An ability a card definition carries, in the `effects` list of a card of a type with `effects: true`. Triggered or continuous. |
| **Triggered Card Effect** | `on`, optional `if` and `do`: when an Event matches the Trigger Matcher and the condition holds, the Action Effects run. |
| **Continuous Card Effect** | `while` and `modify`: while the Field Modifier is active, every read of the field adds its amount. |
| **Trigger Matcher** | What a triggered Card Effect matches among Events: `enter` (the card enters a zone) or `turn_start` (the controller's Turn began). |
| **Field Modifier** | `modify: { field, amount }`: an additive int on a field of the carrying card's definition. It is **active** while its card is in the active zone and its `while` holds. |
| **Derived Read** | A read of a field through a ref: the base value plus the amounts of the active Field Modifiers, computed at read time and stored nowhere. |
| **Active zone** | `effects.active_zone`: the zone where Card Effects act. |
| **Self** | `self`, the carrying card, the only name in a Card Effect's scope. |
