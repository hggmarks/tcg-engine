//! The Observed GameState: what one Viewer sees of a GameState (ADR-0028).
//!
//! It holds base facts only, never a derived value (Derived Reads are offered beside it), and no
//! Card Instance Id, no Random State and no order of a zone the Viewer cannot see.

use super::error::ObservationError;
use super::observed_event::{ObservedApplyError, ObservedEvent};
use super::observer::{sees, Observer};
use super::viewer::{ObservedCard, Viewer};
use crate::card_catalog::CardId;
use crate::declarative::creation::DeclarativeRuleset;
use crate::declarative::decision::DecisionKind;
use crate::declarative::stack::{StackItem, StackSource};
use crate::declarative::state::CardInstanceId;
use crate::declarative::state::{GameState, TerminalOutcome};
use crate::player::{PlayerId, PlayerOrder};
use crate::ruleset_definition::{ActionId, FieldValue, ParamName, PhaseId, StateName, ZoneId};
use crate::turn::TurnNumber;

/// A GameState as one Viewer sees it: the facts every Viewer sees, each zone as the Viewer's
/// Observed Cards or as a count, and the Visible Cards.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObservedGameState {
    viewer: Viewer,
    player_order: PlayerOrder,
    active_player: PlayerId,
    turn_number: Option<TurnNumber>,
    phase: Option<PhaseId>,
    outcome: Option<TerminalOutcome>,
    pending: Option<ObservedPendingDecision>,
    stack: Vec<ObservedStackItem>,
    zones: Vec<(PlayerId, Vec<ObservedZone>)>,
    cards: Vec<VisibleCard>,
}

/// The decision a Resolution is paused at, as every Viewer sees it (ADR-0033): who is asked and
/// what kind. Never the Paused Resolution.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ObservedPendingDecision {
    pub decider: PlayerId,
    pub kind: DecisionKind,
}

/// One card a Stack Item bound, as every Viewer sees it (ADR-0037): the param, the zone it was in
/// once the cost was paid, and its Observed Card; `None` once the card has left, which is what
/// makes the item Fizzle.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObservedBoundCard {
    pub param: ParamName,
    pub zone: ZoneId,
    pub card: Option<ObservedCard>,
}

/// What put a Stack Item on the Stack, as every Viewer sees it (ADR-0037, ADR-0038). An ability's
/// card is an Observed Card while the card is in the Active Zone, and `None` once it has left: a
/// handle names a card only while the Viewer sees it, as for an Observed Bound Card, so the source
/// never shows a card that went to a zone the Viewer cannot see.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ObservedStackSource {
    /// A pushed Action of the ruleset.
    Action(ActionId),
    /// The activated ability at Ability Index `ability` of `card`.
    Ability {
        card: Option<ObservedCard>,
        ability: u32,
    },
}

/// A Stack Item as every Viewer sees it: the Stack is fully public (ADR-0037). The bound cards are
/// in param order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObservedStackItem {
    pub controller: PlayerId,
    pub source: ObservedStackSource,
    pub bound: Vec<ObservedBoundCard>,
}

impl ObservedStackItem {
    /// Whether the item Fizzles when it resolves: a card it bound has left.
    pub fn fizzles(&self) -> bool {
        self.bound.iter().any(|bound| bound.card.is_none())
    }
}

/// The Observed Stack Item of `item` for the Viewer of `observer`: a bound card that left is
/// `None`; any other is the Observed Card the Viewer holds, which it must (a bound card is public).
pub(super) fn observed_item(
    observer: &Observer,
    item: &StackItem,
) -> Result<ObservedStackItem, ObservationError> {
    let mut bound = Vec::with_capacity(item.bound.len());
    for held in &item.bound {
        let card = if held.left {
            None
        } else {
            Some(
                observer
                    .observed_card(held.card)
                    .ok_or(ObservationError::ObserverOutOfStep)?,
            )
        };
        bound.push(ObservedBoundCard {
            param: held.param.clone(),
            zone: held.zone.clone(),
            card,
        });
    }
    let source = match &item.source {
        StackSource::Action(id) => ObservedStackSource::Action(id.clone()),
        StackSource::Ability { card, ability } => ObservedStackSource::Ability {
            card: source_card(item, &bound, *card)?,
            ability: *ability,
        },
    };
    Ok(ObservedStackItem {
        controller: item.controller,
        source,
        bound,
    })
}

/// The Observed Card of the card that carries an ability, as its own bound card (`self`) shows it:
/// `None` once that card left. The item must bind it.
fn source_card(
    item: &StackItem,
    bound: &[ObservedBoundCard],
    card: CardInstanceId,
) -> Result<Option<ObservedCard>, ObservationError> {
    item.bound
        .iter()
        .zip(bound)
        .find(|(held, _)| held.card == card)
        .map(|(_, observed)| observed.card)
        .ok_or(ObservationError::ObserverOutOfStep)
}

impl ObservedGameState {
    /// The decision a Resolution is paused at; `None` when nothing is pending.
    pub fn pending_decision(&self) -> Option<&ObservedPendingDecision> {
        self.pending.as_ref()
    }

    /// The Stack Items waiting to resolve, bottom first; empty when no Response Window is open.
    pub fn stack(&self) -> &[ObservedStackItem] {
        &self.stack
    }

    /// The Player with Priority: the first Player in seat order other than the controller of the
    /// top item; `None` when the Stack is empty.
    pub fn priority(&self) -> Option<PlayerId> {
        let top = self.stack.last()?;
        self.player_order
            .players()
            .iter()
            .copied()
            .find(|&player| player != top.controller)
    }

    /// The Player who acts next: the one asked while a decision is pending, else the Player with
    /// Priority while a window is open, else the Active Player.
    pub fn deciding_player(&self) -> PlayerId {
        self.pending
            .as_ref()
            .map(|pending| pending.decider)
            .or_else(|| self.priority())
            .unwrap_or(self.active_player)
    }
}

impl ObservedGameState {
    /// The Viewer this is the Observed GameState for.
    pub fn viewer(&self) -> Viewer {
        self.viewer
    }

    /// The Players in seat order.
    pub fn players(&self) -> &[PlayerId] {
        self.player_order.players()
    }

    /// The Player whose Turn it is.
    pub fn active_player(&self) -> PlayerId {
        self.active_player
    }

    /// The Turn Number of the Turn in progress; `None` before the first Turn has begun.
    pub fn turn_number(&self) -> Option<TurnNumber> {
        self.turn_number
    }

    /// The phase of the Turn in progress that began last; `None` when no phase has begun.
    pub fn phase(&self) -> Option<&PhaseId> {
        self.phase.as_ref()
    }

    /// The Terminal Outcome; `None` while the Game is on.
    pub fn outcome(&self) -> Option<&TerminalOutcome> {
        self.outcome.as_ref()
    }

    /// The zones of `player` in declared order; `None` for a Player not in the Game.
    pub fn zones(&self, player: PlayerId) -> Option<&[ObservedZone]> {
        self.zones
            .iter()
            .find(|(owner, _)| *owner == player)
            .map(|(_, zones)| zones.as_slice())
    }

    /// One zone of `player`; `None` for an unknown Player or zone.
    pub fn zone(&self, player: PlayerId, zone: &ZoneId) -> Option<&ObservedZone> {
        self.zones(player)?.iter().find(|held| &held.zone == zone)
    }

    /// Every card the Viewer sees, by ascending Observed Card.
    pub fn cards(&self) -> &[VisibleCard] {
        &self.cards
    }

    /// The visible card the Viewer holds as `card`.
    pub fn card(&self, card: ObservedCard) -> Option<&VisibleCard> {
        self.cards
            .binary_search_by_key(&card, |visible| visible.card)
            .ok()
            .and_then(|position| self.cards.get(position))
    }
}

/// Where a zone is held: the position of its Player, then of the zone, in `zones`.
type ZoneAt = (usize, usize);

impl ObservedGameState {
    /// Applies one Observed Event: the facts of one Event of a transition, as this Viewer observes
    /// it (ADR-0029). It mirrors `GameState::apply`: a move takes the card out of `from` and puts
    /// it on the end of `to`; `TurnBegan` resets the phase; `GameEnded` records the Terminal
    /// Outcome, and nothing applies after it.
    ///
    /// Atomic: every check comes before the first change, so on `Err` the state is unchanged. After
    /// `GameEnded` the error is `GameAlreadyEnded`; every other misfit is `EventDoesNotFit`: an
    /// unknown Player, zone, card or state; a zone seen or unseen the other way from what the
    /// variant needs; a card not in `from`, or one that is visible already; a count that would go
    /// below zero or overflow; an order that is not a permutation; a value of the wrong type; a draw
    /// from a zone that is not empty; a phase before any Turn; or bad losses.
    pub fn apply(&mut self, event: &ObservedEvent) -> Result<(), ObservedApplyError> {
        if self.outcome.is_some() {
            return Err(ObservedApplyError::GameAlreadyEnded);
        }
        let misfit = ObservedApplyError::EventDoesNotFit;
        // While a decision is pending only its Answer, or the end of the Game, applies.
        if self.pending.is_some()
            && !matches!(
                event,
                ObservedEvent::DecisionAnswered { .. } | ObservedEvent::GameEnded { .. }
            )
        {
            return Err(misfit);
        }
        match event {
            ObservedEvent::CardCreated {
                card,
                owner,
                definition,
                zone,
                state,
            } => {
                let at = self.locate(*owner, zone).ok_or(misfit)?;
                self.seen_at(at).ok_or(misfit)?;
                let position = self.insertion_point(*card).ok_or(misfit)?;
                self.push_seen(at, *card);
                self.cards.insert(
                    position,
                    VisibleCard {
                        card: *card,
                        owner: *owner,
                        definition: definition.clone(),
                        state: state.clone(),
                    },
                );
                Ok(())
            }
            ObservedEvent::UnseenCardCreated { owner, zone } => {
                let at = self.locate(*owner, zone).ok_or(misfit)?;
                let count = self.unseen_count(at).ok_or(misfit)?;
                let count = count.checked_add(1).ok_or(misfit)?;
                self.set_unseen_count(at, count);
                Ok(())
            }
            ObservedEvent::CardMoved {
                card,
                player,
                from,
                to,
            } => {
                let from = self.locate(*player, from).ok_or(misfit)?;
                let to = self.locate(*player, to).ok_or(misfit)?;
                if !self.seen_at(from).ok_or(misfit)?.contains(card) {
                    return Err(misfit);
                }
                self.seen_at(to).ok_or(misfit)?;
                self.card(*card).ok_or(misfit)?;
                self.remove_seen(from, *card);
                self.push_seen(to, *card);
                self.mark_left(*card);
                Ok(())
            }
            ObservedEvent::CardMovedIntoSight {
                card,
                player,
                from,
                to,
                definition,
                state,
            } => {
                let from = self.locate(*player, from).ok_or(misfit)?;
                let to = self.locate(*player, to).ok_or(misfit)?;
                let remaining = self
                    .unseen_count(from)
                    .ok_or(misfit)?
                    .checked_sub(1)
                    .ok_or(misfit)?;
                self.seen_at(to).ok_or(misfit)?;
                let position = self.insertion_point(*card).ok_or(misfit)?;
                self.set_unseen_count(from, remaining);
                self.push_seen(to, *card);
                self.cards.insert(
                    position,
                    VisibleCard {
                        card: *card,
                        owner: *player,
                        definition: definition.clone(),
                        state: state.clone(),
                    },
                );
                Ok(())
            }
            ObservedEvent::CardMovedOutOfSight {
                card,
                player,
                from,
                to,
            } => {
                let from = self.locate(*player, from).ok_or(misfit)?;
                let to = self.locate(*player, to).ok_or(misfit)?;
                if !self.seen_at(from).ok_or(misfit)?.contains(card) {
                    return Err(misfit);
                }
                let grown = self
                    .unseen_count(to)
                    .ok_or(misfit)?
                    .checked_add(1)
                    .ok_or(misfit)?;
                let position = self
                    .cards
                    .binary_search_by_key(card, |visible| visible.card)
                    .map_err(|_| misfit)?;
                self.remove_seen(from, *card);
                self.set_unseen_count(to, grown);
                self.cards.remove(position);
                self.mark_left(*card);
                Ok(())
            }
            ObservedEvent::UnseenCardMoved { player, from, to } => {
                let from = self.locate(*player, from).ok_or(misfit)?;
                let to = self.locate(*player, to).ok_or(misfit)?;
                let remaining = self
                    .unseen_count(from)
                    .ok_or(misfit)?
                    .checked_sub(1)
                    .ok_or(misfit)?;
                let held = self.unseen_count(to).ok_or(misfit)?;
                if from == to {
                    return Ok(());
                }
                let grown = held.checked_add(1).ok_or(misfit)?;
                self.set_unseen_count(from, remaining);
                self.set_unseen_count(to, grown);
                Ok(())
            }
            ObservedEvent::ZoneShuffled {
                player,
                zone,
                order,
            } => {
                let at = self.locate(*player, zone).ok_or(misfit)?;
                let mut before = self.seen_at(at).ok_or(misfit)?.clone();
                let mut after = order.clone();
                before.sort();
                after.sort();
                if before != after {
                    return Err(misfit);
                }
                if let Some(ZoneView::Seen(cards)) = self.view_mut_at(at) {
                    cards.clone_from(order);
                }
                Ok(())
            }
            ObservedEvent::UnseenZoneShuffled { player, zone } => {
                let at = self.locate(*player, zone).ok_or(misfit)?;
                self.unseen_count(at).ok_or(misfit)?;
                Ok(())
            }
            ObservedEvent::CardStateChanged { card, state, value } => {
                let position = self
                    .cards
                    .binary_search_by_key(card, |visible| visible.card)
                    .map_err(|_| misfit)?;
                let visible = self.cards.get_mut(position).ok_or(misfit)?;
                let entry = visible
                    .state
                    .iter_mut()
                    .find(|(entry, _)| entry == state)
                    .map(|(_, current)| current)
                    .ok_or(misfit)?;
                if entry.field_type() != value.field_type() {
                    return Err(misfit);
                }
                entry.clone_from(value);
                Ok(())
            }
            ObservedEvent::TurnBegan { player, turn } => {
                if !self.stack.is_empty() || !self.player_order.contains(*player) {
                    return Err(misfit);
                }
                self.active_player = *player;
                self.turn_number = Some(*turn);
                self.phase = None;
                Ok(())
            }
            ObservedEvent::PhaseBegan { phase } => {
                if !self.stack.is_empty() || self.turn_number.is_none() {
                    return Err(misfit);
                }
                self.phase = Some(phase.clone());
                Ok(())
            }
            ObservedEvent::DrawFromEmptyZone { player, zone } => {
                let at = self.locate(*player, zone).ok_or(misfit)?;
                let count = self
                    .view_at(at)
                    .map(|view| match view {
                        ZoneView::Seen(cards) => cards.len(),
                        ZoneView::Unseen { count } => *count,
                    })
                    .ok_or(misfit)?;
                if count != 0 {
                    return Err(misfit);
                }
                Ok(())
            }
            ObservedEvent::GameEnded { losses } => {
                let outcome =
                    TerminalOutcome::of(&self.player_order, losses).map_err(|_| misfit)?;
                self.outcome = Some(outcome);
                self.pending = None;
                self.stack.clear();
                Ok(())
            }
            ObservedEvent::DecisionPending { decider, kind } => {
                if !self.player_order.contains(*decider) {
                    return Err(misfit);
                }
                self.pending = Some(ObservedPendingDecision {
                    decider: *decider,
                    kind: *kind,
                });
                Ok(())
            }
            ObservedEvent::DecisionAnswered { player, .. } => {
                let pending = self.pending.as_ref().ok_or(misfit)?;
                if pending.decider != *player {
                    return Err(misfit);
                }
                self.pending = None;
                Ok(())
            }
            ObservedEvent::ItemPushed { item } => {
                // A card of the item that is not left is a card the Viewer sees, and the item has
                // not Fizzled yet.
                let sees_every_card = item
                    .bound
                    .iter()
                    .all(|bound| bound.card.is_some_and(|card| self.card(card).is_some()));
                // An ability's card is seen too, and is one of the bound cards.
                let source_fits = match &item.source {
                    ObservedStackSource::Action(_) => true,
                    ObservedStackSource::Ability { card, .. } => {
                        card.is_some_and(|card| self.card(card).is_some())
                            && item.bound.iter().any(|bound| bound.card == *card)
                    }
                };
                if !self.player_order.contains(item.controller) || !sees_every_card || !source_fits
                {
                    return Err(misfit);
                }
                self.stack.push(item.clone());
                Ok(())
            }
            ObservedEvent::ItemResolved { item } => self.pop_top(item, false),
            ObservedEvent::ItemFizzled { item } => self.pop_top(item, true),
        }
    }

    /// Pops the top item, which must be `item`, and Fizzle or not as `fizzles` says.
    fn pop_top(
        &mut self,
        item: &ObservedStackItem,
        fizzles: bool,
    ) -> Result<(), ObservedApplyError> {
        let misfit = ObservedApplyError::EventDoesNotFit;
        if self.stack.last() != Some(item) || item.fizzles() != fizzles {
            return Err(misfit);
        }
        self.stack.pop();
        Ok(())
    }

    /// A card the Viewer saw moved: every Stack Item that bound it shows it as gone.
    fn mark_left(&mut self, card: ObservedCard) {
        for item in &mut self.stack {
            for bound in &mut item.bound {
                if bound.card == Some(card) {
                    bound.card = None;
                }
            }
            if let ObservedStackSource::Ability { card: source, .. } = &mut item.source {
                if *source == Some(card) {
                    *source = None;
                }
            }
        }
    }

    /// Applies `events` in order to a clone of this state and returns the result; this state is
    /// untouched. Stops at the first Observed Event that does not apply.
    pub fn apply_events(
        &self,
        events: &[ObservedEvent],
    ) -> Result<ObservedGameState, ObservedApplyError> {
        let mut next = self.clone();
        for event in events {
            next.apply(event)?;
        }
        Ok(next)
    }

    fn locate(&self, player: PlayerId, zone: &ZoneId) -> Option<ZoneAt> {
        let player_at = self.zones.iter().position(|(owner, _)| *owner == player)?;
        let zone_at = self
            .zones
            .get(player_at)?
            .1
            .iter()
            .position(|held| &held.zone == zone)?;
        Some((player_at, zone_at))
    }

    fn view_at(&self, at: ZoneAt) -> Option<&ZoneView> {
        self.zones
            .get(at.0)?
            .1
            .get(at.1)
            .map(|observed| &observed.view)
    }

    fn view_mut_at(&mut self, at: ZoneAt) -> Option<&mut ZoneView> {
        self.zones
            .get_mut(at.0)?
            .1
            .get_mut(at.1)
            .map(|observed| &mut observed.view)
    }

    /// The Observed Cards of a zone the Viewer sees.
    fn seen_at(&self, at: ZoneAt) -> Option<&Vec<ObservedCard>> {
        match self.view_at(at)? {
            ZoneView::Seen(cards) => Some(cards),
            ZoneView::Unseen { .. } => None,
        }
    }

    /// The count of a zone the Viewer cannot see.
    fn unseen_count(&self, at: ZoneAt) -> Option<usize> {
        match self.view_at(at)? {
            ZoneView::Seen(_) => None,
            ZoneView::Unseen { count } => Some(*count),
        }
    }

    fn push_seen(&mut self, at: ZoneAt, card: ObservedCard) {
        if let Some(ZoneView::Seen(cards)) = self.view_mut_at(at) {
            cards.push(card);
        }
    }

    fn remove_seen(&mut self, at: ZoneAt, card: ObservedCard) {
        if let Some(ZoneView::Seen(cards)) = self.view_mut_at(at) {
            cards.retain(|member| *member != card);
        }
    }

    fn set_unseen_count(&mut self, at: ZoneAt, count: usize) {
        if let Some(ZoneView::Unseen { count: held }) = self.view_mut_at(at) {
            *held = count;
        }
    }

    /// Where a card the Viewer does not see yet goes in `cards`, which stays sorted by handle;
    /// `None` when the Viewer sees it already.
    fn insertion_point(&self, card: ObservedCard) -> Option<usize> {
        self.cards
            .binary_search_by_key(&card, |visible| visible.card)
            .err()
    }
}

/// What the Viewer sees of one zone: its cards, or only how many there are.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ObservedZone {
    zone: ZoneId,
    view: ZoneView,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum ZoneView {
    /// The Viewer sees the cards, bottom first.
    Seen(Vec<ObservedCard>),
    /// The Viewer sees only the count: not which cards, nor their order.
    Unseen { count: usize },
}

impl ObservedZone {
    /// The zone.
    pub fn zone(&self) -> &ZoneId {
        &self.zone
    }

    /// How many cards the zone holds, whether or not the Viewer sees them.
    pub fn count(&self) -> usize {
        match &self.view {
            ZoneView::Seen(cards) => cards.len(),
            ZoneView::Unseen { count } => *count,
        }
    }

    /// The Observed Cards of the zone, bottom first; `None` when the Viewer cannot see the zone.
    pub fn cards(&self) -> Option<&[ObservedCard]> {
        match &self.view {
            ZoneView::Seen(cards) => Some(cards),
            ZoneView::Unseen { .. } => None,
        }
    }
}

/// A card the Viewer sees: its Observed Card, its owner, its Card Definition and its instance
/// state as stored. Fields of the definition are read from the catalog, and Derived Reads are asked
/// of the Ruleset; neither is held here.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VisibleCard {
    card: ObservedCard,
    owner: PlayerId,
    definition: CardId,
    state: Vec<(StateName, FieldValue)>,
}

impl VisibleCard {
    /// The Observed Card the Viewer holds for it.
    pub fn card(&self) -> ObservedCard {
        self.card
    }

    /// The Player who owns the card.
    pub fn owner(&self) -> PlayerId {
        self.owner
    }

    /// The Card Definition it was made from.
    pub fn definition(&self) -> &CardId {
        &self.definition
    }

    /// The instance state in the order of the card type's schema.
    pub fn state(&self) -> &[(StateName, FieldValue)] {
        &self.state
    }
}

/// The Observed GameState of `state` for the Viewer of `observer`.
pub(super) fn observe(
    ruleset: &DeclarativeRuleset,
    observer: &Observer,
    state: &GameState,
) -> Result<ObservedGameState, ObservationError> {
    let viewer = observer.viewer();
    let Viewer::Player(me) = viewer;
    if !state.player_order().contains(me) {
        return Err(ObservationError::UnknownViewer { viewer });
    }

    let mut zones = Vec::new();
    let mut seen_total = 0usize;
    for &owner in state.player_order().players() {
        let held = state
            .zones(owner)
            .ok_or(ObservationError::ObserverOutOfStep)?;
        let mut observed = Vec::new();
        for stored in held {
            let view = if sees(ruleset, viewer, owner, stored.zone())? {
                let mut handles = Vec::new();
                for &id in stored.cards() {
                    let handle = observer
                        .observed_card(id)
                        .ok_or(ObservationError::ObserverOutOfStep)?;
                    handles.push(handle);
                }
                seen_total = seen_total
                    .checked_add(handles.len())
                    .ok_or(ObservationError::ObserverOutOfStep)?;
                ZoneView::Seen(handles)
            } else {
                ZoneView::Unseen {
                    count: stored.cards().len(),
                }
            };
            observed.push(ObservedZone {
                zone: stored.zone().clone(),
                view,
            });
        }
        zones.push((owner, observed));
    }
    // Every seen card has a handle, so the handles of the seen cards are distinct; the Observer
    // holds no other.
    if seen_total != observer.held() {
        return Err(ObservationError::ObserverOutOfStep);
    }

    let mut stack = Vec::with_capacity(state.stack().len());
    for item in state.stack() {
        stack.push(observed_item(observer, item)?);
    }

    let mut cards = Vec::new();
    for (handle, id) in observer.held_cards() {
        let instance = state.card(id).ok_or(ObservationError::ObserverOutOfStep)?;
        cards.push(VisibleCard {
            card: handle,
            owner: instance.owner(),
            definition: instance.definition().clone(),
            state: instance.state().to_vec(),
        });
    }

    Ok(ObservedGameState {
        viewer,
        player_order: state.player_order().clone(),
        active_player: state.active_player(),
        turn_number: state.turn_number(),
        phase: state.phase().cloned(),
        outcome: state.outcome().cloned(),
        pending: state
            .pending_decision()
            .map(|pending| ObservedPendingDecision {
                decider: pending.decider(),
                kind: pending.kind(),
            }),
        stack,
        zones,
        cards,
    })
}
