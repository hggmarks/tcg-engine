# Closing the self-play report's known gaps

Three gaps that the reviews of Phases D and E left open in the harness are closed (ticket [33], #84, of the Responses roadmap #83). Harness-only: nothing in `src/`, `crates/tcg-loader/` or `rulesets/` changes, and the Golden Games, their observed snapshots and both self-play pins are unchanged.

**The phase is a part the Recomputed GameState names.** `state_difference` compares the phase right after the Active Player: "the phase is `"main"` in the Recomputed GameState and `"start"` in the engine's", `none` when no phase has begun. It used to fall through to the generic "the Random State or another part of the GameState differs" (ADR-0026).

**"Not measured" is a value of its own.** `SelfPlayGame::trigger_cap_rejections` is a `TriggerCapRejections`, an enum, not an `Option<usize>`: `NotMeasured` for a Game run with `Checks::Off`, whose listings (`legal_actions`) never see a refused candidate, and `Counted(n)` otherwise, zero included (`Checks::EngineOnly` lists with rejections as `Checks::On` does, so it counts). Its `Display` is "Trigger Cap Rejections not measured" or "{n} Trigger Cap Rejections", and the `self_play` example prints it, so the three cases read differently. The Game itself is unchanged by the checks; only this field differs between `Off` and the others, so a test comparing an `On` and an `Off` Game compares record, creation, traces and state, and asserts the field.

**The Relabelling re-shuffles a carried shuffle.** Step 3 also visits the `ZoneShuffled` Events carried in `DecisionPending.paused.unscanned`, at the position of the `DecisionPending` in Event order and the carried Events in their own order, with the same hidden-position shuffle and the same generator, through one function shared with the trace path. No `do` emits a shuffle today, so no Game draws differently and nothing is regenerated; a future shuffle effect cannot slip past the leak check.

**Conflicts, amended in place.** ADR-0026 (the state differences, and `trigger_cap_rejections` as `0` with checks off) and ADR-0031 (the Relabelling's step 3). ADR-0020 states no report field, and ADR-0023 no count, so neither changes.

**Out of scope.** Relabelling the Random State and a non-public Active Zone (ADR-0031's known limits), performance work, and everything of [34].

**Status**: accepted
