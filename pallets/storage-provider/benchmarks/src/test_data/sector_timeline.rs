extern crate alloc;

use alloc::{format, string::String, vec::Vec};

use codec::Encode;
use primitives::proofs::assign_proving_period_offset;

use crate::test_data::{
    absolute_block_number::Absolute, deal_timeline::DealTimeline, relative_block_number::Relative,
};

#[derive(Debug, Clone)]
pub struct SectorTimeline<BlockNumber, AccountId> {
    storage_provider_account_id: AccountId,
    register_storage_provider: Absolute<BlockNumber>,
    publish_storage_deals: Absolute<BlockNumber>,
    pre_commit_sectors: Absolute<BlockNumber>,
    deals: Vec<DealTimeline<BlockNumber>>,

    period_deadlines: u32,
    proving_period: Relative<BlockNumber>,
    challenge_window: Relative<BlockNumber>,
    pre_commit_challenge_delay: Relative<BlockNumber>,
    challenge_lookback: Relative<BlockNumber>,
}

impl<BlockNumber, AccountId> SectorTimeline<BlockNumber, AccountId>
where
    AccountId: Encode,
    BlockNumber: sp_runtime::traits::BlockNumber,
{
    pub fn new(
        storage_provider_account_id: AccountId,
        register_storage_provider: Absolute<BlockNumber>,
        publish_storage_deals: Absolute<BlockNumber>,
        pre_commit_sectors: Absolute<BlockNumber>,
        deals: Vec<DealTimeline<BlockNumber>>,

        period_deadlines: u32,
        proving_period: Relative<BlockNumber>,
        challenge_window: Relative<BlockNumber>,
        pre_commit_challenge_delay: Relative<BlockNumber>,
        challenge_lookback: Relative<BlockNumber>,
    ) -> Result<Self, String> {
        if register_storage_provider >= publish_storage_deals {
            return Err(format!(
                "register_storage_provider ({:?}) must be less than publish_storage_deals ({:?})",
                register_storage_provider, publish_storage_deals
            ));
        }
        if publish_storage_deals >= pre_commit_sectors {
            return Err(format!(
                "publish_storage_deals ({:?}) must be less than pre_commit_sectors ({:?})",
                publish_storage_deals, pre_commit_sectors
            ));
        }
        let calculated_period = challenge_window * period_deadlines;
        if calculated_period.0 != proving_period.0 {
            return Err(format!(
                "calculated period ({:?}) must be equal to proving period ({:?})",
                calculated_period, proving_period
            ));
        }
        let t = Self {
            storage_provider_account_id,
            register_storage_provider,
            publish_storage_deals,
            pre_commit_sectors,
            deals,

            period_deadlines,
            proving_period,
            challenge_window,
            pre_commit_challenge_delay,
            challenge_lookback,
        };

        if t.proving_period_start_initial() >= t.seal_randomness_height() {
            return Err(format!(
                "proving period start ({:?}) must be less than seal randomness height ({:?}), try moving the pre_commit_sectors block forward",
                t.proving_period_start_initial().0,
                t.seal_randomness_height().0
            ));
        }
        if t.deadline_start() < t.proving_period_start_initial() {
            return Err(format!(
                "deadline start ({:?}) must be greater than or equal to the initial proving period start ({:?}), try moving the deal start block forward",
                t.deadline_start().0,
                t.proving_period_start_initial().0
            ));
        }

        for deal in &t.deals {
            if deal.start() < t.prove_commit_sectors() {
                return Err(format!(
                    "deal start ({:?}) must be greater than or equal to the initial proving period start ({:?}), try moving the deal start block forward",
                    deal.start().0,
                    t.prove_commit_sectors().0
                ));
            }
        }

        if t.deadline_challenge_block() < t.prove_commit_sectors() {
            return Err(format!(
                "deadline start ({:?}) must be greater than or equal to the initial proving period start ({:?}), try moving the deal start block forward",
                t.deadline_start().0,
                t.proving_period_start_initial().0
            ));
        }

        Ok(t)
    }

    pub fn storage_provider_account_id(&self) -> &AccountId {
        &self.storage_provider_account_id
    }

    pub fn register_storage_provider(&self) -> Absolute<BlockNumber> {
        self.register_storage_provider
    }

    pub fn publish_storage_deals(&self) -> Absolute<BlockNumber> {
        self.publish_storage_deals
    }

    pub fn proving_period(&self) -> Relative<BlockNumber> {
        self.proving_period
    }

    pub fn proving_period_offset(&self) -> Relative<BlockNumber> {
        Relative::from(assign_proving_period_offset::<AccountId, BlockNumber>(
            &self.storage_provider_account_id,
            self.register_storage_provider().0,
            self.proving_period().0,
        ))
    }

    pub fn proving_period_start_initial(&self) -> Absolute<BlockNumber> {
        let global_proving_index = self.register_storage_provider() / self.proving_period();
        let global_proving_start =
            Absolute::zero() + self.proving_period() * (global_proving_index + 1);
        global_proving_start + self.proving_period_offset()
    }

    pub fn challenge_window(&self) -> Relative<BlockNumber> {
        self.challenge_window
    }

    pub fn deals(&self) -> &Vec<DealTimeline<BlockNumber>> {
        &self.deals
    }

    pub fn period_deadlines(&self) -> u32 {
        self.period_deadlines
    }

    pub fn seal_randomness_height(&self) -> Absolute<BlockNumber> {
        // TODO(@Jinxit,28/04/2025): Currently the seal randomness height is only validated to be before
        // the pre-commit sectors, not that it is recent enough, so the 7 here is completely arbitrary.
        self.pre_commit_sectors() - Relative::from(BlockNumber::from(7u32))
    }

    pub fn pre_commit_sectors(&self) -> Absolute<BlockNumber> {
        self.pre_commit_sectors
    }

    pub fn sector_expiration(&self) -> Absolute<BlockNumber> {
        self.deals
            .iter()
            .map(|deal| deal.end())
            .max()
            .expect("at least one deal")
    }

    pub fn pre_commit_challenge_delay(&self) -> Relative<BlockNumber> {
        self.pre_commit_challenge_delay
    }

    pub fn interactive_block_number(&self) -> Absolute<BlockNumber> {
        self.pre_commit_sectors() + self.pre_commit_challenge_delay()
    }

    pub fn prove_commit_sectors(&self) -> Absolute<BlockNumber> {
        // Must be after the interactive block number, but there is no strict delay.
        self.interactive_block_number() + Relative::one()
    }

    pub fn deadline_index(&self) -> u32 {
        let blocks_since_start = self.deadline_start() - self.proving_period_start_initial();
        let periods_since_start = blocks_since_start.div_ceil(self.proving_period());
        periods_since_start % self.period_deadlines
    }

    pub fn deadline_challenge_block(&self) -> Absolute<BlockNumber> {
        self.deadline_start() - self.challenge_lookback()
    }

    pub fn challenge_lookback(&self) -> Relative<BlockNumber> {
        self.challenge_lookback
    }

    pub fn deadline_start(&self) -> Absolute<BlockNumber> {
        let last_deal_start = self
            .deals
            .iter()
            .map(|deal| deal.start())
            .max()
            .expect("at least one deal");

        let start = last_deal_start + self.challenge_lookback();
        let next = start + self.challenge_window();
        let mut current = self.proving_period_start_initial();
        while current < next {
            current = current + self.proving_period();
        }
        current
    }

    pub fn submit_windowed_post(&self) -> Absolute<BlockNumber> {
        // Must be after the deadline, but there is no strict delay.
        self.deadline_start() + Relative::one()
    }

    pub fn deadline_close(&self) -> Absolute<BlockNumber> {
        self.deadline_start() + self.challenge_window()
    }
}
