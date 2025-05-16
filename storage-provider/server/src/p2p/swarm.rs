use std::{
    io::Cursor,
    path::{Path, PathBuf},
    time::Duration,
};

use futures::future::Either;
use libp2p::{
    core::{muxing::StreamMuxerBox, upgrade::Version},
    dns,
    identity::Keypair,
    noise,
    swarm::NetworkBehaviour,
    tcp,
    websocket::{self},
    yamux, Swarm, SwarmBuilder, Transport,
};
use rustls_pki_types::{CertificateDer, PrivateKeyDer};
use tokio::fs;

use super::P2pError;

async fn read_tls_key(path: impl AsRef<Path>) -> Result<PrivateKeyDer<'static>, P2pError> {
    let path = path.as_ref();

    // TODO: read key in a preallocated memory and zero it after use
    let data = fs::read(&path)
        .await
        .map_err(|e| P2pError::TlsInit(format!("{}: {e}", path.display())))?;

    let mut data = Cursor::new(data);

    rustls_pemfile::private_key(&mut data)
        .map_err(|e| P2pError::TlsInit(format!("{}: {e}", path.display())))?
        .ok_or_else(|| P2pError::TlsInit(format!("{}: Key not found in file", path.display())))
}

async fn read_tls_certs(path: impl AsRef<Path>) -> Result<Vec<CertificateDer<'static>>, P2pError> {
    let path = path.as_ref();

    let data = fs::read(path)
        .await
        .map_err(|e| P2pError::TlsInit(format!("{}: {e}", path.display())))?;

    let mut data = Cursor::new(data);
    let certs = rustls_pemfile::certs(&mut data)
        .collect::<Result<Vec<_>, std::io::Error>>()
        .map_err(|e| P2pError::TlsInit(format!("{}: {e}", path.display())))?;

    if certs.is_empty() {
        let e = format!("{}: Certificate not found in file", path.display());
        Err(P2pError::TlsInit(e))
    } else {
        Ok(certs)
    }
}

pub(crate) async fn new_swarm<B>(
    keypair: Keypair,
    behaviour: B,
    tls_key_file: PathBuf,
    tls_cert_file: PathBuf,
) -> Result<Swarm<B>, P2pError>
where
    B: NetworkBehaviour,
{
    let tls_key = match read_tls_key(tls_key_file).await {
        Ok(tls) => Some(tls),
        Err(_) => None,
    };

    let tls_certs = match read_tls_certs(tls_cert_file).await {
        Ok(tls) => Some(tls),
        Err(_) => None,
    };

    // We do not use system's DNS because libp2p caches system DNS
    // servers when `Swarm` get constructed, and doesn't update them
    // later. This can be a problem, if device roams between networks
    // (and old DNS addresses may not be reachable from the new network).
    //
    // Similarly, if node is started when there's no Internet connection,
    // it won't use the DNS servers offered when Internet connectivity
    // is restored. Instead we per-define globally-accessible public DNS servers.
    let dns_config = dns::ResolverConfig::cloudflare();

    let noise_config =
        noise::Config::new(&keypair).map_err(|e| P2pError::InitNoise(e.to_string()))?;

    let wss_transport = {
        let config = if let (Some(key), Some(certs)) = (tls_key, tls_certs) {
            let key = websocket::tls::PrivateKey::new(key.secret_der().to_vec());
            let certs = certs
                .iter()
                .map(|cert| websocket::tls::Certificate::new(cert.to_vec()));

            websocket::tls::Config::new(key, certs)
                .map_err(|e| P2pError::TlsInit(format!("server config: {e}")))?
        } else {
            websocket::tls::Config::client()
        };

        let mut wss_transport = websocket::WsConfig::new(dns::tokio::Transport::custom(
            tcp::tokio::Transport::new(tcp::Config::default()),
            dns_config.clone(),
            dns::ResolverOpts::default(),
        ));

        wss_transport.set_tls_config(config);

        wss_transport
            .upgrade(Version::V1Lazy)
            .authenticate(noise_config.clone())
            .multiplex(yamux::Config::default())
    };

    let tcp_transport = tcp::tokio::Transport::new(tcp::Config::default())
        .upgrade(Version::V1Lazy)
        .authenticate(noise_config)
        .multiplex(yamux::Config::default());

    let transport = wss_transport
        .or_transport(tcp_transport)
        .map(|either, _| match either {
            Either::Left((peer_id, conn)) => (peer_id, StreamMuxerBox::new(conn)),
            Either::Right((peer_id, conn)) => (peer_id, StreamMuxerBox::new(conn)),
        })
        .boxed();

    Ok(SwarmBuilder::with_existing_identity(keypair)
        .with_tokio()
        .with_other_transport(|_| transport)
        .map_err(|e| P2pError::TlsInit(e.to_string()))?
        .with_behaviour(|_| behaviour)
        .expect("Moving behaviour doesn't fail")
        .with_swarm_config(|config| config.with_idle_connection_timeout(Duration::from_secs(10)))
        .build())
}
