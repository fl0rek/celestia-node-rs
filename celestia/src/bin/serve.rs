use anyhow::Result;
use axum::body;
use axum::http::Uri;
use axum::http::{header, StatusCode};
use axum::response::{Html, IntoResponse, Response};
use axum::routing::get;
use axum::Router;
use axum::extract::State;
use tokio::{spawn, time};
use clap::Parser;
use libp2p::Multiaddr;

const BIND_ADDR: &str = "127.0.0.1:9876";

#[derive(rust_embed::RustEmbed)]
#[folder = "pkg"]
struct WasmPackage;

#[derive(Debug, Clone, Parser)]
struct Args {
    #[arg(long)]
    webtransport: Multiaddr
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    let app = Router::new()
        .route("/", get(serve_index_html))
        .fallback(serve_wasm_pkg)
        .with_state(args);

    spawn(axum::Server::bind(&BIND_ADDR.parse()?).serve(app.into_make_service()));

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
            .unwrap()) // XXX
    } else {
        Err(StatusCode::NOT_FOUND)
    }
}

async fn serve_index_html(state: State<Args>) -> Result<impl IntoResponse, StatusCode> {
    let bootnode = state.webtransport.clone();
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
                        await run("{bootnode}");
                    </script>
                </head>
                <body></body>
                </html>
                "#
    )))
}
