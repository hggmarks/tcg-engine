//! Acceptance test for ticket #26: the examples of `rulesets/authoring-guide.md` load. Each example
//! is a fenced `yaml` block directly after a marker line such as `<!-- guide-example: ruleset -->`.

use tcg_engine::declarative::{Action, DeclarativeRuleset};
use tcg_engine::player::PlayerId;
use tcg_engine::random::Seed;
use tcg_engine::ruleset::Ruleset;
use tcg_engine::ruleset_definition::ActionId;
use tcg_loader::{load_card_catalog, load_deck_list, load_ruleset_definition};

const GUIDE: &str = include_str!("../../../rulesets/authoring-guide.md");

/// The text of the `yaml` block that follows the marker line `<!-- guide-example: {name} -->`.
fn example(name: &str) -> String {
    let marker = format!("<!-- guide-example: {name} -->");
    let lines: Vec<&str> = GUIDE.lines().collect();
    let marker_at = lines
        .iter()
        .position(|line| line.trim() == marker)
        .unwrap_or_else(|| panic!("the guide has the marker `{marker}`"));
    let fence_at = marker_at + 1;
    assert_eq!(
        lines.get(fence_at).map(|line| line.trim_end()),
        Some("```yaml"),
        "a `yaml` block follows `{marker}` directly"
    );
    let body: Vec<&str> = lines[fence_at + 1..]
        .iter()
        .copied()
        .take_while(|line| line.trim_end() != "```")
        .collect();
    assert!(
        fence_at + 1 + body.len() < lines.len(),
        "the block of `{marker}` is closed"
    );
    let mut text = body.join("\n");
    text.push('\n');
    text
}

#[test]
fn the_authoring_guide_examples_load_and_create_a_game() {
    let definition = load_ruleset_definition(&example("ruleset"))
        .expect("the ruleset example of the guide loads");
    let catalog = load_card_catalog(&definition, &example("catalog"))
        .expect("the catalog example of the guide loads");
    let ruleset = DeclarativeRuleset::new(definition, catalog);
    let deck = load_deck_list(
        ruleset.definition(),
        ruleset.catalog(),
        &example("deck-list"),
    )
    .expect("the deck list example of the guide loads");

    // One deck for each Player of the example.
    let players = usize::try_from(ruleset.definition().parameters().player_count().get())
        .expect("the player count fits");
    let decks = vec![deck; players];
    let creation = ruleset
        .create_game(&decks, Seed(1))
        .expect("the examples create a game");

    // The Active Player passes the turn in Main.
    let state = creation.opening.state;
    let pass = Action {
        player: state.active_player(),
        action: ActionId::new("pass_turn"),
        arguments: Vec::new(),
    };
    assert_eq!(state.active_player(), PlayerId(0));
    assert!(ruleset.transition(&state, &pass).is_ok());
}
