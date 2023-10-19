use std::time::Duration;

use anyhow::{Context, Result};
use celestia_node::node::{Node, NodeConfig};
use celestia_node::store::IndexedDbStore;
use futures_timer::Delay;
use libp2p::identity;
use wasm_bindgen::prelude::*;
use tracing::info;
use serde_wasm_bindgen::{from_value, to_value};
use js_sys::JSON;

use crate::common::{network_genesis, network_id, Network, WasmNodeArgs};

#[wasm_bindgen]
pub async fn run(args: &str) -> Result<(), JsError> {
    console_error_panic_hook::set_once();

    tracing_wasm::set_as_global_default();

    info!("Arg: {args}");

    let args: WasmNodeArgs = from_value(JSON::parse(args).unwrap())?;

    let network = args.network; //network_id.parse().unwrap();
    let network_id = network_id(network);

    let p2p_local_keypair = identity::Keypair::generate_ed25519();
    let genesis_hash = network_genesis(network).unwrap();

    let boot_peers = vec![
        "/ip4/40.85.94.176/tcp/2121/p2p/12D3KooWNJ3Nf1DTQTz8JZogg2eSvPKKKv8itC6fxxspe4C6bizs",
        "/ip4/40.85.94.176/udp/2121/quic-v1/p2p/12D3KooWNJ3Nf1DTQTz8JZogg2eSvPKKKv8itC6fxxspe4C6bizs",
        "/ip4/40.85.94.176/udp/2121/quic-v1/webtransport/certhash/uEiBf-OX4HzFK9owOpjdCifsDIWRO0SoD3j3vGKlq0pAXKw/certhash/uEiCx1md1BATJ_0NXAjp3KOuwRYG1535E7kUzFdMq8aPaWw/p2p/12D3KooWNJ3Nf1DTQTz8JZogg2eSvPKKKv8itC6fxxspe4C6bizs",
  ];
    //let boot_peers = vec![ "/ip4/172.20.0.3/udp/2121/quic-v1/webtransport/certhash/uEiAhZR1np0puk_gdowf4M57EpjJDIe0FVz6m8YLKcLEugQ/certhash/uEiAeUUxNwU-gIoIDypplmYdFKlnns3aqrEu1Rx2iRDzeEQ/p2p/12D3KooWGeXJcrk96ShMNexLV6uUbxPq7VftME5BqHUUHZfYrrVh" bootnode, ];

    let store = IndexedDbStore::new(&network_id).await.unwrap();
    info!(
        "Initialised store with head height: {:?}",
        store.get_head_height().await
    );

    let node = Node::new(NodeConfig {
        network_id: network_id.to_string(),
        genesis_hash,
        p2p_local_keypair,
        p2p_bootnodes: boot_peers
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
        Delay::new(Duration::from_secs(1)).await;
        tracing::info!("running");
    }
}
