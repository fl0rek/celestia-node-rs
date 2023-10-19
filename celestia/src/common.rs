use std::str::FromStr;

use serde::{Deserialize, Serialize};
use libp2p::Multiaddr;
use anyhow::{Context, Result};
use celestia_types::hash::Hash;
#[cfg(not(target_arch = "wasm32"))]
use clap::ValueEnum;

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
            network => Err(UnknownNetworkError(network.to_string()))
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WasmNodeArgs {
    pub network: Network,
    pub bootnodes: Vec<Multiaddr>,
}
