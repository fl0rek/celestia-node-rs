use anyhow::{Context, Result};
use celestia_types::hash::Hash;
#[cfg(not(target_arch = "wasm32"))]
use clap::ValueEnum;

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(not(target_arch = "wasm32"), derive(ValueEnum))]
#[cfg_attr(target_arch = "wasm32", allow(dead_code))]
pub enum Network {
    Arabica,
    Mocha,
    #[default]
    Private,
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
