# Ruleset: Small
 
A small ruleset for validating the engine's basic functions.
 
## Parameters
 
| Parameter | Value |
|---|---|
| Players | 2 |
| Starting player | Player 0 |
| Starting hand size | 5 |
| Hand size limit | No limit |
 
## Zones
 
Per player:
 
| Zone | Visibility | Order matters |
|---|---|---|
| Deck | Hidden for everyone | Yes |
| Hand | Visible to owner only | No |
| Field | Public | No |
| Trash | Public | Yes (top card = last card put in the trash) |
| Mana Deck | Hidden for everyone | Yes |
| Mana Zone | Public | No |
| Leader Zone | Public | No |
 
## Cards
 
| Type | Fields | Effects |
|---|---|---|
| **Leader** | `name` (text), `power` (int), `life` (int) | No |
| **Creature** | `name` (text), `cost` (int), `power` (int) | No |
| **Mana** | None | No |
 
## Decks
 
- **Main deck**: 50 cards, creatures only, max 4 copies of each.
- **Mana deck**: 10 cards, mana only.
## Card Instance State (mutable)
 
- **Leader**: `current_life` (int), initialized from the Leader's `life`, `tapped` (bool).
- **Creature on field**: `tapped` (bool), `recently_played` (bool).
- **Card in Mana Zone**: `tapped` (bool).
## Game Inputs
 
Each player provides:
 
- **Leader**: exactly 1 Leader card. It is not part of the main deck and starts in the Leader Zone.
- **Main deck**: 50 cards, creatures only, max 4 copies of each.
- **Mana deck**: 10 cards, mana only.

The starter cards are in `small-catalog.yaml` (3 Leaders, 16 creatures, the mana card) and two legal sample decks are in `decks/deck-a.yaml` and `decks/deck-b.yaml`. The file formats are in `../formats.md`.
## Setup

Before the first turn, in this order:

1. **Placement.** Each player's cards are placed in their zones, Player 0 first: the Leader in the Leader Zone, the 50 main deck cards in the Deck and the 10 mana cards in the Mana Deck. Cards are placed in the order of the deck list, one after another, so the first card listed ends at the bottom.
2. **Shuffle.** Each player's main deck is shuffled with the game's seeded random state, Player 0 first and then Player 1. The mana deck is **not** shuffled.
3. **Draw.** Each player in order (Player 0, then Player 1) draws 5 cards from the top of the Deck into the Hand (player 0 draws all 5, then player 1 draws all 5). The top of the Deck is its last card.

Player 0 then takes the first turn.

## Turn

A turn has three phases, in order: Start, Main and End. Player 0's first turn begins as soon as setup ends. Turns are numbered from 1.

### 1. Start (automatic)

1. Untap the cards in the Mana Zone and on the Field (a card that has no `tapped` state is not affected).
2. Clear `recently_played` on the cards on the Field (a card that has no `recently_played` state is not affected).
3. Draw 1 card from the Top of the main deck to the hand. **The starting player (Player 0) skips this draw on the first turn of the game, turn 1, and only then**: Player 1 draws on turn 2 and Player 0 draws on turn 3. If the deck is empty, nothing is drawn and the player is considered to have needed to draw from an empty deck (see Win Conditions).
4. Move 2 cards from the Top of the mana deck to the Mana Zone, one at a time. If the mana deck has fewer than 2 cards, move the cards that are there; when it is empty, nothing moves and the Mana Zone does not grow, and an empty mana deck never makes the player lose. Each player has 10 mana cards, so the mana deck runs out after the player's fifth turn (turns 1, 3, 5, 7 and 9 for Player 0) and the Mana Zone then holds 10 cards for the rest of the game.

### 2. Main

The player may do any of the following, in any order:

- **Play a creature** (from hand to field): pay the creature's cost by tapping mana cards. The creature enters with `recently_played`. The player plays a creature of their own, from their own hand, and needs at least as many untapped cards in their own Mana Zone as the cost, otherwise the play is refused (the opponent's mana does not count). A creature with a negative cost cannot be played. The mana tapped is the first untapped cards, in the order they entered the Mana Zone (mana cards are interchangeable, so the engine chooses). A cost of 0 taps nothing.
- **Attack** with a creature that is neither tapped nor `recently_played`. This starts a combat, and the attacker becomes tapped.
- **Pass the turn.** This ends Main; it is the only way to leave it.

Only the active player acts, and only during Main.

### 3. End

Passing the turn ends the turn: End has no steps of its own. The turn passes to the opponent, and the opponent's Start phase begins at once, in the same instant, so the game always waits in the active player's Main until the game ends.

## Combat
 
The attacker chooses a target: a **tapped creature** of an opponent, or an **opponent's Leader**. An opponent is any player other than the attacking player (with two players, simply the other one). The attacker becomes tapped as soon as the attack is accepted, even when nothing else happens.
 
- **Against a creature**: the attacker never dies. The target creature dies only if the attacker's `power` is greater than or equal to the target's `power`. A creature that dies goes to the top of its owner's trash.
- **Against a Leader**: if the attacker's `power` is greater than or equal to the Leader's `power`, the Leader's owner loses 1 life. Otherwise, nothing happens, a leader can always be attacked, ``tapped`` or not, even while its owner has untapped creatures.
## Win Conditions
- A player whose Leader has `current_life` ≤ 0 loses. The other player wins. This is checked for each Leader, so a Leader created with 0 life or below makes its owner lose at the end of game creation, and if both players have such a Leader the game is drawn.
- A player who needs to draw from an empty deck loses. The other player wins.
- The win conditions are checked after every action and at the end of game creation (setup and the automatic phases of the first turn), once per transition and after everything the transition runs, including the automatic phases it triggers. For example, if passing the turn begins a turn whose Start phase finds the deck empty, that turn's phases still begin, and then the game ends.
- The game ends at once: no action is accepted afterwards.
- If both players lose at the same time, the game is drawn (no winner). Deck-out alone cannot make both players lose at once, because only the active player draws, once per turn.
# Glossary: Small Ruleset 
## Game Structure
 
| Term | Definition |
|---|---|
| **Player** | A participant in the game. This ruleset has exactly 2, numbered from 0. |
| **Starting player** | The player who takes the first turn (Player 0). |
| **Setup** | What happens before the first turn: place each player's cards, shuffle the main decks, then draw the starting hands. |
| **Starting hand** | The 5 cards each player draws from the main deck at the start of the game. |
| **Turn** | One player's complete sequence of phases: Start, Main, End. Turns are numbered from 1; the first turn of the game is turn 1. |
| **Phase** | A segment of a turn, run in order. Start and End are automatic: the game runs them by itself. Main is the phase where the game waits for the active player, who may play creatures, attack and pass the turn. |
| **Game inputs** | What each player provides to start a game: one Leader, a main deck, and a mana deck. |
| **Win condition** | A situation that makes a player lose; the game then ends and the other player wins. |
 
## Zones
 
| Term | Definition |
|---|---|
| **Zone** | A named place where cards can be. Each player has their own set of zones. |
| **Deck** | The player's main draw pile. Hidden for everyone, order matters. |
| **Hand** | Cards the player holds. Visible only to the owner, order doesn't matter. |
| **Field** | Where creatures are in play. Public, order doesn't matter. |
| **Trash** | Where dead creatures go. Public, order matters (top card = last card put in the trash). |
| **Mana Deck** | A separate pile containing only mana cards. Hidden for everyone, order matters. |
| **Mana Zone** | Where mana cards available for paying costs are kept. Public, order doesn't matter. |
| **Leader Zone** | Where the player's Leader is placed. Public, order doesn't matter. |
| **Hidden for everyone** | No player, including the owner, may see the cards or their order. |
| **Visible to owner** | Only the owning player may see the cards. |
| **Public** | Every player may see the cards. |
| **Order matters** | The position of each card in the zone is part of the game state. |
| **Top card** | The card at the end of an ordered zone that is accessed first. |
 
## Cards
 
| Term | Definition |
|---|---|
| **Card definition** | Immutable data describing a card (type and field values). |
| **Card instance** | A card in a running game, carrying mutable state such as `tapped`. |
| **Leader** | A special card representing the player. It is not part of the main deck: each player has exactly one, starting in the Leader Zone. Fields: `name`, `power`, `life`. It can be attacked. |
| **Creature** | A card played to the field that can attack and be attacked. Fields: `name`, `cost`, `power`. |
| **Mana card** | A card with no fields, existing only in the mana deck and Mana Zone. Used to pay costs. |
| **Cost** | The number of mana cards that must be tapped to play a creature. |
| **Power** | A numeric value used in combat to decide whether a target dies or the Leader is damaged. |
| **Life** | A field of the Leader definition: its starting health. |
| **Current life** | Mutable Leader instance state, initialized from `life` and reduced by combat damage. At 0 or below, the player loses. |
| **Effect** | Special card behavior. This ruleset has none. |
 
## States
 
| Term | Definition |
|---|---|
| **Tapped** | A state of a creature or mana card meaning it has been used. |
| **Untapped** | The opposite of tapped: the card is available for use. |
| **Recently played** | A state of a creature that entered the field this turn. It cannot attack. |
 
## Actions
 
| Term | Definition |
|---|---|
| **Action** | A choice made by a player (play a creature, attack, pass the turn). |
| **Draw** | Move the top card of the deck to the hand. |
| **Shuffle** | Put the cards of a zone in a random order, using the game's seeded random state. Only the main deck is shuffled, during setup. |
| **Play** | Move a creature from hand to field, paying its cost. |
| **Tap** | Change a card from untapped to tapped. |
| **Untap** | Change a card from tapped to untapped. |
| **Attack** | Declare a creature as an attacker against a target, tapping it. |
| **Pass the turn** | End the current player's turn and give the turn to the opponent. |
 
## Combat
 
| Term | Definition |
|---|---|
| **Combat** | The resolution that starts when a creature attacks. |
| **Attacker** | The creature that is attacking. It must be neither tapped nor recently played. |
| **Opponent** | Any player other than the attacking player. |
| **Target** | What the attacker is attacking: a tapped creature of an opponent or an opponent's Leader. |
| **Dies** | A creature leaves the field and goes to the top of its owner's trash. |
| **Leader damage** | When the attacker's `power` is ≥ the Leader's `power`, the Leader's `current_life` is reduced by 1. |
 
## Decks
 
| Term | Definition |
|---|---|
| **Main deck** | 50 cards, creatures only, max 4 copies of each. Does not include the Leader. |
| **Mana deck** | 10 cards, mana only. |
| **Empty deck** | A deck with no cards left. Needing to draw from one makes the player lose. An empty mana deck never does. |
