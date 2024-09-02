use std::collections::HashSet;

use super::DropScheduler;

#[derive(Debug)]
pub struct SpecificDropScheduler {
    /// Index of packets that must be dropped.
    to_drop: HashSet<u64>,

    /// Number of packets before looping in the `to_drop` variable again.
    cycle_len: u64,

    /// Current index.
    idx: u64,
}

impl DropScheduler for SpecificDropScheduler {
    fn should_drop(&mut self) -> bool {
        let drop = self.to_drop.contains(&(self.idx % self.cycle_len));
        self.idx += 1;
        drop
    }
}

impl SpecificDropScheduler {
    pub fn new(cycle_len: u64) -> Self {
        Self {
            to_drop: HashSet::new(),
            idx: 0,
            cycle_len,
        }
    }

    pub fn add_to_drop(&mut self, drops: &[u64]) {
        drops.iter().for_each(|v| { self.to_drop.insert(*v); });
    }
}
