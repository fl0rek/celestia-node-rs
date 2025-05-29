use celestia_grpc::Error as GrpcError;
use celestia_grpc::TxClient;
use celestia_grpc::TxConfig;
use celestia_types::nmt::Namespace;
use celestia_types::state::Address;
use celestia_types::Blob;
use clap::Parser;
use ecdsa::SigningKey;
use lumina_node::blockstore::InMemoryBlockstore;
use lumina_node::events::NodeEvent;
use lumina_node::network::Network;
use lumina_node::network::NetworkId;
use lumina_node::node::MIN_PRUNING_DELAY;
use lumina_node::node::MIN_SAMPLING_WINDOW;
use lumina_node::store::InMemoryStore;
use lumina_node::Node;
use lumina_node::NodeError;
use tracing::{error, info, warn};

const MAMMOTH_GRPC: &str = "https://global.grpc.mamochain.com";
const MAMMOTH_BOOTNODE: &str = "/dnsaddr/da-bridge-0.par.mamochain.com/p2p/12D3KooWNc3hDtzLvyKj8xbcE3SFMRy4uX5EojCScCuqYRrz4tzS";
const MAMMOTH_ADDR: &str = "celestia13ragg08622j6lm5ej52hhuvynntmvkz7gdk5re";
//const MAMMOTH_PUBKEY: &str = "AlqDIu+WltdaxgO79Ig7X7IQ/h3Ve806qWhOevWQeOEl";
const MAMMOTH_PRIVKEY: &str = "e42e07120e2a4b5f24789c68f5aebab0384ddc657ec232d2bde8cfb83dd400e6";

#[derive(Parser, Debug)]
struct Params {}

struct App {
    namespace: Namespace,
    node: Node<InMemoryBlockstore, InMemoryStore>,
}

impl App {
    fn new(node: Node<InMemoryBlockstore, InMemoryStore>, namespace: Namespace) -> Self {
        App { node, namespace }
    }

    async fn get_blobs(&self, height: u64) -> Result<Vec<Blob>, NodeError> {
        let header = self.node.get_header_by_height(height).await?;
        let blobs = self
            .node
            .request_all_blobs(&header, self.namespace, None)
            .await;
        blobs
    }

    async fn submit_blobs(&self, blobs: &[Blob]) -> Result<(), GrpcError> {
        let addr: Address = MAMMOTH_ADDR.parse().expect("valid addr");
        //let vk = VerifyingKey::from_sec1_bytes( &hex::decode(MAMMOTH_PUBKEY).expect("valid key encoding"),) .expect("valid key");
        let sk = SigningKey::from_slice(&hex::decode(MAMMOTH_PRIVKEY).expect("valid key encoding"))
            .expect("valid key");
        let vk = sk.verifying_key().clone();
        let txclient = TxClient::with_url(MAMMOTH_GRPC, &addr, vk, sk).await?;

        let info = txclient.submit_blobs(blobs, TxConfig::default()).await?;

        info!("Submitted: {info:?}");
        Ok(())
    }
}

#[tokio::main]
async fn main() {
    let _tracing = init_tracing();

    let p = Params::parse();
    run(p).await
}

pub(crate) async fn run(params: Params) {
    info!("Params: {params:?}");

    let store = InMemoryStore::new();
    let blockstore = InMemoryBlockstore::new();

    let (node, mut events) = Node::builder()
        .store(store)
        .blockstore(blockstore)
        .network(Network::Custom(
            NetworkId::new("mamo-1").expect("networkid"),
        ))
        .sampling_window(MIN_SAMPLING_WINDOW)
        .pruning_delay(MIN_PRUNING_DELAY)
        .bootnodes([MAMMOTH_BOOTNODE.parse().expect("valid bootnode")])
        .start_subscribed()
        .await
        .expect("to work");

    let namespace = Namespace::new_v0(b"foo").expect("valid namespace");

    let app = App::new(node, namespace);

    let b =
        Blob::new(namespace, b"bar".to_vec(), celestia_types::AppVersion::V3).expect("valid blob");
    info!("===");
    app.submit_blobs(&[b]).await.expect("submit ok");
    info!("===");

    while let Ok(ev) = events.recv().await {
        match ev.event {
            NodeEvent::AddedHeaderFromHeaderSub { height } => match app.get_blobs(height).await {
                Ok(blobs) => info!("New header {height}, got {} blobs", blobs.len()),
                Err(e) => error!("Err fetching blobs for {height}: {e}"),
            },
            // Skip noisy events
            NodeEvent::ShareSamplingResult { .. } => continue,
            event if event.is_error() => warn!("{event}"),
            event => info!("{event}"),
        }
    }
}

fn init_tracing() -> tracing_appender::non_blocking::WorkerGuard {
    let (non_blocking, guard) = tracing_appender::non_blocking(std::io::stdout());

    let filter = tracing_subscriber::EnvFilter::builder()
        .with_default_directive(tracing_subscriber::filter::LevelFilter::INFO.into())
        .from_env_lossy();

    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(non_blocking)
        .init();

    guard
}
