use std::sync::Arc;

use jsonrpsee::RpcModule;

use super::{RpcMethod, RpcServerState};

pub mod common;
pub mod wallet;

pub fn create_module(state: Arc<RpcServerState>) -> RpcModule<RpcServerState> {
    let mut module = RpcModule::from_arc(state);

    common::Info::register_async(&mut module);
    wallet::WalletBalance::register_async(&mut module);

    module
}
