# Ruleset: Duel

A face-up duel between two Players: each holds Life Points, draws from a deck of monsters and spells, summons or sets the monsters, casts the spells, and attacks the other Player's monsters, or the Player directly when they have none, until one has no Life Points left or no card to draw. It is the second, independent ruleset of the engine (ticket [39] of the Phase G roadmap, #94): written from scratch, with names of its own, and the test that the engine's primitives are not shaped around one game. The first version of the rules (**v0**; `meta.version` of the files is 1) was the smallest game that plays. **v1** (`meta.version` 2, ticket [40], #97, ADR-0042) added a Battle phase, a Monster Zone of at most 3 monsters, a direct attack only on an empty Monster Zone and no attack on Turn 1. **v2** (`meta.version` 3, ticket [41], #98, ADR-0043) added monster positions, changing position and combat between monsters. **v3** (`meta.version` 4, ticket [42], #99, ADR-0044) adds tribute summons, normal spells, the Spell/Trap Zone and the first real catalog; the *Not yet* section says what is left. The encoding is `duel-ruleset.yaml`, the cards are in `duel-catalog.yaml` and two sample decks are in `decks/`.

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
| `normal_summoned` | bool | false | Whether the Player has made their Normal Summon, Set, Tribute Summon or Tribute Set this Turn. |

## Zones

Per Player:

| Zone | Visibility | Order matters |
|---|---|---|
| Deck | Hidden for everyone | Yes (the top card is the last card; cards are drawn from the top) |
| Hand | Visible to its owner only | No |
| Graveyard | Public | Yes (the top card is the last card put there) |
| Monster Zone | Public | No |
| Spell/Trap Zone | Public | No |

A monster destroyed in combat goes to its owner's Graveyard. The Spell/Trap Zone holds **at most 3 cards** (a spell is in it only while its Play Spell resolves, see *Normal Spells*).

## Cards

| Type | Fields | Instance state | Effects |
|---|---|---|---|
| **Monster** | `name` (text), `level` (int), `atk` (int), `def` (int) | `attacked` (bool, starts false), `position` (text, starts `"attack"`: the value of a card that has not arrived, since every monster arrives by a summon or a set that gives its own), `position_fixed` (bool, starts false) | No |
| **Spell** | `name` (text) | None | Yes: a triggered Card Effect, see *Normal Spells* |

`def` is the monster's defence, read when a monster attacks a monster in defence position. `position` is `"attack"` or `"defence"`; `position_fixed` is true while the monster has arrived or changed position this Turn (see *Position Change*). The catalog lists twenty-three monsters: sixteen of level 1 to 4 (ATK from 300 to 1800), four of level 5 to 6 (ATK 2000 to 2400) and three of level 7 to 8 (ATK 2600 to 3000), and three spells. A catalog must keep to these constraints: **level at least 1, ATK and DEF at least 0**. The rules rely on them (a monster of level 0 or below is not summonable, and an ATK below 0 would heal the opponent).

## Decks

A deck is a Player's **main deck**: 40 cards, monsters and spells, at most 3 copies of each, placed in the Deck. There is no other deck: no extra deck, no side deck, no leader.

## Game Inputs

| Input | Card types | Size | Copies | Zone |
|---|---|---|---|---|
| Main deck | Monster, Spell | 40 | at most 3 | Deck |

## Setup

1. Each Player's deck is shuffled, Player 0 first, with the Game's seeded random state.
2. Each Player draws 5 cards into their Hand.

## Turn

The Turn of a Player has four phases, in order. The first Turn is Player 0's, and Turns then alternate.

### 1. Draw (automatic)

The Active Player draws 1 card into their Hand. **The starting Player does not draw on the Game's first Turn.** A Player who must draw from an empty Deck loses (see *Win Conditions*).

### 2. Main

The first phase where the Active Player acts. They may take any of these Actions, in any order, as the rules allow (To Battle once, since it leaves Main):

- **Normal Summon**
- **Set**
- **Tribute Summon** and **Tribute Set**, with 1 or 2 tributes
- **Play Spell**
- **Change Position**
- **To Battle**
- **End Turn**

### 3. Battle

The second phase where the Active Player acts, entered from Main by **To Battle**; there is no way back to Main. They may take any of these Actions, as the rules allow:

- **Attack**
- **Direct Attack**
- **End Turn**

An Action of Main is not legal in Battle and the other way round. A Player who never goes to Battle ends the Turn from Main, and Battle is passed over.

### 4. End (automatic)

Runs when the Turn ends, from Main or from Battle. The Active Player's `normal_summoned` becomes false, and the `attacked` and `position_fixed` states of every monster in their Monster Zone become false. Then the other Player's Turn begins.

## Actions

### Normal Summon

The acting Player chooses a monster in their Hand whose **level is from 1 to 4**. It moves to their Monster Zone, and their `normal_summoned` becomes true. A Player may Normal Summon **once per Turn**: while `normal_summoned` is true, no Normal Summon is legal. A Monster Zone holds **at most 3 monsters**: with 3 in it, no Normal Summon is legal (the rule reads the number of monsters in the zone, which must be at most 2 before the summon). A monster in the Monster Zone is face-up; a Normal Summon puts it in **attack position**, and it has arrived this Turn (`position_fixed` is true).

### Set

The same as a Normal Summon, with the same level, once-per-Turn and Monster Zone rules, and it uses the same `normal_summoned` flag: a Player makes one Normal Summon **or** one Set a Turn. The monster is put in **defence position** (`position` is `"defence"`) and has arrived this Turn. **The set monster is face-up and public until Phase H**, which adds face-down cards.

### Tribute Summon and Tribute Set

A monster of **level 5 or 6** is summoned with **1 tribute**, one of **level 7 or more** with **2 tributes** (four Actions: Tribute Summon with one or two tributes, and Tribute Set with one or two, which is the same in defence position). The acting Player chooses the monster in their Hand and the tributes, which are **monsters of their own Monster Zone**, and **different cards**. The tributes are sent to the Graveyard first, as the cost, in the order they were named; then the monster moves to the Monster Zone, in attack position (Summon) or defence position (Set), and has arrived this Turn. The Monster Zone's room is checked **after the tributes have left**: the Monster Zone must hold at most 2 monsters once they are gone, so a summon with 2 tributes from a full Monster Zone is legal, and so is one with 1. All of them share the Normal Summon's once-per-Turn flag: a Player makes **one Normal Summon, Set, Tribute Summon or Tribute Set a Turn**. A monster of level 1 to 4 is never tribute summoned, a level 7 monster cannot be summoned with 1 tribute, and a level 5 or 6 monster not with 2. Naming the same monster twice as tributes is not legal; the two orders of two tributes are two Actions, which differ in the order of the two cards in the Graveyard.

### Play Spell

The acting Player chooses a spell in their Hand, only in Main, while their Spell/Trap Zone holds **at most 2 cards**. The spell moves to the Spell/Trap Zone as the cost, which wakes its Card Effect (see *Normal Spells*). A spell has no once-per-Turn limit.

### Change Position

The acting Player chooses a monster in their Monster Zone whose `position_fixed` is false: it becomes true and the monster moves to the other position (attack to defence, defence to attack). A monster therefore changes position **at most once per Turn** and **never on the Turn it arrived** (summoned or set); the End phase lifts the lock. Only in Main.

### To Battle

Moves the Turn from Main to Battle. The Turn, its number and the Active Player do not change.

### Normal Spells

A spell's Card Effect fires when it arrives in the Spell/Trap Zone (the zone is the ruleset's Active Zone, where Card Effects act), and does these in order: **the spell goes to its owner's Graveyard**, then the effect below. The spell leaves first, so that it ends in the Graveyard whether or not its effect finds a target. A spell is therefore in the Spell/Trap Zone only while its Play Spell resolves.

| Spell | Effect |
|---|---|
| **Breaking Bolt** | Its controller chooses a monster in the opponent's Monster Zone, which goes to its owner's Graveyard. With no monster there, nothing is asked and nothing more happens. |
| **Lucky Find** | Its controller draws 2 cards (a draw from an empty Deck still loses, see *Win Conditions*). |
| **Mending Light** | Its controller gains 1000 Life Points. |

### Attack

The acting Player chooses a monster in their Monster Zone, the **attacker**, that **has not attacked this Turn** (`attacked` is false), is in **attack position**, and a monster in the opponent's Monster Zone, the **target**. An attack is never legal on the Game's first Turn (no opponent monster can exist then, as the turn order leaves the opponent no Turn before it, so the rule only states what the turn order guarantees). The attacker becomes `attacked`, and:

- **Target in attack position.** The monster with the lower ATK is destroyed (it goes to its owner's Graveyard) and its owner loses Life Points equal to the difference of the two ATK. Equal ATK destroys both monsters (the target first, then the attacker) and nobody loses Life Points.
- **Target in defence position.** If the attacker's ATK is greater than the target's DEF the target is destroyed and nobody loses Life Points. If it is less, the attacker's owner loses Life Points equal to the difference. If they are equal nothing happens.

The Life Points change comes before the destruction in the Event Trace. A destroyed monster keeps its instance state as it was when it left the Monster Zone, and nothing reads it in the Graveyard. A monster in defence position cannot attack, so the attack is not legal for it.

### Direct Attack

The acting Player chooses a monster in their Monster Zone that **has not attacked this Turn** (`attacked` is false) and is in **attack position**. The attack is legal only **while the opponent's Monster Zone holds no monster**, and **never on the Game's first Turn**. It becomes `attacked`, and the **opponent loses Life Points equal to the monster's ATK**. Each monster attacks at most once per Turn.

### End Turn

Ends the Turn from Main or from Battle: the End phase runs, then the next Player's Turn begins.

## Win Conditions

Checked after every Action and at the end of Game creation, for each Player in seat order.

1. **Life Points** (`life_out`): a Player whose Life Points are 0 or fewer loses.
2. **Deck out** (`deck_out`): a Player who draws from an empty Deck loses.

The first condition that holds for a Player is their reason. If both Players lose in the same transition the Game is drawn.

## Not yet

The v3 rules (`meta.version: 4`) are the fourth step of a face-up game of the Duel family. What they leave out: choosing several cards at once as a Choice, special summons, traps, quick-play and other kinds of spell, and the coverage counts, the 1,000-Seed net, the Duel pin and the Golden Game (ticket [43] of the Phase G roadmap, #94).

Face-down cards, flipping and traps are the successor roadmap's (Phase H), which may also revisit the Active Zone chosen here.

## Glossary: Duel Ruleset

- **Life Points**: the Player state `life_points`. A Player starts at 4000 and loses at 0 or fewer.
- **Normal Summon**: moving a monster of level 1 to 4 from the Hand to the Monster Zone in attack position, once per Turn, while the Monster Zone holds at most 2 monsters.
- **Set**: a Normal Summon in defence position; it shares the Normal Summon's once-per-Turn flag. The set monster is face-up until Phase H.
- **Monster Position**: a monster's `position`, `"attack"` or `"defence"`; **attack position** and **defence position** are its two values. A Normal Summon puts a monster in attack position and a Set in defence position, and a Position Change moves it to the other. Face-up and public this phase (face-down cards are Phase H's). Not the `Position` of the engine's `move` step.
- **Position Change**: the Action that moves a monster to the other position, once per Turn per monster, never on the Turn it arrived.
- **Attack**: an attack-position monster hitting a monster of the opponent's Monster Zone: ATK against ATK, or ATK against DEF, once per Turn per monster, in Battle.
- **Battle**: the second phase where a Player acts, entered by To Battle from Main; attacks are taken there.
- **To Battle**: the Action that moves the Turn from Main to Battle.
- **Direct Attack**: an attack-position monster hitting the opponent for its ATK, once per Turn per monster, in Battle, when the opponent has no monster, and not on the Game's first Turn.
- **Monster Zone**: the public zone where a Player's summoned monsters stand.
- **Graveyard**: the public, ordered zone for cards that leave play; a monster destroyed in combat goes there.
- **Deck out**: drawing from an empty Deck, which makes the Player lose.
- **Tribute Summon**: moving a monster of level 5 or more from the Hand to the Monster Zone, with 1 tribute (level 5 or 6) or 2 (level 7 or more): monsters of the summoner's Monster Zone, sent to the Graveyard as the cost. Shares the Normal Summon's once-per-Turn flag; the Monster Zone's room is checked after the tributes leave. A **Tribute Set** is the same in defence position.
- **Spell/Trap Zone**: the public, unordered zone, at most 3 cards, where a spell stands while its Play Spell resolves. It is the ruleset's Active Zone.
- **Normal Spell**: a spell played from the Hand in Main: the cost moves it to the Spell/Trap Zone, and its `enter` trigger sends it to the Graveyard and does its effect (Breaking Bolt, Lucky Find, Mending Light).
