# Ruleset: Effects

The ruleset of Phase C. It is **derived from the Small Ruleset** (`../small-ruleset/small-ruleset.md`), which is frozen as the canonical ruleset of Phase B: from now on a rule decision is made here, and the Small Ruleset files do not change (ADR-0022).

Every rule not stated on this page is the Small Ruleset's, word for word: parameters, zones, deck inputs, setup, the turn, combat and win conditions. A Game of the Effects Ruleset with the same Seed, Deck Lists of Small cards and Actions plays identically to a Game of the Small Ruleset, Event for Event (a test plays the Small Ruleset's Leader-kill script with the Small sample decks under both). Its sample decks play the effect cards (see Sample decks), so its own golden game, `crates/tcg-loader/tests/golden/effects-leader-kill.trace`, is not the Small Ruleset's: it is a Leader-kill with a trigger resolution in it. A second Effects Golden Game, `crates/tcg-loader/tests/golden/effects-pausing.trace`, pauses for each kind of Choice (ADR-0035).

## What differs

| | Small Ruleset | Effects Ruleset |
|---|---|---|
| `meta` | `id: small`, `version: 1` | `id: effects`, `version: 1` |
| **Creature** `effects` | `false` | `true`: a creature definition may carry Card Effects |
| `effects.active_zone` | none (no `effects` block) | `field`: the Active Zone, where Card Effects act (see Triggered Card Effects) |
| **Leader** and **Mana** `effects` | `false` | `false`: Card Effects act only while the card is on the Field (roadmap #48); a Leader never reaches the Field, and a mana card never does either and has no int field to modify |

The files are `effects-ruleset.yaml` (the encoding), `effects-catalog.yaml` (the cards of the Small catalog, with `meta: { ruleset: effects, ruleset_version: 1 }`, the nine creatures of the triggered Card Effect section, the two of the continuous one and the three auras below) and `decks/deck-a.yaml` and `decks/deck-b.yaml` (the sample decks, see Sample decks).

## Card Effects

A card definition's `effects` is a list of **Card Effects**, allowed only on a card type declared `effects: true`. A Card Effect is one of two shapes, told apart by which shape keys appear (`on`, `if`, `do` or `while`, `modify`, `scope`; any key of both shapes is a mixed shape):

```yaml
effects:
  - on: { enter: field }     # triggered: a Trigger Matcher over Events
    if: { ... }              # optional Expression, evaluated when it fires
    do: [ <action effects> ] # the existing Action Effects; at least one
  - while: { ... }           # continuous: an Expression (required; `true` for an always-on effect)
    scope: { zone: field, card_type: creature }   # optional: the cards it reaches (a Modifier Scope)
    modify: { field: power, amount: 1 }   # an additive int on the fields of those cards
```

- **Triggered**: `on` (required), `if` (optional) and `do` (required, at least one effect).
  - `on` is a single-key map. `enter: <zone>` matches the card entering that zone (a declared zone); `turn_start: {}` matches the controller's Turn beginning (semantics in Triggered Card Effects below). Any other key is an unknown matcher.
  - `if` must be a bool Expression.
  - `do` is a list of Action Effects (`move`, `set_state`, `add_state`, `draw`, `if`, `choose_one`, `choose_card`; ADR-0015, ADR-0016, ADR-0023, ADR-0033, ADR-0034), except that **`end_turn` is rejected anywhere in it**, including inside an `if` effect: a trigger that ends the Turn is not covered yet.
- **Continuous**: `while` (a bool Expression) and `modify: { field, amount }` (required), and an optional `scope` (a Modifier Scope, below). `field` must be a field of type int (an instance state is not a field) that the card's card type declares, or the scope's card type when there is a scope; `amount` is an unquoted integer. The modifier adds `amount` to that field of the carrying card, or of the cards its scope reaches, while it is active (see Continuous Card Effects). `while` is required: an always-on effect writes `while: true`, and a `scope` and `modify` with no `while` has no shape.
- **`self`** is the name an Expression or an effect of a Card Effect may use as a ref from the start: the carrying card, of the card's own card type. `{ ref: self }` is a card and `{ ref: self.power }` is a value (a field or an instance state of the card's type). The names that `choose_card` binds join it after their Choice (see The `choose_card` effect). Any other name is a dangling reference to a param.
- An element has the keys of one shape only: `on` or `if` or `do` together with `while`, `modify` or `scope` is a mixed shape, and an element with neither `on` nor `while` has no shape.

### Validation

The loader reads the syntax and rejects an unknown key of the envelope, of `modify` or of `scope`, an `amount` that is not an unquoted integer, and an `include_self` that is not an unquoted `true` or `false`. The engine checks the rest, the first error winning, in this order: the list against the card type's `effects` flag, then each element in order: its shape, then for a triggered one its `on`, `if` and `do` (and `end_turn` in `do`), for a continuous one its `while`, its `scope` (zone, then card type, then owner) and its `modify`.

A well-formed Card Effect of either shape is kept by its card definition and acts: triggered ones run (ADR-0023) and continuous ones are read as Field Modifiers (ADR-0024, ADR-0025). Nothing well-formed is rejected as unsupported any more (ADR-0022).

## Continuous Card Effects

A Continuous Card Effect is a **derived read**: nothing is written to the Game and no Event is emitted for it. Every time a rule reads an int field of a card through a ref (a param `where`, an effect `if`, a cost's `count`, the combat comparison, a win condition `when`), the value is the field's base value plus the amounts of the Field Modifiers that are active at that moment (ADR-0024).

- **Active zone.** The top-level `effects: { active_zone: field }` block names the zone where Card Effects act. A Field Modifier is active only while its carrying card is in that zone, and its `while` holds. A carrier anywhere else, a false `while`, or a ruleset without the block adds nothing, and a card that no active modifier reaches reads its base values.
- **`while` reads base values only.** The condition of a Field Modifier is evaluated on base values, never on modified ones, whatever field it reads. So `while: { gte: [ { ref: self.power }, 3 ] }` with `modify: { field: power, amount: -1 }` on a power-3 card holds, and the card is power 2 (a derived read of a `while` that read derived values would be circular).
- **Additive and commuting.** Several active modifiers on the same field add up, in any order. An int that would overflow, the sum of modifiers included, is the rejection `DerivedValueOverflow`, never a wrap. The sum is still taken in a fixed order, so an overflow is the same in every replay: first the card's own modifiers that have no scope (while the card is in the Active Zone), in `effects` order, then the scoped modifiers of the carriers in the Active Zone, in Player Order and zone order, each carrier's in `effects` order, a carrier's own `include_self` modifier among them (ADR-0025); an overflow at any step counts. No param `where` of this ruleset reads an int field, so an overflow cannot happen in binding here; the general rule is in `rulesets/formats.md` (ADR-0032).
- **No clamping.** Nothing clamps a modified int at 0: a derived power of 0 or below is reachable (two `dread_herald` on a power-1 creature) and the combat stays decided by `gte`, so nothing is undefined. A modified `cost` below 0 would be refused by the `tap` cost as a negative count, like a base value; no card of the catalog modifies `cost`.
- **Fields only.** Only card fields (int) are modified; instance states (`tapped`, `current_life`) are read as stored.
- **Own card, or a matched set.** With no `scope` a modifier changes a field of the card that carries it. With a `scope` it changes that field of the cards the scope reaches (below).

The catalog carries two creatures that show the modifier on the carrying card. A creature taps when it attacks, before the combat compares powers, so a modifier that holds `while` the creature is tapped is the one a Game flips:

| Card | Cost | Power | Continuous Card Effect |
|---|---|---|---|
| `battle_veteran` | 1 | 2 | `while: { eq: [ { ref: self.tapped }, true ] }`, `modify: { field: power, amount: 1 }`: power 3 once tapped |
| `weary_ox` | 2 | 3 | the same `while`, `amount: -1`: power 2 once tapped |

### Modifier Scope

`scope: { zone, card_type, owner?, include_self? }` selects the cards a Continuous Card Effect reaches (ADR-0025). It is a filter, not an Expression, so the match reads no field of any card and a derived read never needs another derived read.

- `zone` (required) is a declared zone and `card_type` (required) a declared card type; it does not need `effects: true`, because targets carry nothing.
- `owner` is absent (the Player who owns the carrying card, the controller) or `opponent` (any Player other than the carrying card's owner); any other text is an unknown owner. `include_self` is absent or false (the carrying card is not reached) or true.
- A card is **reached** when it is in `zone` of its owner, its definition's type is `card_type`, its owner stands in the `owner` relation to the carrier's owner, and it is not the carrier unless `include_self`.
- The modifier is **active** while its carrying card is in the Active Zone (`field`) and its `while` holds, whatever zone `scope.zone` names; a carrier anywhere else, or a ruleset without the Active Zone, adds nothing. `while` reads the carrier's base values (`self` is the carrier).
- `modify.field` is checked against the scope's `card_type`, not the carrier's, so an aura on a type with no `power` can still grant `power`.
- Two auras add up. Leaving the field removes the bonus, and entering it grants the bonus, in the same transition as the `CardMoved` Event. No Event is emitted for the bonus.

The catalog carries three auras. Every one has `while: true`, `scope: { zone: field, card_type: creature, ... }` and a `power` modifier:

| Card | Cost | Power | Aura |
|---|---|---|---|
| `banner_bearer` | 2 | 1 | `amount: 1`: the controller's other creatures in the field get +1 power, not itself, not the opponent's |
| `dread_herald` | 3 | 2 | `owner: opponent`, `amount: -1`: every opponent creature in the field gets -1 power |
| `rally_captain` | 3 | 2 | `include_self: true`, `amount: 1`: the controller's creatures in the field, itself included, get +1 power |

## The `draw` effect

`draw: { player: controller, from: <zone>, to: <zone>, count: <n> }` is an Action Effect that exists in a Card Effect's `do` (ADR-0023). For `player`, the **Controller** (the owner of `self`, the carrying card; the only selector of an effect), it moves `count` cards one at a time from the Top of `from` onto the Top of `to`, exactly as the Start phase's `draw` step does. If `from` is empty, the effect records a `DrawFromEmptyZone` Event and stops: that Event is what `draw_from_empty` win conditions read, so a trigger can make a Player lose by `deck_out`. `from` must be an ordered zone, `count` an unquoted positive integer, and both zones must be declared. `controller` needs a `self`, so a ruleset action's `effects` rejects it (`ControllerOutsideCardEffect`).

## The `choose_one` effect (Option Choice)

`choose_one: { decider: <controller|opponent>, options: [ [<effects>], [<effects>], ... ] }` is an Action Effect that exists only in a Triggered Card Effect's `do` (ADR-0033), also nested in an `if` there and inside an option of another `choose_one`. The **Decider** picks one of the options and the chosen list runs. `controller` is the owner of `self`, `opponent` the other Player, which is defined for exactly 2 Players (`OpponentDeciderNeedsTwoPlayers` at `choose_one.decider` in a ruleset with another player count). There are at least 2 options, any of which may be empty (`[]`): **"may"** is an Option Choice with an empty option, a pattern and not a keyword (`curious_owl` is `[[draw 1], []]`). There are no labels: an option is its index, from 0.

Reaching a `choose_one` **pauses the Resolution**: the transition ends with the Event `DecisionPending { decider, kind, paused }`, the Decider becomes the **Deciding Player**, and the only Legal Actions are the Decider's Answers, `Answer::Option(0)` to `Answer::Option(N-1)` in index order. A Declared Action is rejected while a decision is pending (`DeclaredWhileDecisionPending`), as is an Answer with none pending (`NoDecisionPending`), an index past the last option (`OptionOutOfRange`) and any Action of a Player who is not the Deciding Player (`NotDecidingPlayer`). The Answer's trace starts with `DecisionAnswered`, runs the chosen option, then the rest of the `do`, then the rest of the Resolution, and may pause again. An `if` on the path to the paused `choose_one` is not read again on resume: the pause records which branch it was in. A pause changes no order: the Events of a paused `do` are scanned after the `do` completes, together with those after the pause, as if the same choice had been inlined. The Trigger Cap counts across the pauses of one Resolution.

Load errors, each at its Definition Path: `ChoiceOutsideTriggeredEffect` at `<effect>.choose_one` in a ruleset action's effects (an `if` of one included); `choose_one` in a setup or turn step, or beside the keys of a Continuous Card Effect, is the loader's unknown key; `TooFewOptions` (fewer than 2) at `choose_one.options`; `UnknownDecider` at `choose_one.decider`. The effects of option `k` are checked at `choose_one.options[k][i]`, and an `end_turn` among them is `EndTurnInCardEffect` there.

## The `choose_card` effect (Card Choice)

`choose_card: { name, decider: <controller|opponent>, zone, card_type, owner?, where? }`, or `{ name, decider, one_of: [ { zone, card_type, owner?, where? }, ... ] }`, is an Action Effect that exists only in a Triggered Card Effect's `do` (ADR-0034), with the placement of `choose_one`. The **Decider** picks one card from the **Candidates**, and the name is a Card Ref (and its fields and instance states Value Refs) for the effects after it in the same list and in everything nested in them. Its spec is an action param (ADR-0015, ADR-0016, ADR-0032): the same `one_of`, `where` (which reads `self`, the earlier names and its own candidate) and narrowing by `is_card_type`. **`owner` is relative to the controller**, the owner of `self`, whoever decides: absent is the controller's zone, `opponent` the controller's opponent's. A name is unique in scope, `self` included, and is out of scope once the enclosing `if` or option ends; `then` and `else` may reuse one.

**The Decider sees every candidate.** For each alternative, its zone is `public`, or `owner` with that zone's owner the Decider: `owner` absent with `decider: controller`, or `owner: opponent` with `decider: opponent`. A `hidden` zone is never seen. Choosing from a zone the Decider cannot see is a peek, which is out of scope (`DeciderCannotSeeZone` at `choose_card.zone`, or `choose_card.one_of[i].zone`).

The Candidates are the cards that could fill the param, in ascending Card Instance Id, evaluated when the Choice is reached with `self` and the earlier names in scope, and recomputed when the Answer arrives. **With none, nobody is asked, no Event is emitted, and the rest of that `do` is skipped**, enclosing `if`s and options included (earlier effects stand, and the Resolution goes on). **With one, it is still asked.** Otherwise the Resolution pauses as for `choose_one`, `kind: Card`, and the Decider's Legal Actions are `Answer::Card(c)`, one per Candidate. An Answer of the other kind is `WrongAnswerKind { kind }`, and a card that is not a Candidate (an unknown id included) is `CardNotCandidate { card }`; neither changes the GameState or emits an Event. The Answer binds the name, and the `do` goes on.

Load errors, each at its Definition Path: `ChoiceOutsideTriggeredEffect` at `<effect>.choose_card` in a ruleset action's effects; `choose_card` in a step is the loader's unknown key; `InvalidId` then `DuplicateChoiceName` at `choose_card.name`; `UnknownDecider` at `choose_card.decider`; `OpponentDeciderNeedsTwoPlayers` at `choose_card.decider` in a ruleset with another player count (`decider: opponent`; found after the effects of the `do` are otherwise valid, as for `choose_one`); the param's own errors as for an action; `DeciderCannotSeeZone`; and a Ref to a name that is unbound or out of scope is the usual `DanglingReference`. The other Viewer's Taken Action names the card an Answer picks exactly when it sees it before or after the transition (a card moved from a hidden hand to a public trash shows; a card that stays in it does not); its Observed `DecisionAnswered` names it by what the Viewer sees at that Event (ADR-0034).

## Triggered Card Effects

A Card Effect acts only on cards in the **Active Zone**, `effects.active_zone`, which is `field` here; a ruleset without the `effects` block has no Active Zone and none of its triggers is ever eligible, `enter` included. With the block, the one exception to the Active Zone is `enter`.

- **`enter: <zone>`** is woken by a `CardMoved` Event that puts the card into that zone, whichever zone it is and whether or not it is the Active Zone: `enter: field` fires when the card is played, `enter: trash` when it dies in combat. A `CardCreated` Event never wakes it: placing the cards of the decks at Game creation and creating cards fire nothing, and no trigger resolves during Game creation.
- **`turn_start: {}`** is woken by the `TurnBegan` Event of the card's controller, and only if the card is in the Active Zone when that Event is scanned. It fires on its controller's Turn only, in the transition that begins it (the transition of the Action that passed the Turn, or Game creation's first Turn, which fires nothing). Its Events land after the Events of the transition's automation, so they follow `PhaseBegan { main }`, and the Start phase's untap and `recently_played` reset cannot undo what it does.
- **Resolution.** After the Action's costs and effects, the Events of the transition are scanned in order from the first, costs included. An Event that wakes a trigger enqueues it; the queue is first in first out. Eligibility is decided when the Event is scanned: an enqueued trigger fires even if its card moved since. When a trigger fires, its `if` is read (on the state as it is then, `self` being the card) and, if false, nothing happens; otherwise its `do` runs to completion, or until a `choose_one` or a `choose_card` pauses the Resolution (see above), or a `choose_card` with no Candidate ends it early. The Events it emits are appended to the same Event Trace and scanned in turn, so a cascade queues behind the triggers already waiting. When no Event is left to scan and the queue is empty, resolution ends.
- **Order.** One Event waking several triggers enqueues them: the cards of the Active Player first, then of the other Players in seat order; within a Player in `state.cards()` order; within a card in the order of its `effects`. (With `enter` and `turn_start`, one Event only wakes the cards of one Player, so the order across Players cannot be observed yet.)
- **The Trigger Cap is 100.** A Resolution, which spans the pauses of a `choose_one` or `choose_card`, resolves at most 100 triggers. Every trigger that leaves the queue counts, one whose `if` is false included. The 101st rejects the whole transition (the Action, or the Answer that would take it) with `TriggerCapExceeded { cap: 100 }`: no Event is returned and the GameState stays as it was. A mutually-triggering pair of effects (on one card, as `restless_shade` has) reaches the cap.
- **Win conditions** are checked once at the end of every accepted transition, after its Resolution ends or pauses (a pausing transition's queued triggers have not run yet), on the final state and the whole Event Trace of that transition (`GameEnded` is the last Event of the trace that emits it, and follows `DecisionPending`). A trigger can cause a `deck_out` loss through `draw`; `leader_dead` is not reachable from a trigger that only reads and changes `self`.
- A trigger asks for no decision unless its `do` holds a `choose_one` or a `choose_card`, whose Answer is an Action like any other: the Game Record is unchanged in shape and holds the Answers (ADR-0019, ADR-0033). Win conditions run at the end of every accepted transition, a pausing one included: `GameEnded` follows `DecisionPending`.

### Catalog cards

One creature per family, in `effects-catalog.yaml`, and `pyre_imp`, the bounded cascade. Every one is in a sample deck except `restless_shade`.

| Card | Effect | Shows |
|---|---|---|
| `lamp_keeper` | `enter: field` → draw 1 | the draw on play, the trigger's Events after the play's |
| `dawn_scout` | `turn_start` → draw 1 | firing on its controller's Turn only, only in the Field |
| `grave_jackal` | `enter: trash` → draw 1 | the `enter` exception to the Active Zone: it fires on dying in combat |
| `eager_recruit` | `enter: field`, `if` `self.recently_played` → draw 1 | an `if` read at fire time: true when played, false when `recently_played` is false, for example on an arrival after the Turn it was played (a false `if` at fire time is exercised by the engine tests; no shipped card or deck reaches it) |
| `restless_shade` | `enter: field` → move self to trash; `enter: trash` → move self to field | a cascade that exceeds the Trigger Cap: playing it is rejected. It is not in a sample deck |
| `curious_owl` | `enter: field` → `choose_one` (controller): draw 1, or nothing | an Option Choice, the "may" pattern: playing it pauses, `Answer::Option(0)` draws and `Answer::Option(1)` does not. It is in both sample decks (2 copies each) |
| `snare_vine` | `enter: field` → `choose_card` `target` (controller): a creature in an opponent's field; `set_state` `target` tapped | a Card Choice: playing it pauses (cost 2, power 1) when an opponent has a creature in the field, `Answer::Card(c)` taps exactly `c`; with none nothing is asked and the rest of the `do` is skipped. It is in both sample decks (2 copies each) |
| `mind_leech` | `enter: field` → `choose_card` `victim` (opponent): a creature in the opponent's own hand; `move` `victim` to trash | a Card Choice decided by the controller's opponent (cost 2, power 2): `owner: opponent` is the controller's opponent, who sees its own hand, so the opponent is the Deciding Player during the other Player's Turn and `Answer::Card(c)` moves `c` to its own trash. It is in both sample decks (2 copies each) |
| `pyre_imp` | `enter: field` → move self to trash; `enter: trash` → draw 1 | a bounded cascade: the first trigger's own Event (the move) wakes the second, which draws and stops. Playing it moves the card to the field, then the trash, then draws one card, in one transition |

Self-play never reaches the cap with the sample decks, because `restless_shade` is in none. A one-off count over Seeds 0..1000, made when the sample decks were first written (before the Choice cards joined them) and not pinned by a test per card, found every effect card of the sample decks played and the cards that act only after another Event (the `grave_jackal` that dies, the `dawn_scout` that stays for a Turn) acting: no card turned out dead or unreachable, so the catalog needed no fix. The tests pin per-family minimums (see ADR-0026) and, since the Choice cards joined the decks, that each kind of Choice is answered or skipped at least once over those 1,000 Seeds (see ADR-0035).

## Sample decks

Both are 1 Leader, 50 creatures (at most 4 of each) and 10 mana. They hold all three families of Card Effects and the three cards that pause a Resolution for a Choice, so a self-played Game plays them. The three joined by swapping vanilla creatures in place (ADR-0035): `iron_guard` 2 became `snare_vine` 2, `wind_hawk` 2 became `mind_leech` 2, and `ember_hound` 4 became `ember_hound` 2 followed by `curious_owl` 2, in both decks, so the decks stay 50 creatures and every other Card Instance Id stays where it was.

| Deck | Main deck (count each) |
|---|---|
| A (`captain_ardel`) | `goblin_scout` 4, `moss_sprite` 4, `ember_hound` 2, `curious_owl` 2, `river_otter` 4, `stone_golem` 4, `shadow_cat` 4, `snare_vine` 2, `mind_leech` 2, `lamp_keeper` 4, `dawn_scout` 4, `grave_jackal` 2, `eager_recruit` 2, `battle_veteran` 2, `weary_ox` 2, `pyre_imp` 2, `banner_bearer` 2, `rally_captain` 2 |
| B (`warden_ilse`) | `moss_sprite` 4, `ember_hound` 2, `curious_owl` 2, `river_otter` 4, `stone_golem` 4, `shadow_cat` 4, `snare_vine` 2, `mind_leech` 2, `thorn_boar` 4, `lamp_keeper` 4, `dawn_scout` 4, `grave_jackal` 2, `eager_recruit` 2, `battle_veteran` 2, `weary_ox` 2, `pyre_imp` 2, `dread_herald` 4 |

The leaders and the mana decks are the Small Ruleset's. The sample decks are the Effects Ruleset's own: the Small Ruleset's are frozen and still load here, since the catalog holds the Small cards.

## Glossary: Effects Ruleset

| Term | Definition |
|---|---|
| **Card Effect** | An ability a card definition carries, in the `effects` list of a card of a type with `effects: true`. Triggered or continuous. |
| **Triggered Card Effect** | `on`, optional `if` and `do`: when an Event matches the Trigger Matcher and the condition holds, the Action Effects run. |
| **Continuous Card Effect** | `while`, `modify` and an optional `scope`: while the Field Modifier is active, every read of the field of a card it reaches adds its amount. |
| **Trigger Matcher** | What a triggered Card Effect matches among Events: `enter` (a `CardMoved` Event puts the card into a zone) or `turn_start` (the controller's Turn began). |
| **Active Zone** | `effects.active_zone` (`field`): the zone where Card Effects act: where a carrier must be. A Modifier Scope names the zone of its targets, which may be any zone. `enter` is the exception. Without the block no Card Effect is active: no modifier applies and no trigger is eligible. |
| **Trigger Cap** | The most trigger resolutions in one Resolution, across its pauses: 100. A false `if` counts. One more rejects the transition (`TriggerCapExceeded`). |
| **Option Choice** | `choose_one: { decider, options }`: the Decider picks one of at least 2 effect lists, any of which may be empty, and the Resolution pauses until they answer (ADR-0033). |
| **Candidate** | A card a Card Choice can be answered with: one that could fill its param for the controller, listed in ascending Card Instance Id, evaluated when the Choice is reached and recomputed when the Answer arrives. |
| **Card Choice** | `choose_card: { name, decider, zone, card_type, owner?, where? }` (or `one_of`): the Decider picks one card among the Candidates, an action param whose `owner` is relative to the controller; the name is a Card Ref after it. With no Candidate the rest of the `do` is skipped; with one it is still asked (ADR-0034). |
| **Deciding Player** | The Player who acts next: the Decider while a decision is pending, otherwise the Active Player. |
| **Answer** | The Deciding Player's Action that settles the pending decision: `Answer::Option(index)`, or `Answer::Card(id)` for a Card Choice. |
| **Controller** | The owner of `self`, the carrying card: the Player a `draw` in a Card Effect draws for. |
| **Draw effect** | `draw: { player, from, to, count }`: draws like the `draw` step; an empty `from` records `DrawFromEmptyZone`. |
| **Field Modifier** | `modify: { field, amount }`: an additive int on a field of the definition of each card it reaches (the carrying card, or the cards of its Modifier Scope). It is **active** while its carrying card is in the Active Zone and its `while` holds. |
| **Modifier Scope** | `scope: { zone, card_type, owner?, include_self? }`: the cards a Field Modifier reaches: in `zone` of their owner, of `card_type`, owned relative to the carrier's owner (`opponent` is any other Player), and not the carrier unless `include_self`. |
| **Derived Read** | A read of a field through a ref: the base value plus the amounts of the active Field Modifiers that reach the card, computed at read time and stored nowhere. |
| **Self** | `self`, the carrying card, the name in a Card Effect's scope from the start; a `choose_card` name joins it after its Choice. |
