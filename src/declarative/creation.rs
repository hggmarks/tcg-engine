//! Game creation: the opening position of a declarative Game.
//!
//! Creation starts from the Empty GameState and applies Events, one at a time, each generated from
//! the state the previous one produced (ADR-0010). First every card of every Deck List is created
//! (seat 0 first; within a seat, the ruleset's Deck Inputs in order, then the Deck List entries in
//! file order, each entry `count` times), then the Setup Steps run (ADR-0011).

use std::fmt;

use super::event::Event;
use super::state::{ApplyError, CardInstanceId, GameState};
use crate::card_catalog::{CardCatalog, CardId};
use crate::deck_list::DeckList;
use crate::player::{PlayerId, PlayerOrder};
use crate::random::{RandomState, Seed};
use crate::ruleset::{EventTrace, Ruleset, Transition};
use crate::ruleset_definition::{
    FieldValue, Initializer, PlayerSelector, RulesetDefinition, RulesetId, RulesetVersion,
    SetupStep, StateName, ZoneId,
};

/// A Ruleset that interprets a [`RulesetDefinition`] and a [`CardCatalog`] (ADR-0004, ADR-0005).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeclarativeRuleset {
    definition: RulesetDefinition,
    catalog: CardCatalog,
}

/// An Action of the declarative Ruleset. It has no variants until ticket [10] gives Actions a
/// representation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {}

/// The reason the declarative Ruleset refuses an Action. It has no variants until ticket [10].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Rejection {}

/// The outcome of Game creation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GameCreation {
    /// The Empty GameState: Players by seat, the Active Player, the Random State of the Seed and
    /// every zone empty.
    pub empty: GameState,
    /// The Opening Position and the Event Trace that, applied to `empty`, produces it.
    pub opening: Transition<GameState, Event>,
}

/// Why a Game cannot be created. The first error wins, in this order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GameCreationError {
    /// The catalog was written for another ruleset id or version; these are the catalog's own.
    CatalogForAnotherRuleset {
        ruleset: RulesetId,
        ruleset_version: RulesetVersion,
    },
    /// The number of Deck Lists is not the number of Players.
    WrongDeckCount { players: u32, decks: usize },
    /// The Deck List of `player` has inputs that differ from the ruleset's Deck Inputs (ids or
    /// order), or lists a card that is not in the catalog.
    DeckListMismatch { player: PlayerId },
    /// The card's Card Type, or a field its instance state copies, is missing from the definition
    /// or the card.
    CardDoesNotFitDefinition { card: CardId },
    /// There are more card instances than a `CardInstanceId` can number.
    TooManyCards,
    /// A setup draw found its source empty. `step` is the index in the ruleset's setup.
    SetupDrawFromEmptyZone {
        step: usize,
        player: PlayerId,
        zone: ZoneId,
    },
    /// An Event creation generated was rejected: an internal inconsistency, never expected.
    EventRejected(ApplyError),
}

impl fmt::Display for GameCreationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::CatalogForAnotherRuleset {
                ruleset,
                ruleset_version,
            } => write!(
                f,
                "the catalog is for the ruleset `{}` version {}",
                ruleset.as_str(),
                ruleset_version.get()
            ),
            Self::WrongDeckCount { players, decks } => {
                write!(f, "{players} Players need {players} Deck Lists, found {decks}")
            }
            Self::DeckListMismatch { player } => write!(
                f,
                "the Deck List of Player {} does not match the ruleset's Deck Inputs and the catalog",
                player.0
            ),
            Self::CardDoesNotFitDefinition { card } => write!(
                f,
                "the card `{}` does not fit the ruleset definition",
                card.as_str()
            ),
            Self::TooManyCards => write!(f, "there are too many cards to number"),
            Self::SetupDrawFromEmptyZone { step, player, zone } => write!(
                f,
                "setup step {step}: zone `{}` of Player {} is empty",
                zone.as_str(),
                player.0
            ),
            Self::EventRejected(error) => write!(f, "an Event was rejected: {error}"),
        }
    }
}

impl std::error::Error for GameCreationError {}

impl DeclarativeRuleset {
    /// Builds the Ruleset from a validated definition and a catalog. A catalog for another ruleset
    /// is reported by [`DeclarativeRuleset::create_game`].
    pub fn new(definition: RulesetDefinition, catalog: CardCatalog) -> Self {
        Self {
            definition,
            catalog,
        }
    }

    /// The Ruleset Definition.
    pub fn definition(&self) -> &RulesetDefinition {
        &self.definition
    }

    /// The Card Catalog.
    pub fn catalog(&self) -> &CardCatalog {
        &self.catalog
    }

    /// Creates a Game: the Empty GameState and the Opening Position with its setup Event Trace.
    /// `decks[i]` belongs to seat `i`. Creation trusts that each Deck List is legal; it checks only
    /// what it needs in order to place cards and never to panic.
    pub fn create_game(
        &self,
        decks: &[DeckList],
        seed: Seed,
    ) -> Result<GameCreation, GameCreationError> {
        let definition = &self.definition;

        let meta = self.catalog.meta();
        if meta.ruleset() != definition.meta().id()
            || meta.ruleset_version() != definition.meta().version()
        {
            return Err(GameCreationError::CatalogForAnotherRuleset {
                ruleset: meta.ruleset().clone(),
                ruleset_version: meta.ruleset_version(),
            });
        }

        let player_count = definition.parameters().player_count();
        if usize::try_from(player_count.get()).ok() != Some(decks.len()) {
            return Err(GameCreationError::WrongDeckCount {
                players: player_count.get(),
                decks: decks.len(),
            });
        }

        let order = PlayerOrder::by_seat(player_count);
        for (&player, deck) in order.players().iter().zip(decks) {
            if !self.deck_matches(deck) {
                return Err(GameCreationError::DeckListMismatch { player });
            }
        }

        let zone_ids: Vec<ZoneId> = definition
            .zones()
            .iter()
            .map(|zone| zone.id().clone())
            .collect();
        let active_player = PlayerId(definition.parameters().starting_player().get());
        let empty = GameState::empty(
            order.clone(),
            active_player,
            &zone_ids,
            RandomState::from_seed(seed),
        );

        let mut creation = Creation {
            state: empty.clone(),
            events: Vec::new(),
        };
        for (&player, deck) in order.players().iter().zip(decks) {
            self.create_cards(&mut creation, player, deck)?;
        }
        self.run_setup(&mut creation)?;

        Ok(GameCreation {
            empty,
            opening: Transition {
                state: creation.state,
                trace: EventTrace::new(creation.events),
            },
        })
    }

    /// Whether the Deck List has exactly the ruleset's Deck Inputs, in order, and only cards of the
    /// catalog.
    fn deck_matches(&self, deck: &DeckList) -> bool {
        let declared = self.definition.deck_inputs();
        deck.inputs().len() == declared.len()
            && deck.inputs().iter().zip(declared).all(|(provided, input)| {
                provided.input() == input.id()
                    && provided
                        .entries()
                        .iter()
                        .all(|entry| self.catalog.card(entry.card()).is_some())
            })
    }

    /// Creates the cards of one Player's Deck List, onto the zone of their Deck Input.
    fn create_cards(
        &self,
        creation: &mut Creation,
        player: PlayerId,
        deck: &DeckList,
    ) -> Result<(), GameCreationError> {
        for (provided, input) in deck.inputs().iter().zip(self.definition.deck_inputs()) {
            for entry in provided.entries() {
                let card = entry.card();
                let state = self.initial_state(card)?;
                for _ in 0..entry.count().get() {
                    let id = creation.next_card_id()?;
                    creation.apply(Event::CardCreated {
                        card: id,
                        owner: player,
                        definition: card.clone(),
                        zone: input.zone().clone(),
                        state: state.clone(),
                    })?;
                }
            }
        }
        Ok(())
    }

    /// The instance state of a new card of Card Definition `card`, in its card type's schema order.
    fn initial_state(
        &self,
        card: &CardId,
    ) -> Result<Vec<(StateName, FieldValue)>, GameCreationError> {
        let misfit = || GameCreationError::CardDoesNotFitDefinition { card: card.clone() };
        let definition = self.catalog.card(card).ok_or_else(misfit)?;
        let card_type = self
            .definition
            .card_type(definition.card_type())
            .ok_or_else(misfit)?;
        let mut state = Vec::with_capacity(card_type.instance_state().len());
        for schema in card_type.instance_state() {
            let value = match schema.init() {
                Initializer::Literal(value) => value.clone(),
                Initializer::FromField(field) => {
                    definition.field(field).cloned().ok_or_else(misfit)?
                }
            };
            if value.field_type() != schema.state_type() {
                return Err(misfit());
            }
            state.push((schema.name().clone(), value));
        }
        Ok(state)
    }

    /// Runs the Setup Steps in order; each step runs fully for one Player before the next.
    fn run_setup(&self, creation: &mut Creation) -> Result<(), GameCreationError> {
        for (step_index, step) in self.definition.setup().iter().enumerate() {
            match step {
                SetupStep::Shuffle { player, zone } => {
                    for player in creation.select(*player) {
                        let mut order = creation.cards_of(player, zone)?;
                        let mut random_state = creation.state.random_state().clone();
                        random_state.shuffle(&mut order);
                        creation.apply(Event::ZoneShuffled {
                            player,
                            zone: zone.clone(),
                            order,
                            random_state,
                        })?;
                    }
                }
                SetupStep::Draw {
                    player,
                    from,
                    to,
                    count,
                } => {
                    for player in creation.select(*player) {
                        for _ in 0..count.get() {
                            let Some(top) = creation.cards_of(player, from)?.last().copied() else {
                                return Err(GameCreationError::SetupDrawFromEmptyZone {
                                    step: step_index,
                                    player,
                                    zone: from.clone(),
                                });
                            };
                            creation.apply(Event::CardMoved {
                                card: top,
                                player,
                                from: from.clone(),
                                to: to.clone(),
                            })?;
                        }
                    }
                }
            }
        }
        Ok(())
    }
}

/// The state being built and the Events applied to it so far.
struct Creation {
    state: GameState,
    events: Vec<Event>,
}

impl Creation {
    /// Applies `event` to the state and records it in the trace.
    fn apply(&mut self, event: Event) -> Result<(), GameCreationError> {
        self.state
            .apply(&event)
            .map_err(GameCreationError::EventRejected)?;
        self.events.push(event);
        Ok(())
    }

    /// The id the next created card gets.
    fn next_card_id(&self) -> Result<CardInstanceId, GameCreationError> {
        u32::try_from(self.state.cards().len())
            .map(CardInstanceId)
            .map_err(|_| GameCreationError::TooManyCards)
    }

    /// The Players a selector chooses, in seat order.
    fn select(&self, selector: PlayerSelector) -> Vec<PlayerId> {
        match selector {
            PlayerSelector::Each => self.state.player_order().players().to_vec(),
        }
    }

    /// The cards of a zone, bottom first.
    fn cards_of(
        &self,
        player: PlayerId,
        zone: &ZoneId,
    ) -> Result<Vec<CardInstanceId>, GameCreationError> {
        self.state
            .zone(player, zone)
            .map(<[CardInstanceId]>::to_vec)
            .ok_or_else(|| {
                GameCreationError::EventRejected(ApplyError::UnknownZone {
                    player,
                    zone: zone.clone(),
                })
            })
    }
}

impl Ruleset for DeclarativeRuleset {
    type State = GameState;
    type Action = Action;
    type Event = Event;
    type Rejection = Rejection;

    fn transition(
        &self,
        _state: &GameState,
        action: &Action,
    ) -> Result<Transition<GameState, Event>, Rejection> {
        match *action {}
    }
}
