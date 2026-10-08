//! The one source of chance: a seeded xorshift generator. The core may not
//! read a random device (BUILDER-SPEC R2), so every roll is reproducible.

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rng(u64);

#[allow(clippy::should_implement_trait)]
impl Rng {
    pub fn new(seed: u64) -> Rng {
        Rng((seed ^ 0x9E37_79B9_7F4A_7C15) | 1)
    }

    pub fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    /// A whole number in `[lo, hi]`.
    pub fn range(&mut self, lo: i32, hi: i32) -> i32 {
        if hi <= lo {
            return lo;
        }
        lo + (self.next() % (hi - lo + 1) as u64) as i32
    }

    /// True `pct` times in a hundred.
    pub fn chance(&mut self, pct: i32) -> bool {
        self.range(0, 99) < pct
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_same_seed_gives_the_same_rolls() {
        let (mut a, mut b) = (Rng::new(7), Rng::new(7));
        for _ in 0..100 {
            assert_eq!(a.next(), b.next());
        }
        assert_ne!(Rng::new(7).next(), Rng::new(8).next());
    }

    #[test]
    fn range_stays_inside_its_bounds_and_reaches_both_ends() {
        let mut r = Rng::new(1);
        let rolls: Vec<i32> = (0..2000).map(|_| r.range(3, 6)).collect();
        assert!(rolls.iter().all(|x| (3..=6).contains(x)));
        assert!(rolls.contains(&3) && rolls.contains(&6));
    }
}
