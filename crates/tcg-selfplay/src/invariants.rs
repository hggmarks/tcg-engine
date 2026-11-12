//! The engine invariants checked after every transition (ADR-0020).
//!
//! The checks use only the public API of `tcg-engine`, and none of its helpers that already assume
//! an invariant: they walk `GameState::zones` and `GameState::cards` themselves and never use
//! `GameState::zone_of`, which would hide a card that is in two zones.
//!
//! [`check_transition`] and [`check_opening`] return every violated [`Invariant`], at most once
//! each and in the order of the enum (the ticket's order). The `detail` of a violation names the
//! first offender found for that Invariant.

use std::collections::{BTreeMap, BTreeSet};

use tcg_engine::card_catalog::CardId;
use tcg_engine::deck_list::DeckList;
use tcg_engine::declarative::{CardInstanceId, DeclarativeRuleset, Event, GameCreation, GameState};
use tcg_engine::player::PlayerId;
use tcg_engine::ruleset::{EventTrace, LegalActions};
use tcg_engine::ruleset_definition::ZoneId;

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
}

/// One violated [`Invariant`], with a human-readable account of the first offender.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InvariantViolation {
    pub invariant: Invariant,
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

/// The violations among `results`, which are given in ticket order.
fn collect(results: [(Invariant, Option<String>); 7]) -> Vec<InvariantViolation> {
    results
        .into_iter()
        .filter_map(|(invariant, detail)| {
            detail.map(|detail| InvariantViolation { invariant, detail })
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

fn not_in_one_zone(state: &GameState) -> Option<String> {
    let mut appearances: BTreeMap<CardInstanceId, usize> = BTreeMap::new();
    for &player in state.player_order().players() {
        for held in state.zones(player).unwrap_or_default() {
            for &card in held.cards() {
                let count = appearances.entry(card).or_insert(0);
                *count = count.saturating_add(1);
            }
        }
    }
    let known: BTreeSet<CardInstanceId> = state.cards().iter().map(|card| card.id()).collect();
    for card in &known {
        let count = appearances.get(card).copied().unwrap_or(0);
        if count != 1 {
            return Some(format!(
                "card instance {} is in {count} zones, not 1",
                card.0
            ));
        }
    }
    appearances
        .keys()
        .find(|card| !known.contains(card))
        .map(|card| format!("a zone holds the unknown card instance {}", card.0))
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

fn instance_states_wrong(ruleset: &DeclarativeRuleset, state: &GameState) -> Option<String> {
    for card in state.cards() {
        let id = card.id().0;
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
