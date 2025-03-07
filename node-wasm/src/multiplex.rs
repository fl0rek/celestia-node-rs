use std::future;
use std::marker::PhantomData;

use futures::stream::{Stream, StreamExt};
use js_sys::{Array, Function, Reflect};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_wasm_bindgen::{from_value, to_value};
use tokio::sync::broadcast;
use tokio::sync::mpsc;
use tokio_stream::wrappers::BroadcastStream;
use tokio_stream::wrappers::errors::BroadcastStreamRecvError;
use tracing::error;
use wasm_bindgen::prelude::*;
use web_sys::MessageEvent;

use crate::error::{Context, Error, Result};

const MULTIPLEX_CHANNEL_SIZE: usize = 16;

#[wasm_bindgen]
extern "C" {
    /// Abstraction over JavaScript MessagePort (but also runtime.Port for browser extension).
    /// Object which can `postMessage` and receive `onmessage` events.
    type SerializableDataPort;

    #[wasm_bindgen(catch, method, structural, js_name = postMessage)]
    fn post_message(this: &SerializableDataPort, message: &JsValue) -> Result<(), JsValue>;
}

/// Wraps JavaScript object with port-like semantics (can `postMessage` and receive `onmessage`),
/// with some convenience and type checking.
pub struct Port {
    port: SerializableDataPort,
    onmessage: Closure<dyn Fn(MessageEvent)>,
}

impl Port {
    /// Create a new Port out of JS object, registering on message callback.
    /// Minimal duck-type checking is performed using reflection when setting appropriate properties.
    pub fn new<F>(object: JsValue, onmessage_callback: F) -> Result<Port>
    where
        F: Fn(MessageEvent) -> Result<()> + 'static,
    {
        let _post_message: Function = Reflect::get(&object, &"postMessage".into())?
            .dyn_into()
            .context("could not get object's postMessage")?;

        let onmessage = Closure::new(move |ev: MessageEvent| {
            if let Err(e) = onmessage_callback(ev) {
                error!("error receiving message: {e}");
            }
        });

        let port = register_onmessage_callback(object, &onmessage)?;

        Ok(Port { port, onmessage })
    }

    /// Send a serialisable message over the port. No checking is performed whether receiver is
    /// able to correctly interpret the message
    pub fn send<T: Serialize>(&self, msg: &T) -> Result<()> {
        let msg = to_value(msg).context("could not serialize message")?;
        self.port
            .post_message(&msg)
            .context("could not send message")?;
        Ok(())
    }
}

impl Drop for Port {
    fn drop(&mut self) {
        unregister_onmessage(&self.port, &self.onmessage)
    }
}

/// counter-style message id for matching responses with requests
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Hash, Default, Debug)]
pub struct MessageId(u128);

impl MessageId {
    /// i++
    fn post_increment(&mut self) -> MessageId {
        let ret = *self;
        let (next, _carry) = self.0.overflowing_add(1);
        self.0 = next;
        ret
    }
}

/// Message being exchanged between MultiplexSender/MultiplexResponder. Carries id for
/// identification and payload
#[derive(Serialize, Deserialize, Clone)]
struct MultiplexMessage<T: Serialize + Clone> {
    /// Id of the message being sent. Id should not be re-used
    id: MessageId,
    /// Actual content of the message
    payload: T,
}

/// Sender part of the multiplexed channel i.e. the one that's responsible for sending request
/// messages that will be received and responded by [`MultiplexResponder`].
pub struct MultiplexSender<Tx, Rx: Serialize + Clone> {
    /// Port over which communication takes place
    port: Port,
    /// Id of the next message to be sent
    next_message_index: MessageId,
    /// Channel for receiving values from onmessage callback which are then converted into
    /// responses
    response_tx: broadcast::Sender<MultiplexMessage<Rx>>,
    /// Make compiler happy
    _send_type: PhantomData<Tx>,
}

impl<Tx, Rx> MultiplexSender<Tx, Rx>
where
    Rx: DeserializeOwned + Serialize + Clone + Send + 'static,
    Tx: DeserializeOwned + Serialize + Clone,
{
    /// Create a new multiplexed communication channel over JS object with [`Port`] semantics
    pub fn new(port: JsValue) -> Result<MultiplexSender<Tx, Rx>> {
        let (response_tx, _) = broadcast::channel(MULTIPLEX_CHANNEL_SIZE);
        //let (pending_messages_tx, pending_messages_rx) = mpsc::unbounded_channel();
        //let callback = Closure::new(error_logging_onmessage(
        let response_sender = response_tx.clone();
        let onmessage = move |ev: MessageEvent| -> Result<()> {
            let message: MultiplexMessage<Rx> =
                from_value(ev.data()).context("could not deserialize response")?;
            response_sender
                .send(message)
                .context("internal response forwarding failed, no receiver waiting for response")?;
            Ok(())
        };

        //let port = register_onmessage_callback(port, &callback)?;
        let port = Port::new(port, onmessage)?;

        Ok(MultiplexSender {
            port,
            next_message_index: Default::default(),
            response_tx,
            _send_type: Default::default(),
        })
    }

    /// Send a value to [`MultiplexResponder`] and return a channel over which response will be
    /// delivered
    pub fn send(&mut self, msg: Tx) -> Result<impl Stream<Item = Option<Rx>>> {
        let mid = self.next_message_index.post_increment();

        let message = MultiplexMessage {
            id: mid,
            payload: msg,
        };

        let response_channel = BroadcastStream::new(self.response_tx.subscribe())
            .filter_map(move |msg| {
                future::ready(match msg {
                    // if response if for our ID, forward `Some(msg.payload)`
                    Ok(MultiplexMessage { id, payload }) if id == mid => Some(Some(payload)),
                    // if it's not, wait for the next value
                    Ok(_) => None,
                    // in case subscriber lags too far behind sender, we'll start losing messages
                    // since receiver is expected to poll almost immediately for the result, this
                    // shouldn't be likely, but we need a way to handle this case, thus send `None`
                    Err(BroadcastStreamRecvError::Lagged(_)) => Some(None),
                })
            })
            .take(1);

        self.port.send(&message)?;

        Ok(response_channel)
    }
}

/// Receiver part of the multiplexed communication channel.
pub struct MultiplexResponder<Tx: Serialize + Clone, Rx> {
    /// Port over which communication takes place
    port: Port,
    /// Channel with pending messages received from onmessage callback
    pending_messages_rx: mpsc::UnboundedReceiver<MultiplexMessage<Tx>>,
    _respond_type: PhantomData<Rx>,
}

impl<Tx, Rx> MultiplexResponder<Tx, Rx>
where
    Rx: Serialize + Clone,
    Tx: Serialize + DeserializeOwned + Clone + 'static,
{
    /// Create a new multiplexed receiving channel over JS object with [`Port`] semantics
    pub fn new(port: JsValue) -> Result<MultiplexResponder<Tx, Rx>> {
        let (pending_messages_tx, pending_messages_rx) = mpsc::unbounded_channel();

        let onmessage = move |ev: MessageEvent| -> Result<()> {
            let message: MultiplexMessage<Tx> =
                from_value(ev.data()).context("could not deserialize message")?;
            pending_messages_tx
                .send(message)
                .context("internal message forwarding failed, should not happen")?;
            Ok(())
        };

        let port = Port::new(port, onmessage)?;

        Ok(MultiplexResponder {
            port,
            pending_messages_rx,
            _respond_type: Default::default(),
        })
    }

    /// Receive next message. Provided message id needs to be passed back to `respond_to`.
    pub async fn recv(&mut self) -> Result<(MessageId, Tx)> {
        let MultiplexMessage { id, payload } = self
            .pending_messages_rx
            .recv()
            .await
            .expect("all internal connections should never close");
        Ok((id, payload))
    }

    /// Send a response to the received message
    /// Sending multiple responses is harmless, but all but first will be ignored.
    pub fn respond_to(&self, id: MessageId, response: Rx) -> Result<()> {
        let message = MultiplexMessage {
            id,
            payload: response,
        };
        self.port.send(&message)
    }
}

// helper to hide slight differences in message passing between runtime.Port used by browser
// extensions and everything else
fn register_onmessage_callback<F>(
    object: JsValue,
    callback: &Closure<F>,
) -> Result<SerializableDataPort, Error>
where
    F: Fn(MessageEvent) + ?Sized, //wut?
{
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

    Ok(SerializableDataPort::from(object))
}

fn unregister_onmessage(port: &JsValue, callback: &Closure<dyn Fn(MessageEvent)>) {
    let object = port.as_ref();

    if Reflect::has(object, &"onMessage".into()).unwrap_or_default() {
        // `runtime.Port` object. Unregistering callback with `removeListener`.
        let listeners =
            Reflect::get(object, &"onMessage".into()).expect("onMessage existence already checked");

        if let Ok(rm_listener) = Reflect::get(&listeners, &"removeListener".into())
            .and_then(|x| x.dyn_into::<Function>())
        {
            let _ = Reflect::apply(&rm_listener, &listeners, &Array::of1(callback.as_ref()));
        }
    } else if Reflect::has(object, &"onmessage".into()).unwrap_or_default() {
        // `MessagePort` object. Unregistering callback by setting `onmessage` to `null`.
        let _ = Reflect::set(object, &"onmessage".into(), &JsValue::NULL);
    }
}


#[cfg(test)]
mod tests {
    use super::*;
    use wasm_bindgen_test::wasm_bindgen_test;
    use web_sys::MessageChannel;

    #[wasm_bindgen_test]
    fn message_id_increment() {
        let mut m = MessageId::default();
        assert_eq!(m.post_increment(), MessageId(0));
        assert_eq!(m.post_increment(), MessageId(1));
        assert_eq!(m.post_increment(), MessageId(2));
    }

    #[wasm_bindgen_test]
    async fn smoke_test() {
        let channel = MessageChannel::new().unwrap();
        let mut sender = MultiplexSender::<i32, i32>::new(channel.port1().into()).unwrap();
        let mut receiver = MultiplexResponder::<i32, i32>::new(channel.port2().into()).unwrap();

        let mut response_channel = sender.send(42).unwrap();

        let (id, msg) = receiver.recv().await.unwrap();
        assert_eq!(msg, 42);
        receiver.respond_to(id, 43).unwrap();

        assert_eq!(response_channel.next().await.unwrap(), Some(43));
    }

    #[wasm_bindgen_test]
    async fn multiple_channels() {
        let channel = MessageChannel::new().unwrap();
        let mut sender = MultiplexSender::<i32, String>::new(channel.port1().into()).unwrap();
        let mut receiver = MultiplexResponder::<i32, String>::new(channel.port2().into()).unwrap();

        let mut response_channels = vec![
            sender.send(0).unwrap(),
            sender.send(1).unwrap(),
            sender.send(2).unwrap(),
            sender.send(3).unwrap(),
            sender.send(4).unwrap(),
            sender.send(5).unwrap(),
        ];

        for i in 0..=5 {
            let (id, msg) = receiver.recv().await.unwrap();
            assert_eq!(i, msg);
            receiver.respond_to(id, format!("R:{msg}")).unwrap();
        }

        for i in (0..=5).rev() {
            assert_eq!(response_channels[i].next().await.unwrap(), Some(format!("R:{i}")));
        }
    }
}
