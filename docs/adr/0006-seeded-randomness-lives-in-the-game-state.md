# Seeded randomness lives in the GameState

The Random State is the engine's only source of randomness. It is xoshiro256** seeded by SplitMix64, with Lemire's unbiased bounded draws and a Fisher–Yates shuffle, implemented in this repository with integer arithmetic only so it behaves identically on every platform, build profile and compiler version, including `wasm32-unknown-unknown`. It is a plain value stored in the GameState and advanced explicitly; there is no global or hidden generator. No other source of randomness is allowed in the engine: no OS entropy, RNG crates, wall-clock time or hash seeds. Known-answer tests pin its output. Any change to that output must be deliberate and versioned, through a superseding ADR and new vectors, because Game records will depend on it.

**Status**: accepted
