use anyhow::Result;
use axum::body;
use axum::extract::State;
use axum::http::Uri;
use axum::http::{header, StatusCode};
use axum::response::{Html, IntoResponse, Response};
use axum::routing::get;
use axum::Router;
use clap::Parser;
use libp2p::Multiaddr;
use std::net::SocketAddr;
use tokio::{spawn, time};

use celestia::common::{Network, WasmNodeArgs};

const BIND_ADDR: &str = "127.0.0.1:9876";

#[derive(rust_embed::RustEmbed)]
#[folder = "pkg"]
struct WasmPackage;

#[derive(Debug, Clone, Parser)]
struct Args {
    /// Network to connect.
    #[arg(short, long, value_enum, default_value_t)]
    network: Network,

    /// Bootnode multiaddr, including peer id. Can be used multiple times.
    #[arg(long)]
    bootnode: Vec<Multiaddr>,

    /// Address to serve app at
    #[arg(long, default_value = BIND_ADDR)]
    listen_addr: SocketAddr,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    let state = WasmNodeArgs {
        network: args.network,
        bootnodes: args.bootnode,
    };

    let app = Router::new()
        .route("/", get(serve_index_html))
        .fallback(serve_wasm_pkg)
        .with_state(state);

    spawn(axum::Server::bind(&args.listen_addr).serve(app.into_make_service()));

    loop {
        time::sleep(time::Duration::from_secs(1)).await;
    }
}

async fn serve_wasm_pkg(uri: Uri) -> Result<Response, StatusCode> {
    let path = uri.path().trim_start_matches('/').to_string();
    if let Some(content) = WasmPackage::get(&path) {
        let mime = mime_guess::from_path(&path).first_or_octet_stream();
        Ok(Response::builder()
            .header(header::CONTENT_TYPE, mime.as_ref())
            .body(body::boxed(body::Full::from(content.data)))
            .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?)
    } else {
        Err(StatusCode::NOT_FOUND)
    }
}

async fn serve_index_html(state: State<WasmNodeArgs>) -> Result<impl IntoResponse, StatusCode> {
    let args = serde_json::to_string(&state.0).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Html(format!(
        r#"
        <!DOCTYPE html>
        <html>
        <head>
            <meta charset="UTF-8" />
            <title>celestia-node-rs</title>
            <script type="module"">
                Error.stackTraceLimit = 99;
                import init, {{ run }} from "/celestia.js";

                // initialize wasm
                await init();
                // run our entrypoint with params from the env
                await run('{args}');
            </script>
        </head>
        <body></body>
        </html>
        "#
    )))
}
