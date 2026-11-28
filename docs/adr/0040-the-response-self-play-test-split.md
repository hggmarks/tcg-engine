# The Response self-play test split

Amended by ADR-0046: the mechanism now covers the Effects Ruleset too, in four Seed Ranges of `effects_games.rs` with its own copy of the code, and the Effects test is no longer unsplit (marked in *What remains over the line* and *Out of scope, by design*).

Amends ADR-0039 (below). The 1,000 pinned Response Seeds are played in four contiguous **Seed Ranges** (`0..250`, `250..500`, `500..750`, `750..1000`), one test each, and a last test joins them (ticket [38], #95, of the Phase G roadmap #94). Test-only: nothing under `src/`, in the harness API or under `rulesets/` changes, and no Seed is dropped, no Invariant weakened, nothing optimised. ADR-0039 reported that `response_self_play_ends_every_game_over_1000_seeds_with_every_invariant_holding` alone took 140.8 s and was the critical path of the whole `cargo test --workspace`; the test harness runs tests on parallel threads, so splitting its Seeds across tests shortens it.

**The tests.** `response_self_play_ends_every_game_over_seeds_<a>_to_<b>_with_every_invariant_holding` plays the Seeds `a..b` under `Checks::On` and asserts, per Game, what the single test asserted: a Terminal Outcome, an empty Stack at the end, `TriggerCapRejections::Counted(0)`, as many `Activate` Actions as `Ability` items pushed, and no `FailureKind`; the Seeds of 0..100 also go through `verify_record`. Failures are collected as data, so a part test reports its own lines. `the_response_seed_ranges_cover_seeds_0_to_1000_once` asserts, without playing a Game, that the ranges are contiguous, disjoint, start at 0 and end at the last pinned Seed (and a `const` assert holds that the count divides 1,000). `response_self_play_covers_every_response_use_and_matches_the_pin_over_1000_seeds` is the last step: it asserts that no range has a failure, the `longest * 2 < MAX_STEPS` headroom, the five coverage counts summed over all 1,000 Seeds, each at least 1 and named in its own message, and then equal to the five values of ADR-0039's table (1,684, 305, 143, 952 and 18,835) by one `assert_eq!` on `ResponseUse`, so that a broken sum fails and not only a count of 0 (replacing the sum by the last range's counts gives 401, 69, 27, 236 and 4,457), and the Response Self-Play Pin compared whole over the 1,000 digest lines in Seed order. The pin file is byte-identical; `regenerate_response_self_play_digests` is unchanged, still one serial loop, and still writes only that file.

**The hand-off.** Each Seed Range is a pure function of its Seeds, so its result (failure lines, longest Game, digest lines, `ResponseUse`) is computed at most once per test process by `std::sync::OnceLock::get_or_init`, in a `static` array of the test file. A part test and the joining test call the same accessor: whichever reaches a range first plays it, and any other caller waits until it is done. Therefore every Seed is played exactly once per `cargo test --workspace`, whatever the order of the tests, with `--test-threads=1`, or with a name filter that runs only the joining test or only one part. The joining test reads the ranges in index order, so its output is deterministic. It plays any Seed Range that no other test has started (usually range 0, and all of them under a name filter), and each range is still played only once per process. If a Seed Range panics while being played, the next caller plays it again; this happens only when a test is already failing. It is test scaffolding: no engine state, no global mutable game state, no randomness other than the seeded one.

**Why not the alternatives.** Threads inside one test: `std::thread` is disallowed by `clippy.toml`, and the test harness already provides the parallelism. Files written at test time: the ticket forbids them. Two passes (the parts play, then the joining test plays again): the Games must be played once.

**Measured.** Warm (everything compiled first), as ADR-0039, on the same machine, which also runs other sessions, so the figures move by tens of seconds from run to run. Before is `main`; after is this branch.

| | `cargo test --workspace` | `selfplay` binary |
|---|---|---|
| Before (`main`, one 1,000-Seed Response test) | 3 min 36 s | 201 s |
| After, 4 Seed Ranges | 2 min 06 s | 115 s (100 s, 115 s and 137 s in three runs) |

The count of Seed Ranges, by measurement of the binary (the figure is the binary's time; the slowest range in the same run):

| Seed Ranges | `selfplay` binary | Slowest Response range | Effects 1,000-Seed test |
|---|---|---|---|
| 4 | 136.8 s | 88 s (`0..250`, which also holds the `verify_record` of Seeds 0..100) | 136.7 s |
| 8 | 145.4 s | 72 s (`0..125`) | 145.4 s |
| 10 | 144.9 s | 63 s (`0..100`) | 144.9 s |

The binary's time is the same within noise for 4, 8 and 10: once the Response Seeds are split, the critical path is no longer the Response test but **`effects_self_play_ends_every_game_over_1000_seeds_with_every_invariant_holding`** *(Amended by ADR-0046: the test is split into four Seed Ranges and gone; the next critical path is the Small 1,000-Seed test)*, which takes 114.7 s run alone on this machine today and 137 to 145 s beside the Response parts (the Small 1,000-Seed test follows with 86 to 96 s). Four is the smallest count within noise of the best, so four it is. More ranges would help only if the Effects test shrank too.

**What remains over the line.** The binary is about 2 min or a little more (100 to 137 s in the runs above, against 201 s before and ADR-0039's 157.5 s on its measurement), not clearly below it, because the Effects test alone takes 115 s here. Splitting the Effects (and Small) test the same way is allowed by the ticket only with this exact mechanism and no extra code path, and is not done here: it is reported on the PR. *(Superseded by ADR-0046: the Effects test is split into four Seed Ranges with this mechanism; the Small test is still one test of about 60 s alone.)*

**Conflicts, amended in place.** ADR-0039 (the *Coverage* paragraph, *The Response Self-Play Pin*, the `verify_record` sentence, *The test profile* and *For [37]*: each carries an `Amended by ADR-0040` or `Superseded by ADR-0040` marker where it stands), ADR-0026 (the Response pin's "same pass") and `CONTEXT.md` (*Self-Play Pin*, *Unanswered Window*; new term **Seed Range**). ADR-0020 and ADR-0035 state no structure of the Response test and need no change.

**Out of scope, by design.** An incremental Relabelled Game, fewer Seeds under `Checks::On`, any optimisation of the engine or of the observation checks, new coverage counts, a change to the Small or Effects tests *(Amended by ADR-0046: the Effects test is split, in ticket [45])*, and any change under `rulesets/`.

**Status**: accepted
