//! The correctness net over Observations (ADR-0031): the three Invariants that hold for every
//! Viewer at every step, and the Relabelling the leak check is made of.
//!
//! - *Observations stay consistent*: the Observed Events of a transition, applied to the previous
//!   Observed GameState, give the next one ([`check_observed_consistency`]).
//! - *Observations ignore hidden cards*: what a Viewer observes does not change when the cards it
//!   cannot see are relabelled in the GameState and the trace alike ([`check_hidden_cards_ignored`],
//!   fed by [`hidden_cards`], [`relabel`] and [`replay_observed_step`]).
//! - *Observed Actions round-trip*: translating a Viewer's Observed Legal Actions back gives its
//!   Actions from `legal_actions`, in the same order ([`check_observed_actions`]).
//!
//! Like the rest of the crate it uses only the public API of `tcg-engine`. Nothing here reads the
//! Observer except through its public accessors, and the Hidden Cards are computed from the Events
//! and the zones' Visibility, never from the Observer, so the check does not trust what it tests.

use std::collections::{BTreeMap, BTreeSet};

use tcg_engine::card_catalog::CardId;
use tcg_engine::declarative::{
    Action, Answer, Argument, CardInstanceId, DeclarativeRuleset, Event, GameState,
    ObservationError, ObservedCard, ObservedEvent, ObservedGameState, Observer, TakenAction,
    Viewer,
};
use tcg_engine::player::PlayerId;
use tcg_engine::random::RandomState;
use tcg_engine::ruleset::{EventTrace, Transition};
use tcg_engine::ruleset_definition::{FieldName, FieldValue, StateName, Visibility, ZoneId};

use crate::invariants::{Invariant, InvariantViolation};

/// The constant the Relabelling's generator is derived with: one
/// `RandomState::from_seed(Seed(seed.0 ^ RELABEL_STREAM))` per Game, separate from the GameState's
/// Random State and from the policy's ([`POLICY_STREAM`](crate::POLICY_STREAM)). The Relabelling
/// never touches either of those, so verifying a record reproduces the same Relabelling.
pub const RELABEL_STREAM: u64 = 0x7265_6c61_6265_6c31;

/// What one Viewer observes at one step: the Opening Position (the creation trace) or one accepted
/// transition.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObservedStep {
    pub viewer: Viewer,
    /// The Observed Events of the creation trace, or of the transition.
    pub events: Vec<ObservedEvent>,
    /// The Taken Action; `None` at the Opening Position.
    pub taken: Option<TakenAction>,
    pub state: ObservedGameState,
    /// For every Visible Card in handle order, every `FieldValue::Int` field of its Card Definition
    /// in catalog order, through `derived_read`.
    pub derived_reads: Vec<(ObservedCard, FieldName, i64)>,
}

/// The Events of a Game so far, in order: the creation trace, then the trace of each accepted
/// transition. The last trace is the current step's, or the creation is when there is none.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct GameEvents {
    pub(crate) creation: Vec<Event>,
    pub(crate) traces: Vec<Vec<Event>>,
}

impl GameEvents {
    /// Every Event of the Game so far, in order.
    fn all(&self) -> impl Iterator<Item = &Event> {
        self.creation.iter().chain(self.traces.iter().flatten())
    }
}

/// A Game replayed from the Empty GameState to the end of one step, and the Observed Step of a
/// Viewer there: the Relabelled Game when the Events were relabelled.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ReplayedStep {
    /// The GameState at the end of the step, folded from the Empty GameState.
    pub(crate) state: GameState,
    pub(crate) step: ObservedStep,
}

/// The Events and the Action of a step after the Relabelling.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Relabelled {
    pub(crate) events: GameEvents,
    pub(crate) action: Option<Action>,
}

/// Whether `viewer` sees the cards of `owner`'s zone `zone` (ADR-0004, ADR-0028): `public` every
/// Viewer, `owner` that zone's Player only, `hidden` nobody, its owner included. A zone the
/// Ruleset does not declare is not seen, and neither is any Viewer this crate does not know.
fn sees(ruleset: &DeclarativeRuleset, viewer: Viewer, owner: PlayerId, zone: &ZoneId) -> bool {
    let Some(declared) = ruleset.definition().zone(zone) else {
        return false;
    };
    match (declared.visibility(), viewer) {
        (Visibility::Public, _) => true,
        (Visibility::Owner, Viewer::Player(player)) => player == owner,
        _ => false,
    }
}

/// The Hidden Cards of `viewer` after `events`: the cards that were never in a zone the Viewer
/// sees after any Event of the Game so far, the step's own trace included. A card is seen when a
/// `CardCreated` puts it in a seen zone, or a `CardMoved` has a seen `from` or `to`.
///
/// They are computed from the Events and the zones' Visibility, never from the Observer.
pub(crate) fn hidden_cards(
    ruleset: &DeclarativeRuleset,
    viewer: Viewer,
    events: &GameEvents,
) -> BTreeSet<CardInstanceId> {
    let mut created = BTreeSet::new();
    let mut seen = BTreeSet::new();
    for event in events.all() {
        match event {
            Event::CardCreated {
                card, owner, zone, ..
            } => {
                created.insert(*card);
                if sees(ruleset, viewer, *owner, zone) {
                    seen.insert(*card);
                }
            }
            Event::CardMoved {
                card,
                player,
                from,
                to,
            } if sees(ruleset, viewer, *player, from) || sees(ruleset, viewer, *player, to) => {
                seen.insert(*card);
            }
            _ => {}
        }
    }
    created.difference(&seen).copied().collect()
}

/// One of the places a `CardCreated` can be: a list of Events and a position in it.
type At = (usize, usize);

/// Relabels the cards of `to_relabel` in `events` and `action`. `to_relabel` is the set of Hidden
/// Cards in a real check; a test passes a seen card to see what a leak looks like.
///
/// It draws from `rng` in this fixed order, with Fisher-Yates:
/// 1. *Definitions*: the `CardCreated.definition` values of those cards are shuffled among them.
/// 2. *Ids*: within each class of those cards created in the creation trace with the same owner
///    and the same creation zone, the classes by smallest id, the ids are shuffled. Ids stay dense
///    in creation order and a moved card still starts in its owner's zone. Each `CardCreated`
///    keeps its position and id and takes the definition and state of the card that is renamed to
///    that id; every other Event, and the Action's arguments, map their ids through the same
///    permutation.
/// 3. *Unseen order*: in each `ZoneShuffled` of a zone `viewer` cannot see, in Event order, the
///    positions that hold those cards are shuffled among themselves. A `ZoneShuffled` carried in
///    the `unscanned` Events of a `DecisionPending` is visited at the position of that Event, the
///    carried Events in their own order, and shuffled by the same code (ADR-0036).
pub(crate) fn relabel(
    ruleset: &DeclarativeRuleset,
    viewer: Viewer,
    to_relabel: &BTreeSet<CardInstanceId>,
    events: &GameEvents,
    action: Option<&Action>,
    rng: &mut RandomState,
) -> Relabelled {
    let mut out = events.clone();
    let mut lists: Vec<&mut Vec<Event>> = std::iter::once(&mut out.creation)
        .chain(out.traces.iter_mut())
        .collect();

    // 1. Definitions, among every `CardCreated` of a card to relabel.
    let mut slots: Vec<At> = Vec::new();
    let mut definitions: Vec<CardId> = Vec::new();
    for (list, events) in lists.iter().enumerate() {
        for (index, event) in events.iter().enumerate() {
            if let Event::CardCreated {
                card, definition, ..
            } = event
            {
                if to_relabel.contains(card) {
                    slots.push((list, index));
                    definitions.push(definition.clone());
                }
            }
        }
    }
    rng.shuffle(&mut definitions);
    for (&(list, index), shuffled) in slots.iter().zip(definitions) {
        if let Some(Event::CardCreated { definition, .. }) = lists[list].get_mut(index) {
            *definition = shuffled;
        }
    }

    // 2. Ids, within the classes of the creation trace.
    let mut classes: BTreeMap<(PlayerId, ZoneId), Vec<CardInstanceId>> = BTreeMap::new();
    let mut created: BTreeMap<CardInstanceId, (CardId, Vec<(StateName, FieldValue)>)> =
        BTreeMap::new();
    for event in lists[0].iter() {
        if let Event::CardCreated {
            card,
            owner,
            definition,
            zone,
            state,
        } = event
        {
            if to_relabel.contains(card) {
                classes
                    .entry((*owner, zone.clone()))
                    .or_default()
                    .push(*card);
                created.insert(*card, (definition.clone(), state.clone()));
            }
        }
    }
    let mut classes: Vec<Vec<CardInstanceId>> = classes.into_values().collect();
    for class in &mut classes {
        class.sort();
    }
    classes.sort_by_key(|class| class.first().copied());
    let mut permutation: BTreeMap<CardInstanceId, CardInstanceId> = BTreeMap::new();
    let mut renamed_from: BTreeMap<CardInstanceId, CardInstanceId> = BTreeMap::new();
    for class in &classes {
        let mut shuffled = class.clone();
        rng.shuffle(&mut shuffled);
        for (&card, &renamed) in class.iter().zip(&shuffled) {
            permutation.insert(card, renamed);
            renamed_from.insert(renamed, card);
        }
    }
    let map = |card: CardInstanceId| permutation.get(&card).copied().unwrap_or(card);
    for (list, events) in lists.iter_mut().enumerate() {
        for event in events.iter_mut() {
            match event {
                Event::CardCreated {
                    card,
                    definition,
                    state,
                    ..
                } if list == 0 => {
                    if let Some((own_definition, own_state)) =
                        renamed_from.get(card).and_then(|from| created.get(from))
                    {
                        *definition = own_definition.clone();
                        *state = own_state.clone();
                    }
                }
                other => relabel_ids(other, &map),
            }
        }
    }
    let action = action.map(|action| match action {
        Action::Declared {
            player,
            action,
            arguments,
        } => Action::Declared {
            player: *player,
            action: action.clone(),
            arguments: arguments
                .iter()
                .map(|&Argument::Card(card)| Argument::Card(map(card)))
                .collect(),
        },
        // A Pass names no card.
        Action::Pass { .. } => action.clone(),
        // An option Answer names no card; a card Answer names one, through the same permutation.
        Action::Answer {
            player,
            answer: Answer::Card(card),
        } => Action::Answer {
            player: *player,
            answer: Answer::Card(map(*card)),
        },
        Action::Answer {
            answer: Answer::Option(_),
            ..
        } => action.clone(),
    });

    // 3. The order of the zones the Viewer cannot see.
    for events in lists.iter_mut() {
        for event in events.iter_mut() {
            match event {
                Event::DecisionPending { paused, .. } => {
                    for carried in &mut paused.unscanned {
                        reshuffle_unseen_order(ruleset, viewer, to_relabel, carried, rng);
                    }
                }
                other => reshuffle_unseen_order(ruleset, viewer, to_relabel, other, rng),
            }
        }
    }

    Relabelled {
        events: out,
        action,
    }
}

/// Step 3 of the Relabelling for one Event: if it is a `ZoneShuffled` of a zone `viewer` cannot
/// see, the positions that hold cards of `to_relabel` are shuffled among themselves. Any other
/// Event, and a shuffle of a zone the Viewer sees, is left as it is and draws nothing.
fn reshuffle_unseen_order(
    ruleset: &DeclarativeRuleset,
    viewer: Viewer,
    to_relabel: &BTreeSet<CardInstanceId>,
    event: &mut Event,
    rng: &mut RandomState,
) {
    if let Event::ZoneShuffled {
        player,
        zone,
        order,
        ..
    } = event
    {
        if sees(ruleset, viewer, *player, zone) {
            return;
        }
        let held: Vec<usize> = (0..order.len())
            .filter(|&index| to_relabel.contains(&order[index]))
            .collect();
        let mut cards: Vec<CardInstanceId> = held.iter().map(|&index| order[index]).collect();
        rng.shuffle(&mut cards);
        for (&index, card) in held.iter().zip(cards) {
            order[index] = card;
        }
    }
}

/// Maps every card id an Event names through `map`. The match is exhaustive, so a new Event must
/// say whether it names cards. A `CardCreated` is relabelled by position, where it is created, not
/// here. The Paused Resolution of a `DecisionPending` names cards too, its trigger, its queue, the
/// Events it carries (ADR-0033) and the cards of its bound names, and so does the card of a Card
/// Answer (ADR-0034): they go through the same permutation.
fn relabel_ids(event: &mut Event, map: &impl Fn(CardInstanceId) -> CardInstanceId) {
    match event {
        Event::CardMoved { card, .. } | Event::CardStateChanged { card, .. } => {
            *card = map(*card);
        }
        Event::ZoneShuffled { order, .. } => {
            for card in order.iter_mut() {
                *card = map(*card);
            }
        }
        Event::DecisionPending { paused, .. } => {
            paused.trigger.card = map(paused.trigger.card);
            for queued in &mut paused.queue {
                queued.card = map(queued.card);
            }
            for carried in &mut paused.unscanned {
                relabel_ids(carried, map);
            }
            for bound in &mut paused.bound {
                bound.card = map(bound.card);
            }
        }
        Event::DecisionAnswered {
            answer: Answer::Card(card),
            ..
        } => {
            *card = map(*card);
        }
        // The Stack Items name the cards they bound (ADR-0037).
        Event::ItemPushed { item } | Event::ItemResolved { item } | Event::ItemFizzled { item } => {
            for bound in &mut item.bound {
                bound.card = map(bound.card);
            }
        }
        Event::CardCreated { .. }
        | Event::TurnBegan { .. }
        | Event::PhaseBegan { .. }
        | Event::DrawFromEmptyZone { .. }
        | Event::GameEnded { .. }
        | Event::DecisionAnswered {
            answer: Answer::Option(_),
            ..
        } => {}
    }
}

/// The Observed Step of one step, from what the Observer followed.
///
/// `observer` has followed the step, `state` is the GameState after it, `observed` is the Observed
/// GameState after it. Derived Reads are taken of every Visible Card in handle order: every int
/// field of its Card Definition, in catalog order.
pub(crate) fn observed_step(
    ruleset: &DeclarativeRuleset,
    observer: &Observer,
    state: &GameState,
    events: Vec<ObservedEvent>,
    taken: Option<TakenAction>,
    observed: ObservedGameState,
) -> Result<ObservedStep, ObservationError> {
    let mut derived_reads = Vec::new();
    for card in observed.cards() {
        let definition = ruleset
            .catalog()
            .card(card.definition())
            .ok_or(ObservationError::ObserverOutOfStep)?;
        for (field, value) in definition.fields() {
            if matches!(value, FieldValue::Int(_)) {
                let read = ruleset.derived_read(observer, state, card.card(), field)?;
                derived_reads.push((card.card(), field.clone(), read));
            }
        }
    }
    Ok(ObservedStep {
        viewer: observer.viewer(),
        events,
        taken,
        state: observed,
        derived_reads,
    })
}

/// The Observed Step of `viewer` at the end of the last step of `events`, built from scratch: the
/// Events folded from `empty` (`apply_trace`), a fresh Observer from `empty` over every Event
/// before the step, then the step itself followed with `action` (`None` at the Opening Position,
/// where the step is the creation trace).
///
/// An error is the text that names it: the Events do not apply, or the Observer cannot follow
/// them.
pub(crate) fn replay_observed_step(
    ruleset: &DeclarativeRuleset,
    viewer: Viewer,
    empty: &GameState,
    events: &GameEvents,
    action: Option<&Action>,
) -> Result<ReplayedStep, String> {
    use tcg_engine::ruleset::Observations;

    let observation = |error: ObservationError| {
        format!("the relabelled Game cannot be observed: {error} ({error:?})")
    };
    let apply = |from: &GameState, trace: &[Event]| {
        from.apply_trace(trace)
            .map_err(|error| format!("the relabelled Events do not apply: {error}"))
    };
    let mut observer = ruleset.observer(viewer, empty, &[]).map_err(observation)?;
    let (state, observed_events, taken, observed) = match events.traces.split_last() {
        None => {
            let observed_events = ruleset
                .follow_events(&mut observer, empty, &events.creation)
                .map_err(observation)?;
            let state = apply(empty, &events.creation)?;
            let observed = ruleset.observe(&observer, &state).map_err(observation)?;
            (state, observed_events, None, observed)
        }
        Some((last, earlier)) => {
            let Some(action) = action else {
                return Err("the relabelled step has Events but no Action".to_string());
            };
            let before: Vec<Event> = events
                .creation
                .iter()
                .chain(earlier.iter().flatten())
                .cloned()
                .collect();
            ruleset
                .follow_events(&mut observer, empty, &before)
                .map_err(observation)?;
            let previous = apply(empty, &before)?;
            let next = apply(&previous, last)?;
            let transition = Transition {
                state: next.clone(),
                trace: EventTrace::new(last.clone()),
            };
            let followed = ruleset
                .follow_transition(&mut observer, &previous, action, &transition)
                .map_err(observation)?;
            (next, followed.events, Some(followed.action), followed.state)
        }
    };
    let step = observed_step(ruleset, &observer, &state, observed_events, taken, observed)
        .map_err(observation)?;
    Ok(ReplayedStep { state, step })
}

fn violation(invariant: Invariant, viewer: Viewer, detail: String) -> Vec<InvariantViolation> {
    vec![InvariantViolation {
        invariant,
        viewer: Some(viewer),
        detail,
    }]
}

/// Observations stay consistent: applying `events`, the Observed Events of a transition, to
/// `previous`, the Viewer's previous Observed GameState, gives `next`. A violation names the first
/// part that differs, or the Observed Event that does not apply.
pub fn check_observed_consistency(
    viewer: Viewer,
    previous: &ObservedGameState,
    events: &[ObservedEvent],
    next: &ObservedGameState,
) -> Vec<InvariantViolation> {
    let detail = match previous.apply_events(events) {
        Err(error) => Some(format!(
            "the Observed Events do not apply to the previous Observed GameState: {error}"
        )),
        Ok(applied) => observed_state_difference(&applied, "applying the Observed Events", next, "the next one")
            .map(|difference| {
                format!("applying the Observed Events does not give the next Observed GameState: {difference}")
            }),
    };
    detail
        .map(|detail| violation(Invariant::ObservationsStayConsistent, viewer, detail))
        .unwrap_or_default()
}

/// Observations ignore hidden cards: the Observed Step of a Viewer, and the Observed Step of the
/// Relabelled Game, must be equal. A violation names the first observed part that differs: an
/// Observed Event, the Taken Action, a field of the Observed GameState, or a Derived Read.
pub fn check_hidden_cards_ignored(
    observed: &ObservedStep,
    relabelled: &ObservedStep,
) -> Vec<InvariantViolation> {
    observed_step_difference(observed, relabelled)
        .map(|difference| {
            violation(
                Invariant::ObservationsIgnoreHiddenCards,
                observed.viewer,
                format!(
                    "the Observed Step changes when the Viewer's hidden cards are relabelled: {difference}"
                ),
            )
        })
        .unwrap_or_default()
}

/// The first part in which two Observed Steps differ; the second is the relabelled one.
fn observed_step_difference(observed: &ObservedStep, relabelled: &ObservedStep) -> Option<String> {
    if let Some(index) = observed
        .events
        .iter()
        .zip(&relabelled.events)
        .position(|(one, other)| one != other)
    {
        return Some(format!(
            "the Observed Event at index {index} differs: {:?}, and {:?} when relabelled",
            observed.events[index], relabelled.events[index]
        ));
    }
    if observed.events.len() != relabelled.events.len() {
        return Some(format!(
            "the number of Observed Events differs: {}, and {} when relabelled",
            observed.events.len(),
            relabelled.events.len()
        ));
    }
    if observed.taken != relabelled.taken {
        return Some(format!(
            "the Taken Action differs: {:?}, and {:?} when relabelled",
            observed.taken, relabelled.taken
        ));
    }
    if let Some(difference) =
        observed_state_difference(&observed.state, "observed", &relabelled.state, "relabelled")
    {
        return Some(format!("the Observed GameState differs: {difference}"));
    }
    if let Some(index) = observed
        .derived_reads
        .iter()
        .zip(&relabelled.derived_reads)
        .position(|(one, other)| one != other)
    {
        let (card, field, read) = &observed.derived_reads[index];
        return Some(format!(
            "the Derived Read of {card:?} {} is {read}, and {:?} when relabelled",
            field.as_str(),
            relabelled.derived_reads[index].2
        ));
    }
    (observed.derived_reads.len() != relabelled.derived_reads.len()).then(|| {
        format!(
            "the number of Derived Reads differs: {}, and {} when relabelled",
            observed.derived_reads.len(),
            relabelled.derived_reads.len()
        )
    })
}

/// The first part in which two Observed GameStates differ, or `None` when they are equal: the
/// Turn Number, the Active Player, the phase, the outcome, a Player's zone (its count, then its
/// cards), then the Visible Cards (their number, then one's owner, definition or state).
fn observed_state_difference(
    one: &ObservedGameState,
    one_name: &str,
    other: &ObservedGameState,
    other_name: &str,
) -> Option<String> {
    if one == other {
        return None;
    }
    if one.turn_number() != other.turn_number() {
        return Some(format!(
            "the Turn Number is {:?} in {one_name} and {:?} in {other_name}",
            one.turn_number().map(|turn| turn.get()),
            other.turn_number().map(|turn| turn.get())
        ));
    }
    if one.active_player() != other.active_player() {
        return Some(format!(
            "the Active Player is {:?} in {one_name} and {:?} in {other_name}",
            one.active_player(),
            other.active_player()
        ));
    }
    if one.phase() != other.phase() {
        return Some(format!(
            "the phase is {:?} in {one_name} and {:?} in {other_name}",
            one.phase(),
            other.phase()
        ));
    }
    if one.outcome() != other.outcome() {
        return Some(format!(
            "the outcome is {:?} in {one_name} and {:?} in {other_name}",
            one.outcome(),
            other.outcome()
        ));
    }
    if one.pending_decision() != other.pending_decision() {
        return Some(format!(
            "the Pending Decision is {:?} in {one_name} and {:?} in {other_name}",
            one.pending_decision(),
            other.pending_decision()
        ));
    }
    if one.stack() != other.stack() {
        return Some(format!(
            "the Stack is {:?} in {one_name} and {:?} in {other_name}",
            one.stack(),
            other.stack()
        ));
    }
    if one.players() != other.players() {
        return Some(format!(
            "the Players are {:?} in {one_name} and {:?} in {other_name}",
            one.players(),
            other.players()
        ));
    }
    for &player in one.players() {
        let (zones, other_zones) = (
            one.zones(player).unwrap_or_default(),
            other.zones(player).unwrap_or_default(),
        );
        if zones.len() != other_zones.len() {
            return Some(format!(
                "Player {} has {} zones in {one_name} and {} in {other_name}",
                player.0,
                zones.len(),
                other_zones.len()
            ));
        }
        for (zone, other_zone) in zones.iter().zip(other_zones) {
            if zone.zone() != other_zone.zone() {
                return Some(format!(
                    "Player {}'s zones are {:?} in {one_name} and {:?} in {other_name}",
                    player.0,
                    zone.zone(),
                    other_zone.zone()
                ));
            }
            if zone.count() != other_zone.count() {
                return Some(format!(
                    "the zone `{}` of Player {} holds {} cards in {one_name} and {} in {other_name}",
                    zone.zone().as_str(),
                    player.0,
                    zone.count(),
                    other_zone.count()
                ));
            }
            if zone.cards() != other_zone.cards() {
                return Some(format!(
                    "the zone `{}` of Player {} holds {:?} in {one_name} and {:?} in {other_name}",
                    zone.zone().as_str(),
                    player.0,
                    zone.cards(),
                    other_zone.cards()
                ));
            }
        }
    }
    let (cards, other_cards) = (one.cards(), other.cards());
    if cards.len() != other_cards.len() {
        return Some(format!(
            "there are {} Visible Cards in {one_name} and {} in {other_name}",
            cards.len(),
            other_cards.len()
        ));
    }
    for (card, other_card) in cards.iter().zip(other_cards) {
        if card.card() != other_card.card() {
            return Some(format!(
                "the Visible Card {:?} in {one_name} is {:?} in {other_name}",
                card.card(),
                other_card.card()
            ));
        }
        if card.owner() != other_card.owner() {
            return Some(format!(
                "the Visible Card {:?} has owner {:?} in {one_name} and {:?} in {other_name}",
                card.card(),
                card.owner(),
                other_card.owner()
            ));
        }
        if card.definition() != other_card.definition() {
            return Some(format!(
                "the Visible Card {:?} has definition {:?} in {one_name} and {:?} in {other_name}",
                card.card(),
                card.definition(),
                other_card.definition()
            ));
        }
        if card.state() != other_card.state() {
            return Some(format!(
                "the Visible Card {:?} has state {:?} in {one_name} and {:?} in {other_name}",
                card.card(),
                card.state(),
                other_card.state()
            ));
        }
    }
    Some("they differ".to_string())
}

/// Observed Actions round-trip: the Observer's Observed Legal Actions in `state`, translated back,
/// are exactly the Actions of `legal` whose Player is the Viewer, in the same order and of the
/// same number. `legal` is the Ruleset's listing of `state`, the one the policy draws from; the
/// Observer has followed `state`. An `ObservationError` from either call is the violation. A
/// violation names the first index at which the two lists differ.
pub fn check_observed_actions(
    ruleset: &DeclarativeRuleset,
    observer: &Observer,
    state: &GameState,
    legal: &[Action],
) -> Vec<InvariantViolation> {
    let viewer = observer.viewer();
    let report = |detail: String| violation(Invariant::ObservedActionsRoundTrip, viewer, detail);
    let Viewer::Player(me) = viewer else {
        return report("the Viewer is not a Player".to_string());
    };
    let listed = match ruleset.observed_legal_actions(observer, state) {
        Ok(listed) => listed,
        Err(error) => {
            return report(format!(
                "the Observed Legal Actions cannot be listed: {error} ({error:?})"
            ));
        }
    };
    let mut translated = Vec::with_capacity(listed.len());
    for (index, observed) in listed.iter().enumerate() {
        match ruleset.translate_action(observer, observed) {
            Ok(action) => translated.push(action),
            Err(error) => {
                return report(format!(
                    "the Observed Legal Action at index {index} cannot be translated back: {error} ({error:?})"
                ));
            }
        }
    }
    let expected: Vec<&Action> = legal
        .iter()
        .filter(|action| action.player() == me)
        .collect();
    for index in 0..translated.len().max(expected.len()) {
        match (translated.get(index), expected.get(index)) {
            (Some(back), Some(listed)) if back == *listed => {}
            (Some(back), Some(listed)) => {
                return report(format!(
                    "the Legal Action at index {index} is {listed:?}, and translating the Observed Legal Action gives {back:?}"
                ));
            }
            (Some(back), None) => {
                return report(format!(
                    "the Legal Action at index {index} is missing, and translating the Observed Legal Action gives {back:?}"
                ));
            }
            (None, Some(listed)) => {
                return report(format!(
                    "the Legal Action at index {index} is {listed:?}, and it has no Observed Legal Action"
                ));
            }
            (None, None) => {}
        }
    }
    Vec::new()
}

#[cfg(test)]
mod tests;
