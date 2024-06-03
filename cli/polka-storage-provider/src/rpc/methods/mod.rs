use std::sync::Arc;

use jsonrpsee::RpcModule;

use super::RpcServerState;
use crate::rpc::reflect::RpcMethod;

pub mod common;

/// The macro `callback` will be passed in each type that implements
/// [`RpcMethod`].
///
/// All methods should be entered here.
macro_rules! for_each_method {
    ($callback:path) => {
        // common
        $callback!(common::Info);
    };
}

pub fn create_module(state: Arc<RpcServerState>) -> RpcModule<RpcServerState> {
    let mut module = RpcModule::from_arc(state);
    macro_rules! register {
        ($ty:ty) => {
            <$ty>::register(&mut module).unwrap();
        };
    }
    for_each_method!(register);
    module
}
