# File formats

The formats of the ruleset, card catalog and deck list files. All three are YAML in the restricted subset of ADR-0004: no anchors, aliases, tags, merge keys, duplicate keys or multiple documents, and no implicit typing (`yes` is not a bool, `~` is not a text). Numbers and bools in the keys of the format (`version`, `players`, `size`, `max_copies`, `count`, `ordered`, `effects` and so on) must be unquoted YAML scalars: a quoted `"3"` or `"true"` is a text and is rejected, as is `yes`. Unknown keys are rejected. Anything where order matters is a list. The decisions are in ADR-0008 and ADR-0009.

To write your own files step by step, see [authoring-guide.md](authoring-guide.md).

## Identifiers

Every declared id and name matches `[a-z][a-z0-9_]*` and is not `true`, `false` or `null`. This holds whether the YAML scalar is quoted or not, so `3`, `"3"`, `Deck`, `a]b` and the empty id are all rejected. A reference that breaks the grammar fails as a reference to something that does not exist, except the text of a `ref`, which fails as an invalid reference (ADR-0015).

## Ruleset structure

See `small-ruleset/small-ruleset.yaml`. The structural sections are `meta`, `parameters`, `zones`, `card_types` and `inputs`. `setup`, `turn`, `actions`, `win_conditions` and `effects` are loaded (see below).

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

- `turn` is optional; absent means the Game has no Turns (creation stops after setup and no Action is legal; no phase lists an action then, so the ruleset declares none, `UnlistedAction` otherwise, ADR-0027). `actions` is optional; absent means none. The order of a phase's `actions` list is the order its Legal Actions are listed in (ADR-0018).
- `turn.phases` is a list of phases, each `{ id, automatic }` or `{ id, actions }`: exactly one of the two keys. `automatic` is a list of steps run in order when the phase begins; `actions` is a list of declared action ids the Active Player may take there. A turn has exactly one phase with `actions`: the Game waits in it, and `end_turn` is the only way out. Validation checks that the Turn can be left and ends at most once (ADR-0027): the phase with `actions` lists at least one action that ends the Turn (its effects end with a top-level `end_turn`, `NoTurnEndingAction` at the phase), every declared action is listed by a phase or by the `actions` of a window (`UnlistedAction` at the action; `opened_by` lists nothing, ADR-0037), and `end_turn` is only the last top-level effect of an action (`EndTurnNotLast` at the first one that another effect follows, including `[end_turn, end_turn]`) and never inside an `if` (`EndTurnInIfEffect`). When several of these hold, the first error wins: an action's placement errors come after its effects are checked, the first misplaced `end_turn` in document order (depth first, `then` before `else`); `NoTurnEndingAction` comes before `InvalidActionPhaseCount`, and `UnlistedAction` after the phases. When the Game is created the first Turn begins and runs to that phase.
- A step is a single-key map naming a primitive, or the `if` form. `player` is `each` or `active` (the Active Player). Steps run for the selected Players in seat order, the zones in the listed order and the cards bottom first.
  - `untap: { player, zones }` sets `tapped` to false on the cards of the listed zones.
  - `set_state: { player, zone, state, value }` sets the instance state `state` to `value` (a scalar of the state's type) on the cards of `zone`.
  - `untap` and `set_state` change only the cards whose card type declares the state, and only when the value actually changes.
  - `draw: { player, from, to, count }` moves up to `count` cards, one at a time, from the top of `from` onto the top of `to`. If `from` runs out, nothing more is drawn for that Player and one draw-from-empty fact is recorded per Player, not per missing card.
  - `move: { player, from, to, count, position }` is the same without the fact: it moves the cards there are. `position` is `top` (the top of `from`).
  - `if: <condition>`, `then: [steps]` and an optional `else: [steps]` are sibling keys of one list element. The only condition is `first_turn: {}`, true on the Game's first Turn (Turn Number 1).
  - `from` of a `draw` and of a `move` must be an ordered zone (ADR-0013); `count` is an unquoted integer of at least 1.
- `windows` is an optional top-level list of Response Windows (ADR-0037), each `{ id, opened_by, actions, abilities? }` with both lists of declared action ids (either may be empty) and an optional unquoted bool `abilities`, default `false` *(Amended by ADR-0038)*. An action in `opened_by`, taken outside a window, pays its cost, is pushed onto the Stack and opens the window; while the window is open the Player with Priority may `Pass`, take one of its `actions`, which is pushed too, or, if the window has `abilities: true`, use an activated ability of a card of theirs in the Active Zone *(Amended by ADR-0038)*, and a Pass resolves the top item, last in first out. The `actions` need not be listed by a phase, and their order is the order the Legal Actions are listed in, after `Pass`. Windows need exactly 2 Players. Errors are addressed by window id (`windows[attack_response].opened_by[0]`): a dangling id (`DanglingReference` to an action), an action that opens two windows (`DuplicateWindowOpener`, also for a repeat inside one window), a repeated window id (`DuplicateId`, addressed by position) or a repeated entry of one window's `actions` (`DuplicateId` at `windows[<id>].actions[i]`), `WindowsNeedTwoPlayers` at `windows`, `AbilitiesWithoutActiveZone` at `windows[<id>].abilities` (a window with `abilities: true` in a ruleset without `effects.active_zone`, ADR-0038) and `AbilitiesActiveZoneNotPublic { zone }` at the same path (the Active Zone is not `public`: the Stack is public); and for each pushed action (one a window lists or opens): `PushedCardNotPublic { param, zone }` when a card param is not in a `public` zone once the costs are paid (the zone checked is the `to` of the param's last `move` cost, reported at the param, else the zone of each alternative, reported at the alternative or its `one_of[i]`), and `EndTurnInPushedAction` at the `end_turn`. A Choice in a pushed action's effects is `ChoiceOutsideTriggeredEffect`, as in any action.
- Each action is `{ id, params?, cost?, effects }`; `params` and `cost` are optional lists (absent means none).
  - A param is `{ name, zone, card_type, owner?, where? }`: a card instance of that type in that zone of its owner, who is the acting Player (`owner` absent) or any other Player (`owner: opponent`, the only value), and for which the `where` expression holds. `{ name, one_of: [alternative, ...] }` takes one or more alternatives `{ zone, card_type, owner?, where? }` instead (`card_type` is required in each) and cannot also have `zone`, `card_type`, `owner` or `where`; a param needs one of the two forms. The first alternative the card fits binds it. A `where` that overflows a derived read (a Field Modifier sum past an int) makes its alternative not fit when the param has several alternatives, and the next one is tried; if none fits the Rejection is the usual "fits no alternative". With one alternative the `DerivedValueOverflow` is the Rejection (ADR-0032). Names are identifiers, unique within the action. An Action supplies one argument per param, in param order. A `where` is checked when the ruleset loads and evaluated before the costs; it reads its own param (as that alternative's card type alone) and earlier params only.
  - A ref is `{ ref: <param> }` (the card) or `{ ref: <param>.<name> }` (a value of the card: a field of its card type, or an instance state; a name that is both is an error). Refs are checked when the ruleset loads: the param and the name must exist on every card type the param may take (the card types of its alternatives), as the same kind (field or state) and type, and the value must have the type of its slot.
  - An expression is a scalar (`5`, `true`, `"text"`), a value ref, or a single-key map with an operator: `eq: [a, b]` (two operands of one type), `gte: [a, b]` and `lte: [a, b]` (two ints), `and: [e, ...]` (bools; an empty `and` holds) and `is_card_type: [{ ref: <param> }, <card type id>]`. Every operator gives a bool, and `where`, `if` and `when` need one. A card ref is not a value, so only `is_card_type` takes one. An operator with the wrong number of operands is a syntax error; an unknown operator is an unknown key. Errors name the operator key and operand position (`actions[attack].params[attacker].where.and[0].eq[1]`).
  - `cost` is a list of single-key maps. `tap: { zone, count }` taps `count` untapped cards of the acting Player's `zone`: the first ones in list order, bottom first, among the cards whose card type declares `tapped` and whose value is false. `count` is a ref to an int value (a literal is not accepted yet); 0 taps nothing, a negative value refuses the Action, and too few untapped cards refuses it too. Some card type must declare `tapped` as a bool, and no card type may declare it as another type (a mismatch is rejected when the ruleset loads). The costs of an activated ability (ADR-0038) are the same steps, with `self` as the only name (`tap: { card: { ref: self } }`, `count: { ref: self.cost }`); the last `move: { card: { ref: self }, to }` of the cost must end in a `public` zone (`PushedCardNotPublic { param: self, zone }` at `cost[k].move.to`), as the card stays on the public Stack. `tap: { card: { ref: <param> } }` taps one card instead: it must be untapped (else the Action is refused), and every card type the param may take must declare a bool `tapped` (`TapCardWithoutTappedState` at `cost[i].tap.card`); `zone` and `count` cannot be mixed with `card`. `move: { card: { ref: <param> }, to: <zone> }` moves the card like the `move` effect, before the later costs and, in a pushed action, before the item is pushed: a param moved by a cost is bound in the zone of its last `move`, which a pushed action needs `public` (ADR-0037).
  - `effects` is a list of single-key maps: `end_turn: {}` (the phases after the current one run, then the next Player's Turn begins, in the same transition; only as the last top-level effect of the action, never inside an `if`, ADR-0027), `move: { card: <ref>, to }` (the card goes from the zone it is in onto the top of `to`, a zone of its owner), `set_state: { card: <ref>, state, value }` (the state must be declared with a value of its type on every card type the param may take; `value` is a literal scalar, not a ref; nothing happens if the value is already that), `add_state: { card: <ref>, state, amount }` (the state must be an int on every card type the param may take; `amount` is an unquoted integer literal; the sum is checked, and an overflow in either direction refuses the Action; nothing happens if the sum is the current value) and `draw: { player: controller, from, to, count }` (only in a Card Effect, see Card catalog: `controller` is the owner of `self`, so in an action it is `ControllerOutsideCardEffect`; `from` is an ordered zone, `count` an unquoted positive integer; it moves the Top cards of `from` onto the Top of `to` one at a time, and an empty `from` records a draw-from-empty fact for the win conditions, ADR-0014). A `move` keeps the instance state of the card (a played creature stays `recently_played` if it is moved again).
  - An `if` effect is `if: <expression>` with `then: [effects]` and an optional `else: [effects]` as sibling keys of one list element, like an `if` step. Its condition must be a bool. When the condition is exactly `is_card_type: [{ ref: <param> }, T]` it narrows what the param may be: it starts from the card types the param may still have at that point, so narrowing nests. In `then` it keeps those that are T (none is a load error, `ImpossibleCardType`, at `...if.is_card_type`, ADR-0032; for example `target` narrowed to `[leader]` in an `else`, then an inner `is_card_type: [{ ref: target }, creature]`), in `else` the other card types (all of them if none would be left); that is why `add_state` of `current_life` is valid on a param that may be a creature or a Leader in the `else` only. Errors are addressed like steps: `actions[attack].effects[1].if`, `.effects[1].then[0].then[0].move.to`.
  - The decisions are in ADR-0015 and ADR-0016.
- Turn validation follows `actions`: phases are addressed by id in errors (`turn.phases[main].actions[0]`) and steps by position, with `then` and `else` as keys (`turn.phases[start].automatic[2].else[0].draw.from`). The decisions are in ADR-0012 and ADR-0027.

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
- `effects` is an optional top-level block `{ active_zone: <zone> }`: the zone where Card Effects act (ADR-0024). `active_zone` is required in it and must be a declared zone (`DanglingReference` at `effects.active_zone`); any other key is an unknown key. Without the block no Card Effect is ever active. It is validated after `win_conditions`.

## Effects block

```yaml
effects:
  active_zone: field
```

- `effects` is an optional top-level block with one required key, `active_zone`: the zone where Card Effects act. It must be a declared zone (`DanglingReference` at `effects.active_zone`); an unknown key in the block is a syntax error. With the block, a `turn_start` triggered Card Effect fires only while its card is in that zone (the Active Zone), and an `enter` one fires on arrival in the zone it names, whichever zone that is. A continuous Card Effect modifies a field only while its carrier is in that zone; a Modifier Scope names the zone of the cards it reaches, which may be any declared zone. A ruleset without the block has no Active Zone: no triggered Card Effect is then ever eligible, `enter` included, and no Field Modifier is active. The block is validated last, after `win_conditions`. The decisions are in ADR-0023, ADR-0024 and ADR-0025.

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
- `effects` is an optional list of Card Effects, the **Card Effect envelope** (ADR-0022). It must be empty or absent on a card type with `effects: false` (the Effects Ruleset sets `effects: true` on `creature`). An element has one of three shapes *(Amended by ADR-0038: the activated shape)*:

  ```yaml
  effects:
    - on: { enter: field }     # triggered: a matcher, `enter: <zone>` or `turn_start: {}`
      if: { ... }              # optional: a bool expression
      do: [ <action effects> ] # at least one; `end_turn` is not allowed in it; `choose_one` and `choose_card` are allowed (below)
    - activated:               # activated (ADR-0038): used as a response, in a window with `abilities: true`
        cost: [ <cost steps> ]   # optional: the action cost steps, with only `self` in scope
        do: [ <action effects> ] # at least one; `end_turn`, `choose_one` and `choose_card` are not allowed in it
    - while: { ... }           # continuous: a bool expression; required, `true` for an always-on effect
      scope: { zone: field, card_type: creature, owner: opponent }   # optional: the cards it reaches; owner and include_self are optional keys
      modify: { field: power, amount: 1 }   # an int field of the card's type (with a scope, of the scope's card type); an unquoted integer
  ```

  - Keys of two shapes in one element are an error (`scope` is a continuous key; `activated` is its own shape), and so is an element with none of `on`, `while` and `activated`: a `scope` needs a `while`. An unknown key of the envelope, of `modify` or of `scope` is a syntax error, and so are an `amount` that is not an unquoted integer and an `include_self` that is not an unquoted `true` or `false`. An unknown matcher key is not: the loader keeps it and the engine rejects it.
  - `if`, `while` and `do` are the expressions and action effects of the ruleset format (see Turn and actions). Their refs start with `self`, the carrying card: `{ ref: self }` is the card, `{ ref: self.power }` a field or an instance state of the card's own card type. No other name is in scope, except the names that a `choose_card` binds, from the effect after it on (below).
  - `scope` is a **Modifier Scope**: `zone` (a declared zone) and `card_type` (a declared card type) are required, `owner` is absent (the carrying card's owner) or `opponent` (any other Player), and `include_self` is absent or an unquoted `true` or `false` (absent is false; it matters only when `owner` is absent: the carrier is never in the opponent relation to itself, so `owner: opponent` with `include_self: true` is accepted and changes nothing). The `owner` key has two bases: a param's is relative to the acting Player (ADR-0016), a Modifier Scope's is relative to the carrier's owner (ADR-0025). A card is reached when it is in `zone` of its owner, of type `card_type`, owned relative to the carrier's owner as `owner` says, and not the carrier unless `include_self`; the match reads no field. `modify.field` is then an int field of the scope's `card_type`, which needs no `effects: true`. Errors are at `scope.zone`, `scope.card_type` and `scope.owner`, in that order, after `while` and before `modify`.
  - `choose_one: { decider, options }` is an action effect of a triggered `do` only (ADR-0033; not an activated `do`, ADR-0038): `decider` is `controller` or `opponent` (`UnknownDecider` at `choose_one.decider`; `opponent` is defined for exactly 2 Players, else `OpponentDeciderNeedsTwoPlayers` at `choose_one.decider`), `options` is a list of at least 2 lists of action effects (`TooFewOptions` at `choose_one.options`), each of which may be empty, and the effects of option `k` are at `choose_one.options[k][i]`. It may be nested in an `if` or in an option. In an action's `effects` it is `ChoiceOutsideTriggeredEffect` at `<effect>.choose_one`; as a setup step, an automatic step, or a key of a continuous Card Effect it is an unknown key.
  - `choose_card: { name, decider, zone, card_type, owner?, where? }`, or `{ name, decider, one_of: [ { zone, card_type, owner?, where? }, ... ] }`, is an action effect of a triggered `do` only (ADR-0034; not an activated `do`, ADR-0038): the keys of a param (ADR-0015, ADR-0016) plus `decider`, and an unknown key is a syntax error, like the two forms mixed or neither given. `name` is bound for the effects after it in the same list and in everything nested in them, and is not in scope after the enclosing `if` or option ends; it is unique in scope, `self` included. `owner` is relative to the controller, the owner of `self` (absent: the controller's zone; `opponent`: the controller's opponent's), whoever decides. The decider must see every alternative's zone: `public`, or `owner` with the decider as its owner (`owner` absent and `decider: controller`, or `owner: opponent` and `decider: opponent`); otherwise `DeciderCannotSeeZone` at `choose_card.zone` or `choose_card.one_of[i].zone`, and a `hidden` zone is never seen. The checks run in this order: `ChoiceOutsideTriggeredEffect` at `choose_card` in an action's `effects`, `InvalidId` then `DuplicateChoiceName` at `choose_card.name`, `UnknownDecider` at `choose_card.decider`, the param as for an action, then the visibility, and last, after the whole `do` is otherwise valid, `OpponentDeciderNeedsTwoPlayers` at `choose_card.decider` for `decider: opponent` unless the ruleset has exactly 2 Players. With no candidate nothing is asked and the rest of the `do` is skipped; with one it is still asked.
  - Errors are addressed like any catalog error, by card id: `cards[goblin].effects[0].do`, and for an error of an expression or an action effect at its own path under it, such as `cards[goblin].effects[0].if.and[0].eq[1]`.
  - A well-formed triggered Card Effect is kept and runs (ADR-0023): `enter: <zone>` fires when a `CardMoved` Event puts the card into that zone (game creation and `CardCreated` never fire it), `turn_start: {}` when its controller's Turn begins while the card is in the Active Zone. A fired `do` runs to completion (or to a `choose_one` or `choose_card`, which pauses the Resolution until the Deciding Player answers, ADR-0033, ADR-0034, or to a `choose_card` with no candidate, which ends it) and its Events can wake further triggers, first in first out; a Resolution, across its pauses, may resolve at most 100 triggers, and one more rejects the whole transition. A continuous one is a derived read of a field (ADR-0024), active while its carrier is in the ruleset's `effects.active_zone`, over its own card or over the cards of its `scope` (ADR-0025); its `while` reads base values only. The decisions are in ADR-0022, ADR-0023, ADR-0024 and ADR-0025.

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

## Starter data for the Effects Ruleset

The Small Ruleset is frozen; the Effects Ruleset is derived from it (ADR-0022) and is frozen since Phase F (ADR-0037). `effects-ruleset/effects-ruleset.yaml` and `effects-catalog.yaml` are the Small files with `meta` renamed, `effects: true` on `creature` and the `effects` block with `active_zone: field` (the catalog is the Small catalog plus effect cards: six creatures that carry a triggered Card Effect, ADR-0023, two that carry a continuous one, `battle_veteran` and `weary_ox`, ADR-0024, and three auras, ADR-0025), and `decks/deck-a.yaml` and `deck-b.yaml` are the Effects Ruleset's own legal decks (1 Leader, 50 creatures, 10 mana) that list the triggered, continuous and aura cards (ADR-0026).

## Starter data for the Response Ruleset

The Effects Ruleset is frozen; the Response Ruleset is derived from it (ADR-0037). `response-ruleset/response-ruleset.yaml` and `response-catalog.yaml` are the Effects files with `meta` renamed, a `holding` zone, a `fast` bool field on `creature` (every catalog creature says it), `attack` with a `tap: { card }` cost, `fast_play`, and the `attack_response` window, which has `abilities: true` (ADR-0038); the catalog adds the fast creatures `swift_sparrow` and `flash_fox` and the activated `skittish_hare`, and `decks/deck-a.yaml` and `deck-b.yaml` are the Effects decks with copies swapped for them.
