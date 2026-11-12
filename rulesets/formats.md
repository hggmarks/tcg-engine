# File formats

The formats of the ruleset, card catalog and deck list files. All three are YAML in the restricted subset of ADR-0004: no anchors, aliases, tags, merge keys, duplicate keys or multiple documents, and no implicit typing (`yes` is not a bool, `~` is not a text). Numbers and bools in the keys of the format (`version`, `players`, `size`, `max_copies`, `count`, `ordered`, `effects` and so on) must be unquoted YAML scalars: a quoted `"3"` or `"true"` is a text and is rejected, as is `yes`. Unknown keys are rejected. Anything where order matters is a list. The decisions are in ADR-0008 and ADR-0009.

## Identifiers

Every declared id and name matches `[a-z][a-z0-9_]*` and is not `true`, `false` or `null`. This holds whether the YAML scalar is quoted or not, so `3`, `"3"`, `Deck`, `a]b` and the empty id are all rejected. A reference that breaks the grammar fails as a reference to something that does not exist, except the text of a `ref`, which fails as an invalid reference (ADR-0015).

## Ruleset structure

See `small-ruleset/small-ruleset.yaml`. The structural sections are `meta`, `parameters`, `zones`, `card_types` and `inputs`. `setup`, `turn`, `actions` and `win_conditions` are loaded (see below).

Each entry of `inputs` is `{ id, card_types, size, max_copies?, zone }`, with these constraints:

- `size` is at least 1;
- `max_copies`, when present, is at least 1 and at most `size`; omit the key for "no limit" (`max_copies: ~` is an error);
- `card_types` is a non-empty list of declared card types without repeats.

## Setup

```yaml
setup:
  - shuffle: { player: each, zone: deck }
  - draw: { player: each, from: deck, to: hand, count: 5 }
```

- `setup` is an optional list of steps; absent means no steps. Each step is a single-key map naming a primitive: `shuffle` or `draw`. An unknown primitive is an unknown key.
- `player` is the only key that chooses Players. Its values are `each` (every Player in seat order, a step running fully for one Player before the next) and `active` (the Active Player, which during setup is the starting Player).
- `shuffle: { player, zone }` shuffles the zone of each selected Player with the Game's seeded random state. `zone` must be an ordered zone.
- `draw: { player, from, to, count }` moves `count` cards (an unquoted integer of at least 1), one at a time, from the top of `from` onto the top of `to`. The top of a zone is the end of its list. Drawing from an empty zone fails Game creation. `from` must be an ordered zone.
- Zones must be declared zones; the source of a shuffle or a draw must also be `ordered: true` (ADR-0013). Setup runs after each Player's cards are placed on the zone of their deck input, and is checked after `inputs`, steps in order. The decisions are in ADR-0011.

A card type's `fields` give the schema of a card: `{ name, type }` with `type` one of `int`, `bool`, `text`. Int values have no minimum or maximum.

## Turn and actions

```yaml
turn:
  phases:
    - id: start
      automatic:
        - untap: { player: active, zones: [mana_zone, field] }
        - set_state: { player: active, zone: field, state: recently_played, value: false }
        - if: { first_turn: {} }
          then: []
          else:
            - draw: { player: active, from: deck, to: hand, count: 1 }
        - move: { player: active, from: mana_deck, to: mana_zone, count: 2, position: top }
    - id: main
      actions: [pass_turn]
    - id: end
      automatic: []

actions:
  - id: pass_turn
    effects: [ { end_turn: {} } ]
```

- `turn` is optional; absent means the Game has no Turns (creation stops after setup and no Action is legal). `actions` is optional; absent means none. The order of a phase's `actions` list is the order its Legal Actions are listed in (ADR-0018).
- `turn.phases` is a list of phases, each `{ id, automatic }` or `{ id, actions }`: exactly one of the two keys. `automatic` is a list of steps run in order when the phase begins; `actions` is a list of declared action ids the Active Player may take there. A turn has exactly one phase with `actions`: the Game waits in it, and `end_turn` is the only way out. Validation does not yet check that the action phase can be left (it may list no action, or none with `end_turn`, and the Game then waits there forever), nor that an action ends at most one Turn (`[end_turn, end_turn]` ends two); an action no phase lists loads and is always rejected. These, and effects listed after an `end_turn` (which load and run in the next Player's Turn), are an open gap recorded in ADR-0015 and ADR-0016. When the Game is created the first Turn begins and runs to that phase.
- A step is a single-key map naming a primitive, or the `if` form. `player` is `each` or `active` (the Active Player). Steps run for the selected Players in seat order, the zones in the listed order and the cards bottom first.
  - `untap: { player, zones }` sets `tapped` to false on the cards of the listed zones.
  - `set_state: { player, zone, state, value }` sets the instance state `state` to `value` (a scalar of the state's type) on the cards of `zone`.
  - `untap` and `set_state` change only the cards whose card type declares the state, and only when the value actually changes.
  - `draw: { player, from, to, count }` moves up to `count` cards, one at a time, from the top of `from` onto the top of `to`. If `from` runs out, nothing more is drawn for that Player and one draw-from-empty fact is recorded per Player, not per missing card.
  - `move: { player, from, to, count, position }` is the same without the fact: it moves the cards there are. `position` is `top` (the top of `from`).
  - `if: <condition>`, `then: [steps]` and an optional `else: [steps]` are sibling keys of one list element. The only condition is `first_turn: {}`, true on the Game's first Turn (Turn Number 1).
  - `from` of a `draw` and of a `move` must be an ordered zone (ADR-0013); `count` is an unquoted integer of at least 1.
- Each action is `{ id, params?, cost?, effects }`; `params` and `cost` are optional lists (absent means none).
  - A param is `{ name, zone, card_type, owner?, where? }`: a card instance of that type in that zone of its owner, who is the acting Player (`owner` absent) or any other Player (`owner: opponent`, the only value), and for which the `where` expression holds. `{ name, one_of: [alternative, ...] }` takes one or more alternatives `{ zone, card_type, owner?, where? }` instead (`card_type` is required in each) and cannot also have `zone`, `card_type`, `owner` or `where`; a param needs one of the two forms. The first alternative the card fits binds it. Names are identifiers, unique within the action. An Action supplies one argument per param, in param order. A `where` is checked when the ruleset loads and evaluated before the costs; it reads its own param (as that alternative's card type alone) and earlier params only.
  - A ref is `{ ref: <param> }` (the card) or `{ ref: <param>.<name> }` (a value of the card: a field of its card type, or an instance state; a name that is both is an error). Refs are checked when the ruleset loads: the param and the name must exist on every card type the param may take (the card types of its alternatives), as the same kind (field or state) and type, and the value must have the type of its slot.
  - An expression is a scalar (`5`, `true`, `"text"`), a value ref, or a single-key map with an operator: `eq: [a, b]` (two operands of one type), `gte: [a, b]` and `lte: [a, b]` (two ints), `and: [e, ...]` (bools; an empty `and` holds) and `is_card_type: [{ ref: <param> }, <card type id>]`. Every operator gives a bool, and `where`, `if` and `when` need one. A card ref is not a value, so only `is_card_type` takes one. An operator with the wrong number of operands is a syntax error; an unknown operator is an unknown key. Errors name the operator key and operand position (`actions[attack].params[attacker].where.and[0].eq[1]`).
  - `cost` is a list of single-key maps. `tap: { zone, count }` taps `count` untapped cards of the acting Player's `zone`: the first ones in list order, bottom first, among the cards whose card type declares `tapped` and whose value is false. `count` is a ref to an int value (a literal is not accepted yet); 0 taps nothing, a negative value refuses the Action, and too few untapped cards refuses it too. Some card type must declare `tapped` as a bool, and no card type may declare it as another type (a mismatch is rejected when the ruleset loads).
  - `effects` is a list of single-key maps: `end_turn: {}` (the phases after the current one run, then the next Player's Turn begins, in the same transition), `move: { card: <ref>, to }` (the card goes from the zone it is in onto the top of `to`, a zone of its owner), `set_state: { card: <ref>, state, value }` (the state must be declared with a value of its type on every card type the param may take; `value` is a literal scalar, not a ref; nothing happens if the value is already that) and `add_state: { card: <ref>, state, amount }` (the state must be an int on every card type the param may take; `amount` is an unquoted integer literal; the sum is checked, and an overflow in either direction refuses the Action; nothing happens if the sum is the current value). A `move` keeps the instance state of the card (a played creature stays `recently_played` if it is moved again).
  - An `if` effect is `if: <expression>` with `then: [effects]` and an optional `else: [effects]` as sibling keys of one list element, like an `if` step. Its condition must be a bool. When the condition is exactly `is_card_type: [{ ref: <param> }, T]` it narrows what the param may be: to T in `then`, to the other card types in `else` (all of them if none would be left); that is why `add_state` of `current_life` is valid on a param that may be a creature or a Leader in the `else` only. Errors are addressed like steps: `actions[attack].effects[1].if`, `.effects[1].then[0].then[0].move.to`.
  - The decisions are in ADR-0015 and ADR-0016.
- Turn validation follows `actions`: phases are addressed by id in errors (`turn.phases[main].actions[0]`) and steps by position, with `then` and `else` as keys (`turn.phases[start].automatic[2].else[0].draw.from`). The decisions are in ADR-0012.

## Win conditions

```yaml
win_conditions:
  - id: deck_out
    when: { draw_from_empty: deck }
    loser: owner
```

- `win_conditions` is an optional list; absent means the Game never ends. Each entry is `{ id, when, loser }`, with `id` an identifier unique among the win conditions.
- `when` is a single-key map. `draw_from_empty: <zone>` holds for a Player when a draw of that Player in the transition found the zone empty; the zone must be declared. An expression (`lte`, `gte`, `eq`, `and`, `is_card_type`, as `leader_dead` of the Small Ruleset is written) is about one card type, its **Subject**: the first name of every ref in it is a card type id (`leader.current_life`) and it must read exactly one. It holds for a Player when some card instance of the Subject owned by that Player, in any zone, makes it true in the state the transition produced. A scalar `when` is read as an expression too: `true` or `false` is rejected as `InvalidSubjectCount` (it reads no card type), and an int or a text as an expression type mismatch. Any other key is an unknown key.
- `loser` is `owner`: the Player the condition holds for (for an expression, the owner of the card instance that makes it true). No other value exists.
- Win conditions are evaluated once after every accepted action and at the end of Game creation, after everything the transition ran: `draw_from_empty` holds by the Events of that transition, and an expression reads the resulting state (so a Leader created at 0 life or below loses at once). For each Player in seat order the reason is the first condition, in declaration order, that holds for them; every Player who has one loses, the others win, and if all lose the Game is drawn. The Game then ends: no Action is accepted. Errors are addressed by id: `win_conditions[deck_out].when`, `win_conditions[deck_out].when.draw_from_empty`, `win_conditions[deck_out].loser`, and inside an expression `win_conditions[leader_dead].when.lte[0]`. Win conditions are validated last, after `turn`. The decisions are in ADR-0014 and ADR-0016.

## Card catalog

```yaml
meta: { ruleset: small, ruleset_version: 1 }

cards:
  - id: goblin_scout
    type: creature
    fields: { name: "Goblin Scout", cost: 1, power: 1 }
  - { id: mana, type: mana, fields: {} }
```

- `meta.ruleset` and `meta.ruleset_version` name the ruleset the catalog is written for; they must equal the loaded ruleset's `meta`.
- Each card has `id` (unique in the catalog), `type` (a card type of the ruleset), `fields` (required, a map; `{}` for a type without fields) and optionally `effects`.
- `fields` must give exactly the fields of the card type, each once, in any order. The loader keeps the file order; the model keeps the schema order. A field's kind comes from the scalar: `5` is an int, `"5"` is a text, `true` is a bool, so a quoted number is a type mismatch for an int field. Floating point numbers, nulls and nested values are rejected.
- `effects` is an optional list of single-key maps, `{ <primitive>: <arguments> }`. It must be empty or absent on a card type with `effects: false`. No primitive exists yet, so any entry on a type that allows effects is unsupported.

## Deck list

```yaml
inputs:
  - input: leader
    cards:
      - { card: captain_ardel, count: 1 }
  - input: main_deck
    cards:
      - { card: goblin_scout, count: 4 }
```

- Each entry of `inputs` provides one deck input of the ruleset by id; every input of the ruleset is provided exactly once, in any order.
- Each entry of `cards` is `{ card, count }`: a card of the catalog and a number of copies of at least 1. A card is listed once per input; copies are never summed across entries.
- Only card types allowed by the input are accepted, a card has at most `max_copies` copies in an input, and the counts of an input add up to exactly its `size`. The Leader input of the Small Ruleset has `size: 1`, so it takes exactly one Leader.
- The order of the `cards` of an input is kept: it is the order the deck has when its cards are placed, before any setup step such as a shuffle.

## Starter data for the Small Ruleset

- `small-ruleset/small-catalog.yaml`: 3 Leaders, 16 creatures and the mana card.
- `small-ruleset/decks/deck-a.yaml` and `deck-b.yaml`: two legal decks, each with 1 Leader, 50 creatures (at most 4 copies of each) and 10 mana cards.
