use std::time::Duration;

use anyhow::{Context, Result};
use celestia_node::node::{Node, NodeConfig};
use celestia_node::store::IndexedDbStore;
use futures_timer::Delay;
use libp2p::identity;
use wasm_bindgen::prelude::*;

use crate::common::{network_bootnodes, network_genesis, network_id, Network};

#[wasm_bindgen]
pub async fn run(bootnode: &str) -> Result<(), JsError> {
    console_error_panic_hook::set_once();

    tracing_wasm::set_as_global_default();

    let network = Network::Private; // TODO

    let p2p_local_keypair = identity::Keypair::generate_ed25519();
    //let p2p_bootstrap_peers = network_bootnodes(network).await.unwrap();
    let network_id = network_id(network).to_owned();
    let genesis_hash = network_genesis(network).unwrap();

    let boot_peers = vec![
    //"/ip4/172.20.0.3/udp/2121/quic-v1/webtransport/certhash/uEiAhZR1np0puk_gdowf4M57EpjJDIe0FVz6m8YLKcLEugQ/certhash/uEiAeUUxNwU-gIoIDypplmYdFKlnns3aqrEu1Rx2iRDzeEQ/p2p/12D3KooWGeXJcrk96ShMNexLV6uUbxPq7VftME5BqHUUHZfYrrVh"
    bootnode
    ];

    let store = IndexedDbStore::new().await.unwrap();
    let node = Node::new(NodeConfig {
        network_id,
        genesis_hash,
        p2p_local_keypair,
        p2p_bootstrap_peers: boot_peers
            .iter()
            .map(|addr| addr.parse().unwrap())
            .collect::<Vec<_>>(),
        p2p_listen_on: vec![],
        store,
    })
    .await
    .context("Failed to start node")
    .unwrap();

    tracing::trace!("waiting for peers to connect");
    node.p2p().wait_connected_trusted().await?;

    tracing::trace!("connected to boot peer");

    // We have nothing else to do, but we want to keep main alive
    loop {
        let now = Delay::new(Duration::from_secs(1)).await;
        tracing::info!("running");
    }

    Ok(())
}

