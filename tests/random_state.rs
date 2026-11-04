//! Acceptance tests for the Random State (ticket #18, ADR-0006).
//!
//! Every expected value below is a known answer produced by the published
//! reference implementations, never by this crate:
//!
//! - xoshiro256** 1.0: <https://prng.di.unimi.it/xoshiro256starstar.c>
//! - SplitMix64:       <https://prng.di.unimi.it/splitmix64.c>
//!
//! The bounded draws and the shuffle come from a C harness that links those
//! two files unchanged (renaming their `next` functions and un-`static`-ing
//! their state) and adds, to be regenerated with any C compiler:
//!
//! ```c
//! // Seeding: x = seed; for i in 0..4 { s[i] = splitmix64_next(); }
//! // Lemire 2019, "Fast Random Integer Generation in an Interval",
//! // nearly divisionless listing, 64 bits with a 128-bit product:
//! uint64_t below(uint64_t s) {
//!   uint64_t x = random64();                       // xoshiro256** next()
//!   unsigned __int128 m = (unsigned __int128)x * s;
//!   uint64_t l = (uint64_t)m;
//!   if (l < s) {
//!     uint64_t t = -s % s;
//!     while (l < t) { x = random64(); m = (unsigned __int128)x * s; l = (uint64_t)m; }
//!   }
//!   return (uint64_t)(m >> 64);
//! }
//! // Shuffle (Fisher-Yates, Durstenfeld): a[i] = i for i in 0..52, then
//! // for (i = 51; i >= 1; i--) { j = below(i + 1); swap(a[i], a[j]); }
//! ```
//!
//! The harness counts every raw `next()` call, which gives the draw counts
//! used by the rejection test.

use std::num::NonZeroU64;

use tcg_engine::random::{RandomState, Seed};

fn bound(n: u64) -> NonZeroU64 {
    NonZeroU64::new(n).expect("test bounds are non-zero")
}

fn draws(state: &mut RandomState, count: usize) -> Vec<u64> {
    (0..count).map(|_| state.next_u64()).collect()
}

fn advanced(seed: Seed, raw_draws: usize) -> RandomState {
    let mut state = RandomState::from_seed(seed);
    for _ in 0..raw_draws {
        state.next_u64();
    }
    state
}

#[test]
fn random_state_from_seed_matches_reference_sequence() {
    let cases: [(Seed, [u64; 8]); 4] = [
        (
            Seed(0),
            [
                0x99ec_5f36_cb75_f2b4,
                0xbf6e_1f78_4956_452a,
                0x1a5f_849d_4933_e6e0,
                0x6aa5_94f1_262d_2d2c,
                0xbba5_ad4a_1f84_2e59,
                0xffef_8375_d9eb_caca,
                0x6c16_0dee_d2f5_4c98,
                0x8920_ad64_8fc3_0a3f,
            ],
        ),
        (
            Seed(1),
            [
                0xb3f2_af6d_0fc7_10c5,
                0x853b_5596_4736_4cea,
                0x92f8_9756_082a_4514,
                0x642e_1c7b_c266_a3a7,
                0xb27a_48e2_9a23_3673,
                0x24c1_2312_6ffd_a722,
                0x1230_04ef_8df5_10e6,
                0x6195_4dcc_47b1_e89d,
            ],
        ),
        (
            Seed(0x0123_4567_89AB_CDEF),
            [
                0xa2c2_a420_38d4_ec3d,
                0x05fc_25d0_738e_7b0f,
                0x625e_7bff_938e_701e,
                0x1ba4_ddc6_fe2b_5726,
                0xdf0a_2482_ac92_54cf,
                0x3939_eda8_66ab_abe8,
                0xb59d_922d_b3f2_da81,
                0x8047_2ee1_3a97_0c2a,
            ],
        ),
        (
            Seed(u64::MAX),
            [
                0x8f55_20d5_2a7e_ad08,
                0xc476_a018_caa1_802d,
                0x81de_31c0_d260_469e,
                0xbf65_8d7e_065f_3c2f,
                0x9135_93fd_a1bc_a32a,
                0xbb53_5e93_941b_a525,
                0x5ecd_a415_c3c6_dfde,
                0xc487_398f_c9de_9ae2,
            ],
        ),
    ];
    for (seed, expected) in cases {
        let mut state = RandomState::from_seed(seed);
        assert_eq!(draws(&mut state, 8), expected, "seed {seed:?}");
    }
}

#[test]
fn random_state_is_a_plain_comparable_value() {
    let mut original = RandomState::from_seed(Seed(99));
    let mut copy = original.clone();
    assert_eq!(original, copy);
    assert_eq!(draws(&mut original, 5), draws(&mut copy, 5));
    assert_eq!(original, copy);

    original.next_u64();
    assert_ne!(original, copy);

    assert!(format!("{original:?}").contains("RandomState"));
}

#[test]
fn advancing_a_clone_leaves_the_original_unchanged() {
    let original = RandomState::from_seed(Seed(7));
    let saved = original.clone();
    let mut clone = original.clone();
    draws(&mut clone, 10);
    assert_eq!(original, saved);
    assert_ne!(clone, saved);
}

#[test]
fn next_below_matches_reference_sequence() {
    let mut state = RandomState::from_seed(Seed(0x0123_4567_89AB_CDEF));
    let below_six: Vec<u64> = (0..20).map(|_| state.next_below(bound(6))).collect();
    assert_eq!(
        below_six,
        [3, 0, 2, 0, 5, 1, 4, 3, 5, 3, 5, 5, 4, 4, 2, 3, 2, 0, 1, 2]
    );
    let below_max: Vec<u64> = (0..5).map(|_| state.next_below(bound(u64::MAX))).collect();
    assert_eq!(
        below_max,
        [
            0x141f_55e4_d87e_8f02,
            0xe792_0082_7057_1698,
            0xf888_4748_b625_482f,
            0x34f5_0681_5090_7c6f,
            0x9b79_afe4_caa3_1e67,
        ]
    );
    // 20 + 5 results; the harness reports 25 raw draws, so no rejection
    // happened for these bounds.
    assert_eq!(state, advanced(Seed(0x0123_4567_89AB_CDEF), 25));
}

#[test]
fn next_below_rejection_path_matches_reference() {
    // Bound (1 << 63) + 1 rejects close to half of the raw draws, so a
    // plain-modulo or multiply-shift implementation cannot match.
    let mut state = RandomState::from_seed(Seed(0));
    let results: Vec<u64> = (0..10)
        .map(|_| state.next_below(bound((1 << 63) + 1)))
        .collect();
    assert_eq!(
        results,
        [
            0x4cf6_2f9b_65ba_f95a,
            0x5fb7_0fbc_24ab_2295,
            0x7ff7_c1ba_ecf5_e565,
            0x089b_0dfa_9350_a5da,
            0x5a2a_f562_1a8c_3336,
            0x465f_3bbb_2cc5_3cc6,
            0x0765_efbf_fe6b_93f2,
            0x505e_8062_c923_88bb,
            0x7e4d_c1d1_d063_1dcf,
            0x26bc_3607_8545_c477,
        ]
    );
    // The harness reports 26 raw draws for these 10 results: 16 rejections.
    let raw_draws = 26;
    assert!(raw_draws > 10);
    assert_eq!(state, advanced(Seed(0), raw_draws));
}

#[test]
fn next_below_one_is_zero_and_consumes_one_draw() {
    let mut state = RandomState::from_seed(Seed(3));
    let results: Vec<u64> = (0..5).map(|_| state.next_below(bound(1))).collect();
    assert_eq!(results, [0; 5]);
    assert_eq!(state, advanced(Seed(3), 5));
}

#[test]
fn shuffle_matches_reference_permutation() {
    let mut state = RandomState::from_seed(Seed(42));
    let mut items: Vec<u64> = (0..52).collect();
    state.shuffle(&mut items);
    assert_eq!(
        items,
        [
            40, 1, 51, 35, 31, 3, 18, 21, 42, 37, 7, 20, 46, 6, 0, 16, 44, 14, 43, 41, 26, 9, 30,
            15, 50, 10, 49, 8, 17, 13, 5, 2, 23, 24, 29, 22, 39, 27, 12, 32, 11, 28, 25, 48, 38,
            33, 36, 47, 45, 34, 19, 4
        ]
    );
    // The harness reports 51 raw draws (one per index 51 down to 1).
    assert_eq!(state, advanced(Seed(42), 51));
    assert_eq!(state.next_u64(), 0x5146_6fd4_3f32_a0ec);
}

#[test]
fn shuffling_zero_or_one_items_leaves_random_state_unchanged() {
    let mut state = RandomState::from_seed(Seed(5));
    let before = state.clone();

    let mut empty: Vec<u64> = Vec::new();
    state.shuffle(&mut empty);
    assert_eq!(state, before);

    let mut single = [17_u64];
    state.shuffle(&mut single);
    assert_eq!(single, [17]);
    assert_eq!(state, before);
}
