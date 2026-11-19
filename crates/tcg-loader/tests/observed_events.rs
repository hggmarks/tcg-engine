//! Acceptance tests for ticket #64: observing transitions (ADR-0029), on the whole real Effects
//! Ruleset, its real catalog and its two real sample decks, on the Small Ruleset for the
//! consistency test, and on a test-local catalog for the card that leaves a Viewer's sight.
//!
//! The Observer of a Viewer folds each accepted transition (`follow_transition`) and yields the
//! **Observed Events** of its Event Trace, in trace order, the next Observed GameState and the
//! **Observed Transition**: the **Taken Action** (acting Player, action, card arguments as Observed
//! Cards or hidden), the Observed Events and the next Observed GameState. A card that leaves the
//! Viewer's sight is forgotten, and a card that comes back gets a fresh Observed Card.
//!
//! The oracles are written here: expected Observed Events are literals read off the Events of the
//! real traces (printed once with `Debug`, then pinned), never computed by the code under test.

use tcg_engine::card_catalog::CardId;
use tcg_engine::deck_list::DeckList;
use tcg_engine::declarative::{
    Action, Argument, CardInstanceId, DeclarativeRuleset, Event, GameCreation, GameRecord,
    GameState, Loss, ObservationError, ObservedApplyError, ObservedCard, ObservedEvent,
    ObservedGameState, ObservedTransition, Observer, TakenAction, TakenArgument, Viewer,
};
use tcg_engine::player::PlayerId;
use tcg_engine::random::Seed;
use tcg_engine::ruleset::{LegalActions, Observations, Ruleset, Transition};
use tcg_engine::ruleset_definition::{
    ActionId, FieldValue, PhaseId, StateName, WinConditionId, ZoneId,
};
use tcg_engine::turn::TurnNumber;
use tcg_loader::{load_card_catalog, load_deck_list, load_ruleset_definition};

const EFFECTS_RULESET: &str =
    include_str!("../../../rulesets/effects-ruleset/effects-ruleset.yaml");
const EFFECTS_CATALOG: &str =
    include_str!("../../../rulesets/effects-ruleset/effects-catalog.yaml");
const EFFECTS_DECK_A: &str = include_str!("../../../rulesets/effects-ruleset/decks/deck-a.yaml");
const EFFECTS_DECK_B: &str = include_str!("../../../rulesets/effects-ruleset/decks/deck-b.yaml");

const SMALL_RULESET: &str = include_str!("../../../rulesets/small-ruleset/small-ruleset.yaml");
const SMALL_CATALOG: &str = include_str!("../../../rulesets/small-ruleset/small-catalog.yaml");
const SMALL_DECK_A: &str = include_str!("../../../rulesets/small-ruleset/decks/deck-a.yaml");
const SMALL_DECK_B: &str = include_str!("../../../rulesets/small-ruleset/decks/deck-b.yaml");

/// A creature that moves itself from the field to the top of its owner's deck when it enters the
/// field. It exists only in this file: appended to the Effects catalog and swapped into a copy of
/// sample deck A, so nothing under `rulesets/` changes.
const BOOMERANG_CARD: &str = "
  - id: boomerang
    type: creature
    fields: { name: \"Boomerang\", cost: 1, power: 1 }
    effects:
      - on: { enter: field }
        do: [ { move: { card: { ref: self }, to: deck } } ]
";

/// The Seed of the Effects Games below. With the sample decks, seat 0 opens with a `pyre_imp`
/// (card 46) and seat 1 with a `battle_veteran` (card 102); the tests assert it.
const SEED: u64 = 7;
const PYRE_IMP_OF_SEAT_0: CardInstanceId = CardInstanceId(46);
const VETERAN_OF_SEAT_1: CardInstanceId = CardInstanceId(102);

/// The Seed of the Game with the test-local catalog: seat 0 opens with a `boomerang` (card 1).
const BOOMERANG_SEED: u64 = 9;
const BOOMERANG: CardInstanceId = CardInstanceId(1);

/// The Seed of the Small Ruleset Game.
const SMALL_SEED: u64 = 9;

/// A scripted Game ends well before this many accepted Actions.
const STEP_CAP: usize = 3000;

// ---- helpers

fn p(seat: u32) -> PlayerId {
    PlayerId(seat)
}

fn zone(id: &str) -> ZoneId {
    ZoneId::new(id)
}

fn viewer(seat: u32) -> Viewer {
    Viewer::Player(p(seat))
}

fn h(handle: u32) -> ObservedCard {
    ObservedCard(handle)
}

fn turn(number: u32) -> TurnNumber {
    TurnNumber::new(number).expect("a Turn Number is at least 1")
}

fn effects() -> DeclarativeRuleset {
    let definition = load_ruleset_definition(EFFECTS_RULESET).expect("the ruleset loads");
    let catalog = load_card_catalog(&definition, EFFECTS_CATALOG).expect("the catalog loads");
    DeclarativeRuleset::new(definition, catalog)
}

fn small() -> DeclarativeRuleset {
    let definition = load_ruleset_definition(SMALL_RULESET).expect("the ruleset loads");
    let catalog = load_card_catalog(&definition, SMALL_CATALOG).expect("the catalog loads");
    DeclarativeRuleset::new(definition, catalog)
}

/// The Effects Ruleset with the test-local catalog (the real one plus the boomerang), and the two
/// Deck Lists: deck A with its four `goblin_scout`s swapped for four `boomerang`s, and deck B.
fn boomerang_ruleset_and_decks() -> (DeclarativeRuleset, [String; 2]) {
    let definition = load_ruleset_definition(EFFECTS_RULESET).expect("the ruleset loads");
    let catalog = load_card_catalog(&definition, &format!("{EFFECTS_CATALOG}{BOOMERANG_CARD}"))
        .expect("the test-local catalog loads");
    let swapped = EFFECTS_DECK_A.replacen(
        "{ card: goblin_scout,   count: 4 }",
        "{ card: boomerang,      count: 4 }",
        1,
    );
    assert_ne!(swapped, EFFECTS_DECK_A, "the goblin scouts are swapped");
    (
        DeclarativeRuleset::new(definition, catalog),
        [swapped, EFFECTS_DECK_B.to_string()],
    )
}

fn decks(ruleset: &DeclarativeRuleset, yamls: [&str; 2]) -> Vec<DeckList> {
    yamls
        .iter()
        .map(|yaml| {
            load_deck_list(ruleset.definition(), ruleset.catalog(), yaml)
                .expect("the sample deck is legal")
        })
        .collect()
}

fn create(ruleset: &DeclarativeRuleset, yamls: [&str; 2], seed: u64) -> GameCreation {
    ruleset
        .create_game(&decks(ruleset, yamls), Seed(seed))
        .expect("the decks create a game")
}

fn create_effects(ruleset: &DeclarativeRuleset) -> GameCreation {
    create(ruleset, [EFFECTS_DECK_A, EFFECTS_DECK_B], SEED)
}

fn action(player: u32, id: &str, cards: &[CardInstanceId]) -> Action {
    Action {
        player: p(player),
        action: ActionId::new(id),
        arguments: cards.iter().copied().map(Argument::Card).collect(),
    }
}

fn pass(player: u32) -> Action {
    action(player, "pass_turn", &[])
}

fn play(player: u32, card: CardInstanceId) -> Action {
    action(player, "play_creature", &[card])
}

fn name(entry: &str) -> StateName {
    StateName::new(entry)
}

/// The instance state of a fresh creature, in the order of its card type's schema.
fn creature_state(tapped: bool, recently_played: bool) -> Vec<(StateName, FieldValue)> {
    vec![
        (name("tapped"), FieldValue::Bool(tapped)),
        (name("recently_played"), FieldValue::Bool(recently_played)),
    ]
}

/// The instance state of a fresh mana card.
fn mana_state() -> Vec<(StateName, FieldValue)> {
    vec![(name("tapped"), FieldValue::Bool(false))]
}

/// The card a `CardMoved`, `CardStateChanged` or `CardCreated` of `trace` at `index` names.
fn card_at(trace: &[Event], index: usize) -> CardInstanceId {
    match trace.get(index) {
        Some(
            Event::CardMoved { card, .. }
            | Event::CardStateChanged { card, .. }
            | Event::CardCreated { card, .. },
        ) => *card,
        other => panic!("trace[{index}] names no card: {other:?}"),
    }
}

/// The Observed Card an Observed Event names, if it names one.
fn card_of(event: &ObservedEvent) -> Option<ObservedCard> {
    match event {
        ObservedEvent::CardCreated { card, .. }
        | ObservedEvent::CardMoved { card, .. }
        | ObservedEvent::CardMovedIntoSight { card, .. }
        | ObservedEvent::CardMovedOutOfSight { card, .. }
        | ObservedEvent::CardStateChanged { card, .. } => Some(*card),
        ObservedEvent::UnseenCardCreated { .. }
        | ObservedEvent::UnseenCardMoved { .. }
        | ObservedEvent::ZoneShuffled { .. }
        | ObservedEvent::UnseenZoneShuffled { .. }
        | ObservedEvent::TurnBegan { .. }
        | ObservedEvent::PhaseBegan { .. }
        | ObservedEvent::DrawFromEmptyZone { .. }
        | ObservedEvent::GameEnded { .. } => None,
    }
}

/// How many Observed Cards the events have given so far: the dense sequence's next handle. A card
/// that left sight is still counted, since its handle is retired and never reused.
fn handles_given(events: &[ObservedEvent]) -> u32 {
    let given = events
        .iter()
        .filter(|event| {
            matches!(
                event,
                ObservedEvent::CardCreated { .. } | ObservedEvent::CardMovedIntoSight { .. }
            )
        })
        .count();
    u32::try_from(given).expect("few handles")
}

/// A Game followed by both seats' Observers, one transition at a time, with every Observed Event
/// each seat has been given.
struct Followed<'a> {
    ruleset: &'a DeclarativeRuleset,
    state: GameState,
    observers: [Observer; 2],
    log: [Vec<ObservedEvent>; 2],
}

impl<'a> Followed<'a> {
    /// The Game of `creation` followed from its Empty GameState through its creation trace.
    fn start(ruleset: &'a DeclarativeRuleset, creation: &GameCreation) -> Followed<'a> {
        let mut observers = [0, 1].map(|seat| {
            ruleset
                .observer(viewer(seat), &creation.empty, &[])
                .expect("the Observer is built")
        });
        let mut log = [Vec::new(), Vec::new()];
        for (observer, events) in observers.iter_mut().zip(log.iter_mut()) {
            *events = ruleset
                .follow_events(observer, &creation.empty, creation.opening.trace.events())
                .expect("the creation trace is followed");
        }
        Followed {
            ruleset,
            state: creation.opening.state.clone(),
            observers,
            log,
        }
    }

    fn observed(&self, seat: usize) -> ObservedGameState {
        self.ruleset
            .observe(&self.observers[seat], &self.state)
            .expect("the GameState is observed")
    }

    /// The Observed Card `seat` holds for `card` now.
    fn handle(&self, seat: usize, card: CardInstanceId) -> Option<ObservedCard> {
        self.observers[seat].observed_card(card)
    }

    /// Takes `action`, which must be accepted, and folds the transition into both Observers.
    fn play(&mut self, action: &Action) -> (Transition<GameState, Event>, [ObservedTransition; 2]) {
        let transition = self
            .ruleset
            .transition(&self.state, action)
            .expect("the Action is accepted");
        let followed = [0, 1].map(|seat| {
            self.ruleset
                .follow_transition(&mut self.observers[seat], &self.state, action, &transition)
                .expect("the transition is followed")
        });
        for (log, observed) in self.log.iter_mut().zip(&followed) {
            log.extend(observed.events.iter().cloned());
        }
        self.state = transition.state.clone();
        (transition, followed)
    }
}

/// A whole Game played by a fixed, deterministic policy over the Legal Actions.
struct Scripted {
    creation: GameCreation,
    decks: Vec<DeckList>,
    seed: Seed,
    steps: Vec<(Action, Transition<GameState, Event>)>,
}

impl Scripted {
    /// The GameState each step starts from, then the final one.
    fn states(&self) -> Vec<&GameState> {
        let mut states = vec![&self.creation.opening.state];
        states.extend(self.steps.iter().map(|(_, transition)| &transition.state));
        states
    }
}

/// Plays a Game to its end: at each step the policy takes the Legal Action at a position that
/// depends only on the step, so the Game is a pure function of the Ruleset, the decks and the Seed.
fn play_scripted(ruleset: &DeclarativeRuleset, yamls: [&str; 2], seed: u64) -> Scripted {
    let decks = decks(ruleset, yamls);
    let creation = ruleset
        .create_game(&decks, Seed(seed))
        .expect("the decks create a game");
    let mut state = creation.opening.state.clone();
    let mut steps = Vec::new();
    while state.outcome().is_none() {
        assert!(steps.len() < STEP_CAP, "the scripted Game does not end");
        let legal = ruleset.legal_actions(&state);
        assert!(!legal.is_empty(), "an unfinished Game has a Legal Action");
        let pick = (steps.len() * 7 + 3) % legal.len();
        let chosen = legal.into_iter().nth(pick).expect("the position is listed");
        let transition = ruleset
            .transition(&state, &chosen)
            .expect("a listed Action is accepted");
        state = transition.state.clone();
        steps.push((chosen, transition));
    }
    Scripted {
        creation,
        decks,
        seed: Seed(seed),
        steps,
    }
}

/// What `seat` observes of a scripted Game: the Observed Events of the creation, each Observed
/// Transition, and the Observer at the end.
struct Observed {
    creation_events: Vec<ObservedEvent>,
    after_creation: Observer,
    transitions: Vec<ObservedTransition>,
    observers: Vec<Observer>,
}

fn follow_all(ruleset: &DeclarativeRuleset, game: &Scripted, seat: u32) -> Observed {
    let mut observer = ruleset
        .observer(viewer(seat), &game.creation.empty, &[])
        .expect("the Observer is built");
    let creation_events = ruleset
        .follow_events(
            &mut observer,
            &game.creation.empty,
            game.creation.opening.trace.events(),
        )
        .expect("the creation trace is followed");
    let after_creation = observer.clone();
    let mut transitions = Vec::new();
    let mut observers = Vec::new();
    for ((action, transition), previous) in game.steps.iter().zip(game.states()) {
        transitions.push(
            ruleset
                .follow_transition(&mut observer, previous, action, transition)
                .expect("the transition is followed"),
        );
        observers.push(observer.clone());
    }
    Observed {
        creation_events,
        after_creation,
        transitions,
        observers,
    }
}

/// Applying each seat's Observed Events to its Observed GameState from before the transition gives
/// the Observed GameState the transition carries.
fn assert_applies(before: &[ObservedGameState; 2], seen: &[ObservedTransition; 2]) {
    for (seat, (previous, transition)) in before.iter().zip(seen).enumerate() {
        assert_eq!(
            previous.apply_events(&transition.events),
            Ok(transition.state.clone()),
            "seat {seat}"
        );
    }
}

// ---- tests

#[test]
fn an_opponents_draw_is_an_unseen_move_and_the_drawer_sees_a_new_observed_card() {
    let ruleset = effects();
    let creation = create_effects(&ruleset);
    let mut game = Followed::start(&ruleset, &creation);
    let before = [game.observed(0), game.observed(1)];
    let next = [
        u32::try_from(before[0].cards().len()).expect("few cards"),
        u32::try_from(before[1].cards().len()).expect("few cards"),
    ];
    assert_eq!(
        next,
        [handles_given(&game.log[0]), handles_given(&game.log[1])]
    );

    // Seat 0 passes: seat 1's Turn begins, draws a card and moves two mana cards into play.
    let (transition, seen) = game.play(&pass(0));
    let trace = transition.trace.events();
    assert_eq!(trace.len(), 7, "{trace:?}");
    let drawn = card_at(trace, 3);
    let mana = [card_at(trace, 4), card_at(trace, 5)];
    assert_eq!(
        trace.get(3),
        Some(&Event::CardMoved {
            card: drawn,
            player: p(1),
            from: zone("deck"),
            to: zone("hand")
        })
    );
    let drawn_definition = game
        .state
        .card(drawn)
        .expect("the drawn card exists")
        .definition()
        .clone();

    // Seat 0 sees an anonymous move at the draw's trace position; the two mana cards that enter
    // the public mana zone are new Observed Cards.
    assert_eq!(
        seen[0].events,
        vec![
            ObservedEvent::PhaseBegan {
                phase: PhaseId::new("end")
            },
            ObservedEvent::TurnBegan {
                player: p(1),
                turn: turn(2)
            },
            ObservedEvent::PhaseBegan {
                phase: PhaseId::new("start")
            },
            ObservedEvent::UnseenCardMoved {
                player: p(1),
                from: zone("deck"),
                to: zone("hand")
            },
            ObservedEvent::CardMovedIntoSight {
                card: h(next[0]),
                player: p(1),
                from: zone("mana_deck"),
                to: zone("mana_zone"),
                definition: game.state.card(mana[0]).expect("mana").definition().clone(),
                state: mana_state(),
            },
            ObservedEvent::CardMovedIntoSight {
                card: h(next[0] + 1),
                player: p(1),
                from: zone("mana_deck"),
                to: zone("mana_zone"),
                definition: game.state.card(mana[1]).expect("mana").definition().clone(),
                state: mana_state(),
            },
            ObservedEvent::PhaseBegan {
                phase: PhaseId::new("main")
            },
        ]
    );
    // The opponent's hand count rises by one, and seat 0 still cannot see it.
    let hand_before = before[0]
        .zone(p(1), &zone("hand"))
        .expect("the hand")
        .count();
    let hand_after = seen[0]
        .state
        .zone(p(1), &zone("hand"))
        .expect("the hand")
        .count();
    assert_eq!(hand_after, hand_before + 1);
    assert_eq!(
        seen[0]
            .state
            .zone(p(1), &zone("hand"))
            .and_then(|z| z.cards()),
        None
    );
    assert_eq!(
        seen[0].action,
        TakenAction {
            player: p(0),
            action: ActionId::new("pass_turn"),
            arguments: vec![]
        }
    );

    // Seat 1 sees the same draw as a new Observed Card, with the exact definition and state.
    assert_eq!(
        seen[1].events,
        vec![
            ObservedEvent::PhaseBegan {
                phase: PhaseId::new("end")
            },
            ObservedEvent::TurnBegan {
                player: p(1),
                turn: turn(2)
            },
            ObservedEvent::PhaseBegan {
                phase: PhaseId::new("start")
            },
            ObservedEvent::CardMovedIntoSight {
                card: h(next[1]),
                player: p(1),
                from: zone("deck"),
                to: zone("hand"),
                definition: drawn_definition,
                state: creature_state(false, false),
            },
            ObservedEvent::CardMovedIntoSight {
                card: h(next[1] + 1),
                player: p(1),
                from: zone("mana_deck"),
                to: zone("mana_zone"),
                definition: game.state.card(mana[0]).expect("mana").definition().clone(),
                state: mana_state(),
            },
            ObservedEvent::CardMovedIntoSight {
                card: h(next[1] + 2),
                player: p(1),
                from: zone("mana_deck"),
                to: zone("mana_zone"),
                definition: game.state.card(mana[1]).expect("mana").definition().clone(),
                state: mana_state(),
            },
            ObservedEvent::PhaseBegan {
                phase: PhaseId::new("main")
            },
        ]
    );
    let hand = seen[1].state.zone(p(1), &zone("hand")).expect("the hand");
    assert_eq!(hand.count(), hand_before + 1);
    assert_eq!(hand.cards().and_then(<[_]>::last), Some(&h(next[1])));
    let visible = seen[1]
        .state
        .card(h(next[1]))
        .expect("the drawn card is visible");
    assert_eq!(visible.owner(), p(1));
    assert_eq!(visible.state(), creature_state(false, false).as_slice());
}

#[test]
fn an_opponents_creature_enters_the_field_as_a_new_observed_card_named_by_the_taken_action() {
    let ruleset = effects();
    let creation = create_effects(&ruleset);
    let mut game = Followed::start(&ruleset, &creation);
    let veteran = VETERAN_OF_SEAT_1;
    assert_eq!(
        game.state
            .card(veteran)
            .map(|card| card.definition().clone()),
        Some(CardId::new("battle_veteran")),
        "seat 1 opens with a battle_veteran"
    );
    let _ = game.play(&pass(0));
    let in_hand = game.handle(1, veteran).expect("seat 1 holds its own hand");
    assert_eq!(game.handle(0, veteran), None);
    let next = u32::try_from(game.observed(0).cards().len()).expect("few cards");
    assert_eq!(next, handles_given(&game.log[0]));

    let (transition, seen) = game.play(&play(1, veteran));
    let mana = card_at(transition.trace.events(), 0);
    assert_eq!(transition.trace.events().len(), 3);
    let tapped = |handle: ObservedCard| ObservedEvent::CardStateChanged {
        card: handle,
        state: name("tapped"),
        value: FieldValue::Bool(true),
    };
    let played = |handle: ObservedCard| ObservedEvent::CardStateChanged {
        card: handle,
        state: name("recently_played"),
        value: FieldValue::Bool(true),
    };

    // Seat 0: the creature comes into sight in the field, with the state it has at the move: the
    // later `recently_played` change is its own Observed Event.
    let new = h(next);
    assert_eq!(
        seen[0].events,
        vec![
            tapped(game.handle(0, mana).expect("the public mana is seen")),
            ObservedEvent::CardMovedIntoSight {
                card: new,
                player: p(1),
                from: zone("hand"),
                to: zone("field"),
                definition: CardId::new("battle_veteran"),
                state: creature_state(false, false),
            },
            played(new),
        ]
    );
    assert_eq!(
        seen[0].action,
        TakenAction {
            player: p(1),
            action: ActionId::new("play_creature"),
            arguments: vec![TakenArgument::Card(new)],
        }
    );
    let visible = seen[0].state.card(new).expect("the creature is visible");
    assert_eq!(visible.card(), new);
    assert_eq!(visible.owner(), p(1));
    assert_eq!(visible.definition(), &CardId::new("battle_veteran"));
    assert_eq!(visible.state(), creature_state(false, true).as_slice());
    assert_eq!(
        seen[0]
            .state
            .zone(p(1), &zone("field"))
            .and_then(|z| z.cards()),
        Some([new].as_slice())
    );

    // Seat 1, the actor: the card keeps the handle it held in its hand, and the Taken Action names it.
    assert_eq!(
        seen[1].events,
        vec![
            tapped(game.handle(1, mana).expect("the own mana is seen")),
            ObservedEvent::CardMoved {
                card: in_hand,
                player: p(1),
                from: zone("hand"),
                to: zone("field"),
            },
            played(in_hand),
        ]
    );
    assert_eq!(
        seen[1].action,
        TakenAction {
            player: p(1),
            action: ActionId::new("play_creature"),
            arguments: vec![TakenArgument::Card(in_hand)],
        }
    );
}

#[test]
fn a_card_that_leaves_sight_and_comes_back_gets_a_fresh_observed_card() {
    let (ruleset, yamls) = boomerang_ruleset_and_decks();
    let creation = create(&ruleset, [&yamls[0], &yamls[1]], BOOMERANG_SEED);
    let mut game = Followed::start(&ruleset, &creation);
    assert_eq!(
        game.state
            .card(BOOMERANG)
            .map(|card| card.definition().clone()),
        Some(CardId::new("boomerang")),
        "seat 0 opens with a boomerang"
    );
    let old = game
        .handle(0, BOOMERANG)
        .expect("seat 0 holds its own hand");
    assert_eq!(game.handle(1, BOOMERANG), None);
    let first_for_seat_1 = handles_given(&game.log[1]);

    // Seat 0 plays it: it enters the field, then moves itself to the top of its deck.
    let before = [game.observed(0), game.observed(1)];
    let (transition, seen) = game.play(&play(0, BOOMERANG));
    assert_applies(&before, &seen);
    let mana = card_at(transition.trace.events(), 0);
    assert_eq!(transition.trace.events().len(), 4);
    let boomerang = CardId::new("boomerang");
    assert_eq!(
        seen[0].events,
        vec![
            ObservedEvent::CardStateChanged {
                card: game.handle(0, mana).expect("the public mana is seen"),
                state: name("tapped"),
                value: FieldValue::Bool(true),
            },
            ObservedEvent::CardMoved {
                card: old,
                player: p(0),
                from: zone("hand"),
                to: zone("field"),
            },
            ObservedEvent::CardStateChanged {
                card: old,
                state: name("recently_played"),
                value: FieldValue::Bool(true),
            },
            ObservedEvent::CardMovedOutOfSight {
                card: old,
                player: p(0),
                from: zone("field"),
                to: zone("deck"),
            },
        ]
    );
    // The owner no longer holds the handle, and the Taken Action still names the card it held
    // before the transition.
    assert_eq!(game.observers[0].card_instance(old), None);
    assert_eq!(game.handle(0, BOOMERANG), None);
    assert_eq!(seen[0].state.card(old), None);
    assert_eq!(
        seen[0].action.arguments,
        vec![TakenArgument::Card(old)],
        "a card the Viewer held before the transition and no longer holds"
    );

    // The other seat saw it in the field in between, under a handle it never held before or after:
    // the Taken Action hides it.
    let fresh_for_seat_1 = h(first_for_seat_1);
    assert_eq!(
        seen[1].events,
        vec![
            ObservedEvent::CardStateChanged {
                card: game.handle(1, mana).expect("the public mana is seen"),
                state: name("tapped"),
                value: FieldValue::Bool(true),
            },
            ObservedEvent::CardMovedIntoSight {
                card: fresh_for_seat_1,
                player: p(0),
                from: zone("hand"),
                to: zone("field"),
                definition: boomerang.clone(),
                state: creature_state(false, false),
            },
            ObservedEvent::CardStateChanged {
                card: fresh_for_seat_1,
                state: name("recently_played"),
                value: FieldValue::Bool(true),
            },
            ObservedEvent::CardMovedOutOfSight {
                card: fresh_for_seat_1,
                player: p(0),
                from: zone("field"),
                to: zone("deck"),
            },
        ]
    );
    assert_eq!(seen[1].action.arguments, vec![TakenArgument::HiddenCard]);
    assert_eq!(game.handle(1, BOOMERANG), None);

    // Two passes later seat 0's Turn begins and it draws the card on top of its deck: the boomerang.
    let before = [game.observed(0), game.observed(1)];
    let (_, seen) = game.play(&pass(0));
    assert_applies(&before, &seen);
    let given_before_draw = handles_given(&game.log[0]);
    assert_eq!(
        game.observed(0).cards().len() + 1,
        usize::try_from(given_before_draw).expect("few handles"),
        "the retired handle is counted as given, not as visible"
    );
    let before = [game.observed(0), game.observed(1)];
    let (_, seen) = game.play(&pass(1));
    assert_applies(&before, &seen);
    let draws: Vec<&ObservedEvent> = seen[0]
        .events
        .iter()
        .filter(|event| {
            matches!(event, ObservedEvent::CardMovedIntoSight { definition, .. } if *definition == boomerang)
        })
        .collect();
    let [ObservedEvent::CardMovedIntoSight {
        card: new,
        player,
        from,
        to,
        state,
        ..
    }] = draws.as_slice()
    else {
        panic!("one boomerang comes into sight: {draws:?}");
    };
    assert_eq!((*player, from, to), (p(0), &zone("deck"), &zone("hand")));
    // Its instance state is what it has now: played this Turn, so `recently_played` is still set.
    assert_eq!(state, &creature_state(false, true));
    assert_ne!(*new, old);
    assert_eq!(
        *new,
        h(given_before_draw),
        "the next handle in the dense sequence"
    );
    assert_eq!(game.observers[0].card_instance(*new), Some(BOOMERANG));
    assert_eq!(game.handle(0, BOOMERANG), Some(*new));

    // The retired handle never reappears, in an Observed Event or in an Observed GameState.
    assert_eq!(game.observers[0].card_instance(old), None);
    assert_eq!(seen[0].state.card(old), None);
    assert!(seen[0].state.card(*new).is_some());
    assert!(
        seen[0]
            .events
            .iter()
            .all(|event| card_of(event) != Some(old)),
        "the retired handle is named again"
    );
}

#[test]
fn a_hidden_shuffle_is_observed_with_no_order_and_no_random_state() {
    let ruleset = effects();
    let creation = create_effects(&ruleset);
    let trace = creation.opening.trace.events();
    assert!(
        trace
            .iter()
            .all(|event| !matches!(event, Event::CardStateChanged { .. })),
        "the creation trace has no event that an Observer may drop, so positions line up"
    );
    let shuffles: Vec<(usize, PlayerId, ZoneId)> = trace
        .iter()
        .enumerate()
        .filter_map(|(index, event)| match event {
            Event::ZoneShuffled { player, zone, .. } => Some((index, *player, zone.clone())),
            _ => None,
        })
        .collect();
    assert_eq!(
        shuffles
            .iter()
            .map(|(_, player, zone)| (*player, zone.as_str().to_string()))
            .collect::<Vec<_>>(),
        vec![(p(0), "deck".to_string()), (p(1), "deck".to_string())],
        "creation shuffles both decks"
    );

    let game = Followed::start(&ruleset, &creation);
    for seat in [0, 1] {
        let events = &game.log[seat];
        assert_eq!(events.len(), trace.len(), "one Observed Event per Event");
        for (index, player, zone) in &shuffles {
            assert_eq!(
                events.get(*index),
                Some(&ObservedEvent::UnseenZoneShuffled {
                    player: *player,
                    zone: zone.clone()
                }),
                "seat {seat} at trace position {index}"
            );
        }
        assert!(
            events
                .iter()
                .all(|event| !matches!(event, ObservedEvent::ZoneShuffled { .. })),
            "no deck is seen, so no order is observed"
        );
    }
}

/// Binds every field of every variant under its type. A new field, or a field of another type,
/// stops this from compiling, so a Card Instance Id or a Random State cannot slip into an Observed
/// Event, a Taken Action or its arguments unnoticed.
fn binds_every_field_with_its_type(event: &ObservedEvent) {
    match event {
        ObservedEvent::CardCreated {
            card,
            owner,
            definition,
            zone,
            state,
        } => {
            let _: &ObservedCard = card;
            let _: &PlayerId = owner;
            let _: &CardId = definition;
            let _: &ZoneId = zone;
            let _: &Vec<(StateName, FieldValue)> = state;
        }
        ObservedEvent::UnseenCardCreated { owner, zone } => {
            let _: &PlayerId = owner;
            let _: &ZoneId = zone;
        }
        ObservedEvent::CardMoved {
            card,
            player,
            from,
            to,
        } => {
            let _: &ObservedCard = card;
            let _: &PlayerId = player;
            let _: &ZoneId = from;
            let _: &ZoneId = to;
        }
        ObservedEvent::CardMovedIntoSight {
            card,
            player,
            from,
            to,
            definition,
            state,
        } => {
            let _: &ObservedCard = card;
            let _: &PlayerId = player;
            let _: &ZoneId = from;
            let _: &ZoneId = to;
            let _: &CardId = definition;
            let _: &Vec<(StateName, FieldValue)> = state;
        }
        ObservedEvent::CardMovedOutOfSight {
            card,
            player,
            from,
            to,
        } => {
            let _: &ObservedCard = card;
            let _: &PlayerId = player;
            let _: &ZoneId = from;
            let _: &ZoneId = to;
        }
        ObservedEvent::UnseenCardMoved { player, from, to } => {
            let _: &PlayerId = player;
            let _: &ZoneId = from;
            let _: &ZoneId = to;
        }
        ObservedEvent::ZoneShuffled {
            player,
            zone,
            order,
        } => {
            let _: &PlayerId = player;
            let _: &ZoneId = zone;
            let _: &Vec<ObservedCard> = order;
        }
        ObservedEvent::UnseenZoneShuffled { player, zone } => {
            let _: &PlayerId = player;
            let _: &ZoneId = zone;
        }
        ObservedEvent::CardStateChanged { card, state, value } => {
            let _: &ObservedCard = card;
            let _: &StateName = state;
            let _: &FieldValue = value;
        }
        ObservedEvent::TurnBegan { player, turn } => {
            let _: &PlayerId = player;
            let _: &TurnNumber = turn;
        }
        ObservedEvent::PhaseBegan { phase } => {
            let _: &PhaseId = phase;
        }
        ObservedEvent::DrawFromEmptyZone { player, zone } => {
            let _: &PlayerId = player;
            let _: &ZoneId = zone;
        }
        ObservedEvent::GameEnded { losses } => {
            let _: &Vec<Loss> = losses;
        }
    }
}

/// The same for the Taken Action and its arguments.
fn binds_every_taken_field_with_its_type(taken: &TakenAction) {
    let TakenAction {
        player,
        action,
        arguments,
    } = taken;
    let _: &PlayerId = player;
    let _: &ActionId = action;
    let _: &Vec<TakenArgument> = arguments;
    for argument in arguments {
        match argument {
            TakenArgument::Card(card) => {
                let _: &ObservedCard = card;
            }
            TakenArgument::HiddenCard => {}
        }
    }
}

#[test]
fn no_observed_event_carries_a_card_instance_id_or_a_random_state() {
    let ruleset = effects();
    let game = play_scripted(&ruleset, [EFFECTS_DECK_A, EFFECTS_DECK_B], SEED);
    assert!(game.steps.len() > 20, "a whole Game, not a stub");

    let mut checked = 0usize;
    for seat in [0, 1] {
        let observed = follow_all(&ruleset, &game, seat);
        let events = observed.creation_events.iter().chain(
            observed
                .transitions
                .iter()
                .flat_map(|transition| &transition.events),
        );
        for event in events {
            binds_every_field_with_its_type(event);
            let text = format!("{event:?}");
            assert!(!text.contains("CardInstanceId"), "{text}");
            assert!(!text.contains("RandomState"), "{text}");
            checked += 1;
        }
        for transition in &observed.transitions {
            binds_every_taken_field_with_its_type(&transition.action);
            let text = format!("{transition:?}");
            assert!(!text.contains("CardInstanceId"), "{text}");
            assert!(!text.contains("RandomState"), "{text}");
        }
    }
    assert!(checked > 100, "every Observed Event of the Game: {checked}");
}

#[test]
fn trigger_events_and_a_cascade_are_projected_in_trace_order() {
    let ruleset = effects();
    let creation = create_effects(&ruleset);
    let mut game = Followed::start(&ruleset, &creation);
    let pyre_imp = PYRE_IMP_OF_SEAT_0;
    assert_eq!(
        game.state
            .card(pyre_imp)
            .map(|card| card.definition().clone()),
        Some(CardId::new("pyre_imp")),
        "seat 0 opens with a pyre_imp"
    );
    let own = game.handle(0, pyre_imp).expect("seat 0 holds its own hand");
    assert_eq!(game.handle(1, pyre_imp), None);
    let next = [handles_given(&game.log[0]), handles_given(&game.log[1])];

    // Playing a pyre_imp: pay the mana, move to the field, `recently_played`, then the cascade: the
    // enter-field trigger moves it to the trash and the enter-trash trigger draws a card.
    let (transition, seen) = game.play(&play(0, pyre_imp));
    let trace = transition.trace.events();
    assert_eq!(trace.len(), 5, "{trace:?}");
    let mana = card_at(trace, 0);
    let drawn = card_at(trace, 4);
    assert_eq!(
        trace.get(3),
        Some(&Event::CardMoved {
            card: pyre_imp,
            player: p(0),
            from: zone("field"),
            to: zone("trash")
        })
    );
    assert_eq!(
        trace.get(4),
        Some(&Event::CardMoved {
            card: drawn,
            player: p(0),
            from: zone("deck"),
            to: zone("hand")
        })
    );
    let drawn_definition = game
        .state
        .card(drawn)
        .expect("the drawn card exists")
        .definition()
        .clone();
    let tapped = |handle: ObservedCard| ObservedEvent::CardStateChanged {
        card: handle,
        state: name("tapped"),
        value: FieldValue::Bool(true),
    };
    let played = |handle: ObservedCard| ObservedEvent::CardStateChanged {
        card: handle,
        state: name("recently_played"),
        value: FieldValue::Bool(true),
    };
    let to_trash = |handle: ObservedCard| ObservedEvent::CardMoved {
        card: handle,
        player: p(0),
        from: zone("field"),
        to: zone("trash"),
    };

    // Seat 0 owns the card and the draw.
    assert_eq!(
        seen[0].events,
        vec![
            tapped(game.handle(0, mana).expect("mana")),
            ObservedEvent::CardMoved {
                card: own,
                player: p(0),
                from: zone("hand"),
                to: zone("field"),
            },
            played(own),
            to_trash(own),
            ObservedEvent::CardMovedIntoSight {
                card: h(next[0]),
                player: p(0),
                from: zone("deck"),
                to: zone("hand"),
                definition: drawn_definition,
                state: creature_state(false, false),
            },
        ]
    );
    assert_eq!(
        seen[0].action,
        TakenAction {
            player: p(0),
            action: ActionId::new("play_creature"),
            arguments: vec![TakenArgument::Card(own)],
        }
    );

    // Seat 1 sees the creature arrive, change and die in the public zones and the draw as an
    // anonymous move.
    let arrived = h(next[1]);
    assert_eq!(
        seen[1].events,
        vec![
            tapped(game.handle(1, mana).expect("mana")),
            ObservedEvent::CardMovedIntoSight {
                card: arrived,
                player: p(0),
                from: zone("hand"),
                to: zone("field"),
                definition: CardId::new("pyre_imp"),
                state: creature_state(false, false),
            },
            played(arrived),
            to_trash(arrived),
            ObservedEvent::UnseenCardMoved {
                player: p(0),
                from: zone("deck"),
                to: zone("hand"),
            },
        ]
    );
    assert_eq!(
        seen[1].action,
        TakenAction {
            player: p(0),
            action: ActionId::new("play_creature"),
            arguments: vec![TakenArgument::Card(arrived)],
        }
    );
    let in_trash = seen[1]
        .state
        .zone(p(0), &zone("trash"))
        .and_then(|z| z.cards())
        .expect("the trash is public");
    assert_eq!(in_trash, [arrived].as_slice());
}

/// The consistency criterion on one Game: for each seat, the Observed Events of the creation
/// applied to the Observed GameState of the Empty GameState give that of the Opening Position;
/// each transition's Observed Events applied to the previous Observed GameState give the next one;
/// and the incremental Observer equals the Observer rebuilt from every Event so far.
fn assert_consistent(ruleset: &DeclarativeRuleset, game: &Scripted) {
    assert!(game.steps.len() > 5, "a whole Game");
    let final_state = game.states().last().copied().expect("the Game has a state");
    assert!(
        final_state.outcome().is_some(),
        "the Game was played to its end"
    );

    for seat in [0, 1] {
        let fresh = ruleset
            .observer(viewer(seat), &game.creation.empty, &[])
            .expect("the Observer is built");
        let mut observed = ruleset
            .observe(&fresh, &game.creation.empty)
            .expect("the Empty GameState is observed");

        let mut observer = fresh;
        let creation_events = ruleset
            .follow_events(
                &mut observer,
                &game.creation.empty,
                game.creation.opening.trace.events(),
            )
            .expect("the creation trace is followed");
        let opening = ruleset
            .observe(&observer, &game.creation.opening.state)
            .expect("the Opening Position is observed");
        assert_eq!(
            observed.apply_events(&creation_events),
            Ok(opening.clone()),
            "seat {seat}: the creation"
        );
        observed = opening;

        let mut all_events: Vec<Event> = game.creation.opening.trace.events().to_vec();
        for (index, ((action, transition), previous)) in
            game.steps.iter().zip(game.states()).enumerate()
        {
            let followed = ruleset
                .follow_transition(&mut observer, previous, action, transition)
                .expect("the transition is followed");
            assert_eq!(
                observed.apply_events(&followed.events),
                Ok(followed.state.clone()),
                "seat {seat}, step {index}"
            );
            observed = followed.state;

            all_events.extend(transition.trace.events().iter().cloned());
            assert_eq!(
                ruleset.observer(viewer(seat), &game.creation.empty, &all_events),
                Ok(observer.clone()),
                "seat {seat}, step {index}: the incremental Observer equals the rebuilt one"
            );
        }
        assert!(observed.outcome().is_some());
    }
}

#[test]
fn applying_observed_events_gives_the_next_observed_gamestate_on_each_ruleset() {
    let effects_ruleset = effects();
    let effects_game = play_scripted(&effects_ruleset, [EFFECTS_DECK_A, EFFECTS_DECK_B], SEED);
    assert_consistent(&effects_ruleset, &effects_game);

    let small_ruleset = small();
    let small_game = play_scripted(&small_ruleset, [SMALL_DECK_A, SMALL_DECK_B], SMALL_SEED);
    assert_consistent(&small_ruleset, &small_game);
}

#[test]
fn an_observer_rebuilt_from_the_game_record_equals_the_live_one() {
    let ruleset = effects();
    let live = play_scripted(&ruleset, [EFFECTS_DECK_A, EFFECTS_DECK_B], SEED);
    let mut record = GameRecord::new(&ruleset, &live.decks, live.seed);
    for (action, _) in &live.steps {
        record.push_accepted(action.clone());
    }
    let replay = ruleset.replay(&record).expect("the record replays");

    for seat in [0, 1] {
        let followed = follow_all(&ruleset, &live, seat);

        // Rebuilt: the creation, then each replayed transition.
        let mut observer = ruleset
            .observer(viewer(seat), &replay.creation.empty, &[])
            .expect("the Observer is built");
        let creation_events = ruleset
            .follow_events(
                &mut observer,
                &replay.creation.empty,
                replay.creation.opening.trace.events(),
            )
            .expect("the creation trace is followed");
        assert_eq!(creation_events, followed.creation_events, "seat {seat}");
        assert_eq!(
            observer, followed.after_creation,
            "seat {seat}: after creation"
        );

        let mut previous = replay.creation.opening.state.clone();
        assert_eq!(replay.traces.len(), record.actions().len());
        for (index, (action, trace)) in record.actions().iter().zip(&replay.traces).enumerate() {
            let next = previous
                .apply_trace(trace.events())
                .expect("the replayed trace applies");
            let transition = Transition {
                state: next.clone(),
                trace: trace.clone(),
            };
            let rebuilt = ruleset
                .follow_transition(&mut observer, &previous, action, &transition)
                .expect("the replayed transition is followed");
            assert_eq!(
                Some(&rebuilt),
                followed.transitions.get(index),
                "seat {seat}, step {index}: the Observed Transition"
            );
            assert_eq!(
                Some(&observer),
                followed.observers.get(index),
                "seat {seat}, step {index}: the Observer"
            );
            previous = next;
        }
        assert_eq!(&previous, &replay.state);
        let live_final = ruleset
            .observe(&observer, &replay.state)
            .expect("the final GameState is observed");
        assert_eq!(
            followed.transitions.last().map(|last| &last.state),
            Some(&live_final)
        );
    }
}

#[test]
fn a_trace_that_does_not_fit_leaves_the_observer_unchanged() {
    let ruleset = effects();
    let creation = create_effects(&ruleset);
    let start = &creation.opening.state;
    let mut observer = ruleset
        .observer(viewer(0), &creation.empty, &[])
        .expect("the Observer is built");
    ruleset
        .follow_events(
            &mut observer,
            &creation.empty,
            creation.opening.trace.events(),
        )
        .expect("the creation trace is followed");
    let before = observer.clone();

    // A trace that applies for seven Events and then does not: card 46 is in seat 0's hand, not its
    // deck. Its first Events would advance the Observer (two mana cards come into sight).
    let transition = ruleset
        .transition(start, &pass(0))
        .expect("the pass is accepted");
    let mut advanced = observer.clone();
    ruleset
        .follow_events(&mut advanced, start, transition.trace.events())
        .expect("the real trace is followed");
    assert_ne!(
        advanced, before,
        "the premise: the real trace advances the Observer"
    );

    let mut bogus = transition.trace.events().to_vec();
    bogus.push(Event::CardMoved {
        card: PYRE_IMP_OF_SEAT_0,
        player: p(0),
        from: zone("deck"),
        to: zone("hand"),
    });
    assert_eq!(
        ruleset.follow_events(&mut observer, start, &bogus),
        Err(ObservationError::ObserverOutOfStep)
    );
    assert_eq!(observer, before);

    // A transition whose next GameState is not the one its trace leads to. The Observer still stays
    // as it was.
    let mismatched = Transition {
        state: start.clone(),
        trace: transition.trace.clone(),
    };
    assert_eq!(
        ruleset.follow_transition(&mut observer, start, &pass(0), &mismatched),
        Err(ObservationError::ObserverOutOfStep)
    );
    assert_eq!(observer, before);

    // The real transition still follows from the unchanged Observer.
    let followed = ruleset
        .follow_transition(&mut observer, start, &pass(0), &transition)
        .expect("the real transition is followed");
    assert_eq!(observer, advanced);
    assert_eq!(followed.events.len(), transition.trace.events().len());
}

#[test]
fn a_transition_whose_gamestate_is_not_the_one_its_trace_leads_to_leaves_the_observer_unchanged() {
    let ruleset = effects();
    let creation = create_effects(&ruleset);
    let start = &creation.opening.state;
    let mut observer = ruleset
        .observer(viewer(0), &creation.empty, &[])
        .expect("the Observer is built");
    ruleset
        .follow_events(
            &mut observer,
            &creation.empty,
            creation.opening.trace.events(),
        )
        .expect("the creation trace is followed");
    let before = observer.clone();

    // The real trace, with a state that is the real one plus one more Event: an extra phase.
    let transition = ruleset
        .transition(start, &pass(0))
        .expect("the pass is accepted");
    let forged = Transition {
        state: transition
            .state
            .apply_trace(&[Event::PhaseBegan {
                phase: PhaseId::new("end"),
            }])
            .expect("the extra phase applies"),
        trace: transition.trace.clone(),
    };
    assert_ne!(forged.state, transition.state);
    assert_eq!(
        ruleset.follow_transition(&mut observer, start, &pass(0), &forged),
        Err(ObservationError::ObserverOutOfStep)
    );
    assert_eq!(observer, before);

    // The real transition still follows from the unchanged Observer.
    assert!(ruleset
        .follow_transition(&mut observer, start, &pass(0), &transition)
        .is_ok());
    assert_ne!(observer, before);
}

#[test]
fn an_observed_event_that_does_not_fit_leaves_the_observed_gamestate_unchanged() {
    let ruleset = effects();
    let creation = create_effects(&ruleset);
    let game = Followed::start(&ruleset, &creation);
    let opening = game.observed(0);
    let own_leader = h(0);
    assert!(opening.card(own_leader).is_some(), "a handle that is held");

    let empty = {
        let observer = ruleset
            .observer(viewer(0), &creation.empty, &[])
            .expect("the Observer is built");
        ruleset
            .observe(&observer, &creation.empty)
            .expect("the Empty GameState is observed")
    };

    // Each of these fits no Observed GameState it is applied to.
    let on_the_opening = [
        // a Player or a zone that does not exist
        ObservedEvent::UnseenCardCreated {
            owner: p(9),
            zone: zone("deck"),
        },
        ObservedEvent::DrawFromEmptyZone {
            player: p(0),
            zone: zone("nowhere"),
        },
        // a seen move out of a zone the Viewer cannot see, and an unseen move out of a seen one
        ObservedEvent::CardMoved {
            card: own_leader,
            player: p(0),
            from: zone("deck"),
            to: zone("hand"),
        },
        ObservedEvent::UnseenCardMoved {
            player: p(0),
            from: zone("field"),
            to: zone("hand"),
        },
        // a card that is not in `from`, and one that is visible already
        ObservedEvent::CardMoved {
            card: own_leader,
            player: p(0),
            from: zone("field"),
            to: zone("trash"),
        },
        ObservedEvent::CardMovedIntoSight {
            card: own_leader,
            player: p(1),
            from: zone("deck"),
            to: zone("field"),
            definition: CardId::new("any"),
            state: Vec::new(),
        },
        // a card that is not visible
        ObservedEvent::CardMovedOutOfSight {
            card: h(999),
            player: p(0),
            from: zone("hand"),
            to: zone("deck"),
        },
        // an order that is not a permutation, and a shuffle seen the wrong way
        ObservedEvent::ZoneShuffled {
            player: p(0),
            zone: zone("hand"),
            order: vec![own_leader],
        },
        ObservedEvent::UnseenZoneShuffled {
            player: p(0),
            zone: zone("hand"),
        },
        // a state the card does not have, a value of the wrong type, a card that is not visible
        ObservedEvent::CardStateChanged {
            card: own_leader,
            state: name("no_such_state"),
            value: FieldValue::Bool(true),
        },
        ObservedEvent::CardStateChanged {
            card: own_leader,
            state: name("tapped"),
            value: FieldValue::Int(1),
        },
        ObservedEvent::CardStateChanged {
            card: h(999),
            state: name("tapped"),
            value: FieldValue::Bool(true),
        },
        // a draw from a zone that is not empty, a Turn of a Player who is not in the Game, bad losses
        ObservedEvent::DrawFromEmptyZone {
            player: p(0),
            zone: zone("hand"),
        },
        ObservedEvent::TurnBegan {
            player: p(9),
            turn: turn(2),
        },
        ObservedEvent::GameEnded { losses: Vec::new() },
        ObservedEvent::GameEnded {
            losses: vec![Loss {
                player: p(9),
                reason: WinConditionId::new("deck_out"),
            }],
        },
    ];
    for event in &on_the_opening {
        let mut state = opening.clone();
        assert_eq!(
            state.apply(event),
            Err(ObservedApplyError::EventDoesNotFit),
            "{event:?}"
        );
        assert_eq!(state, opening, "{event:?}");
        assert_eq!(
            opening.apply_events(std::slice::from_ref(event)),
            Err(ObservedApplyError::EventDoesNotFit),
            "{event:?}"
        );
    }

    // A phase that begins before any Turn has begun.
    let phase = ObservedEvent::PhaseBegan {
        phase: PhaseId::new("main"),
    };
    let mut state = empty.clone();
    assert_eq!(
        state.apply(&phase),
        Err(ObservedApplyError::EventDoesNotFit)
    );
    assert_eq!(state, empty);

    // A count that would go below zero: move the five unseen cards out of seat 1's hand, then one more.
    let drain = ObservedEvent::UnseenCardMoved {
        player: p(1),
        from: zone("hand"),
        to: zone("deck"),
    };
    let mut state = opening.clone();
    for _ in 0..5 {
        state.apply(&drain).expect("the hand holds five cards");
    }
    assert_eq!(
        state.zone(p(1), &zone("hand")).map(|hand| hand.count()),
        Some(0)
    );
    let drained = state.clone();
    assert_eq!(
        state.apply(&drain),
        Err(ObservedApplyError::EventDoesNotFit)
    );
    assert_eq!(state, drained);

    // After the end, no Observed Event applies, and the state is unchanged.
    let scripted = play_scripted(&ruleset, [EFFECTS_DECK_A, EFFECTS_DECK_B], SEED);
    let observed = follow_all(&ruleset, &scripted, 0);
    let ended = observed
        .transitions
        .last()
        .map(|last| last.state.clone())
        .expect("the Game has steps");
    assert!(ended.outcome().is_some());
    for event in [phase, drain] {
        let mut state = ended.clone();
        assert_eq!(
            state.apply(&event),
            Err(ObservedApplyError::GameAlreadyEnded),
            "{event:?}"
        );
        assert_eq!(state, ended);
        assert_eq!(
            ended.apply_events(&[event]),
            Err(ObservedApplyError::GameAlreadyEnded)
        );
    }
}
