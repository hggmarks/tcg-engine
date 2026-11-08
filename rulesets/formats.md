# File formats

The formats of the ruleset, card catalog and deck list files. All three are YAML in the restricted subset of ADR-0004: no anchors, aliases, tags, merge keys, duplicate keys or multiple documents, and no implicit typing (`yes` is not a bool, `~` is not a text). Numbers and bools in the keys of the format (`version`, `players`, `size`, `max_copies`, `count`, `ordered`, `effects` and so on) must be unquoted YAML scalars: a quoted `"3"` or `"true"` is a text and is rejected, as is `yes`. Unknown keys are rejected. Anything where order matters is a list. The decisions are in ADR-0008 and ADR-0009.

## Identifiers

Every declared id and name matches `[a-z][a-z0-9_]*` and is not `true`, `false` or `null`. This holds whether the YAML scalar is quoted or not, so `3`, `"3"`, `Deck`, `a]b` and the empty id are all rejected. A reference that breaks the grammar fails as a reference to something that does not exist.

## Ruleset structure

See `small-ruleset/small-ruleset.yaml`. The structural sections are `meta`, `parameters`, `zones`, `card_types` and `inputs`. `setup` is loaded (see below); the other behavior sections (`turn`, `actions`, `win_conditions`) are not loaded yet, and a file that has one is rejected.

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
- `player` is the only key that chooses Players. Its only value is `each`: every Player in seat order, a step running fully for one Player before the next.
- `shuffle: { player, zone }` shuffles the zone of each selected Player with the Game's seeded random state.
- `draw: { player, from, to, count }` moves `count` cards (an unquoted integer of at least 1), one at a time, from the top of `from` onto the top of `to`. The top of a zone is the end of its list. Drawing from an empty zone fails Game creation.
- Zones must be declared zones. Setup runs after each Player's cards are placed on the zone of their deck input, and is checked after `inputs`, steps in order. The decisions are in ADR-0011.

A card type's `fields` give the schema of a card: `{ name, type }` with `type` one of `int`, `bool`, `text`. Int values have no minimum or maximum.

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
- The order of the `cards` of an input is kept: it is the order the deck has before the seeded shuffle.

## Starter data for the Small Ruleset

- `small-ruleset/small-catalog.yaml`: 3 Leaders, 16 creatures and the mana card.
- `small-ruleset/decks/deck-a.yaml` and `deck-b.yaml`: two legal decks, each with 1 Leader, 50 creatures (at most 4 copies of each) and 10 mana cards.
