# Ruleset: Duel

A face-up duel between two Players: each holds Life Points, draws from a deck of monsters, summons them and attacks the other Player directly until one has no Life Points left or no card to draw. It is the second, independent ruleset of the engine (ticket [39] of the Phase G roadmap, #94): written from scratch, with names of its own, and the test that the engine's primitives are not shaped around one game. The first version of the rules (**v0**; `meta.version` of the files is 1) is the smallest game that plays; the *Not yet* section says what each later ticket adds. The encoding is `duel-ruleset.yaml`, the monsters are in `duel-catalog.yaml` and two sample decks are in `decks/`.

## Parameters

| Parameter | Value |
|---|---|
| Players | 2 |
| Starting player | Player 0 |
| Starting hand size | 5 |
| Hand size limit | No limit |
| Starting Life Points | 4000 |

## Player state

Each Player holds two values (ADR-0041), both public:

| Name | Type | Starts at | Meaning |
|---|---|---|---|
| `life_points` | int | 4000 | The Player's Life Points. They may go below 0 (the Game ends then). |
| `normal_summoned` | bool | false | Whether the Player has made their Normal Summon this Turn. |

## Zones

Per Player:

| Zone | Visibility | Order matters |
|---|---|---|
| Deck | Hidden for everyone | Yes (the top card is the last card; cards are drawn from the top) |
| Hand | Visible to its owner only | No |
| Graveyard | Public | Yes (the top card is the last card put there) |
| Monster Zone | Public | No |

The Graveyard is declared and nothing sends a card there yet (see *Not yet*).

## Cards

| Type | Fields | Instance state | Effects |
|---|---|---|---|
| **Monster** | `name` (text), `level` (int), `atk` (int), `def` (int) | `attacked` (bool, starts false) | No |

`def` is the monster's defence. Nothing reads it yet (see *Not yet*). The catalog lists sixteen monsters of level 1 to 4, with ATK from 300 to 1800. A catalog must keep to these constraints: **level at least 1, ATK and DEF at least 0**. The rules rely on them (a monster of level 0 or below is not summonable, and an ATK below 0 would heal the opponent).

## Decks

A deck is a Player's **main deck**: 40 monsters, at most 3 copies of each, placed in the Deck. There is no other deck: no extra deck, no side deck, no leader.

## Game Inputs

| Input | Card types | Size | Copies | Zone |
|---|---|---|---|---|
| Main deck | Monster | 40 | at most 3 | Deck |

## Setup

1. Each Player's deck is shuffled, Player 0 first, with the Game's seeded random state.
2. Each Player draws 5 cards into their Hand.

## Turn

The Turn of a Player has three phases, in order. The first Turn is Player 0's, and Turns then alternate.

### 1. Draw (automatic)

The Active Player draws 1 card into their Hand. **The starting Player does not draw on the Game's first Turn.** A Player who must draw from an empty Deck loses (see *Win Conditions*).

### 2. Main

The only phase where the Active Player acts. They may take any of these Actions, in any order and any number of times each, as the rules allow:

- **Normal Summon**
- **Direct Attack**
- **End Turn**

### 3. End (automatic)

Runs when the Turn ends. The Active Player's `normal_summoned` becomes false, and the `attacked` state of every monster in their Monster Zone becomes false. Then the other Player's Turn begins.

## Actions

### Normal Summon

The acting Player chooses a monster in their Hand whose **level is from 1 to 4**. It moves to their Monster Zone, and their `normal_summoned` becomes true. A Player may Normal Summon **once per Turn**: while `normal_summoned` is true, no Normal Summon is legal. A monster in the Monster Zone is face-up, in attack position.

### Direct Attack

The acting Player chooses a monster in their Monster Zone that **has not attacked this Turn** (`attacked` is false). It becomes `attacked`, and the **opponent loses Life Points equal to the monster's ATK**. Each monster attacks at most once per Turn.

### End Turn

Ends the Turn: the End phase runs, then the next Player's Turn begins.

## Win Conditions

Checked after every Action and at the end of Game creation, for each Player in seat order.

1. **Life Points** (`life_out`): a Player whose Life Points are 0 or fewer loses.
2. **Deck out** (`deck_out`): a Player who draws from an empty Deck loses.

The first condition that holds for a Player is their reason. If both Players lose in the same transition the Game is drawn.

## Not yet

The v0 rules (`meta.version: 1`) are the first step of a face-up game of the Duel family. What it leaves out, and the ticket of the Phase G roadmap (#94) that adds it:

- **[40] Phases and counting**: several action phases, so a separate Main and **Battle**; the Monster Zone **capacity of 3**; the **direct attack only when the opponent has no monster**; and **no attack on Turn 1**. Until [40], a monster may attack on Turn 1, whatever the opponent's Monster Zone holds, and the Monster Zone has no limit.
- **[41] Combat**: **positions** (attack and defence) as explicit state, changing position, **combat between monsters** (ATK against ATK, ATK against DEF) and the damage it does; `def` is read there. Until [41] a summoned monster is simply in the Monster Zone, face-up in attack position by rule, and only direct attacks exist.
- **[42] Tributes and spells**: monsters of level 5 and more, summoned with 1 or 2 tributes sent to the Graveyard; spells and the Spell/Trap Zone; and the first real catalog. Until [42] a monster of level 5 or more cannot be summoned, the Graveyard stays empty and the catalog holds only level 1 to 4.

Face-down cards, flipping and traps are the successor roadmap's (Phase H).

## Glossary: Duel Ruleset

- **Life Points**: the Player state `life_points`. A Player starts at 4000 and loses at 0 or fewer.
- **Normal Summon**: moving a monster of level 1 to 4 from the Hand to the Monster Zone, once per Turn.
- **Direct Attack**: a monster hitting the opponent for its ATK, once per Turn per monster.
- **Monster Zone**: the public zone where a Player's summoned monsters stand.
- **Graveyard**: the public, ordered zone for cards that leave play; nothing uses it in the v0 rules.
- **Deck out**: drawing from an empty Deck, which makes the Player lose.
