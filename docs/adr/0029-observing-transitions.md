# Observing transitions

Amended by ADR-0037: the Observed Event table has three more rows, the item Events, and the fold knows the Stack.

The second half of the Observer (ticket [24], #64, of the Observations roadmap #61), on ADR-0028: for one Viewer, the Observed Events of each accepted transition, the next Observed GameState and an Observed Transition, with an apply on the Observed GameState so that the facts and the state agree. It follows the model the roadmap's checkpoint decided: a projection per Event, fresh handles, the observed Action, consistency.

**The API.** Two inherent methods of `DeclarativeRuleset`, like `derived_read` (ADR-0028), not trait methods: `follow_events(observer, previous, events)`, which folds `events`, applied in order to `previous` (the GameState the Observer has followed), into the Observer and returns the Observed Events in trace order; and `follow_transition(observer, previous, action, transition)`, which does so for the trace of one accepted transition and returns an `ObservedTransition`. `Observations`, `Ruleset`, the Events, the Game Record and the golden snapshots are unchanged: self-play is concrete, and the trait carries only what every observing Ruleset has. The creation trace is projected too (`follow_events` over it, from the Empty GameState): the only shuffles happen there. The Observer folds only accepted transitions: a trace and the Action that produced it, never an Action alone.

**The fold.** Per Event: apply it to a working copy of the previous GameState (an `ApplyError` is `ObserverOutOfStep`: ADR-0028 is amended), fold it into the Observer (`Observer::follow`, ADR-0028, whose `Sight` is the one rule and is never re-derived here), then project. A card that comes into sight carries its Card Definition and instance state **as the working copy has them after that Event**, not as the transition ends: a play sets `recently_played` later in the same trace, and that change is its own Observed Event. Both functions are all-or-nothing: they fold a clone and commit on success, so on `Err` the Observer equals its value before the call (for `follow_transition`, also when the next GameState does not fit). `follow_transition` also checks that the GameState its trace leads to from `previous` is `transition.state`, and is `ObserverOutOfStep` with the Observer unchanged when it differs: the Observed GameState is only ever built from a GameState the trace really produced.

**The projection.** One Observed Event per Event, except a state change of a card the Viewer cannot see. No variant carries a Card Instance Id or a Random State (`ObservedCard` handles only).

| Event | Sight | Observed Event |
|---|---|---|
| `CardCreated` | appeared (zone seen) | `CardCreated`: new handle, owner, definition, zone, state |
| `CardCreated` | unseen | `UnseenCardCreated`: owner, zone |
| `CardMoved` | kept | `CardMoved`: handle, player, `from`, `to` |
| `CardMoved` | appeared | `CardMovedIntoSight`: new handle, player, zones, definition, state at that point |
| `CardMoved` | disappeared | `CardMovedOutOfSight`: the retired handle, player, zones |
| `CardMoved` | unseen | `UnseenCardMoved`: player, zones |
| `ZoneShuffled` | zone seen | `ZoneShuffled`: the new order as handles, bottom first |
| `ZoneShuffled` | zone unseen | `UnseenZoneShuffled`: player, zone; no order |
| `CardStateChanged` | kept | `CardStateChanged`: handle, state, value |
| `CardStateChanged` | unseen | none |
| `TurnBegan`, `PhaseBegan`, `DrawFromEmptyZone`, `GameEnded` | | the same, always |
| `DecisionPending` *(added by ADR-0033)* | | `DecisionPending`: decider and kind, always; never the Paused Resolution |
| `ItemPushed`, `ItemResolved`, `ItemFizzled` *(added by ADR-0037)* | | the same Event with the whole Observed Stack Item (controller, source, bound cards as handles, `None` once left), always |
| `DecisionAnswered` *(added by ADR-0033)* | | `DecisionAnswered`: the Player and the `ObservedAnswer` (the option index *(Amended by ADR-0034: or, for a Card Answer, the card as the Viewer sees it at that Event: the handle the Observer holds, else `HiddenCard`; the Taken Action rule is the Taken Action's)*), always |

A seen shuffle maps each id through the Observer; an id with no handle is `ObserverOutOfStep`.

**Fresh handles.** Unchanged from ADR-0028 and now pinned: a handle is retired by `CardMovedOutOfSight`, never reused, and the card gets the next handle in the dense sequence when it comes back.

**The Taken Action and the Observed Transition.** `TakenAction` is the observed form of the transition's Action (not an Action the Viewer can take, which is [25]'s): *(Amended by ADR-0037: an enum with a third variant, `Pass { player }`)* *(Amended by ADR-0033: an enum, `Declared { player, action, arguments }` and `Answer { player, answer }` with an `ObservedAnswer`; amended by ADR-0034: the card of a Card Answer is named by the same rule as a card argument)* the acting Player, the action id and each card argument as a `TakenArgument`. The argument is `Card(h)` with the handle the Viewer holds **after** the transition; else the handle it held **before** (now retired: a card that leaves sight in the same transition is still named); else `HiddenCard`. `ObservedTransition` is the whole result, mirroring `Transition`: the Taken Action, the Observed Events and the next Observed GameState (`observe` of the folded Observer and the transition's GameState).

**The apply.** *(Amended by ADR-0037: it pushes and pops the Observed Stack like `GameState::apply`, sets a bound card to `None` on any observed move of it, and refuses `TurnBegan` and `PhaseBegan` with a non-empty Stack; a mismatch is `EventDoesNotFit`)* *(Amended by ADR-0033: while an Observed Pending Decision is held only `DecisionAnswered` and `GameEnded` apply, and `DecisionPending`, with none held, stores it; every other use of the two new Observed Events is `EventDoesNotFit`.)* `ObservedGameState::apply(&ObservedEvent)` and `apply_events(&[ObservedEvent])` (clone and fold, like `GameState::apply_trace`), so that applying a transition's Observed Events to the previous Observed GameState gives the next one, from the Observed GameState of the Empty GameState through the creation. It mirrors `GameState::apply`: a move takes the card out of `from` and pushes it onto the end of `to`; `TurnBegan` resets the phase; a card into sight is a new Visible Card owned by the moving Player, and the Visible Cards stay sorted by handle. It is atomic (every check before the first change) and checked: counts use checked arithmetic and nothing panics. It has its own coarse error, `ObservedApplyError`, because it needs no Ruleset and no Observer: `GameAlreadyEnded` once the outcome is set, and `EventDoesNotFit` for everything else: an unknown Player or zone; a zone whose seen or unseen view is not what the variant needs; a card not in `from`, or visible already; a count going below 0 or overflowing; an order that is not a permutation; an unknown state or a value of the wrong type; a phase with no Turn; bad losses; and a `DrawFromEmptyZone` of a zone whose count is not 0 (as `GameState::apply`'s `ZoneNotEmpty`). The losses are checked by `TerminalOutcome::of`, the function both applies now share (it was the private `outcome_of`).

**Errors.** `ObserverOutOfStep` also covers an Event of a trace that does not apply to the GameState it is followed from, and a transition whose GameState is not the one its trace leads to (ADR-0028 is amended at its Errors paragraph).

**Honest limits.** The apply takes the Observed Events as true and checks them only against the Observed GameState, so it cannot tell a forged `CardMovedIntoSight` from a real one. The consistency test covers a scripted Game on each ruleset, to its end, for both seats; a ruleset that moves a card through a zone of a Player other than its owner would need a change to the move variants (the owner of a revealed card is the moving Player).

**Amended by ADR-0033:** `TakenAction` is an enum, there are two more Observed Events (`DecisionPending` and `DecisionAnswered`) and the fold knows the Pending Decision, each marked where it stands.

**Amended by ADR-0034:** the Answer of a Card Choice names a card, in the Taken Action by the Taken Action rule and in the Observed `DecisionAnswered` by the Viewer's sight at that Event, each marked where it stands.

**Conflicts.** ADR-0028's `ObserverOutOfStep` is extended and marked as amended there. ADR-0010's event model is unchanged.

**Status**: accepted
