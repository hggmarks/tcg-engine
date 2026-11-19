//! The Derived Read of a card the Viewer sees (ADR-0024, ADR-0025, ADR-0028).

use super::error::ObservationError;
use super::observer::{sees, Observer};
use super::viewer::{ObservedCard, Viewer};
use crate::declarative::action::Rejection;
use crate::declarative::creation::DeclarativeRuleset;
use crate::declarative::expression;
use crate::declarative::state::GameState;
use crate::ruleset_definition::{CardValue, FieldName, FieldType, FieldValue};

impl DeclarativeRuleset {
    /// A Derived Read of the int field `field` of a card the Viewer sees: its base value with the
    /// Field Modifiers that are active in `state`, self and aura, added, as the engine reads it
    /// (ADR-0024, ADR-0025). `state` is the GameState `observer` has followed.
    ///
    /// The checks come in this order: the Viewer is a Player of `state` (`UnknownViewer`); the
    /// Viewer holds `card` (`UnknownObservedCard`); the card is in `state` and in a zone the Viewer
    /// sees (`ObserverOutOfStep`); its Card Definition has an int field of that name
    /// (`NotAnIntField`); the sum fits an int (`DerivedValueOverflow`).
    pub fn derived_read(
        &self,
        observer: &Observer,
        state: &GameState,
        card: ObservedCard,
        field: &FieldName,
    ) -> Result<i64, ObservationError> {
        let viewer = observer.viewer();
        let Viewer::Player(me) = viewer;
        if !state.player_order().contains(me) {
            return Err(ObservationError::UnknownViewer { viewer });
        }
        let id = observer
            .card_instance(card)
            .ok_or(ObservationError::UnknownObservedCard { card })?;

        let out_of_step = ObservationError::ObserverOutOfStep;
        let instance = state.card(id).ok_or(out_of_step.clone())?;
        let zone = state.zone_of(id).ok_or(out_of_step.clone())?;
        if !sees(self, viewer, instance.owner(), zone)? {
            return Err(out_of_step);
        }

        let definition = self
            .catalog()
            .card(instance.definition())
            .ok_or(ObservationError::ObserverOutOfStep)?;
        let card_type = self
            .definition()
            .card_type(definition.card_type())
            .ok_or(ObservationError::ObserverOutOfStep)?;
        let is_int = card_type
            .fields()
            .iter()
            .any(|schema| schema.name() == field && schema.field_type() == FieldType::Int);
        if !is_int {
            return Err(ObservationError::NotAnIntField {
                card,
                field: field.clone(),
            });
        }

        match expression::read(self, state, id, &CardValue::Field(field.clone())) {
            Ok(FieldValue::Int(value)) => Ok(value),
            Ok(_) => Err(ObservationError::ObserverOutOfStep),
            Err(Rejection::DerivedValueOverflow { .. }) => {
                Err(ObservationError::DerivedValueOverflow {
                    card,
                    field: field.clone(),
                })
            }
            Err(_) => Err(ObservationError::ObserverOutOfStep),
        }
    }
}
