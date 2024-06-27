use std::{
    fmt::{self, Debug},
    marker::PhantomData,
};

use jsonrpsee::{
    core::{
        client::{BatchResponse, ClientT, Subscription, SubscriptionClientT},
        params::{ArrayParams, BatchRequestBuilder, ObjectParams},
        traits::ToRpcParams,
    },
    http_client::HttpClientBuilder,
    ws_client::WsClientBuilder,
};
use serde::de::DeserializeOwned;
use serde_json::Value;
use thiserror::Error;
use tracing::{debug, instrument};
use url::Url;

use super::{
    methods::RpcRequest,
    version::{ApiVersion, V0},
};

/// Type alias for the V0 client instance
pub type ClientV0 = Client<V0>;

/// Errors that can occur when working with the client
#[derive(Debug, Error)]
pub enum ClientError {
    #[error("Unsupported scheme error: {0}")]
    UnsupportedUrlScheme(String),

    #[error("Invalid parameter type: {0}")]
    InvalidParameter(Value),

    #[error(transparent)]
    Url(#[from] url::ParseError),

    #[error(transparent)]
    JsonRpcClient(#[from] jsonrpsee::core::ClientError),

    #[error(transparent)]
    Json(#[from] serde_json::Error),
}

/// Represents a single connection to the URL server
pub struct Client<Version> {
    url: Url,
    inner: ClientInner,
    _version: PhantomData<Version>,
}

impl<Version> Debug for Client<Version> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("InnerClient")
            .field("url", &self.url)
            .finish_non_exhaustive()
    }
}

impl<Version> Client<Version>
where
    Version: ApiVersion,
{
    pub async fn new(base_url: Url) -> Result<Self, ClientError> {
        let url = base_url.join(Version::version())?;
        let inner = match url.scheme() {
            "ws" | "wss" => ClientInner::Ws(WsClientBuilder::new().build(&url).await?),
            "http" | "https" => ClientInner::Https(HttpClientBuilder::new().build(&url)?),
            it => return Err(ClientError::UnsupportedUrlScheme(it.to_string())),
        };

        Ok(Self {
            url,
            inner,
            _version: PhantomData,
        })
    }

    #[instrument(skip_all, fields(url = %self.url, method = %Request::NAME))]
    pub async fn execute<Request>(&self, request: Request) -> Result<Request::Ok, ClientError>
    where
        Request: RpcRequest<Version>,
        Version: ApiVersion,
    {
        let method_name = Request::NAME;
        let params = serde_json::to_value(request.get_params())?;

        let result = match params {
            Value::Null => self.inner.request(method_name, ArrayParams::new()),
            Value::Array(it) => {
                let mut params = ArrayParams::new();
                for param in it {
                    params.insert(param)?
                }

                self.inner.request(method_name, params)
            }
            Value::Object(it) => {
                let mut params = ObjectParams::new();
                for (name, param) in it {
                    params.insert(&name, param)?
                }

                self.inner.request(method_name, params)
            }
            param @ (Value::Bool(_) | Value::Number(_) | Value::String(_)) => {
                return Err(ClientError::InvalidParameter(param))
            }
        }
        .await;

        debug!(?result, "response received");

        // We cant return result directly because compiler needs some help to
        // understand the types
        Ok(result?)
    }
}

enum ClientInner {
    Ws(jsonrpsee::ws_client::WsClient),
    Https(jsonrpsee::http_client::HttpClient),
}

#[async_trait::async_trait]
impl ClientT for ClientInner {
    async fn notification<Params>(
        &self,
        method: &str,
        params: Params,
    ) -> Result<(), jsonrpsee::core::ClientError>
    where
        Params: ToRpcParams + Send,
    {
        match &self {
            ClientInner::Ws(client) => client.notification(method, params).await,
            ClientInner::Https(client) => client.notification(method, params).await,
        }
    }

    async fn request<R, Params>(
        &self,
        method: &str,
        params: Params,
    ) -> Result<R, jsonrpsee::core::ClientError>
    where
        R: DeserializeOwned,
        Params: ToRpcParams + Send,
    {
        match &self {
            ClientInner::Ws(client) => client.request(method, params).await,
            ClientInner::Https(client) => client.request(method, params).await,
        }
    }

    async fn batch_request<'a, R>(
        &self,
        batch: BatchRequestBuilder<'a>,
    ) -> Result<BatchResponse<'a, R>, jsonrpsee::core::ClientError>
    where
        R: DeserializeOwned + fmt::Debug + 'a,
    {
        match &self {
            ClientInner::Ws(client) => client.batch_request(batch).await,
            ClientInner::Https(client) => client.batch_request(batch).await,
        }
    }
}

#[async_trait::async_trait]
impl SubscriptionClientT for ClientInner {
    async fn subscribe<'a, Notif, Params>(
        &self,
        subscribe_method: &'a str,
        params: Params,
        unsubscribe_method: &'a str,
    ) -> Result<Subscription<Notif>, jsonrpsee::core::ClientError>
    where
        Params: ToRpcParams + Send,
        Notif: DeserializeOwned,
    {
        match &self {
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
    ) -> Result<Subscription<Notif>, jsonrpsee::core::ClientError>
    where
        Notif: DeserializeOwned,
    {
        match &self {
            ClientInner::Ws(it) => it.subscribe_to_method(method).await,
            ClientInner::Https(it) => it.subscribe_to_method(method).await,
        }
    }
}
