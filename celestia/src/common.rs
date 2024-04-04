use anyhow::Result;
use celestia_node::network::Network;
use clap::{Parser, ValueEnum};
use serde_repr::Serialize_repr;

use tracing_appender::non_blocking;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::Layer;

use crate::{native, server};

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, ValueEnum, Serialize_repr)]
#[repr(u8)]
pub(crate) enum ArgNetwork {
    #[default]
    Mainnet,
    Arabica,
    Mocha,
    Private,
}

#[derive(Debug, Parser)]
pub(crate) enum CliArgs {
    /// Run native node locally
    Node(native::Params),
    /// Serve compiled wasm node to be run in the browser
    Browser(server::Params),
}

pub async fn run_cli() -> Result<()> {
    let _ = dotenvy::dotenv();
    let args = CliArgs::parse();
    let _guard = init_tracing();

    match args {
        CliArgs::Node(args) => native::run(args).await,
        CliArgs::Browser(args) => server::run(args).await,
    }
}

fn init_tracing() -> tracing_appender::non_blocking::WorkerGuard {
    //let console_layer = console_subscriber::spawn();

    let (non_blocking, guard) =
        tracing_appender::non_blocking(tracing_appender::rolling::never("logs", "lumina"));

    let filter = tracing_subscriber::EnvFilter::builder()
        .with_default_directive(tracing_subscriber::filter::LevelFilter::INFO.into())
        .from_env_lossy();

    //tracing_subscriber::fmt()
    //.with_env_filter(filter)
    //.with_writer(non_blocking)

    let fmt_layer = tracing_subscriber::fmt::layer()
        .with_writer(non_blocking)
        .with_ansi(false)
        .with_filter(filter);

    tracing_subscriber::registry()
        .with(fmt_layer)
        //.with(console_layer)
        //.with_filter(filter)
        //.with_writer(non_blocking)
        .init();

    guard
}

impl From<ArgNetwork> for Network {
    fn from(network: ArgNetwork) -> Network {
        match network {
            ArgNetwork::Mainnet => Network::Mainnet,
            ArgNetwork::Arabica => Network::Arabica,
            ArgNetwork::Mocha => Network::Mocha,
            ArgNetwork::Private => Network::Private,
        }
    }
}

impl From<Network> for ArgNetwork {
    fn from(network: Network) -> ArgNetwork {
        match network {
            Network::Mainnet => ArgNetwork::Mainnet,
            Network::Arabica => ArgNetwork::Arabica,
            Network::Mocha => ArgNetwork::Mocha,
            Network::Private => ArgNetwork::Private,
        }
    }
}
