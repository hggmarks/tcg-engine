# Authoring guide

How to write a ruleset, its card catalog and a deck list. This is a how-to; [formats.md](formats.md) is the reference for every key, and `small-ruleset/` is the full example (rules in `small-ruleset.md`, encoding in `small-ruleset.yaml`).

## The three files and the load order

- The **ruleset** (`*.yaml`) declares zones, card types, deck inputs, setup, turn, actions and win conditions.
- The **card catalog** lists the cards. It names the ruleset it is written for.
- A **deck list** says which cards fill each deck input of one Player.

They load in this order, each one checked against the one before: `load_ruleset_definition(ruleset_yaml)`, then `load_card_catalog(&definition, catalog_yaml)`, then `load_deck_list(&definition, &catalog, deck_yaml)` once per Player. `DeclarativeRuleset::new(definition, catalog)` then offers `create_game(&decks, seed)`, with one deck list per Player in seat order.

## 1. The ruleset

Start with the structure: `meta`, `parameters`, `zones`, `card_types` and `inputs`. Then add `setup`, `actions`, `turn` and `win_conditions`. The loader checks them in that order, so an error in an early section hides later ones.

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
      actions: [pass_turn]
    - id: end
      automatic: []

actions:
  - id: pass_turn
    effects: [ { end_turn: {} } ]

win_conditions:
  - id: deck_out
    when: { draw_from_empty: deck }
    loser: owner
```

- Every zone says who sees it (`hidden`, `owner` or `public`) and whether its order matters. `shuffle` and `draw` need an `ordered` zone.
- `inputs` say what each Player brings. Each one fills the zone it names when the Game is created, before `setup` runs.
- The turn has exactly one phase with `actions`: the Game waits there for the Active Player, and `end_turn` is the only way out. `pass_turn` is a plain action whose effect is `end_turn`.
- `loser: owner` is the only value; `draw_from_empty` makes a Player lose when they draw from an empty zone.

## 2. The card catalog

Each card gives exactly the `fields` of its card type, in any order. Its `meta` names the ruleset's `id` and `version`.

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

Syntax errors carry a path and a line and column instead. Fix the first error and load again: validation stops at the first one.

## YAML pitfalls

The files are a restricted subset of YAML (ADR-0004).

- No anchors, aliases, tags, merge keys, duplicate keys or several documents.
- No implicit typing: `yes` is not a bool and `~` is not a text. Numbers and bools in the keys of the format (`size`, `count`, `ordered`, `effects`) are unquoted; a quoted `"3"` is a text and is rejected.
- Ids and names match `[a-z][a-z0-9_]*`, and are not `true`, `false` or `null`.
- Unknown keys are rejected, so a typo fails instead of being ignored.
- Where order matters, the format uses a list, never a map.

## Not supported yet

- Card effects: a card type has `effects: false`, and a catalog entry cannot carry effects on a type that allows none; no effect primitive exists yet.
- `tap` costs take a ref (`{ ref: card.cost }`), not a literal.
- Expressions have `eq`, `gte`, `lte`, `and` and `is_card_type` only: no `not`, `or` or arithmetic.
- `owner` takes only `opponent`; the only step condition is `first_turn`; `position` is only `top`.
- The loader does not check that the action phase can be left, or that an action ends one turn at most (the gap recorded in ADR-0015 and ADR-0016).

See [small-ruleset/small-ruleset.yaml](small-ruleset/small-ruleset.yaml) for a complete ruleset with creatures, mana, combat and two win conditions.
