# Declarative GameState changes only by applying Events

The declarative Ruleset's GameState has private fields, and the only function that changes it is `GameState::apply(&mut self, &Event)`. It checks the whole Event first, so on an error the state is unchanged (atomic). Engine Events are generic and know nothing about any ruleset: `CardCreated`, `CardMoved` and `ZoneShuffled`. Applying an Event Trace to the prior GameState (`apply_trace`) therefore reproduces the next GameState exactly. Game creation starts from an Empty GameState (Players by seat, the Active Player, the Random State of the Seed, empty zones, no Card Instances) and creates every Card Instance with a `CardCreated` Event too, so the Opening Position is a pure fold of the setup Event Trace over the Empty GameState, and creation returns both.

`ZoneShuffled` carries the full resulting order and the Random State after the shuffle, so `apply` needs no generator and replay is exact. The consequence is a rule for the future: **per-Player observations must never expose `random_state`, because it reveals future shuffles, nor the order of a hidden zone.** Hidden-information filtering is out of scope here and must respect this when it arrives.

`Action` and `Rejection` of the declarative Ruleset are empty enums until ticket [10], so `transition` is `match *action {}`. There is no conflict with ADR-0002: a transition applies Events to a clone of the prior GameState, and `apply_trace` already does so. The Ruleset seam of ADR-0005 is unchanged, and creation stays off it.

**Status**: accepted
