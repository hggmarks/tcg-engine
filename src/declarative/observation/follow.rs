//! Following transitions: the Observed Events of an Event Trace and the Observed Transition of an
//! accepted Action (ADR-0029).

#[cfg(test)]
mod tests {
    use super::*;
    use crate::card_catalog::{CardCatalog, CardCatalogDraft, CardId, CatalogMetaDraft};
    use crate::declarative::observation::viewer::{ObservedCard, Viewer};
    use crate::declarative::state::CardInstanceId;
    use crate::player::{PlayerId, PlayerOrder};
    use crate::random::{RandomState, Seed};
    use crate::ruleset::Observations;
    use crate::ruleset_definition::{
        MetaDraft, ParametersDraft, RulesetDefinition, RulesetDraft, Visibility, ZoneDraft, ZoneId,
    };

    /// Two Players, a `field` every Viewer sees and a `hand` only its owner sees; no cards defined.
    fn ruleset() -> DeclarativeRuleset {
        let definition = RulesetDefinition::validate(RulesetDraft {
            meta: MetaDraft {
                id: "unit".to_string(),
                version: 1,
            },
            parameters: ParametersDraft {
                players: 2,
                starting_player: 0,
            },
            zones: vec![
                ZoneDraft {
                    id: "field".to_string(),
                    visibility: Visibility::Public,
                    ordered: true,
                },
                ZoneDraft {
                    id: "hand".to_string(),
                    visibility: Visibility::Owner,
                    ordered: true,
                },
            ],
            card_types: Vec::new(),
            inputs: Vec::new(),
            setup: Vec::new(),
            actions: Vec::new(),
            turn: None,
            win_conditions: Vec::new(),
            effects: None,
        })
        .expect("the unit ruleset is valid");
        let catalog = CardCatalog::validate(
            &definition,
            CardCatalogDraft {
                meta: CatalogMetaDraft {
                    ruleset: "unit".to_string(),
                    ruleset_version: 1,
                },
                cards: Vec::new(),
            },
        )
        .expect("the unit catalog is valid");
        DeclarativeRuleset::new(definition, catalog)
    }

    fn empty() -> GameState {
        GameState::empty(
            PlayerOrder::new(vec![PlayerId(0), PlayerId(1)]).expect("two Players"),
            PlayerId(0),
            &[ZoneId::new("field"), ZoneId::new("hand")],
            RandomState::from_seed(Seed(1)),
        )
    }

    fn created(card: u32, owner: u32, zone: &str) -> Event {
        Event::CardCreated {
            card: CardInstanceId(card),
            owner: PlayerId(owner),
            definition: CardId::new("any"),
            zone: ZoneId::new(zone),
            state: Vec::new(),
        }
    }

    fn shuffled(player: u32, zone: &str, order: &[u32], seed: u64) -> Event {
        Event::ZoneShuffled {
            player: PlayerId(player),
            zone: ZoneId::new(zone),
            order: order.iter().copied().map(CardInstanceId).collect(),
            random_state: RandomState::from_seed(Seed(seed)),
        }
    }

    #[test]
    fn a_shuffle_of_a_seen_zone_gives_the_new_order_of_observed_cards_and_of_an_unseen_zone_no_order(
    ) {
        let ruleset = ruleset();
        let start = empty();
        let viewer = Viewer::Player(PlayerId(0));
        let mut observer = ruleset
            .observer(viewer, &start, &[])
            .expect("the Viewer is a Player");

        // Seat 0 sees the public field (cards 0, 1, 2: handles 0, 1, 2) and not seat 1's hand.
        let creation = [
            created(0, 0, "field"),
            created(1, 0, "field"),
            created(2, 0, "field"),
            created(3, 1, "hand"),
            created(4, 1, "hand"),
        ];
        ruleset
            .follow_events(&mut observer, &start, &creation)
            .expect("the creation is followed");
        let created_state = start.apply_trace(&creation).expect("the cards are created");

        let shuffles = [
            shuffled(0, "field", &[2, 0, 1], 9),
            shuffled(1, "hand", &[4, 3], 10),
        ];
        let observed = ruleset
            .follow_events(&mut observer, &created_state, &shuffles)
            .expect("the shuffles are followed");

        assert_eq!(
            observed,
            vec![
                ObservedEvent::ZoneShuffled {
                    player: PlayerId(0),
                    zone: ZoneId::new("field"),
                    order: vec![ObservedCard(2), ObservedCard(0), ObservedCard(1)],
                },
                ObservedEvent::UnseenZoneShuffled {
                    player: PlayerId(1),
                    zone: ZoneId::new("hand"),
                },
            ]
        );
    }
}
