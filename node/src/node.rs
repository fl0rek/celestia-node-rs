//! High-level integration of [`P2p`], [`Store`], [`Syncer`].
//!
//! [`P2p`]: crate::p2p::P2p
//! [`Store`]: crate::store::Store
//! [`Syncer`]: crate::syncer::Syncer

use std::ops::RangeBounds;
use std::sync::Arc;

use celestia_types::hash::Hash;
use celestia_types::ExtendedHeader;
use libp2p::identity::Keypair;
use libp2p::swarm::NetworkInfo;
use libp2p::{Multiaddr, PeerId};

use crate::p2p::{P2p, P2pArgs, P2pError};
use crate::peer_tracker::PeerTrackerInfo;
use crate::store::{Store, StoreError};
use crate::syncer::{Syncer, SyncerArgs, SyncerError, SyncingInfo};

use crate::p2p::Cid;

type Result<T, E = NodeError> = std::result::Result<T, E>;

/// Representation of all the errors that can occur when interacting with the [`Node`].
#[derive(Debug, thiserror::Error)]
pub enum NodeError {
    /// An error propagated from the [`P2p`] module.
    #[error(transparent)]
    P2p(#[from] P2pError),

    /// An error propagated from the [`Syncer`] module.
    #[error(transparent)]
    Syncer(#[from] SyncerError),

    /// An error propagated from the [`Store`] module.
    #[error(transparent)]
    Store(#[from] StoreError),
}

/// Node conifguration.
pub struct NodeConfig<S>
where
    S: Store + 'static,
{
    /// An id of the network to connect to.
    pub network_id: String,
    /// The hash of the genesis block in network.
    pub genesis_hash: Option<Hash>,
    /// The keypair to be used as [`Node`]s identity.
    pub p2p_local_keypair: Keypair,
    /// List of bootstrap nodes to connect to and trust.
    pub p2p_bootnodes: Vec<Multiaddr>,
    /// List of the addresses where [`Node`] will listen for incoming connections.
    pub p2p_listen_on: Vec<Multiaddr>,
    /// The store for headers.
    pub store: S,
}

/// Celestia node.
pub struct Node<S>
where
    S: Store + 'static,
{
    p2p: Arc<P2p<S>>,
    store: Arc<S>,
    syncer: Arc<Syncer<S>>,
}

impl<S> Node<S>
where
    S: Store,
{
    /// Creates and starts a new celestia node with a given config.
    pub async fn new(config: NodeConfig<S>) -> Result<Self> {
        let store = Arc::new(config.store);

        let p2p = Arc::new(P2p::start(P2pArgs {
            network_id: config.network_id,
            local_keypair: config.p2p_local_keypair,
            bootnodes: config.p2p_bootnodes,
            listen_on: config.p2p_listen_on,
            store: store.clone(),
        })?);

        let syncer = Arc::new(Syncer::start(SyncerArgs {
            genesis_hash: config.genesis_hash,
            store: store.clone(),
            p2p: p2p.clone(),
        })?);

        Ok(Node { p2p, store, syncer })
    }

    /// Get node's local peer ID.
    pub fn local_peer_id(&self) -> &PeerId {
        self.p2p.local_peer_id()
    }

    /// Get current [`PeerTracker`] info.
    ///
    /// [`PeerTracker`]: crate::peer_tracker::PeerTracker
    pub fn peer_tracker_info(&self) -> PeerTrackerInfo {
        self.p2p.peer_tracker_info().clone()
    }

    /// Wait until the node is connected to at least 1 peer.
    pub async fn wait_connected(&self) -> Result<()> {
        Ok(self.p2p.wait_connected().await?)
    }

    /// Wait until the node is connected to at least 1 trusted peer.
    pub async fn wait_connected_trusted(&self) -> Result<()> {
        Ok(self.p2p.wait_connected_trusted().await?)
    }

    /// Get current network info.
    pub async fn network_info(&self) -> Result<NetworkInfo> {
        Ok(self.p2p.network_info().await?)
    }

    /// Get all the multiaddresses on which the node listens.
    pub async fn listeners(&self) -> Result<Vec<Multiaddr>> {
        Ok(self.p2p.listeners().await?)
    }

    /// Get all the peers that node is connected to.
    pub async fn connected_peers(&self) -> Result<Vec<PeerId>> {
        Ok(self.p2p.connected_peers().await?)
    }

    /// Trust or untrust the peer with a given ID.
    pub async fn set_peer_trust(&self, peer_id: PeerId, is_trusted: bool) -> Result<()> {
        Ok(self.p2p.set_peer_trust(peer_id, is_trusted).await?)
    }

    /// Request the head header from the network.
    pub async fn request_head_header(&self) -> Result<ExtendedHeader> {
        Ok(self.p2p.get_head_header().await?)
    }

    /// Request a header for the block with a given hash from the network.
    pub async fn request_header_by_hash(&self, hash: &Hash) -> Result<ExtendedHeader> {
        Ok(self.p2p.get_header(*hash).await?)
    }

    /// Request a header for the block with a given height from the network.
    pub async fn request_header_by_height(&self, hash: u64) -> Result<ExtendedHeader> {
        Ok(self.p2p.get_header_by_height(hash).await?)
    }

    /// Request headers in range (from, from + amount] from the network.
    ///
    /// The headers will be verified with the `from` header.
    pub async fn request_verified_headers(
        &self,
        from: &ExtendedHeader,
        amount: u64,
    ) -> Result<Vec<ExtendedHeader>> {
        Ok(self.p2p.get_verified_headers_range(from, amount).await?)
    }

    /// Get current header syncing info.
    pub async fn syncer_info(&self) -> Result<SyncingInfo> {
        Ok(self.syncer.info().await?)
    }

    /// Get the latest header announced in the network.
    pub fn get_network_head_header(&self) -> Option<ExtendedHeader> {
        self.p2p.header_sub_watcher().borrow().clone()
    }

    /// Get the latest locally synced header.
    pub async fn get_local_head_header(&self) -> Result<ExtendedHeader> {
        Ok(self.store.get_head().await?)
    }

    /// Get a synced header for the block with a given hash.
    pub async fn get_header_by_hash(&self, hash: &Hash) -> Result<ExtendedHeader> {
        Ok(self.store.get_by_hash(hash).await?)
    }

    /// Get a synced header for the block with a given height.
    pub async fn get_header_by_height(&self, height: u64) -> Result<ExtendedHeader> {
        Ok(self.store.get_by_height(height).await?)
    }

    pub async fn mingle(&self, cid: Cid) -> Result<()> {
        Ok(self.p2p.mingle(cid).await?)
    }

    /// Get synced headers from the given heights range.
    ///
    /// If start of the range is unbounded, the first returned header will be of height 1.
    /// If end of the range is unbounded, the last returned header will be the last header in the
    /// store.
    ///
    /// # Errors
    ///
    /// If range contains a height of a header that is not found in the store or [`RangeBounds`]
    /// cannot be converted to a valid range.
    pub async fn get_headers<R>(&self, range: R) -> Result<Vec<ExtendedHeader>>
    where
        R: RangeBounds<u64> + Send,
    {
        Ok(self.store.get_range(range).await?)
    }
}
