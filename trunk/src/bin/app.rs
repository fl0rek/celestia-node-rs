use js_sys::Reflect;
use lumina_node_wasm::NodeClient;
use lumina_node_wasm::utils::setup_logging;
use wasm_bindgen_futures::spawn_local;
use wasm_bindgen::prelude::*;
use web_sys::{window, Worker, WorkerOptions, WorkerType};

#[wasm_bindgen(inline_js = "export function set_node(node) { console.log(node); window.node = node }")]
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

    web_sys::console::log_1(&"not worker starting".into());
    let worker = worker_new("./worker_loader.js");

    spawn_local(async move {
        let node_client = NodeClient::new(worker.clone().into()).await.expect("to initialise NodeClient");
        set_node(node_client)
        //Reflect::set(&window().expect("to access window"), &"node".into(), node_client.dyn_ref().unwrap());
    })


    //let document = window() .and_then(|win| win.document()) .expect("Could not access the document");
    //let body = document.body().expect("Could not access document.body");
    //let text_node = document.create_text_node("Hello, world from Vanilla Rust!");
    //body.append_child(text_node.as_ref()) .expect("Failed to append text");
}
