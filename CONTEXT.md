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
The ordered record of Events produced while an accepted Action changes a GameState, or while Game creation sets up the opening position. Applying it to the prior GameState reproduces the next one.

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

**Turn Number**:
The 1-based count of Turns since the Game began. Every accepted `PassTurn` increments it. The declarative GameState holds it as an optional value (absent until the first Turn begins), and the First Turn condition is true when it is 1.

**PassTurn**:
An Action by the Active Player that ends their Turn, makes the next Player in the cyclic Player Order active and increments the Turn Number.

**TurnPassed**:
The Toy Ruleset's Event fact (the declarative Ruleset uses TurnBegan) that an accepted PassTurn ended a Turn, including the previous and next Active Player and the resulting Turn Number.

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

## Ruleset Definitions

**RulesetDefinition**:
The validated, typed data model of a ruleset's structure: meta, parameters, zones, card types and deck inputs. It is data, not behavior; the Ruleset is the behavioral seam.
_Avoid_: Ruleset (for this model)

**Ruleset Draft**:
The plain, unchecked data a frontend fills (strings for ids, no invariants) and hands to `RulesetDefinition::validate`.

**Loader**:
The component that turns file text into a Ruleset Draft and checks syntax only; every semantic check belongs to the engine.

**Definition Path**:
The location of an element in a Ruleset Definition, Card Catalog or Deck List, such as `card_types[creature].fields[cost]` or `inputs[1].cards[3]`; semantic errors address declarations by id and references (deck list elements) by position, syntax errors by position.

**Zone**:
A named place where cards are held, with a Visibility and an ordered flag. A shuffle, a draw and a move step that take cards from the Top take them only from an ordered zone, which validation enforces (ADR-0013); a destination may be unordered. A `move` effect of an action takes a named card from any zone.

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
A declared source of cards for a Game: the allowed Card Types (at least one, without repeats), a size of at least 1, an optional Copy Limit and the destination Zone.

**Player Position**:
A 0-based position in the Player Order, such as the starting player.

**Unsupported Section**:
A behavior section of a ruleset file (now only `win_conditions`) that the Loader rejects explicitly instead of ignoring.

**FieldValue**:
A value of one of the supported Field Types: a bool, an integer or a text. Its kind comes from the YAML scalar, so `5` is an integer and `"5"` is a text; int values have no minimum or maximum yet.

**Player Count**:
The number of Players in a Game, as the ruleset's `parameters.players`; at least 1.

## Card Catalogs and Deck Lists

**Identifier**:
A declared name (ruleset id, zone, card type, field name, instance-state name, Deck Input, card) matching `[a-z][a-z0-9_]*` and not `true`, `false` or `null`, whether or not the YAML scalar is quoted. The engine checks it; a reference that breaks it fails as a dangling reference, except the text of a Ref, which fails as `InvalidReference` (ADR-0015).

**Card Catalog**:
The validated, typed set of Card Definitions of one ruleset id and version, in file order. It is built only by `CardCatalog::validate` against an already loaded Ruleset Definition (stage 2 of loading).

**Card Definition**:
The immutable definition of a card: its Card Id, its Card Type and one FieldValue per field of that type, kept in the schema order of the type. It is distinct from a card instance, which is runtime state.

**Card Id**:
The Identifier of a Card Definition, unique within its Card Catalog.

**Deck List**:
The validated, typed cards one Player provides for every Deck Input of a ruleset. It is Game input, built only by `DeckList::validate`, so a Deck List is always legal. It keeps its inputs in the ruleset's input order and the entries of each input in file order.

**Deck Entry**:
A Card Id and a count of at least 1 in a Deck List. A card is listed once per input; entries are never summed.

**Copy Limit**:
A Deck Input's optional `max_copies`: the most copies of one Card Definition an input may hold. It is at least 1 and at most the input's size; an absent key means no limit.

## Declarative Games

**Declarative Ruleset**:
The Ruleset that interprets a RulesetDefinition and a Card Catalog. Its GameState changes only by Event Application. Its Action is a Player, an Action Definition id and one Argument per Action Parameter, and its Rejection is one of Unknown Player, Not Active Player, Unknown Action, Action Not In Current Phase, Argument Count Mismatch, Unknown Card, Not Card Owner, Card Not In Zone, Wrong Card Type, Negative Count, Not Enough Untapped Cards, Turn Number Overflow, EventRejected, CardDataMismatch or UnboundParam (ADR-0012, ADR-0015).

**EventRejected**:
A declarative Rejection for an internal inconsistency: an Event the Ruleset generated was rejected because the GameState does not fit this Ruleset. It is never expected.

**CardDataMismatch**:
A declarative Rejection for an internal inconsistency: a card's definition is missing from the catalog, or a Ref reads a value the card does not have or of another type. It is never expected, and it is only for card data.

**UnboundParam**:
A declarative Rejection for an internal inconsistency: a Ref names a parameter that no Argument was bound to. Validation guarantees every Ref names a parameter of its action, so it is never expected.

**Seat**:
A Player's 0-based position in the Player Order. `PlayerId(n)` sits at seat `n`, and Deck List `n` belongs to seat `n`.

**Setup**:
The Setup Steps of a ruleset, run once after the cards of the Deck Lists are placed. Absent means no steps.

**Setup Step**:
One entry of the ruleset's `setup`: a single-key map naming a primitive (`shuffle` or `draw`) and its arguments.

**Shuffle**:
A Setup Step that reorders a zone of each selected Player with the GameState's Random State. It emits one `ZoneShuffled` per Player, even for a zone of 0 or 1 cards.

**Draw**:
A step that moves `count` cards, one at a time, from the Top of an explicit `from` zone onto the Top of an explicit `to` zone. As a Setup Step, drawing from an empty zone fails Game creation. In an Automatic Step it draws nothing more for that Player and emits one `DrawFromEmptyZone` per Player, and the transition succeeds.

**Player Selector**:
The value of a step's `player` key, accepted by Setup Steps and Automatic Steps alike. `each` is every Player in seat order, and a step runs fully for one Player before the next. `active` is the Active Player of the GameState (the starting Player during setup).

**Top**:
The end of a zone's list. Placing a card appends it, so the first card placed is at the bottom; a draw takes the last card.

**Card Instance**:
A card in a running Game: owner, Card Definition and mutable instance state, initialized per the card type's schema in schema order. It does not store its location, which is derived from the zones.

**Card Instance Id**:
The identity of a Card Instance, dense from 0 in creation order: seat 0 first, then for each seat the Deck Inputs in ruleset order, the Deck List entries in file order, each entry `count` times.

**CardCreated**:
The Event that a Card Instance came into being on the Top of its owner's zone.

**CardMoved**:
The Event that a card left `from`, wherever it was in that list, and went onto the Top of `to`; both zones belong to the same Player. An action's `move` takes `from` from the GameState (`GameState::zone_of`), the zone that holds the card.

**ZoneShuffled**:
The Event that a zone was reordered. It carries the complete resulting order and the Random State after the shuffle, so replay is exact. Observations must never expose its Random State, nor the order of a hidden zone (ADR-0010).

**Apply Error**:
The typed error `GameState::apply` returns for an Event that does not fit the GameState; the state is unchanged. Its `UnknownPlayer` is not the Action Rejection of the same name.

**Event Application**:
`GameState::apply`, the single atomic function through which a declarative GameState changes. A rejected Event leaves the state unchanged.

**Empty GameState**:
The state Game creation starts from: Players by seat, the Active Player, the Random State of the Seed, every zone empty and no Card Instances.

**Opening Position**:
The GameState after Game creation: cards placed, setup run, and, if the ruleset has a turn, the first Turn has begun and waits in its Action Phase. Without a turn, no Turn has begun.

**Game Creation**:
`DeclarativeRuleset::create_game`: from one Deck List per seat and a Seed it returns the Empty GameState and the Opening Position with its setup Event Trace. It is not on the Ruleset seam (ADR-0005).

## The Turn Cycle

**Phase**:
One segment of a Turn, addressed by a Phase Id unique among the phases. It is either an Automatic Phase or the Action Phase.

**Automatic Phase**:
A phase with an `automatic` list of Automatic Steps, run in order when the phase begins.

**Action Phase**:
The phase with `actions` where the Game waits for the Active Player. A turn has exactly one (ADR-0012). The Game leaves it only through End Turn.

**Automatic Step**:
One entry of an Automatic Phase: a single-key map naming a primitive (Untap, Set State, Draw, Move) or an If Step.

**Untap**:
An Automatic Step that sets `tapped` to false on the cards of the listed zones of each selected Player. Like Set State, it touches only cards whose card type declares the state, and emits `CardStateChanged` only for a value that changes.

**Set State**:
An Automatic Step that sets an instance state to a value on every card of a zone of each selected Player whose card type declares it.

**Move**:
An Automatic Step that moves up to `count` cards, one at a time, from the Top of an ordered `from` zone onto the Top of `to`. It moves the cards there are (that is how the mana deck runs out) and emits no fact for a missing card. `position` is `top`, the Top of the source zone.

**Condition**:
What an If Step tests when it runs. The only one is First Turn.

**First Turn**:
The Condition that holds when the Turn Number is 1, the Game's first Turn; the Small Ruleset uses it so that the starting Player skips the Start-phase draw on that Turn only.

**If Step**:
An Automatic Step `if: <condition>` with a `then` list and an optional `else` list of Automatic Steps, run according to the Condition at the moment the step runs.

**Action Definition**:
A declared action of the ruleset: an id, a list of Action Parameters, a list of Costs and a list of Action Effects. An action without parameters or costs (`pass_turn`) is valid.

**Action Parameter**:
A card an Action supplies: a name unique within its action, a zone and a card type. The card must be an instance of that card type in that zone of the acting Player, who is also its owner (an `owner` key arrives with [11]). Arguments bind to parameters by position.

**Argument**:
The value an Action supplies for one Action Parameter; today only a Card Instance Id.

**Ref**:
A reference inside an Action Definition to a parameter: `<param>` is a Card Ref (the card) and `<param>.<name>` is a Value Ref (a field of the card's definition or one of its instance states). The name is resolved at load against the parameter's card type to a field or a state, and a name that is both is an `AmbiguousReference`. A Ref is also type-checked at load against the slot that holds it.

**Cost**:
What an action pays before its effects run. The only Cost is Tap. Paying is atomic: the whole transition runs on a clone of the GameState (ADR-0002).

**Tap**:
The Cost `tap: { zone, count }`. It needs `count` (a Value Ref to an int, so `tap.count` takes a Ref and no literal yet) untapped cards in that zone of the acting Player and taps the first `count` of them in zone list order, bottom first, even in an unordered zone. An untapped card is one whose card type declares `tapped` and whose value is false. A count of 0 taps nothing.

**Action Effect**:
What an action does once its costs are paid, built from engine primitives: End Turn, `move: { card, to }` (the card goes from the zone it is in onto the Top of `to`, a zone of its owner) and `set_state: { card, state, value }` (an Event only if the value changes). Distinct from a **Card Effect**: an ability written on a card (`effects` on a card type or card), which is out of scope until a later ticket.

**Card Effect**:
An ability carried by a card definition. It is not an Action Effect and no primitive for it exists yet.

**End Turn**:
The effect `end_turn: {}`. It runs every phase after the current one, then the next Player in seat order begins Turn n+1 and their phases run until the Action Phase, all in the same transition.

**Action** (declarative):
A Player, the id of an Action Definition they take and one Argument per Action Parameter, in parameter order. It is distinct from the Action Definition it names.

**TurnBegan**:
The Event that a Turn began for a Player with a Turn Number: it sets the Active Player and the Turn Number and clears the phase. Applying it does not check the sequence of Turn Numbers.

**PhaseBegan**:
The Event that a phase of the Turn in progress began; it requires a Turn in progress.

**CardStateChanged**:
The Event that an instance state of a card changed to a value of the same type as before.

**DrawFromEmptyZone**:
The Event that a draw found its source zone empty. It changes nothing and is emitted once per Player per draw step; win conditions use it in [09].

**Unknown Action**:
A Rejection: the Action names an action the ruleset does not declare.

**Action Not In Current Phase**:
A Rejection: the action is declared but the current phase does not list it, because there is no current phase, the phase is not an Action Phase, or it does not list the action.

**Argument Count Mismatch**:
A Rejection: the Action has a different number of Arguments than the action has Action Parameters.

**Unknown Card**:
A Rejection: an Argument names a Card Instance Id that does not exist.

**Not Card Owner**:
A Rejection: the card of an Argument belongs to another Player than the acting one.

**Card Not In Zone**:
A Rejection: the card of an Argument is not in the parameter's zone of the acting Player.

**Wrong Card Type**:
A Rejection: the card of an Argument is not of the parameter's card type.

**Negative Count**:
A Rejection: a Cost's count resolved to a negative value.

**Not Enough Untapped Cards**:
A Rejection: a Tap Cost needs more untapped cards in the zone than the acting Player has.
