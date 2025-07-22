use crate::{absolute_block_number::Absolute, relative_block_number::Relative};

#[derive(Debug, Clone)]
pub struct DealTimeline<BlockNumber> {
    start: Absolute<BlockNumber>,
    duration: Relative<BlockNumber>,
}

impl<BlockNumber> DealTimeline<BlockNumber>
where
    BlockNumber: sp_runtime::traits::BlockNumber,
{
    pub fn new(start: Absolute<BlockNumber>, duration: Relative<BlockNumber>) -> Self {
        assert!(!duration.0.is_zero());
        Self { start, duration }
    }

    pub fn start(&self) -> Absolute<BlockNumber> {
        self.start
    }

    pub fn duration(&self) -> Relative<BlockNumber> {
        self.duration
    }

    pub fn end(&self) -> Absolute<BlockNumber> {
        self.start() + self.duration()
    }
}
