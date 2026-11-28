//! The machinery the Golden Game tests share: a Golden Game before and after it is played, the
//! text of its Event Trace snapshot (ADR-0017) and of the snapshot as each Player observed it
//! (ADR-0031), the byte-for-byte comparison with the committed file, and the replay of its record
//! (ADR-0019). `effects_ruleset.rs` (the Small and Effects Golden Games), `response_ruleset.rs`
//! (the Response Golden Game) and `duel_golden_game.rs` (the Duel Golden Game) use it.

use tcg_engine::deck_list::DeckList;
use tcg_engine::declarative::{
    Action, Answer, Argument, CardInstanceId, DeclarativeRuleset, Event, GameRecord, GameState,
    TakenAction, Viewer,
};
use tcg_engine::player::PlayerId;
use tcg_engine::random::Seed;
use tcg_engine::ruleset::{Observations, Ruleset};
use tcg_engine::ruleset_definition::ActionId;
use tcg_loader::load_deck_list;

/// The two sample decks, loaded under `ruleset`: deck A for seat 0, deck B for seat 1.
pub fn decks(ruleset: &DeclarativeRuleset, yamls: [&str; 2]) -> Vec<DeckList> {
    yamls
        .iter()
        .map(|yaml| {
            load_deck_list(ruleset.definition(), ruleset.catalog(), yaml)
                .expect("the sample deck is legal")
        })
        .collect()
}

pub fn action(player: u32, id: &str, cards: &[CardInstanceId]) -> Action {
    Action::Declared {
        player: PlayerId(player),
        action: ActionId::new(id),
        arguments: cards.iter().copied().map(Argument::Card).collect(),
    }
}

pub fn attack(player: u32, attacker: CardInstanceId, target: CardInstanceId) -> Action {
    action(player, "attack", &[attacker, target])
}

/// A Golden Game before it is played: its name (the stem of its snapshots), its Seed and its literal
/// script. The decks it is played with are the caller's.
pub struct GoldenGame {
    pub name: &'static str,
    pub seed: u64,
    pub script: Vec<Action>,
}

/// A Golden Game as it was played: the Event Trace of Game creation, each accepted Action with its
/// Event Trace, and the final GameState.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Played {
    pub name: &'static str,
    pub seed: u64,
    pub creation: Vec<Event>,
    pub steps: Vec<(Action, Vec<Event>)>,
    pub final_state: GameState,
}

/// Plays the literal script. Every Action must be accepted.
pub fn play_game(ruleset: &DeclarativeRuleset, decks: &[DeckList], game: &GoldenGame) -> Played {
    let name = game.name;
    let created = ruleset
        .create_game(decks, Seed(game.seed))
        .expect("the sample decks create a game");
    let creation = created.opening.trace.events().to_vec();
    let mut state = created.opening.state;
    let mut steps = Vec::new();
    for (index, action) in game.script.iter().enumerate() {
        let transition = ruleset
            .transition(&state, action)
            .unwrap_or_else(|rejection| {
                panic!(
                    "action {} of {name} ({action:?}) is rejected: {rejection:?}",
                    index + 1
                )
            });
        steps.push((action.clone(), transition.trace.events().to_vec()));
        state = transition.state;
    }
    Played {
        name,
        seed: game.seed,
        creation,
        steps,
        final_state: state,
    }
}

/// The text of a Golden Snapshot (ADR-0017), in the format of the Small Ruleset's snapshots.
pub fn render(played: &Played) -> String {
    let mut text = format!(
        "# {} | seed {} | seat 0 deck-a, seat 1 deck-b\n== creation\n",
        played.name, played.seed
    );
    for event in &played.creation {
        text.push_str(&format!("{event:?}\n"));
    }
    for (index, (action, events)) in played.steps.iter().enumerate() {
        match action {
            Action::Declared {
                player,
                action: id,
                arguments,
            } => {
                let cards: Vec<String> = arguments
                    .iter()
                    .map(|argument| match argument {
                        Argument::Card(card) => card.0.to_string(),
                    })
                    .collect();
                text.push_str(&format!(
                    "== action {}: player {} {} [{}]\n",
                    index + 1,
                    player.0,
                    id.as_str(),
                    cards.join(", ")
                ));
            }
            // An Answer is written `answer option <index>` or `answer card <id>`: a Golden Game
            // that pauses holds them (ADR-0035).
            Action::Answer { player, answer } => {
                let answer = match answer {
                    Answer::Option(option) => format!("option {option}"),
                    Answer::Card(card) => format!("card {}", card.0),
                };
                text.push_str(&format!(
                    "== action {}: player {} answer {answer}\n",
                    index + 1,
                    player.0
                ));
            }
            // A Pass and an Activate: the Golden Game of the Response Ruleset holds them
            // (ADR-0039); no Small or Effects one does, those rulesets having no Response Window.
            Action::Pass { player } => {
                text.push_str(&format!(
                    "== action {}: player {} pass\n",
                    index + 1,
                    player.0
                ));
            }
            Action::Activate {
                player,
                card,
                ability,
            } => {
                text.push_str(&format!(
                    "== action {}: player {} activate card {} ability {ability}\n",
                    index + 1,
                    player.0,
                    card.0
                ));
            }
        }
        for event in events {
            text.push_str(&format!("{event:?}\n"));
        }
    }
    let outcome = played
        .final_state
        .outcome()
        .expect("a Golden Game ends the Game");
    text.push_str(&format!("== outcome\n{outcome:?}\n"));
    text
}

/// Compares a Golden Game's text with its committed snapshot byte for byte and reports the first
/// differing line, with the command that rewrites the snapshot. A snapshot ends with exactly one
/// newline.
pub fn assert_matches_snapshot(what: &str, snapshot: &str, actual: &str, regenerate: &str) {
    if actual != snapshot {
        let mut expected_lines = snapshot.lines();
        let mut actual_lines = actual.lines();
        for line_number in 1.. {
            let (expected, found) = (expected_lines.next(), actual_lines.next());
            assert!(
                expected == found,
                "the Golden Game {what} differs from its snapshot at line {line_number}\n  \
                 expected: {}\n  actual:   {}\nIf the change is deliberate, rewrite the \
                 snapshot with:\n  {regenerate}",
                expected.unwrap_or("<end of the snapshot>"),
                found.unwrap_or("<end of the trace>"),
            );
            if expected.is_none() {
                break;
            }
        }
        panic!(
            "the snapshot of the Golden Game {what} differs in its line endings; rewrite it with:\n  {regenerate}"
        );
    }
    assert!(snapshot.ends_with('\n') && !snapshot.ends_with("\n\n"));
}

/// The line of a Taken Action in the observed snapshots (ADR-0033). A Taken Declared Action keeps the
/// `Debug` line of the struct it was before `TakenAction` became an enum, `TakenAction { player: ..,
/// action: .., arguments: [..] }`, so the committed snapshots stay byte for byte what they were; an
/// Answer, a Pass and an Activate are their own `Debug`.
fn taken_line(taken: &TakenAction) -> String {
    match taken {
        TakenAction::Declared {
            player,
            action,
            arguments,
        } => format!(
            "TakenAction {{ player: {player:?}, action: {action:?}, arguments: {arguments:?} }}"
        ),
        TakenAction::Answer { .. } | TakenAction::Pass { .. } | TakenAction::Activate { .. } => {
            format!("{taken:?}")
        }
    }
}

/// The text of the snapshot of the Golden Game `game`, played with `decks` under `ruleset`, as seat
/// `seat` observed it: the Observer follows the creation trace and then each accepted Action of
/// the script. Each snapshot is in the format of ADR-0017 with Observed Events in place of Events:
/// a header, then `== creation` and one `Debug` line per Observed Event of the creation trace,
/// then for each accepted Action `== action N`, the `Debug` line of its Taken Action and one
/// `Debug` line per Observed Event of its transition, then `== outcome` and the Terminal Outcome
/// as the Viewer sees it.
pub fn render_observed(
    ruleset: &DeclarativeRuleset,
    decks: &[DeckList],
    game: &GoldenGame,
    seat: u32,
) -> String {
    let created = ruleset
        .create_game(decks, Seed(game.seed))
        .expect("the sample decks create a game");
    let mut observer = ruleset
        .observer(Viewer::Player(PlayerId(seat)), &created.empty, &[])
        .expect("the Observer is built");
    let creation = ruleset
        .follow_events(
            &mut observer,
            &created.empty,
            created.opening.trace.events(),
        )
        .expect("the creation trace is followed");
    let mut text = format!(
        "# {} observed by player {seat} | seed {} | seat 0 deck-a, seat 1 deck-b\n== creation\n",
        game.name, game.seed
    );
    for event in &creation {
        text.push_str(&format!("{event:?}\n"));
    }
    let mut state = created.opening.state;
    let mut last = ruleset
        .observe(&observer, &state)
        .expect("the Opening Position is observed");
    for (index, action) in game.script.iter().enumerate() {
        let transition = ruleset
            .transition(&state, action)
            .expect("the script's Action is accepted");
        let observed = ruleset
            .follow_transition(&mut observer, &state, action, &transition)
            .expect("the transition is followed");
        text.push_str(&format!(
            "== action {}\n{}\n",
            index + 1,
            taken_line(&observed.action)
        ));
        for event in &observed.events {
            text.push_str(&format!("{event:?}\n"));
        }
        state = transition.state;
        last = observed.state;
    }
    let outcome = last
        .outcome()
        .expect("a Golden Game ends the Game for every Viewer");
    text.push_str(&format!("== outcome\n{outcome:?}\n"));
    text
}

/// The Game Record of a Golden Game, built from its script, replays to the Event Traces and the
/// final GameState of the Game as it was played (ADR-0019).
pub fn assert_replaying_the_record_reproduces(
    ruleset: &DeclarativeRuleset,
    decks: &[DeckList],
    game: &GoldenGame,
) {
    let played = play_game(ruleset, decks, game);
    let mut record = GameRecord::new(ruleset, decks, Seed(game.seed));
    for action in &game.script {
        record.push_accepted(action.clone());
    }
    let replay = ruleset.replay(&record).expect("the record replays");
    assert_eq!(replay.creation.opening.trace.events(), played.creation);
    let replayed: Vec<Vec<Event>> = replay
        .traces
        .iter()
        .map(|trace| trace.events().to_vec())
        .collect();
    let original: Vec<Vec<Event>> = played
        .steps
        .iter()
        .map(|(_, events)| events.clone())
        .collect();
    assert_eq!(replayed, original, "Event Traces of {}", game.name);
    assert_eq!(
        replay.state, played.final_state,
        "final GameState of {}",
        game.name
    );
}
