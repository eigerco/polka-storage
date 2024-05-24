/// SectorSize indicates one of a set of possible sizes in the network.
#[derive(Clone, Debug, PartialEq, Eq, Copy)]
#[repr(u64)]
pub enum SectorSize {
    _2KiB = 2 << 10,
    _8MiB = 8 << 20,
    _512MiB = 512 << 20,
    _32GiB = 32 << 30,
    _64GiB = 2 * (32 << 30),
}

/// Proof of spacetime type, indicating version and sector size of the proof.
#[derive(PartialEq, Eq, Copy, Clone, Debug, Hash)]
pub enum RegisteredPoStProof {
    StackedDRGWinning2KiBV1,
    StackedDRGWinning8MiBV1,
    StackedDRGWinning512MiBV1,
    StackedDRGWinning32GiBV1,
    StackedDRGWinning64GiBV1,
    StackedDRGWindow2KiBV1,
    StackedDRGWindow8MiBV1,
    StackedDRGWindow512MiBV1,
    StackedDRGWindow32GiBV1,
    StackedDRGWindow64GiBV1,
    StackedDRGWindow2KiBV1P1,
    StackedDRGWindow8MiBV1P1,
    StackedDRGWindow512MiBV1P1,
    StackedDRGWindow32GiBV1P1,
    StackedDRGWindow64GiBV1P1,
    Invalid(i64),
}

impl RegisteredPoStProof {
    /// Returns the sector size of the proof type, which is measured in bytes.
    pub fn sector_size(self) -> Result<SectorSize, String> {
        use RegisteredPoStProof::*;
        match self {
            StackedDRGWindow2KiBV1P1 | StackedDRGWindow2KiBV1 | StackedDRGWinning2KiBV1 => {
                Ok(SectorSize::_2KiB)
            }
            StackedDRGWindow8MiBV1P1 | StackedDRGWindow8MiBV1 | StackedDRGWinning8MiBV1 => {
                Ok(SectorSize::_8MiB)
            }
            StackedDRGWindow512MiBV1P1 | StackedDRGWindow512MiBV1 | StackedDRGWinning512MiBV1 => {
                Ok(SectorSize::_512MiB)
            }
            StackedDRGWindow32GiBV1P1 | StackedDRGWindow32GiBV1 | StackedDRGWinning32GiBV1 => {
                Ok(SectorSize::_32GiB)
            }
            StackedDRGWindow64GiBV1P1 | StackedDRGWindow64GiBV1 | StackedDRGWinning64GiBV1 => {
                Ok(SectorSize::_64GiB)
            }
            Invalid(i) => Err(format!("unsupported proof type: {}", i)),
        }
    }

    /// Proof size for each PoStProof type
    pub fn proof_size(self) -> Result<usize, String> {
        use RegisteredPoStProof::*;
        match self {
            StackedDRGWinning2KiBV1
            | StackedDRGWinning8MiBV1
            | StackedDRGWinning512MiBV1
            | StackedDRGWinning32GiBV1
            | StackedDRGWinning64GiBV1
            | StackedDRGWindow2KiBV1
            | StackedDRGWindow8MiBV1
            | StackedDRGWindow512MiBV1
            | StackedDRGWindow32GiBV1
            | StackedDRGWindow64GiBV1
            | StackedDRGWindow2KiBV1P1
            | StackedDRGWindow8MiBV1P1
            | StackedDRGWindow512MiBV1P1
            | StackedDRGWindow32GiBV1P1
            | StackedDRGWindow64GiBV1P1 => Ok(192),
            Invalid(i) => Err(format!("unsupported proof type: {}", i)),
        }
    }
    /// Returns the partition size, in sectors, associated with a proof type.
    /// The partition size is the number of sectors proven in a single PoSt proof.
    pub fn window_post_partitions_sector(self) -> Result<u64, String> {
        // Resolve to post proof and then compute size from that.
        use RegisteredPoStProof::*;
        match self {
            StackedDRGWinning64GiBV1 | StackedDRGWindow64GiBV1 | StackedDRGWindow64GiBV1P1 => {
                Ok(2300)
            }
            StackedDRGWinning32GiBV1 | StackedDRGWindow32GiBV1 | StackedDRGWindow32GiBV1P1 => {
                Ok(2349)
            }
            StackedDRGWinning2KiBV1 | StackedDRGWindow2KiBV1 | StackedDRGWindow2KiBV1P1 => Ok(2),
            StackedDRGWinning8MiBV1 | StackedDRGWindow8MiBV1 | StackedDRGWindow8MiBV1P1 => Ok(2),
            StackedDRGWinning512MiBV1 | StackedDRGWindow512MiBV1 | StackedDRGWindow512MiBV1P1 => {
                Ok(2)
            }
            Invalid(i) => Err(format!("unsupported proof type: {}", i)),
        }
    }
}
