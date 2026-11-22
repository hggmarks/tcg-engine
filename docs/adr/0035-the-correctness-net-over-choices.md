# The correctness net over choices

Self-play and the Golden Games now exercise Choices on the shipped Effects decks, under every Invariant of ADR-0020, ADR-0026 and ADR-0031 (ticket [31], #77, of the Choices roadmap #73; checkpoint #67 decisions). No new Invariant: the twelve existing ones cover Answers. The Uniform Random Policy, the Step Cap, `FailureKind` and the harness API are unchanged, and nothing under `rulesets/small-ruleset/` changes (ADR-0022). Nothing in `src/` or in the harness changes: coverage is counted in the tests from the Game Record and Event Traces, as ADR-0026's `effect_use` is.

**Decks: one swap per card, in place, in both Effects decks.** `iron_guard` 2 becomes `snare_vine` 2, `wind_hawk` 2 becomes `mind_leech` 2, and `ember_hound` 4 becomes `ember_hound` 2 followed directly by `curious_owl` 2. Only vanilla creatures leave, so the family minimums of ADR-0026 (900 / 650 / 700 Games) keep their cards, and the swaps sit at the same positions, so deck sizes stay 50 and every other Card Instance Id stays where it was: the Effects Leader-kill script (ids 1, 7, 13, 22, 31, 34, 48, seat 1 only passes) is valid unchanged. Each swap is recorded in `effects-ruleset.md` (Sample decks) and in the deck and catalog comments, which no longer say the three cards are in no shipped deck.

**Coverage, counted over the 1,000 pinned Seeds in the same pass** as the Invariants (`effects_self_play_ends_every_game_over_1000_seeds_with_every_invariant_holding`; no second 1,000-Seed loop). `ChoiceUse` has five counts, read from the Event Traces and the catalog only; each `DecisionAnswered` is paired with the `DecisionPending` before it. A test asserts each is at least 1, naming the count. Measured:

| Count | Over Seeds 0..1000 |
|---|---|
| Card Choices answered by the controller (the Deciding Player owns the carrying card) | 706 |
| Card Choices answered by the opponent | 738 |
| Option Choices answered with a non-empty option | 406 |
| Option Choices answered with an empty option | 401 |
| Card Choices skipped for want of a candidate | 95 |

An option is empty when `options[i].is_empty()` in the catalog, read through the Paused Resolution's trigger and path (`TriggeredCardEffect::body()`, then the `EffectPath` to the `ChooseOne`), never from the length of a trace; that needs no new public engine API.

**Skipped, defined.** A card whose Card Definition holds a `choose_card` entered the field, and no `DecisionPending` with `paused.trigger.card` equal to that card occurs from that trace up to and including the first trace that ends with no Pending Decision and no `GameEnded`: only then has the Resolution certainly completed (a trigger queued behind another pause is asked in a later trace, and a Game that ends mid-Resolution proves nothing). If the Game ends first, nothing is counted. The scan includes the entering trace itself, which can be that completing trace.

**The Pausing Golden Game.** `effects-pausing`, Seed 204, the shipped Effects decks, 23 Actions, ending in a Leader kill by seat 0 (`effects_pausing_script`, its cards named by `PAUSING_*` constants): `curious_owl` pauses for seat 0 and is answered with option 0; `mind_leech` is played by seat 1 and seat 0, the opponent, answers by moving one of its own creatures to its trash; `snare_vine` is played by seat 1 while seat 0 has creatures in the field and seat 1, its controller, taps one. The Seed was found by a throwaway search (a scripted bot over Seeds 0..400 that prefers the three cards and attacks the Leader, keeping the shortest Game that holds all three Choices and ends); the search is not committed, the literal script is the Game. Its snapshots are `crates/tcg-loader/tests/golden/effects-pausing.trace` and `effects-pausing.observed-by-player-0.trace` and `-1.trace`, in ADR-0017's and ADR-0031's layouts. An Answer is written `== action N: player P answer option I` or `answer card C` in the Event Trace snapshot; the observed ones use the Taken Action's own `Debug`. They are compared byte for byte, reporting the first differing line; the two observed snapshots differ from each other; the test asserts a Card `DecisionPending` whose decider owns the carrying card, one whose decider does not, an Option one, and a Terminal Outcome with winners `[PlayerId(0)]`, so a script edit that stops short is caught outside the snapshot. One ignored regenerator, `regenerate_pausing_golden_snapshots`, writes the three files and touches no other.

**Regenerated, deliberately.** `effects-leader-kill.trace` changes only in the `CardCreated` lines of swapped definitions (and nothing else); `effects-leader-kill.observed-by-player-1.trace` in one line (a card seat 1 draws is now `mind_leech`); `effects-leader-kill.observed-by-player-0.trace` is unchanged. The Effects Self-Play Pin is regenerated with `regenerate_effects_self_play_digests`: every line changes, because every Game's creation trace names the new definitions. The Small snapshots, the Small pin and the freeze digests are untouched and `the_small_ruleset_is_frozen_since_phase_c` passes. The Effects Golden Game helpers of `effects_ruleset.rs` are generalised over `GoldenGame { name, seed, script }` so the pausing game reuses them.

**Replay.** For the Effects Leader-kill, the Pausing, the Small Leader-kill and the Small Deck-out Golden Games, a test builds a `GameRecord` from the script and asserts `replay` returns the same creation, the same Event Traces and the same final GameState as the Game played (`replaying_the_record_of_<game>_reproduces_its_game`). Aura-combat's own replay test is unchanged. `effects_observing_never_changes_a_game` keeps its asserts and its Seeds 7 to 11, which already hold a Card Answer and an Option Answer, and now asserts both.

**Step Cap.** The longest Game of the 1,000 is 134 Actions, so `longest * 2 < MAX_STEPS` keeps its headroom and `MAX_STEPS` stays 1,000.

**One existing test changed.** `invariant_observations_stay_consistent_detects_a_violation` picked the first transition with at least 3 Observed Events, which with the new decks is a paused step, and while a decision is pending only its Answer applies (ADR-0033), so its part (b) could not add a card. It now picks the first such transition that does not end paused; its assertions are the same.

**Throughput, measured.** Release, `cargo run --release -p tcg-selfplay --example self_play`, 1,000 Games per cell, Games per second (Actions per second), in ADR-0031's layout:

| Ruleset | Off | EngineOnly | On |
|---|---|---|---|
| Small | 190 (7,479) | 150 (5,911) | 18 (726) |
| Effects | 110 (5,479) | 97 (4,838) | 12 (602) |

Not optimised. The Small cells are the same code on the same Games as ADR-0031's table (195, 153, 19): the difference is the machine and noise. Effects Games are longer now (49,880 Actions over 1,000 Games, Answers included), and the per-Action rate fell about 7 percent with checks off (5,886 to 5,479), about 2 percent with every check on (603 to 602): a pause costs one more listing and one more Action, and the checks scale with the number of Actions.

**The test profile.** Measured the same way on both, warm (everything compiled, then `cargo test --workspace` timed): `main` 1 min 25 s, of which the `selfplay` test binary 78.0 s; this branch 1 min 41 s, of which `selfplay` 93.8 s. About 18 percent more, from the extra Actions of the Games and the choice counting in the existing pass; no second 1,000-Seed loop was added.

**Conflicts, amended in place.** ADR-0026 (the sample decks' composition, the Effects Golden Game and Effects Self-Play Pin being regenerated, and its ADR-0033 amendment "No shipped deck holds such a card"), ADR-0031 (Observed Golden Games), ADR-0017 (a committed Golden Game now holds Answers, and the Effects snapshots were regenerated), ADR-0033 and ADR-0034 (their statements that the shipped decks do not list `curious_owl`, `snare_vine` and `mind_leech`, and that no shipped Golden Game or pin holds a pause). Each carries an `Amended by ADR-0035:` line and the clause is marked where it stands. No other ADR changes.

**Out of scope, by design.** New Invariants, other policies, optimisation (an incremental Relabelled Game included), new cards, any change to the Small Ruleset, and a change to the harness API.

**Status**: accepted
