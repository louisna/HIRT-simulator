use super::DropScheduler;

#[derive(Debug)]
pub struct ConstantDropScheduler {
    /// Drop a packet every `step` calls to [`Self::should_drop`].
    step: u64,

    /// Counter of calls from the last time a packet was dropped.
    nb_calls: u64,
}

impl DropScheduler for ConstantDropScheduler {
    fn should_drop(&mut self) -> bool {
        self.nb_calls += 1;
        if self.nb_calls >= self.step {
            self.nb_calls = 0;
            true
        } else {
            false
        }
    }
}

impl ConstantDropScheduler {
    pub fn new(step: u64) -> Self {
        Self { step, nb_calls: 0 }
    }
}
