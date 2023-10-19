use std::time::Duration;

use anyhow::{Context, Result};
use celestia_node::node::{Node, NodeConfig};
use celestia_node::store::IndexedDbStore;
use futures_timer::Delay;
use js_sys::JSON;
use libp2p::identity;
use serde_wasm_bindgen::from_value;
use tracing::{info, trace, debug};
use wasm_bindgen::prelude::*;

use crate::common::{canonical_network_bootnodes, network_genesis, network_id, WasmNodeArgs};

#[wasm_bindgen]
pub async fn run(args: &str) -> Result<(), JsError> {
    console_error_panic_hook::set_once();

    tracing_wasm::set_as_global_default();

    info!("Arg: {args}");

    let args: WasmNodeArgs = from_value(JSON::parse(args).unwrap())?;

    let network = args.network;
    let network_id = network_id(network);

    let p2p_bootnodes = if args.bootnodes.is_empty() {
        canonical_network_bootnodes(network).unwrap()
    } else {
        args.bootnodes
    };

    let p2p_local_keypair = identity::Keypair::generate_ed25519();
    let genesis_hash = network_genesis(network).unwrap();

    let store = IndexedDbStore::new(&network_id).await.unwrap();
    info!(
        "Initialised store with head height: {:?}",
        store.get_head_height().await
    );

    let node = Node::new(NodeConfig {
        network_id: network_id.to_string(),
        genesis_hash,
        p2p_local_keypair,
        p2p_bootnodes,
        p2p_listen_on: vec![],
        store,
    })
    .await
    .context("Failed to start node")
    .unwrap();

    node.p2p().wait_connected_trusted().await?;
    trace!("connected at least one trusted peer");

    // We have nothing else to do, but we want to keep main alive
    loop {
        Delay::new(Duration::from_secs(1)).await;
        debug!("running");
    }
}
