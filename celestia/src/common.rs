use std::str::FromStr;

use anyhow::{Context, Result};
use celestia_types::hash::Hash;
#[cfg(not(target_arch = "wasm32"))]
use clap::ValueEnum;
use libp2p::Multiaddr;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WasmNodeArgs {
    pub network: Network,
    pub bootnodes: Vec<Multiaddr>,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(not(target_arch = "wasm32"), derive(ValueEnum))]
pub enum Network {
    Arabica,
    Mocha,
    #[default]
    Private,
}

#[derive(Debug)]
pub struct UnknownNetworkError(String);

impl FromStr for Network {
    type Err = UnknownNetworkError;

    fn from_str(network_id: &str) -> Result<Self, Self::Err> {
        match network_id {
            "arabica-10" => Ok(Network::Arabica),
            "mocha-4" => Ok(Network::Mocha),
            "private" => Ok(Network::Private),
            network => Err(UnknownNetworkError(network.to_string())),
        }
    }
}

pub fn network_id(network: Network) -> &'static str {
    match network {
        Network::Arabica => "arabica-10",
        Network::Mocha => "mocha-4",
        Network::Private => "private",
    }
}

pub(crate) fn network_genesis(network: Network) -> Result<Option<Hash>> {
    let hex = match network {
        Network::Arabica => "5904E55478BA4B3002EE885621E007A2A6A2399662841912219AECD5D5CBE393",
        Network::Mocha => "B93BBE20A0FBFDF955811B6420F8433904664D45DB4BF51022BE4200C1A1680D",
        Network::Private => return Ok(None),
    };

    let bytes = hex::decode(hex).context("Failed to decode genesis hash")?;
    let array = bytes
        .try_into()
        .ok()
        .context("Failed to decode genesis hash")?;

    Ok(Some(Hash::Sha256(array)))
}

pub(crate) fn canonical_network_bootnodes(network: Network) -> Result<Vec<Multiaddr>> {
    match network {
        Network::Arabica => Ok(
            [
                "/dns4/da-bridge.celestia-arabica-10.com/tcp/2121/p2p/12D3KooWM3e9MWtyc8GkP8QRt74Riu17QuhGfZMytB2vq5NwkWAu",
                "/dns4/da-bridge-2.celestia-arabica-10.com/tcp/2121/p2p/12D3KooWKj8mcdiBGxQRe1jqhaMnh2tGoC3rPDmr5UH2q8H4WA9M",
                "/dns4/da-full-1.celestia-arabica-10.com/tcp/2121/p2p/12D3KooWBWkgmN7kmJSFovVrCjkeG47FkLGq7yEwJ2kEqNKCsBYk",
                "/dns4/da-full-2.celestia-arabica-10.com/tcp/2121/p2p/12D3KooWRByRF67a2kVM2j4MP5Po3jgTw7H2iL2Spu8aUwPkrRfP",
            ]
            .iter()
            .map(|s| s.parse().unwrap())
            .collect()
        ),
        Network::Mocha => Ok(
            [
            "/ip4/40.85.94.176/tcp/2121/p2p/12D3KooWNJ3Nf1DTQTz8JZogg2eSvPKKKv8itC6fxxspe4C6bizs",
            "/ip4/40.85.94.176/udp/2121/quic-v1/p2p/12D3KooWNJ3Nf1DTQTz8JZogg2eSvPKKKv8itC6fxxspe4C6bizs",
            "/ip4/40.85.94.176/udp/2121/quic-v1/webtransport/certhash/uEiBf-OX4HzFK9owOpjdCifsDIWRO0SoD3j3vGKlq0pAXKw/certhash/uEiCx1md1BATJ_0NXAjp3KOuwRYG1535E7kUzFdMq8aPaWw/p2p/12D3KooWNJ3Nf1DTQTz8JZogg2eSvPKKKv8itC6fxxspe4C6bizs",
            "/ip4/40.85.94.176/udp/2121/quic-v1/p2p/12D3KooWQUYAApYb4DJnhS1QmAwRr5HRvUeHJYocchCpwEhCtDGu",
            "/ip4/40.85.94.176/udp/2121/quic-v1/webtransport/certhash/uEiBr4-sr95BpqfA-ttpjiLdjbGABhTvX8oxrTXf3Ubfibw/certhash/uEiBSVgyze9xG1UbbNuTwyEUWLPq7l2N9pyeQSs3OtEhGRg/p2p/12D3KooWQUYAApYb4DJnhS1QmAwRr5HRvUeHJYocchCpwEhCtDGu",
            "/dns4/da-bridge-mocha-4.celestia-mocha.com/udp/2121/quic/p2p/12D3KooWCBAbQbJSpCpCGKzqz3rAN4ixYbc63K68zJg9aisuAajg",
            "/dns4/da-bridge-mocha-4-2.celestia-mocha.com/udp/2121/quic/p2p/12D3KooWK6wJkScGQniymdWtBwBuU36n6BRXp9rCDDUD6P5gJr3G",
            "/dns4/da-full-1-mocha-4.celestia-mocha.com/udp/2121/quic/p2p/12D3KooWCUHPLqQXZzpTx1x3TAsdn3vYmTNDhzg66yG8hqoxGGN8",
            "/dns4/da-full-2-mocha-4.celestia-mocha.com/udp/2121/quic/p2p/12D3KooWR6SHsXPkkvhCRn6vp1RqSefgaT1X1nMNvrVjU2o3GoYy",
            ]
            .iter()
            .map(|s| s.parse().unwrap())
            .collect()
        ),
        Network::Private => Ok(vec![])
    }
}
