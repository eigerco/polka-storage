use std::{
    fmt::{self, Debug},
    marker::PhantomData,
};

use jsonrpsee::{
    core::{
        client::{BatchResponse, ClientT, Subscription, SubscriptionClientT},
        params::{ArrayParams, BatchRequestBuilder, ObjectParams},
        traits::ToRpcParams,
        ClientError,
    },
    http_client::HttpClientBuilder,
    ws_client::WsClientBuilder,
};
use serde::de::DeserializeOwned;
use tokio::sync::OnceCell;
use tracing::{debug, debug_span, Instrument};
use url::Url;

use super::ApiVersion;

pub struct RpcClient {
    base_url: Url,
    v0: OnceCell<Client>,
}

impl RpcClient {
    /// Create a new RPC client with the given base URL.
    pub fn new(base_url: Url) -> Self {
        Self {
            base_url,
            v0: OnceCell::new(),
        }
    }

    /// Call an RPC server with the given request.
    pub async fn call<T: DeserializeOwned + std::fmt::Debug>(
        &self,
        req: Request<T>,
    ) -> Result<T, ClientError> {
        let Request {
            method_name,
            params,
            api_version,
            ..
        } = req;

        let client = self.get_or_init_client(api_version).await?;
        let span = debug_span!("request", method = %method_name, url = %client.url, ?params);

        let work = async {
            let result = match params {
                serde_json::Value::Null => client.request(method_name, ArrayParams::new()),
                serde_json::Value::Array(it) => {
                    let mut params = ArrayParams::new();
                    for param in it {
                        params.insert(param)?
                    }
                    client.request(method_name, params)
                }
                serde_json::Value::Object(it) => {
                    let mut params = ObjectParams::new();
                    for (name, param) in it {
                        params.insert(&name, param)?
                    }
                    client.request(method_name, params)
                }
                prim @ (serde_json::Value::Bool(_)
                | serde_json::Value::Number(_)
                | serde_json::Value::String(_)) => {
                    return Err(ClientError::Custom(format!(
                        "invalid parameter type: `{}`",
                        prim
                    )))
                }
            }
            .await;
            debug!(?result);

            result
        };

        work.instrument(span.or_current()).await
    }

    /// Get or initialize a client for the given API version.
    async fn get_or_init_client(&self, version: ApiVersion) -> Result<&Client, ClientError> {
        match version {
            ApiVersion::V0 => &self.v0,
        }
        .get_or_try_init(|| async {
            let version_part = match version {
                ApiVersion::V0 => "rpc/v0",
            };

            let url = self.base_url.join(version_part).map_err(|it| {
                ClientError::Custom(format!("creating url for endpoint failed: {}", it))
            })?;
            Client::new(url).await
        })
        .await
    }
}

/// Represents a single connection to the URL server
struct Client {
    url: Url,
    specific: ClientInner,
}

impl Debug for Client {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("InnerClient")
            .field("url", &self.url)
            .finish_non_exhaustive()
    }
}

impl Client {
    async fn new(url: Url) -> Result<Self, ClientError> {
        let specific = match url.scheme() {
            "ws" | "wss" => ClientInner::Ws(WsClientBuilder::new().build(&url).await?),
            "http" | "https" => ClientInner::Https(HttpClientBuilder::new().build(&url)?),
            it => {
                return Err(ClientError::Custom(format!(
                    "Unsupported URL scheme: {}",
                    it
                )))
            }
        };

        Ok(Self { url, specific })
    }
}

enum ClientInner {
    Ws(jsonrpsee::ws_client::WsClient),
    Https(jsonrpsee::http_client::HttpClient),
}

#[async_trait::async_trait]
impl ClientT for Client {
    async fn notification<Params>(&self, method: &str, params: Params) -> Result<(), ClientError>
    where
        Params: ToRpcParams + Send,
    {
        match &self.specific {
            ClientInner::Ws(client) => client.notification(method, params).await,
            ClientInner::Https(client) => client.notification(method, params).await,
        }
    }

    async fn request<R, Params>(&self, method: &str, params: Params) -> Result<R, ClientError>
    where
        R: DeserializeOwned,
        Params: ToRpcParams + Send,
    {
        match &self.specific {
            ClientInner::Ws(client) => client.request(method, params).await,
            ClientInner::Https(client) => client.request(method, params).await,
        }
    }

    async fn batch_request<'a, R>(
        &self,
        batch: BatchRequestBuilder<'a>,
    ) -> Result<BatchResponse<'a, R>, ClientError>
    where
        R: DeserializeOwned + fmt::Debug + 'a,
    {
        match &self.specific {
            ClientInner::Ws(client) => client.batch_request(batch).await,
            ClientInner::Https(client) => client.batch_request(batch).await,
        }
    }
}

#[async_trait::async_trait]
impl SubscriptionClientT for Client {
    async fn subscribe<'a, Notif, Params>(
        &self,
        subscribe_method: &'a str,
        params: Params,
        unsubscribe_method: &'a str,
    ) -> Result<Subscription<Notif>, ClientError>
    where
        Params: ToRpcParams + Send,
        Notif: DeserializeOwned,
    {
        match &self.specific {
            ClientInner::Ws(it) => {
                it.subscribe(subscribe_method, params, unsubscribe_method)
                    .await
            }
            ClientInner::Https(it) => {
                it.subscribe(subscribe_method, params, unsubscribe_method)
                    .await
            }
        }
    }

    async fn subscribe_to_method<'a, Notif>(
        &self,
        method: &'a str,
    ) -> Result<Subscription<Notif>, ClientError>
    where
        Notif: DeserializeOwned,
    {
        match &self.specific {
            ClientInner::Ws(it) => it.subscribe_to_method(method).await,
            ClientInner::Https(it) => it.subscribe_to_method(method).await,
        }
    }
}

/// Represents a single RPC request.
#[derive(Debug)]
pub struct Request<T = serde_json::Value> {
    pub method_name: &'static str,
    pub params: serde_json::Value,
    pub result_type: PhantomData<T>,
    pub api_version: ApiVersion,
}

impl<T> ToRpcParams for Request<T> {
    fn to_rpc_params(self) -> Result<Option<Box<serde_json::value::RawValue>>, serde_json::Error> {
        Ok(Some(serde_json::value::to_raw_value(&self.params)?))
    }
}
