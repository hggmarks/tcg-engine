//! Errors of Ruleset Definition validation.

use std::fmt;

use super::draft::FieldTypeDraft;
use super::model::FieldType;
use crate::definition_path::DefinitionPath;

/// The first error found in a draft, and where.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DefinitionError {
    pub path: DefinitionPath,
    pub kind: DefinitionErrorKind,
}

/// What a reference points to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReferenceTarget {
    Zone,
    CardType,
    DeckInput,
    Card,
    Action,
    /// An instance-state name that no card type declares.
    InstanceState,
    /// A param that the action does not declare.
    Param,
    /// A name that the param's card type declares as neither a field nor an instance state.
    FieldOrInstanceState,
    /// A Player state that the ruleset's `player_state` does not declare (ADR-0041).
    PlayerState,
}

/// What a ref evaluates to, or what a slot needs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RefType {
    /// A card: `{ ref: <param> }`.
    Card,
    /// A value of a field type: `{ ref: <param>.<name> }`.
    Value(FieldType),
}

/// The kinds of error. The comment on each variant is the path its error carries.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DefinitionErrorKind {
    /// An id that breaks the identifier grammar. The path is positional and ends with the key that
    /// holds the id, such as `zones[3].id`, `card_types[c].fields[2].name` or `meta.id`.
    InvalidId { id: String },
    /// `parameters.players` (it is 0).
    InvalidPlayerCount { players: u32 },
    /// `parameters.starting_player`.
    InvalidStartingPlayer { starting_player: u32, players: u32 },
    /// The duplicate element, such as `zones[3]`; `first` is the earlier one, such as `zones[0]`.
    DuplicateId { id: String, first: DefinitionPath },
    /// `inputs[main_deck].size` (it is 0).
    InvalidDeckSize,
    /// `inputs[main_deck].max_copies` (it is 0 or above the size).
    InvalidMaxCopies { max_copies: u32, size: u32 },
    /// `inputs[main_deck].card_types` (it is empty).
    NoCardTypes,
    /// `inputs[main_deck].zone` or `inputs[main_deck].card_types[0]`; a step's zone; the zone of a
    /// win condition (`win_conditions[deck_out].when.draw_from_empty`); an action of a
    /// phase (`turn.phases[main].actions[0]`); the `state` of a `set_state`, or the step itself for
    /// an `untap` (no card type declares `tapped`); the `zone` or `card_type` of a param
    /// (`actions[play_creature].params[card].zone`); the zone of a cost or of a `move`; the cost
    /// itself for a `tap` when no card type declares `tapped`; the slot of a ref to a param the
    /// action lacks or to a name its card type lacks; the `state` of an action's `set_state` when
    /// the card type of its card lacks it, or a possible card type of it when the param has
    /// several; a ref to a name that is not a param, or in a win condition not a card type
    /// (`win_conditions[leader_dead].when.lte[0]`); the `owner`, `where` and `one_of` keys of a
    /// param are addressed at the param or at `one_of[i]`.
    DanglingReference { target: ReferenceTarget, id: String },
    /// `card_types[c].fields[f].type` or `card_types[c].instance_state[s].type`.
    UnsupportedFieldType { field_type: FieldTypeDraft },
    /// `win_conditions[deck_out].loser`: only `owner` exists.
    UnknownLoser { loser: String },
    /// `setup[0].shuffle.player` or `turn.phases[start].automatic[0].untap.player`: only `each` and
    /// `active` exist. The selector of a Player ref or of a Player-state write of an action or a
    /// Card Effect is addressed the same way, and the selectors that exist depend on the context
    /// (ADR-0041).
    UnknownPlayerSelector { selector: String },
    /// A `count` of a draw or a move (it is 0), such as `setup[1].draw.count` or
    /// `turn.phases[start].automatic[3].move.count`.
    InvalidCount,
    /// The `player` of a `draw` effect, such as `cards[goblin].effects[0].do[0].draw.player`: only
    /// `controller` exists.
    UnknownEffectPlayer { player: String },
    /// The `player` of a `draw` effect in an action, such as
    /// `actions[study].effects[0].draw.player`: `controller` is the owner of `self`, which only a
    /// Card Effect has.
    ControllerOutsideCardEffect,
    /// The source of a shuffle, a draw or a move (ADR-0013): a zone with `ordered: false`, such as
    /// `setup[0].shuffle.zone` or `turn.phases[start].automatic[2].else[0].draw.from`.
    ZoneNotOrdered { zone: String },
    /// `turn.phases[start].automatic[3].move.position`: only `top` exists.
    UnknownPosition { position: String },
    /// `turn.phases[end]`: a phase has exactly one of `automatic` and `actions`.
    InvalidPhaseKind,
    /// `turn.phases`: a turn has at least one phase with `actions`, or the Game would never wait
    /// for a Player (ADR-0042).
    NoActionPhase,
    /// `turn.phases[start].automatic[1].set_state.value`, or the step itself for an `untap`: the
    /// value is not the type some card type declares for the state.
    StateValueTypeMismatch {
        state: String,
        declared: FieldType,
        found: FieldType,
    },
    /// `card_types[c].instance_state[s].init`.
    InitializerTypeMismatch {
        declared: FieldType,
        found: FieldType,
    },
    /// `card_types[c].instance_state[s].init.from_field`.
    FromFieldMissing { field: String },
    /// `card_types[c].instance_state[s].init.from_field`.
    FromFieldTypeMismatch {
        field: String,
        declared: FieldType,
        found: FieldType,
    },
    /// The slot of a ref, such as `actions[play_creature].cost[0].tap.count`: the text is not
    /// `<id>` or `<id>.<id>`.
    InvalidReference { reference: String },
    /// The slot of a ref: the card type of the param declares the name as both a field and an
    /// instance state.
    AmbiguousReference { name: String },
    /// The slot of a ref: it evaluates to `found` but the slot needs `expected`.
    ReferenceTypeMismatch { expected: RefType, found: RefType },
    /// `actions[attack].params[target].owner` or `...one_of[1].owner`: only `opponent` exists.
    UnknownOwner { owner: String },
    /// `actions[attack].params[target].one_of`: the list is empty, and a param needs at least one
    /// alternative.
    NoAlternatives,
    /// The offending expression, such as `actions[attack].params[attacker].where.and[0]`: it has
    /// type `found` but its operator or its slot needs `expected`. For `eq`, `expected` is the
    /// type of the first operand and the path is the second.
    ExpressionTypeMismatch {
        expected: FieldType,
        found: FieldType,
    },
    /// The operand of an expression, such as `...where.eq[0]`: a Card Ref (`{ ref: <param> }`)
    /// names a card and is not a value; only `is_card_type` takes one.
    CardRefNotAValue { reference: String },
    /// The slot of a ref to a param that several card types may fill: the name is a field or an
    /// instance state of a different kind or type on `other` than on `card_type`, the first card
    /// type the param may take.
    InconsistentReference {
        name: String,
        card_type: String,
        other: String,
    },
    /// `win_conditions[leader_dead].when`: an expression win condition reads the card type it is
    /// about (the Subject) through its refs, and exactly one; `found` is how many different card
    /// types its refs name.
    InvalidSubjectCount { found: usize },
    /// An `end_turn` inside an `if` of an action, at its own path, such as
    /// `actions[x].effects[1].then[0]` or `actions[x].effects[0].else[0].then[2]`: `end_turn` is
    /// only ever a top-level effect (ADR-0027).
    EndTurnInIfEffect,
    /// A top-level `end_turn` of an action that is followed by another effect, at its own path,
    /// such as `actions[x].effects[0]`; for `[end_turn, end_turn]` it is the first one (ADR-0027).
    EndTurnNotLast,
    /// An `advance_phase` inside an `if` of an action, at its own path, such as
    /// `actions[x].effects[1].then[0]`: like `end_turn`, it is only ever a top-level effect
    /// (ADR-0042).
    AdvancePhaseInIfEffect,
    /// A top-level `advance_phase` of an action that is followed by another effect, at its own
    /// path, such as `actions[x].effects[0]`: an action has at most one effect that leaves its
    /// phase, and it is the last (ADR-0042).
    AdvancePhaseNotLast,
    /// An `advance_phase` in the effects of a pushed action (one a window lists or opens), at its
    /// own path, such as `actions[x].effects[0]`: a Stack Item's effects run on a Pass, which does
    /// not leave the action phase (ADR-0042).
    AdvancePhaseInPushedAction,
    /// The last action phase, such as `turn.phases[battle]`: it lists the action `action`, which
    /// ends with an `advance_phase`, and no action phase follows (ADR-0042).
    AdvancePhaseFromLastActionPhase { action: String },
    /// A `count` or a `first_turn` in the `when` of a win condition, at the operator's own path,
    /// such as `win_conditions[lost].when.eq[0]`: a win condition judges the GameState a transition
    /// produced, for the Player or the card it is about, and reads neither (ADR-0042). `operator`
    /// is `count` or `first_turn`.
    OperatorNotInWinCondition { operator: String },
    /// The action phase, such as `turn.phases[main]`: none of the actions it lists ends the Turn
    /// (its effects end with a top-level `end_turn`), so the Game could wait there forever.
    NoTurnEndingAction,
    /// An action that no phase lists, such as `actions[idle]`; every action of a ruleset without a
    /// `turn` is one.
    UnlistedAction,
    /// A `choose_one` or `choose_card` outside a Triggered Card Effect's `do`, at its own path, such as
    /// `actions[x].effects[0].choose_one` (also nested in an `if`): a Choice asks the Deciding
    /// Player while a Resolution is paused, which only a Triggered Card Effect can be (ADR-0033).
    ChoiceOutsideTriggeredEffect,
    /// The `options` of a `choose_one`, such as `cards[owl].effects[0].do[0].choose_one.options`:
    /// a Choice has at least 2 options, and `found` is how many it has.
    TooFewOptions { found: u64 },
    /// The `decider` of a `choose_one`, such as `cards[owl].effects[0].do[0].choose_one.decider`:
    /// only `controller` and `opponent` exist.
    UnknownDecider { decider: String },
    /// A `decider: opponent`, at its `decider`, such as `cards[owl].effects[0].do[0].choose_one.decider`,
    /// in a ruleset that does not have exactly 2 Players: "the controller's opponent" is defined for
    /// two Players only, and `players` is how many the ruleset has.
    OpponentDeciderNeedsTwoPlayers { players: u64 },
    /// A zone of a `choose_card`, at its `zone` (`choose_card.zone`, or `choose_card.one_of[i].zone`):
    /// the decider cannot see every candidate there. A zone is seen when it is `public`, or `owner`
    /// and the decider is its owner; choosing from an unseen zone is a peek, which the Card Choice
    /// does not do (ADR-0034).
    DeciderCannotSeeZone { zone: String },
    /// The `name` of a `choose_card`, at `choose_card.name`: a name already in scope, `self`
    /// included, is not bound again (ADR-0034).
    DuplicateChoiceName { name: String },
    /// The `is_card_type` of an `if` condition, such as `actions[x].effects[0].if.is_card_type`:
    /// the card type `card_type` is none that `param` can have, so `then` could never run and the
    /// test would retype the param (ADR-0032).
    ImpossibleCardType { param: String, card_type: String },
    /// `windows`, in a ruleset that does not have exactly 2 Players: Priority is "the opponent of
    /// the top item's controller", defined for two Players only (ADR-0037); `players` is how many
    /// the ruleset has.
    WindowsNeedTwoPlayers { players: u64 },
    /// `windows[attack_response].abilities`: a window with `abilities: true` in a ruleset without
    /// `effects.active_zone`, where no Card Effect is ever active, so the flag could never do
    /// anything (ADR-0038).
    AbilitiesWithoutActiveZone,
    /// `windows[attack_response].abilities`: a window with `abilities: true` in a ruleset whose
    /// `effects.active_zone` is not `public`. The card of an ability is bound in the Active Zone
    /// and the Stack is public (ADR-0037, ADR-0038); `zone` is the Active Zone.
    AbilitiesActiveZoneNotPublic { zone: String },
    /// An action named in the `opened_by` of two windows, or twice in one, at the later one, such
    /// as `windows[second].opened_by[0]`; `first` is the earlier one (ADR-0037).
    DuplicateWindowOpener {
        action: String,
        first: DefinitionPath,
    },
    /// An `end_turn` in the effects of a pushed action (one a window lists or opens), at its own
    /// path, such as `actions[x].effects[1]`: a Stack Item's effects run on a Pass, which does not
    /// leave its action phase (ADR-0037, ADR-0042).
    EndTurnInPushedAction,
    /// A card param of a pushed action that is not in a `public` zone once the costs are paid,
    /// at the param (when a `move` cost moves it: `zone` is the `to` of its last one) or at its
    /// `one_of[i]` alternative (`zone` is the zone the alternative binds from), such as
    /// `actions[fast_play].params[card]`: the Stack is public, so the bound cards must be seen by
    /// every Player in the zone they are bound in (ADR-0037). For an activated ability, `param` is
    /// `self` and the error is at the `to` of its last `move: self` cost, such as
    /// `cards[x].effects[0].activated.cost[0].move.to` (ADR-0038).
    PushedCardNotPublic { param: String, zone: String },
    /// The card of a `tap: { card }` cost, such as `actions[attack].cost[0].tap.card`: a card type
    /// its param may take has no bool `tapped` instance state; `card_type` is the first such one.
    TapCardWithoutTappedState { card_type: String },
    /// A card type id or an action param name that is a word the format keeps, at the path an
    /// `InvalidId` of it would have, such as `card_types[0].id` or `actions[x].params[0].name`:
    /// `player` starts the Player refs, `player.<id>` (ADR-0041).
    ReservedId { id: String },
    /// A `set_state` or `add_state` of an action, a Card Effect or a branch of either, at the key
    /// of the primitive, such as `actions[x].effects[0].set_state`: it must name exactly one of
    /// `card` and `player` (ADR-0041).
    InvalidStateTarget,
    /// A ref to a Player state without the `player:` key, at the ref, such as
    /// `actions[x].params[card].where.eq[0]`: outside a win condition a Player ref says whose
    /// state it reads (ADR-0041).
    MissingPlayerSelector,
    /// The `amount` of an `add_state` in a setup or an automatic step that is not an int literal,
    /// at `setup[i].add_state.amount`: nothing binds a card or names a Player to read there, so a
    /// ref, an operator or a literal of another type is it (ADR-0041, ADR-0043). An amount of an
    /// action, a cost or a Card Effect is an int Expression, and a non-int one is an
    /// `ExpressionTypeMismatch`.
    InvalidAmount,
    /// A `player:` key on a Player Ref inside a win condition, at the ref, such as
    /// `win_conditions[life_out].when.lte[0]`: a Player-state win condition reads the Player it
    /// judges, so its refs have no selector (ADR-0041).
    PlayerSelectorInWinCondition,
    /// A win condition that reads both a card type (its Subject) and Player state, at
    /// `win_conditions[<id>].when`: a condition is judged for one kind of thing (ADR-0041).
    MixedWinConditionSubject,
    /// A `player: opponent` (a read or a write) in a ruleset that does not have exactly 2 Players,
    /// at the ref or the `player` key of the write: "the opponent" is defined for two Players only,
    /// and `players` is how many the ruleset has (ADR-0041).
    OpponentNeedsTwoPlayers { players: u64 },
}

impl fmt::Display for DefinitionErrorKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidId { id } => write!(
                f,
                "`{id}` is not a valid id: it must match [a-z][a-z0-9_]* and not be true, false or null"
            ),
            Self::InvalidDeckSize => write!(f, "the size of a deck input must be at least 1"),
            Self::InvalidMaxCopies { max_copies, size } => write!(
                f,
                "max_copies must be between 1 and the size {size}, found {max_copies}"
            ),
            Self::NoCardTypes => write!(f, "a deck input must allow at least one card type"),
            Self::InvalidPlayerCount { players } => {
                write!(
                    f,
                    "the number of players must be at least 1, found {players}"
                )
            }
            Self::InvalidStartingPlayer {
                starting_player,
                players,
            } => write!(
                f,
                "the starting player {starting_player} is not a position among {players} players"
            ),
            Self::DuplicateId { id, first } => {
                write!(f, "duplicate id `{id}`, first declared at `{first}`")
            }
            Self::DanglingReference { target, id } => {
                let target = match target {
                    ReferenceTarget::Zone => "zone",
                    ReferenceTarget::CardType => "card type",
                    ReferenceTarget::DeckInput => "deck input",
                    ReferenceTarget::Card => "card",
                    ReferenceTarget::Action => "action",
                    ReferenceTarget::InstanceState => "instance state",
                    ReferenceTarget::Param => "param",
                    ReferenceTarget::FieldOrInstanceState => "field or instance state",
                    ReferenceTarget::PlayerState => "player state",
                };
                write!(f, "no {target} has the id `{id}`")
            }
            Self::UnknownPlayerSelector { selector } => write!(
                f,
                "`{selector}` is not a player selector here; the selectors depend on where it stands (`each`, `active`, `opponent`, `controller`)"
            ),
            Self::InvalidCount => write!(f, "a draw or a move must take at least 1 card"),
            Self::UnknownEffectPlayer { player } => write!(
                f,
                "`{player}` is not a player of an effect; the only one is `controller`"
            ),
            Self::ControllerOutsideCardEffect => write!(
                f,
                "`controller` is the owner of `self`, which only a Card Effect has"
            ),
            Self::ZoneNotOrdered { zone } => write!(
                f,
                "the zone `{zone}` is not ordered, so cards cannot be shuffled or taken from it"
            ),
            Self::UnknownPosition { position } => {
                write!(f, "`{position}` is not a position; the only one is `top`")
            }
            Self::InvalidPhaseKind => write!(
                f,
                "a phase must have exactly one of `automatic` and `actions`"
            ),
            Self::NoActionPhase => write!(
                f,
                "a turn must have at least one phase with actions, or the Game would never wait for a Player"
            ),
            Self::StateValueTypeMismatch {
                state,
                declared,
                found,
            } => write!(
                f,
                "the state `{state}` is {declared:?} but the value is {found:?}"
            ),
            Self::InvalidReference { reference } => write!(
                f,
                "`{reference}` is not a ref: it is `<param>` or `<param>.<name>`"
            ),
            Self::AmbiguousReference { name } => write!(
                f,
                "`{name}` is both a field and an instance state of the card type"
            ),
            Self::ReferenceTypeMismatch { expected, found } => {
                write!(f, "the ref is {found:?} but the slot needs {expected:?}")
            }
            Self::UnknownOwner { owner } => {
                write!(f, "`{owner}` is not an owner; the only one is `opponent`")
            }
            Self::NoAlternatives => write!(f, "a param needs at least one alternative"),
            Self::ExpressionTypeMismatch { expected, found } => {
                write!(f, "the expression is {found:?} but {expected:?} is needed")
            }
            Self::CardRefNotAValue { reference } => write!(
                f,
                "`{reference}` is a card, not a value: only `is_card_type` takes a card"
            ),
            Self::InconsistentReference {
                name,
                card_type,
                other,
            } => write!(
                f,
                "`{name}` is not the same on `{card_type}` and `{other}`: a ref to a param that takes several card types needs one kind and one type"
            ),
            Self::InvalidSubjectCount { found } => write!(
                f,
                "an expression win condition must read exactly one card type, found {found}"
            ),
            Self::EndTurnInIfEffect => write!(
                f,
                "`end_turn` cannot be inside an `if`: it is only the last top-level effect of an action"
            ),
            Self::EndTurnNotLast => write!(
                f,
                "`end_turn` must be the last effect of an action: no effect may follow it"
            ),
            Self::AdvancePhaseInIfEffect => write!(
                f,
                "`advance_phase` cannot be inside an `if`: it is only the last top-level effect of an action"
            ),
            Self::AdvancePhaseNotLast => write!(
                f,
                "`advance_phase` must be the last effect of an action: no effect may follow it"
            ),
            Self::AdvancePhaseInPushedAction => write!(
                f,
                "a pushed action cannot advance the phase: its effects run on a Pass"
            ),
            Self::AdvancePhaseFromLastActionPhase { action } => write!(
                f,
                "the last action phase lists `{action}`, which advances the phase, and no action phase follows"
            ),
            Self::OperatorNotInWinCondition { operator } => write!(
                f,
                "`{operator}` is not allowed in a win condition"
            ),
            Self::NoTurnEndingAction => write!(
                f,
                "the action phase lists no action that ends the Turn, so the Game could not leave it"
            ),
            Self::UnlistedAction => write!(f, "no phase lists this action, so it could never be taken"),
            Self::ChoiceOutsideTriggeredEffect => write!(
                f,
                "`choose_one` and `choose_card` are only allowed in the `do` of a triggered Card Effect"
            ),
            Self::TooFewOptions { found } => {
                write!(f, "a `choose_one` needs at least 2 options, found {found}")
            }
            Self::OpponentDeciderNeedsTwoPlayers { players } => write!(
                f,
                "`decider: opponent` needs exactly 2 players, the ruleset has {players}"
            ),
            Self::DeciderCannotSeeZone { zone } => write!(
                f,
                "the decider cannot see the zone `{zone}`: choosing from it would be a peek"
            ),
            Self::DuplicateChoiceName { name } => {
                write!(f, "`{name}` is already a name in scope of this effect")
            }
            Self::UnknownDecider { decider } => write!(
                f,
                "`{decider}` is not a decider; the deciders are `controller` and `opponent`"
            ),
            Self::ImpossibleCardType { param, card_type } => write!(
                f,
                "`{param}` can never be a `{card_type}`: testing for it would retype the param"
            ),
            Self::AbilitiesWithoutActiveZone => write!(
                f,
                "a window allows abilities, but the ruleset declares no `effects.active_zone`"
            ),
            Self::WindowsNeedTwoPlayers { players } => write!(
                f,
                "Response Windows need exactly 2 players, the ruleset has {players}"
            ),
            Self::DuplicateWindowOpener { action, first } => write!(
                f,
                "the action `{action}` already opens a window, at `{first}`"
            ),
            Self::EndTurnInPushedAction => write!(
                f,
                "a pushed action cannot end the Turn: its effects run on a Pass"
            ),
            Self::PushedCardNotPublic { param, zone } => write!(
                f,
                "the card `{param}` of a pushed action or ability is in the zone `{zone}` once the costs are paid, which is not public"
            ),
            Self::AbilitiesActiveZoneNotPublic { zone } => write!(
                f,
                "a window allows abilities, but the Active Zone `{zone}` is not public"
            ),
            Self::TapCardWithoutTappedState { card_type } => write!(
                f,
                "the card type `{card_type}` has no bool `tapped` state to tap"
            ),
            Self::ReservedId { id } => write!(f, "`{id}` is a reserved id"),
            Self::InvalidStateTarget => write!(
                f,
                "a `set_state` or `add_state` names exactly one of `card` and `player`"
            ),
            Self::MissingPlayerSelector => write!(
                f,
                "a ref to a Player state needs the `player:` key that says whose it is"
            ),
            Self::InvalidAmount => write!(
                f,
                "the amount of a setup or an automatic step is an integer"
            ),
            Self::PlayerSelectorInWinCondition => write!(
                f,
                "a win condition reads the Player it judges: its Player refs take no `player:` key"
            ),
            Self::MixedWinConditionSubject => write!(
                f,
                "a win condition reads either one card type or Player state, not both"
            ),
            Self::OpponentNeedsTwoPlayers { players } => write!(
                f,
                "`opponent` needs exactly 2 players, the ruleset has {players}"
            ),
            Self::UnsupportedFieldType { field_type } => {
                write!(f, "the field type {field_type:?} is not supported yet")
            }
            Self::UnknownLoser { loser } => {
                write!(f, "`{loser}` is not a loser; the only one is `owner`")
            }
            Self::InitializerTypeMismatch { declared, found } => {
                write!(
                    f,
                    "the initializer is {found:?} but the state is {declared:?}"
                )
            }
            Self::FromFieldMissing { field } => {
                write!(f, "the card type has no field named `{field}`")
            }
            Self::FromFieldTypeMismatch {
                field,
                declared,
                found,
            } => write!(
                f,
                "the field `{field}` is {found:?} but the state is {declared:?}"
            ),
        }
    }
}

impl std::error::Error for DefinitionErrorKind {}

impl fmt::Display for DefinitionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.path, self.kind)
    }
}

impl std::error::Error for DefinitionError {}
