//! Seeded, portable randomness: the engine's only source of randomness.
//!
//! See ADR-0006. Nothing here reads the operating system, the clock or a
//! hash seed, and everything is integer arithmetic, so a [`Seed`] gives the
//! same numbers on every platform, build profile and compiler version.

use std::num::NonZeroU64;

/// The 64-bit value a [`RandomState`] is created from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Seed(pub u64);

/// The engine's Random State: a plain value, stored in the GameState, that
/// is advanced explicitly by `&mut self` calls.
///
/// Algorithms, all implemented in this repository:
///
/// - the generator is xoshiro256** 1.0 (Blackman and Vigna, "Scrambled
///   Linear Pseudorandom Number Generators");
/// - [`RandomState::from_seed`] fills its four state words with the first
///   four outputs of SplitMix64 (Steele, Lea and Flood, 2014), which can
///   never produce the all-zero state;
/// - [`RandomState::next_below`] uses Lemire's nearly divisionless method
///   (Lemire 2019, "Fast Random Integer Generation in an Interval") with 64
///   bits and a rejection loop, so it is unbiased;
/// - [`RandomState::shuffle`] is Fisher–Yates in Durstenfeld's form.
///
/// The output for a given [`Seed`] is pinned by ADR-0006 and by known-answer
/// tests. It must never change silently.
///
/// It is deliberately not `Copy`: an accidental copy would replay the same
/// numbers. Use `clone` when a copy is really wanted. It is unrelated to
/// `std::hash::RandomState`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RandomState {
    state: [u64; 4],
}

/// One step of SplitMix64: advances `x` and returns the next output.
fn splitmix64_next(x: &mut u64) -> u64 {
    *x = x.wrapping_add(0x9e37_79b9_7f4a_7c15);
    let mut z = *x;
    z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    z ^ (z >> 31)
}

impl RandomState {
    /// Creates the Random State for `seed`.
    pub fn from_seed(seed: Seed) -> RandomState {
        let mut x = seed.0;
        let mut state = [0; 4];
        for word in &mut state {
            *word = splitmix64_next(&mut x);
        }
        RandomState { state }
    }

    /// Returns the next 64 uniformly distributed bits (xoshiro256** `next`).
    pub fn next_u64(&mut self) -> u64 {
        let [s0, s1, s2, s3] = self.state;
        let result = s1.wrapping_mul(5).rotate_left(7).wrapping_mul(9);
        let t = s1 << 17;
        let s2 = s2 ^ s0;
        let s3 = s3 ^ s1;
        let s1 = s1 ^ s2;
        let s0 = s0 ^ s3;
        let s2 = s2 ^ t;
        let s3 = s3.rotate_left(45);
        self.state = [s0, s1, s2, s3];
        result
    }

    /// Returns a uniformly distributed integer in `[0, bound)`.
    ///
    /// Always consumes at least one raw draw, including for a bound of 1.
    pub fn next_below(&mut self, bound: NonZeroU64) -> u64 {
        let s = bound.get();
        let mut m = u128::from(self.next_u64()) * u128::from(s);
        // Keeping the low 64 bits is the intent of this truncation.
        let mut low = m as u64;
        if low < s {
            let threshold = s.wrapping_neg() % s;
            while low < threshold {
                m = u128::from(self.next_u64()) * u128::from(s);
                low = m as u64;
            }
        }
        // `m >> 64` is below `s`, so it always fits in 64 bits.
        (m >> 64) as u64
    }

    /// Shuffles `items` in place with Fisher–Yates (Durstenfeld's form):
    /// for `i` from `len - 1` down to 1, swap `items[i]` with `items[j]`,
    /// where `j` is drawn uniformly from `[0, i]`. Slices of fewer than two
    /// items consume nothing.
    pub fn shuffle<T>(&mut self, items: &mut [T]) {
        for i in (1..items.len()).rev() {
            let Some(j) = self.index_up_to(i) else {
                // Unreachable on platforms whose `usize` is at most 64 bits.
                return;
            };
            items.swap(i, j);
        }
    }

    /// Draws an index uniformly from `[0, last]`; `None` only if the value
    /// does not fit the platform's `usize` or `u64`.
    fn index_up_to(&mut self, last: usize) -> Option<usize> {
        let count = u64::try_from(last).ok()?.checked_add(1)?;
        let bound = NonZeroU64::new(count)?;
        usize::try_from(self.next_below(bound)).ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seed_expansion_matches_reference_splitmix64() {
        // First four outputs of the reference splitmix64.c for each seed.
        assert_eq!(
            RandomState::from_seed(Seed(0)).state,
            [
                0xe220_a839_7b1d_cdaf,
                0x6e78_9e6a_a1b9_65f4,
                0x06c4_5d18_8009_454f,
                0xf88b_b8a8_724c_81ec,
            ]
        );
        assert_eq!(
            RandomState::from_seed(Seed(u64::MAX)).state,
            [
                0xe4d9_7177_1b65_2c20,
                0xe99f_f867_dbf6_82c9,
                0x382f_f84c_b272_81e9,
                0x6d1d_b36c_cba9_82d2,
            ]
        );
    }

    #[test]
    fn raw_state_one_two_three_four_matches_published_sequence() {
        // Second, independent source: the rand_xoshiro crate's `reference`
        // test (decimal there) for raw state [1, 2, 3, 4]. Read outside this
        // repository; the crate is not a dependency.
        let mut random_state = RandomState {
            state: [1, 2, 3, 4],
        };
        let expected: [u64; 10] = [
            11_520,
            0,
            1_509_978_240,
            1_215_971_899_390_074_240,
            1_216_172_134_540_287_360,
            607_988_272_756_665_600,
            16_172_922_978_634_559_625,
            8_476_171_486_693_032_832,
            10_595_114_339_597_558_777,
            2_904_607_092_377_533_576,
        ];
        for value in expected {
            assert_eq!(random_state.next_u64(), value);
        }
    }
}
