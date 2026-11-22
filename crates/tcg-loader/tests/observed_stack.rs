//! Acceptance tests for ticket #85: observing the Stack (ADR-0037), on the real Response Ruleset,
//! its real catalog and its real sample decks, with a test-only response (`snipe`) where a card has
//! to leave.
//!
//! The Stack is fully public: both Viewers see each Stack Item's controller, source and bound
//! cards as Observed Cards, and the Player with Priority. A bound card that has left is shown as
//! `None`. The three Stack Events are observed in full, and `Pass` is an Observed Action that
//! round-trips. The oracles are literals read off the arrangement of each scenario.
//!
//! A scenario is arranged with Events applied outside a transition, which are given to the
//! Observers as part of the trace they fold from the Empty GameState, so both Viewers know the
//! arranged cards.

use tcg_engine::deck_list::DeckList;
use tcg_engine::declarative::{
    Action, Argument, CardInstanceId, DeclarativeRuleset, Event, GameCreation, GameState,
    ObservationError, ObservedAction, ObservedArgument, ObservedBoundCard, ObservedCard,
    ObservedEvent, ObservedGameState, ObservedStackItem, Observer, StackSource, TakenAction,
    TakenArgument, Viewer,
};
use tcg_engine::player::PlayerId;
use tcg_engine::random::Seed;
use tcg_engine::ruleset::{LegalActions, Observations, Ruleset};
use tcg_engine::ruleset_definition::{ActionId, FieldValue, ParamName, StateName, ZoneId};
use tcg_loader::{load_card_catalog, load_deck_list, load_ruleset_definition};

const RESPONSE_RULESET: &str =
    include_str!("../../../rulesets/response-ruleset/response-ruleset.yaml");
const RESPONSE_CATALOG: &str =
    include_str!("../../../rulesets/response-ruleset/response-catalog.yaml");
const RESPONSE_DECK_A: &str = include_str!("../../../rulesets/response-ruleset/decks/deck-a.yaml");
const RESPONSE_DECK_B: &str = include_str!("../../../rulesets/response-ruleset/decks/deck-b.yaml");

/// A response that moves an opponent's creature to the trash. It exists only in this file: the
/// window of the edited ruleset lists it.
const SNIPE: &str = "  - id: snipe
    params:
      - { name: victim, zone: field, card_type: creature, owner: opponent }
    effects:
      - move: { card: { ref: victim }, to: trash }
  - id: pass_turn
";

const SEED: u64 = 4;

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

fn edit(yaml: &str, from: &str, to: &str) -> String {
    assert_eq!(
        yaml.matches(from).count(),
        1,
        "`{from}` occurs once in the fixture"
    );
    yaml.replace(from, to)
}

fn response() -> DeclarativeRuleset {
    let definition = load_ruleset_definition(RESPONSE_RULESET).expect("the ruleset loads");
    let catalog = load_card_catalog(&definition, RESPONSE_CATALOG).expect("the catalog loads");
    DeclarativeRuleset::new(definition, catalog)
}

/// The Response Ruleset whose window also lists `snipe`.
fn response_with_snipe() -> DeclarativeRuleset {
    let yaml = edit(
        RESPONSE_RULESET,
        "    actions: [fast_play]\n",
        "    actions: [fast_play, snipe]\n",
    );
    let yaml = edit(&yaml, "  - id: pass_turn\n", SNIPE);
    let definition = load_ruleset_definition(&yaml).expect("the ruleset loads");
    let catalog = load_card_catalog(&definition, RESPONSE_CATALOG).expect("the catalog loads");
    DeclarativeRuleset::new(definition, catalog)
}

fn decks(ruleset: &DeclarativeRuleset) -> Vec<DeckList> {
    [RESPONSE_DECK_A, RESPONSE_DECK_B]
        .iter()
        .map(|yaml| {
            load_deck_list(ruleset.definition(), ruleset.catalog(), yaml)
                .expect("the sample deck is legal")
        })
        .collect()
}

fn declared(player: u32, id: &str, cards: &[CardInstanceId]) -> Action {
    Action::Declared {
        player: p(player),
        action: ActionId::new(id),
        arguments: cards.iter().copied().map(Argument::Card).collect(),
    }
}

fn pass(player: u32) -> Action {
    Action::Pass { player: p(player) }
}

fn in_zone(state: &GameState, seat: u32, id: &str) -> Vec<CardInstanceId> {
    state
        .zone(p(seat), &zone(id))
        .expect("the zone exists")
        .to_vec()
}

/// A Game arranged for an attack: the Game of the Response sample decks with both hands emptied,
/// seat 0's `attacker` (a `river_otter`) on the field, seat 1's `target` (a `moss_sprite`) on the
/// field and tapped, two untapped mana for seat 1 and a `swift_sparrow` in seat 1's hand. The Events
/// of the creation and of the arrangement are kept, for the Observers.
struct Arranged {
    creation: GameCreation,
    state: GameState,
    events: Vec<Event>,
    attacker: CardInstanceId,
    target: CardInstanceId,
    sparrow: CardInstanceId,
}

impl Arranged {
    fn apply(&mut self, event: Event) {
        self.state
            .apply(&event)
            .expect("the scenario Event is accepted");
        self.events.push(event);
    }

    fn place(&mut self, definition: &str, seat: u32, to: &str) -> CardInstanceId {
        let found = self
            .state
            .cards()
            .iter()
            .find(|card| {
                card.owner() == p(seat)
                    && card.definition().as_str() == definition
                    && self.state.zone_of(card.id()) != Some(&zone(to))
            })
            .map(|card| card.id())
            .expect("the deck holds the card");
        let from = self.state.zone_of(found).expect("in a zone").clone();
        self.apply(Event::CardMoved {
            card: found,
            player: p(seat),
            from,
            to: zone(to),
        });
        found
    }

    /// The Observer of `seat` over every Event so far.
    fn observer(&self, ruleset: &DeclarativeRuleset, seat: u32) -> Observer {
        ruleset
            .observer(viewer(seat), &self.creation.empty, &self.events)
            .expect("the Observer is built")
    }
}

fn arranged(ruleset: &DeclarativeRuleset) -> Arranged {
    let creation = ruleset
        .create_game(&decks(ruleset), Seed(SEED))
        .expect("the decks create a game");
    let mut scene = Arranged {
        state: creation.opening.state.clone(),
        events: creation.opening.trace.events().to_vec(),
        creation,
        attacker: CardInstanceId(0),
        target: CardInstanceId(0),
        sparrow: CardInstanceId(0),
    };
    for seat in [0, 1] {
        for card in in_zone(&scene.state, seat, "hand") {
            scene.apply(Event::CardMoved {
                card,
                player: p(seat),
                from: zone("hand"),
                to: zone("deck"),
            });
        }
    }
    scene.place("mana", 1, "mana_zone");
    scene.place("mana", 1, "mana_zone");
    scene.attacker = scene.place("river_otter", 0, "field");
    scene.target = scene.place("moss_sprite", 1, "field");
    scene.apply(Event::CardStateChanged {
        card: scene.target,
        state: StateName::new("tapped"),
        value: FieldValue::Bool(true),
    });
    scene.sparrow = scene.place("swift_sparrow", 1, "hand");
    scene
}

fn observed_bound(param: &str, zone_id: &str, card: Option<ObservedCard>) -> ObservedBoundCard {
    ObservedBoundCard {
        param: ParamName::new(param),
        zone: zone(zone_id),
        card,
    }
}

/// The Observed Stack Item of the attack, as the Viewer of `observer` sees it.
fn observed_attack(
    observer: &Observer,
    attacker: CardInstanceId,
    target: CardInstanceId,
    attacker_left: bool,
) -> ObservedStackItem {
    ObservedStackItem {
        controller: p(0),
        source: StackSource::Action(ActionId::new("attack")),
        bound: vec![
            observed_bound(
                "attacker",
                "field",
                if attacker_left {
                    None
                } else {
                    Some(observer.observed_card(attacker).expect("a seen card"))
                },
            ),
            observed_bound(
                "target",
                "field",
                Some(observer.observed_card(target).expect("a seen card")),
            ),
        ],
    }
}

// ---- the Observed GameState

#[test]
fn both_viewers_see_the_stack_and_priority() {
    let ruleset = response();
    let scene = arranged(&ruleset);
    let (a, t) = (scene.attacker, scene.target);
    let attack = declared(0, "attack", &[a, t]);
    let transition = ruleset
        .transition(&scene.state, &attack)
        .expect("the attack is accepted");

    for seat in [0, 1] {
        let mut observer = scene.observer(&ruleset, seat);
        let before = ruleset
            .observe(&observer, &scene.state)
            .expect("the GameState is observed");
        assert!(before.stack().is_empty());
        assert_eq!(before.priority(), None);
        assert_eq!(before.deciding_player(), p(0));

        let followed = ruleset
            .follow_transition(&mut observer, &scene.state, &attack, &transition)
            .expect("the transition is followed");

        // Both Viewers see the same item: its controller, its source and its two bound cards, each
        // under the Observed Card the Viewer holds, in the zone it was bound from.
        let item = observed_attack(&observer, a, t, false);
        let after = &followed.state;
        assert_eq!(after.stack(), std::slice::from_ref(&item), "seat {seat}");
        // Priority is the attacker's opponent's, who is the Deciding Player.
        assert_eq!(after.priority(), Some(p(1)), "seat {seat}");
        assert_eq!(after.deciding_player(), p(1), "seat {seat}");
        // The Events, as observed: the tap of a seen card, then the push, in full.
        let handle = observer.observed_card(a).expect("a seen card");
        assert_eq!(
            followed.events,
            [
                ObservedEvent::CardStateChanged {
                    card: handle,
                    state: StateName::new("tapped"),
                    value: FieldValue::Bool(true)
                },
                ObservedEvent::ItemPushed { item }
            ],
            "seat {seat}"
        );
        assert_eq!(
            followed.action,
            TakenAction::Declared {
                player: p(0),
                action: ActionId::new("attack"),
                arguments: vec![
                    TakenArgument::Card(handle),
                    TakenArgument::Card(observer.observed_card(t).expect("a seen card"))
                ]
            }
        );
        // The Observed Events applied to the previous Observed GameState give the next one.
        assert_eq!(before.apply_events(&followed.events), Ok(after.clone()));
        assert_eq!(
            ruleset.observe(&observer, &transition.state).as_ref(),
            Ok(after)
        );
    }
}

/// One Viewer's Observer and Observed GameState, advanced one accepted Action at a time.
struct Follow<'a> {
    ruleset: &'a DeclarativeRuleset,
    seat: u32,
    state: GameState,
    observer: Observer,
    observed: ObservedGameState,
}

impl Follow<'_> {
    /// Takes `action`, which must be accepted, and returns its Observed Events. They must fold into
    /// the Observed GameState the Observer gives for the next GameState.
    fn step(&mut self, action: &Action) -> Vec<ObservedEvent> {
        let transition = self
            .ruleset
            .transition(&self.state, action)
            .expect("the Action is accepted");
        let followed = self
            .ruleset
            .follow_transition(&mut self.observer, &self.state, action, &transition)
            .expect("the transition is followed");
        assert_eq!(
            self.observed.apply_events(&followed.events),
            Ok(followed.state.clone()),
            "seat {}",
            self.seat
        );
        self.observed = followed.state;
        self.state = transition.state;
        followed.events
    }

    fn handle(&self, card: CardInstanceId) -> ObservedCard {
        self.observer.observed_card(card).expect("a seen card")
    }
}

#[test]
fn stack_events_are_observed_in_full() {
    let ruleset = response_with_snipe();
    let scene = arranged(&ruleset);
    let (a, t) = (scene.attacker, scene.target);

    for seat in [0, 1] {
        let observer = scene.observer(&ruleset, seat);
        let observed = ruleset
            .observe(&observer, &scene.state)
            .expect("the GameState is observed");
        let mut follow = Follow {
            ruleset: &ruleset,
            seat,
            state: scene.state.clone(),
            observer,
            observed,
        };

        // The attack: the tap, then `ItemPushed` last, with the whole item.
        let pushed_attack = follow.step(&declared(0, "attack", &[a, t]));
        assert_eq!(
            pushed_attack,
            [
                ObservedEvent::CardStateChanged {
                    card: follow.handle(a),
                    state: StateName::new("tapped"),
                    value: FieldValue::Bool(true)
                },
                ObservedEvent::ItemPushed {
                    item: observed_attack(&follow.observer, a, t, false)
                }
            ],
            "seat {seat}"
        );

        // Seat 1 snipes the attacker; seat 0 passes, which resolves the snipe.
        let snipe_item = ObservedStackItem {
            controller: p(1),
            source: StackSource::Action(ActionId::new("snipe")),
            bound: vec![observed_bound("victim", "field", Some(follow.handle(a)))],
        };
        let pushed_snipe = follow.step(&declared(1, "snipe", &[a]));
        assert_eq!(
            pushed_snipe,
            [ObservedEvent::ItemPushed {
                item: snipe_item.clone()
            }],
            "seat {seat}"
        );
        let resolved_snipe = follow.step(&pass(0));
        // `ItemResolved` shows the item as it was when it resolved, then the move of the victim.
        assert_eq!(
            resolved_snipe,
            [
                ObservedEvent::ItemResolved { item: snipe_item },
                ObservedEvent::CardMoved {
                    card: follow.handle(a),
                    player: p(0),
                    from: zone("field"),
                    to: zone("trash")
                }
            ],
            "seat {seat}"
        );

        // The attack item below now shows its attacker as gone: `None`, though the card sits in the
        // public trash and the Viewer still holds its handle.
        let gone = observed_attack(&follow.observer, a, t, true);
        assert_eq!(
            follow.observed.stack(),
            std::slice::from_ref(&gone),
            "seat {seat}"
        );

        // Seat 1 passes: the attack Fizzles, `ItemFizzled` alone, with the item as it was.
        let fizzled = follow.step(&pass(1));
        assert_eq!(
            fizzled,
            [ObservedEvent::ItemFizzled { item: gone }],
            "seat {seat}"
        );
        assert!(follow.observed.stack().is_empty());
        assert_eq!(follow.observed.priority(), None);
    }
}

#[test]
fn an_observed_pass_round_trips() {
    let ruleset = response();
    let scene = arranged(&ruleset);
    let (a, t, sparrow) = (scene.attacker, scene.target, scene.sparrow);
    let attack = declared(0, "attack", &[a, t]);
    let pushed = ruleset
        .transition(&scene.state, &attack)
        .expect("the attack is accepted");

    let observers = [0, 1].map(|seat| {
        let mut observer = scene.observer(&ruleset, seat);
        ruleset
            .follow_transition(&mut observer, &scene.state, &attack, &pushed)
            .expect("the transition is followed");
        observer
    });

    // The Player with Priority is offered `Pass`, then the `fast_play`, each as an Observed Action;
    // the other Player is offered nothing.
    let sparrow_handle = observers[1]
        .observed_card(sparrow)
        .expect("seat 1 sees its hand");
    let offered = ruleset
        .observed_legal_actions(&observers[1], &pushed.state)
        .expect("the Legal Actions are observed");
    assert_eq!(
        offered,
        [
            ObservedAction::Pass { player: p(1) },
            ObservedAction::Declared {
                player: p(1),
                action: ActionId::new("fast_play"),
                arguments: vec![ObservedArgument::Card(sparrow_handle)]
            }
        ]
    );
    assert_eq!(
        ruleset.observed_legal_actions(&observers[0], &pushed.state),
        Ok(vec![])
    );

    // Translated back, the Observed Legal Actions are the Legal Actions, in order.
    let translated: Vec<Action> = offered
        .iter()
        .map(|action| {
            ruleset
                .translate_action(&observers[1], action)
                .expect("the Observed Action translates")
        })
        .collect();
    assert_eq!(translated, ruleset.legal_actions(&pushed.state));
    assert_eq!(translated.first(), Some(&pass(1)));

    // A Pass of another Player than the Viewer is not translated.
    assert_eq!(
        ruleset.translate_action(&observers[0], &ObservedAction::Pass { player: p(1) }),
        Err(ObservationError::ActionOfAnotherPlayer { player: p(1) })
    );

    // Taking it: the Taken Action of a Pass shows it, to both Viewers, and the trace opens with
    // `ItemResolved`.
    let passed = ruleset
        .transition(&pushed.state, &pass(1))
        .expect("the Pass is accepted");
    for (seat, mut observer) in observers.into_iter().enumerate() {
        let followed = ruleset
            .follow_transition(&mut observer, &pushed.state, &pass(1), &passed)
            .expect("the Pass is followed");
        assert_eq!(
            followed.action,
            TakenAction::Pass { player: p(1) },
            "seat {seat}"
        );
        assert!(
            matches!(
                followed.events.first(),
                Some(ObservedEvent::ItemResolved { .. })
            ),
            "seat {seat}"
        );
        assert!(followed.state.stack().is_empty());
        assert_eq!(followed.state.priority(), None);
        assert_eq!(followed.state.deciding_player(), p(0));
    }
}
