# Authoring guide

How to write a ruleset, its card catalog and a deck list. This is a how-to; [formats.md](formats.md) is the reference for every key, and `small-ruleset/` is the full example (rules in `small-ruleset.md`, encoding in `small-ruleset.yaml`).

## The three files and the load order

- The **ruleset** (`*.yaml`) declares zones, card types, Player state, deck inputs, setup, turn, actions and win conditions.
- The **card catalog** lists the cards. It names the ruleset it is written for.
- A **deck list** says which cards fill each deck input of one Player.

They load in this order, each one checked against the one before: `load_ruleset_definition(ruleset_yaml)`, then `load_card_catalog(&definition, catalog_yaml)`, then `load_deck_list(&definition, &catalog, deck_yaml)` once per Player. `DeclarativeRuleset::new(definition, catalog)` then offers `create_game(&decks, seed)`, with one deck list per Player in seat order.

## 1. The ruleset

Start with the structure: `meta`, `parameters`, `zones`, `card_types`, `player_state` (if Players hold values of their own: see below) and `inputs`. Then add `setup`, `actions`, `turn` and `win_conditions`. The loader first reads the whole file for syntax (unknown keys, forbidden YAML features, values of the wrong type). The engine then validates the meaning section by section in that order, so a semantic error in an early section hides one in a later section.

<!-- guide-example: ruleset -->
```yaml
meta: { id: mini, version: 1 }

parameters: { players: 2, starting_player: 0 }

zones:
  - { id: deck, visibility: hidden, ordered: true }
  - { id: hand, visibility: owner,  ordered: false }

card_types:
  - id: creature
    fields:
      - { name: name,  type: text }
      - { name: power, type: int }
    instance_state: []
    effects: false

player_state:
  - { id: life_points, type: int, init: 20 }

inputs:
  - { id: main_deck, card_types: [creature], size: 6, max_copies: 2, zone: deck }

setup:
  - shuffle: { player: each, zone: deck }
  - draw: { player: each, from: deck, to: hand, count: 2 }

turn:
  phases:
    - id: start
      automatic:
        - if: { first_turn: {} }
          then: []
          else:
            - draw: { player: active, from: deck, to: hand, count: 1 }
    - id: main
      actions: [smite, pass_turn]
    - id: end
      automatic: []

actions:
  - id: smite
    effects:
      - add_state: { player: opponent, state: life_points, amount: -3 }
  - id: pass_turn
    effects: [ { end_turn: {} } ]

win_conditions:
  - id: deck_out
    when: { draw_from_empty: deck }
    loser: owner
```

- `player_state` declares values every Player holds, each `{ id, type: int | bool, init }` (ADR-0041). The engine names none, so Life Points are the ruleset's: here `life_points` starts at 20 for each Player. An action changes it with `set_state` or `add_state` and a `player:` in place of `card:` (`smite` takes 3 from the opponent: a negative `amount` subtracts, and `each`, `active` and `opponent` are the selectors of an action; a Card Effect also has `controller`), and an expression reads it as `{ ref: player.life_points, player: opponent }`, for a `where` for example. Each change is the Event `PlayerStateChanged`, and `player` is a reserved id, so no param (an action's or a `choose_card`'s) or card type is named so. See `formats.md`, Player state.
- Every zone says who sees it (`hidden`, `owner` or `public`) and whether its order matters. The zone a `shuffle` shuffles and the `from` of a `draw` must be `ordered` (ADR-0013); the `to` of a `draw` need not be.
- `inputs` say what each Player brings. Each one fills the zone it names when the Game is created, before `setup` runs.
- The turn has exactly one phase with `actions`: the Game waits there for the Active Player (or, while a Response Window is open, for the Player with Priority, ADR-0037), and `end_turn` is the only way out. `pass_turn` is a plain action whose effect is `end_turn`. The phase must list at least one action that ends the Turn (its effects end with a top-level `end_turn`, and `end_turn` is allowed nowhere else in an action, never inside an `if`), and every action you declare must be listed by it, or by the `actions` of a Response Window (ADR-0027, ADR-0037).
- `loser: owner` is the only value; `draw_from_empty` makes a Player lose when a draw of theirs during a turn found the zone empty. A `draw` in `setup` from an empty zone is not a loss: it fails Game creation (ADR-0011, ADR-0014).

## 2. The card catalog

Each card gives exactly the `fields` of its card type, in any order. The `meta` of the catalog names the `id` and `version` of the ruleset it is written for, as `ruleset` and `ruleset_version`.

<!-- guide-example: catalog -->
```yaml
meta: { ruleset: mini, ruleset_version: 1 }

cards:
  - { id: sprout, type: creature, fields: { name: "Sprout", power: 1 } }
  - { id: bear,   type: creature, fields: { name: "Bear",   power: 3 } }
  - { id: eagle,  type: creature, fields: { name: "Eagle",  power: 2 } }
```

## 3. The deck list

One entry per deck input of the ruleset, each once. The counts of an input add up to its `size`, no card goes over `max_copies`, and the order of the cards is the order the deck is placed in.

<!-- guide-example: deck-list -->
```yaml
inputs:
  - input: main_deck
    cards:
      - { card: sprout, count: 2 }
      - { card: bear,   count: 2 }
      - { card: eagle,  count: 2 }
```

## When a file is wrong

An error names the place in the file as a Definition Path, with ids in brackets and positions as numbers. If the `main_deck` input of the ruleset above named a zone that does not exist, for example `zone: shelf`, loading fails with:

```
inputs[main_deck].zone: no zone has the id `shelf`
```

Syntax errors carry a path and, when known, a line and column instead. Loading stops at the first error: fix it and load again.

## YAML pitfalls

The files are a restricted subset of YAML (ADR-0004).

- No anchors, aliases, tags, merge keys, duplicate keys or several documents.
- No implicit typing: `yes` is not a bool and `~` is not a text. Numbers and bools in the keys of the format (`size`, `count`, `ordered`, `effects`) are unquoted; a quoted `"3"` is a text and is rejected.
- Ids and names match `[a-z][a-z0-9_]*`, and are not `true`, `false` or `null`.
- Unknown keys are rejected, so a typo fails instead of being ignored.
- Where order matters, the format uses a list, never a map.

## Not supported yet

- Card effects: `effects: true` on a card type loads and the envelope of a Card Effect is checked (`on`/`if`/`do`, `while`/`modify` with an optional `scope`, or `activated: { cost, do }` *(ADR-0038)*, see `formats.md`). A triggered one executes (ADR-0023) and a continuous one is a derived read over its carrier or its Modifier Scope (ADR-0024, ADR-0025); the ruleset names the zone where Card Effects act in its `effects` block. Nothing well-formed is rejected as unsupported, but the format has no more than that: the trigger matchers are `enter` and `turn_start`, a triggered `do` cannot `end_turn`, a triggered `do` may ask a Player with `choose_one: { decider: controller|opponent, options: [[effects], [effects], ...] }` (at least 2 options, any of them may be empty, which is how to write "may"; `decider: opponent` needs exactly 2 Players, `OpponentDeciderNeedsTwoPlayers` otherwise; the Resolution pauses until the Deciding Player answers, ADR-0033; it is a load error anywhere but a triggered `do`, an activated `do` included), or pick a card with `choose_card: { name, decider, zone, card_type, owner?, where? }` (or `one_of: [...]`; ADR-0034: the param keys plus `decider`; `owner` is relative to the controller; `name` is a Card Ref for the effects after it, unique in scope and gone after the enclosing `if` or option; the decider must see the zone: `public`, or `owner` visibility with the decider as its owner, else `DeciderCannotSeeZone`; with no candidate the rest of the `do` is skipped), and a modifier only adds an int to a card field while its carrier is in the Active Zone.
- Response Windows exist (`windows: [{ id, opened_by, actions, abilities? }]`, ADR-0037, see `formats.md` and the Response Ruleset), but only for 2 Players, with responses that are ruleset actions or, in a window with `abilities: true`, activated abilities (ADR-0038, a Card Effect `activated: { cost, do }`, only from the Active Zone, which must be `public`, as must the zone a cost that moves `self` ends in: the card sits on the public Stack): no triggers on the Stack, no Choice in a pushed action, and an action cannot end the Turn while pushed.
- The `enum` and `enum set` field types (ADR-0004); a field is `int`, `bool` or `text`.
- `tap` costs take a ref (`{ ref: card.cost }`), not a literal. `tap: { card: { ref } }` and `move: { card: { ref }, to }` are costs too (ADR-0037).
- Expressions have `eq`, `gte`, `lte`, `and` and `is_card_type` only: no `not`, `or` or arithmetic (the one exception is the `amount` of an `add_state`, which may be a ref to an int or `sub: [a, b]`: `amount: { sub: [0, { ref: card.power }] }`, ADR-0041).
- `owner` takes only `opponent`; the only step condition is `first_turn`; `position` is only `top`.

See [small-ruleset/small-ruleset.yaml](small-ruleset/small-ruleset.yaml) for a complete ruleset with creatures, mana, combat and two win conditions.
