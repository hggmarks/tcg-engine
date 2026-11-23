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
A Rejection of the Toy Ruleset: the Player performing an Action is not the Active Player. The declarative Ruleset's is the more general Not Deciding Player.

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
The Ruleset's determination that an Action is permitted in the current GameState. In the declarative Ruleset it includes that the Action's Player is the Deciding Player, and that a Declared Action is taken only while no decision is pending and an Answer only while one is (ADR-0033).

**Player Order**:
The canonical, ordered membership of Players in a GameState, used to determine cyclic turn succession.

**Active Player**:
The Player whose turn currently owns the next legal turn action.

**Player**:
A participant identified by a strongly typed identity and positioned in the immutable Player Order.

**PlayerState**:
The domain facts owned by one Player during a Game. In the Toy Ruleset it is typed: that Player's identity and LifeTotal. *(Amended by ADR-0041: the declarative Ruleset has a generic counterpart, the values of the ruleset's Player-State Schemas held by the GameState in Player order, each Player's list in the order of the schemas; `GameState::player_state(player)` and `player_state_value(player, state)` read it, and the Event `PlayerStateChanged` is the only way it changes. It is fully public.)*

**Player-State Schema**:
The id, Field Type (`int` or `bool`) and initial value (`init`) of one value every Player holds, declared by the ruleset's top-level `player_state` list (ADR-0041), beside the Instance-State Schemas of the card types. The engine names none: Life Points are a ruleset notion. Every Player starts with each at its `init`, in the Empty GameState; an absent `player_state` means none.

**Player Ref**:
`{ ref: player.<id>, player: <selector> }`: an expression that reads one Player's value of the Player state `<id>` (ADR-0041). The one expression map with two keys; `player` is a reserved id (`ReservedId`) so the ref cannot be a param. Without the `player:` key it is `MissingPlayerSelector`; the selectors depend on the context (see Player Selector).

**PlayerStateChanged**:
The Event that the Player state `state` of `player` changed to `value`. Emitted only when the value actually changes, one per Player in seat order for `each`; applying it needs the Player, the state and a value of the type of the current one (`UnknownPlayer`, `UnknownPlayerState`, `PlayerStateTypeMismatch`). Observed in full (ADR-0041).

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
Who may see a Zone's cards: `public` every Viewer, `owner` only the Zone's owner, `hidden` nobody, its owner included. A Viewer who cannot see a zone still sees its card count (ADR-0004, ADR-0028).

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
The immutable definition of a card: its Card Id, its Card Type and one FieldValue per field of that type, kept in the schema order of the type, and its Card Effects, of any of three shapes (ADR-0022, ADR-0023, ADR-0024; *Amended by ADR-0038: the third shape is the activated one*). It is distinct from a card instance, which is runtime state.

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
The Ruleset that interprets a RulesetDefinition and a Card Catalog. Its GameState changes only by Event Application. Its Action is a Declared Action (a Player, an Action Definition id and one Argument per Action Parameter), an Answer (a Player and an Answer) a Pass *(Amended by ADR-0037)* or an Activate *(Amended by ADR-0038)*, and its Rejection is one of Game Over (checked before every other), Unknown Player, Not Deciding Player, Declared While Decision Pending, Pass While Decision Pending, Pass Without Window, Action Not In Window, Card Already Tapped *(the last four added by ADR-0037)*, Activate While Decision Pending, Activate Without Window, Abilities Not In Window, Not An Activated Ability *(the last four added by ADR-0038)*, No Decision Pending, Option Out Of Range, Wrong Answer Kind, Card Not Candidate, ChoiceOutsideTriggeredEffect (an internal inconsistency: a ruleset action's effects paused), Unknown Action, Action Not In Current Phase, Argument Count Mismatch, Unknown Card, Not Card Owner, Not Opponent Card, Card Not In Zone, Wrong Card Type, Where Condition Not Met, No Alternative Matched, Negative Count, Not Enough Untapped Cards, State Overflow, Turn Number Overflow, EventRejected, CardDataMismatch, ExpressionTypeMismatch or UnboundParam (ADR-0012, ADR-0014, ADR-0015, ADR-0016). It lists its Legal Actions, filtered through its own `transition` (ADR-0018).

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
One entry of the ruleset's `setup`: a single-key map naming a primitive (`shuffle` or `draw`, *Amended by ADR-0041: or `set_state` or `add_state` of a Player state*) and its arguments.

**Shuffle**:
A Setup Step that reorders a zone of each selected Player with the GameState's Random State. It emits one `ZoneShuffled` per Player, even for a zone of 0 or 1 cards.

**Draw**:
A step that moves `count` cards, one at a time, from the Top of an explicit `from` zone onto the Top of an explicit `to` zone. As a Setup Step, drawing from an empty zone fails Game creation. In an Automatic Step it draws nothing more for that Player and emits one `DrawFromEmptyZone` per Player, and the transition succeeds.

**Player Selector**:
The value of a step's `player` key, accepted by Setup Steps and Automatic Steps alike. `each` is every Player in seat order, and a step runs fully for one Player before the next. `active` is the Active Player of the GameState (the starting Player during setup). *(Amended by ADR-0041: a `player:` key also selects the Players of a Player-state write and the Player of a Player Ref, and the selectors depend on the context. Setup and automatic steps: `each`, `active`. Actions (param `where`, costs, effects): a read takes `active` and `opponent`, the acting Player's opponent; a write takes those plus `each`; `controller` is `ControllerOutsideCardEffect`. Card Effects (a triggered `if` and `do`, an activated ability, a continuous `while`): a read takes `controller`, `opponent`, the controller's opponent, and `active`; a write takes those plus `each`. Any other selector is `UnknownPlayerSelector`, and `opponent` needs exactly 2 Players, `OpponentNeedsTwoPlayers`.)*

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
The state Game creation starts from: Players by seat, the Active Player, the Random State of the Seed, every zone empty, no Card Instances and each Player's Player state at the `init` of its Player-State Schemas (ADR-0041): no Event creates the initial values.

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
The phase with `actions` where the Game waits for the Active Player. A turn has exactly one (ADR-0012). The Game leaves it only through End Turn, so it lists at least one action that ends the Turn, and every declared action is listed by it or by a Response Window (ADR-0027: `NoTurnEndingAction`, `UnlistedAction`; *Amended by ADR-0037: a window's `actions` list an action too, and while a window is open the Game waits in this phase for the Player with Priority*).

**Automatic Step**:
One entry of an Automatic Phase: a single-key map naming a primitive (Untap, Set State, Draw, Move) or an If Step *(Amended by ADR-0041: Set State without `zone` and Add State write Player state)*.

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
One way to fill an Action Parameter: `{ zone, card_type, owner?, where? }`. The card is an instance of that card type in that zone of its owner, who is the acting Player (`owner` absent) or an Opponent (`owner: opponent`), and the Where Condition holds. The first alternative the card fits binds it; an alternative whose Where Condition overflows a derived read does not fit (ADR-0032). A parameter's possible card types are those of its alternatives, in order, each once; a Ref to it must resolve on every one of them.

**Where Condition**:
The bool Expression `where` of a Param Alternative. It reads its own parameter, typed as that alternative's card type alone, and the parameters declared before it, on the GameState before the costs are paid. If it overflows a derived read, the alternative does not fit when the parameter has several alternatives, and the overflow itself (`DerivedValueOverflow`) is the Rejection when it has one (ADR-0032).

**Opponent**:
Any Player other than the acting Player. With two Players it is the other one; the engine does not assume two.

**Expression**:
A literal scalar, a Value Ref *(Amended by ADR-0041: or a Player Ref)*, or one operator over Expressions: `eq: [a, b]` (same type), `gte: [a, b]` and `lte: [a, b]` (ints), `and: [e, ...]` (bools; `and: []` holds) and `is_card_type: [<card ref>, <card type id>]`. The types are the Field Types and the operators give bool; a Card Ref is not a value. Checked at load, depth-first and left to right.

**Subject**:
The card type an expression Win Condition is about: the first name of its Refs (`leader` in `leader.current_life`). The expression must read exactly one *(a Player-state condition has none: it reads no card)*.

**Add State**:
The Action Effect `add_state: { card, state, amount }`. It adds the integer literal `amount` to an int instance state with checked arithmetic: an overflow in either direction is a State Overflow Rejection, and a sum equal to the current value emits no Event.

**If Effect**:
The Action Effect `if: <expression>`, `then: [effects]` and an optional `else: [effects]`, siblings of one list element, run by the Expression at the moment the effect runs. When the condition is exactly `is_card_type: [<param>, T]` it narrows the parameter's possible card types: in `then` to those that are also T (an intersection, in the parameter's order; none is a load error, Impossible Card Type, ADR-0032), in `else` to the others (unchanged if removing T would leave none).

**Argument**:
The value an Action supplies for one Action Parameter; today only a Card Instance Id.

**Ref**:
A reference inside an Action Definition to a parameter *(Amended by ADR-0041: or, with a `player:` key, a Player Ref `player.<id>`)*: `<param>` is a Card Ref (the card) and `<param>.<name>` is a Value Ref (a field of the card's definition or one of its instance states). The name is resolved at load against every possible card type of the parameter to a field or a state, of one kind and one type on all of them (`InconsistentReference` otherwise), and a name that is both is an `AmbiguousReference`. A Ref is also type-checked at load against the slot that holds it. In an expression Win Condition the first name is a card type id, the Subject.

**Cost**:
What an action pays before its effects run *(Amended by ADR-0041: a cost may also be `set_state` or `add_state` of a Player state)*: Tap (`tap: { zone, count }`), Tap Card (`tap: { card: { ref: <param> } }`: the one card, which must be untapped, else `CardAlreadyTapped { card }`; every card type its param may take declares a bool `tapped`, else `TapCardWithoutTappedState { card_type }`) and a `move` cost (`move: { card: { ref: <param> }, to: <zone> }`, with the semantics and the load checks of the `move` effect). For a pushed action the cost is paid at once and the effects wait on the Stack (ADR-0037). Paying is atomic: the whole transition runs on a clone of the GameState (ADR-0002).

**Tap**:
The Cost `tap: { zone, count }`. It needs `count` (a Value Ref to an int, so `tap.count` takes a Ref and no literal yet) untapped cards in that zone of the acting Player and taps the first `count` of them in zone list order, bottom first, even in an unordered zone. An untapped card is one whose card type declares `tapped` and whose value is false. A count of 0 taps nothing.

**Action Effect**:
What an action does once its costs are paid, built from engine primitives: End Turn, `move: { card, to }` (the card goes from the zone it is in onto the Top of `to`, a zone of its owner), `set_state: { card, state, value }` (an Event only if the value changes), Add State *(Amended by ADR-0041: `set_state` and `add_state` take `player: <selector>` in place of `card` to write a Player state, emitting `PlayerStateChanged`)*, **Draw Effect** (only in a Card Effect) and If Effect. Distinct from a **Card Effect**: an ability a Card Definition carries. The `do` of a triggered Card Effect is a list of Action Effects, except End Turn.

**Card Effect**:
An ability carried by a Card Definition, one element of its `effects` list, allowed only on a card type declared `effects: true` (`EffectsNotAllowed` otherwise). It has one of three shapes, the **Card Effect envelope** *(Amended by ADR-0038: three shapes, not two)*: a **Triggered Card Effect** (`on`, optional `if`, `do`), a **Continuous Card Effect** (`while`, `modify`) or an **Activated Card Effect** (`activated`). An element with keys of two shapes is `MixedCardEffectShapes`, one with none of `on`, `while` and `activated` is `MissingCardEffectShape`. The position of an effect in the card's `effects`, counting every shape, is its **Ability Index** when it is activated. The envelope is validated by the engine against the Ruleset Definition. A Triggered Card Effect executes (ADR-0023), a Continuous one is a Derived Read (ADR-0024, ADR-0025) and an Activated one is used with `Action::Activate` (ADR-0038). Distinct from an Action Effect, which an action performs.

**Triggered Card Effect**:
A Card Effect `on: <Trigger Matcher>`, optional `if: <Expression>` and `do: [<Action Effects>]` with at least one effect (`MissingDo`, `EmptyDo`). When an Event matches the matcher it is enqueued, and when it fires its effects run if the condition holds (**Trigger Resolution**, below). `end_turn` anywhere in `do`, nested in an `if` effect included, is `EndTurnInCardEffect`. Its condition is checked with `self` as the only name in scope (a Self Ref); its effects with `self` and the Bound Names of the Card Choices before them (ADR-0034).

**Activated Card Effect**:
A Card Effect `activated: { cost, do }` (ADR-0038; `ActivatedCardEffect::costs()`, `body()`): an ability the card's controller uses as a response. `cost` is optional (none by default) and takes the action cost steps (`tap: { zone, count }`, `tap: { card }`, `move: { card, to }`) with `self` as the only name in scope, `ref: self` included; `do` has at least one effect (`MissingDo`, `EmptyDo`, at `activated.do`) with `self` in scope and no Choice, also nested in an `if` (`InvalidCardEffect { ChoiceOutsideTriggeredEffect }` at `activated.do[j].choose_one` or `.choose_card`); `end_turn` in it is `EndTurnInCardEffect`; an unknown cost step is the loader's unknown-key error at `activated.cost[i]`. It is usable only by the card's owner, only while the card is in the Active Zone, only inside an open Response Window whose declaration has `abilities: true`, and is always pushed (see Activate). `activated` beside `on`, `if`, `do`, `while`, `modify` or `scope` is `MixedCardEffectShapes`. Inert for triggers and modifiers.

**Ability Index**:
The position of an Activated Card Effect among all the effects of its card's definition, whatever their shapes: a card with `[triggered, activated]` has its one ability at index 1. A plain `u32` in `Action::Activate` and in `StackSource::Ability` (ADR-0038).

**Activate**:
`Action::Activate { player, card, ability }`, the fourth engine Action (ADR-0038): the Player with Priority uses the ability at Ability Index `ability` of `card`. The checks run in this order after Game Over, Unknown Player and Not Deciding Player: `ActivateWhileDecisionPending`, `ActivateWithoutWindow` (the Stack is empty, the Action phase included), `AbilitiesNotInWindow { window }`, `UnknownCard { param: self }`, `NotAnActivatedAbility { card, ability }` (out of range or not activated), `NotCardOwner { param: self, .. }`, `CardNotInZone { param: self, zone: <Active Zone> }`, then each cost in order (`CardAlreadyTapped`, `NotEnoughUntappedCards`, ...). The costs are paid at once with `self` bound to the card, then `ItemPushed` pushes an item with source `StackSource::Ability` and `self` bound in the zone the card is in once the cost was paid, then the Trigger Resolution scans the cost's Events. It resolves or Fizzles like any item.

**Continuous Card Effect**:
A Card Effect `while: <Expression>` with `modify: { field, amount }` (`MissingModify`, at `modify`) and an optional `scope`. `while` is required: an always-on effect writes `while: true`, and `scope` and `modify` with no `while` is `MissingCardEffectShape`; a `scope` beside `on`, `if`, `do` or `activated` is `MixedCardEffectShapes`. Its Field Modifier is active while the carrying card is in the Active Zone and the condition, a bool Expression checked in the scope of `self` (the carrier), holds; the condition reads base values only (ADR-0024). With no `scope` the modifier reaches the carrying card alone; with one it reaches the cards of a **Modifier Scope** (ADR-0025).

**Modifier Scope**:
`scope: { zone, card_type, owner?, include_self? }` of a Continuous Card Effect: a filter, not an Expression, naming the cards its Field Modifier reaches (ADR-0025). `zone` (a declared zone, `DanglingReference` at `scope.zone`) and `card_type` (a declared card type, `DanglingReference` at `scope.card_type`) are required; `owner` is absent (the carrying card's owner, `ScopeOwner::Controller`) or `opponent` (any Player other than the carrying card's owner, `ScopeOwner::Opponent`; any other text is `UnknownOwner` at `scope.owner`); `include_self` is absent or false (the carrier is not a target) or true; it matters only when `owner` is absent (with `owner: opponent` it is accepted and changes nothing). The `owner` key has two bases: a param's is relative to the acting Player, a Modifier Scope's to the carrier's owner. A card is **reached** when it is in `zone` of its owner, its Card Definition's type is `card_type`, its owner stands in the `owner` relation to the carrier's owner, and it is not the carrier unless `include_self`. The match reads no field, so a Derived Read never recurses. The scope's `card_type` needs no `effects: true` (targets carry nothing), and `modify.field` is checked against the scope's card type, not the carrier's (`UnknownModifyField`, `ModifyFieldNotInt`, at `modify.field`). The scope is checked after `while` and before `modify`: zone, card type, then owner.

**Trigger Matcher**:
What a Triggered Card Effect matches among Events, the single key of `on`: `enter: <zone>` (the card enters a declared zone, `DanglingReference` if the zone does not exist) or `turn_start: {}` (the controller's Turn began). `enter` matches a `CardMoved` Event only, never `CardCreated`, so Game creation fires nothing. Any other key is `UnknownTriggerMatcher`: the loader keeps it as `Other` and the engine rejects it.

**Self Ref**:
`self`, the carrying card, the name in the scope of a Card Effect's `if`, `while`, `do` and, for an Activated Card Effect, `cost` *(Amended by ADR-0038)* from the start, typed as the card's own card type (exactly one possible card type). The Bound Names of Card Choices join it after their Choice (ADR-0034). `{ ref: self }` is a Card Ref and `{ ref: self.power }` a Value Ref, resolved by the machinery of Refs and Expressions (ADR-0015, ADR-0016); any name that is neither `self` nor a Bound Name in scope is a dangling reference to a param.

**Field Modifier**:
`modify: { field, amount }` of a Continuous Card Effect: an additive integer on a field of the Card Definition of the card it reaches (the carrying card, or the cards of its Modifier Scope), not on its instance state. The field must be declared by the card type it reaches (`UnknownModifyField`, an instance state included) and be an int (`ModifyFieldNotInt`); `amount` is an unquoted integer. It is **active** while its carrying card is in the Active Zone and its `while` holds, and then adds `amount` to every Derived Read of the field; additive modifiers commute.

**Derived Read**:
The value a ref gives for an int field of a card: the base value plus the amounts of the active Field Modifiers, computed at read time by `expression::read` and stored nowhere, with no Event (ADR-0024). Instance states are read as stored. A Viewer asks for one through `DeclarativeRuleset::derived_read(observer, state, observed_card, field)`, for an int field of a card it sees; it is not a method of the `Observations` trait, and its errors come in this order: `UnknownViewer`, `UnknownObservedCard`, `ObserverOutOfStep` (the card is absent or in an unseen zone), `NotAnIntField`, and a `DerivedValueOverflow` naming the Observed Card (ADR-0028). An overflow is `DerivedValueOverflow`. The `while` of a Field Modifier is evaluated with **base values only**, so a derived read never needs another derived read. `modifier_sum(ruleset, state, card, field)` is the one seam: it adds the own modifiers of `card` (no scope, `card` in the Active Zone) and then scans the carriers in the Active Zone, in Player Order and stored zone order, for the set-scoped modifiers whose Modifier Scope reaches `card` (ADR-0025); `expression::read` and its callers are unchanged.

**Active Zone**:
`effects: { active_zone: <zone> }` of a ruleset, the zone where Card Effects act (`RulesetDefinition::effects_active_zone`; `DanglingReference` at `effects.active_zone` if undeclared). The Effects Ruleset's is `field`. A Field Modifier is active only while its **carrier** is there (ADR-0024); a Modifier Scope names the zone its **targets** are in, which may be any declared zone (ADR-0025). A `turn_start` trigger fires only while its card is there, while `enter: <zone>` fires on arrival in the matcher's own zone, whichever it is (the enter exception, ADR-0023). Absent means no Card Effect is ever active: no modifier applies and no trigger is eligible, `enter` included.

**Effects Ruleset**:
The ruleset of Phase C (`rulesets/effects-ruleset/`, id `effects`), derived from the Small Ruleset, which is frozen. *(Amended by ADR-0037: it is itself frozen since Phase F, by the digest test `the_effects_ruleset_is_frozen_since_phase_f`; the Response Ruleset is derived from it.)* It differs by `meta`, by `effects: true` on `creature` and by `effects.active_zone: field`; its catalog adds nine creatures that carry a triggered Card Effect (one of them, `pyre_imp`, a bounded cascade, one, `curious_owl`, an Option Choice, and two, `snare_vine` and `mind_leech`, Card Choices), two that carry a continuous one on themselves and three auras (a continuous one over a Modifier Scope). Its sample decks play all of them but `restless_shade`, which exists only to reach the Trigger Cap; `curious_owl`, `snare_vine` and `mind_leech` joined both decks, two copies each, in place of vanilla creatures (ADR-0035).

**Trigger Resolution**:
After an action's costs and effects (for a pushed action or an Activate after its costs and `ItemPushed` *(Amended by ADR-0038: an Activate)*, for a Pass after `ItemResolved` and the item's effects, ADR-0037), the Events of the transition are scanned in order from the first, costs included; an Event that matches a Trigger Matcher enqueues that trigger, first in first out, in the canonical order (cards of the Active Player first, then seat order, then `state.cards()` order, then the order of the card's `effects`). A trigger fires by reading its `if` and running its `do` until its end or a Choice; the Events it emits are scanned in turn, so a **cascade** queues behind the triggers already waiting. Eligibility is decided when an Event is scanned: an enqueued trigger fires even if its card has moved. A Resolution runs from the Action that wakes the first trigger until the queue is empty, and **spans the pauses** a Choice makes: the Events of a paused `do` are scanned only after the `do` completes, together with those after the pause, so a pause changes no order. Resolution runs only in a transition, never in Game creation, and before the win check, which runs once on the final state of every accepted transition, a pausing one included (ADR-0023, ADR-0033).

**Response Window**:
A window a ruleset declares with `windows: [{ id, opened_by, actions, abilities? }]` (`Window`, `WindowId`, `RulesetDefinition::windows()`; `abilities` is a bool, default false, `Window::abilities()`, *added by ADR-0038*: whether the Player with Priority may also use the activated abilities of their cards in the Active Zone, which are pushed too; `AbilitiesWithoutActiveZone` at `windows[<id>].abilities` if the ruleset declares no Active Zone, `AbilitiesActiveZoneNotPublic { zone }` there if the Active Zone is not `public`). An action named in `opened_by`, taken outside a window, is pushed and opens it; while it is open every action taken is pushed, and the Player with Priority may take only the window's `actions`, or Pass *(Amended by ADR-0038: or, if the window has `abilities: true`, Activate)*. A window is open exactly while the Stack is not empty, so it is derived: the open window is the one whose `opened_by` holds the source action of the bottom Stack Item. Windows need exactly 2 Players (`WindowsNeedTwoPlayers`) (ADR-0037).

**Stack**:
`GameState::stack()`, the Stack Items waiting to resolve, bottom first. Resolution is last in, first out. A Terminal Outcome clears it (ADR-0037).

**Stack Item**:
`StackItem { controller, source, bound }`: a pushed action *(Amended by ADR-0038: or an activated ability)* waiting to resolve, its controller (the Player who took it), its Stack Source and its Bound Cards in param order. `fizzles()` is true when any Bound Card has `left` (ADR-0037).

**Stack Source**:
What put a Stack Item on the Stack: `StackSource::Action(ActionId)`, or `StackSource::Ability { card, ability }` *(Amended by ADR-0038: the second variant, which ADR-0037 announced)*, the card and Ability Index of the activated ability whose effects the item holds; the card is also bound as `self`. It is not `#[non_exhaustive]`, so every `match` on it shows where a source must be handled (ADR-0037). An observed source is an `ObservedStackSource` (see Observed Stack Item).

**Bound Card**:
`BoundCard { param, card, zone, left }`: a card a pushed action bound *(Amended by ADR-0038: or the card of an ability, as `self`)*, the zone it was in once the cost was paid (the bound-from zone unless the cost moved it), and `left`, set on every Stack Item that binds a card when it moves, even if it comes back: state, not history. An Observed Bound Card shows `None` for a card that left (ADR-0037).

**Push**:
Taking an action as a pushed one: its params are bound and its cost paid at once, in its own transition with the cost Events as before, then `ItemPushed { item }` is applied instead of the effects, then the Trigger Resolution (ADR-0037).

**Observed Stack Item**:
`ObservedStackItem { controller, source, bound }` (ADR-0037): the Stack is fully public. `source` is an `ObservedStackSource`: `Action(ActionId)`, or `Ability { card: Option<ObservedCard>, ability }` (ADR-0038), whose card is `None` once the `self` Bound Card has `left`, because a handle names a card only while the Viewer sees it; the source never shows a card that went to a zone the Viewer cannot see. Each `ObservedBoundCard { param, zone, card: Option<ObservedCard> }` is `None` once its card has left.

**Pass**:
`Action::Pass { player }`, the Action of the Player with Priority that resolves the top Stack Item. Its trace is `ItemResolved` (or `ItemFizzled`, then no effects), the item's effect Events, the Trigger Resolution, the pause if any, then the win conditions. Rejected as `PassWhileDecisionPending` while a decision is pending and `PassWithoutWindow` with no window open (ADR-0037).

**Priority**:
The Player who may respond (with a window action or, with `abilities: true`, an ability) or pass while a window is open: the first Player in seat order other than the controller of the top Stack Item, the rule of `decider: opponent`; `GameState::priority()`, `None` when the Stack is empty. Derived, never stored. It applies only while no decision is pending (ADR-0037).

**Unanswered Window**:
An attack Response Window that closed with the opponent passing and no response pushed: in a Game's traces, a `Pass` that resolves or fizzles an `attack` item which the Action just before it pushed (ADR-0039). It is read from the Event Traces and the Game Record only, since the harness exposes no listings: it proves that the window closed with no response, not that a response was available. It is one of the five coverage counts of the Response self-play pass, each asserted at least 1 over the 1,000 pinned Seeds *(Amended by ADR-0040: asserted by the test that sums the counts of the Seed Ranges)*.

**Fizzle**:
A Stack Item whose Bound Card has left does nothing when it resolves: all or nothing, its cost is not refunded, `ItemFizzled { item }` is the first Event of the Pass in the place of `ItemResolved`, and `where` conditions are not read again (ADR-0037).

**Response Ruleset**:
The ruleset of Phase F (`rulesets/response-ruleset/`, id `response`), a copy of the Effects Ruleset with a `holding` zone, a `fast` field on creatures, `attack` tapping its attacker as a cost, `fast_play` and the `attack_response` window, which has `abilities: true` *(added by ADR-0038)*; two fast creatures, `swift_sparrow` and `flash_fox`, and two `skittish_hare`s, an activated ability that pays 1 mana and returns the hare to the hand *(added by ADR-0038)*, are in both sample decks. Rule decisions are made here from Phase F's ticket [34] (ADR-0037).

**End Turn In Pushed Action**:
A Definition Error at an `end_turn` in the effects of a pushed action (one a Response Window lists or opens), at its own path, such as `actions[x].effects[1]`: its effects run on a Pass, which does not leave the Action phase (ADR-0037).

**Pushed Card Not Public**:
A Definition Error at a card param of a pushed action, or at its `one_of[i]` alternative, whose card is not in a `public` zone once the costs are paid: the zone is the `to` of the param's last `move` cost if it has one, else the zone of the alternative. `PushedCardNotPublic { param, zone }` with that zone. The Stack is public, so the bound cards must be seen by every Player (ADR-0037). *(Amended by ADR-0038: the same error is reported for the `self` of an activated ability whose last `move: self` cost ends in a zone that is not `public`, at `cards[x].effects[i].activated.cost[k].move.to`, wrapped in `InvalidCardEffect`.)*

**Duplicate Window Opener**:
A Definition Error at an action named in the `opened_by` of two Response Windows, or twice in one, at the later one (`windows[second].opened_by[0]`); `first` is the earlier (ADR-0037).

**Stack Not Empty**:
The Apply Error of a `TurnBegan` or `PhaseBegan` while the Stack holds an item: a window closes before the Turn goes on (ADR-0037). Its siblings are `StackEmpty` (an item resolves on an empty Stack), `NotTopItem` and `FizzleMismatch` (the item's Fizzle state does not fit the Event, or an `ItemPushed` of an item that already has a card that left).

**Trigger Cap**:
The hard limit of trigger resolutions in one Resolution: 100, counted across its pauses (the Paused Resolution carries the count, which an Answer does not reset). Every trigger taken from the queue counts, one whose `if` is false included. The 101st rejects the whole transition, the Action or Answer that would take it, with `Rejection::TriggerCapExceeded { cap }`; the input GameState is untouched (ADR-0023, ADR-0033).

**Controller**:
The owner of `self`, the carrying card: the Player a Draw Effect in a Card Effect draws for (`player: controller`). Outside a Card Effect there is no `self`, so an action's `draw` with `controller` is `ControllerOutsideCardEffect`.

**Choice**:
An Action Effect that asks a Player to decide, which pauses the Resolution until the Deciding Player answers (ADR-0033). Only a Triggered Card Effect's `do` may hold one, also nested in an `if` there and inside an option of another Choice: anywhere else (a ruleset action's effects, a Continuous Card Effect, an Activated Card Effect's `cost` or `do` *(Amended by ADR-0038)*, a setup or turn step) it is a typed load error. There are two: the Option Choice and the Card Choice (ADR-0034). A Choice found on a straight run always pauses, except a Card Choice with no Candidate, which skips the rest of its whole `do`.

**Option Choice**:
The Choice `choose_one: { decider, options: [[effects], ...] }`: the Decider picks one of at least two effect lists (`TooFewOptions { found }` at `choose_one.options`; an unknown decider is `UnknownDecider` at `choose_one.decider`; outside a Card Effect `ChoiceOutsideTriggeredEffect` at `choose_one`), and the chosen list runs. An option may be empty, and no option has a label. The effects of an option are checked like those of a `do`, at `choose_one.options[k][i]`. The **"may" pattern** is an Option Choice with an empty option, a pattern and not a keyword: `choose_one` with `[[draw], []]` is "you may draw". `curious_owl` is one.

**Card Choice**:
The Choice `choose_card: { name, decider, zone, card_type, owner?, where? }` or `{ name, decider, one_of: [...] }` (ADR-0034): the Decider picks one card from the Candidates. The spec is an Action Param read by the Param machinery (ADR-0015, ADR-0016, ADR-0032): the same `one_of`, `where` and narrowing, and `owner` is relative to the **controller**, whoever decides (absent is the controller's zone, `opponent` the controller's opponent). `name` is a Bound Name. Same placement rules as the Option Choice (`ChoiceOutsideTriggeredEffect` at `choose_card`). The checks run in this order: outside a Card Effect, the name (`InvalidId`, then `DuplicateChoiceName` at `choose_card.name`), the decider (`UnknownDecider`), the param (as for an action), then **the Decider sees every Candidate**, alternative by alternative: its zone is `public`, or `owner` with the owner the Decider (`owner` absent and `decider: controller`, or `owner: opponent` and `decider: opponent`); otherwise `DeciderCannotSeeZone { zone }` at `choose_card.zone` or `choose_card.one_of[i].zone`. A `hidden` zone is never seen: choosing from an unseen zone is a peek, out of scope. `snare_vine` and `mind_leech` are the catalog's. The loader reads the keys of a param plus `decider`, refusing unknown keys.

**Candidate**:
A card a Card Choice can be answered with: `candidates(ruleset, state, controller, scope, param)` in `arguments.rs`, which is the cards of `candidate_cards` that `bind_param` accepts, with the controller as the acting Player, in ascending Card Instance Id (ADR-0018). Evaluated when the Choice is reached, on the GameState of that moment with `self` and the earlier Bound Names in scope, and recomputed when the Answer arrives; never stored. With none the Choice asks nobody and the rest of the whole `do` is skipped, with no Event; with one it is still asked (ADR-0034).

**Bound Name**:
The name a Card Choice binds: a Card Ref, and its fields and instance states Value Refs, for the effects after the Choice in the same list and everything nested in them, not after the enclosing `if` or option ends. Names are unique in scope, `self` included (`DuplicateChoiceName`); siblings (`then` and `else`, two options) may reuse one; a Ref to an unbound or out-of-scope name is the usual dangling reference. A straight run never binds one: the Choice pauses, and the name is bound when the Answer resumes the `do`. A Paused Resolution keeps the names in scope at its Choice as `bound: Vec<BoundName>` (`BoundName { name, card }`, binding order, `self` left out), and a resume rebuilds the scope of each list from the Card Choices before its path, so a name bound inside a branch never reaches the list that follows it (ADR-0034).

**Decider**:
Whom a Choice asks: `controller` (the owner of `self`) or `opponent` (the controller's opponent, the other Player), which is defined for exactly 2 Players: with another player count `OpponentDeciderNeedsTwoPlayers { players }` at `choose_one.decider` or `choose_card.decider` (ADR-0033, ADR-0034).

**Pending Decision**:
The decision a Resolution is paused at, held by the GameState: the Decider, the kind of decision (`DecisionKind::Option` or `DecisionKind::Card`, which stores no candidates) and the Paused Resolution. It is stored by applying `DecisionPending`, cleared by `DecisionAnswered` or by `GameEnded`, and `pending_decision()` reads it. While it is held only `DecisionAnswered` and `GameEnded` apply (ADR-0033).

**Paused Resolution**:
What the engine needs to go on once a decision is answered, `PausedResolution { trigger, at, queue, unscanned, resolutions, bound }`: the trigger whose `do` is paused (`self` is its card), the Effect Path of the Choice, the triggers still queued (front first), the Events the paused `do` emitted that no trigger has scanned yet, the Trigger Cap count so far (the paused trigger included) and the Bound Names in scope at the Choice. Never observed: no Observed GameState, Observed Event or Taken Action carries it (ADR-0033).

**Effect Path**:
The position of a Choice in the `do` of a Triggered Card Effect: `EffectPath { branches, choice }`, the branches entered from the `do` outermost first (`Branch { effect, into }`: the index of an `if` or Choice in its list, and `Then`, `Else` or `Option(k)`), then the index of the Choice in the innermost list. A resume follows it and never reads an `if` on it again.

**Deciding Player**:
The Player who acts next, derived and never stored: the Decider while a decision is pending, otherwise the Player with Priority while a Response Window is open, otherwise the Active Player. `GameState::deciding_player()`. While a decision is pending the Answers are the only Legal Actions, listed for the Deciding Player: in option index order for an Option Choice, one per Candidate in ascending Card Instance Id for a Card Choice (ADR-0033, ADR-0034).

**Answer**:
The Action of the Deciding Player that settles the pending decision: `Action::Answer { player, answer }` with `Answer::Option(index)`, the index of the option picked, counted from 0, or `Answer::Card(id)`, the card picked in a Card Choice. Its Event Trace starts with `DecisionAnswered`. A Declared Action while a decision is pending is `DeclaredWhileDecisionPending`, an Answer with none pending `NoDecisionPending`, an Answer of the other kind `WrongAnswerKind { kind }` (the pending one), an index past the last option `OptionOutOfRange { index, options }`, a card that is not a Candidate (an unknown id included) `CardNotCandidate { card }`, and an Action of any other Player `NotDecidingPlayer { player, deciding }`. The checks run in this order: `GameOver`, `UnknownPlayer`, `NotDecidingPlayer`; then for a Declared Action `DeclaredWhileDecisionPending` and the checks of ADR-0015 (with a window open `ActionNotInWindow` in place of the phase check, ADR-0037); for a Pass `PassWhileDecisionPending`, then `PassWithoutWindow`; for an Answer `NoDecisionPending`, the kind (`WrongAnswerKind`), `OptionOutOfRange` or `CardNotCandidate`, then the rejections of running it (the Trigger Cap among them). A Rejection leaves the GameState untouched and emits no Event.

**Draw Effect**:
The Action Effect `draw: { player, from, to, count }`, only meaningful in a Card Effect: moves `count` cards one at a time from the Top of `from` (an ordered zone) onto the Top of `to`; an empty `from` emits `DrawFromEmptyZone` and stops, which is how a trigger reaches the `draw_from_empty` win conditions (ADR-0014).

**End Turn**:
The effect `end_turn: {}`. It runs every phase after the current one, then the next Player in seat order begins Turn n+1 and their phases run until the Action Phase, all in the same transition. In an action it is only the last top-level effect, never inside an If Effect (ADR-0027); an action whose effects end with it is an action that ends the Turn. A Card Effect has no End Turn at all (ADR-0022).

**Action** (declarative):
A Player, the id of an Action Definition they take and one Argument per Action Parameter, in parameter order (`Action::Declared`). It is one of four engine Actions: a Declared Action, an Answer, a Pass (`Action::Pass { player }`, ADR-0037) and an Activate (`Action::Activate { player, card, ability }`, ADR-0038). It is distinct from the Action Definition it names.

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
A Rejection: the action is declared but the current phase does not list it, because the current phase is absent, unknown to the ruleset or not an Action Phase (the Action Phase lists every declared action, ADR-0027). *(Amended by ADR-0037: a window's `actions` are listed by no phase, so this is also the Rejection of such an action taken with no window open; with a window open the Rejection is Action Not In Window.)*

**End Turn In If Effect**:
A Definition Error at the `end_turn` of an action that is inside an `if` effect, `then` or `else`, at any depth, even when the `if` is the last effect (ADR-0027). Distinct from End Turn In Card Effect.

**End Turn Not Last**:
A Definition Error at a top-level `end_turn` of an action that another effect follows; for `[end_turn, end_turn]` the first one (ADR-0027).

**No Turn Ending Action**:
A Definition Error at the Action Phase when none of the actions it lists ends the Turn (including an empty list), so the Game could wait there forever (ADR-0027).

**Unlisted Action**:
A Definition Error at an action that no phase lists and no Response Window lists in its `actions`; without a `turn` or a window every action is one (ADR-0027, ADR-0037). Being in a window's `opened_by` lists nothing. Where the Rejection Action Not In Current Phase is about the Game, this is about the ruleset.

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

**Impossible Card Type**:
A load error (`ImpossibleCardType`), not a Rejection: the `is_card_type` of an `if` condition tests a Ref for a card type none of the Ref's possible card types is, so narrowing would leave it none. It is reported at the test, `...if.is_card_type` (ADR-0032).

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

**Listing with Rejections**:
`DeclarativeRuleset::listing_with_rejections(state)`: every candidate Action of the GameState in Listing Order, each with the Rejection `transition` gave it, or none when `transition` accepts it. Its accepted candidates, in order, are exactly `legal_actions(state)`: both go through one candidate walk, and `legal_actions` keeps no rejected candidate, so its cost is unchanged. Self-play uses it to see which candidates the Trigger Cap refused (ADR-0026).

**Listing Order**:
The order of a Legal Action Listing in the Declarative Ruleset: while a Response Window is open `Pass`, then the actions in the order the window lists them (ADR-0037), then, if it has `abilities: true`, each Activate of the Player with Priority in ascending Card Instance Id, then Ability Index (ADR-0038); otherwise the actions in the order the current Action Phase lists them; for one action the argument lists in lexicographic order, param by param, each by ascending Card Instance Id whatever the zone. A card that fits several alternatives of a param is one candidate (ADR-0018). The listing visits only the cards of the zones a param names, which gives the same order (ADR-0021).

## Observations

**Observations**:
The optional capability of a Ruleset whose GameState a Viewer can observe, a trait beside `LegalActions` that the Declarative Ruleset implements and the Toy Ruleset does not (a `compile_fail` doctest pins it). `observer(viewer, empty, events)` builds an Observer by folding Events applied to the Empty GameState; `observe(observer, state)` gives the Observed GameState. `Ruleset` is unchanged, there is no serde and no I/O (ADR-0028).

**Viewer**:
Who observes. Its only kind is a Player of the Game (`Viewer::Player`); a Spectator is a later, additive kind (the enum is non-exhaustive).

**Observation**:
What a Viewer is given of a Game: an Observed GameState and, for each accepted transition, Observed Events and an Observed Transition (ADR-0028, ADR-0029). It holds base facts only; Derived Reads are asked beside it, never stored in it (ADR-0028).

**Observed GameState**:
A GameState as one Viewer sees it: always the Players, the Active Player, the Turn Number, the phase, the Terminal Outcome, the Observed Pending Decision, the Stack (`stack()`, each Observed Stack Item) with the Player with Priority (`priority()`; `deciding_player()` is the Decider, else the Player with Priority, else the Active Player) and the card count of every zone; for each zone the Viewer sees, its Observed Cards bottom first; for the Visible Cards, their facts. Never the Random State, never the order of a zone the Viewer cannot see, never a Card Instance Id.

**Observer**:
One Viewer's pure fold of the Events applied to the Empty GameState: it holds the mapping between Observed Cards and Card Instances (`card_instance`, `observed_card`) and is never shown to the Viewer. It changes no GameState, Event, Game Record or snapshot. A card appears (takes the next handle) when a `CardCreated` puts it, or a `CardMoved` brings it from a zone the Viewer cannot see, into a zone the Viewer sees; it disappears (its handle is retired, never reused) when a `CardMoved` takes it from a seen zone to an unseen one; otherwise it keeps its handle. Every other Event leaves the mapping alone.

**Observed Card**:
A Viewer's handle (`ObservedCard(u32)`) for a card it has been shown, in place of the Card Instance Id. Dense from 0 per Viewer, in the order cards become visible, in Event order, so by creation-trace order, not by id or zone. A handle names the card only while the Viewer sees it.

**Visible Card**:
A card the Viewer sees, in an Observed GameState: its Observed Card, owner, Card Definition and instance state as stored; the fields of its definition come from the catalog and no derived value is held.

**Observed Zone**:
One zone as a Viewer sees it: its count always, and its Observed Cards, bottom first, only when the Viewer sees the zone (`cards()` is `None` otherwise).

**Observed Event**:
One Event as a Viewer observes it, in trace order: `CardCreated`/`UnseenCardCreated`, `CardMoved`, `CardMovedIntoSight` (a card the Viewer starts to see, with its Card Definition and its instance state at that point of the trace), `CardMovedOutOfSight` (its Observed Card is retired), `UnseenCardMoved`, `ZoneShuffled` (the new order of Observed Cards, for a zone the Viewer sees) and `UnseenZoneShuffled`, `CardStateChanged` (only for a card the Viewer sees; none otherwise), `TurnBegan`, `PhaseBegan`, `DrawFromEmptyZone`, `GameEnded`, `DecisionPending` (the decider and the kind, never the Paused Resolution), `DecisionAnswered` (the Player and the Observed Answer) and `ItemPushed`, `ItemResolved` and `ItemFizzled` (the whole Observed Stack Item; an ability's source shows its card as an Observed Card, `None` once the card left, *ADR-0038*). No variant carries a Card Instance Id or a Random State. `ObservedGameState::apply` folds them; `follow_events` and `follow_transition` of the Declarative Ruleset produce them (ADR-0029).

**Observed Transition**:
What a Viewer is given of one accepted transition, mirroring `Transition`: the Taken Action, the Observed Events in trace order and the next Observed GameState. Built by `follow_transition`, which folds the transition's Event Trace into the Observer and fails whole or not at all (ADR-0029).

**Taken Action**:
The Action of a transition as a Viewer observes it taken: for a Declared Action the acting Player, the action id and each card argument as a `TakenArgument` (the Observed Card the Viewer holds after the transition, else the one it held before (now retired), else `HiddenCard`), `TakenAction::Declared`; for an Answer the Player and its `ObservedAnswer`, `TakenAction::Answer`; for a Pass the Player, `TakenAction::Pass`; for an Activate the Player, the card as a plain Observed Card (it was in the public Active Zone) and the Ability Index, `TakenAction::Activate` *(ADR-0038)*; where a card an Answer names follows the same rule as a card argument (the Observed `DecisionAnswered` does not: it reads the Viewer's sight at its own Event, so the two differ for a card the Viewer first sees in the same transition). It is not an Action the Viewer can take (ADR-0029, ADR-0033).

**Observed Pending Decision**:
What every Viewer sees of a pending decision: `ObservedPendingDecision { decider, kind }`, read through `pending_decision()` of the Observed GameState, which also offers `deciding_player()`. Never the Paused Resolution (ADR-0033).

**Observed Answer**:
An Answer as a Viewer observes it: `ObservedAnswer::Option(index)`, or `ObservedAnswer::Card(TakenArgument)` for a Card Choice, in the Taken Action of an Answer, in `ObservedEvent::DecisionAnswered { player, answer }` and in an Observed Action. Every Viewer sees every Answer, a card as far as it sees the card: in the Observed `DecisionAnswered` by the handle the Viewer holds at that Event, else `HiddenCard` (so `follow_events` gives the same Observed Events however a trace is sliced); in the Taken Action by the Taken Action rule, its handle after the transition, else before, else `HiddenCard` (ADR-0033, ADR-0034). A `HiddenCard` is never in an Observed Action the Viewer takes: `translate_action` refuses it as `AnswerNamesHiddenCard`.

**Observed Apply Error**:
The `ObservedApplyError` of applying an Observed Event to an Observed GameState: `GameAlreadyEnded` once the Terminal Outcome is set, and `EventDoesNotFit` for every other misfit (an unknown Player, zone, card or state, a zone seen or unseen the wrong way, a card not in its source zone or visible already, a count below zero, an order that is not a permutation, a value of the wrong type, a draw from a zone that is not empty, a phase before any Turn, bad losses). Atomic; it needs no Ruleset and no Observer (ADR-0029).

**Unknown Viewer**:
The `ObservationError` for a Viewer that is not a Player of the Game, never a panic.

**Observed Action**:
An Action as a Viewer expresses it (ADR-0030): the engine Action with an Observed Card for each card argument. `ObservedAction::Declared { player, action, arguments }`, each argument an `ObservedArgument::Card`, `ObservedAction::Answer { player, answer }` with an `ObservedAnswer` (ADR-0033, ADR-0034), `ObservedAction::Pass { player }` (ADR-0037) or `ObservedAction::Activate { player, card, ability }` with an Observed Card (ADR-0038).

**Observed Legal Actions**:
The Legal Actions (ADR-0018) whose Player is the Viewer, each card argument as its Observed Card and each Answer as its option index or, for a Card Choice, as the Observed Card of its candidate (a missing handle is `ObserverOutOfStep`), in the engine's Listing Order: `DeclarativeRuleset::observed_legal_actions(observer, state)`. A Viewer that is not the Deciding Player gets an empty list; the Player with Priority gets `Pass` first, and its Activates last.

**Translating an Observed Action**:
Turning an Observed Action back into the engine Action through the Observer's mapping: `DeclarativeRuleset::translate_action(observer, action)`. It checks only the Player and the handles; the action id, the arity and the legality stay with `transition`.

**Action Of Another Player**:
The `ObservationError` for an Observed Action whose Player is not the Viewer, checked before any handle.

**Legal Action Names Unseen Card**:
The `ObservationError` for a Legal Action of the Viewer that names a card in a zone the Viewer cannot see; listing it would expose the card. It names the action, never the card. No shipped ruleset produces one.

**Unknown Observed Card**:
The `ObservationError` for an Observed Card the Viewer does not hold now: never given, or retired.

**Observer Out Of Step**:
The `ObservationError` for Events or a GameState that do not fit the Observer or the Ruleset: a `CardCreated` of a card that already has a handle; a move out of a zone the Viewer sees of a card with no handle, or out of an unseen zone of a card that holds one; a zone the Ruleset does not declare; a seen card without a handle, or a handle without a seen card; a card missing from the GameState or the catalog; an Event of a trace that does not apply to the GameState it is followed from, or a transition whose GameState is not the one its trace leads to (ADR-0029). In `observed_legal_actions`, a seen card without a handle or a card missing from the GameState; a card in an unseen zone is Legal Action Names Unseen Card instead (ADR-0030). An internal inconsistency, never expected from a Game followed from its Empty GameState, and an error rather than a panic.

## Ending the Game

**Win Condition**:
A declared rule `{ id, when, loser }` of the ruleset's `win_conditions`. When its `when` holds for a Player, that Player is a loser of the Game, and the id is the reason. A `when` is `draw_from_empty: <zone>` (holds for a Player when a draw of theirs in the transition found the zone empty) or an Expression about one Subject (holds for a Player when some card instance of the Subject owned by them, in any zone, in `state.cards()` order, makes it true in the GameState the transition produced) *(Amended by ADR-0041: or a Player-state condition, an Expression whose refs are Player Refs without a `player:` key, which holds for a Player when it is true reading that Player's own Player state in the GameState the transition produced; a `player:` key there is `PlayerSelectorInWinCondition`, and mixing a Subject with Player state is `MixedWinConditionSubject`)*. The only `loser` is `owner`: the Player the condition holds for, for an expression the owner of that card instance *(for a Player-state condition, the Player whose state is read)*. Absent means the Game never ends.

**Loss**:
A Player who lost and the Win Condition id that made them lose. A Player has at most one Loss: for each Player in seat order, the first Win Condition in declaration order that holds for them is their reason.

**Drawn Game**:
A Game in which every Player lost in the same transition, so it has no winner: the Terminal Outcome has no winners. It is not the Draw step; In the Small Ruleset deck-out alone cannot cause it, because only the Active Player draws.

**DecisionPending**:
The Event that a Resolution paused for a decision, `DecisionPending { decider, kind, paused }`. It is the last Event of the trace of the transition that pauses, before `GameEnded` if the same transition ends the Game; applying it stores the Pending Decision. `DecisionPending` while one is pending is `ApplyError::DecisionAlreadyPending`, and any Event but `DecisionAnswered` and `GameEnded` while one is pending is `ApplyError::DecisionPending` (ADR-0033).

**DecisionAnswered**:
The Event that the Deciding Player answered, `DecisionAnswered { player, answer }`. It is the first Event of the trace of an Answer; applying it clears the Pending Decision. With none pending it is `ApplyError::NoDecisionPending`, and from a Player who is not the decider `ApplyError::AnswerFromAnotherPlayer { player, decider }` (ADR-0033).

**GameEnded**:
The Event that the Game ended, with its Losses in seat order. Applying it sets the Terminal Outcome and clears any Pending Decision; no Event applies after it, and it is the last Event of the trace that emits it, after a `DecisionPending` of the same transition. A `GameEnded` without a Loss, with a Player twice or out of seat order, or with a Player not in the Game is an Apply Error.

**Game Over**:
A Rejection: the Game has ended, so every Action is rejected, whoever takes it and whatever it is. It is checked before Unknown Player.

## Golden Games

**Golden Game**:
A Game played from a Seed, the two sample decks (deck A at seat 0, deck B at seat 1) and a literal list of Actions written in the test source. It has no policy and tries nothing: every Action is accepted or the test fails. The Small Ruleset has two, one ending by deck-out and one by a Leader kill, and they are frozen since Phase C: a pinned digest asserts that their snapshots and the Small self-play pin did not change. The Effects Ruleset has two, a Leader kill played with its own sample decks that includes a trigger resolution (ADR-0026) and the **Pausing Golden Game** (ADR-0035). The Response Ruleset has one, the **Response Golden Game** (ADR-0039), whose script holds Passes and Activates.

**Pausing Golden Game**:
The Effects Ruleset's second Golden Game (`effects-pausing`, Seed 204, the shipped Effects decks, 23 Actions): its script holds `Action::Answer`s for an Option Choice (`curious_owl`), a Card Choice decided by the controller (`snare_vine`) and a Card Choice decided by the controller's opponent (`mind_leech`), and ends in a Leader kill. It has the Event Trace snapshot `effects-pausing.trace` and the two observed snapshots `effects-pausing.observed-by-player-0.trace` and `-1.trace`, rewritten together by the ignored test `regenerate_pausing_golden_snapshots`. An Answer is written `== action N: player P answer option I` or `answer card C` in the Event Trace snapshot (ADR-0035).

**Response Golden Game**:
The Response Ruleset's Golden Game (`response-fizzle`, Seed 294, the shipped Response decks, 36 Actions), after the Pausing Golden Game's pattern (ADR-0039). Its script has seat 1 attack with its `skittish_hare`, which taps it; seat 0 attacks the tapped hare, seat 1 activates the hare (Stack depth 2), seat 0 answers with `fast_play` of `flash_fox` (depth 3), and three Passes resolve the fox, the ability and then the attack, which Fizzles because the hare has left; seat 0 then kills seat 1's Leader. It has the Event Trace snapshot `response-fizzle.trace` and the two observed snapshots `response-fizzle.observed-by-player-0.trace` and `-1.trace`, written together, and alone, by the ignored test `regenerate_response_golden_snapshots`. A Pass is written `== action N: player P pass` and an Activate `== action N: player P activate card C ability A`. The script is kept exactly as the throwaway search found it, plays the Game does not need included.

**Golden Snapshot**:
The committed text file (`crates/tcg-loader/tests/golden/*.trace`) holding the Event Trace of a Golden Game: a header line (name, Seed and decks), the creation trace, the trace of each Action under a line naming it, and an `== outcome` line with the Terminal Outcome, with one `Debug` line per Event. A test compares it byte for byte with the trace the Game produces now. It changes only by a deliberate update, an ignored regenerator test (`regenerate_golden_snapshots` for the Small Golden Games, `regenerate_golden_snapshot` for the Effects Leader-kill; the observed, pausing and Response Golden Games have their own, ADR-0017, ADR-0031, ADR-0035, ADR-0039), and it is not a serialization format.

## Self-Play and Replay

**Game Record**:
What is needed to play a Game again: the ruleset id and version, the catalog identity (`CatalogMeta`), the Deck Lists (a Deck List has no id, so its identity is its value, one per Seat), the Seed and the accepted Actions in order. It is an in-memory value with no serialization and no content hash (ADR-0019).

**Replay**:
`DeclarativeRuleset::replay(&record)`: it checks the ruleset identity, then the catalog identity, then re-runs Game creation and each Action. It returns the creation, one Event Trace per Action and the final GameState, which equal those of the original Game, or a `ReplayError` (identity mismatch, creation error, or the index of a rejected Action) (ADR-0019).

**Self-Play**:
Playing a whole Game with a policy that picks every Action, from one Seed, until the Game has a Terminal Outcome. It lives in the crate `tcg-selfplay` and not in the engine (ADR-0020).

**Uniform Random Policy**:
The self-play policy that picks one of the Legal Actions, in Listing Order, uniformly. It draws from its own Random State, derived from the Seed with a fixed constant, and never from the GameState's (ADR-0020).

**Seed Range**:
A contiguous, equal share of the 1,000 pinned Response Seeds (`0..250`, `250..500`, `500..750`, `750..1000`), played by one test under `Checks::On` with every Invariant (ADR-0040). A Seed Range is a pure function of its Seeds, computed at most once per test process and cached in a write-once `OnceLock` of the test file; the part test of the range and the test that joins the ranges both read it, in Seed order. The joining test sums the five coverage counts and compares the Response Self-Play Pin whole. Test scaffolding only: no engine or harness state.

**Self-Play Pin**:
There is one per ruleset, three in all: `crates/tcg-selfplay/tests/selfplay/golden/self-play-seeds-0-1000.digest` for the Small Ruleset, `effects-self-play-seeds-0-1000.digest` for the Effects Ruleset and `response-self-play-seeds-0-1000.digest` for the Response Ruleset (the **Response Self-Play Pin**, ADR-0039), written only by the ignored tests `regenerate_self_play_digests`, `regenerate_effects_self_play_digests` and `regenerate_response_self_play_digests`. Each is the committed text file: one line `seed actions digest` per Seed 0..1000, where `actions` is the number of accepted Actions and `digest` is a 64-bit FNV-1a, in hex, of the `Debug` lines of the creation trace, of each accepted Action followed by the Events of its trace, and of the Terminal Outcome. The 1,000-seed self-play test compares it byte for byte in the same pass that plays the Games, and reports the first Seed that differs *(Amended by ADR-0040: the Response pin is compared whole by the test that joins the Seed Ranges, over the Games the Seed Range tests played)*; only its ignored test rewrites it, so a change to a Game Record or an Event Trace is always a deliberate, reviewed diff, like a Golden Snapshot (ADR-0017, ADR-0021).

**Invariant**:
A property that holds in every GameState and across every transition: no card gained or lost, each instance in exactly one zone, ordered zones keep their order rules, instances carry only the states their type declares, event-sourced consistency, an ended Game has no Legal Actions, and the Turn and Active Player are consistent (ADR-0020); and, since the Effects Ruleset, derived reads do not leak into stored state (the Recomputed GameState equals the engine's) and a rejected Action leaves the GameState intact (ADR-0026). Since Observations there are three more, checked for every Viewer: observations stay consistent, observations ignore hidden cards and observed actions round-trip (ADR-0031). The twelve are checked with `Checks::On`; `Checks::EngineOnly` checks the nine engine ones and does not observe (it exists so that the example can measure the cost of observing); `Checks::Off` checks none. The same Seed gives the same Game under all three.

**Invariant Violation**:
One violated Invariant with a detail naming the first offender, and, for the three over Observations, the Viewer whose Observations violate it (`viewer: Option<Viewer>`, `None` for the nine engine Invariants). A step reports at most one per Invariant, and per Viewer for the observation ones, ordered by Invariant and then by seat (ADR-0020, ADR-0031).

**Recomputed GameState**:
The fold of every Event of a Game, in order, onto the Empty GameState, with nothing the engine carried forward. Self-play and verification keep one and check that it equals the engine's GameState at the Opening Position and after every transition (`DerivedReadsDoNotLeak`). It differs from event-sourced consistency, which folds from the engine's own prior GameState and so cannot see a value carried outside an Event (ADR-0026). When the two differ, the violation names the first part that differs: the Turn Number, the Active Player, the phase (both values, ADR-0036), the Terminal Outcome, the Pending Decision, then the cards and zones.

**Observed Step**:
What one Viewer observes at one step of a Game, the Opening Position or one accepted transition: its Observed Events (of the creation trace, or of the transition), the Taken Action (absent at the Opening Position), the Observed GameState after the step, and its Derived Reads (for every Visible Card in handle order, every int field of its Card Definition in catalog order, through `derived_read`). The leak check compares the real one with the one of the Relabelled Game (ADR-0031).

**Hidden Cards**:
Of a Viewer at a step: the cards that were never in a zone the Viewer sees after any Event of the Game so far, the step's own trace included. A card is seen when a `CardCreated` puts it in a seen zone, or a `CardMoved` has a seen `from` or `to`. They are computed by the harness from the Events and the zones' Visibility (ADR-0028's meaning), never from the Observer, so that the check does not trust what it tests. A card seen in the middle of a trace and gone again is not hidden (ADR-0031).

**Relabelling**:
The permutation the leak check applies to the Hidden Cards of a Viewer, built fresh per step and per Viewer, in the creation trace, every trace so far and the step's Action: the definitions are shuffled among the Hidden Cards (instance states stay with their instance), the ids are shuffled within the classes of Hidden Cards created with the same owner in the same zone (so the apply rules still hold), and in each shuffle of a zone the Viewer cannot see the positions that hold Hidden Cards are shuffled among themselves, one carried in the `unscanned` Events of a Paused Resolution included (ADR-0036). It draws from `RandomState::from_seed(Seed(seed.0 ^ RELABEL_STREAM))`, one per Game, never from the GameState's or the policy's generator, so verifying a record reproduces it (ADR-0031).

**Relabelled Game**:
The Game of the relabelled Events: they are folded from the Empty GameState, a fresh Observer follows them from the Empty GameState, and the relabelled Action and step are followed last. Its Observed Step must equal the real one. Built per step, so it costs O(steps squared) per Game (ADR-0031).

**Trigger Cap Rejection**:
A candidate Action that the Trigger Cap refused (`TriggerCapExceeded`, ADR-0023) while a GameState was listed with rejections. Self-play counts them per Game (`trigger_cap_rejections`, a `TriggerCapRejections`: `Counted(n)`, zero included, with checks on, and `NotMeasured` with `Checks::Off`, whose listings never see a refused candidate, so that "none seen" is never confused with "not measured" *(ADR-0036)*) and checks that each leaves the GameState intact (`RejectionLeavesStateIntact`). It is a counted outcome, never a Self-Play Failure: `pass_turn` is always listed, so the cap cannot leave a Game without a Legal Action (ADR-0026). *(Amended by ADR-0033: while a decision is pending it can. Amended by ADR-0037: so it can while a window is open, when the cap refuses the Pass and every window action; the Game then reports `NoLegalAction`, a Self-Play Failure like any other, with no special rule.)*

**Step Cap**:
The most Actions a self-played Game may accept. A Game that has not ended after that many is a Self-Play Failure; one that ends on its last allowed Action is not.

**Self-Play Failure**:
What self-play or the verification of a record reports instead of a Game: the Seed, the index of the last accepted Action (absent at the Opening Position), a Game Record that reproduces it and its kind (an Invariant Violation, the Step Cap, no Legal Action, a Legal Action that was rejected, a creation error, or a replay error). What the record reproduces depends on the kind: for an Invariant Violation, verifying it fails with the same violation at the same index; for `RejectionLeavesStateIntact`, which is found while listing, it replays to the GameState in which the Action was rejected; for the Step Cap and no Legal Action, it verifies and replays to the GameState the failure stopped in; for a rejected Legal Action, it replays to the GameState in which that Action is rejected; for a creation error, replaying it fails with the creation error.
