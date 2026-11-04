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
Execution where the same initial GameState, **Random State**, and Action sequence produce the same Event Trace and final GameState.

**Random State**:
The seeded pseudo-random generator state held inside a GameState, and the engine's only source of randomness; it is a plain value advanced explicitly, never a hidden global.

**Seed**:
The 64-bit value from which a Random State is created; the same Seed always yields the same sequence.

**Toy Ruleset**:
The intentionally small, game-agnostic proving scenario used to validate the first Runtime increment.

**Turn**:
A period of play owned by one active Player, ending when the rules accept that Player's `PassTurn` Action.

**State Transition**:
The atomic application of one Action to a GameState, yielding either a new GameState and Event Trace or an error with no state change.

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
