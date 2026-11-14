# Ruleset: Effects

The ruleset of Phase C. It is **derived from the Small Ruleset** (`../small-ruleset/small-ruleset.md`), which is frozen as the canonical ruleset of Phase B: from now on a rule decision is made here, and the Small Ruleset files do not change (ADR-0022).

Every rule not stated on this page is the Small Ruleset's, word for word: parameters, zones, decks, setup, the turn, combat and win conditions. A Game of the Effects Ruleset with the same Seed, decks and Actions plays identically to a Game of the Small Ruleset, Event for Event (its golden game, `crates/tcg-loader/tests/golden/effects-leader-kill.trace`, is the Small Ruleset's Leader-kill game).

## What differs

| | Small Ruleset | Effects Ruleset |
|---|---|---|
| `meta` | `id: small`, `version: 1` | `id: effects`, `version: 1` |
| **Creature** `effects` | `false` | `true`: a creature definition may carry Card Effects |
| `effects.active_zone` | none (no `effects` block) | `field`: the Active Zone, where Card Effects act (see Triggered Card Effects) |
| **Leader** and **Mana** `effects` | `false` | `false`: Card Effects act only while the card is on the Field (roadmap #48); a Leader never reaches the Field, and a mana card never does either and has no int field to modify |
| `effects.active_zone` | none | `field`: the zone where Card Effects act (below) |

The files are `effects-ruleset.yaml` (the encoding), `effects-catalog.yaml` (the cards of the Small catalog, with `meta: { ruleset: effects, ruleset_version: 1 }`, the five creatures of the triggered Card Effect section and the two of the continuous one below) and `decks/deck-a.yaml` and `decks/deck-b.yaml` (the Small sample decks, which list none of the seven new creatures: that keeps the golden game equal to the Small Ruleset's).

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
  - `on` is a single-key map. `enter: <zone>` matches the card entering that zone (a declared zone); `turn_start: {}` matches the controller's Turn beginning (semantics in Triggered Card Effects below). Any other key is an unknown matcher.
  - `if` must be a bool Expression.
  - `do` is a list of Action Effects (`move`, `set_state`, `add_state`, `draw`, `if`; ADR-0015, ADR-0016, ADR-0023), except that **`end_turn` is rejected anywhere in it**, including inside an `if` effect: a trigger that ends the Turn is not covered yet.
- **Continuous**: `while` (a bool Expression) and `modify: { field, amount }` (required). `field` must be a field the card's card type declares, of type int (an instance state is not a field); `amount` is an unquoted integer. The modifier adds `amount` to that field of the carrying card while it is active (see Continuous Card Effects).
- **`self`** is the one name an Expression or an effect of a Card Effect may use as a ref: the carrying card, of the card's own card type. `{ ref: self }` is a card and `{ ref: self.power }` is a value (a field or an instance state of the card's type). Any other name is a dangling reference to a param.
- An element has the keys of one shape only: `on` or `if` or `do` together with `while` or `modify` is a mixed shape, and an element with neither `on` nor `while` has no shape.

### Validation

The loader reads the syntax and rejects an unknown key of the envelope or of `modify`, and an `amount` that is not an unquoted integer. The engine checks the rest, the first error winning, in this order: the list against the card type's `effects` flag, then each element in order: its shape, then for a triggered one its `on`, `if` and `do` (and `end_turn` in `do`), for a continuous one its `while` and `modify`.

A well-formed Card Effect of either shape is kept by its card definition and acts: triggered ones run (ADR-0023) and continuous ones are read as Field Modifiers (ADR-0024). Nothing well-formed is rejected as unsupported any more (ADR-0022).

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

## The `draw` effect

`draw: { player: controller, from: <zone>, to: <zone>, count: <n> }` is an Action Effect that exists in a Card Effect's `do` (ADR-0023). For `player`, the **Controller** (the owner of `self`, the carrying card; the only selector of an effect), it moves `count` cards one at a time from the Top of `from` onto the Top of `to`, exactly as the Start phase's `draw` step does. If `from` is empty, the effect records a `DrawFromEmptyZone` Event and stops: that Event is what `draw_from_empty` win conditions read, so a trigger can make a Player lose by `deck_out`. `from` must be an ordered zone, `count` an unquoted positive integer, and both zones must be declared. `controller` needs a `self`, so a ruleset action's `effects` rejects it (`ControllerOutsideCardEffect`).

## Triggered Card Effects

A Card Effect acts only on cards in the **Active Zone**, `effects.active_zone`, which is `field` here; a ruleset without the `effects` block has no Active Zone and none of its triggers is ever eligible, `enter` included. With the block, the one exception to the Active Zone is `enter`.

- **`enter: <zone>`** is woken by a `CardMoved` Event that puts the card into that zone, whichever zone it is and whether or not it is the Active Zone: `enter: field` fires when the card is played, `enter: trash` when it dies in combat. A `CardCreated` Event never wakes it: placing the cards of the decks at Game creation and creating cards fire nothing, and no trigger resolves during Game creation.
- **`turn_start: {}`** is woken by the `TurnBegan` Event of the card's controller, and only if the card is in the Active Zone when that Event is scanned. It fires on its controller's Turn only, in the transition that begins it (the transition of the Action that passed the Turn, or Game creation's first Turn, which fires nothing). Its Events land after the Events of the transition's automation, so they follow `PhaseBegan { main }`, and the Start phase's untap and `recently_played` reset cannot undo what it does.
- **Resolution.** After the Action's costs and effects, the Events of the transition are scanned in order from the first, costs included. An Event that wakes a trigger enqueues it; the queue is first in first out. Eligibility is decided when the Event is scanned: an enqueued trigger fires even if its card moved since. When a trigger fires, its `if` is read (on the state as it is then, `self` being the card) and, if false, nothing happens; otherwise its `do` runs to completion. The Events it emits are appended to the same Event Trace and scanned in turn, so a cascade queues behind the triggers already waiting. When no Event is left to scan and the queue is empty, resolution ends.
- **Order.** One Event waking several triggers enqueues them: the cards of the Active Player first, then of the other Players in seat order; within a Player in `state.cards()` order; within a card in the order of its `effects`. (With `enter` and `turn_start`, one Event only wakes the cards of one Player, so the order across Players cannot be observed yet.)
- **The Trigger Cap is 100.** A transition resolves at most 100 triggers. Every trigger that leaves the queue counts, one whose `if` is false included. The 101st rejects the whole transition with `TriggerCapExceeded { cap: 100 }`: no Event is returned and the GameState stays as it was. A mutually-triggering pair of effects (on one card, as `restless_shade` has) reaches the cap.
- **Win conditions** are checked once, after all triggers, on the final state and the whole Event Trace (`GameEnded` is the last Event). A trigger can cause a `deck_out` loss through `draw`; `leader_dead` is not reachable from a trigger that only reads and changes `self`.
- A trigger asks for no decision: the Game Record is unchanged (ADR-0019).

### Catalog cards

One creature per family, in `effects-catalog.yaml`; none is in a shipped deck.

| Card | Effect | Shows |
|---|---|---|
| `lamp_keeper` | `enter: field` → draw 1 | the draw on play, the trigger's Events after the play's |
| `dawn_scout` | `turn_start` → draw 1 | firing on its controller's Turn only, only in the Field |
| `grave_jackal` | `enter: trash` → draw 1 | the `enter` exception to the Active Zone: it fires on dying in combat |
| `eager_recruit` | `enter: field`, `if` `self.recently_played` → draw 1 | an `if` read at fire time: true when played, false when `recently_played` is false, for example on an arrival after the Turn it was played (a false `if` at fire time is exercised by the engine tests; no shipped card or deck reaches it) |
| `restless_shade` | `enter: field` → move self to trash; `enter: trash` → move self to field | a cascade that exceeds the Trigger Cap: playing it is rejected |

## Glossary: Effects Ruleset

| Term | Definition |
|---|---|
| **Card Effect** | An ability a card definition carries, in the `effects` list of a card of a type with `effects: true`. Triggered or continuous. |
| **Triggered Card Effect** | `on`, optional `if` and `do`: when an Event matches the Trigger Matcher and the condition holds, the Action Effects run. |
| **Continuous Card Effect** | `while` and `modify`: while the Field Modifier is active, every read of the field adds its amount. |
| **Trigger Matcher** | What a triggered Card Effect matches among Events: `enter` (a `CardMoved` Event puts the card into a zone) or `turn_start` (the controller's Turn began). |
| **Active Zone** | `effects.active_zone` (`field`): the zone where Card Effects act. `enter` is the exception. Without the block no Card Effect is active: no modifier applies and no trigger is eligible. |
| **Trigger Cap** | The most trigger resolutions in one transition: 100. A false `if` counts. One more rejects the transition (`TriggerCapExceeded`). |
| **Controller** | The owner of `self`, the carrying card: the Player a `draw` in a Card Effect draws for. |
| **Draw effect** | `draw: { player, from, to, count }`: draws like the `draw` step; an empty `from` records `DrawFromEmptyZone`. |
| **Field Modifier** | `modify: { field, amount }`: an additive int on a field of the carrying card's definition. It is **active** while its card is in the Active Zone and its `while` holds. |
| **Derived Read** | A read of a field through a ref: the base value plus the amounts of the active Field Modifiers, computed at read time and stored nowhere. |
| **Self** | `self`, the carrying card, the only name in a Card Effect's scope. |
