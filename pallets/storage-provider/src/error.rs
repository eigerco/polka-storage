use codec::{Decode, Encode};
use frame_support::{pallet_prelude::RuntimeDebug, PalletError};
use scale_info::TypeInfo;

#[derive(Decode, Encode, PalletError, TypeInfo, RuntimeDebug)]
pub enum GeneralPalletError {
    /// Partition error module types
    /// Emitted when adding sectors fails
    PartitionErrorFailedToAddSector,
    /// Emitted when trying to add a sector number that has already been used in this partition.
    PartitionErrorDuplicateSectorNumber,
    /// Emitted when adding faults fails
    PartitionErrorFailedToAddFaults,
    /// Emitted when removing recovering sectors fails
    PartitionErrorFailedToRemoveRecoveries,

    /// Deadline error module types
    /// Emitted when the passed in deadline index supplied for `submit_windowed_post` is out of range.
    DeadlineErrorDeadlineIndexOutOfRange,
    /// Emitted when a trying to get a deadline index but fails because that index does not exist.
    DeadlineErrorDeadlineNotFound,
    /// Emitted when constructing `DeadlineInfo` fails.
    DeadlineErrorCouldNotConstructDeadlineInfo,
    /// Emitted when a proof is submitted for a partition that is already proven.
    DeadlineErrorPartitionAlreadyProven,
    /// Emitted when trying to retrieve a partition that does not exit.
    DeadlineErrorPartitionNotFound,
    /// Emitted when trying to update proven partitions fails.
    DeadlineErrorProofUpdateFailed,
    /// Emitted when max partition for a given deadline have been reached.
    DeadlineErrorMaxPartitionsReached,
    /// Emitted when trying to add sectors to a deadline fails.
    DeadlineErrorCouldNotAddSectors,
    /// Emitted when trying to use sectors which haven't been prove committed yet.
    DeadlineErrorSectorsNotFound,
    /// Emitted when trying to recover non-faulty sectors,
    DeadlineErrorSectorsNotFaulty,
    /// Emitted when assigning sectors to deadlines fails.
    DeadlineErrorCouldNotAssignSectorsToDeadlines,
    /// Emitted when trying to update fault expirations fails
    DeadlineErrorFailedToUpdateFaultExpiration,

    /// StorageProvider module error types
    /// Happens when an SP tries to pre-commit more sectors than SECTOR_MAX.
    StorageProviderErrorMaxPreCommittedSectorExceeded,
    /// Happens when trying to access a sector that does not exist.
    StorageProviderErrorSectorNotFound,
    /// Happens when a sector number is already in use.
    StorageProviderErrorSectorNumberInUse,

    /// SectorMap module error types
    /// Emitted when trying to insert sector(s) fails.
    SectorMapErrorFailedToInsertSector,
    /// Emitted when trying to insert partition fails.
    SectorMapErrorFailedToInsertPartition,

    /// ExpirationQueue module error types
    /// Expiration set not found
    ExpirationQueueErrorExpirationSetNotFound,
    /// Sector not found in expiration set
    ExpirationQueueErrorSectorNotFound,
    /// Insertion failed
    ExpirationQueueErrorInsertionFailed,
}
