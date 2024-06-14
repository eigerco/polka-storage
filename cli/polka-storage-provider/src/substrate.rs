use polkadot::runtime_types::{frame_system::AccountInfo, pallet_balances::types::AccountData};
use subxt::{utils::AccountId32, Error, OnlineClient, PolkadotConfig};
use tracing::info;

#[subxt::subxt(runtime_metadata_path = "artifacts/metadata.scale")]
pub mod polkadot {}

// PolkadotConfig or SubstrateConfig will suffice for this example at the moment,
// but PolkadotConfig is a little more correct, having the right `Address` type.
type Config = PolkadotConfig;

/// Polkadot client type alias.
pub type Client = OnlineClient<Config>;

/// Initialize a Polkadot client.
pub async fn init_client(url: impl AsRef<str>) -> Result<Client, Error> {
    let inner = OnlineClient::<Config>::from_url(url).await?;
    info!("Connection with parachain established.");
    Ok(inner)
}

/// Get system balances for the particular account.
pub async fn get_system_balances(
    client: &Client,
    account: &AccountId32,
) -> Result<Option<AccountInfo<u32, AccountData<u128>>>, Error> {
    let storage_query = polkadot::storage().system().account(account);

    let result = client
        .storage()
        .at_latest()
        .await?
        .fetch(&storage_query)
        .await?;

    Ok(result)
}