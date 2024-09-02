use std::fmt::Debug;

pub trait DropScheduler: Debug {
    fn should_drop(&mut self) -> bool;
}

pub mod constant;
pub mod uniform;
pub mod none;
pub mod specific;
pub mod ge;