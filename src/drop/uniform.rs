use super::DropScheduler;
use rand::rngs::SmallRng;
use rand::{Rng, SeedableRng};

#[derive(Debug)]
pub struct UniformDropScheduler {
    rng: SmallRng,

    rate: f64,
}

impl DropScheduler for UniformDropScheduler {
    fn should_drop(&mut self) -> bool {
        self.rng.gen_bool(self.rate)
    }
}

impl UniformDropScheduler {
    pub fn new(rate: f64, seed: u64) -> Self {
        Self {
            rng: SmallRng::seed_from_u64(seed),
            rate,
        }
    }
}