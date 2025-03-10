use std::collections::HashMap;
use std::marker::PhantomData;

use futures::future::{FutureExt, LocalBoxFuture};
use futures::stream::{FuturesUnordered, StreamExt};
use js_sys::{Array, Function, Reflect};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_wasm_bindgen::{from_value, to_value};
use tokio::select;
use tokio::sync::mpsc;
use tokio::sync::oneshot;
use tokio_util::sync::CancellationToken;
use tracing::{error, warn};
use wasm_bindgen::prelude::*;
use web_sys::MessageEvent;

use crate::error::{Context, Error, Result};
use crate::utils::MessageEventExt;
use lumina_node::executor::{spawn, JoinHandle};

type Transferable = Option<JsValue>;

#[wasm_bindgen]
extern "C" {
    /// Abstraction over JavaScript MessagePort (but also runtime.Port for browser extension).
    /// Object which can `postMessage` and receive `onmessage` events.
    type SerializableDataPort;

    #[wasm_bindgen(catch, method, structural, js_name = postMessage)]
    fn post_message(this: &SerializableDataPort, message: &JsValue) -> Result<(), JsValue>;

    #[wasm_bindgen(catch, method, structural, js_name = postMessage)]
    pub fn post_message_with_transferable(
        this: &SerializableDataPort,
        message: &JsValue,
        transferable: &JsValue,
    ) -> Result<(), JsValue>;
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

    pub fn new_with_channels<T>(
        object: JsValue,
        data_channel: mpsc::UnboundedSender<T>,
        port_channel: Option<mpsc::UnboundedSender<JsValue>>,
    ) -> Result<Port>
    where
        T: DeserializeOwned + 'static,
    {
        Port::new(object, move |ev: MessageEvent| -> Result<()> {
            if let Some(port) = ev.get_port() {
                if let Some(port_channel) = &port_channel {
                    port_channel
                        .send(port)
                        .context("port forwarding channel closed, shouldn't happen: {e}")?;
                }
            }
            let message: T = from_value(ev.data()).context("could not deserialize message")?;
            data_channel
                .send(message)
                .context("forwarding failed, no receiver waiting")?;
            Ok(())
        })
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

    pub fn send_with_transferable<T: Serialize>(&self, msg: &T, object: JsValue) -> Result<()> {
        let msg = to_value(msg).context("could not serialize message")?;
        self.port
            .post_message_with_transferable(&msg, &Array::of1(&object))
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
#[derive(Serialize, Deserialize)]
struct MultiplexMessage<T: Serialize> {
    /// Id of the message being sent. Id should not be re-used
    id: MessageId,
    /// Actual content of the message
    payload: T,
}

//type ReceiverWithResponder<Tx, Rx> = mpsc::Receiver<(Tx, oneshot::Sender<Rx>)>;
//type SenderWithResponder<Tx, Rx> = mpsc::Sender<(Tx, oneshot::Receiver<Rx>)>;

pub struct Client<Request, Response> {
    worker_join_handle: JoinHandle,
    request_tx: mpsc::UnboundedSender<(Request, Transferable, oneshot::Sender<Response>)>,
    cancellation_token: CancellationToken,
}

impl<Request, Response> Client<Request, Response>
where
    Request: Serialize + 'static,
    Response: Serialize + DeserializeOwned + 'static,
{
    pub fn start(port: JsValue) -> Result<Client<Request, Response>> {
        let cancellation_token = CancellationToken::new();
        let (request_tx, request_rx) = mpsc::unbounded_channel();
        let mut worker = ClientWorker::new(port, request_rx, cancellation_token.child_token())?;

        let worker_join_handle = spawn(async move {
            if let Err(e) = worker.run().await {
                error!("Clientworker stopped because of a fatal error: {e}");
            }
        });

        Ok(Client {
            worker_join_handle,
            request_tx,
            cancellation_token,
        })
    }

    pub async fn stop(self) {
        self.cancellation_token.cancel();
        self.worker_join_handle.join().await
    }

    pub async fn send(
        &self,
        request: Request,
        transferable: Option<JsValue>,
    ) -> Result<oneshot::Receiver<Response>> {
        let (tx, rx) = oneshot::channel();
        self.request_tx
            .send((request, transferable, tx))
            .context("could not forward the request to ClientWorker")?;
        Ok(rx)
    }
}

pub struct Server<Request, Response> {
    connections: Vec<ServerConnection<Request, Response>>,
    requests_tx: mpsc::UnboundedSender<(Request, oneshot::Sender<Response>)>,
    requests_rx: mpsc::UnboundedReceiver<(Request, oneshot::Sender<Response>)>,
    ports_tx: mpsc::UnboundedSender<JsValue>,
    ports_rx: mpsc::UnboundedReceiver<JsValue>,
}

impl<Request, Response> Server<Request, Response>
where
    Request: Serialize + DeserializeOwned + 'static,
    Response: Serialize + 'static,
{
    pub fn new() -> Self {
        let (requests_tx, requests_rx) = mpsc::unbounded_channel();
        let (ports_tx, ports_rx) = mpsc::unbounded_channel();

        Server {
            connections: vec![],
            requests_tx,
            requests_rx,
            ports_tx,
            ports_rx,
        }
    }

    pub async fn recv(&mut self) -> Result<(Request, oneshot::Sender<Response>)> {
        loop {
            select! {
                request = self.requests_rx.recv() => {
                    return Ok(request.expect("request channel should not drop"))
                }
                port = self.ports_rx.recv() => {
                    if let Err(e) = self.add_connection(port.expect("port channel should not drop")) {
                        error!("Failed to add new client connection: {e}");
                    }
                }
            }
        }
    }

    fn add_connection(&mut self, port: JsValue) -> Result<()> {
        let client_connection =
            ServerConnection::start(port, self.requests_tx.clone(), self.ports_tx.clone())?;
        self.connections.push(client_connection);
        Ok(())
    }

    pub fn get_port_channel(&self) -> mpsc::UnboundedSender<JsValue> {
        self.ports_tx.clone()
    }
}

pub struct ServerConnection<Request, Response> {
    worker_join_handle: JoinHandle,
    cancellation_token: CancellationToken,
    _transferred_types: PhantomData<(Request, Response)>,
}

impl<Request, Response> ServerConnection<Request, Response>
where
    Request: Serialize + DeserializeOwned + 'static,
    Response: Serialize + 'static,
{
    pub fn start(
        port: JsValue,
        request_tx: mpsc::UnboundedSender<(Request, oneshot::Sender<Response>)>,
        ports_tx: mpsc::UnboundedSender<JsValue>,
    ) -> Result<ServerConnection<Request, Response>> {
        let cancellation_token = CancellationToken::new();
        let mut worker =
            ServerWorker::new(port, request_tx, ports_tx, cancellation_token.child_token())?;

        let worker_join_handle = spawn(async move {
            if let Err(e) = worker.run().await {
                error!("Serverworker stopped because of a fatal error: {e}");
            }
        });

        Ok(ServerConnection::<Request, Response> {
            worker_join_handle,
            cancellation_token,
            _transferred_types: PhantomData::default(),
        })
    }

    pub async fn stop(self) {
        self.cancellation_token.cancel();
        self.worker_join_handle.join().await
    }
}

struct ClientWorker<Request, Response: Serialize> {
    /// Port over which communication takes place
    port: Port,
    /// Queued responses from the onmessage callback
    incoming_responses: mpsc::UnboundedReceiver<MultiplexMessage<Response>>,
    /// Map of message ids waiting for response to oneshot channels to send the response over
    pending_responses_map: HashMap<MessageId, oneshot::Sender<Response>>,
    /// Queued requests to be sent
    outgoing_requests: mpsc::UnboundedReceiver<(Request, Transferable, oneshot::Sender<Response>)>,
    /// MessageId to be used for the next request
    next_message_index: MessageId,
    /// Cancellation token to stop the worker
    cancellation_token: CancellationToken,
}

impl<Request, Response> ClientWorker<Request, Response>
where
    Request: Serialize,
    Response: Serialize + DeserializeOwned + 'static,
{
    fn new(
        port: JsValue,
        request_tx: mpsc::UnboundedReceiver<(Request, Transferable, oneshot::Sender<Response>)>,
        cancellation_token: CancellationToken,
    ) -> Result<ClientWorker<Request, Response>> {
        let (incoming_responses_tx, incoming_responses) = mpsc::unbounded_channel();

        let port = Port::new_with_channels(port, incoming_responses_tx, None)?;

        Ok(ClientWorker {
            port,
            incoming_responses,
            outgoing_requests: request_tx,
            next_message_index: Default::default(),
            pending_responses_map: Default::default(),
            cancellation_token,
        })
    }

    pub async fn run(&mut self) -> Result<()> {
        loop {
            select! {
                _ = self.cancellation_token.cancelled() => {
                    return Ok(())
                }
                msg = self.incoming_responses.recv() => {
                    let Some(MultiplexMessage {id, payload, }) = msg else {
                        return Err(Error::new("Incoming message channel closed, should not happen"));
                    };
                    self.handle_incoming_response(id, payload);
                }
                request = self.outgoing_requests.recv() => {
                    let Some((msg, transferable, response_tx)) = request else {
                        return Err(Error::new("Outgoing requests channel closed, should not happen"));
                    };
                    self.handle_outgoing_request(msg, transferable, response_tx)?;
                }
            }
        }
    }

    fn handle_incoming_response(&mut self, id: MessageId, payload: Response) {
        let Some(response_sender) = self.pending_responses_map.remove(&id) else {
            warn!("received unsolicited response for {id:?}, ignoring");
            return;
        };

        if response_sender.send(payload).is_err() {
            warn!("receiver for {id:?} dropped");
        }
    }

    fn handle_outgoing_request(
        &mut self,
        payload: Request,
        transferable: Transferable,
        response_tx: oneshot::Sender<Response>,
    ) -> Result<()> {
        let mid = self.next_message_index.post_increment();
        let message = MultiplexMessage { id: mid, payload };

        if self
            .pending_responses_map
            .insert(mid, response_tx)
            .is_some()
        {
            return Err(Error::new("collision in message ids, should not happen"));
        }

        if let Some(transferable) = transferable {
            self.port
                .send_with_transferable(&message, transferable)
                .context("failed to send outgoing request")?;
        } else {
            self.port
                .send(&message)
                .context("failed to send outgoing request")?;
        }

        Ok(())
    }
}

struct ServerWorker<Request: Serialize, Response> {
    /// Port over which communication takes place
    port: Port,
    /// Queued requests from the onmessage callback
    incoming_requests: mpsc::UnboundedReceiver<MultiplexMessage<Request>>,
    /// Futures waiting for completion to be send as responses
    pending_responses_map: FuturesUnordered<LocalBoxFuture<'static, (MessageId, Option<Response>)>>,
    /// Channel to send requests and response senders over
    request_tx: mpsc::UnboundedSender<(Request, oneshot::Sender<Response>)>,
    /// Cancellation token to stop the worker
    cancellation_token: CancellationToken,
}

impl<Request, Response> ServerWorker<Request, Response>
where
    Request: Serialize + DeserializeOwned + 'static,
    Response: Serialize + 'static,
{
    fn new(
        port: JsValue,
        request_tx: mpsc::UnboundedSender<(Request, oneshot::Sender<Response>)>,
        port_queue: mpsc::UnboundedSender<JsValue>,
        cancellation_token: CancellationToken,
    ) -> Result<ServerWorker<Request, Response>> {
        let (incoming_requests_tx, incoming_requests) = mpsc::unbounded_channel();

        let port = Port::new_with_channels(port, incoming_requests_tx, Some(port_queue))?;

        Ok(ServerWorker {
            port,
            incoming_requests,
            pending_responses_map: Default::default(),
            request_tx,
            cancellation_token,
        })
    }

    async fn run(&mut self) -> Result<()> {
        loop {
            select! {
                _ = self.cancellation_token.cancelled() => {
                    return Ok(())
                }
                msg = self.incoming_requests.recv() => {
                    let Some(MultiplexMessage {id, payload, }) = msg else {
                        return Err(Error::new("Incoming message channel closed, should not happen"));
                    };
                    self.handle_incoming_request(id, payload).await?;
                }
                res = self.pending_responses_map.next(), if !self.pending_responses_map.is_empty() => {
                    let Some((mid, response) ) = res else {
                    return Err(Error::new("Responses channel closed, should not happen"));
                };
                    self.handle_outgoing_response(mid, response)?;
                }
            }
        }
    }

    async fn handle_incoming_request(&mut self, mid: MessageId, payload: Request) -> Result<()> {
        let (response_tx, response_rx) = oneshot::channel();

        self.request_tx
            .send((payload, response_tx))
            .context("forwarding received request failed, no receiver waiting")?;

        let tagged_response = response_rx.map(move |r| (mid, r.ok())).boxed_local();

        self.pending_responses_map.push(tagged_response);

        Ok(())
    }

    fn handle_outgoing_response(&mut self, id: MessageId, payload: Option<Response>) -> Result<()> {
        // TODO: should we care about responding on closed channel
        let message = MultiplexMessage {
            id,
            payload: payload.ok_or(Error::new(
                "response channel dropped before sending response",
            ))?,
        };

        self.port
            .send(&message)
            .context("failed to send outgoing response ")?;

        Ok(())
    }
}

/*
pub struct MultiplexWorker<Tx, Rx>
where
    Tx: Serialize,
    Rx: Serialize,
{
    /// Port over which communication takes place
    port: Port,
    /// Queued messages from the onmessage callback (both requests and responses)
    incoming_messages: mpsc::UnboundedReceiver<MultiplexMessage<Rx>>,
    /// MessageId to be used for the next outgoing request
    next_message_index: MessageId,
    /// Queued requests to be send
    outgoing_requests: mpsc::Receiver<(Tx, oneshot::Sender<Rx>)>,
    /// Map of message ids waiting for response to oneshot channels to send the response over
    incoming_responses_map: HashMap<MessageId, oneshot::Sender<Rx>>,
    /// Cancellation token to stop the worker
    cancellation_token: CancellationToken,

    outgoing_responses_map: FuturesUnordered<LocalBoxFuture<'static, (MessageId, Option<Tx>)>>,
    recv_queue: mpsc::Sender<(Rx, oneshot::Sender<Tx>)>,
}

impl<Tx, Rx> MultiplexWorker<Tx, Rx>
where
    Tx: Serialize + DeserializeOwned + 'static,
    Rx: Serialize + DeserializeOwned + 'static,
{
    fn new(
        port: JsValue,
        send_queue: mpsc::Receiver<(Tx, oneshot::Sender<Rx>)>,
        recv_queue: mpsc::Sender<(Rx, oneshot::Sender<Tx>)>,
        cancellation_token: CancellationToken,
    ) -> Result<MultiplexWorker<Tx, Rx>> {
        let (incoming_messages_tx, incoming_messages) = mpsc::unbounded_channel();

        let port = Port::new_with_channel(port, incoming_messages_tx)?;

        Ok(MultiplexWorker {
            port,
            incoming_messages,
            next_message_index: Default::default(),
            outgoing_requests: send_queue,
            incoming_responses_map: Default::default(),
            cancellation_token,

            outgoing_responses_map: Default::default(),
            recv_queue,
        })
    }

    async fn run(&mut self) -> Result<()> {
        loop {
            select! {
                _ = self.cancellation_token.cancelled() => {
                    return Ok(())
                }
                msg = self.incoming_messages.recv() => {
                    let Some(MultiplexMessage {id, payload, direction }) = msg else {
                        return Err(Error::new("Incoming message channel closed, should not happen"));
                    };
                    match direction {
                        MultiplexMessageIntent::Request => self.handle_incoming_request(id, payload).await?,
                        MultiplexMessageIntent::Response => self.handle_incoming_response(id, payload),
                    };
                }
                event = self.outgoing_requests.recv() => {
                    let Some((msg, response_tx)) = event else {
                        return Err(Error::new("Outgoing requests channel closed, should not happen"));
                    };
                    self.handle_outgoing_request(msg, response_tx)?;
                }
                event = self.outgoing_responses_map.next() => {
                    let Some((mid, response)) = event else {
                        return Err(Error::new("Outgoing responses channel closed, should not happen"));
                    };
                    self.handle_outgoing_response(mid, response)?;
                }
            }
        }
    }

    async fn handle_incoming_request(&mut self, mid: MessageId, payload: Rx) -> Result<()> {
        let (response_tx, response_rx) = oneshot::channel();

        self.recv_queue
            .send((payload, response_tx))
            .await
            .context("forwarding received request failed, no receiver waiting")?;

        let tagged_response = response_rx.map(move |r| (mid, r.ok())).boxed_local();

        self.outgoing_responses_map.push(tagged_response);

        Ok(())
    }

    fn handle_incoming_response(&mut self, id: MessageId, payload: Rx) {
        let Some(response_sender) = self.incoming_responses_map.remove(&id) else {
            warn!("received unsolicited response for {id:?}, ignoring");
            return;
        };

        if let Err(e) = response_sender.send(payload) {
            warn!("receiver for {id:?} dropped");
        }
    }

    fn handle_outgoing_request(
        &mut self,
        payload: Tx,
        response_tx: oneshot::Sender<Rx>,
    ) -> Result<()> {
        let mid = self.next_message_index.post_increment();
        let message = MultiplexMessage {
            id: mid,
            payload,
            direction: MultiplexMessageIntent::Request,
        };

        self.port
            .send(&message)
            .context("failed to send outgoing request")?;

        if self
            .incoming_responses_map
            .insert(mid, response_tx)
            .is_some()
        {
            return Err(Error::new("collision in message ids, should not happen"));
        }

        Ok(())
    }

    fn handle_outgoing_response(&mut self, id: MessageId, payload: Option<Tx>) -> Result<()> {
        let message = MultiplexMessage {
            id,
            payload: payload.ok_or(Error::new(
                "response channel dropped before sending response",
            ))?,
            direction: MultiplexMessageIntent::Response,
        };

        self.port
            .send(&message)
            .context("failed to send outgoing response ")?;

        Ok(())
    }
}
*/

/*
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

*/

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
        let client = Client::<i32, i32>::start(channel.port1().into()).unwrap();

        let (request_tx, mut request_rx) = mpsc::unbounded_channel();
        let (port_tx, _) = mpsc::unbounded_channel();
        let server_connection =
            ServerConnection::<i32, i32>::start(channel.port2().into(), request_tx, port_tx)
                .unwrap();

        let response = client.send(42, None).await.unwrap();

        let (request, responder) = request_rx.recv().await.expect("failedd to recv");
        assert_eq!(request, 42);
        responder.send(43).unwrap();

        assert_eq!(response.await.unwrap(), 43);
        client.stop().await;
        server_connection.stop().await;
    }

    #[wasm_bindgen_test]
    async fn multiple_channels() {
        let channel = MessageChannel::new().unwrap();
        let client = Client::<i32, String>::start(channel.port1().into()).unwrap();

        let (request_tx, mut request_rx) = mpsc::unbounded_channel();
        let (port_tx, _) = mpsc::unbounded_channel();
        let server_connection =
            ServerConnection::<i32, String>::start(channel.port2().into(), request_tx, port_tx)
                .unwrap();

        let mut responses = vec![
            Some(client.send(0, None).await.unwrap()),
            Some(client.send(1, None).await.unwrap()),
            Some(client.send(2, None).await.unwrap()),
            Some(client.send(3, None).await.unwrap()),
            Some(client.send(4, None).await.unwrap()),
            Some(client.send(5, None).await.unwrap()),
        ];

        for i in 0..=5 {
            let (request, responder) = request_rx.recv().await.unwrap();
            assert_eq!(i, request);
            responder.send(format!("R:{request}")).unwrap();
        }

        for i in (0..=5).rev() {
            assert_eq!(
                responses[i].take().unwrap().await.unwrap(),
                format!("R:{i}")
            );
        }
        client.stop().await;
        server_connection.stop().await;
    }

    #[wasm_bindgen_test]
    async fn client_server() {
        let channel = MessageChannel::new().unwrap();
        let mut server = Server::<i32, i32>::new();
        let port_channel = server.get_port_channel();
        port_channel.send(channel.port2().into()).unwrap();

        let client = Client::<i32, i32>::start(channel.port1().into()).unwrap();

        let response = client.send(1, None).await.unwrap();

        let (request, responder) = server.recv().await.unwrap();
        assert_eq!(request, 1);
        responder.send(2).unwrap();

        assert_eq!(response.await.unwrap(), 2);
    }
}
