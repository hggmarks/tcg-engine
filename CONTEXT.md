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
_Avoid_: Ruleset for the typed data model; that is the RulesetDefinition.

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

## Ruleset Definitions

**RulesetDefinition**:
The validated, typed data model of a ruleset's structure: meta, parameters, zones, card types and deck inputs. It is data, not behavior; the Ruleset is the behavioral seam.
_Avoid_: Ruleset (for this model)

**Ruleset Draft**:
The plain, unchecked data a frontend fills (strings for ids, no invariants) and hands to `RulesetDefinition::validate`.

**Loader**:
The component that turns file text into a Ruleset Draft and checks syntax only; every semantic check belongs to the engine.

**Definition Path**:
The location of an element in a Ruleset Definition, such as `card_types[creature].fields[cost]`; semantic errors address elements by id, syntax errors by position.

**Zone**:
A named place where cards are held, with a Visibility and an ordered flag (stored, not yet enforced).

**Visibility**:
Who may see a Zone's cards: hidden, owner or public.

**Card Type**:
A kind of card with a Field Schema list, an Instance-State Schema list and an effects flag.

**Field Schema**:
The name and Field Type of one immutable definition field of a Card Type.

**Field Type**:
One of the closed set of field data types. `int`, `bool` and `text` are supported; `enum` and `enum set` are recognized but rejected as unsupported.

**Instance-State Schema**:
The name, Field Type and Initializer of one value that changes during a Game and belongs to a card instance.

**Initializer**:
How an instance-state value starts: a literal of the declared type, or the value of a same-typed field of the same Card Type (`from_field`).

**Deck Input**:
A declared source of cards for a Game: the allowed Card Types, size, optional maximum copies and the destination Zone.

**Player Position**:
A 0-based position in the Player Order, such as the starting player.

**Unsupported Section**:
A behavior section of a ruleset file (`setup`, `turn`, `actions`, `win_conditions`) that the Loader rejects explicitly instead of ignoring.
