#[cfg(not(target_arch = "wasm32"))]
#[tokio::main(flavor = "current_thread")]
async fn main() -> anyhow::Result<()> {
    celestia::run_cli().await
}

// Placeholder to allow compilation
#[cfg(target_arch = "wasm32")]
fn main() {}
