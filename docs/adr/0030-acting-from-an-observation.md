# Acting from an Observation

The second half of acting on hidden information (ticket [25], #65, of the Observations roadmap #61): a Viewer lists its Legal Actions with Observed Cards and turns an Observed Action back into the engine Action. It builds on ADR-0028 and reads the Observer's mapping only through `Observer::{viewer, card_instance, observed_card}`. ADR-0018 (Legal Actions, Listing Order) and ADR-0021 (the listing is a filter through `transition`) are unchanged.

**1. Where it lives.** Two inherent methods of the Declarative Ruleset, like `derived_read` and `listing_with_rejections`, not methods of the `Observations` trait, which carries only what every observing Ruleset has: `observed_legal_actions(observer, state)` and `translate_action(observer, action)`. `Observations` and `Ruleset` are unchanged.

**2. The types.** *(Amended by ADR-0033: `ObservedAction` is an enum, `Declared { player, action, arguments }` and `Answer { player, answer }` with an `ObservedAnswer`; amended by ADR-0034: an `ObservedAnswer` may be `Card(TakenArgument)`.)* `ObservedAction { player, action, arguments }` mirrors `Action`, and `ObservedArgument::Card(ObservedCard)` mirrors `Argument::Card`. There is no encoding into vectors or indices.

**3. The listing.** `legal_actions(state)` filtered to the Actions whose Player is the Viewer, in the engine's order, each card argument mapped to its Observed Card *(Amended by ADR-0034: and the card of each Card Answer, as `Card(TakenArgument::Card(handle))`; a missing handle is `ObserverOutOfStep`)*. The other Player's empty list comes from the engine (only the Active Player has Legal Actions *(Amended by ADR-0033: only the Deciding Player has)*); nothing special-cases the Active Player. Nothing is re-ordered, de-duplicated or re-run through `transition`. The Viewer's Player not being in the GameState is `UnknownViewer`.

**4. A Legal Action naming a card its Player cannot see.** *(Amended by ADR-0034: an `Answer::Card` is listed by handle only: the decider sees every candidate, checked when the ruleset loads, so it is never `LegalActionNamesUnseenCard`, and a candidate the Observer holds no handle for is `ObserverOutOfStep`.)* It cannot be expressed without a leak, so the call fails with `LegalActionNamesUnseenCard { action }`. "Unseen" is decided from the GameState's zone of the card and the zone's Visibility (ADR-0004, ADR-0028), not from whether the Observer holds a handle, so a stale Observer can neither leak the card nor misreport it. A seen card the Observer holds no handle for, or a card absent from the GameState, is `ObserverOutOfStep`. The first such argument, per Legal Action in order and per argument in param order, fails the whole call: no partial list, no skipping. No shipped ruleset produces `LegalActionNamesUnseenCard`; a test over two test-only rulesets (an action whose param is over the acting Player's `deck` or the opponent's `hand`) pins the branch.

**5. Translating back.** It needs only the Observer. Checks in order: the Observed Action's Player is not the Viewer (`ActionOfAnotherPlayer { player }`); then each argument in order, a handle the Viewer does not hold (`UnknownObservedCard { card }`, never published or retired). The action id, the arity and the legality are not checked: `transition` decides them, so an Observed Action that translates can still be rejected, as any Action can.

**6. Errors.** Two variants appended to `ObservationError`, after `ObservedCardOverflow`: `ActionOfAnotherPlayer { player }` and `LegalActionNamesUnseenCard { action }`. Neither carries a Card Instance Id. Nothing panics on any input.

**Out of scope here.** Observed Events and the incremental Observer ([24]), what happens to a handle retired between listing and acting (pinned in [26]), self-play invariants ([26]), encodings for AI, a Spectator, serde.

**Amended by ADR-0033:** the Observed Action is an enum and a Viewer that is not the Deciding Player gets the empty list, each marked where it stands. Translating an Observed Answer checks only the Player (`ActionOfAnotherPlayer`); the option index is `transition`'s. *Amended by ADR-0034: the card of a Card Answer is mapped back through the Observer (`UnknownObservedCard` for a handle the Viewer does not hold), and an Answer that names `HiddenCard` is `AnswerNamesHiddenCard`; whether the card is a candidate is `transition`'s.*

**Conflicts.** This amends ADR-0028's Errors paragraph (the variant list), marked there. No other ADR changes and no ruleset file changes.

**Status**: accepted
