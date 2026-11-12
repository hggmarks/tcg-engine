# Self-play harness and engine invariants

**Where it lives.** Self-play and the invariant checks are the crate `tcg-selfplay`, which depends only on `tcg-engine` and uses only its public API. The engine core stays free of AI and harness code. `tcg-loader` is a dev-dependency, so its tests and example load the real Small Ruleset.

**The policy.** `self_play(ruleset, decks, seed, max_steps, checks)` plays one Game from one `Seed`. The Game uses it through `create_game`. The Uniform Random Policy owns a separate `RandomState::from_seed(Seed(seed.0 ^ POLICY_STREAM))` (a documented `u64` constant) and picks index `next_below(len)` of the Legal Actions in Listing Order; it never reads or advances the GameState's Random State. This does not conflict with ADR-0006, whose rule is that the engine's only source of randomness is the Random State stored in the GameState. The policy is not the engine: it is a caller, like a human choosing Actions, and its randomness enters a Game only as the Actions it chooses, which the Game Record keeps (ADR-0019). The engine's own draws (shuffles) still come only from the GameState, and the policy's generator is the same portable integer-only `RandomState`, so a Seed still gives the same Game on every platform. A Game ends when its `outcome()` is set. Failures are typed (`SelfPlayFailure` with the seed, the index of the last accepted Action, a record that reproduces it and a `FailureKind`), never panics; `verify_record` replays a record with the checks on, which is how a failure's record reproduces it. A `Checks::Off` setting exists only so that the example can report the throughput of the engine alone.

**The Step Cap is a failure.** A Game that has not ended after `max_steps` accepted Actions is a `StepCapReached` failure; a Game that ends on its `max_steps`-th Action is not. A Game that has not ended but has no Legal Action is `NoLegalAction`; the Small Ruleset's Action Phase cannot reach it, but a ruleset whose Action Phase lists no action does (the test edits the YAML), so the typed path is exercised.

**The invariants.** After the Opening Position and after every transition, `check_opening` and `check_transition` return every violated Invariant, at most one violation per Invariant (its `detail` names the first offender), in the order of the ticket:
1. No card gained or lost. A transition keeps the same instances, owners and Card Definitions; the Opening Position holds, per Seat, as many instances of each Card Id as that Seat's Deck List totals.
2. Each instance is in exactly one zone: walk the zones of every Player, each id of `cards()` appears exactly once and no zone holds an unknown id. It does not use `zone_of`, which would hide a duplicate.
3. Ordered zones keep their order rules: for each ordered zone, folding the trace over the prior list (a card leaving is removed and the others keep their order; a card arriving by `CardMoved` or `CardCreated` goes on the Top; a `ZoneShuffled` must be a permutation and replaces the list) gives the next list. A `ZoneShuffled` of an unordered zone is a violation (ADR-0013).
4. Instances carry only the states their type declares: instance, catalog card, Card Type; the state names equal the Instance-State Schema names in schema order and each value has the declared Field Type.
5. Event-sourced consistency: the prior state with the trace applied equals the next state (from the Empty GameState for the opening).
6. An ended Game has no Legal Actions.
7. The Turn and Active Player are consistent: with no Turn begun the Active Player is the starting Player; in Turn `n` it is at position `(start + n - 1) mod player_count` of the Player Order; across a transition the Turn Number never decreases.

Invariants 2 and 6 cannot be violated through the public API (the GameState's fields are private and `apply` keeps zones consistent; an ended GameState lists no Action by construction), so they have no "detects a violation" test; the other five do, each through the public API.

**Throughput.** The measured Games per second are in the PR that introduced this crate and from the example `self_play`; most of the time is the Legal Action listing, which runs one State Transition per candidate (ADR-0018), and the invariant checks add a fraction of it. No older ADR conflicts.

**Status**: accepted
