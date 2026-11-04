//! Seeded randomness (skeleton for the red test checkpoint of ticket #18).

use std::num::NonZeroU64;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Seed(pub u64);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RandomState {
    state: [u64; 4],
}

impl RandomState {
    pub fn from_seed(_seed: Seed) -> RandomState {
        todo!()
    }

    pub fn next_u64(&mut self) -> u64 {
        todo!()
    }

    pub fn next_below(&mut self, _bound: NonZeroU64) -> u64 {
        todo!()
    }

    pub fn shuffle<T>(&mut self, _items: &mut [T]) {
        todo!()
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
