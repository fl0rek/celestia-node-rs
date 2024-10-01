use wasm_bindgen_futures::spawn_local;
use wasm_bindgen::prelude::*;
use lumina_node_wasm::NodeWorker;
use lumina_node_wasm::utils::setup_logging;
use web_sys::DedicatedWorkerGlobalScope;
use tracing::info;

fn main() {
    setup_logging();

    info!("in worker");

    let scope = DedicatedWorkerGlobalScope::from(JsValue::from(js_sys::global()));
    let mut node_worker = NodeWorker::new(scope.into());

    spawn_local(async move {
        node_worker.run().await.expect("worker loop to not return error");
    })
}

