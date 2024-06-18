use std::{fmt, fmt::Debug};

use jsonrpsee::{
    core::{
        client::{BatchResponse, ClientT, Subscription, SubscriptionClientT},
        params::BatchRequestBuilder,
        traits::ToRpcParams,
        ClientError,
    },
    http_client::HttpClientBuilder,
    ws_client::WsClientBuilder,
};
use serde::de::DeserializeOwned;
use tokio::sync::OnceCell;
use url::Url;

pub struct RpcClient {
    base_url: Url,
    v0: OnceCell<ClientSpecific>,
}

impl RpcClient {
    pub fn new(base_url: Url) -> Self {
        Self {
            base_url,
            v0: OnceCell::new(),
        }
    }
}

/// Represents a single connection to the URL server
struct InnerClient {
    url: Url,
    specific: ClientSpecific,
}

impl Debug for InnerClient {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("InnerClient")
            .field("url", &self.url)
            .finish_non_exhaustive()
    }
}

impl InnerClient {
    async fn new(url: Url) -> Result<Self, ClientError> {
        let specific = match url.scheme() {
            "ws" | "wss" => ClientSpecific::Ws(WsClientBuilder::new().build(&url).await?),
            "http" | "https" => ClientSpecific::Https(HttpClientBuilder::new().build(&url)?),
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

enum ClientSpecific {
    Ws(jsonrpsee::ws_client::WsClient),
    Https(jsonrpsee::http_client::HttpClient),
}

#[async_trait::async_trait]
impl ClientT for InnerClient {
    async fn notification<Params>(&self, method: &str, params: Params) -> Result<(), ClientError>
    where
        Params: ToRpcParams + Send,
    {
        match &self.specific {
            ClientSpecific::Ws(client) => client.notification(method, params).await,
            ClientSpecific::Https(client) => client.notification(method, params).await,
        }
    }

    async fn request<R, Params>(&self, method: &str, params: Params) -> Result<R, ClientError>
    where
        R: DeserializeOwned,
        Params: ToRpcParams + Send,
    {
        match &self.specific {
            ClientSpecific::Ws(client) => client.request(method, params).await,
            ClientSpecific::Https(client) => client.request(method, params).await,
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
            ClientSpecific::Ws(client) => client.batch_request(batch).await,
            ClientSpecific::Https(client) => client.batch_request(batch).await,
        }
    }
}

#[async_trait::async_trait]
impl SubscriptionClientT for InnerClient {
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
            ClientSpecific::Ws(it) => {
                it.subscribe(subscribe_method, params, unsubscribe_method)
                    .await
            }
            ClientSpecific::Https(it) => {
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
            ClientSpecific::Ws(it) => it.subscribe_to_method(method).await,
            ClientSpecific::Https(it) => it.subscribe_to_method(method).await,
        }
    }
}
