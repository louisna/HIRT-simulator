use super::DropScheduler;
use rand::rngs::SmallRng;
use rand::{Rng, SeedableRng};

#[derive(Debug)]
enum State {
    Keep,
    Drop,
}

#[derive(Debug)]
pub struct GilbertEliotDropSheduler {
    /// Current state of the scheduler.
    state: State,

    /// Probability to move from the `Keep` state to the `Drop` state.
    g2b: f64,

    /// Probability yo move from the `Drop` state to the `Keep` state.
    b2g: f64,

    /// Probability to drop a packet in the `Keep` state.
    /// Currently always set to 0.
    dg: f64,

    /// Probability to drop a packet in the `Drop` state.
    /// Currently always set to 1.
    db: f64,

    /// Random number generator.
    rng: SmallRng,
}

impl DropScheduler for GilbertEliotDropSheduler {
    fn should_drop(&mut self) -> bool {
        let (proba_change, proba_drop) = match self.state {
            State::Keep => (self.g2b, self.dg),
            State::Drop => (self.b2g, self.db),
        };
        if self.rng.gen_bool(proba_change) {
            self.change_state();
        }
        self.rng.gen_bool(proba_drop)
    }
}

impl GilbertEliotDropSheduler {
    fn change_state(&mut self) {
        self.state = match self.state {
            State::Keep => State::Drop,
            State::Drop => State::Keep,
        }
    }

    pub fn new_simple(g2b: f64, b2g: f64, seed: u64) -> Self {
        Self {
            state: State::Keep,
            g2b,
            b2g,
            dg: 0.0,
            db: 1.0,
            rng: SmallRng::seed_from_u64(seed),
        }
    }
}
