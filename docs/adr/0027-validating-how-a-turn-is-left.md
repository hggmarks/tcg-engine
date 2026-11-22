# Validating how a Turn is left

Amended by ADR-0037: an action listed by a window's `actions` is not an `UnlistedAction`; being in `opened_by` lists nothing.

Phase D closes the known gap that ADR-0012 recorded and ADR-0015 and ADR-0016 left open: validation did not check that the Action Phase can be left, nor that an action ends at most one Turn. `end_turn` runs the remaining phases and begins the next Player's Turn inside the same transition, so an effect after it ran in the next Turn, `[end_turn, end_turn]` ended two Turns and an `end_turn` in an `if` ran like a top-level one. The rule is checked statically by `RulesetDefinition::validate`; the runtime is unchanged (ADR-0012's runtime checks stay, and no new one is added).

**The rule.** `end_turn` is allowed in an action's effects only as the **last top-level effect**, so at most once, and never inside an `if` (`then` or `else`, at any depth). The phase with `actions` lists **at least one action that ends the Turn**, an action whose effects end with a top-level `end_turn`. **Every action is listed by a phase**: *(Amended by ADR-0037: or by the `actions` of a Response Window)* with exactly one phase with `actions` (ADR-0012, unchanged), that is the action phase. Card Effects keep their own rule, `EndTurnInCardEffect` (ADR-0022): the two rules share no code, because one forbids `end_turn` everywhere and the other forbids it only where the Turn cannot end cleanly.

**Four new `DefinitionErrorKind`s**, each with the path it carries:
- `EndTurnInIfEffect`: the `end_turn`'s own path, such as `actions[x].effects[1].then[0]` or `actions[x].effects[0].else[0].then[2]`. Being the last top-level effect does not excuse nesting: an `if` whose `then` holds `end_turn` is rejected even when the `if` is last.
- `EndTurnNotLast`: the `end_turn`'s own path, such as `actions[x].effects[0]`. For `[end_turn, end_turn]` it is the first one.
- `NoTurnEndingAction`: the action phase, such as `turn.phases[main]`; an empty `actions: []` is included.
- `UnlistedAction`: the action, such as `actions[idle]`. *(Amended by ADR-0037: an action a window lists is listed)*

**Validation order** (first error wins, ADR-0008). In `actions`, in declaration order: id, duplicate, params, cost, effects (all of them), and then the placement of that action's `end_turn`: its effects are walked in order, depth first, `then` before `else`, and the first `end_turn` that is inside an `if` (`EndTurnInIfEffect`) or top-level and not last (`EndTurnNotLast`) is the error; after the effects were checked, as for `EndTurnInCardEffect`. Then `turn.phases`, in order, each unchanged (id, duplicate, kind, steps or action list: dangling, duplicate), and for the phase with `actions`, right after its list, `NoTurnEndingAction`. Then `InvalidActionPhaseCount` (unchanged position, after the phase loop), so two action phases whose first cannot be left report `NoTurnEndingAction`. Then `UnlistedAction`: the actions in declaration order, the first that no phase lists. Win conditions and `effects.active_zone` follow, unchanged.

**No `turn` at all.** No phase lists anything, so a ruleset with a declared action and no `turn` fails with `UnlistedAction` at its first action. *(Amended by ADR-0037: unless a window lists it: windows do not need a `turn`)* A ruleset with neither `turn` nor actions is valid, as before. Consequently `ActionNotInCurrentPhase` is reachable only from a GameState whose current phase is absent, unknown to the ruleset or automatic (the Empty GameState, a trace that began an automatic phase, or a phase the ruleset does not declare), and a ruleset without a `turn` rejects every Action as `UnknownAction`.

Amends ADR-0020 (a ruleset whose Action Phase lists no action no longer loads), ADR-0012 (the known gap is closed; a declared action is always listed in the action phase; a ruleset without a `turn` declares no action), ADR-0015 (the gap sentence) and ADR-0016 (the gap sentence, which also named `end_turn` in an `if`), and edits their bodies under the amendment rule of `CLAUDE.md`. ADR-0022's `EndTurnInCardEffect` is unchanged.

**Status**: accepted
