use anyhow::{Context, Result};
use celestia_types::hash::Hash;
#[cfg(not(target_arch = "wasm32"))]
use clap::ValueEnum;
use libp2p::Multiaddr;

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(not(target_arch = "wasm32"), derive(ValueEnum))]
pub(crate) enum Network {
    Arabica,
    Mocha,
    #[default]
    Private,
}

pub(crate) async fn network_bootnodes(network: Network) -> Result<Vec<Multiaddr>> {
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
                "/ip4/127.0.0.1/udp/2121/quic/p2p/12D3KooWCSdbDpir7iBvwrkbcofgiZX6ww9r2ED5k8srsP1tV1Da",
                "/ip4/127.0.0.1/tcp/2121/p2p/12D3KooWCSdbDpir7iBvwrkbcofgiZX6ww9r2ED5k8srsP1tV1Da",
                "/dns4/da-bridge-mocha-4.celestia-mocha.com/udp/2121/quic/p2p/12D3KooWCBAbQbJSpCpCGKzqz3rAN4ixYbc63K68zJg9aisuAajg",
                "/dns4/da-bridge-mocha-4-2.celestia-mocha.com/udp/2121/quic/p2p/12D3KooWK6wJkScGQniymdWtBwBuU36n6BRXp9rCDDUD6P5gJr3G",
                "/dns4/da-full-1-mocha-4.celestia-mocha.com/udp/2121/quic/p2p/12D3KooWCUHPLqQXZzpTx1x3TAsdn3vYmTNDhzg66yG8hqoxGGN8",
                "/dns4/da-full-2-mocha-4.celestia-mocha.com/udp/2121/quic/p2p/12D3KooWR6SHsXPkkvhCRn6vp1RqSefgaT1X1nMNvrVjU2o3GoYy",
            ]
            .iter()
            .map(|s| s.parse().unwrap())
            .collect()
        ),
        Network::Private => Ok(
            //fetch_bridge_multiaddrs("ws://localhost:26658").await?
            unimplemented!()
        )
    }
}

pub(crate) fn network_id(network: Network) -> &'static str {
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
