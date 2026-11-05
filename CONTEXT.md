# TCG Engine

The domain context for a generic, deterministic Trading Card Game runtime.

## Core Runtime Concepts

**Game**:
A single bounded execution of a ruleset involving an ordered set of Players.
_Avoid_: Match

**Runtime**:
The game-agnostic execution boundary that validates player intent and produces the next game state.

**GameState**:
The complete state of one running game at a point in time.

**Ruleset**:
The game-specific definition of legal actions and state transitions that the generic Runtime invokes.

**Action**:
A description of player or system intent, without execution logic.

**Event Trace**:
The ordered record of facts produced while an accepted Action changes a GameState.

**Deterministic Execution**:
Execution where the same initial GameState, random state, and Action sequence produce the same Event Trace and final GameState.

**Toy Ruleset**:
The intentionally small, game-agnostic proving scenario used to validate the first Runtime increment.

**Turn**:
A period of play owned by one active Player, ending when the rules accept that Player's `PassTurn` Action.

**Turn Number**:
The 1-based count of Turns since the Game began. Every accepted `PassTurn` increments it.

**PassTurn**:
An Action by the Active Player that ends their Turn, makes the next Player in the cyclic Player Order active and increments the Turn Number.

**TurnPassed**:
The Event fact that an accepted PassTurn ended a Turn, including the previous and next Active Player and the resulting Turn Number.

**State Transition**:
The atomic application of one Action to a GameState, yielding either a new GameState and Event Trace or an error with no state change.

**Rejection**:
The typed error a Ruleset returns for an Action it does not accept; the GameState is unchanged and no Events are produced.

**Unknown Player**:
A Rejection: an Action names a Player who is not in the Player Order.

**Not Active Player**:
A Rejection: the Player performing an Action is not the Active Player.

**Self-Target**:
A Rejection: a DealDamage Action whose source and target are the same Player.

**Non-Positive Amount**:
A Rejection: a DealDamage Action whose amount is zero or negative.

**LifeTotal Underflow**:
A Rejection: a DealDamage Action that would take the target's LifeTotal below the minimum representable value.

**Turn Number Overflow**:
A Rejection: a PassTurn Action that would take the Turn Number above the maximum representable value.

**Domain Fact**:
Information that defines the game at a point in time and is therefore eligible to belong in GameState or its Event Trace.

**Legality**:
The Ruleset's determination that an Action is permitted in the current GameState.

**Player Order**:
The canonical, ordered membership of Players in a GameState, used to determine cyclic turn succession.

**Active Player**:
The Player whose turn currently owns the next legal turn action.

**Player**:
A participant identified by a strongly typed identity and positioned in the immutable Player Order.

**PlayerState**:
The domain facts owned by one Player during a Game; in the current increment, it contains that Player's identity and LifeTotal.

**LifeTotal**:
A signed numeric measure of a Player's remaining life; it may become negative without ending the Game.

**DealDamage**:
An Action by the Active Player that reduces the opposing Player's LifeTotal by a positive amount.

**DamageDealt**:
The Event fact that an accepted DealDamage Action reduced a target Player's LifeTotal, including source, target, amount, and resulting LifeTotal.

**Terminal Outcome**:
A domain fact that a game has ended and identifies its result; this is outside the first increment.
