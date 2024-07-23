use std::sync::Arc;

use blockstore::Blockstore;
use celestia_tendermint::Time;
use celestia_types::ExtendedHeader;
use cid::Cid;
use instant::Duration;
use tokio::select;
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, info, warn};

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

        Ok( Pruner { cancellation_token })
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
    event_pub: EventPublisher,
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
            event_pub: args.event_pub,
            store: args.store,
            blockstore: args.blockstore,
        }
    }

    async fn run(&mut self) -> Result<()> {
        // TODO: about this error handling...
        //self.prune_old_headers().await?;

        let estimated_block_time = Duration::from_secs(BLOCK_PRODUCTION_TIME_ESTIMATE_SECS);
        loop { 
            self.try_prune_tail().await?;

            select! {
                _ = self.cancellation_token.cancelled() => break,
                _ = sleep(estimated_block_time) => ()
            }
        }

        debug!("Pruner stopped");
        Ok(())
    }

    async fn try_prune_tail(&self) -> Result<()> {
        let pruning_window_end = Time::now().checked_sub(PRUNING_WINDOW).unwrap_or_else(|| {
            warn!("underflow when computing pruning window start, defaulting to unix epoch");
            Time::unix_epoch()
        });

        loop {
            let Some((tail_header, cids)) = self.get_current_tail().await? else {
                // empty store == nothing to prune
                return Ok(());
            };



            if tail_header.time() < pruning_window_end {
                for cid in cids {
                    self.blockstore.remove(&cid).await?;
                }
                self.store.remove_tail(tail_header.height().value()).await?;
                continue; // re-check the new tail
            }
        }
    }

    async fn get_current_tail(&self) -> Result<Option<(ExtendedHeader, Vec<Cid>)>> {
        let Some(current_tail_height) = self.store.get_stored_header_ranges().await?.tail() else {
            // empty store == nothing to prune
            return Ok(None);
        };

        let header = self.store.get_by_height(current_tail_height).await?;

        let metadata = self.store.get_sampling_metadata(header.height().value()).await?;

        Ok(Some((header, metadata.map(|m| m.cids).unwrap_or_default())))
    }

    /*
    // TODO: Name
    async fn prune_old_headers(&self) -> Result<()> {
        let Some(current_tail_height) = self.store.get_stored_header_ranges().await?.tail() else {
            // empty store == nothing to prune
            return Ok(());
        };

        // TODO: different error handling ? 
        let current_tail = self.store.get_by_height(current_tail_height).await? ;

        let Some(pruning_window_end) = Time::now().checked_sub(PRUNING_WINDOW) else {
            return Err(PrunerError::TimeOutOfRange);
        };

        let mut tail_estimate = 0; // not a valid header height
        let mut new_tail_estimate = estimate_header_height_at_time(&current_tail, pruning_window_end);

        // we should get the same estimated height once it's within 12sec of pruning window
        while tail_estimate != new_tail_estimate {
            let new_tail = self.store.get_by_height(tail_estimate).await?;

            tail_estimate = new_tail_estimate;
            new_tail_estimate = estimate_header_height_at_time(&new_tail, pruning_window_end);
            debug!("estimate: {tail_estimate}");
        }

        info!("found pruning height for old headers: {tail_estimate}");

        self.store.remove_tail(tail_estimate).await?;

        Ok(())
    }
    */
}

/// Given a reference_header, estimate height of the block produced at provided time, assuming 12s
/// per block. Result needs to be verified against real data in the store and should get more
/// accurate the closer the reference_header is to the provided time.
// TODO: prove this converges?
fn estimate_header_height_at_time(reference_header: &ExtendedHeader, time: Time) -> u64 {
    let reference_header_after = reference_header.time().after(time);

    let time_delta = if reference_header_after {
        reference_header.time().duration_since(time)
    } else {
        time.duration_since(reference_header.time())
    }
    .expect("time between headers should fit Duration");

    println!("time delta: {time_delta:?}");

    let estimated_height_delta = time_delta
        .as_secs()
        .div_ceil(BLOCK_PRODUCTION_TIME_ESTIMATE_SECS);
    println!("height delta: {estimated_height_delta:?}");

    if reference_header_after {
        reference_header
            .height()
            .value()
            .saturating_sub(estimated_height_delta)
    } else {
        reference_header
            .height()
            .value()
            .saturating_add(estimated_height_delta)
    }
}

