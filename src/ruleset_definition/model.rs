//! The validated, typed Ruleset Definition. Values of these types are built only by
//! [`RulesetDefinition::validate`], so their invariants hold.

use std::num::NonZeroU32;

use super::draft::RulesetDraft;
use super::error::DefinitionError;
use crate::id::id_type;

id_type!(
    /// The identity of a Ruleset (`meta.id`).
    RulesetId
);
id_type!(
    /// The identity of a zone, unique among the zones.
    ZoneId
);
id_type!(
    /// The identity of a card type, unique among the card types.
    CardTypeId
);
id_type!(
    /// The name of a field, unique within its card type.
    FieldName
);
id_type!(
    /// The name of an instance-state entry, unique within its card type.
    StateName
);
id_type!(
    /// The identity of a Player-State Schema, unique among them.
    PlayerStateId
);
id_type!(
    /// The identity of a deck input, unique among the inputs.
    DeckInputId
);
id_type!(
    /// The identity of an action, unique among the actions.
    ActionId
);
id_type!(
    /// The identity of a phase, unique among the phases of the turn.
    PhaseId
);
id_type!(
    /// The identity of a Response Window, unique among the windows.
    WindowId
);
id_type!(
    /// The name of an action parameter, unique within its action.
    ParamName
);
id_type!(
    /// The identity of a win condition, unique among the win conditions. It is the reason a Player
    /// lost.
    WinConditionId
);

/// A validated Ruleset Definition (the typed model ADR-0004 first called `Ruleset`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RulesetDefinition {
    pub(super) meta: Meta,
    pub(super) parameters: Parameters,
    pub(super) zones: Vec<Zone>,
    pub(super) card_types: Vec<CardType>,
    pub(super) player_state: Vec<PlayerStateSchema>,
    pub(super) deck_inputs: Vec<DeckInput>,
    pub(super) setup: Vec<SetupStep>,
    pub(super) actions: Vec<ActionDefinition>,
    pub(super) windows: Vec<Window>,
    pub(super) turn: Option<TurnStructure>,
    pub(super) win_conditions: Vec<WinCondition>,
    pub(super) effects_active_zone: Option<ZoneId>,
}

impl RulesetDefinition {
    /// Validates a draft and converts it into the typed model, reporting the first error.
    pub fn validate(draft: RulesetDraft) -> Result<RulesetDefinition, DefinitionError> {
        super::validate::validate(draft)
    }

    pub fn meta(&self) -> &Meta {
        &self.meta
    }

    pub fn parameters(&self) -> &Parameters {
        &self.parameters
    }

    /// The zones in declared order.
    pub fn zones(&self) -> &[Zone] {
        &self.zones
    }

    pub fn zone(&self, id: &ZoneId) -> Option<&Zone> {
        self.zones.iter().find(|zone| zone.id() == id)
    }

    /// The card types in declared order.
    pub fn card_types(&self) -> &[CardType] {
        &self.card_types
    }

    pub fn card_type(&self, id: &CardTypeId) -> Option<&CardType> {
        self.card_types
            .iter()
            .find(|card_type| card_type.id() == id)
    }

    /// The Player-State Schemas in declared order; empty when the ruleset declares none (ADR-0041).
    pub fn player_state(&self) -> &[PlayerStateSchema] {
        &self.player_state
    }

    /// The deck inputs in declared order.
    pub fn deck_inputs(&self) -> &[DeckInput] {
        &self.deck_inputs
    }

    /// The Setup Steps in declared order; empty when the ruleset declares none.
    pub fn setup(&self) -> &[SetupStep] {
        &self.setup
    }

    /// The actions in declared order; empty when the ruleset declares none.
    pub fn actions(&self) -> &[ActionDefinition] {
        &self.actions
    }

    pub fn action(&self, id: &ActionId) -> Option<&ActionDefinition> {
        self.actions.iter().find(|action| action.id() == id)
    }

    /// The Response Windows in declared order; empty when the ruleset declares none.
    pub fn windows(&self) -> &[Window] {
        &self.windows
    }

    /// The window that `action` opens when it is taken outside a window: the one that lists it in
    /// `opened_by`. Validation guarantees an action opens at most one.
    pub fn window_opened_by(&self, action: &ActionId) -> Option<&Window> {
        self.windows
            .iter()
            .find(|window| window.opened_by.contains(action))
    }

    /// The structure of a Turn; `None` when the ruleset declares none, and the Game then has no
    /// Turns.
    pub fn turn(&self) -> Option<&TurnStructure> {
        self.turn.as_ref()
    }

    /// The win conditions in declared order; empty when the ruleset declares none, and the Game
    /// then never ends.
    pub fn win_conditions(&self) -> &[WinCondition] {
        &self.win_conditions
    }

    /// The Active Zone, where Card Effects act (`effects.active_zone`): a Field Modifier applies
    /// only to a card in it (ADR-0024) and a `turn_start` trigger is eligible only there
    /// (ADR-0023). `None` when the ruleset declares none, and no Card Effect is then ever active:
    /// no modifier applies and no trigger is eligible, `enter` included.
    pub fn effects_active_zone(&self) -> Option<&ZoneId> {
        self.effects_active_zone.as_ref()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Meta {
    pub(super) id: RulesetId,
    pub(super) version: RulesetVersion,
}

impl Meta {
    pub fn id(&self) -> &RulesetId {
        &self.id
    }

    pub fn version(&self) -> RulesetVersion {
        self.version
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct RulesetVersion(pub(super) u32);

impl RulesetVersion {
    pub fn get(self) -> u32 {
        self.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Parameters {
    pub(super) player_count: PlayerCount,
    pub(super) starting_player: PlayerPosition,
}

impl Parameters {
    pub fn player_count(&self) -> PlayerCount {
        self.player_count
    }

    pub fn starting_player(&self) -> PlayerPosition {
        self.starting_player
    }
}

/// The number of Players in a Game; at least 1.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct PlayerCount(pub(super) NonZeroU32);

impl PlayerCount {
    pub fn get(self) -> u32 {
        self.0.get()
    }
}

/// A 0-based position in the Player Order; always below the [`PlayerCount`] it was validated with.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct PlayerPosition(pub(super) u32);

impl PlayerPosition {
    pub fn get(self) -> u32 {
        self.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Zone {
    pub(super) id: ZoneId,
    pub(super) visibility: Visibility,
    pub(super) ordered: bool,
}

impl Zone {
    pub fn id(&self) -> &ZoneId {
        &self.id
    }

    pub fn visibility(&self) -> Visibility {
        self.visibility
    }

    /// Whether the zone keeps its cards in order. A shuffle, a draw and a move take cards only from
    /// an ordered zone (ADR-0013).
    pub fn ordered(&self) -> bool {
        self.ordered
    }
}

/// Who may see the cards of a zone.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Visibility {
    Hidden,
    Owner,
    Public,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CardType {
    pub(super) id: CardTypeId,
    pub(super) fields: Vec<FieldSchema>,
    pub(super) instance_state: Vec<InstanceStateSchema>,
    pub(super) allows_effects: bool,
}

impl CardType {
    pub fn id(&self) -> &CardTypeId {
        &self.id
    }

    /// The fields in declared order.
    pub fn fields(&self) -> &[FieldSchema] {
        &self.fields
    }

    /// The instance-state entries in declared order.
    pub fn instance_state(&self) -> &[InstanceStateSchema] {
        &self.instance_state
    }

    pub fn allows_effects(&self) -> bool {
        self.allows_effects
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldSchema {
    pub(super) name: FieldName,
    pub(super) field_type: FieldType,
}

impl FieldSchema {
    pub fn name(&self) -> &FieldName {
        &self.name
    }

    pub fn field_type(&self) -> FieldType {
        self.field_type
    }
}

/// The supported field types. `enum` and `enum set` belong to ADR-0004's closed set but are not
/// supported yet, so they are not represented here.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FieldType {
    Int,
    Bool,
    Text,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstanceStateSchema {
    pub(super) name: StateName,
    pub(super) state_type: FieldType,
    pub(super) init: Initializer,
}

impl InstanceStateSchema {
    pub fn name(&self) -> &StateName {
        &self.name
    }

    pub fn state_type(&self) -> FieldType {
        self.state_type
    }

    pub fn init(&self) -> &Initializer {
        &self.init
    }
}

/// How an instance-state entry gets its value when a card instance is created.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Initializer {
    Literal(FieldValue),
    /// The value of the named field of the same card type.
    FromField(FieldName),
}

/// A value of one of the supported field types.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FieldValue {
    Int(i64),
    Bool(bool),
    Text(String),
}

impl FieldValue {
    pub fn field_type(&self) -> FieldType {
        match self {
            Self::Int(_) => FieldType::Int,
            Self::Bool(_) => FieldType::Bool,
            Self::Text(_) => FieldType::Text,
        }
    }
}

/// A Player-State Schema (ADR-0041): one value every Player holds, its type and the value every
/// Player starts the Game with. The ruleset names it; the engine names no Player state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlayerStateSchema {
    pub(super) id: PlayerStateId,
    pub(super) state_type: FieldType,
    pub(super) init: FieldValue,
}

impl PlayerStateSchema {
    pub fn id(&self) -> &PlayerStateId {
        &self.id
    }

    pub fn state_type(&self) -> FieldType {
        self.state_type
    }

    /// The value every Player starts with; it has the type of the schema.
    pub fn init(&self) -> &FieldValue {
        &self.init
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeckInput {
    pub(super) id: DeckInputId,
    pub(super) card_types: Vec<CardTypeId>,
    pub(super) size: u32,
    pub(super) max_copies: Option<u32>,
    pub(super) zone: ZoneId,
}

impl DeckInput {
    pub fn id(&self) -> &DeckInputId {
        &self.id
    }

    /// The card types a deck for this input may hold, in declared order.
    pub fn card_types(&self) -> &[CardTypeId] {
        &self.card_types
    }

    pub fn size(&self) -> u32 {
        self.size
    }

    pub fn max_copies(&self) -> Option<u32> {
        self.max_copies
    }

    /// The zone the cards of this input are placed in.
    pub fn zone(&self) -> &ZoneId {
        &self.zone
    }
}

/// Which Players a step applies to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlayerSelector {
    /// Every Player, in seat order. A step runs fully for one Player before the next Player.
    Each,
    /// The Active Player: during a Turn, the Player whose Turn it is; during setup, the starting
    /// Player.
    Active,
}

/// One step of Game setup, run after the cards of the Deck Lists are placed. Its zones and Player
/// states exist in the Ruleset Definition it was validated with.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SetupStep {
    /// Shuffles `zone` of each selected Player with the GameState's Random State.
    Shuffle {
        player: PlayerSelector,
        zone: ZoneId,
    },
    /// Moves `count` cards, one at a time from the top of `from` onto the top of `to`.
    Draw {
        player: PlayerSelector,
        from: ZoneId,
        to: ZoneId,
        count: NonZeroU32,
    },
    /// Sets the Player state `state` of each selected Player to `value`, if it changes (ADR-0041).
    SetPlayerState {
        player: PlayerSelector,
        state: PlayerStateId,
        value: FieldValue,
    },
    /// Adds `amount` to the int Player state `state` of each selected Player, with checked
    /// arithmetic (ADR-0041).
    AddPlayerState {
        player: PlayerSelector,
        state: PlayerStateId,
        amount: i64,
    },
}

/// A declared action: the cards it takes as parameters, the costs paid and the effects that run
/// when a Player takes it. Its id is unique among the actions. Every ref in it names one of its
/// params and a field or instance state of that param's card type, of the type its slot needs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActionDefinition {
    pub(super) id: ActionId,
    pub(super) params: Vec<Param>,
    pub(super) costs: Vec<Cost>,
    pub(super) effects: Vec<Effect>,
}

impl ActionDefinition {
    pub fn id(&self) -> &ActionId {
        &self.id
    }

    /// The params in declared order; an Action supplies one argument for each, in this order.
    pub fn params(&self) -> &[Param] {
        &self.params
    }

    /// The costs, in the order they are paid.
    pub fn costs(&self) -> &[Cost] {
        &self.costs
    }

    /// The effects, in the order they run.
    pub fn effects(&self) -> &[Effect] {
        &self.effects
    }
}

/// A Response Window (ADR-0037): the Actions that open it, and the Actions the Player with
/// Priority may take while it is open. Every id names a declared action. An action taken in an open
/// window, and an opening action taken outside one, is pushed onto the Stack instead of resolving.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Window {
    pub(super) id: WindowId,
    pub(super) opened_by: Vec<ActionId>,
    pub(super) actions: Vec<ActionId>,
    pub(super) abilities: bool,
}

impl Window {
    pub fn id(&self) -> &WindowId {
        &self.id
    }

    /// The actions that open this window, in declared order. No action opens two windows.
    pub fn opened_by(&self) -> &[ActionId] {
        &self.opened_by
    }

    /// The actions that may be taken while this window is open, in declared order: the order the
    /// Legal Actions of the Player with Priority list them.
    pub fn actions(&self) -> &[ActionId] {
        &self.actions
    }

    /// Whether the activated abilities of cards in the Active Zone may be used while this window is
    /// open (ADR-0038). The declaration of the window, never a property of an ability.
    pub fn abilities(&self) -> bool {
        self.abilities
    }
}

/// A card the Action supplies. It must fit one of the param's alternatives (at least one).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Param {
    pub(super) name: ParamName,
    pub(super) alternatives: Vec<ParamAlternative>,
}

impl Param {
    pub fn name(&self) -> &ParamName {
        &self.name
    }

    /// The alternatives in declared order; never empty. The first one the card fits is the one that
    /// binds.
    pub fn alternatives(&self) -> &[ParamAlternative] {
        &self.alternatives
    }
}

/// One way to fill a param: a card instance of `card_type` in the `zone` of its owner, who is the
/// acting Player or an Opponent, and that makes the Where Condition hold.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParamAlternative {
    pub(super) zone: ZoneId,
    pub(super) card_type: CardTypeId,
    pub(super) owner: ParamOwner,
    pub(super) condition: Option<Expression>,
}

impl ParamAlternative {
    pub fn zone(&self) -> &ZoneId {
        &self.zone
    }

    pub fn card_type(&self) -> &CardTypeId {
        &self.card_type
    }

    pub fn owner(&self) -> ParamOwner {
        self.owner
    }

    /// The `where` condition, read on the card and the params declared before it.
    pub fn condition(&self) -> Option<&Expression> {
        self.condition.as_ref()
    }
}

/// Whose card a param alternative takes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParamOwner {
    /// The acting Player (`owner` absent).
    Actor,
    /// Any Player other than the acting Player (`owner: opponent`).
    Opponent,
}

/// A validated expression. Its operands have the types their operator needs: `Eq` compares two
/// values of one type, `Gte` and `Lte` two ints, `And` any number of bools, and all of them are
/// bool.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Expression {
    Literal(FieldValue),
    /// The value a [`ValueRef`] reads.
    Value(ValueRef),
    Eq(Box<Expression>, Box<Expression>),
    Gte(Box<Expression>, Box<Expression>),
    Lte(Box<Expression>, Box<Expression>),
    /// Holds when every operand holds; with no operand it holds.
    And(Vec<Expression>),
    /// Holds when the card is a card instance of `card_type`.
    IsCardType {
        card: CardRef,
        card_type: CardTypeId,
    },
    /// The value of a Player state of one Player (ADR-0041).
    PlayerValue(PlayerRef),
}

/// The Player a Player Ref reads (ADR-0041).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlayerRefSelector {
    /// No `player:` key: the Player a win condition judges. Only a `WinConditionWhen::PlayerState`
    /// has it.
    Judged,
    /// The Active Player.
    Active,
    /// The owner of `self`, the carrying card. Only a Card Effect has it.
    Controller,
    /// The opponent of the acting Player in an action, of the controller in a Card Effect.
    Opponent,
}

/// The Players a Player-state write in an action or a Card Effect changes (ADR-0041).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlayerTarget {
    /// Every Player, in seat order.
    Each,
    Active,
    Controller,
    Opponent,
}

/// `{ ref: player.<id>, player: <selector> }`: a Player state of one Player (ADR-0041).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlayerRef {
    pub(super) selector: PlayerRefSelector,
    pub(super) state: PlayerStateId,
}

impl PlayerRef {
    pub fn selector(&self) -> PlayerRefSelector {
        self.selector
    }

    pub fn state(&self) -> &PlayerStateId {
        &self.state
    }
}

/// `{ ref: <param> }`: the card a param is bound to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CardRef {
    pub(super) param: ParamName,
}

impl CardRef {
    pub fn param(&self) -> &ParamName {
        &self.param
    }
}

/// `{ ref: <param>.<name> }`: a value of the card a param is bound to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValueRef {
    pub(super) param: ParamName,
    pub(super) value: CardValue,
}

impl ValueRef {
    pub fn param(&self) -> &ParamName {
        &self.param
    }

    pub fn value(&self) -> &CardValue {
        &self.value
    }
}

/// Which value of a card a [`ValueRef`] reads, decided when the definition is validated.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CardValue {
    /// A field of the card's definition.
    Field(FieldName),
    /// An instance state of the card.
    State(StateName),
}

/// A cost of an action, paid before its effects run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Cost {
    /// Taps the first `count` untapped cards (a card whose card type declares `tapped` and whose
    /// value is false), in list order, of the acting Player's `zone`.
    Tap { zone: ZoneId, count: ValueRef },
    /// Taps `card`, which must be untapped; every card type its param may take declares a bool
    /// `tapped` instance state.
    TapCard { card: CardRef },
    /// Moves `card` from the zone it is in onto the Top of `to`, a zone of its owner, as the
    /// `move` effect does.
    Move { card: CardRef, to: ZoneId },
    /// Sets the Player state `state` of each target to `value`, if it changes (ADR-0041).
    SetPlayerState {
        player: PlayerTarget,
        state: PlayerStateId,
        value: FieldValue,
    },
    /// Adds `amount` to the int Player state `state` of each target, with checked arithmetic
    /// (ADR-0041). The amount is evaluated once, when the cost is paid.
    AddPlayerState {
        player: PlayerTarget,
        state: PlayerStateId,
        amount: Amount,
    },
}

/// An effect of an action.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Effect {
    /// Ends the Turn: the phases after the current one run, then the next Player's Turn begins.
    EndTurn,
    /// Moves `card` from the zone it is in onto the Top of `to`, a zone of its owner.
    Move { card: CardRef, to: ZoneId },
    /// Sets the instance state `state` of `card` to `value`, if it changes.
    SetState {
        card: CardRef,
        state: StateName,
        value: FieldValue,
    },
    /// Adds `amount` to the int instance state `state` of `card`, with checked arithmetic. The
    /// amount is evaluated when the effect runs (ADR-0041).
    AddState {
        card: CardRef,
        state: StateName,
        amount: Amount,
    },
    /// Draws for `player`: moves `count` cards one at a time from the Top of `from` onto the Top
    /// of `to` of that player; an empty `from` emits DrawFromEmptyZone and stops.
    Draw {
        player: EffectPlayer,
        from: ZoneId,
        to: ZoneId,
        count: NonZeroU32,
    },
    /// Runs `then` if the condition holds when the effect runs, `otherwise` if not.
    If {
        condition: Expression,
        then: Vec<Effect>,
        otherwise: Vec<Effect>,
    },
    /// An Option Choice (ADR-0033): `decider` picks one of `options`, at least two effect lists
    /// (any of them may be empty), and the chosen one runs. Only a Triggered Card Effect has it.
    ChooseOne {
        decider: Decider,
        options: Vec<Vec<Effect>>,
    },
    /// A Card Choice (ADR-0034): `decider` picks one card among the Candidates of `param`, and the
    /// name `param.name()` reads it in the effects after this one in the same list and in
    /// everything nested in them. `param` is an Action Param validated like any other. Only a
    /// Triggered Card Effect has it.
    ChooseCard { decider: Decider, param: Param },
    /// Sets the Player state `state` of each target to `value`, if it changes (ADR-0041).
    SetPlayerState {
        player: PlayerTarget,
        state: PlayerStateId,
        value: FieldValue,
    },
    /// Adds `amount` to the int Player state `state` of each target, with checked arithmetic; an
    /// overflow refuses the Action (ADR-0041). The amount is evaluated once, when the effect runs,
    /// and every target gets that amount.
    AddPlayerState {
        player: PlayerTarget,
        state: PlayerStateId,
        amount: Amount,
    },
}

/// What an `add_state` of an action, a cost or a Card Effect adds: an int literal, an int read, or
/// the difference of two (ADR-0041).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Amount {
    Literal(i64),
    Operand(AmountOperand),
    /// `sub: [a, b]`: `a - b`, checked.
    Sub(AmountOperand, AmountOperand),
}

/// An int an [`Amount`] reads: a literal, a card's int field or int instance state (a field with its
/// active Field Modifiers), or a Player's int state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AmountOperand {
    Literal(i64),
    Value(ValueRef),
    PlayerValue(PlayerRef),
}

/// The Player a Choice asks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Decider {
    /// The owner of `self`, the carrying card.
    Controller,
    /// The other Player: the controller's opponent.
    Opponent,
}

/// The player an effect acts for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EffectPlayer {
    /// The owner of `self`, the carrying card.
    Controller,
}

/// The phases of every Turn, in order. Exactly one has actions: the Game waits there for the
/// Active Player.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TurnStructure {
    pub(super) phases: Vec<Phase>,
}

impl TurnStructure {
    /// The phases in declared order.
    pub fn phases(&self) -> &[Phase] {
        &self.phases
    }
}

/// One phase of a Turn. Its id is unique among the phases.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Phase {
    pub(super) id: PhaseId,
    pub(super) kind: PhaseKind,
}

impl Phase {
    pub fn id(&self) -> &PhaseId {
        &self.id
    }

    pub fn kind(&self) -> &PhaseKind {
        &self.kind
    }
}

/// What a phase does.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PhaseKind {
    /// Runs its steps in order when the phase begins.
    Automatic(Vec<AutomaticStep>),
    /// Waits for the Active Player, who may take only these actions.
    Actions(Vec<ActionId>),
}

/// Where a `move` takes its cards from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Position {
    /// The Top of the source zone.
    Top,
}

/// A condition of an `if` step, evaluated when the step runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Condition {
    /// Holds on the Game's first Turn only: the Turn Number is 1.
    FirstTurn,
}

/// One step of an automatic phase. Its zones, states and values exist and fit in the Ruleset
/// Definition it was validated with.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AutomaticStep {
    /// Sets `tapped` to false on every card of the listed zones of each selected Player whose card
    /// type declares it.
    Untap {
        player: PlayerSelector,
        zones: Vec<ZoneId>,
    },
    /// Sets `state` to `value` on every card of `zone` of each selected Player whose card type
    /// declares it.
    SetState {
        player: PlayerSelector,
        zone: ZoneId,
        state: StateName,
        value: FieldValue,
    },
    /// Moves up to `count` cards, one at a time from the Top of `from` onto the Top of `to`. If
    /// `from` runs out first, a draw-from-empty fact is recorded.
    Draw {
        player: PlayerSelector,
        from: ZoneId,
        to: ZoneId,
        count: NonZeroU32,
    },
    /// Moves up to `count` cards, one at a time from `position` of `from` onto the Top of `to`;
    /// it moves the cards there are.
    Move {
        player: PlayerSelector,
        from: ZoneId,
        to: ZoneId,
        count: NonZeroU32,
        position: Position,
    },
    /// Runs `then` if the condition holds when the step runs, `otherwise` if not.
    If {
        condition: Condition,
        then: Vec<AutomaticStep>,
        otherwise: Vec<AutomaticStep>,
    },
    /// Sets the Player state `state` of each selected Player to `value`, if it changes (ADR-0041).
    SetPlayerState {
        player: PlayerSelector,
        state: PlayerStateId,
        value: FieldValue,
    },
    /// Adds `amount` to the int Player state `state` of each selected Player, with checked
    /// arithmetic (ADR-0041).
    AddPlayerState {
        player: PlayerSelector,
        state: PlayerStateId,
        amount: i64,
    },
}

/// A declared win condition: when it holds for a Player, that Player loses (ADR-0014). Its id is
/// unique among the win conditions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WinCondition {
    pub(super) id: WinConditionId,
    pub(super) when: WinConditionWhen,
    pub(super) loser: Loser,
}

impl WinCondition {
    pub fn id(&self) -> &WinConditionId {
        &self.id
    }

    pub fn when(&self) -> &WinConditionWhen {
        &self.when
    }

    pub fn loser(&self) -> Loser {
        self.loser
    }
}

/// When a win condition holds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WinConditionWhen {
    /// A draw from this zone found it empty, in the transition being evaluated. The zone exists in
    /// the Ruleset Definition it was validated with.
    DrawFromEmpty(ZoneId),
    /// `condition` holds for a Player when some card instance of the card type `subject` owned by
    /// that Player makes it true in the GameState the transition produced. Its refs keep the
    /// subject's card type id as their param.
    Expression {
        subject: CardTypeId,
        condition: Expression,
    },
    /// `condition` holds for a Player when it is true, reading that Player's Player state (its
    /// Player Refs are all `PlayerRefSelector::Judged`), in the GameState the transition produced
    /// (ADR-0041).
    PlayerState(Expression),
}

/// Which Player loses when a win condition holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Loser {
    /// The Player the condition holds for: for `draw_from_empty`, the Player who drew; for an
    /// expression, the owner of the card instance that makes it true.
    Owner,
}
