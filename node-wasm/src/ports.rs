#![allow(unused_imports, unused_variables, dead_code)]

use std::future::Future;

use futures::stream::SelectAll;
use js_sys::{Array, Function, Reflect};
use serde::Serialize;
use serde_wasm_bindgen::{from_value, to_value, Serializer};
use tokio::sync::{mpsc, Mutex};
use tracing::{error, info, trace};
use wasm_bindgen::closure::Closure;
use wasm_bindgen::prelude::*;
use wasm_bindgen::{JsCast, JsValue};
use web_sys::{MessageEvent, MessagePort};

use crate::commands::{NodeCommand, WorkerResponse};
use crate::error::{Context, Error, Result};
use crate::multiplex::{Client, Server};
use crate::utils::MessageEventExt;

// Instead of supporting communication with just `MessagePort`, allow using any object which
// provides compatible interface, eg. `Worker`
#[wasm_bindgen]
extern "C" {
    pub type MessagePortLike;

    #[wasm_bindgen(catch, method, structural, js_name = postMessage)]
    pub fn post_message(this: &MessagePortLike, message: &JsValue) -> Result<(), JsValue>;

    #[wasm_bindgen(catch, method, structural, js_name = postMessage)]
    pub fn post_message_with_transferable(
        this: &MessagePortLike,
        message: &JsValue,
        transferable: &JsValue,
    ) -> Result<(), JsValue>;
}

impl From<MessagePort> for MessagePortLike {
    fn from(port: MessagePort) -> Self {
        JsValue::from(port).into()
    }
}

pub(crate) type WorkerServer = Server<NodeCommand, WorkerResponse>;

pub struct WorkerClient {
    client: Client<NodeCommand, WorkerResponse>,
}

impl WorkerClient {
    pub fn new(object: JsValue) -> Result<Self> {
        Ok(WorkerClient {
            client: Client::start(object)?,
        })
    }

    pub(crate) async fn add_connection_to_worker(&self, port: JsValue) -> Result<()> {
        let response = self.client.send(NodeCommand::InternalPing, Some(port)).await?;

        let worker_response = response.await
            .context("Response oneshot dropped, should not happen")?;

        if !worker_response.is_internal_pong() {
            Err(Error::new(&format!(
            "invalid response, expected InternalPing got {worker_response:?}"
        )))
        } else {
            Ok(())
        }
    }

    pub(crate) async fn exec(&self, command: NodeCommand) -> Result<WorkerResponse> {
        let response = self.client.send(command, None).await?;
        response
            .await
            .context("Response oneshot dropped, should not happen")
    }
}

// helper to hide slight differences in message passing between runtime.Port used by browser
// extensions and everything else
fn prepare_message_port(
    object: JsValue,
    callback: &Closure<dyn Fn(MessageEvent)>,
) -> Result<MessagePortLike, Error> {
    // check whether provided object has `postMessage` method
    let _post_message: Function = Reflect::get(&object, &"postMessage".into())?
        .dyn_into()
        .context("could not get object's postMessage")?;

    if Reflect::has(&object, &JsValue::from("onMessage"))
        .context("failed to reflect onMessage property")?
    {
        // Browser extension runtime.Port has `onMessage` property, on which we should call
        // `addListener` on.
        let listeners = Reflect::get(&object, &"onMessage".into())
            .context("could not get `onMessage` property")?;

        let add_listener: Function = Reflect::get(&listeners, &"addListener".into())
            .context("could not get `onMessage.addListener` property")?
            .dyn_into()
            .context("expected `onMessage.addListener` to be a function")?;
        Reflect::apply(&add_listener, &listeners, &Array::of1(callback.as_ref()))
            .context("error calling `onMessage.addListener`")?;
    } else if Reflect::has(&object, &JsValue::from("onmessage"))
        .context("failed to reflect onmessage property")?
    {
        // MessagePort, as well as message passing via Worker instance, requires setting
        // `onmessage` property to callback
        Reflect::set(&object, &"onmessage".into(), callback.as_ref())
            .context("could not set onmessage callback")?;
    } else {
        return Err(Error::new("Don't know how to register onmessage callback"));
    }

    Ok(MessagePortLike::from(object))
}

#[cfg(test)]
mod tests {
    use super::*;
    use wasm_bindgen_futures::spawn_local;
    use wasm_bindgen_test::wasm_bindgen_test;
    use web_sys::MessageChannel;

    #[wasm_bindgen_test]
    async fn client_server() {
        crate::utils::setup_logging();

        let channel0 = MessageChannel::new().unwrap();
        let mut server = WorkerServer::new();
        let port_channel = server.get_port_channel();

        spawn_local(async move {
            let (request, responder) = server.recv().await.unwrap();
            assert!(matches!(request, NodeCommand::IsRunning));
            responder.send(WorkerResponse::IsRunning(false)).unwrap();

            let (request, responder) = server.recv().await.unwrap();
            assert!(matches!(request, NodeCommand::InternalPing));
            responder.send(WorkerResponse::InternalPong).unwrap();

            let (request, responder) = server.recv().await.unwrap();
            assert!(matches!(request, NodeCommand::IsRunning));
            responder.send(WorkerResponse::IsRunning(true)).unwrap();
        });

        port_channel.send(channel0.port1().into()).unwrap();
        let client0 = WorkerClient::new(channel0.port2().into()).unwrap();

        let response = client0.exec(NodeCommand::IsRunning).await.unwrap();
        assert!(matches!(response, WorkerResponse::IsRunning(false)));

        let channel1 = MessageChannel::new().unwrap();
        client0.add_connection_to_worker(channel1.port1().into()).await.unwrap();
        let client1 = WorkerClient::new(channel1.port2().into()).unwrap();

        let response = client1.exec(NodeCommand::IsRunning).await.unwrap();
        assert!(matches!(response, WorkerResponse::IsRunning(true)));

    }
}
