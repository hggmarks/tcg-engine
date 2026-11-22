# Ruleset: Response

The ruleset of Phase F. It is **derived from the Effects Ruleset** (`../effects-ruleset/effects-ruleset.md`), which is frozen from this ticket on, as the Small Ruleset has been since Phase C: from now on a rule decision is made here, and the Effects Ruleset files do not change (ADR-0037).

Every rule not stated on this page is the Effects Ruleset's, word for word, and through it the Small Ruleset's: parameters, zones, deck inputs, setup, the turn, combat, win conditions, Card Effects and Choices. A Game of the Response Ruleset in which nobody ever responds is not the Effects Game, Event for Event: `attack` pays its tap as a cost and its combat waits on the Stack for a Pass, so a trace differs by `ItemPushed`, `ItemResolved` and the Pass.

## What differs

| | Effects Ruleset | Response Ruleset |
|---|---|---|
| `meta` | `id: effects`, `version: 1` | `id: response`, `version: 1` |
| Zones | seven | the same and **`holding`**: `public`, `ordered: false`, where a fast creature waits on the Stack |
| **Creature** fields | `name`, `cost`, `power` | and **`fast`** (bool): a fast creature can be played in a Response Window. A field has no default: every creature of the catalog says `fast`, `false` for all the Effects cards |
| `attack` | taps the attacker as its first effect | taps the attacker as its **cost** (`tap: { card: { ref: attacker } }`), and its combat is pushed onto the Stack: **it opens the `attack_response` window** |
| `fast_play` | none | a new action, a response (below), listed by no phase |
| `windows` | none | `attack_response`: `opened_by: [attack]`, `actions: [fast_play]`, `abilities: true` |
| Who acts | "Only the active player acts, and only during Main" (Small Ruleset): the Active Player, in Main, or the Decider of a pending Choice | **overridden while a window is open**: the Player with Priority acts, by `Pass` or by a window action, and the Active Player does not |
| Catalog | the Effects cards | the same, each with `fast: false`, two fast creatures and a creature with an activated ability |
| Sample decks | the Effects decks | the same with copies swapped for the fast creatures and the hare |

The files are `response-ruleset.yaml` (the encoding), `response-catalog.yaml` (the cards of the Effects catalog with `meta: { ruleset: response, ruleset_version: 1 }` and `fast: false` on each creature, then `swift_sparrow`, `flash_fox` and `skittish_hare`) and `decks/deck-a.yaml` and `decks/deck-b.yaml` (the sample decks, see Sample decks).

## Response Windows

A ruleset declares them with a top-level `windows: [{ id, opened_by, actions, abilities? }]` (`formats.md`, ADR-0037, ADR-0038). The Response Ruleset has one:

```yaml
windows:
  - id: attack_response
    opened_by: [attack]
    abilities: true
    actions: [fast_play]
```

- **Push.** `attack`, taken in the Action phase with no window open, binds its params and pays its cost at once, in its own transition: the tap is a `CardStateChanged`. Its effects, the combat, do not run: `ItemPushed { item }` is the next Event, and the **Stack** holds one **Stack Item**: its controller, its source (`attack`) and the two cards it bound, `attacker` and `target`, each with the zone it is in once the cost was paid (the zone it was bound from, unless a cost moved it: `field` or `leader_zone` for `attack`, `holding` for `fast_play`). The window is open while the Stack is not empty.
- **Priority.** The Player with **Priority** is the opponent of the controller of the top item, and is the Deciding Player: the Active Player is not, until the Stack is empty again. Their Legal Actions are `Pass`, then the legal `fast_play`s, in ascending Card Instance Id, then their legal abilities (see Activated abilities). Nobody else has any, and everything else is a Rejection: a Declared Action or a Pass from the other Player (`NotDecidingPlayer`), an Action-phase action such as `play_creature` or `pass_turn` or an `attack` (`ActionNotInWindow`), a Pass with no window open (`PassWithoutWindow`), a Pass while a decision is pending (`PassWhileDecisionPending`: only an Answer is accepted).
- **A response is pushed too.** `fast_play` taken in the window pays its cost and puts a second item on top; Priority flips to the other Player, who may respond to it in turn (the attacker with a `fast_play` of their own, for example) or pass. A Player never responds to their own item while the other holds Priority: they have no Legal Action until Priority returns.
- **One Pass resolves.** `Pass` by the Player with Priority resolves the top item, last in, first out: the trace starts with `ItemResolved { item }`, then the item's effects as the actions run them, then the triggers they wake. The next item down then follows the same rule, with Priority to the opponent of its controller. When the Stack is empty the window is closed and the Active Player is the Deciding Player again, in the Action phase.
- **Fizzle.** When the top item resolves, every card it bound must not have moved since the item was pushed. A card that has moved since, even to come back to the same zone, makes the item Fizzle: `ItemFizzled { item }` takes the place of `ItemResolved`, nothing of the item's effects happens, and its cost is not refunded (the attacker stays tapped). `where` conditions are not read again. This ruleset cannot make a card leave: only a test-only ruleset has a response that does.
- **Abilities.** `abilities: true` lets the Player with Priority also use the activated abilities of their cards in the field (see Activated abilities); the Legal Actions are `Pass`, the legal `fast_play`s, then those abilities. Without it, the default, the window offers none.
- **Triggers.** A trigger woken by a push (by its cost Events) or by a resolving item resolves in that transition, as after any Action, and may pause on a Choice: then the decider is the Deciding Player until they Answer, and after it the Player with Priority. Triggers are not on the Stack.
- **Winning.** The win conditions run at the end of every accepted transition, a push and a Pass included, so a push or a Pass can end the Game: `GameEnded` is the last Event and the Stack is cleared.

## `attack`

The Effects Ruleset's `attack`, with its tap moved from the first effect to the cost. The attacker is an untapped creature of the acting Player that was not played this turn; the target is a tapped creature of an opponent or an opponent's Leader. Paying the tap taps the attacker at once, so a response can no longer be answered by attacking with it again, and the tap stands even when the attack Fizzles. When the item resolves, a creature dies when the attacker's power is at least its own and goes to the top of its owner's trash; a Leader loses 1 life likewise.

## `fast_play`

```yaml
- id: fast_play
  params:
    - name: card
      zone: hand
      card_type: creature
      where: { eq: [ { ref: card.fast }, true ] }
  cost:
    - tap: { zone: mana_zone, count: { ref: card.cost } }
    - move: { card: { ref: card }, to: holding }
  effects:
    - move: { card: { ref: card }, to: field }
    - set_state: { card: { ref: card }, state: recently_played, value: true }
```

A fast creature of the acting Player's hand is played as a response. The cost taps `cost` untapped mana, the first ones in list order, exactly like `play_creature`, and moves the card to `holding`. A card bound by a Stack Item must be seen by every Player, because the Stack is public, and the hand is not: moving it to the public `holding` zone as part of the cost is what makes it public, and it is bound **in the zone it is in once the cost was paid**, `holding`. When the item resolves the creature moves to the field and is marked `recently_played`, as after `play_creature`, so it wakes its `enter: field` triggers then and not before.

## Activated abilities

A Card Effect `activated: { cost, do }` (ADR-0038) is an ability its controller uses as a response. Using it is the engine Action `Activate { player, card, ability }`, `ability` being the position of the effect among all of the card's effects. The rules, for the Response Ruleset:

- **Who, where, when.** Only the Player who owns the card, only while it is in the `field` (the Active Zone), only in a window whose declaration has `abilities: true`: in the `attack_response` window, by the Player with Priority. Never in the Action phase, never for a card in the hand, the trash or any zone but the field.
- **Always pushed.** The cost is paid at once, in its own transition, with the cost Events and then `ItemPushed`, whose item names the card and the ability index and binds the card as `self`, in the zone it is in once the cost was paid. Priority flips as for any push. The triggers the cost's Events wake resolve in that transition.
- **Resolve or Fizzle.** A Pass by the Player with Priority resolves the top item: `ItemResolved`, then the `do`. If the card moved since the ability was pushed (for example a response moved it to the trash), the item Fizzles: `ItemFizzled`, nothing of the `do` happens, and the cost is not refunded.
- **No Choice.** A Choice in an activated `do` is a load error; a `do` only names `self`.
- **Rejected**, with the Game unchanged and no Events, in this order: while a decision is pending, with no window open (the Action phase included), in a window without `abilities`, for a card that does not exist, for an index that is not an activated effect, for a card the Player does not own, for a card outside the field, and when a cost cannot be paid (a tapped card, or not enough untapped mana).

## Catalog cards

The cards of the Effects catalog, each with `fast: false`, two fast creatures and one creature with an activated ability (their names are this ruleset's choice):

| Card | Cost | Power | `fast` |
|---|---|---|---|
| `swift_sparrow` (Swift Sparrow) | 1 | 1 | true |
| `flash_fox` (Flash Fox) | 2 | 2 | true |
| `skittish_hare` (Skittish Hare) | 1 | 1 | false |

`skittish_hare` carries `activated: { cost: [ { tap: { zone: mana_zone, count: { ref: self.cost } } } ], do: [ { move: { card: { ref: self }, to: hand } } ] }`: while it is in the field, its owner may pay 1 mana (its own `cost`) as a response to return it to their hand. It pays mana and does not tap itself because the target of an attack is tapped: a hare that tapped itself as its cost could never dodge the attack that targets it. When the hare is the target, the ability resolves before the attack beneath it and the attack Fizzles (the attacker stays tapped, nothing dies); when it is not, it is only a bounce.

A fast creature can also be played in the Action phase with `play_creature`, which does not check `fast`.

## Sample decks

Both are 1 Leader, 50 creatures (at most 4 of each) and 10 mana, the Effects sample decks with copies swapped for the fast creatures, so that self-play exercises the window: in deck A `goblin_scout` 4 became `goblin_scout` 2 and `swift_sparrow` 2, and `ember_hound` 2 became `flash_fox` 2; in both decks `stone_golem` 4 became `stone_golem` 2 and `skittish_hare` 2 (ADR-0038); in deck B `moss_sprite` 4 became `moss_sprite` 2 and `swift_sparrow` 2, and `ember_hound` 2 became `flash_fox` 2. Everything else is the Effects decks'.

## Glossary: Response Ruleset

| Term | Definition |
|---|---|
| **Response Window** | `windows: [{ id, opened_by, actions, abilities? }]`: while it is open, the Player with Priority may Pass, take one of its `actions` or, with `abilities: true`, activate an ability. Open exactly while the Stack is not empty. |
| **Stack** | The Stack Items waiting to resolve, bottom first, resolved last in, first out. |
| **Stack Item** | A pushed action or ability: its controller, its source (an action id, or a card and an ability index) and its Bound Cards. |
| **Activated ability** | A Card Effect `activated: { cost, do }`, used with `Action::Activate { player, card, ability }`; always pushed. |
| **Bound Card** | A card a pushed action bound: its param, the card, the zone it is in once the cost was paid (the bound-from zone unless a cost moved it) and `left`, set when the card moves, even if it comes back. |
| **Push** | Taking an action whose params are bound and cost paid at once while its effects wait as a Stack Item (`ItemPushed`), its cards bound in the zones they are in once the cost was paid. |
| **Pass** | `Action::Pass { player }`: the Player with Priority resolves the top item. |
| **Priority** | The opponent of the controller of the top item; derived, never stored; none when the Stack is empty. |
| **Fizzle** | An item whose Bound Card has moved since the item was pushed does nothing when it resolves (`ItemFizzled`); no refund. |
| **Deciding Player** | The Decider while a decision is pending, else the Player with Priority while a window is open, else the Active Player. |
