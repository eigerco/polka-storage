use jsonrpsee::{
    core::{
        client::{BatchResponse, ClientT, Subscription, SubscriptionClientT},
        params::BatchRequestBuilder,
        traits::ToRpcParams,
    },
    http_client::HttpClientBuilder,
    ws_client::WsClientBuilder,
};
use serde::de::DeserializeOwned;
use url::Url;

pub enum PolkaStorageRpcClient {
    Ws(jsonrpsee::ws_client::WsClient),
    Https(jsonrpsee::http_client::HttpClient),
}

impl PolkaStorageRpcClient {
    pub async fn new(url: &Url) -> Result<Self, jsonrpsee::core::ClientError> {
        match url.scheme() {
            "ws" | "wss" => Ok(PolkaStorageRpcClient::Ws(
                WsClientBuilder::new().build(url).await?,
            )),
            "http" | "https" => Ok(PolkaStorageRpcClient::Https(
                HttpClientBuilder::new().build(url)?,
            )),
            scheme => Err(jsonrpsee::core::ClientError::Custom(format!(
                "unsupported url scheme: {}",
                scheme
            ))),
        }
    }
}

#[async_trait::async_trait]
impl ClientT for PolkaStorageRpcClient {
    async fn notification<Params>(
        &self,
        method: &str,
        params: Params,
    ) -> Result<(), jsonrpsee::core::ClientError>
    where
        Params: ToRpcParams + Send,
    {
        match &self {
            PolkaStorageRpcClient::Ws(client) => client.notification(method, params).await,
            PolkaStorageRpcClient::Https(client) => client.notification(method, params).await,
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
            PolkaStorageRpcClient::Ws(client) => client.request(method, params).await,
            PolkaStorageRpcClient::Https(client) => client.request(method, params).await,
        }
    }

    async fn batch_request<'a, R>(
        &self,
        batch: BatchRequestBuilder<'a>,
    ) -> Result<BatchResponse<'a, R>, jsonrpsee::core::ClientError>
    where
        R: DeserializeOwned + std::fmt::Debug + 'a,
    {
        match &self {
            PolkaStorageRpcClient::Ws(client) => client.batch_request(batch).await,
            PolkaStorageRpcClient::Https(client) => client.batch_request(batch).await,
        }
    }
}

#[async_trait::async_trait]
impl SubscriptionClientT for PolkaStorageRpcClient {
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
            PolkaStorageRpcClient::Ws(it) => {
                it.subscribe(subscribe_method, params, unsubscribe_method)
                    .await
            }
            PolkaStorageRpcClient::Https(it) => {
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
            PolkaStorageRpcClient::Ws(it) => it.subscribe_to_method(method).await,
            PolkaStorageRpcClient::Https(it) => it.subscribe_to_method(method).await,
        }
    }
}
