use std::fmt::Debug;

use enum_as_inner::EnumAsInner;
use js_sys::Array;
use libp2p::Multiaddr;
use libp2p::PeerId;
use serde::{Deserialize, Serialize};
use tracing::error;
use wasm_bindgen::JsValue;

use celestia_types::hash::Hash;
use lumina_node::peer_tracker::PeerTrackerInfo;
use lumina_node::store::SamplingMetadata;
use lumina_node::syncer::SyncingInfo;

use crate::node::WasmNodeConfig;
use crate::utils::JsResult;
use crate::worker::Result;
use crate::worker::WorkerError;
use crate::wrapper::libp2p::NetworkInfoSnapshot;

#[derive(Debug, Serialize, Deserialize)]
pub(crate) enum NodeCommand {
    IsRunning,
    StartNode(WasmNodeConfig),
    GetLocalPeerId,
    GetSyncerInfo,
    GetPeerTrackerInfo,
    GetNetworkInfo,
    GetConnectedPeers,
    SetPeerTrust {
        peer_id: PeerId,
        is_trusted: bool,
    },
    WaitConnected {
        trusted: bool,
    },
    GetListeners,
    RequestHeader(SingleHeaderQuery),
    GetVerifiedHeaders {
        #[serde(with = "serde_wasm_bindgen::preserve")]
        from: JsValue,
        amount: u64,
    },
    GetHeadersRange {
        start_height: Option<u64>,
        end_height: Option<u64>,
    },
    GetHeader(SingleHeaderQuery),
    LastSeenNetworkHead,
    GetSamplingMetadata {
        height: u64,
    },
}

#[derive(Serialize, Deserialize, Debug)]
pub(crate) enum SingleHeaderQuery {
    Head,
    ByHash(Hash),
    ByHeight(u64),
}

#[derive(Serialize, Deserialize, Debug, EnumAsInner)]
pub(crate) enum WorkerResponse {
    IsRunning(bool),
    NodeStarted(Result<()>),
    LocalPeerId(String),
    SyncerInfo(Result<SyncingInfo>),
    PeerTrackerInfo(PeerTrackerInfo),
    NetworkInfo(Result<NetworkInfoSnapshot>),
    ConnectedPeers(Result<Vec<String>>),
    SetPeerTrust(Result<()>),
    Connected(Result<()>),
    Listeners(Result<Vec<Multiaddr>>),
    Header(JsResult<JsValue, WorkerError>),
    Headers(JsResult<Array, WorkerError>),
    #[serde(with = "serde_wasm_bindgen::preserve")]
    LastSeenNetworkHead(JsValue),
    SamplingMetadata(Result<Option<SamplingMetadata>>),
}

pub(crate) trait CheckableResponseExt {
    type Output;
    fn check_variant(self) -> Result<Self::Output, WorkerError>;
}

impl<T> CheckableResponseExt for Result<T, WorkerResponse> {
    type Output = T;

    fn check_variant(self) -> Result<Self::Output, WorkerError> {
        self.map_err(|response| {
            error!("invalid response, received: {response:?}");
            WorkerError::InvalidResponseType
        })
    }
}
