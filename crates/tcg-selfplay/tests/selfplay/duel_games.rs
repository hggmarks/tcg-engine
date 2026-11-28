//! The correctness net over the Duel Ruleset (ticket #96, ADR-0041; v1 since ticket #97, ADR-0042;
//! v2 since ticket #98, ADR-0043; v3 since ticket #99, ADR-0044; the net since ticket #100,
//! ADR-0045): its sample decks are legal through the engine's Deck List validation; the 1,000
//! pinned Seeds end with every Invariant checked (`Checks::On`: the twelve Invariants, the leak
//! check included), counting what the Games exercised and pinning their records and Event Traces
//! (the Duel Self-Play Pin); Games replay exactly and are deterministic from the same Seed.
//!
//! Everything runs on the real Duel Ruleset, catalog and sample decks.

use std::sync::OnceLock;

use tcg_engine::declarative::{
    Action, Argument, CardInstanceId, Event, GameState, ObservedEvent, ObservedGameState, Viewer,
};
use tcg_engine::player::PlayerId;
use tcg_engine::random::Seed;
use tcg_engine::ruleset::{Observations, Ruleset};
use tcg_engine::ruleset_definition::{ActionId, FieldName, FieldValue, PhaseId, StateName, ZoneId};
use tcg_selfplay::{self_play, verify_record, Checks, SelfPlayGame, TriggerCapRejections};

use crate::common::{
    declared, digest_line, duel, duel_decks, edit, pin_difference, DUEL_DECK_A, DUEL_DECK_B,
    MAX_STEPS,
};
use tcg_loader::load_deck_list;

/// The Seeds of the replay tests below: `0..SEEDS`.
const SEEDS: u64 = 100;

/// Whether the record of `game` holds the declared action `id`.
fn holds_action(game: &SelfPlayGame, id: &str) -> bool {
    game.record.actions().iter().any(|action| {
        declared(action).is_some_and(|(_, declared_id, _)| declared_id == &ActionId::new(id))
    })
}

/// Whether the record of `game` holds a tribute summon (not a set).
fn holds_tribute_summon(game: &SelfPlayGame) -> bool {
    ["tribute_summon_one", "tribute_summon_two"]
        .iter()
        .any(|id| holds_action(game, id))
}

#[test]
fn the_duel_sample_decks_are_legal() {
    let ruleset = duel();
    for (name, yaml) in [("A", DUEL_DECK_A), ("B", DUEL_DECK_B)] {
        // The engine's validation accepts the deck: the input exists, every card is a monster, the
        // 40 cards have at most 3 copies each.
        let deck = load_deck_list(ruleset.definition(), ruleset.catalog(), yaml)
            .unwrap_or_else(|error| panic!("deck {name} is not legal: {error:?}"));
        assert_eq!(deck.inputs().len(), 1);
    }

    // The main deck admits monsters and spells (ticket #99): both decks hold spells, and the
    // validation accepted them.
    let creation = ruleset
        .create_game(&duel_decks(&ruleset), Seed(0))
        .expect("the decks create a game");
    for seat in 0..2 {
        let spells = creation
            .opening
            .state
            .cards()
            .iter()
            .filter(|card| card.owner() == PlayerId(seat))
            .filter(|card| {
                ruleset
                    .catalog()
                    .card(card.definition())
                    .is_some_and(|definition| definition.card_type().as_str() == "spell")
            })
            .count();
        assert!(spells > 0, "the deck of seat {seat} holds no spell");
    }

    // The same validation refuses a deck that is not legal, so the check above means something: a
    // fourth copy of a card, and a deck of 39 cards.
    let four_copies = edit(
        DUEL_DECK_A,
        "{ card: lantern_moth, count: 3 }",
        "{ card: lantern_moth, count: 4 }",
    );
    assert!(load_deck_list(ruleset.definition(), ruleset.catalog(), &four_copies).is_err());
    let short = edit(
        DUEL_DECK_A,
        "{ card: lantern_moth, count: 3 }",
        "{ card: lantern_moth, count: 2 }",
    );
    assert!(load_deck_list(ruleset.definition(), ruleset.catalog(), &short).is_err());
}

/// The Seeds of the Duel Self-Play Pin: `0..PINNED_SEEDS`.
const PINNED_SEEDS: u64 = 1000;

/// The Seeds whose records are also verified (replayed, with every Invariant checked at each
/// step) in the same pass: the Seeds of the 100-Seed test this pass replaced (ticket #100).
const VERIFIED_SEEDS: u64 = 100;

/// The committed pin of the Game Records and Event Traces of the Seeds `0..PINNED_SEEDS` over the
/// Duel Ruleset, one `seed actions digest` line per Seed, in the format of the Small, Effects and
/// Response pins.
const DIGESTS: &str = include_str!("golden/duel-self-play-seeds-0-1000.digest");

const DIGESTS_PATH: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/selfplay/golden/duel-self-play-seeds-0-1000.digest"
);

/// What the Duel Games of the pinned Seeds exercised, counted from the Game Records and the Event
/// Traces only (ADR-0045), as ADR-0035 counts Choices and ADR-0039 the Response uses. Each is a
/// count of occurrences over all Games, except the two wins, which count Games. Each is asserted
/// at least 1.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
struct DuelUse {
    /// `tribute_summon_one` Actions: a summon with 1 tribute.
    one_tribute_summons: usize,
    /// `tribute_summon_two` Actions: a summon with 2 tributes.
    two_tribute_summons: usize,
    /// `attack` Actions whose trace sends both the attacker and the target to the Graveyard: an
    /// ATK-against-ATK tie.
    ties: usize,
    /// `attack` Actions that change the Life Points of the attacker's owner and move no card: the
    /// attacker's ATK below the defence-position target's DEF.
    higher_def_damage: usize,
    /// `direct_attack` Actions.
    direct_attacks: usize,
    /// Cards that went from the Spell/Trap Zone to the Graveyard: a spell resolved by its own
    /// trigger (ADR-0044). It also counts a `breaking_bolt` played with no Candidate: the spell
    /// still moves to the Graveyard and only its effect is skipped.
    spells_resolved: usize,
    /// Games won on Life Points: exactly one Loss, for `life_out`.
    life_point_wins: usize,
    /// Games won by a draw from an empty deck: exactly one Loss, for `deck_out`.
    deck_out_wins: usize,
    /// `set_monster` Actions (ticket #98).
    sets: usize,
    /// `change_position` Actions (ticket #98).
    position_changes: usize,
}

impl DuelUse {
    fn add(&mut self, other: DuelUse) {
        self.one_tribute_summons += other.one_tribute_summons;
        self.two_tribute_summons += other.two_tribute_summons;
        self.ties += other.ties;
        self.higher_def_damage += other.higher_def_damage;
        self.direct_attacks += other.direct_attacks;
        self.spells_resolved += other.spells_resolved;
        self.life_point_wins += other.life_point_wins;
        self.deck_out_wins += other.deck_out_wins;
        self.sets += other.sets;
        self.position_changes += other.position_changes;
    }
}

/// Whether `events` hold a `CardMoved` of `card` to the Graveyard.
fn sends_to_graveyard(events: &[Event], card: CardInstanceId) -> bool {
    events.iter().any(|event| {
        matches!(
            event,
            Event::CardMoved { card: moved, to, .. }
                if *moved == card && to.as_str() == "graveyard"
        )
    })
}

/// The coverage counts of one Game, read from its record and its Event Traces alone (never by
/// replaying it).
fn duel_use(game: &SelfPlayGame) -> DuelUse {
    let mut used = DuelUse::default();
    for (action, trace) in game.record.actions().iter().zip(&game.traces) {
        let Some((player, id, arguments)) = declared(action) else {
            continue;
        };
        let events = trace.events();
        match id.as_str() {
            "tribute_summon_one" => used.one_tribute_summons += 1,
            "tribute_summon_two" => used.two_tribute_summons += 1,
            "direct_attack" => used.direct_attacks += 1,
            "set_monster" => used.sets += 1,
            "change_position" => used.position_changes += 1,
            "attack" => {
                let [Argument::Card(attacker), Argument::Card(target)] = arguments else {
                    panic!("an `attack` has an attacker and a target: {arguments:?}");
                };
                if sends_to_graveyard(events, *attacker) && sends_to_graveyard(events, *target) {
                    used.ties += 1;
                }
                let owner_damaged = events.iter().any(|event| {
                    matches!(
                        event,
                        Event::PlayerStateChanged { player: changed, state, .. }
                            if *changed == player && state.as_str() == "life_points"
                    )
                });
                let a_card_moved = events
                    .iter()
                    .any(|event| matches!(event, Event::CardMoved { .. }));
                if owner_damaged && !a_card_moved {
                    used.higher_def_damage += 1;
                }
            }
            _ => {}
        }
    }
    used.spells_resolved += game
        .traces
        .iter()
        .flat_map(|trace| trace.events())
        .filter(|event| {
            matches!(
                event,
                Event::CardMoved { from, to, .. }
                    if from.as_str() == "spell_trap_zone" && to.as_str() == "graveyard"
            )
        })
        .count();
    if let Some(outcome) = game.state.outcome() {
        if let [loss] = outcome.losses() {
            match loss.reason.as_str() {
                "life_out" => used.life_point_wins += 1,
                "deck_out" => used.deck_out_wins += 1,
                _ => {}
            }
        }
    }
    used
}

/// The number of Seed Ranges the pinned Seeds are split into (ADR-0040): one test each, run on
/// parallel threads by the test harness. Chosen by measurement (ADR-0045).
const PARTS: usize = 4;

const _: () = assert!(
    PINNED_SEEDS.is_multiple_of(PARTS as u64),
    "the Seed Ranges partition the pinned Seeds evenly"
);

/// The Seeds of Seed Range `part`: `part * LEN .. (part + 1) * LEN`, contiguous and in order.
fn seeds_of_range(part: usize) -> std::ops::Range<u64> {
    let len = PINNED_SEEDS / (PARTS as u64);
    let part = part as u64;
    part * len..(part + 1) * len
}

/// What one Seed Range's Games gave, computed once per test process by `seed_range` (ADR-0040).
struct SeedRangePlay {
    /// One line per failed check.
    failures: Vec<String>,
    /// The most Actions of any Game of the range.
    longest: usize,
    /// The `digest_line` of every Seed of the range, in Seed order.
    digests: Vec<String>,
    /// The coverage counts of the range's Games, summed.
    used: DuelUse,
    /// The Games of the range that ended in a drawn Game.
    drawn: usize,
    /// The Losses of the range's Games for `life_out`, then for `deck_out`.
    losses: (usize, usize),
}

/// Plays every Seed of Seed Range `part` under `Checks::On` and checks each Game. Failures are
/// collected as data, never raised, so the part test reports its own lines.
fn play_seed_range(part: usize) -> SeedRangePlay {
    let ruleset = duel();
    let decks = duel_decks(&ruleset);
    let mut play = SeedRangePlay {
        failures: Vec::new(),
        longest: 0,
        digests: Vec::new(),
        used: DuelUse::default(),
        drawn: 0,
        losses: (0, 0),
    };
    for seed in seeds_of_range(part) {
        match self_play(&ruleset, &decks, Seed(seed), MAX_STEPS, Checks::On) {
            Ok(game) => {
                play.used.add(duel_use(&game));
                match game.state.outcome() {
                    None => play
                        .failures
                        .push(format!("seed {seed}: the Game has no Terminal Outcome")),
                    Some(outcome) => {
                        play.drawn += usize::from(outcome.is_drawn_game());
                        for loss in outcome.losses() {
                            match loss.reason.as_str() {
                                "life_out" => play.losses.0 += 1,
                                "deck_out" => play.losses.1 += 1,
                                other => play
                                    .failures
                                    .push(format!("seed {seed}: unknown reason `{other}`")),
                            }
                        }
                    }
                }
                if game.trigger_cap_rejections != TriggerCapRejections::Counted(0) {
                    play.failures.push(format!(
                        "seed {seed}: {}, but no shipped deck lists a card that reaches the cap",
                        game.trigger_cap_rejections
                    ));
                }
                if seed < VERIFIED_SEEDS {
                    if let Err(failure) = verify_record(&ruleset, &game.record) {
                        play.failures.push(format!(
                            "seed {seed}: the record does not verify: {:?}",
                            failure.kind
                        ));
                    }
                }
                play.longest = play.longest.max(game.record.actions().len());
                play.digests.push(digest_line(seed, &game));
            }
            Err(failure) => play.failures.push(format!(
                "seed {}: failed after Action index {:?}: {:?}",
                failure.seed.0, failure.action_index, failure.kind
            )),
        }
    }
    play
}

/// Each Seed Range is a pure function of its Seeds, played at most once per test process: the part
/// test and the joining test both read it here, and whichever reaches a range first plays it while
/// any other caller waits. Test scaffolding only; no engine state (ADR-0040).
static SEED_RANGES: [OnceLock<SeedRangePlay>; PARTS] = [const { OnceLock::new() }; PARTS];

fn seed_range(part: usize) -> &'static SeedRangePlay {
    SEED_RANGES[part].get_or_init(|| play_seed_range(part))
}

/// The part tests assert, for every Game of their Seed Range: a Terminal Outcome,
/// `TriggerCapRejections::Counted(0)`, `verify_record` on Seeds 0..100 and no `FailureKind`.
fn assert_seed_range_holds(part: usize) {
    let play = seed_range(part);
    assert!(
        play.failures.is_empty(),
        "Seeds {:?}:\n{}",
        seeds_of_range(part),
        play.failures.join("\n")
    );
}

/// The part tests below are written by hand for four Seed Ranges: changing `PARTS` means renaming
/// them (ADR-0040).
const _: () = assert!(
    PARTS == 4,
    "the part tests are written for four Seed Ranges: add or remove part tests and rename them"
);

#[test]
fn duel_self_play_ends_every_game_over_seeds_0_to_250_with_every_invariant_holding() {
    assert_seed_range_holds(0);
}

#[test]
fn duel_self_play_ends_every_game_over_seeds_250_to_500_with_every_invariant_holding() {
    assert_seed_range_holds(1);
}

#[test]
fn duel_self_play_ends_every_game_over_seeds_500_to_750_with_every_invariant_holding() {
    assert_seed_range_holds(2);
}

#[test]
fn duel_self_play_ends_every_game_over_seeds_750_to_1000_with_every_invariant_holding() {
    assert_seed_range_holds(3);
}

#[test]
fn the_duel_seed_ranges_cover_seeds_0_to_1000_once() {
    let mut next = 0;
    for part in 0..PARTS {
        let seeds = seeds_of_range(part);
        assert_eq!(
            seeds.start, next,
            "the Seed Range {part} starts where the last one ended"
        );
        assert!(
            seeds.start < seeds.end,
            "the Seed Range {part} is not empty"
        );
        next = seeds.end;
    }
    assert_eq!(
        next, PINNED_SEEDS,
        "the Seed Ranges end at the last pinned Seed"
    );
}

/// The last step: the coverage counts summed over every Seed Range, and the Duel Self-Play Pin
/// compared whole over the digest lines of all 1,000 Seeds in Seed order. It reads the Seed Ranges
/// through the same cache as the part tests (ADR-0040), so each range is played once per process.
#[test]
fn duel_self_play_covers_every_duel_use_and_matches_the_pin_over_1000_seeds() {
    let plays: Vec<&SeedRangePlay> = (0..PARTS).map(seed_range).collect();
    for (part, play) in plays.iter().enumerate() {
        assert!(
            play.failures.is_empty(),
            "the Seeds {:?} have failures, listed by the part test for that Seed Range",
            seeds_of_range(part)
        );
    }
    let longest = plays.iter().map(|play| play.longest).max().unwrap_or(0);
    let mut used = DuelUse::default();
    let mut digests = Vec::new();
    let (mut drawn, mut life_out, mut deck_out) = (0, 0, 0);
    for play in &plays {
        used.add(play.used);
        digests.extend(play.digests.iter().cloned());
        drawn += play.drawn;
        life_out += play.losses.0;
        deck_out += play.losses.1;
    }
    // Reported, not asserted: the Losses by reason (a drawn Game counts both), the drawn Games, the
    // longest Game in Actions, and every count.
    println!(
        "duel self-play, Seeds 0..{PINNED_SEEDS}: {life_out} life_out losses, {deck_out} deck_out losses, {drawn} drawn Games, longest {longest} Actions; {used:?}"
    );
    assert!(
        longest * 2 < MAX_STEPS,
        "the Step Cap of {MAX_STEPS} has no headroom over the longest Game ({longest} Actions)"
    );

    // The Uniform Random Policy reaches every count over the 1,000 Games (ADR-0045). A count of 0
    // means the sample decks stopped reaching a card; change the decks, not the policy.
    for (name, count) in [
        ("one_tribute_summons", used.one_tribute_summons),
        ("two_tribute_summons", used.two_tribute_summons),
        ("ties", used.ties),
        ("higher_def_damage", used.higher_def_damage),
        ("direct_attacks", used.direct_attacks),
        ("spells_resolved", used.spells_resolved),
        ("life_point_wins", used.life_point_wins),
        ("deck_out_wins", used.deck_out_wins),
        ("sets", used.sets),
        ("position_changes", used.position_changes),
    ] {
        assert!(
            count >= 1,
            "coverage count {name} is 0 over Seeds 0..1000: {used:?}"
        );
    }

    // The summed counts are exactly the ones ADR-0045 measured, so a broken sum over the Seed
    // Ranges fails here and not only a count of 0. If the decks or the policy change on purpose,
    // update the values with the pin.
    assert_eq!(
        used,
        DuelUse {
            one_tribute_summons: 3223,
            two_tribute_summons: 1045,
            ties: 256,
            higher_def_damage: 2100,
            direct_attacks: 697,
            spells_resolved: 6610,
            life_point_wins: 915,
            deck_out_wins: 85,
            sets: 8744,
            position_changes: 18002,
        },
        "the coverage counts over Seeds 0..1000 are not the ones of ADR-0045's table"
    );

    // The same Games, played once: their Game Records and Event Traces are the pinned ones.
    if let Some(difference) = pin_difference(DIGESTS, &digests) {
        panic!(
            "{difference}\nIf the change is deliberate, run: cargo test -p tcg-selfplay --test selfplay -- --ignored regenerate_duel_self_play_digests\n(coverage so far: {used:?}, longest Game {longest} Actions)"
        );
    }
}

/// Rewrites the pin of the 1,000 Duel Games. Run it by name when a rule, an Event's shape or a
/// shipped Duel deck changes on purpose, and read the diff before committing it. It writes that
/// one file.
#[test]
#[ignore = "deliberate update: rewrites the Duel self-play pin"]
fn regenerate_duel_self_play_digests() {
    let ruleset = duel();
    let decks = duel_decks(&ruleset);
    let mut text = String::new();
    for seed in 0..PINNED_SEEDS {
        let game = self_play(&ruleset, &decks, Seed(seed), MAX_STEPS, Checks::On)
            .expect("the Game is played");
        text.push_str(&digest_line(seed, &game));
    }
    std::fs::write(DIGESTS_PATH, text).expect("the Duel self-play pin is written");
}

#[test]
fn duel_self_play_is_deterministic_from_the_same_seed() {
    let ruleset = duel();
    let decks = duel_decks(&ruleset);
    for seed in [0, 1, 7, 42] {
        let play = || {
            self_play(&ruleset, &decks, Seed(seed), MAX_STEPS, Checks::On)
                .expect("the Game is played")
        };
        let (first, second) = (play(), play());
        assert_eq!(first.record, second.record, "Seed {seed}: the record");
        assert_eq!(first.creation, second.creation, "Seed {seed}: the creation");
        assert_eq!(
            first.traces, second.traces,
            "Seed {seed}: every Event Trace"
        );
        assert_eq!(
            first.state, second.state,
            "Seed {seed}: the final GameState"
        );
    }

    // Another Seed plays another Game.
    let one = self_play(&ruleset, &decks, Seed(0), MAX_STEPS, Checks::On).expect("played");
    let other = self_play(&ruleset, &decks, Seed(1), MAX_STEPS, Checks::On).expect("played");
    assert_ne!(one.record.actions(), other.record.actions());
}

#[test]
fn replay_reproduces_a_duel_game() {
    let ruleset = duel();
    let decks = duel_decks(&ruleset);
    for seed in [0, 3, 11] {
        let game = self_play(&ruleset, &decks, Seed(seed), MAX_STEPS, Checks::On).expect("played");
        let replay = ruleset
            .replay(&game.record)
            .expect("the record of a self-played Game replays");
        assert_eq!(replay.creation, game.creation, "Seed {seed}: the creation");
        assert_eq!(replay.traces, game.traces, "Seed {seed}: every Event Trace");
        assert_eq!(replay.state, game.state, "Seed {seed}: the final GameState");
        assert!(
            replay.state.outcome().is_some(),
            "Seed {seed}: the Terminal Outcome is reproduced"
        );
        assert_eq!(replay.state.outcome(), game.state.outcome());
        // The Game visits Battle (ticket #97): the replay reproduces the phase change.
        let battle = Event::PhaseBegan {
            phase: PhaseId::new("battle"),
        };
        assert!(
            replay
                .traces
                .iter()
                .any(|trace| trace.events().contains(&battle)),
            "Seed {seed}: the replayed Game has a PhaseBegan of Battle"
        );
    }
}

#[test]
fn replay_reproduces_a_duel_game_holding_a_fight_between_monsters() {
    let ruleset = duel();
    let decks = duel_decks(&ruleset);
    // Games that hold an `attack`, a fight between monsters with its destruction or its Life Point
    // change, replay exactly: the full GameState and the full Event Trace.
    let mut replayed = 0usize;
    for seed in 0..SEEDS {
        let game = self_play(&ruleset, &decks, Seed(seed), MAX_STEPS, Checks::On).expect("played");
        if !holds_action(&game, "attack") {
            continue;
        }
        let replay = ruleset
            .replay(&game.record)
            .expect("the record of a self-played Game replays");
        assert_eq!(replay.creation, game.creation, "Seed {seed}: the creation");
        assert_eq!(replay.traces, game.traces, "Seed {seed}: every Event Trace");
        assert_eq!(replay.state, game.state, "Seed {seed}: the final GameState");
        replayed += 1;
        if replayed == 3 {
            break;
        }
    }
    assert_eq!(replayed, 3, "three Games hold a fight between monsters");
}

#[test]
fn replay_reproduces_a_duel_game_holding_a_tribute_summon_and_a_spell() {
    let ruleset = duel();
    let decks = duel_decks(&ruleset);
    // Games that hold a tribute summon or set and a spell, with its cost, its trigger and, for
    // Breaking Bolt, its Choice, replay exactly: the full creation, every Event Trace and the final
    // GameState.
    let mut replayed = 0usize;
    for seed in 0..SEEDS {
        let game = self_play(&ruleset, &decks, Seed(seed), MAX_STEPS, Checks::On).expect("played");
        if !(holds_tribute_summon(&game) && holds_action(&game, "play_spell")) {
            continue;
        }
        let replay = ruleset
            .replay(&game.record)
            .expect("the record of a self-played Game replays");
        assert_eq!(replay.creation, game.creation, "Seed {seed}: the creation");
        assert_eq!(replay.traces, game.traces, "Seed {seed}: every Event Trace");
        assert_eq!(replay.state, game.state, "Seed {seed}: the final GameState");
        replayed += 1;
        if replayed == 3 {
            break;
        }
    }
    assert_eq!(replayed, 3, "three Games hold a tribute summon and a spell");
}

fn viewer(seat: u32) -> Viewer {
    Viewer::Player(PlayerId(seat))
}

/// The instance state `name` of each monster in the Monster Zone of `owner`, as the Viewer of
/// `observed` sees it.
fn observed_monster_states(
    observed: &ObservedGameState,
    owner: u32,
    name: &str,
) -> Vec<FieldValue> {
    let zone = observed
        .zone(PlayerId(owner), &ZoneId::new("monster_zone"))
        .expect("the Monster Zone is observed");
    zone.cards()
        .expect("the Monster Zone is public")
        .iter()
        .map(|&handle| {
            let card = observed.card(handle).expect("the monster is visible");
            card.state()
                .iter()
                .find(|(state, _)| state == &StateName::new(name))
                .map(|(_, value)| value.clone())
                .unwrap_or_else(|| panic!("the monster has no `{name}`"))
        })
        .collect()
}

#[test]
fn observations_show_each_monsters_position() {
    let ruleset = duel();
    let decks = duel_decks(&ruleset);
    let creation = ruleset.create_game(&decks, Seed(1)).expect("created");
    let declare = |player: u32, id: &str, arguments: Vec<Argument>| Action::Declared {
        player: PlayerId(player),
        action: ActionId::new(id),
        arguments,
    };
    // The first card of the hand that can be normal summoned or set: a monster of level 4 or less
    // (the hand may hold spells and monsters that need tributes since ticket #99).
    let first_in_hand = |state: &GameState, seat: u32| {
        state
            .zone(PlayerId(seat), &ZoneId::new("hand"))
            .expect("the hand exists")
            .iter()
            .copied()
            .find(|&card| {
                let definition = state.card(card).expect("the card exists").definition();
                matches!(
                    ruleset
                        .catalog()
                        .card(definition)
                        .and_then(|definition| definition.field(&FieldName::new("level"))),
                    Some(FieldValue::Int(1..=4))
                )
            })
            .expect("the hand holds a monster of level 4 or less")
    };

    // Both Viewers follow a Game from the Empty GameState: seat 0 summons, then seat 1 sets.
    let mut observers: Vec<_> = (0..2)
        .map(|seat| {
            ruleset
                .observer(
                    viewer(seat),
                    &creation.empty,
                    creation.opening.trace.events(),
                )
                .expect("the Observer is built")
        })
        .collect();
    let mut observed_events: Vec<Vec<ObservedEvent>> = vec![Vec::new(), Vec::new()];
    let mut state = creation.opening.state.clone();
    for step in 0..3 {
        let action = match step {
            0 => declare(
                0,
                "normal_summon",
                vec![Argument::Card(first_in_hand(&state, 0))],
            ),
            1 => declare(0, "end_turn", vec![]),
            _ => declare(
                1,
                "set_monster",
                vec![Argument::Card(first_in_hand(&state, 1))],
            ),
        };
        let transition = ruleset
            .transition(&state, &action)
            .expect("the script's Action is accepted");
        for (seat, observer) in observers.iter_mut().enumerate() {
            let followed = ruleset
                .follow_transition(observer, &state, &action, &transition)
                .expect("the transition is followed");
            observed_events[seat].extend(followed.events);
        }
        state = transition.state;
    }

    let defence = FieldValue::Text("defence".to_string());
    for (seat, observer) in observers.iter().enumerate() {
        let observed = ruleset
            .observe(observer, &state)
            .expect("the GameState is observed");
        // Every Viewer sees both monsters' position, the set one included (face-up this phase).
        assert_eq!(
            observed_monster_states(&observed, 0, "position"),
            vec![FieldValue::Text("attack".to_string())],
            "Viewer {seat}: the summoned monster"
        );
        assert_eq!(
            observed_monster_states(&observed, 1, "position"),
            vec![defence.clone()],
            "Viewer {seat}: the set monster"
        );
        // The set reached the Viewer as an Observed Event.
        assert!(
            observed_events[seat].iter().any(|event| matches!(
                event,
                ObservedEvent::CardStateChanged { state, value, .. }
                    if state == &StateName::new("position") && value == &defence
            )),
            "Viewer {seat}: the position change is observed"
        );
    }
}
