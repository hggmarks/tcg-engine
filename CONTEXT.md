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
How a Game ended. In the declarative Ruleset it is `GameState::outcome()`, absent while the Game is on, and holds the Losses and the winners: the Players who did not lose. It is set by applying `GameEnded` (ADR-0014). The Toy Ruleset has none.

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
The Ruleset that interprets a RulesetDefinition and a Card Catalog. Its GameState changes only by Event Application. Its Action is a Player, an Action Definition id and one Argument per Action Parameter, and its Rejection is one of Game Over (checked before every other), Unknown Player, Not Active Player, Unknown Action, Action Not In Current Phase, Argument Count Mismatch, Unknown Card, Not Card Owner, Not Opponent Card, Card Not In Zone, Wrong Card Type, Where Condition Not Met, No Alternative Matched, Negative Count, Not Enough Untapped Cards, State Overflow, Turn Number Overflow, EventRejected, CardDataMismatch, ExpressionTypeMismatch or UnboundParam (ADR-0012, ADR-0014, ADR-0015, ADR-0016). It lists its Legal Actions, filtered through its own `transition` (ADR-0018).

**EventRejected**:
A declarative Rejection for an internal inconsistency: an Event the Ruleset generated was rejected because the GameState does not fit this Ruleset. It is never expected.

**CardDataMismatch**:
A declarative Rejection for an internal inconsistency: a card's definition is missing from the catalog, or a Ref reads a value the card does not have or of another type. It is never expected, and it is only for card data.

**ExpressionTypeMismatch**:
A declarative Rejection for an internal inconsistency: an Expression evaluated to a value of another type than its operator or slot needs. Validation guarantees the types, so it is never expected.

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
The GameState after Game creation: cards placed, setup run, and, if the ruleset has a turn, the first Turn has begun and waits in its Action Phase. Without a turn, no Turn has begun. Win conditions are evaluated at the end of creation, so it may already hold a Terminal Outcome.

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
What an automatic If Step tests when it runs. The only one is First Turn. (The `if` of an Action Effect and the `where` of a param test an Expression instead.)

**First Turn**:
The Condition that holds when the Turn Number is 1, the Game's first Turn; the Small Ruleset uses it so that the starting Player skips the Start-phase draw on that Turn only.

**If Step**:
An Automatic Step `if: <condition>` with a `then` list and an optional `else` list of Automatic Steps, run according to the Condition at the moment the step runs.

**Action Definition**:
A declared action of the ruleset: an id, a list of Action Parameters, a list of Costs and a list of Action Effects. An action without parameters or costs (`pass_turn`) is valid.

**Action Parameter**:
A card an Action supplies: a name unique within its action and one or more Param Alternatives. Written as `{ name, zone, card_type, owner?, where? }` (one alternative) or `{ name, one_of: [...] }`. Arguments bind to parameters by position; the card must fit one of the alternatives.

**Param Alternative**:
One way to fill an Action Parameter: `{ zone, card_type, owner?, where? }`. The card is an instance of that card type in that zone of its owner, who is the acting Player (`owner` absent) or an Opponent (`owner: opponent`), and the Where Condition holds. The first alternative the card fits binds it. A parameter's possible card types are those of its alternatives, in order, each once; a Ref to it must resolve on every one of them.

**Where Condition**:
The bool Expression `where` of a Param Alternative. It reads its own parameter, typed as that alternative's card type alone, and the parameters declared before it, on the GameState before the costs are paid.

**Opponent**:
Any Player other than the acting Player. With two Players it is the other one; the engine does not assume two.

**Expression**:
A literal scalar, a Value Ref, or one operator over Expressions: `eq: [a, b]` (same type), `gte: [a, b]` and `lte: [a, b]` (ints), `and: [e, ...]` (bools; `and: []` holds) and `is_card_type: [<card ref>, <card type id>]`. The types are the Field Types and the operators give bool; a Card Ref is not a value. Checked at load, depth-first and left to right.

**Subject**:
The card type an expression Win Condition is about: the first name of its Refs (`leader` in `leader.current_life`). The expression must read exactly one.

**Add State**:
The Action Effect `add_state: { card, state, amount }`. It adds the integer literal `amount` to an int instance state with checked arithmetic: an overflow in either direction is a State Overflow Rejection, and a sum equal to the current value emits no Event.

**If Effect**:
The Action Effect `if: <expression>`, `then: [effects]` and an optional `else: [effects]`, siblings of one list element, run by the Expression at the moment the effect runs. When the condition is exactly `is_card_type: [<param>, T]` it narrows the parameter's possible card types: to T in `then`, to the others in `else` (unchanged if removing T would leave none).

**Argument**:
The value an Action supplies for one Action Parameter; today only a Card Instance Id.

**Ref**:
A reference inside an Action Definition to a parameter: `<param>` is a Card Ref (the card) and `<param>.<name>` is a Value Ref (a field of the card's definition or one of its instance states). The name is resolved at load against every possible card type of the parameter to a field or a state, of one kind and one type on all of them (`InconsistentReference` otherwise), and a name that is both is an `AmbiguousReference`. A Ref is also type-checked at load against the slot that holds it. In an expression Win Condition the first name is a card type id, the Subject.

**Cost**:
What an action pays before its effects run. The only Cost is Tap. Paying is atomic: the whole transition runs on a clone of the GameState (ADR-0002).

**Tap**:
The Cost `tap: { zone, count }`. It needs `count` (a Value Ref to an int, so `tap.count` takes a Ref and no literal yet) untapped cards in that zone of the acting Player and taps the first `count` of them in zone list order, bottom first, even in an unordered zone. An untapped card is one whose card type declares `tapped` and whose value is false. A count of 0 taps nothing.

**Action Effect**:
What an action does once its costs are paid, built from engine primitives: End Turn, `move: { card, to }` (the card goes from the zone it is in onto the Top of `to`, a zone of its owner), `set_state: { card, state, value }` (an Event only if the value changes), Add State and If Effect. Distinct from a **Card Effect**: an ability written on a card (`effects` on a card type or card), which is out of scope until a later ticket.

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
The Event that a draw found its source zone empty. It changes nothing and is emitted once per Player per draw step; the `draw_from_empty` Win Condition reads it.

**Unknown Action**:
A Rejection: the Action names an action the ruleset does not declare.

**Action Not In Current Phase**:
A Rejection: the action is declared but the current phase does not list it, because there is no current phase, the phase is not an Action Phase, or it does not list the action.

**Argument Count Mismatch**:
A Rejection: the Action has a different number of Arguments than the action has Action Parameters.

**Unknown Card**:
A Rejection: an Argument names a Card Instance Id that does not exist.

**Not Card Owner**:
A Rejection: the card of an Argument belongs to another Player than the acting one, for a parameter that takes the acting Player's cards.

**Not Opponent Card**:
A Rejection: the card of an Argument belongs to the acting Player, for a parameter with `owner: opponent`.

**Card Not In Zone**:
A Rejection: the card of an Argument is not in the parameter's zone of its owner.

**Wrong Card Type**:
A Rejection: the card of an Argument is not of the parameter's card type.

**Where Condition Not Met**:
A Rejection: the card of an Argument has the owner, zone and card type of a parameter with one alternative, but its Where Condition does not hold.

**No Alternative Matched**:
A Rejection: the parameter has several alternatives and the card of the Argument fits none. For one alternative the Rejection is the first check that fails, in the order owner, zone, card type, where.

**State Overflow**:
A Rejection: an Add State would take an int instance state beyond what an int can hold, in either direction. The GameState is unchanged.

**Negative Count**:
A Rejection: a Cost's count resolved to a negative value.

**Not Enough Untapped Cards**:
A Rejection: a Tap Cost needs more untapped cards in the zone than the acting Player has.

## Legal Actions

**Legal Action**:
An Action the Ruleset's `transition` accepts in a GameState, nothing more and nothing less: an Action that fails for any reason, an internal inconsistency included, is not legal. After the Game has ended no Action is legal (ADR-0018).

**Legal Action Listing**:
The optional `LegalActions` capability of a Ruleset: `legal_actions(state)` returns every Legal Action of the GameState exactly once, as a `Vec` with no error type and no Player parameter, each Action carrying its Player. It is separate from `Ruleset` so a Ruleset with an unbounded set of Actions, like the Toy Ruleset, need not implement it. `Runtime::legal_actions` forwards to it (ADR-0018).

**Listing Order**:
The order of a Legal Action Listing in the Declarative Ruleset: the actions in the order the current Action Phase lists them; for one action the argument lists in lexicographic order, param by param, each by ascending Card Instance Id whatever the zone. A card that fits several alternatives of a param is one candidate (ADR-0018).

## Ending the Game

**Win Condition**:
A declared rule `{ id, when, loser }` of the ruleset's `win_conditions`. When its `when` holds for a Player, that Player is a loser of the Game, and the id is the reason. A `when` is `draw_from_empty: <zone>` (holds for a Player when a draw of theirs in the transition found the zone empty) or an Expression about one Subject (holds for a Player when some card instance of the Subject owned by them, in any zone, in `state.cards()` order, makes it true in the GameState the transition produced). The only `loser` is `owner`: the Player the condition holds for, for an expression the owner of that card instance. Absent means the Game never ends.

**Loss**:
A Player who lost and the Win Condition id that made them lose. A Player has at most one Loss: for each Player in seat order, the first Win Condition in declaration order that holds for them is their reason.

**Drawn Game**:
A Game in which every Player lost in the same transition, so it has no winner: the Terminal Outcome has no winners. It is not the Draw step; In the Small Ruleset deck-out alone cannot cause it, because only the Active Player draws.

**GameEnded**:
The Event that the Game ended, with its Losses in seat order. Applying it sets the Terminal Outcome; no Event applies after it, and it is the last Event of the trace that emits it. A `GameEnded` without a Loss, with a Player twice or out of seat order, or with a Player not in the Game is an Apply Error.

**Game Over**:
A Rejection: the Game has ended, so every Action is rejected, whoever takes it and whatever it is. It is checked before Unknown Player.

## Golden Games

**Golden Game**:
A Game played from a Seed, the two sample decks (deck A at seat 0, deck B at seat 1) and a literal list of Actions written in the test source. It has no policy and tries nothing: every Action is accepted or the test fails. The Small Ruleset has two, one ending by deck-out and one by a Leader kill.

**Golden Snapshot**:
The committed text file (`crates/tcg-loader/tests/golden/*.trace`) holding the Event Trace of a Golden Game: a header line (name, Seed and decks), the creation trace, the trace of each Action under a line naming it, and an `== outcome` line with the Terminal Outcome, with one `Debug` line per Event. A test compares it byte for byte with the trace the Game produces now. It changes only by a deliberate update, the ignored test `regenerate_golden_snapshots`, and it is not a serialization format.

## Self-Play and Replay

**Game Record**:
What is needed to play a Game again: the ruleset id and version, the catalog identity (`CatalogMeta`), the Deck Lists (a Deck List has no id, so its identity is its value, one per Seat), the Seed and the accepted Actions in order. It is an in-memory value with no serialization and no content hash (ADR-0019).

**Replay**:
`DeclarativeRuleset::replay(&record)`: it checks the ruleset identity, then the catalog identity, then re-runs Game creation and each Action. It returns the creation, one Event Trace per Action and the final GameState, which equal those of the original Game, or a `ReplayError` (identity mismatch, creation error, or the index of a rejected Action) (ADR-0019).

**Self-Play**:
Playing a whole Game with a policy that picks every Action, from one Seed, until the Game has a Terminal Outcome. It lives in the crate `tcg-selfplay` and not in the engine (ADR-0020).

**Uniform Random Policy**:
The self-play policy that picks one of the Legal Actions, in Listing Order, uniformly. It draws from its own Random State, derived from the Seed with a fixed constant, and never from the GameState's (ADR-0020).

**Invariant**:
A property that holds in every GameState and across every transition: no card gained or lost, each instance in exactly one zone, ordered zones keep their order rules, instances carry only the states their type declares, event-sourced consistency, an ended Game has no Legal Actions, and the Turn and Active Player are consistent (ADR-0020).

**Invariant Violation**:
One violated Invariant with a detail naming the first offender. A check returns at most one per Invariant, in the order above.

**Step Cap**:
The most Actions a self-played Game may accept. A Game that has not ended after that many is a Self-Play Failure; one that ends on its last allowed Action is not.

**Self-Play Failure**:
What self-play or the verification of a record reports instead of a Game: the Seed, the index of the last accepted Action (absent at the Opening Position), a Game Record that reproduces it and its kind (an Invariant Violation, the Step Cap, no Legal Action, a Legal Action that was rejected, a creation error, or a replay error). What the record reproduces depends on the kind: for an Invariant Violation, verifying it fails with the same violation at the same index; for the Step Cap and no Legal Action, it verifies and replays to the GameState the failure stopped in; for a rejected Legal Action, it replays to the GameState in which that Action is rejected; for a creation error, replaying it fails with the creation error.
