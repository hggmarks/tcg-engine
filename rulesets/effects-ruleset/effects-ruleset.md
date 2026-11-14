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

The files are `effects-ruleset.yaml` (the encoding), `effects-catalog.yaml` (the cards of the Small catalog, with `meta: { ruleset: effects, ruleset_version: 1 }`, and the five creatures of the triggered Card Effect section below) and `decks/deck-a.yaml` and `decks/deck-b.yaml` (the Small sample decks, which list none of the five new creatures: that keeps the golden game equal to the Small Ruleset's).

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
- **Continuous**: `while` (a bool Expression) and `modify: { field, amount }` (required). `field` must be a field the card's card type declares, of type int (an instance state is not a field); `amount` is an unquoted integer. The modifier is meant to add `amount` to that field of the carrying card while the condition holds.
- **`self`** is the one name an Expression or an effect of a Card Effect may use as a ref: the carrying card, of the card's own card type. `{ ref: self }` is a card and `{ ref: self.power }` is a value (a field or an instance state of the card's type). Any other name is a dangling reference to a param.
- An element has the keys of one shape only: `on` or `if` or `do` together with `while` or `modify` is a mixed shape, and an element with neither `on` nor `while` has no shape.

### Validation

The loader reads the syntax and rejects an unknown key of the envelope or of `modify`, and an `amount` that is not an unquoted integer. The engine checks the rest, the first error winning, in this order: the list against the card type's `effects` flag, then each element in order: its shape, then for a triggered one its `on`, `if` and `do` (and `end_turn` in `do`), for a continuous one its `while` and `modify`.

A well-formed triggered Card Effect is kept by its card definition and runs (ADR-0023). **Continuous Card Effects do not execute yet:** a well-formed one is rejected as an unsupported continuous Card Effect at `cards[<id>].effects[<i>]`; a later ticket of Phase C lifts it (ADR-0022).

## The `draw` effect

`draw: { player: controller, from: <zone>, to: <zone>, count: <n> }` is an Action Effect that exists in a Card Effect's `do` (ADR-0023). For `player`, the **Controller** (the owner of `self`, the carrying card; the only selector of an effect), it moves `count` cards one at a time from the Top of `from` onto the Top of `to`, exactly as the Start phase's `draw` step does. If `from` is empty, the effect records a `DrawFromEmptyZone` Event and stops: that Event is what `draw_from_empty` win conditions read, so a trigger can make a Player lose by `deck_out`. `from` must be an ordered zone, `count` an unquoted positive integer, and both zones must be declared. `controller` needs a `self`, so a ruleset action's `effects` rejects it (`ControllerOutsideCardEffect`).

## Triggered Card Effects

A Card Effect acts only on cards in the **Active Zone**, `effects.active_zone`, which is `field` here; a ruleset without the `effects` block has no Active Zone and none of its triggers is ever eligible, `enter` included. With the block, the one exception to the Active Zone is `enter`.

- **`enter: <zone>`** is woken by a `CardMoved` Event that puts the card into that zone, whichever zone it is and whether or not it is the Active Zone: `enter: field` fires when the card is played, `enter: trash` when it dies in combat. A `CardCreated` Event never wakes it: placing the cards of the decks at Game creation and creating cards fire nothing, and no trigger resolves during Game creation.
- **`turn_start: {}`** is woken by the `TurnBegan` Event of the card's controller, and only if the card is in the Active Zone when that Event is scanned. It fires on its controller's Turn only, in the transition that begins it (the Start phase automation, or the transition of the Action that passed the Turn).
- **Resolution.** After the Action's costs and effects, the Events of the transition are scanned in order from the first, costs included. An Event that wakes a trigger enqueues it; the queue is first in first out. Eligibility is decided when the Event is scanned: an enqueued trigger fires even if its card moved since. When a trigger fires, its `if` is read (on the state as it is then, `self` being the card) and, if false, nothing happens; otherwise its `do` runs to completion. The Events it emits are appended to the same Event Trace and scanned in turn, so a cascade queues behind the triggers already waiting. When no Event is left to scan and the queue is empty, resolution ends.
- **Order.** One Event waking several triggers enqueues them: the cards of the Active Player first, then of the other Players in seat order; within a Player in `state.cards()` order; within a card in the order of its `effects`. (With `enter` and `turn_start`, one Event only wakes the cards of one Player, so the order across Players cannot be observed yet.)
- **The Trigger Cap is 100.** A transition resolves at most 100 triggers. Every trigger that leaves the queue counts, one whose `if` is false included. The 101st rejects the whole transition with `TriggerCapExceeded { cap: 100 }`: no Event is returned and the GameState stays as it was. A mutually-triggering pair reaches the cap.
- **Win conditions** are checked once, after all triggers, on the final state and the whole Event Trace (`GameEnded` is the last Event). A trigger can cause a `deck_out` loss through `draw`; `leader_dead` is not reachable from a trigger that only reads and changes `self`.
- A trigger asks for no decision: the Game Record is unchanged (ADR-0019).

### Catalog cards

One creature per family, in `effects-catalog.yaml`; none is in a shipped deck.

| Card | Effect | Shows |
|---|---|---|
| `lamp_keeper` | `enter: field` → draw 1 | the draw on play, the trigger's Events after the play's |
| `dawn_scout` | `turn_start` → draw 1 | firing on its controller's Turn only, only in the Field |
| `grave_jackal` | `enter: trash` → draw 1 | the `enter` exception to the Active Zone: it fires on dying in combat |
| `eager_recruit` | `enter: field`, `if` `self.recently_played` → draw 1 | an `if` read at fire time: true when played, false for any other arrival in the Field |
| `restless_shade` | `enter: field` → move self to trash; `enter: trash` → move self to field | a cascade that exceeds the Trigger Cap: playing it is rejected |

## Glossary: Effects Ruleset

| Term | Definition |
|---|---|
| **Card Effect** | An ability a card definition carries, in the `effects` list of a card of a type with `effects: true`. Triggered or continuous. |
| **Triggered Card Effect** | `on`, optional `if` and `do`: when an Event matches the Trigger Matcher and the condition holds, the Action Effects run. |
| **Continuous Card Effect** | `while` and `modify`: while the condition holds, the Field Modifier applies to the carrying card. |
| **Trigger Matcher** | What a triggered Card Effect matches among Events: `enter` (a `CardMoved` Event puts the card into a zone) or `turn_start` (the controller's Turn began). |
| **Active Zone** | `effects.active_zone` (`field`): the zone where Card Effects act. `enter` is the exception. Without the block no trigger is eligible. |
| **Trigger Cap** | The most trigger resolutions in one transition: 100. A false `if` counts. One more rejects the transition (`TriggerCapExceeded`). |
| **Controller** | The owner of `self`, the carrying card: the Player a `draw` in a Card Effect draws for. |
| **Draw effect** | `draw: { player, from, to, count }`: draws like the `draw` step; an empty `from` records `DrawFromEmptyZone`. |
| **Field Modifier** | `modify: { field, amount }`: an additive int on a field of the carrying card's definition. |
| **Self** | `self`, the carrying card, the only name in a Card Effect's scope. |
