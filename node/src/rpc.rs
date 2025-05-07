//! A collection of node-specific RPC methods.
//! Substrate provides the `sc-rpc` crate, which defines the core RPC layer
//! used by Substrate nodes. This file extends those RPC definitions with
//! capabilities that are specific to this project's runtime configuration.

#![warn(missing_docs)]

use std::sync::Arc;

use jsonrpsee::core::RpcResult;
use libp2p::Multiaddr;
use polka_storage_runtime::{opaque::Block, AccountId, Balance, Nonce};
use sc_transaction_pool_api::TransactionPool;
use sp_api::ProvideRuntimeApi;
use sp_block_builder::BlockBuilder;
use sp_blockchain::{Error as BlockChainError, HeaderBackend, HeaderMetadata};

use crate::service::p2p::BootstrapConfig;

/// A type representing all RPC extensions.
pub type RpcExtension = jsonrpsee::RpcModule<()>;

/// Full client dependencies
pub struct FullDeps<C, P> {
    /// The client instance to use.
    pub client: Arc<C>,
    /// Transaction pool instance.
    pub pool: Arc<P>,

    /// The P2P bootstrap config.
    pub p2p: Option<BootstrapConfig>,
}

/// Instantiate all RPC extensions.
pub fn create_full<C, P>(
    deps: FullDeps<C, P>,
) -> Result<RpcExtension, Box<dyn std::error::Error + Send + Sync>>
where
    C: ProvideRuntimeApi<Block>
        + HeaderBackend<Block>
        + HeaderMetadata<Block, Error = BlockChainError>
        + Send
        + Sync
        + 'static,
    C::Api: pallet_transaction_payment_rpc::TransactionPaymentRuntimeApi<Block, Balance>,
    C::Api: substrate_frame_rpc_system::AccountNonceApi<Block, AccountId, Nonce>,
    C::Api: BlockBuilder<Block>,
    P: TransactionPool + Sync + Send + 'static,
{
    use pallet_transaction_payment_rpc::{TransactionPayment, TransactionPaymentApiServer};
    use substrate_frame_rpc_system::{System, SystemApiServer};

    let mut module = RpcExtension::new(());
    let FullDeps {
        client, pool, p2p, ..
    } = deps;

    module.merge(System::new(client.clone(), pool).into_rpc())?;
    module.merge(TransactionPayment::new(client).into_rpc())?;
    if let Some(config) = p2p {
        let mut addresses = vec![];
        if let Some(addr) = config.public_tcp_address {
            addresses.push(addr);
        }
        if let Some(addr) = config.public_websocket_address {
            addresses.push(addr);
        }
        addresses.push(config.tcp_address);
        addresses.push(config.websocket_address);
        module.merge(PolkaStorageServices::new(addresses).into_rpc())?;
    }
    Ok(module)
}

#[jsonrpsee::proc_macros::rpc(client, server)]
trait PolkaStorageServicesApi {
    /// Exposes the local listen multiaddresses. It will usually be either `0.0.0.0` or `127.0.0.1`.
    ///
    /// This is used by Delia to resolve the libp2p addresses for the request/response protocols.
    #[method(name = "polkaStorage_getP2pMultiaddrs")]
    async fn get_p2p_multiaddrs(&self) -> RpcResult<Vec<String>>;
}

struct PolkaStorageServices {
    listen_addresses: Vec<Multiaddr>,
}

impl PolkaStorageServices {
    fn new(listen_addresses: Vec<Multiaddr>) -> Self {
        Self { listen_addresses }
    }
}

#[async_trait::async_trait]
impl PolkaStorageServicesApiServer for PolkaStorageServices {
    async fn get_p2p_multiaddrs(&self) -> RpcResult<Vec<String>> {
        return Ok(self
            .listen_addresses
            .iter()
            .map(|addr| addr.to_string())
            .collect());
    }
}
