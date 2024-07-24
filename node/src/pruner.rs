use std::sync::Arc;

use blockstore::Blockstore;
use celestia_tendermint::Time;
use celestia_types::ExtendedHeader;
use cid::Cid;
use instant::Duration;
use tokio::select;
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, warn};

use crate::events::{EventPublisher, NodeEvent};
use crate::executor::{sleep, spawn};
use crate::p2p::P2pError;
use crate::store::{Store, StoreError};
use crate::syncer::SYNCING_WINDOW;

const BLOCK_PRODUCTION_TIME_ESTIMATE_SECS: u64 = 12;
// 1 hour behind syncing window
const PRUNING_WINDOW: Duration = SYNCING_WINDOW.saturating_add(Duration::from_secs(60 * 60));

type Result<T, E = PrunerError> = std::result::Result<T, E>;

/// Representation of all the errors that can occur when interacting with the [`Pruner`].
#[derive(Debug, thiserror::Error)]
pub enum PrunerError {
    /// An error propagated from the [`P2p`] module.
    #[error(transparent)]
    P2p(#[from] P2pError),

    /// An error propagated from the [`Store`] module.
    #[error(transparent)]
    Store(#[from] StoreError),

    #[error(transparent)]
    Blockstore(#[from] blockstore::Error),
}

pub struct Pruner {
    cancellation_token: CancellationToken,
}

pub struct PrunerArgs<S, B>
where
    S: Store,
    B: Blockstore,
{
    /// Handler for the peer to peer messaging.
    //pub p2p: Arc<P2p>,
    /// Headers storage.
    pub store: Arc<S>,
    /// Block storage.
    pub blockstore: Arc<B>,
    /// Event publisher.
    pub event_pub: EventPublisher,
}

impl Pruner {
    pub fn start<S, B>(args: PrunerArgs<S, B>) -> Result<Self>
    where
        S: Store + 'static,
        B: Blockstore + 'static,
    {
        let cancellation_token = CancellationToken::new();
        let event_pub = args.event_pub.clone();

        let mut worker = Worker::new(args, cancellation_token.child_token());

        spawn(async move {
            if let Err(e) = worker.run().await {
                error!("Pruner stopped because of a fatal error: {e}");

                event_pub.send(NodeEvent::FatalPrunerError {
                    error: e.to_string(),
                });
            }
        });

        Ok(Pruner { cancellation_token })
    }

    pub fn stop(&self) {
        self.cancellation_token.cancel();
    }
}

impl Drop for Pruner {
    fn drop(&mut self) {
        self.cancellation_token.cancel();
    }
}

struct Worker<S, B>
where
    S: Store + 'static,
    B: Blockstore + 'static,
{
    cancellation_token: CancellationToken,
    _event_pub: EventPublisher, // TODO: send events on pruning
    store: Arc<S>,
    blockstore: Arc<B>,
}

impl<S, B> Worker<S, B>
where
    S: Store,
    B: Blockstore,
{
    fn new(args: PrunerArgs<S, B>, cancellation_token: CancellationToken) -> Self {
        Worker {
            cancellation_token,
            _event_pub: args.event_pub,
            store: args.store,
            blockstore: args.blockstore,
        }
    }

    async fn run(&mut self) -> Result<()> {
        let estimated_block_time = Duration::from_secs(BLOCK_PRODUCTION_TIME_ESTIMATE_SECS);
        loop {
            self.remove_headers_outside_pruning_window().await?;

            select! {
                _ = self.cancellation_token.cancelled() => break,
                _ = sleep(estimated_block_time) => ()
            }
        }

        debug!("Pruner stopped");
        Ok(())
    }

    async fn remove_headers_outside_pruning_window(&self) -> Result<()> {
        let pruning_window_end = Time::now().checked_sub(PRUNING_WINDOW).unwrap_or_else(|| {
            warn!("underflow when computing pruning window start, defaulting to unix epoch");
            Time::unix_epoch()
        });

        loop {
            let Some((tail_header, cids)) = self.get_current_tail_header().await? else {
                // empty store == nothing to prune
                return Ok(());
            };

            if tail_header.time() < pruning_window_end {
                for cid in cids {
                    self.blockstore.remove(&cid).await?;
                }
                let removed = self.store.remove_last().await?;
                debug_assert_eq!(tail_header.height().value(), removed);
                continue; // re-check the new tail
            }
        }
    }

    async fn get_current_tail_header(&self) -> Result<Option<(ExtendedHeader, Vec<Cid>)>> {
        let Some(current_tail_height) = self.store.get_stored_header_ranges().await?.tail() else {
            // empty store == nothing to prune
            return Ok(None);
        };

        let header = self.store.get_by_height(current_tail_height).await?;
        let metadata = self
            .store
            .get_sampling_metadata(header.height().value())
            .await?;

        Ok(Some((header, metadata.map(|m| m.cids).unwrap_or_default())))
    }
}
