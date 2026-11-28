# The Effects self-play test split and the YAML 1.2 `on:` key

Amends ADR-0040 (the mechanism now covers Effects), ADR-0026 and ADR-0035 (the structure of the Effects 1,000-Seed test and its minimum checks), ADR-0004 (the YAML version) and ADR-0043 (one word). Ticket [45] (#110) of the Phase H roadmap #109. Test-only and docs: nothing under `src/`, in the harness API or under `rulesets/*/*.yaml` changes; every pin, Golden Game, snapshot and frozen ruleset file is byte-identical, and the digest files stay in the freeze lists untouched.

**The split.** The 1,000 pinned Effects Seeds are played in four contiguous **Seed Ranges** (`0..250`, `250..500`, `500..750`, `750..1000`) with ADR-0040's mechanism, copied into `effects_games.rs` as `response_games.rs` and `duel_games.rs` hold it: `PARTS`, `seeds_of_range`, `SeedRangePlay`, `play_seed_range`, a `static` array of `OnceLock<SeedRangePlay>`, `seed_range`, `assert_seed_range_holds`, the `PARTS == 4` const-assert, four hand-written part tests `effects_self_play_ends_every_game_over_seeds_<a>_to_<b>_with_every_invariant_holding`, `the_effects_seed_ranges_cover_seeds_0_to_1000_once` and the joining test `effects_self_play_covers_every_effect_use_and_matches_the_pin_over_1000_seeds`. A part test plays its range under `Checks::On` and asserts, per Game, what the single test asserted: a Terminal Outcome, `TriggerCapRejections::Counted(0)` and no `FailureKind`; it reports its own failure lines, naming the Seeds. The joining test reads the ranges in index order through `seed_range` (never `play_seed_range`), so every Seed is played once per process whatever the order, and asserts that no range failed, the `longest * 2 < MAX_STEPS` headroom, the coverage counts, and the Effects Self-Play Pin compared whole over the 1,000 digest lines. The pin file is untouched and `regenerate_effects_self_play_digests` is unchanged: one serial loop, writing only that file. The `PARTS` constant, its const-assert and the part test names change together.

**The counts are exact.** ADR-0026 asserted minimums (at least 900, 650 and 700 Games) and ADR-0035 asserted each Choice count at least 1; a sum broken by the split could pass both. The joining test sums an `EffectsCoverage` (the three Game counts and a `ChoiceUse`, eight counts) over the ranges and compares the whole struct with one `assert_eq!`, so `Debug` names the count that differs. The constants `MIN_GAMES_*` are gone; their meaning is in the field docs. The values were measured on unchanged `main` (a throwaway `eprintln!` in the single test, not committed), then matched by the sum of the new code at the first run:

| Count | Over Seeds 0..1000 |
|---|---|
| Games that played a card with a triggered effect | 997 |
| Games with an aura in the field | 770 |
| Games with a self-continuous card in the field | 819 |
| Card Choices answered by the owner of the carrying card | 706 |
| Card Choices answered by the opponent | 738 |
| Option Choices answered with a non-empty option | 406 |
| Option Choices answered with an empty option | 401 |
| Card Choices skipped for want of a candidate | 95 |

ADR-0026 measured 988, 759 and 795 for the first three; they moved to 997, 770 and 819 when ADR-0035 swapped six vanilla creatures for choice cards in the decks. The five Choice counts are ADR-0035's table, unchanged. The longest Effects Game is 134 Actions, as before.

**No shared helper.** The same mechanism now spans three files, which the ticket allows to extract, but it is not: the shareable part is about ten lines (`seeds_of_range` and the failure formatting); the `static`, `SeedRangePlay`, the per-Game checks and the counts differ from file to file, and extracting would edit two passing files for no gain.

**The number of ranges, by measurement.** Warm, on a machine that also runs other sessions, so figures move by tens of seconds and more from run to run. Binary = the `selfplay` test binary run alone.

| Layout | `selfplay` binary, runs | Slowest Effects range alone |
|---|---|---|
| Before (`main`, one 1,000-Seed Effects test) | 99.5 s, 100.5 s, 110.0 s | 89.0 s (the whole test) |
| 4 Seed Ranges | 93.2 s, 90.7 s, 91.4 s, 95.7 s | 21.5 s (`0..250`) |
| 8 Seed Ranges | 105.4 s, 269.5 s (an outlier), 90.8 s, 125.1 s | 10.6 s (`0..125`) |

Eight is not better than four within the noise, so four is the smallest count within noise of the best. In one binary run with `--report-time` (140 s that time, the machine busy), the four Effects ranges took 31.4 s, 32.2 s, 35.0 s and 36.4 s beside the Response (57 to 83 s), Duel (63 to 67 s, and 111 s for the first and the joining test, which waited on each other's ranges) and Small (101 s) tests, and `replaying_an_effects_record_reproduces_the_traces_and_the_final_state`, which this ticket does not touch (a 1,000-Seed loop under `Checks::Off`), took 15.2 s. The Effects test is no longer the critical path: the Small 1,000-Seed test is (61.8 s alone, 86 to 101 s beside the others), then the Response `0..250` range, as ADR-0040 foresaw. The binary gains about 8 to 10 s and not more, because the work is conserved: the three rulesets' ranges and the Small test already keep the 12 cores of this machine busy, so the split balances the load instead of removing any.

Warm `cargo test --workspace` (everything compiled first), three runs each, including the doc-tests: before (`main`) 384 s, 302 s and 260 s; after 307 s, 335 s and 318 s, and 287 s in a fourth run with timestamps, of which the `selfplay` binary took 266 s. These figures cannot be told apart: in the same session the binary took 91 to 96 s run alone and 266 s inside `cargo test --workspace`, so the machine was slower under `cargo` (and slower than at ADR-0045's 2 min 49 s) for a reason outside the tests. Only the binary run alone shows the change, and it is a small one. It is reported here and not optimised, as ticket [45] says; splitting the Small test is the next step and is for a later ticket.

**The `on:` key.** The catalogs write the key of a triggered Card Effect unquoted (`- on: { enter: field }`). YAML 1.1 reads a bare `on` as the boolean `true`; the loader uses `serde-saphyr`, YAML 1.2, where it is the string `on`. This is the one shipped spelling that a YAML 1.1 reader would read differently, and it is accepted on purpose: the frozen catalogs stay byte-identical, and the test `an_unquoted_on_key_loads_as_the_trigger_key` (`tcg-loader/tests/triggered_effects.rs`) pins it with a block-style entry, asserting the exact loaded matcher and body (the existing `triggered` helper writes the flow style). ADR-0004's decision 2 says "YAML" and "restricted subset"; no rule that 1.1 and 1.2 must read a file the same way was ever written, so none is amended, only the version is stated. `rulesets/formats.md` says so in its first paragraph. The loader does not change.

**"Owner" in ADR-0043.** Its combat paragraph said that the controller of the lower-ATK monster, and the attacker's controller, lose Life Points; the Duel `.md` and the glossary say owner. They are the same Player while control never changes (control never changes in the Duel Ruleset today). Both clauses now say "owner", each marked as an amendment.

**Conflicts, amended in place.** ADR-0040 (*What remains over the line*, *Out of scope, by design*, a header line), ADR-0026 (the minimums, "The Effects Self-Play Pin", "the Effects test is unchanged"), ADR-0035 (the family minimums, the *Coverage* paragraph and its "at least 1"), ADR-0045 (the options "For [44]" and "not split"), ADR-0004 (decision 2) and ADR-0043 (the two clauses). Each carries an `Amended by ADR-0046:` line and the clause is marked where it stands. `CONTEXT.md` (*Seed Range*, *Self-Play Pin*, new **Effects Coverage Counts**) and `rulesets/formats.md` (the stale "which also says what ticket [42] adds", the YAML version) are updated. No rule changes; the Duel `.md` and `.yaml` are unchanged.

**Out of scope, by design.** An incremental Relabelled Game, fewer Seeds under `Checks::On`, any optimisation of the engine or of the observation checks, new coverage counts, splitting the Small test, anything of ticket [46] (Face, face-down, Concealed Card), a shared Seed Range helper, and any change to a frozen ruleset.

**Status**: accepted
