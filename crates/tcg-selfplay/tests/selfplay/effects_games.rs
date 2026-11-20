//! The correctness net over the Effects Ruleset (ticket #53): sample decks that play the effect
//! cards, seeded self-play with every Invariant, Trigger Cap Rejections as a counted outcome, the
//! Recomputed GameState, exact replay of Games with cascades, and the freeze of the Small Ruleset.
//!
//! Everything runs on the real Effects Ruleset, the real catalog and the real sample decks, except
//! where a test says that it edits one on purpose.

use std::collections::BTreeSet;

use tcg_engine::card_catalog::{CardEffect, CardId};
use tcg_engine::deck_list::DeckList;
use tcg_engine::declarative::{
    Action, Argument, CardInstanceId, DeclarativeRuleset, Event, GameState, Rejection,
};
use tcg_engine::player::PlayerId;
use tcg_engine::random::Seed;
use tcg_engine::ruleset::{LegalActions, Ruleset};
use tcg_engine::ruleset_definition::{ActionId, DeckInputId, FieldValue, StateName, ZoneId};
use tcg_selfplay::{
    check_recomputation, check_rejection, self_play, verify_record, Checks, FailureKind, Invariant,
    SelfPlayGame,
};

use crate::common::{
    digest_line, effects, effects_deck, effects_decks, effects_with_ruleset, fnv1a, pin_difference,
    steps, DECK_A, DECK_B, EFFECTS_DECK_A, EFFECTS_DECK_B, FNV_START, MAX_STEPS, SMALL_CATALOG,
    SMALL_RULESET,
};

/// The Seeds of the Effects self-play pin: `0..PINNED_SEEDS`.
const PINNED_SEEDS: u64 = 1000;

/// The committed pin of the Game Records and Event Traces of the Seeds `0..PINNED_SEEDS` over the
/// Effects Ruleset, one `seed actions digest` line per Seed, in the format of the Small pin.
const DIGESTS: &str = include_str!("golden/effects-self-play-seeds-0-1000.digest");

const DIGESTS_PATH: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/selfplay/golden/effects-self-play-seeds-0-1000.digest"
);

/// The frozen Small Ruleset (ADR-0022): the files that make it up, then the files that pin its
/// Games, each with the FNV-1a digest of its bytes. The pinned files are the Golden Snapshots of
/// `tcg-loader` and the self-play pin, so regenerating one of them by accident (which would
/// leave it matching itself) changes a digest here.
const SMALL_RULESET_FREEZE: [(&str, &str, u64); 7] = [
    (
        "rulesets/small-ruleset/small-ruleset.yaml",
        SMALL_RULESET,
        0xce2a_cebc_67e5_3385,
    ),
    (
        "rulesets/small-ruleset/small-catalog.yaml",
        SMALL_CATALOG,
        0x7ccc_e965_cd34_7cb9,
    ),
    (
        "rulesets/small-ruleset/decks/deck-a.yaml",
        DECK_A,
        0x09f5_643b_28cb_3cbb,
    ),
    (
        "rulesets/small-ruleset/decks/deck-b.yaml",
        DECK_B,
        0xa6f8_3a9e_6a00_42b9,
    ),
    (
        "crates/tcg-loader/tests/golden/small-deck-out.trace",
        include_str!("../../../tcg-loader/tests/golden/small-deck-out.trace"),
        0xfab5_3951_d756_e90c,
    ),
    (
        "crates/tcg-loader/tests/golden/small-leader-kill.trace",
        include_str!("../../../tcg-loader/tests/golden/small-leader-kill.trace"),
        0xc230_4dd1_64b2_2a91,
    ),
    (
        "crates/tcg-selfplay/tests/selfplay/golden/self-play-seeds-0-1000.digest",
        include_str!("golden/self-play-seeds-0-1000.digest"),
        0x433c_3493_2a7e_fbb6,
    ),
];

/// The Trigger Cap Rejections seen over the listings of the Game of two shade decks and Seed 3: a
/// non-zero count, which the test also checks against an independent walk.
const SHADE_GAME_TRIGGER_CAP_REJECTIONS: usize = 8;

/// A Deck List for the Effects Ruleset that holds `restless_shade`, which no shipped deck lists:
/// its two effects wake each other, so playing it exceeds the Trigger Cap (ADR-0023).
const SHADE_DECK: &str = "inputs:
  - input: leader
    cards:
      - { card: captain_ardel, count: 1 }
  - input: main_deck
    cards:
      - { card: restless_shade, count: 4 }
      - { card: goblin_scout,   count: 4 }
      - { card: moss_sprite,    count: 4 }
      - { card: ember_hound,    count: 4 }
      - { card: river_otter,    count: 4 }
      - { card: stone_golem,    count: 4 }
      - { card: shadow_cat,     count: 4 }
      - { card: iron_guard,     count: 4 }
      - { card: wind_hawk,      count: 4 }
      - { card: thorn_boar,     count: 4 }
      - { card: frost_wolf,     count: 4 }
      - { card: ash_drake,      count: 4 }
      - { card: tide_serpent,   count: 2 }
  - input: mana_deck
    cards:
      - { card: mana, count: 10 }
";

fn play_creature(player: u32, card: CardInstanceId) -> Action {
    Action {
        player: PlayerId(player),
        action: ActionId::new("play_creature"),
        arguments: vec![Argument::Card(card)],
    }
}

fn shade_decks(ruleset: &DeclarativeRuleset) -> Vec<DeckList> {
    vec![
        effects_deck(ruleset, SHADE_DECK),
        effects_deck(ruleset, SHADE_DECK),
    ]
}

/// The effects of the Card Definition `card` of `ruleset`.
fn effects_of<'a>(ruleset: &'a DeclarativeRuleset, card: &CardId) -> &'a [CardEffect] {
    ruleset
        .catalog()
        .card(card)
        .map_or(&[], |definition| definition.effects())
}

fn is_triggered(effect: &CardEffect) -> bool {
    matches!(effect, CardEffect::Triggered(_))
}

/// A continuous effect of the carrying card alone, without a `scope`.
fn is_self_continuous(effect: &CardEffect) -> bool {
    matches!(effect, CardEffect::Continuous(continuous) if continuous.scope().is_none())
}

/// A continuous effect over a matched set: an aura.
fn is_aura(effect: &CardEffect) -> bool {
    matches!(effect, CardEffect::Continuous(continuous) if continuous.scope().is_some())
}

#[test]
fn the_effects_sample_decks_hold_triggered_continuous_and_aura_cards() {
    let ruleset = effects();
    let decks = effects_decks(&ruleset);
    assert_eq!(decks.len(), 2);
    let main_deck = DeckInputId::new("main_deck");
    for (seat, deck) in decks.iter().enumerate() {
        let entries = deck
            .input(&main_deck)
            .expect("a deck has a main deck")
            .entries();
        let total: u32 = entries.iter().map(|entry| entry.count().get()).sum();
        assert_eq!(total, 50, "seat {seat}: the main deck totals 50 cards");
        assert!(
            entries.iter().all(|entry| entry.count().get() <= 4),
            "seat {seat}: at most 4 copies of a card"
        );
        let carries = |wanted: fn(&CardEffect) -> bool| {
            entries
                .iter()
                .any(|entry| effects_of(&ruleset, entry.card()).iter().any(wanted))
        };
        assert!(carries(is_triggered), "seat {seat}: a triggered card");
        assert!(
            carries(is_self_continuous),
            "seat {seat}: a self-continuous card"
        );
        assert!(carries(is_aura), "seat {seat}: an aura");
    }

    // They load under the Effects Ruleset, so they are legal, and they make a Game.
    ruleset
        .create_game(&decks, Seed(0))
        .expect("the sample decks create a Game");
}

/// Which effect cards a Game actually used, read from its Game Record and Event Traces only.
#[derive(Default)]
struct EffectUse {
    /// A card that carries a triggered effect was played (`play_creature`).
    triggered_card_played: bool,
    /// A card that carries an aura was in the field after some Action.
    aura_in_field: bool,
    /// A card that carries a self-continuous effect was in the field after some Action.
    self_continuous_in_field: bool,
}

fn effect_use(ruleset: &DeclarativeRuleset, game: &SelfPlayGame) -> EffectUse {
    let carries = |card: CardInstanceId, wanted: fn(&CardEffect) -> bool| {
        game.state.card(card).is_some_and(|instance| {
            effects_of(ruleset, instance.definition())
                .iter()
                .any(wanted)
        })
    };
    let mut used = EffectUse::default();
    let mut field = BTreeSet::new();
    for (action, trace) in game.record.actions().iter().zip(&game.traces) {
        if let (true, [Argument::Card(card)]) = (
            action.action.as_str() == "play_creature",
            action.arguments.as_slice(),
        ) {
            used.triggered_card_played |= carries(*card, is_triggered);
        }
        for event in trace.events() {
            if let Event::CardMoved { card, from, to, .. } = event {
                if from.as_str() == "field" {
                    field.remove(card);
                }
                if to.as_str() == "field" {
                    field.insert(*card);
                }
            }
        }
        used.aura_in_field |= field.iter().any(|card| carries(*card, is_aura));
        used.self_continuous_in_field |=
            field.iter().any(|card| carries(*card, is_self_continuous));
    }
    used
}

/// The least number of the 1,000 pinned Games in which each kind of effect card was used. A drop
/// under a minimum means the sample decks stopped exercising that family. Measured when the
/// minimums were set: 988 Games played a triggered card, 759 had an aura in the field and 795 had a
/// self-continuous card in the field.
const MIN_GAMES_PLAYING_A_TRIGGERED_CARD: usize = 900;
const MIN_GAMES_WITH_AN_AURA_IN_THE_FIELD: usize = 650;
const MIN_GAMES_WITH_A_SELF_CONTINUOUS_CARD_IN_THE_FIELD: usize = 700;

#[test]
fn effects_self_play_ends_every_game_over_1000_seeds_with_every_invariant_holding() {
    let ruleset = effects();
    let decks = effects_decks(&ruleset);
    let mut failures = Vec::new();
    let mut longest = 0;
    let mut digests = Vec::new();
    let (mut triggered, mut auras, mut self_continuous) = (0_usize, 0_usize, 0_usize);
    for seed in 0..PINNED_SEEDS {
        match self_play(&ruleset, &decks, Seed(seed), MAX_STEPS, Checks::On) {
            Ok(game) => {
                let used = effect_use(&ruleset, &game);
                triggered += usize::from(used.triggered_card_played);
                auras += usize::from(used.aura_in_field);
                self_continuous += usize::from(used.self_continuous_in_field);
                if game.state.outcome().is_none() {
                    failures.push(format!("seed {seed}: the Game has no Terminal Outcome"));
                }
                if game.trigger_cap_rejections != 0 {
                    failures.push(format!(
                        "seed {seed}: {} Trigger Cap Rejections, but no shipped deck lists a card that reaches the cap",
                        game.trigger_cap_rejections
                    ));
                }
                longest = longest.max(game.record.actions().len());
                digests.push(digest_line(seed, &game));
            }
            Err(failure) => failures.push(format!(
                "seed {}: failed after Action index {:?}: {:?}",
                failure.seed.0, failure.action_index, failure.kind
            )),
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
    assert!(
        longest * 2 < MAX_STEPS,
        "the Step Cap of {MAX_STEPS} has no headroom over the longest Game ({longest} Actions)"
    );

    // The decks play the effect cards, not only list them.
    assert!(
        triggered >= MIN_GAMES_PLAYING_A_TRIGGERED_CARD,
        "only {triggered} Games played a card with a triggered effect"
    );
    assert!(
        auras >= MIN_GAMES_WITH_AN_AURA_IN_THE_FIELD,
        "only {auras} Games had an aura in the field"
    );
    assert!(
        self_continuous >= MIN_GAMES_WITH_A_SELF_CONTINUOUS_CARD_IN_THE_FIELD,
        "only {self_continuous} Games had a self-continuous card in the field"
    );

    // The same Games, one pass: their Game Records and Event Traces are the pinned ones.
    if let Some(difference) = pin_difference(DIGESTS, &digests) {
        panic!(
            "{difference}\nIf the change is deliberate, run: cargo test -p tcg-selfplay --test selfplay -- --ignored regenerate_effects_self_play_digests"
        );
    }
}

/// Rewrites the pin of the 1,000 Effects Games. Run it by name when a rule, an Event's shape or a
/// shipped Effects deck changes on purpose, and read the diff before committing it.
#[test]
#[ignore = "deliberate update: rewrites the Effects self-play pin"]
fn regenerate_effects_self_play_digests() {
    let ruleset = effects();
    let decks = effects_decks(&ruleset);
    let mut text = String::new();
    for seed in 0..PINNED_SEEDS {
        let game = self_play(&ruleset, &decks, Seed(seed), MAX_STEPS, Checks::On)
            .expect("the Game is played");
        text.push_str(&digest_line(seed, &game));
    }
    std::fs::write(DIGESTS_PATH, text).expect("the Effects self-play pin is written");
}

#[test]
fn an_effects_self_play_failure_reproduces_itself_from_its_record() {
    // `pass_turn` needs a card that no card can be (`where: false`; every action must be listed
    // and the Phase must list one that ends the Turn, ADR-0027): a Player who has played what they
    // can has no Legal Action left in a Game that is on.
    let ruleset = effects_with_ruleset(
        "  - id: pass_turn\n    effects:",
        "  - id: pass_turn\n    params:\n      - { name: card, zone: hand, card_type: creature, where: false }\n    effects:",
    );
    let decks = effects_decks(&ruleset);
    let failure = self_play(&ruleset, &decks, Seed(0), MAX_STEPS, Checks::On)
        .expect_err("a Game whose pass_turn cannot be taken runs out of Legal Actions");
    assert_eq!(failure.seed, Seed(0));
    assert_eq!(failure.action_index, Some(1));
    assert_eq!(failure.kind, FailureKind::NoLegalAction);
    assert_eq!(failure.record.actions().len(), 2);

    // The record is the Actions the Game accepted, and it reproduces the failure: it verifies, and
    // replays to the GameState the failure stopped in, which has not ended and lists nothing.
    let replay = ruleset.replay(&failure.record).expect("the record replays");
    assert!(replay.state.outcome().is_none());
    assert_eq!(ruleset.legal_actions(&replay.state), Vec::new());
    assert_eq!(replay.traces.len(), 2);
    let verified = verify_record(&ruleset, &failure.record).expect("a good record verifies");
    assert!(verified.state == replay.state, "the stopped GameState");
    assert!(verified.traces == replay.traces, "Event Traces");

    // The same Seed with checks off fails identically: the checks only read.
    let unchecked = self_play(&ruleset, &decks, Seed(0), MAX_STEPS, Checks::Off)
        .expect_err("checks off fails the same way");
    assert!(unchecked == failure, "the failure with checks off");

    // Another Seed, another index.
    let other = self_play(&ruleset, &decks, Seed(1), MAX_STEPS, Checks::On)
        .expect_err("a Game whose pass_turn cannot be taken runs out of Legal Actions");
    assert_eq!(other.seed, Seed(1));
    assert_eq!(other.action_index, Some(0));
    assert_eq!(other.kind, FailureKind::NoLegalAction);
}

/// Over Effects Games in which auras are in the field and cards with triggers are played, the fold
/// of every Event of the Game onto the Empty GameState equals the engine's GameState at every step.
#[test]
fn the_recomputed_game_state_equals_the_engine_state_after_every_transition() {
    let ruleset = effects();
    let decks = effects_decks(&ruleset);
    let mut saw_an_aura_in_the_field = false;
    let mut saw_a_trigger_play = false;
    for seed in 0..20 {
        let game = self_play(&ruleset, &decks, Seed(seed), MAX_STEPS, Checks::On)
            .expect("the Game is played");

        // The fold starts from the Empty GameState, with nothing the engine carried forward.
        let mut recomputed = game
            .creation
            .empty
            .apply_trace(game.creation.opening.trace.events())
            .expect("the opening trace applies to the Empty GameState");
        assert!(
            recomputed == game.creation.opening.state,
            "seed {seed}: the Opening Position"
        );
        for (index, (step, action)) in steps(&ruleset, &game)
            .iter()
            .zip(game.record.actions())
            .enumerate()
        {
            recomputed = recomputed
                .apply_trace(step.trace.events())
                .expect("the trace applies");
            assert!(
                recomputed == step.next,
                "seed {seed}, Action index {index}: the Recomputed GameState differs from the engine's"
            );
            assert_eq!(check_recomputation(&recomputed, &step.next), Vec::new());

            saw_an_aura_in_the_field |= step.next.cards().iter().any(|card| {
                step.next
                    .zone_of(card.id())
                    .is_some_and(|zone| zone.as_str() == "field")
                    && effects_of(&ruleset, card.definition()).iter().any(is_aura)
            });
            if let [Argument::Card(card)] = action.arguments.as_slice() {
                saw_a_trigger_play |= action.action.as_str() == "play_creature"
                    && step.prior.card(*card).is_some_and(|played| {
                        effects_of(&ruleset, played.definition())
                            .iter()
                            .any(is_triggered)
                    });
            }
        }
        assert!(recomputed == game.state, "seed {seed}: the final GameState");
    }
    assert!(saw_an_aura_in_the_field, "an aura was in the field");
    assert!(saw_a_trigger_play, "a card with a trigger was played");
}

#[test]
fn invariant_derived_reads_do_not_leak_detects_a_violation() {
    let ruleset = effects();
    let game = self_play(
        &ruleset,
        &effects_decks(&ruleset),
        Seed(1),
        MAX_STEPS,
        Checks::On,
    )
    .expect("the Game is played");
    let step = steps(&ruleset, &game)
        .into_iter()
        .find(|step| !step.trace.events().is_empty() && step.prior != step.next)
        .expect("a Game changes its state");
    assert_eq!(check_recomputation(&step.next, &step.next), Vec::new());

    // A Recomputed GameState in which one creature's `tapped` is not what the engine carries: a
    // value that was kept outside the Events.
    let tapped = StateName::new("tapped");
    let (card, held) = step
        .next
        .cards()
        .iter()
        .find_map(|card| match card.state_value(&tapped) {
            Some(FieldValue::Bool(held)) => Some((card.id(), *held)),
            _ => None,
        })
        .expect("a card has the state `tapped`");
    let mut recomputed = step.next.clone();
    recomputed
        .apply(&Event::CardStateChanged {
            card,
            state: tapped,
            value: FieldValue::Bool(!held),
        })
        .expect("the state change applies");
    let violations = check_recomputation(&recomputed, &step.next);
    assert_eq!(violations.len(), 1, "{violations:?}");
    assert_eq!(violations[0].invariant, Invariant::DerivedReadsDoNotLeak);
    assert!(
        violations[0]
            .detail
            .contains(&format!("card instance {}", card.0)),
        "the detail names the card that differs: {}",
        violations[0].detail
    );
    assert!(
        violations[0].detail.contains("tapped"),
        "the detail names the state that differs: {}",
        violations[0].detail
    );
}

/// The state of Turn 1, Main, in a Game of two shade decks, with the first Restless Shade of
/// Player 0 arranged into the hand, and the Action that plays it.
fn a_shade_in_hand() -> (DeclarativeRuleset, GameState, Action) {
    let ruleset = effects();
    let decks = shade_decks(&ruleset);
    let mut state = ruleset
        .create_game(&decks, Seed(1))
        .expect("the decks create a Game")
        .opening
        .state;
    let shade = state
        .cards()
        .iter()
        .find(|card| card.owner() == PlayerId(0) && card.definition().as_str() == "restless_shade")
        .expect("the deck has a Restless Shade")
        .id();
    let from = state.zone_of(shade).expect("in a zone").clone();
    if from.as_str() != "hand" {
        state
            .apply(&Event::CardMoved {
                card: shade,
                player: PlayerId(0),
                from,
                to: ZoneId::new("hand"),
            })
            .expect("the arrangement is legal");
    }
    (ruleset, state, play_creature(0, shade))
}

#[test]
fn a_trigger_cap_rejection_leaves_the_game_state_intact_and_emits_nothing() {
    let (ruleset, state, play) = a_shade_in_hand();
    let before = state.clone();

    // Nothing is emitted: the rejection carries no Event Trace and no GameState.
    assert_eq!(
        ruleset.transition(&state, &play).err(),
        Some(Rejection::TriggerCapExceeded { cap: 100 })
    );
    assert!(state == before, "the GameState it was offered is untouched");

    // It is the same on a second try, and the Action is not listed.
    assert_eq!(
        ruleset.transition(&state, &play).err(),
        Some(Rejection::TriggerCapExceeded { cap: 100 })
    );
    assert!(state == before, "still untouched");
    assert!(
        !ruleset.legal_actions(&state).contains(&play),
        "an Action the cap refuses is not a Legal Action"
    );
}

/// The Trigger Cap Rejections of `game`, counted by an oracle that does not use the listing: in
/// each GameState an Action was chosen from, the cards of the Active Player's hand that
/// `play_creature` would play and that `transition` refuses with the Trigger Cap.
///
/// Looking only at the hand is complete for the shade decks: the Trigger Cap is reached only by a
/// Restless Shade entering the field, and `play_creature`, whose one parameter is
/// `{ name: card, zone: hand, card_type: creature }`, binds only cards of the Active Player's hand.
/// A shade never rests in the field (entering it exceeds the cap and refuses the play), so no
/// `attack` target and no other candidate can start the chain. The candidates the listing refuses
/// with the cap are therefore exactly these hand cards.
fn trigger_cap_rejections_by_playing_every_card_in_hand(
    ruleset: &DeclarativeRuleset,
    game: &SelfPlayGame,
) -> usize {
    let hand = ZoneId::new("hand");
    let mut state = game.creation.opening.state.clone();
    let mut count = 0;
    for action in game.record.actions() {
        let player = state.active_player();
        for card in state.zone(player, &hand).unwrap_or_default() {
            let refused = matches!(
                ruleset.transition(&state, &play_creature(player.0, *card)),
                Err(Rejection::TriggerCapExceeded { .. })
            );
            if refused {
                count += 1;
            }
        }
        state = ruleset
            .transition(&state, action)
            .expect("a recorded Action is accepted")
            .state;
    }
    count
}

#[test]
fn self_play_counts_trigger_cap_rejections_without_failing_the_run() {
    let ruleset = effects();
    let decks = shade_decks(&ruleset);
    let game = self_play(&ruleset, &decks, Seed(3), MAX_STEPS, Checks::On)
        .expect("a Trigger Cap Rejection is not a failure of the run");
    assert!(
        game.state.outcome().is_some(),
        "the Game has a Terminal Outcome"
    );
    assert_eq!(
        game.trigger_cap_rejections, SHADE_GAME_TRIGGER_CAP_REJECTIONS,
        "the Restless Shades in hand that the cap refused over the Game's listings"
    );

    // The count is what an independent walk over the Game's states sees.
    assert_eq!(
        game.trigger_cap_rejections,
        trigger_cap_rejections_by_playing_every_card_in_hand(&ruleset, &game)
    );

    // With checks off the Game is the same one, and the count is not kept.
    let unchecked =
        self_play(&ruleset, &decks, Seed(3), MAX_STEPS, Checks::Off).expect("the Game is played");
    assert_eq!(unchecked.trigger_cap_rejections, 0);
    assert!(unchecked.record == game.record, "the same Game Record");
    assert!(unchecked.traces == game.traces, "the same Event Traces");
    assert!(unchecked.state == game.state, "the same final GameState");

    // The ordinary decks never reach the cap.
    let shipped = self_play(
        &ruleset,
        &effects_decks(&ruleset),
        Seed(3),
        MAX_STEPS,
        Checks::On,
    )
    .expect("the Game is played");
    assert_eq!(shipped.trigger_cap_rejections, 0);
}

#[test]
fn invariant_rejection_leaves_state_intact_detects_a_violation() {
    let (ruleset, state, play) = a_shade_in_hand();
    let cap = Rejection::TriggerCapExceeded { cap: 100 };

    // The Action is rejected the same way when run again, and the GameState is untouched.
    assert_eq!(check_rejection(&ruleset, &state, &play, &cap), Vec::new());

    // A Rejection that the re-run does not reproduce.
    let other = Rejection::GameOver;
    let violations = check_rejection(&ruleset, &state, &play, &other);
    assert_eq!(violations.len(), 1, "{violations:?}");
    assert_eq!(
        violations[0].invariant,
        Invariant::RejectionLeavesStateIntact
    );
    assert!(!violations[0].detail.is_empty());

    // An Action that the Ruleset accepts was not rejected at all.
    let accepted = ruleset
        .legal_actions(&state)
        .into_iter()
        .next()
        .expect("a Game that is on has a Legal Action");
    let violations = check_rejection(&ruleset, &state, &accepted, &cap);
    assert_eq!(violations.len(), 1, "{violations:?}");
    assert_eq!(
        violations[0].invariant,
        Invariant::RejectionLeavesStateIntact
    );
}

#[test]
fn listing_with_rejections_accepts_exactly_the_legal_actions_in_listing_order() {
    let ruleset = effects();
    let mut saw_a_rejection = false;
    for (decks, seed) in [
        (shade_decks(&ruleset), 3),
        (shade_decks(&ruleset), 4),
        (effects_decks(&ruleset), 5),
    ] {
        let game = self_play(&ruleset, &decks, Seed(seed), MAX_STEPS, Checks::Off)
            .expect("the Game is played");
        let mut state = game.creation.opening.state.clone();
        let mut states = vec![state.clone()];
        for action in game.record.actions() {
            state = ruleset
                .transition(&state, action)
                .expect("a recorded Action is accepted")
                .state;
            states.push(state.clone());
        }
        for (index, state) in states.iter().enumerate() {
            let listing = ruleset.listing_with_rejections(state);
            let accepted: Vec<Action> = listing
                .iter()
                .filter(|(_, rejection)| rejection.is_none())
                .map(|(action, _)| action.clone())
                .collect();
            assert!(
                accepted == ruleset.legal_actions(state),
                "seed {seed}, GameState {index}: the accepted candidates are the Legal Actions, in order"
            );
            for (action, rejection) in &listing {
                match (rejection, ruleset.transition(state, action)) {
                    (None, Ok(_)) => {}
                    (Some(listed), Err(actual)) => {
                        assert_eq!(listed, &actual, "seed {seed}, GameState {index}");
                        saw_a_rejection = true;
                    }
                    (listed, actual) => panic!(
                        "seed {seed}, GameState {index}: {action:?} is listed as {listed:?} but transition gives {:?}",
                        actual.map(|_| "an accepted transition")
                    ),
                }
            }
        }
    }
    assert!(saw_a_rejection, "some candidate was rejected");
}

#[test]
fn replaying_an_effects_record_reproduces_the_traces_and_the_final_state() {
    let ruleset = effects();
    let decks = effects_decks(&ruleset);
    // A second, separately loaded Ruleset, loaded once: replay must not depend on the first value.
    let replaying = effects();
    for seed in 0..PINNED_SEEDS {
        let game = self_play(&ruleset, &decks, Seed(seed), MAX_STEPS, Checks::Off)
            .expect("the Game is played");

        let replay = replaying.replay(&game.record).expect("the record replays");

        assert!(replay.creation == game.creation, "seed {seed}: creation");
        assert_eq!(replay.traces.len(), game.record.actions().len());
        assert!(replay.traces == game.traces, "seed {seed}: Event Traces");
        assert!(replay.state == game.state, "seed {seed}: final GameState");
    }
}

#[test]
fn a_cascade_of_triggers_replays_identically() {
    let ruleset = effects();
    let decks = effects_decks(&ruleset);
    let pyre_imp = CardId::new("pyre_imp");

    // The first Game over the pinned Seeds in which a Pyre Imp is played: playing it moves it to
    // the field, its first trigger moves it to the trash, and that wakes its second trigger,
    // which draws a card: a cascade, in the Events of one Action.
    let (game, index) = (0..PINNED_SEEDS)
        .find_map(|seed| {
            let game = self_play(&ruleset, &decks, Seed(seed), MAX_STEPS, Checks::Off)
                .expect("the Game is played");
            let index = game
                .record
                .actions()
                .iter()
                .zip(steps(&ruleset, &game))
                .position(|(action, step)| {
                    action.action.as_str() == "play_creature"
                        && matches!(
                            action.arguments.as_slice(),
                            [Argument::Card(card)]
                                if step.prior.card(*card).is_some_and(|played| played.definition() == &pyre_imp)
                        )
                })?;
            Some((game, index))
        })
        .expect("a Pyre Imp is played in the pinned Games");

    let action = &game.record.actions()[index];
    let Some(Argument::Card(imp)) = action.arguments.first() else {
        panic!("the play names the card");
    };
    let events = game.traces[index].events();
    let position = |wanted: &dyn Fn(&Event) -> bool| events.iter().position(wanted);
    let to_field = position(&|event| {
        matches!(event, Event::CardMoved { card, to, .. } if card == imp && to.as_str() == "field")
    })
    .expect("the play moves the imp to the field");
    let to_trash = position(&|event| {
        matches!(event, Event::CardMoved { card, from, to, .. }
            if card == imp && from.as_str() == "field" && to.as_str() == "trash")
    })
    .expect("its first trigger moves it to the trash");
    let drew = events
        .iter()
        .enumerate()
        .skip(to_trash.saturating_add(1))
        .find_map(|(index, event)| {
            matches!(event, Event::CardMoved { player, from, to, .. }
                if *player == action.player && from.as_str() == "deck" && to.as_str() == "hand")
            .then_some(index)
        })
        .expect("its second trigger draws a card after the imp reaches the trash");
    assert!(
        to_field < to_trash && to_trash < drew,
        "the Events cascade in order: {events:?}"
    );

    // The record replays, to the same traces event for event and the same final GameState.
    let replay = effects().replay(&game.record).expect("the record replays");
    assert_eq!(replay.traces.len(), game.traces.len());
    for (i, (replayed, original)) in replay.traces.iter().zip(&game.traces).enumerate() {
        assert!(
            replayed.events() == original.events(),
            "Action index {i}: the replayed Events differ"
        );
    }
    assert!(replay.state == game.state, "final GameState");

    // The record verifies too, with every Invariant checked.
    let verified = verify_record(&ruleset, &game.record).expect("the record verifies");
    assert!(verified.traces == game.traces, "verified Event Traces");
}

#[test]
fn effects_self_play_with_the_same_seed_is_deterministic() {
    let ruleset = effects();
    let decks = effects_decks(&ruleset);
    let first = self_play(&ruleset, &decks, Seed(7), MAX_STEPS, Checks::On).expect("played");
    let second = self_play(&ruleset, &decks, Seed(7), MAX_STEPS, Checks::On).expect("played");
    assert!(first.state == second.state, "final GameState");
    assert!(first.traces == second.traces, "Event Traces");
    assert!(first.record == second.record, "records");
    assert!(first == second, "the whole Game");

    // The checks only read: switching them off changes nothing.
    let unchecked = self_play(&ruleset, &decks, Seed(7), MAX_STEPS, Checks::Off).expect("played");
    assert!(first == unchecked, "the same Game with checks off");

    let other = self_play(&ruleset, &decks, Seed(8), MAX_STEPS, Checks::On).expect("played");
    assert_ne!(first.record.actions(), other.record.actions());
}

#[test]
fn the_small_ruleset_is_frozen_since_phase_c() {
    let changed: Vec<String> = SMALL_RULESET_FREEZE
        .iter()
        .filter_map(|(name, text, pinned)| {
            let digest = fnv1a(FNV_START, text.as_bytes());
            (digest != *pinned)
                .then(|| format!("{name}: digest {digest:016x}, pinned {pinned:016x}"))
        })
        .collect();
    assert!(
        changed.is_empty(),
        "the Small Ruleset is frozen since Phase C (ADR-0022), and its Games with it, but these changed:\n{}",
        changed.join("\n")
    );

    // The Effects sample decks are not the Small ones.
    assert_ne!(EFFECTS_DECK_A, DECK_A);
    assert_ne!(EFFECTS_DECK_B, DECK_B);
}

#[test]
fn effects_observing_never_changes_a_game() {
    // The same Seed gives the same Game with every check on, with the engine's checks only, and
    // with none. `EngineOnly` lists with rejections as `On` does, so even the count of Trigger Cap
    // Rejections is the same; `Off` lists through `legal_actions` and sees none (ADR-0026).
    let ruleset = effects();
    let decks = effects_decks(&ruleset);
    for seed in [7, 8, 9, 10, 11] {
        let on = self_play(&ruleset, &decks, Seed(seed), MAX_STEPS, Checks::On).expect("played");
        let engine_only =
            self_play(&ruleset, &decks, Seed(seed), MAX_STEPS, Checks::EngineOnly).expect("played");
        let off = self_play(&ruleset, &decks, Seed(seed), MAX_STEPS, Checks::Off).expect("played");
        assert!(on == engine_only, "seed {seed}: On and EngineOnly");
        assert!(on.record == off.record, "seed {seed}: records");
        assert!(on.creation == off.creation, "seed {seed}: creation");
        assert!(on.traces == off.traces, "seed {seed}: Event Traces");
        assert!(on.state == off.state, "seed {seed}: final GameState");
        assert_eq!(off.trigger_cap_rejections, 0);
    }
}
