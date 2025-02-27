use std::{any::type_name, time::Duration};

use blockstore::Blockstore;
use mater::blockstore::ReadOnlyBlockstore;
use rand::{thread_rng, Rng};
use tokio::io::{AsyncRead, AsyncSeek};

/// Adds random delays on the `get` calls.
#[allow(dead_code)]
struct ChaosReadOnlyStore<R>(ReadOnlyBlockstore<R>);

impl<R> Blockstore for ChaosReadOnlyStore<R>
where
    R: AsyncRead + AsyncSeek + Unpin + blockstore::cond_send::CondSync,
{
    fn get<const S: usize>(
        &self,
        cid: &cid::CidGeneric<S>,
    ) -> impl futures::Future<Output = blockstore::Result<Option<Vec<u8>>>>
           + blockstore::cond_send::CondSend {
        async {
            if rand::thread_rng().gen_bool(0.5) {
                let dur = Duration::from_millis(thread_rng().gen_range(250..=1000));
                tracing::info!("sleeping for {}", dur.as_millis());
                tokio::time::sleep(dur).await;
            }
            self.0.get(cid).await
        }
    }

    fn put_keyed<const S: usize>(
        &self,
        _: &cid::CidGeneric<S>,
        _: &[u8],
    ) -> impl futures::Future<Output = blockstore::Result<()>> + blockstore::cond_send::CondSend
    {
        async {
            Err(blockstore::Error::FatalDatabaseError(format!(
                "{} is read-only",
                type_name::<Self>()
            )))
        }
    }

    fn remove<const S: usize>(
        &self,
        _: &cid::CidGeneric<S>,
    ) -> impl futures::Future<Output = blockstore::Result<()>> + blockstore::cond_send::CondSend
    {
        async {
            Err(blockstore::Error::FatalDatabaseError(format!(
                "{} is read-only",
                type_name::<Self>()
            )))
        }
    }

    fn close(
        self,
    ) -> impl futures::Future<Output = blockstore::Result<()>> + blockstore::cond_send::CondSend
    {
        async { Ok(()) }
    }
}
