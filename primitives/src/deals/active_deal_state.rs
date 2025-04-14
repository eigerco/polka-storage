use codec::{Decode, Encode, MaxEncodedLen};
use scale_info::TypeInfo;
use sp_runtime::RuntimeDebug;

use crate::sector::SectorNumber;

#[derive(Clone, Eq, PartialEq, Encode, Decode, RuntimeDebug, TypeInfo, MaxEncodedLen)]
/// State only related to the activated deal
/// Reference: <https://github.com/filecoin-project/builtin-actors/blob/17ede2b256bc819dc309edf38e031e246a516486/actors/market/src/deal.rs#L138>
pub struct ActiveDealState<BlockNumber> {
    /// Sector in which given piece has been included
    pub sector_number: SectorNumber,

    /// At which block (time) the deal's sector has been activated.
    pub sector_start_block: BlockNumber,

    /// The last block (time) when the deal was updated — i.e. when a deal payment settlement was made.
    ///
    /// In Filecoin this happens under two circumstances:
    /// * Someone starts the payment settlement procedure.
    /// * Cron tick (deprecated) settles legacy deals.
    ///
    /// Sources:
    /// * <https://github.com/filecoin-project/builtin-actors/blob/17ede2b256bc819dc309edf38e031e246a516486/actors/market/src/lib.rs#L985>
    /// * <https://github.com/filecoin-project/builtin-actors/blob/17ede2b256bc819dc309edf38e031e246a516486/actors/market/src/lib.rs#L1315>
    pub last_updated_block: Option<BlockNumber>,

    /// When the deal was last slashed, can be never.
    ///
    /// In Filecoin, slashing can happen in two cases, storage faults and consensus faults,
    /// in our case, we're only concerned about the storage faults, as the consensus is
    /// handled by the collators.
    ///
    /// Slashing is related to three main kinds of penalties:
    /// * Fault Fee — incurred for each day a sector is offline.
    /// * Storage Penalty — incurred when sectors that were not declared as faulty before a WindowPoSt are detected.
    /// * Termination Penalty — incurred when a sector is voluntarily (the miner "gave up on the deal") or
    ///   involuntarily (when a sector is faulty for 42 days in a row) terminated and removed from the network.
    ///
    /// Slashing is applied (i.e. `slash_epoch` is updated) in a single place:
    /// * During [`on_miners_sector_terminate`][1], by termination penalty since the deal was terminated early.
    ///   The deal is first settled — i.e. the storage provider gets paid for the storage time since they last settled the deal —
    ///   then storage provider has their collateral slashed and burned and the client gets their funds unlocked (i.e. refunded).
    ///
    /// However, slashing is performed in other places, it just does not update `slash_epoch` (`slash_block` in our case).
    /// * During [`get_active_deal_or_process_timeout`][2], slashing will happen if the deal has expired
    ///   — i.e. if and when the deal is published but fails to be activated in a given period.
    ///   This function is called in [`cron_tick`][3] and [`settle_deal_payments`][4].
    /// * During [`process_deal_update`][5], if the deal has a `slash_epoch`, any remaining payments will be settled
    ///   and the provider will have its collateral slashed.
    /// * During [`cron_tick`][7], by means of [`get_active_deal_or_process_timeout`][8] and finally [`process_deal_init_timed_out`][9].
    ///
    /// Sources:
    /// * <https://spec.filecoin.io/#section-glossary.storage-fault-slashing>
    /// * <https://spec.filecoin.io/#section-systems.filecoin_mining.sector.lifecycle>
    ///
    /// [1]: https://github.com/filecoin-project/builtin-actors/blob/17ede2b256bc819dc309edf38e031e246a516486/actors/market/src/lib.rs#L852-L853
    /// [2]: https://github.com/filecoin-project/builtin-actors/blob/17ede2b256bc819dc309edf38e031e246a516486/actors/market/src/state.rs#L741-L797
    /// [3]: https://github.com/filecoin-project/builtin-actors/blob/17ede2b256bc819dc309edf38e031e246a516486/actors/market/src/lib.rs#L904-L924
    /// [4]: https://github.com/filecoin-project/builtin-actors/blob/17ede2b256bc819dc309edf38e031e246a516486/actors/market/src/lib.rs#L1240-L1271
    /// [5]: https://github.com/filecoin-project/builtin-actors/blob/17ede2b256bc819dc309edf38e031e246a516486/actors/market/src/state.rs#L886-L912
    /// [6]: https://github.com/filecoin-project/builtin-actors/blob/54236ae89880bf4aa89b0dba6d9060c3fd2aacee/actors/market/src/state.rs#L922-L962
    /// [7]: https://github.com/filecoin-project/builtin-actors/blob/54236ae89880bf4aa89b0dba6d9060c3fd2aacee/actors/market/src/lib.rs#L904-L924
    /// [8]: https://github.com/filecoin-project/builtin-actors/blob/17ede2b256bc819dc309edf38e031e246a516486/actors/market/src/state.rs#L765
    /// [9]: https://github.com/filecoin-project/builtin-actors/blob/17ede2b256bc819dc309edf38e031e246a516486/actors/market/src/state.rs#L964-L997
    pub slash_block: Option<BlockNumber>,
}

impl<BlockNumber> ActiveDealState<BlockNumber> {
    pub fn new(
        sector_number: SectorNumber,
        sector_start_block: BlockNumber,
    ) -> ActiveDealState<BlockNumber> {
        ActiveDealState {
            sector_number,
            sector_start_block,
            last_updated_block: None,
            slash_block: None,
        }
    }
}
