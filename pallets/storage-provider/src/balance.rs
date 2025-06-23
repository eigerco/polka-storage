use codec::{Decode, Encode, MaxEncodedLen};
use scale_info::TypeInfo;
use sp_core::RuntimeDebug;

/// Stores balances info for both Storage Providers and Storage Users
/// We do not use the ReservableCurrency::reserve mechanism,
/// as the Market works as a liaison between Storage Providers and Storage Clients.
/// Market has its own account on which funds of all parties are stored.
/// It's Market reposibility to manage deposited funds, lock/unlock and pay them out when necessary.
#[derive(Clone, Eq, PartialEq, Encode, Decode, RuntimeDebug, Default, TypeInfo, MaxEncodedLen)]
pub struct BalanceEntry<Balance> {
    /// Amount of Balance that has been deposited for future deals/earned from deals.
    /// It can be withdrawn at any time.
    pub free: Balance,
    /// Amount of Balance that has been staked as Deal Collateral
    /// It's locked to a deal and cannot be withdrawn until the deal ends.
    pub locked: Balance,
}
