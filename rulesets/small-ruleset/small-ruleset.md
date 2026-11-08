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
| **Leader** | `name` (string), `power` (int), `life` (int) | No |
| **Creature** | `name` (string), `cost` (int), `power` (int) | No |
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
## Turn
 
### 1. Start (automatic)
 
1. Untap all of the player's mana and creature cards.
2. Clear `recently_played` on all of the player's cards.
3. Draw 1 card from the main deck to the hand.
4. Move 2 cards from the top of the mana deck to the Mana Zone.
### 2. Main
 
The player may do any of the following, in any order:
 
- **Play a creature** (from hand to field): pay the creature's cost by tapping mana cards. The creature enters with `recently_played`.
- **Attack** with a creature that is neither tapped nor `recently_played`. This starts a combat, and the attacker becomes tapped.
### 3. End
 
The player passes the turn, and the turn passes to the opponent.
 
## Combat
 
The attacker chooses a target: a **tapped creature** of the opponent, or the **opponent's Leader**.
 
- **Against a creature**: the attacker never dies. The target creature dies only if the attacker's `power` is greater than or equal to the target's `power`. A creature that dies goes to the trash.
- **Against a Leader**: if the attacker's `power` is greater than or equal to the Leader's `power`, the Leader's owner loses 1 life. Otherwise, nothing happens, a leader can always be attacked, ``tapped`` or not.
## Win Conditions
- A player whose Leader has `current_life` ≤ 0 loses. The other player wins.
- A player who needs to draw from an empty deck loses. The other player wins.
# Glossary: Small Ruleset 
## Game Structure
 
| Term | Definition |
|---|---|
| **Player** | A participant in the game. This ruleset has exactly 2, numbered from 0. |
| **Starting player** | The player who takes the first turn (Player 0). |
| **Starting hand** | The 5 cards each player draws from the main deck at the start of the game. |
| **Turn** | One player's complete sequence of phases: Start, Main, End. |
| **Phase** | A segment of a turn. The phases are Start, Main, and End. |
| **Game inputs** | What each player provides to start a game: one Leader, a main deck, and a mana deck. |
| **Win condition** | A situation that ends the game and makes a player the winner. |
 
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
| **Target** | What the attacker is attacking: a tapped opposing creature or the opposing Leader. |
| **Dies** | A creature leaves the field and goes to its owner's trash. |
| **Leader damage** | When the attacker's `power` is ≥ the Leader's `power`, the Leader's `current_life` is reduced by 1. |
 
## Decks
 
| Term | Definition |
|---|---|
| **Main deck** | 50 cards, creatures only, max 4 copies of each. Does not include the Leader. |
| **Mana deck** | 10 cards, mana only. |
| **Empty deck** | A deck with no cards left. Needing to draw from one makes the player lose. |
