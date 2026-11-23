//! The engine invariants checked after every transition (ADR-0020).
//!
//! The checks use only the public API of `tcg-engine`, and none of its helpers that already assume
//! an invariant: they walk `GameState::zones` and `GameState::cards` themselves and never use
//! `GameState::zone_of`, which would hide a card that is in two zones.
//!
//! [`check_transition`] and [`check_opening`] return every violated [`Invariant`], at most once
//! each and in the order of the enum (the ticket's order). The `detail` of a violation names the
//! first offender found for that Invariant.
//!
//! The two Invariants added for the Effects Ruleset (ADR-0026) are checked by functions of their
//! own, because they need what those two do not have: [`check_recomputation`] needs the fold of
//! every Event of the Game, and [`check_rejection`] needs an Action that was refused.

use std::collections::{BTreeMap, BTreeSet};

use tcg_engine::card_catalog::CardId;
use tcg_engine::deck_list::DeckList;
use tcg_engine::declarative::{
    Action, CardInstanceId, DeclarativeRuleset, Event, GameCreation, GameState, Rejection, Viewer,
};
use tcg_engine::player::PlayerId;
use tcg_engine::ruleset::{EventTrace, LegalActions, Ruleset};
use tcg_engine::ruleset_definition::{CardType, ZoneId};

/// A property that holds in every GameState and across every transition of a correct engine, in
/// the order of the ticket.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Invariant {
    /// No card is gained or lost overall: a transition keeps the same card instances, with the
    /// same owners and Card Definitions, and the Opening Position holds exactly the cards of each
    /// seat's Deck List.
    NoCardGainedOrLost,
    /// Each card instance is in exactly one zone, counting the zones of every Player.
    EachInstanceInOneZone,
    /// Ordered zones keep their order rules: the Events of the trace turn the zone of the prior
    /// GameState into the zone of the next one (leaving removes, entering goes on the Top, a
    /// shuffle is a permutation), and only an ordered zone is shuffled (ADR-0013).
    OrderedZonesKeepOrder,
    /// Each instance carries exactly the instance states its Card Type declares, in schema order
    /// and of the declared Field Type.
    InstanceStatesMatchType,
    /// Event-sourced consistency: applying the trace to the prior GameState gives the next one.
    EventSourcedConsistency,
    /// A Game that has ended has no Legal Actions.
    EndedGameHasNoLegalActions,
    /// The Turn Number and the Active Player agree with the Player Order and the starting Player,
    /// and the Turn Number never decreases.
    TurnAndActivePlayerConsistent,
    /// Derived reads never leak into stored state: the GameState the engine carries equals the
    /// GameState obtained by folding every Event of the Game, in order, onto the Empty GameState,
    /// with nothing the engine carried forward. It differs from `EventSourcedConsistency`, which
    /// folds from the engine's own prior state and so cannot see a value the engine carries
    /// forward outside an Event, such as a cached Field Modifier.
    DerivedReadsDoNotLeak,
    /// A rejected Action leaves the GameState it was offered untouched, emits no Event, and is
    /// rejected the same way when it is run again.
    RejectionLeavesStateIntact,
    /// Observations stay consistent: applying the Observed Events of a transition to the previous
    /// Observed GameState of a Viewer gives its next Observed GameState (ADR-0031).
    ObservationsStayConsistent,
    /// Observations ignore hidden cards: the Observed Step of a Viewer does not change when the
    /// cards it cannot see are relabelled in the GameState and the trace alike (ADR-0031).
    ObservationsIgnoreHiddenCards,
    /// Observed Actions round-trip: translating a Viewer's Observed Legal Actions back gives
    /// exactly its Actions from `legal_actions`, in the same order (ADR-0031).
    ObservedActionsRoundTrip,
}

/// One violated [`Invariant`], with a human-readable account of the first offender.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InvariantViolation {
    pub invariant: Invariant,
    /// The Viewer whose Observations violate the Invariant; `None` for the nine engine
    /// Invariants, which no Viewer is involved in (ADR-0031).
    pub viewer: Option<Viewer>,
    pub detail: String,
}

/// Checks the Opening Position of `creation` against the Deck Lists it was made from
/// (`decks[i]` for seat `i`). The trace is folded from the Empty GameState. Every violated
/// [`Invariant`] is returned once, in ticket order.
pub fn check_opening(
    ruleset: &DeclarativeRuleset,
    decks: &[DeckList],
    creation: &GameCreation,
) -> Vec<InvariantViolation> {
    let state = &creation.opening.state;
    collect([
        (
            Invariant::NoCardGainedOrLost,
            opening_cards_differ(decks, state),
        ),
        (Invariant::EachInstanceInOneZone, not_in_one_zone(state)),
        (
            Invariant::OrderedZonesKeepOrder,
            zone_order_broken(
                ruleset,
                &creation.empty,
                creation.opening.trace.events(),
                state,
            ),
        ),
        (
            Invariant::InstanceStatesMatchType,
            instance_states_wrong(ruleset, state),
        ),
        (
            Invariant::EventSourcedConsistency,
            trace_does_not_give(&creation.empty, creation.opening.trace.events(), state),
        ),
        (
            Invariant::EndedGameHasNoLegalActions,
            ended_with_legal_actions(ruleset, state),
        ),
        (
            Invariant::TurnAndActivePlayerConsistent,
            turn_inconsistent(ruleset, state),
        ),
    ])
}

/// Checks one accepted transition: `trace` took `prior` to `next`. Every violated [`Invariant`]
/// is returned once, in ticket order. `prior` is trusted to have been checked already: the
/// state-level checks run on `next` only.
pub fn check_transition(
    ruleset: &DeclarativeRuleset,
    prior: &GameState,
    trace: &EventTrace<Event>,
    next: &GameState,
) -> Vec<InvariantViolation> {
    collect([
        (Invariant::NoCardGainedOrLost, cards_changed(prior, next)),
        (Invariant::EachInstanceInOneZone, not_in_one_zone(next)),
        (
            Invariant::OrderedZonesKeepOrder,
            zone_order_broken(ruleset, prior, trace.events(), next),
        ),
        (
            Invariant::InstanceStatesMatchType,
            instance_states_wrong(ruleset, next),
        ),
        (
            Invariant::EventSourcedConsistency,
            trace_does_not_give(prior, trace.events(), next),
        ),
        (
            Invariant::EndedGameHasNoLegalActions,
            ended_with_legal_actions(ruleset, next),
        ),
        (
            Invariant::TurnAndActivePlayerConsistent,
            turn_inconsistent(ruleset, next).or_else(|| turn_went_back(prior, next)),
        ),
    ])
}

/// Checks that the Recomputed GameState equals the GameState the engine produced. `recomputed` is
/// the fold of every Event of the Game so far, in order, onto the Empty GameState
/// (`GameCreation::empty`): it must never start from a GameState the engine carried, or this would
/// repeat `EventSourcedConsistency`. A violation names the first part that differs.
pub fn check_recomputation(recomputed: &GameState, next: &GameState) -> Vec<InvariantViolation> {
    state_difference(recomputed, next)
        .map(|detail| InvariantViolation {
            invariant: Invariant::DerivedReadsDoNotLeak,
            viewer: None,
            detail,
        })
        .into_iter()
        .collect()
}

/// Checks that a rejected Action leaves its GameState intact: `action` is run against `prior`
/// again, the Rejection must be `rejection` once more, and `prior` must equal the clone taken
/// before the call. A rejection has no Event Trace and no GameState, so nothing is emitted.
pub fn check_rejection(
    ruleset: &DeclarativeRuleset,
    prior: &GameState,
    action: &Action,
    rejection: &Rejection,
) -> Vec<InvariantViolation> {
    let before = prior.clone();
    let rerun = ruleset.transition(prior, action);
    let detail = match rerun {
        Err(again) if again == *rejection => None,
        Err(again) => Some(format!(
            "{action:?} was rejected with {rejection:?} and then with {again:?}"
        )),
        Ok(_) => Some(format!(
            "{action:?} was rejected with {rejection:?} and then accepted"
        )),
    }
    .or_else(|| (*prior != before).then(|| "a rejected Action changed its GameState".to_string()));
    detail
        .map(|detail| InvariantViolation {
            invariant: Invariant::RejectionLeavesStateIntact,
            viewer: None,
            detail,
        })
        .into_iter()
        .collect()
}

/// The first part in which `recomputed` and `next` differ, or `None` when they are equal.
fn state_difference(recomputed: &GameState, next: &GameState) -> Option<String> {
    if recomputed == next {
        return None;
    }
    if recomputed.turn_number() != next.turn_number() {
        return Some(format!(
            "the Turn Number is {:?} in the Recomputed GameState and {:?} in the engine's",
            recomputed.turn_number().map(|turn| turn.get()),
            next.turn_number().map(|turn| turn.get())
        ));
    }
    if recomputed.active_player() != next.active_player() {
        return Some(format!(
            "the Active Player is Player {} in the Recomputed GameState and Player {} in the engine's",
            recomputed.active_player().0,
            next.active_player().0
        ));
    }
    if recomputed.phase() != next.phase() {
        let named = |state: &GameState| {
            state
                .phase()
                .map_or("none".to_string(), |phase| format!("{:?}", phase.as_str()))
        };
        return Some(format!(
            "the phase is {} in the Recomputed GameState and {} in the engine's",
            named(recomputed),
            named(next)
        ));
    }
    if recomputed.outcome() != next.outcome() {
        return Some("the Terminal Outcome differs".to_string());
    }
    if recomputed.pending_decision() != next.pending_decision() {
        return Some("the Pending Decision differs".to_string());
    }
    if recomputed.stack() != next.stack() {
        return Some(format!(
            "the Stack holds {} items in the Recomputed GameState and {} in the engine's, or they differ",
            recomputed.stack().len(),
            next.stack().len()
        ));
    }
    let (left, right) = (recomputed.cards(), next.cards());
    if left.len() != right.len() {
        return Some(format!(
            "the Recomputed GameState has {} card instances and the engine's has {}",
            left.len(),
            right.len()
        ));
    }
    for (folded, carried) in left.iter().zip(right) {
        if folded == carried {
            continue;
        }
        let id = carried.id().0;
        let entry = folded
            .state()
            .iter()
            .zip(carried.state())
            .find(|(one, other)| one != other);
        return Some(match entry {
            Some(((name, folded_value), (_, carried_value))) => format!(
                "card instance {id}: its state `{}` is {folded_value:?} in the Recomputed GameState and {carried_value:?} in the engine's",
                name.as_str()
            ),
            None => format!(
                "card instance {id} is {:?} of Player {} in the Recomputed GameState and {:?} of Player {} in the engine's",
                folded.definition().as_str(),
                folded.owner().0,
                carried.definition().as_str(),
                carried.owner().0
            ),
        });
    }
    for &player in next.player_order().players() {
        let (folded, carried) = (
            recomputed.player_state(player).unwrap_or_default(),
            next.player_state(player).unwrap_or_default(),
        );
        if folded != carried {
            return Some(
                match folded.iter().zip(carried).find(|(one, other)| one != other) {
                    Some(((state, folded_value), (_, carried_value))) => format!(
                        "the Player state `{}` of Player {} is {folded_value:?} in the Recomputed GameState and {carried_value:?} in the engine's",
                        state.as_str(),
                        player.0
                    ),
                    None => format!(
                        "Player {} has {} Player state values in the Recomputed GameState and {} in the engine's",
                        player.0,
                        folded.len(),
                        carried.len()
                    ),
                },
            );
        }
        if recomputed.zones(player) != next.zones(player) {
            let zone = next
                .zones(player)
                .unwrap_or_default()
                .iter()
                .find(|held| recomputed.zone(player, held.zone()) != Some(held.cards()));
            return Some(match zone {
                Some(held) => format!(
                    "zone `{}` of Player {} holds {:?} in the Recomputed GameState and {:?} in the engine's",
                    held.zone().as_str(),
                    player.0,
                    recomputed.zone(player, held.zone()).map(ids),
                    ids(held.cards())
                ),
                None => format!("the zones of Player {} differ", player.0),
            });
        }
    }
    Some("the Random State or another part of the GameState differs".to_string())
}

/// The violations among `results`, which are given in ticket order.
fn collect(results: [(Invariant, Option<String>); 7]) -> Vec<InvariantViolation> {
    results
        .into_iter()
        .filter_map(|(invariant, detail)| {
            detail.map(|detail| InvariantViolation {
                invariant,
                viewer: None,
                detail,
            })
        })
        .collect()
}

// ---- 1. no card gained or lost

/// Across a transition: the next GameState has the same instances, owners and definitions.
fn cards_changed(prior: &GameState, next: &GameState) -> Option<String> {
    let (before, after) = (prior.cards(), next.cards());
    if before.len() != after.len() {
        return Some(format!(
            "the card count went from {} to {}",
            before.len(),
            after.len()
        ));
    }
    before
        .iter()
        .zip(after)
        .find(|(old, new)| {
            old.id() != new.id()
                || old.owner() != new.owner()
                || old.definition() != new.definition()
        })
        .map(|(old, new)| {
            format!(
                "card instance {} was {:?} of Player {} and is now {:?} of Player {}",
                old.id().0,
                old.definition().as_str(),
                old.owner().0,
                new.definition().as_str(),
                new.owner().0
            )
        })
}

/// At the opening: each seat owns, per Card Id, as many instances as its Deck List totals.
fn opening_cards_differ(decks: &[DeckList], state: &GameState) -> Option<String> {
    let mut held: BTreeMap<u32, BTreeMap<&CardId, u64>> = BTreeMap::new();
    for card in state.cards() {
        let count = held
            .entry(card.owner().0)
            .or_default()
            .entry(card.definition())
            .or_insert(0);
        *count = count.saturating_add(1);
    }
    let mut seats: BTreeSet<u32> = held.keys().copied().collect();
    let mut wanted: BTreeMap<u32, BTreeMap<&CardId, u64>> = BTreeMap::new();
    for (seat, deck) in decks.iter().enumerate() {
        let Ok(seat) = u32::try_from(seat) else {
            return Some(format!("seat {seat} does not fit a Player id"));
        };
        seats.insert(seat);
        let totals = wanted.entry(seat).or_default();
        for entry in deck.inputs().iter().flat_map(|input| input.entries()) {
            let total = totals.entry(entry.card()).or_insert(0);
            *total = total.saturating_add(u64::from(entry.count().get()));
        }
    }
    let none = BTreeMap::new();
    for seat in seats {
        let have = held.get(&seat).unwrap_or(&none);
        let need = wanted.get(&seat).unwrap_or(&none);
        if have != need {
            let cards: BTreeSet<&&CardId> = have.keys().chain(need.keys()).collect();
            let offender = cards
                .into_iter()
                .find(|card| have.get(**card) != need.get(**card));
            return Some(match offender {
                Some(card) => format!(
                    "seat {seat} holds {} of `{}` and its Deck List has {}",
                    have.get(*card).copied().unwrap_or(0),
                    card.as_str(),
                    need.get(*card).copied().unwrap_or(0)
                ),
                None => format!("seat {seat} does not hold the cards of its Deck List"),
            });
        }
    }
    None
}

// ---- 2. each instance in exactly one zone

/// One pass over the zones and one over the cards, no map: the cards are counted in a list indexed
/// by Card Instance Id, which the engine makes dense from 0 in creation order (`GameState::apply`
/// refuses an id out of sequence). The first offender is the lowest id, as before.
fn not_in_one_zone(state: &GameState) -> Option<String> {
    let cards = state.cards();
    let mut appearances = vec![0_usize; cards.len()];
    let mut unknown: Option<CardInstanceId> = None;
    for &player in state.player_order().players() {
        for held in state.zones(player).unwrap_or_default() {
            for &card in held.cards() {
                match usize::try_from(card.0)
                    .ok()
                    .and_then(|position| appearances.get_mut(position))
                {
                    Some(count) => *count = count.saturating_add(1),
                    None => unknown = Some(unknown.map_or(card, |first| first.min(card))),
                }
            }
        }
    }
    for (position, (instance, &count)) in cards.iter().zip(&appearances).enumerate() {
        let id = instance.id();
        if usize::try_from(id.0).ok() != Some(position) {
            return Some(format!(
                "card instance {} is at position {position}: ids are not dense from 0 in creation order",
                id.0
            ));
        }
        if count != 1 {
            return Some(format!("card instance {} is in {count} zones, not 1", id.0));
        }
    }
    unknown.map(|card| format!("a zone holds the unknown card instance {}", card.0))
}

// ---- 3. ordered zones keep their order rules

fn zone_order_broken(
    ruleset: &DeclarativeRuleset,
    prior: &GameState,
    events: &[Event],
    next: &GameState,
) -> Option<String> {
    let is_ordered = |zone: &ZoneId| {
        ruleset
            .definition()
            .zone(zone)
            .is_some_and(|declared| declared.ordered())
    };
    for event in events {
        if let Event::ZoneShuffled { player, zone, .. } = event {
            if !is_ordered(zone) {
                return Some(format!(
                    "the unordered zone `{}` of Player {} was shuffled",
                    zone.as_str(),
                    player.0
                ));
            }
        }
    }
    for &player in prior.player_order().players() {
        for held in prior.zones(player).unwrap_or_default() {
            let zone = held.zone();
            if !is_ordered(zone) {
                continue;
            }
            let folded = match fold_zone(player, zone, held.cards(), events) {
                Ok(folded) => folded,
                Err(detail) => return Some(detail),
            };
            if next.zone(player, zone) != Some(folded.as_slice()) {
                return Some(format!(
                    "the Events leave zone `{}` of Player {} as {:?}, but the next GameState has {:?}",
                    zone.as_str(),
                    player.0,
                    ids(&folded),
                    next.zone(player, zone).map(ids)
                ));
            }
        }
    }
    None
}

fn ids(cards: &[CardInstanceId]) -> Vec<u32> {
    cards.iter().map(|card| card.0).collect()
}

/// The cards `zone` of `player` holds after `events`, starting from `start`, by the order rules:
/// a card that leaves is removed and the others keep their order, a card that arrives goes on the
/// Top, a shuffle must be a permutation and replaces the list.
fn fold_zone(
    player: PlayerId,
    zone: &ZoneId,
    start: &[CardInstanceId],
    events: &[Event],
) -> Result<Vec<CardInstanceId>, String> {
    let mut cards = start.to_vec();
    for event in events {
        match event {
            Event::CardCreated {
                card,
                owner,
                zone: placed,
                ..
            } if *owner == player && placed == zone => cards.push(*card),
            Event::CardMoved {
                card,
                player: mover,
                from,
                to,
            } if *mover == player => {
                if from == zone {
                    cards.retain(|member| member != card);
                }
                if to == zone {
                    cards.push(*card);
                }
            }
            Event::ZoneShuffled {
                player: shuffled,
                zone: shuffled_zone,
                order,
                ..
            } if *shuffled == player && shuffled_zone == zone => {
                let mut before = cards.clone();
                let mut after = order.clone();
                before.sort();
                after.sort();
                if before != after {
                    return Err(format!(
                        "the shuffle of zone `{}` of Player {} is not a permutation of its cards",
                        zone.as_str(),
                        player.0
                    ));
                }
                cards.clone_from(order);
            }
            _ => {}
        }
    }
    Ok(cards)
}

// ---- 4. instances carry only the states their type declares

/// The copies of a card are numbered one after the other, so the Card Type resolved for the
/// previous card is the one of the next while the Card Definition is equal: each definition is
/// resolved once per run of equal definitions, not twice per card (ADR-0021). A definition that
/// cannot be resolved is reported at the first card that has it, as before.
fn instance_states_wrong(ruleset: &DeclarativeRuleset, state: &GameState) -> Option<String> {
    let mut resolved: Option<(&CardId, &CardType)> = None;
    for card in state.cards() {
        let id = card.id().0;
        let card_type = match resolved {
            Some((definition, card_type)) if definition == card.definition() => card_type,
            _ => {
                let Some(definition) = ruleset.catalog().card(card.definition()) else {
                    return Some(format!(
                        "card instance {id} is of `{}`, which is not in the catalog",
                        card.definition().as_str()
                    ));
                };
                let Some(card_type) = ruleset.definition().card_type(definition.card_type()) else {
                    return Some(format!(
                        "card instance {id} is of the Card Type `{}`, which the ruleset does not declare",
                        definition.card_type().as_str()
                    ));
                };
                resolved = Some((card.definition(), card_type));
                card_type
            }
        };
        let schema = card_type.instance_state();
        let matches = card.state().len() == schema.len()
            && card
                .state()
                .iter()
                .zip(schema)
                .all(|((name, value), declared)| {
                    name == declared.name() && value.field_type() == declared.state_type()
                });
        if !matches {
            let held: Vec<&str> = card.state().iter().map(|(name, _)| name.as_str()).collect();
            let declared: Vec<&str> = schema.iter().map(|entry| entry.name().as_str()).collect();
            return Some(format!(
                "card instance {id} of `{}` carries the states {held:?} (or their types) but its type declares {declared:?}",
                card_type.id().as_str()
            ));
        }
    }
    None
}

// ---- 5. event-sourced consistency

fn trace_does_not_give(prior: &GameState, events: &[Event], next: &GameState) -> Option<String> {
    match prior.apply_trace(events) {
        Ok(applied) if applied == *next => None,
        Ok(_) => Some("applying the trace to the prior GameState gives another GameState".into()),
        Err(error) => Some(format!(
            "the trace does not apply to the prior GameState: {error}"
        )),
    }
}

// ---- 6. an ended Game has no Legal Actions

fn ended_with_legal_actions(ruleset: &DeclarativeRuleset, state: &GameState) -> Option<String> {
    state.outcome()?;
    let legal = ruleset.legal_actions(state);
    (!legal.is_empty()).then(|| format!("the Game has ended but has {} Legal Actions", legal.len()))
}

// ---- 7. the Turn and the Active Player are consistent

/// The Active Player must follow from the starting Player and the Turn Number: with no Turn
/// begun it is the starting Player, in Turn `n` it is at position `(start + n - 1) mod count` of
/// the Player Order.
fn turn_inconsistent(ruleset: &DeclarativeRuleset, state: &GameState) -> Option<String> {
    let players = state.player_order().players();
    let start = u64::from(ruleset.definition().parameters().starting_player().get());
    let advanced = state
        .turn_number()
        .map_or(0, |turn| u64::from(turn.get().saturating_sub(1)));
    // An empty Player Order has no Active Player to expect: that is a violation, not a pass.
    let Some(position) = u64::try_from(players.len())
        .ok()
        .and_then(|count| start.saturating_add(advanced).checked_rem(count))
    else {
        return Some(
            "the Player Order is empty, so no Player can be the Active Player".to_string(),
        );
    };
    let expected = usize::try_from(position)
        .ok()
        .and_then(|position| players.get(position))
        .copied();
    (expected != Some(state.active_player())).then(|| {
        format!(
            "the Active Player is Player {} but the starting position {start} and Turn {:?} give {:?}",
            state.active_player().0,
            state.turn_number().map(|turn| turn.get()),
            expected.map(|player| player.0)
        )
    })
}

/// Across a transition the Turn Number never decreases, and a begun Turn does not unbegin.
fn turn_went_back(prior: &GameState, next: &GameState) -> Option<String> {
    let before = prior.turn_number()?;
    match next.turn_number() {
        Some(after) if after >= before => None,
        other => Some(format!(
            "the Turn Number went from {} to {:?}",
            before.get(),
            other.map(|turn| turn.get())
        )),
    }
}

#[cfg(test)]
mod tests;
