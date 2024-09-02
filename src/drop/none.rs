use super::DropScheduler;

#[derive(Debug)]
pub struct NoDropScheduler {}

impl DropScheduler for NoDropScheduler {
    fn should_drop(&mut self) -> bool {
        false
    }
}