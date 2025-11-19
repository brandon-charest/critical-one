pub trait Roller {
    fn roll_in_range(&mut self, max: u32) -> u32;
}

pub struct ThreadRngRoller {
    rng: rand::rngs::ThreadRng,
}

impl ThreadRngRoller {
    pub fn new() -> Self {
        Self { rng: rand::rng() }
    }
}

impl Roller for ThreadRngRoller {
    fn roll_in_range(&mut self, max: u32) -> u32 {
        use rand::Rng;
        self.rng.random_range(1..=max)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_thread_rng_roller_roll_in_range() {
        let mut roller = ThreadRngRoller::new();

        // Test roll_in_range returns a value between 1 and max (inclusive)
        for _ in 0..100 {
            let roll = roller.roll_in_range(10);
            assert!(roll >= 1 && roll <= 10);
        }

        // Test roll_in_range returns the same value when max is 1
        let mut roller = ThreadRngRoller::new();
        let roll = roller.roll_in_range(1);
        assert_eq!(roll, 1);
    }
}
