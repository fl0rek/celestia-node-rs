use js_sys::Reflect;
use lumina_node_wasm::NodeClient;
use lumina_node_wasm::utils::setup_logging;
use wasm_bindgen_futures::spawn_local;
use wasm_bindgen::prelude::*;
use web_sys::{window, Worker, WorkerOptions, WorkerType, CustomEvent, CustomEventInit};

#[wasm_bindgen(inline_js = "export function set_node(node) { window.node = node }")]
extern "C" {
    fn set_node(node: NodeClient);
}

fn worker_new(url: &str) -> Worker {
    let options = WorkerOptions::new();
    options.set_type(WorkerType::Module);
    Worker::new_with_options(url, &options).expect("failed to spawn worker")
}

fn main() {
    setup_logging();

    let worker = worker_new("./worker_loader.js");

    spawn_local(async move {
        let node_client = NodeClient::new(worker.clone().into()).await.expect("to initialise NodeClient");
        set_node(node_client);
        //let event_params = CustomEventInit::new();
        //event_params.set_detail(node_client);
        let lumina_event = CustomEvent::new("LuminaReady").expect("to create an event");
        window().expect("to access window").dispatch_event(&lumina_event);
        //Reflect::set(&window().expect("to access window"), &"node".into(), node_client.dyn_ref().unwrap());
    });
}
