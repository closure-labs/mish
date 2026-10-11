//! Native serial adapter for the documented Meshtastic client protocol.
//! https://meshtastic.org/docs/development/device/client-api/
//! These independently written partial wire types express protocol field numbers;
//! no upstream generated bindings, schemas, or GPL implementation are included.
//! A completed serial write is submission, never proof of RF or human receipt.

use crate::channel_store::channel_fingerprint;
use crate::coordinator::Coordinator;
use crate::domain::{MessageKind, TransmissionUpdate};
use crate::provision::{DeviceChannel, decode_channel, decode_lora_config};
use anyhow::{Context, Result, bail};
use prost::Message;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    future::Future,
    pin::Pin,
    sync::{Arc, Mutex},
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tokio::{
    io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, WriteHalf},
    sync::{mpsc, watch},
    task::JoinHandle,
};
use tokio_serial::SerialPortBuilderExt;

const MAX_FRAME: usize = 512;
const MAX_TEXT: usize = 200;
const INGRESS_CAPACITY: usize = 1024;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Adapter {
    Disabled,
    Simulator,
    Meshtastic,
}
impl Adapter {
    fn as_str(self) -> &'static str {
        match self {
            Self::Disabled => "disabled",
            Self::Simulator => "simulator",
            Self::Meshtastic => "meshtastic",
        }
    }
}

#[derive(Clone, PartialEq, Message)]
struct EnvelopeOut {
    #[prost(message, optional, tag = "1")]
    packet: Option<WirePacket>,
    #[prost(uint32, tag = "3")]
    config_request: u32,
    #[prost(message, optional, tag = "7")]
    heartbeat: Option<Heartbeat>,
}
#[derive(Clone, PartialEq, Message)]
struct Heartbeat {
    #[prost(uint32, tag = "1")]
    nonce: u32,
}
#[derive(Clone, PartialEq, Message)]
struct EnvelopeIn {
    #[prost(message, optional, tag = "2")]
    packet: Option<WirePacket>,
    #[prost(message, optional, tag = "3")]
    identity: Option<NodeIdentity>,
    #[prost(uint32, tag = "7")]
    configured: u32,
    #[prost(bytes = "vec", optional, tag = "5")]
    config: Option<Vec<u8>>,
    #[prost(bool, tag = "8")]
    rebooted: bool,
    #[prost(bytes = "vec", optional, tag = "10")]
    channel: Option<Vec<u8>>,
}
#[derive(Clone, PartialEq, Message)]
struct NodeIdentity {
    #[prost(uint32, tag = "1")]
    node: u32,
}
#[derive(Clone, PartialEq, Message)]
struct WirePacket {
    #[prost(fixed32, tag = "1")]
    sender: u32,
    #[prost(fixed32, tag = "2")]
    destination: u32,
    #[prost(uint32, tag = "3")]
    channel: u32,
    #[prost(message, optional, tag = "4")]
    data: Option<WireData>,
    #[prost(fixed32, tag = "6")]
    id: u32,
    #[prost(bool, tag = "10")]
    request_ack: bool,
    #[prost(bool, tag = "17")]
    pki_encrypted: bool,
}
#[derive(Clone, PartialEq, Message)]
struct WireData {
    #[prost(uint32, tag = "1")]
    port: u32,
    #[prost(bytes = "vec", tag = "2")]
    payload: Vec<u8>,
    #[prost(fixed32, tag = "6")]
    request: u32,
}
#[derive(Clone, PartialEq, Message)]
struct Routing {
    #[prost(bytes = "vec", optional, tag = "1")]
    route_request: Option<Vec<u8>>,
    #[prost(bytes = "vec", optional, tag = "2")]
    route_reply: Option<Vec<u8>>,
    #[prost(uint32, optional, tag = "3")]
    error: Option<u32>,
}

fn wall_time() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs_f64()
}
fn nonzero_id() -> u32 {
    loop {
        let id = rand::random();
        if id != 0 {
            return id;
        }
    }
}
fn framed(message: &impl Message) -> Result<Vec<u8>> {
    let bytes = message.encode_to_vec();
    if bytes.len() > MAX_FRAME {
        bail!("radio frame exceeds 512 bytes");
    }
    let mut frame = vec![0x94, 0xc3, (bytes.len() >> 8) as u8, bytes.len() as u8];
    frame.extend(bytes);
    Ok(frame)
}

/// Constant-space parser tolerates radio boot text, partial frames and bad lengths.
#[derive(Default)]
struct Framer {
    bytes: Vec<u8>,
}
impl Framer {
    fn push(&mut self, byte: u8) -> Option<Vec<u8>> {
        self.bytes.push(byte);
        loop {
            if self.bytes.first() != Some(&0x94) {
                self.bytes.clear();
                return None;
            }
            if self.bytes.len() == 1 {
                return None;
            }
            if self.bytes[1] != 0xc3 {
                self.bytes.remove(0);
                continue;
            }
            if self.bytes.len() < 4 {
                return None;
            }
            let size = usize::from(u16::from_be_bytes([self.bytes[2], self.bytes[3]]));
            if size > MAX_FRAME {
                self.bytes.remove(0);
                continue;
            }
            if self.bytes.len() < size + 4 {
                return None;
            }
            let packet = self.bytes[4..].to_vec();
            self.bytes.clear();
            return Some(packet);
        }
    }
}

#[derive(Clone, Debug)]
pub enum RadioEvent {
    Text {
        packet_id: String,
        node: String,
        channel: i64,
        text: String,
    },
    Confirmation(String),
}
type SendFuture<'a> = Pin<Box<dyn Future<Output = Result<String>> + Send + 'a>>;
pub trait Transport: Send {
    fn send<'a>(&'a mut self, text: &'a str, channel: u32, record_id: &'a str) -> SendFuture<'a>;
    fn poll(&mut self) -> Result<Vec<RadioEvent>>;
    fn fingerprint(&self, _channel: u32) -> Option<&str> {
        None
    }
}

#[derive(Default)]
pub struct Simulator {
    pub sent: Vec<(String, u32, String)>,
    events: Vec<RadioEvent>,
}
impl Simulator {
    pub fn inject(&mut self, event: RadioEvent) -> bool {
        if self.events.len() >= INGRESS_CAPACITY {
            return false;
        }
        self.events.push(event);
        true
    }
}
impl Transport for Simulator {
    fn send<'a>(&'a mut self, text: &'a str, channel: u32, record_id: &'a str) -> SendFuture<'a> {
        Box::pin(async move {
            let id = format!(
                "sim-{}",
                &hex::encode(Sha256::digest(record_id.as_bytes()))[..16]
            );
            if self.sent.len() >= INGRESS_CAPACITY {
                self.sent.remove(0);
            }
            self.sent.push((text.into(), channel, id.clone()));
            Ok(id)
        })
    }
    fn poll(&mut self) -> Result<Vec<RadioEvent>> {
        Ok(std::mem::take(&mut self.events))
    }
}

fn event(packet: WirePacket, channel: u32, own_node: u32) -> Option<RadioEvent> {
    let data = packet.data?;
    if data.port == 5 && data.request != 0 {
        // Firmware allocAckNak addresses the original sender on its channel;
        // ReliableRouter's local implicit broadcast ACK preserves that channel.
        // https://github.com/meshtastic/firmware/blob/develop/src/mesh/MeshModule.cpp
        // https://github.com/meshtastic/firmware/blob/develop/src/mesh/ReliableRouter.cpp
        // Local implicit ACKs legitimately have our own node as their sender.
        if own_node == 0
            || packet.channel != channel
            || packet.destination != own_node
            || packet.sender == 0
            || packet.id == 0
        {
            return None;
        }
        let routing = Routing::decode(data.payload.as_slice()).ok()?;
        if routing.route_request.is_none()
            && routing.route_reply.is_none()
            && routing.error.unwrap_or(0) == 0
        {
            return Some(RadioEvent::Confirmation(format!(
                "{channel}:{}",
                data.request
            )));
        }
        return None;
    }
    if data.port != 1
        || packet.channel != channel
        || packet.id == 0
        || packet.sender == 0
        || packet.sender == own_node
        || packet.destination != u32::MAX
        || packet.pki_encrypted
        || data.payload.len() > MAX_TEXT
    {
        return None;
    }
    let text = String::from_utf8(data.payload).ok()?;
    if text.trim().is_empty() {
        return None;
    }
    Some(RadioEvent::Text {
        packet_id: packet.id.to_string(),
        node: format!("!{:08x}", packet.sender),
        channel: i64::from(packet.channel),
        text,
    })
}

struct SerialConnection<S: AsyncRead + AsyncWrite + Send + Unpin + 'static> {
    writer: WriteHalf<S>,
    receiver: mpsc::Receiver<EnvelopeIn>,
    reader: JoinHandle<()>,
    own_node: u32,
    channel: u32,
    bindings: HashMap<u32, String>,
}
impl<S: AsyncRead + AsyncWrite + Send + Unpin + 'static> Drop for SerialConnection<S> {
    fn drop(&mut self) {
        self.reader.abort();
    }
}
impl<S: AsyncRead + AsyncWrite + Send + Unpin + 'static> SerialConnection<S> {
    #[cfg(test)]
    async fn connect(stream: S, channel: u32, store: &Coordinator) -> Result<Self> {
        Self::connect_checked(stream, channel, store, false).await
    }
    async fn connect_checked(
        stream: S,
        channel: u32,
        store: &Coordinator,
        require_registry: bool,
    ) -> Result<Self> {
        let registered = store.radio_channels().await?;
        if require_registry && registered.is_empty() {
            bail!("native radio requires a commissioned channel registry");
        }
        let (mut read, writer) = tokio::io::split(stream);
        let (tx, receiver) = mpsc::channel(INGRESS_CAPACITY);
        let reader = tokio::spawn(async move {
            let mut parser = Framer::default();
            while let Ok(byte) = read.read_u8().await {
                if let Some(bytes) = parser.push(byte) {
                    if let Ok(message) = EnvelopeIn::decode(bytes.as_slice()) {
                        if let Err(mpsc::error::TrySendError::Closed(_)) = tx.try_send(message) {
                            break;
                        }
                        // Overflow drops input without inventing delivery success.
                    }
                }
            }
        });
        let mut connection = Self {
            writer,
            receiver,
            reader,
            own_node: 0,
            channel,
            bindings: HashMap::new(),
        };
        let nonce = nonzero_id();
        connection
            .write(&EnvelopeOut {
                config_request: nonce,
                ..Default::default()
            })
            .await?;
        let mut channels: HashMap<u32, DeviceChannel> = HashMap::new();
        let mut lora = None;
        tokio::time::timeout(Duration::from_secs(15), async {
            loop {
                let message = connection
                    .receiver
                    .recv()
                    .await
                    .context("radio disconnected during configuration")?;
                if let Some(identity) = message.identity {
                    connection.own_node = identity.node;
                }
                if let Some(raw) = message.channel {
                    let observed = decode_channel(&raw)?;
                    channels.insert(observed.index, observed);
                }
                if let Some(raw) = message.config {
                    if let Some(observed) = decode_lora_config(&raw)? {
                        lora = Some(observed);
                    }
                }
                // Traffic arriving before the completed verified snapshot is discarded.
                if message.configured == nonce && connection.own_node != 0 {
                    for registered in &registered {
                        let observed = channels
                            .get(&registered.metadata.slot)
                            .context("radio did not report a registered channel")?;
                        if observed.role != if observed.index == 0 { 1 } else { 2 } {
                            bail!("radio channel role differs from registry");
                        }
                        let settings = observed
                            .settings
                            .as_deref()
                            .context("radio channel has no settings")?;
                        let observed_lora = lora
                            .as_deref()
                            .context("radio did not report LoRa configuration")?;
                        let fingerprint =
                            channel_fingerprint(settings, Some(observed_lora), observed.index);
                        store
                            .validate_binding(&registered.metadata.id, &fingerprint)
                            .await?;
                        connection.bindings.insert(observed.index, fingerprint);
                    }
                    return Ok::<_, anyhow::Error>(());
                }
            }
        })
        .await
        .context("radio configuration timed out")??;
        Ok(connection)
    }
    async fn write(&mut self, message: &impl Message) -> Result<()> {
        let frame = framed(message)?;
        tokio::time::timeout(Duration::from_secs(5), async {
            self.writer.write_all(&frame).await?;
            self.writer.flush().await
        })
        .await
        .context("serial write timed out")??;
        Ok(())
    }
    async fn heartbeat(&mut self) -> Result<()> {
        self.write(&EnvelopeOut {
            heartbeat: Some(Heartbeat {
                nonce: nonzero_id(),
            }),
            ..Default::default()
        })
        .await
    }
}
impl<S: AsyncRead + AsyncWrite + Send + Unpin + 'static> Transport for SerialConnection<S> {
    fn send<'a>(&'a mut self, text: &'a str, channel: u32, _record_id: &'a str) -> SendFuture<'a> {
        Box::pin(async move {
            let id = nonzero_id();
            let packet = WirePacket {
                sender: self.own_node,
                destination: u32::MAX,
                channel,
                id,
                request_ack: true,
                pki_encrypted: false,
                data: Some(WireData {
                    port: 1,
                    payload: text.as_bytes().to_vec(),
                    ..Default::default()
                }),
            };
            self.write(&EnvelopeOut {
                packet: Some(packet),
                ..Default::default()
            })
            .await?;
            Ok(format!("{channel}:{id}"))
        })
    }
    fn poll(&mut self) -> Result<Vec<RadioEvent>> {
        let mut events = Vec::new();
        loop {
            match self.receiver.try_recv() {
                Ok(message) => {
                    if message.rebooted || message.channel.is_some() || message.config.is_some() {
                        bail!("radio configuration changed; reconnect and verify before routing");
                    }
                    if message
                        .identity
                        .is_some_and(|identity| identity.node != self.own_node)
                    {
                        bail!("radio identity changed; reconnect before routing");
                    }
                    if let Some(packet) = message.packet {
                        let channel = packet.channel;
                        if !self.bindings.is_empty() && !self.bindings.contains_key(&channel) {
                            continue;
                        }
                        if self.bindings.is_empty() && channel != self.channel {
                            continue;
                        }
                        if let Some(value) = event(packet, channel, self.own_node) {
                            events.push(value);
                        }
                    }
                }
                Err(mpsc::error::TryRecvError::Empty) => return Ok(events),
                Err(mpsc::error::TryRecvError::Disconnected) => {
                    if events.is_empty() {
                        bail!("radio disconnected")
                    } else {
                        return Ok(events);
                    }
                }
            }
        }
    }
    fn fingerprint(&self, channel: u32) -> Option<&str> {
        self.bindings.get(&channel).map(String::as_str)
    }
}

async fn apply(store: &Coordinator, event: RadioEvent, channel: u32) -> Result<()> {
    match event {
        RadioEvent::Confirmation(id) => {
            store.transport_confirmation(&id).await?;
        }
        RadioEvent::Text {
            packet_id,
            node,
            channel: incoming,
            text,
        } => {
            let registered = store.radio_channels().await?;
            let admitted = if registered.is_empty() {
                incoming == i64::from(channel)
            } else {
                match u32::try_from(incoming) {
                    Ok(slot) => store
                        .route_radio_channel(slot)
                        .await?
                        .is_some_and(|binding| binding.verified),
                    Err(_) => false,
                }
            };
            if admitted && !text.trim().is_empty() && text.len() <= MAX_TEXT {
                store.receive(&packet_id, &node, &text, incoming).await?;
            }
        }
    }
    Ok(())
}

/// Testable one-coordinator scheduling state. Wall time handles record expiry;
/// monotonic time handles pacing and bounded exponential connection backoff.
pub struct Worker {
    pub adapter: String,
    mode: Adapter,
    pub status: Arc<Mutex<String>>,
    store: Coordinator,
    channel: u32,
    device: String,
    interval: f64,
    next_send: f64,
    pub retry_at: f64,
    backoff: f64,
}
impl Worker {
    pub fn new(store: Coordinator, config: &Value) -> Result<Self> {
        let settings = &config["radio"]["meshtastic"];
        let mode = match &config["radio"]["adapter"] {
            Value::Null => {
                if settings["enable"].as_bool().unwrap_or(false) {
                    Adapter::Meshtastic
                } else {
                    Adapter::Disabled
                }
            }
            Value::String(value) if value == "disabled" => Adapter::Disabled,
            Value::String(value) if value == "simulator" => Adapter::Simulator,
            Value::String(value) if value == "meshtastic" => Adapter::Meshtastic,
            _ => bail!("unknown radio adapter"),
        };
        let adapter = mode.as_str().to_owned();
        let channel = if settings["channel"].is_null() {
            0
        } else {
            settings["channel"]
                .as_u64()
                .context("radio channel must be integer 0..7")?
        };
        if channel > 7 {
            bail!("radio channel must be integer 0..7");
        }
        let interval = if settings["sendIntervalSeconds"].is_null() {
            if adapter == "meshtastic" { 30.0 } else { 0.0 }
        } else {
            settings["sendIntervalSeconds"]
                .as_f64()
                .context("radio send interval must be numeric")?
        };
        if !interval.is_finite() || interval < 0.0 || (adapter == "meshtastic" && interval == 0.0) {
            bail!("radio send interval must be finite and positive (simulator permits zero)");
        }
        let device = settings["device"].as_str().unwrap_or_default().to_owned();
        if adapter == "meshtastic"
            && (std::path::Path::new(&device).parent()
                != Some(std::path::Path::new("/dev/serial/by-id"))
                || std::path::Path::new(&device).file_name().is_none())
        {
            bail!("radio device must use /dev/serial/by-id/");
        }
        let status = Arc::new(Mutex::new(
            if adapter == "disabled" {
                "disabled"
            } else {
                "disconnected"
            }
            .to_owned(),
        ));
        Ok(Self {
            adapter,
            mode,
            status,
            store,
            channel: channel as u32,
            device,
            interval,
            next_send: 0.0,
            retry_at: 0.0,
            backoff: 1.0,
        })
    }
    pub fn disconnected(&mut self, now: f64) {
        self.retry_at = now + self.backoff;
        self.backoff = (self.backoff * 2.0).min(60.0);
        *self.status.lock().unwrap() = "disconnected".into();
    }
    pub async fn step<T: Transport>(&mut self, transport: &mut T, now: f64) -> Result<bool> {
        if self.mode == Adapter::Disabled {
            return Ok(true);
        }
        for value in transport.poll()? {
            apply(&self.store, value, self.channel).await?;
        }
        for record in self.store.pending().await? {
            if now < self.next_send {
                break;
            }
            let id = &record.id;
            if record.expires.is_some_and(|expiry| expiry <= wall_time()) {
                self.store
                    .transition(id, TransmissionUpdate::Expired)
                    .await?;
                continue;
            }
            let registered = self.store.radio_channels().await?;
            let send_channel = if registered.is_empty() {
                if self.mode == Adapter::Meshtastic {
                    bail!("native radio requires commissioned channel bindings");
                }
                self.channel
            } else {
                let Some(fingerprint) = record.channel_fingerprint.as_deref() else {
                    continue;
                };
                let Ok(binding) = self
                    .store
                    .validate_binding(&record.channel_id, fingerprint)
                    .await
                else {
                    continue;
                };
                if self.mode == Adapter::Meshtastic
                    && transport.fingerprint(binding.slot) != Some(fingerprint)
                {
                    bail!("live radio channel differs from durable message binding");
                }
                binding.slot
            };
            let wire_text = match record.kind {
                MessageKind::Alert | MessageKind::Urgent => format!(
                    "{} {}: {}",
                    record.kind.as_str().to_uppercase(),
                    id,
                    record.text
                ),
                MessageKind::Message => record.text,
            };
            if wire_text.len() > MAX_TEXT {
                self.store
                    .transition(id, TransmissionUpdate::Failed)
                    .await?;
                continue;
            }
            if !self.store.transition(id, TransmissionUpdate::Claim).await? {
                continue;
            }
            self.next_send = now + self.interval;
            match transport.send(&wire_text, send_channel, id).await {
                Ok(packet_id) => {
                    self.store
                        .transition(id, TransmissionUpdate::Submitted { packet_id })
                        .await?;
                    self.backoff = 1.0;
                }
                Err(error) => {
                    self.store
                        .transition(id, TransmissionUpdate::Uncertain)
                        .await?;
                    self.disconnected(now);
                    eprintln!("radio submission uncertain: {error}; automatic retry suppressed");
                    return Ok(false);
                }
            }
        }
        Ok(true)
    }
}

pub struct RadioHandle {
    pub adapter: String,
    pub status: Arc<Mutex<String>>,
    shutdown: watch::Sender<bool>,
    task: JoinHandle<()>,
}
impl RadioHandle {
    pub async fn stop(self) {
        let _ = self.shutdown.send(true);
        let mut task = self.task;
        if tokio::time::timeout(Duration::from_secs(6), &mut task)
            .await
            .is_err()
        {
            task.abort();
            let _ = task.await;
        }
    }
}

pub fn start(store: Coordinator, config: &Value) -> Result<RadioHandle> {
    let mut worker = Worker::new(store, config)?;
    let adapter = worker.adapter.clone();
    let status = worker.status.clone();
    let (shutdown, mut receiver) = watch::channel(false);
    let task = tokio::spawn(async move {
        let began = tokio::time::Instant::now();
        let mut simulator = Simulator::default();
        let mut serial = None;
        let mut heartbeat_at = 0.0;
        while !*receiver.borrow() {
            let now = began.elapsed().as_secs_f64();
            if worker.mode == Adapter::Simulator {
                *worker.status.lock().unwrap() = "simulated".into();
                if let Err(error) = worker.step(&mut simulator, now).await {
                    eprintln!("radio worker failed: {error}");
                    worker.disconnected(now);
                }
            } else if worker.mode == Adapter::Meshtastic {
                if serial.is_none() && now >= worker.retry_at {
                    *worker.status.lock().unwrap() = "configuring".into();
                    let opened = tokio_serial::new(&worker.device, 115200).open_native_async();
                    let connected = match opened {
                        Ok(stream) => {
                            tokio::select! { result = SerialConnection::connect_checked(stream, worker.channel, &worker.store, true) => Some(result), _ = receiver.changed() => None }
                        }
                        Err(error) => Some(Err(error.into())),
                    };
                    match connected {
                        Some(Ok(connection)) => {
                            serial = Some(connection);
                            heartbeat_at = now + 30.0;
                            *worker.status.lock().unwrap() = "connected".into();
                        }
                        Some(Err(error)) => {
                            eprintln!("radio connection failed: {error}");
                            worker.disconnected(now);
                        }
                        None => break,
                    }
                }
                let now = began.elapsed().as_secs_f64();
                if let Some(connection) = serial.as_mut() {
                    let result = worker.step(connection, now).await;
                    if !matches!(result, Ok(true)) {
                        serial = None;
                        if !matches!(result, Ok(false)) {
                            worker.disconnected(now);
                        }
                    } else if now >= heartbeat_at {
                        if connection.heartbeat().await.is_err() {
                            serial = None;
                            worker.disconnected(now);
                        }
                        heartbeat_at = now + 30.0;
                    }
                }
            }
            tokio::select! { _ = tokio::time::sleep(Duration::from_millis(500)) => {}, _ = receiver.changed() => break }
        }
    });
    Ok(RadioHandle {
        adapter,
        status,
        shutdown,
        task,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        appliance::EventStore,
        domain::{MessageText, NewRecord, StaffName, TransmissionState},
    };
    use serde_json::json;

    fn fixture() -> (tempfile::TempDir, Coordinator) {
        let directory = tempfile::tempdir().unwrap();
        let store = EventStore::new(directory.path(), 1000, true).unwrap();
        let coordinator = Coordinator::start(store, 7);
        (directory, coordinator)
    }
    fn simulator_config() -> Value {
        json!({"radio":{"adapter":"simulator","meshtastic":{"channel":2}}})
    }
    fn registered_fixture() -> (
        tempfile::TempDir,
        Coordinator,
        Vec<crate::channel_store::RadioChannel>,
    ) {
        use crate::channel_store::{ChannelPlan, ChannelScope, PlannedChannel};
        let directory = tempfile::tempdir().unwrap();
        let store = EventStore::new(directory.path(), 1000, true).unwrap();
        let channels = [
            ("community", "AstralMesh", ChannelScope::Community),
            ("staff", "Staff", ChannelScope::Staff),
            ("medical", "Medical", ChannelScope::Department),
        ]
        .into_iter()
        .enumerate()
        .map(|(slot, (id, name, scope))| PlannedChannel {
            id: id.into(),
            scope,
            slot: slot as u32,
            settings: crate::channels::generated(name).unwrap().settings,
        })
        .collect();
        store
            .install_channel_plan(&ChannelPlan {
                channels,
                lora: Some(vec![0x08, 0x01]),
            })
            .unwrap();
        let observed: Vec<_> = store
            .radio_channels()
            .unwrap()
            .into_iter()
            .map(|channel| (channel.metadata.slot, channel.metadata.fingerprint))
            .collect();
        store.mark_deployment_verified(&observed).unwrap();
        let material = store.radio_channels().unwrap();
        (directory, Coordinator::start(store, 7), material)
    }

    struct VerifiedTransport {
        simulator: Simulator,
        bindings: HashMap<u32, String>,
    }
    impl Transport for VerifiedTransport {
        fn send<'a>(&'a mut self, text: &'a str, channel: u32, id: &'a str) -> SendFuture<'a> {
            self.simulator.send(text, channel, id)
        }
        fn poll(&mut self) -> Result<Vec<RadioEvent>> {
            self.simulator.poll()
        }
        fn fingerprint(&self, slot: u32) -> Option<&str> {
            self.bindings.get(&slot).map(String::as_str)
        }
    }

    #[tokio::test]
    async fn durable_channel_binding_selects_correct_slot_and_blocks_changed_live_key() {
        let (_directory, store, material) = registered_fixture();
        let staff = message(&store, "Staff work").await;
        let medical = store
            .create(
                NewRecord::message(MessageText::new("Medical work").unwrap(), "staff")
                    .unwrap()
                    .on_channel("medical")
                    .unwrap(),
            )
            .await
            .unwrap();
        let config = json!({"radio":{"adapter":"meshtastic","meshtastic":{"device":"/dev/serial/by-id/fake","sendIntervalSeconds":30}}});
        let mut worker = Worker::new(store.clone(), &config).unwrap();
        let mut transport = VerifiedTransport {
            simulator: Simulator::default(),
            bindings: material
                .iter()
                .map(|channel| (channel.metadata.slot, channel.metadata.fingerprint.clone()))
                .collect(),
        };
        worker.step(&mut transport, 0.0).await.unwrap();
        assert_eq!(transport.simulator.sent[0].1, 1);
        assert_eq!(
            record(&store, &staff.id).await.state,
            TransmissionState::Submitted
        );
        transport.bindings.insert(2, "changed-key".into());
        assert!(worker.step(&mut transport, 30.0).await.is_err());
        assert_eq!(transport.simulator.sent.len(), 1);
        assert_eq!(
            record(&store, &medical.id).await.state,
            TransmissionState::Queued
        );
        transport
            .bindings
            .insert(2, material[2].metadata.fingerprint.clone());
        worker.step(&mut transport, 31.0).await.unwrap();
        assert_eq!(transport.simulator.sent[1].1, 2);
    }

    #[derive(Clone, PartialEq, Message)]
    struct ReadbackChannel {
        #[prost(uint32, tag = "1")]
        index: u32,
        #[prost(bytes = "vec", optional, tag = "2")]
        settings: Option<Vec<u8>>,
        #[prost(int32, tag = "3")]
        role: i32,
    }
    #[derive(Clone, PartialEq, Message)]
    struct ReadbackConfig {
        #[prost(bytes = "vec", optional, tag = "6")]
        lora: Option<Vec<u8>>,
    }

    #[tokio::test]
    async fn handshake_verifies_actual_settings_and_refuses_wrong_private_key() {
        let (_directory, store, material) = registered_fixture();
        let (client, mut radio) = tokio::io::duplex(4096);
        let device = tokio::spawn(async move {
            let request = EnvelopeOut::decode(read_message(&mut radio).await.as_slice()).unwrap();
            radio
                .write_all(
                    &framed(&EnvelopeIn {
                        identity: Some(NodeIdentity { node: 9 }),
                        ..Default::default()
                    })
                    .unwrap(),
                )
                .await
                .unwrap();
            radio
                .write_all(
                    &framed(&EnvelopeIn {
                        config: Some(
                            ReadbackConfig {
                                lora: material[0].lora.clone(),
                            }
                            .encode_to_vec(),
                        ),
                        ..Default::default()
                    })
                    .unwrap(),
                )
                .await
                .unwrap();
            for channel in material {
                let settings = if channel.metadata.slot == 1 {
                    crate::channels::generated("Staff").unwrap().settings
                } else {
                    channel.settings
                };
                radio
                    .write_all(
                        &framed(&EnvelopeIn {
                            channel: Some(
                                ReadbackChannel {
                                    index: channel.metadata.slot,
                                    role: if channel.metadata.slot == 0 { 1 } else { 2 },
                                    settings: Some(settings),
                                }
                                .encode_to_vec(),
                            ),
                            ..Default::default()
                        })
                        .unwrap(),
                    )
                    .await
                    .unwrap();
            }
            radio
                .write_all(
                    &framed(&EnvelopeIn {
                        configured: request.config_request,
                        ..Default::default()
                    })
                    .unwrap(),
                )
                .await
                .unwrap();
        });
        assert!(
            SerialConnection::connect_checked(client, 1, &store, true)
                .await
                .is_err()
        );
        device.await.unwrap();
    }

    #[test]
    fn direct_and_pki_messages_cannot_become_public_or_department_broadcasts() {
        let broadcast = WirePacket {
            sender: 10,
            destination: u32::MAX,
            id: 42,
            channel: 0,
            data: Some(WireData {
                port: 1,
                payload: b"Public hello".to_vec(),
                ..Default::default()
            }),
            ..Default::default()
        };
        assert!(event(broadcast.clone(), 0, 9).is_some());
        assert!(
            event(
                WirePacket {
                    destination: 9,
                    ..broadcast.clone()
                },
                0,
                9
            )
            .is_none()
        );
        assert!(
            event(
                WirePacket {
                    pki_encrypted: true,
                    ..broadcast
                },
                0,
                9
            )
            .is_none()
        );
    }
    async fn message(store: &Coordinator, text: &str) -> crate::domain::Record {
        store
            .create(NewRecord::message(MessageText::new(text).unwrap(), "staff").unwrap())
            .await
            .unwrap()
    }
    async fn record(store: &Coordinator, id: &str) -> crate::domain::Record {
        store.record(id).await.unwrap().unwrap()
    }
    async fn read_message<S: AsyncRead + Unpin>(reader: &mut S) -> Vec<u8> {
        let mut parser = Framer::default();
        loop {
            if let Some(bytes) = parser.push(reader.read_u8().await.unwrap()) {
                return bytes;
            }
        }
    }

    #[test]
    fn framing_resynchronizes_and_never_allocates_unbounded_input() {
        let encoded = framed(&EnvelopeIn {
            configured: 300,
            ..Default::default()
        })
        .unwrap();
        let mut parser = Framer::default();
        let mut packets = Vec::new();
        for byte in b"boot text\n"
            .iter()
            .copied()
            .chain([0x94, 0x94, 0xc3, 0xff, 0xff])
            .chain(encoded)
        {
            if let Some(bytes) = parser.push(byte) {
                packets.push(bytes);
            }
            assert!(parser.bytes.len() <= MAX_FRAME + 4);
        }
        assert_eq!(packets.len(), 1);
        assert_eq!(
            EnvelopeIn::decode(packets[0].as_slice())
                .unwrap()
                .configured,
            300
        );
        let huge = EnvelopeOut {
            packet: Some(WirePacket {
                data: Some(WireData {
                    payload: vec![0; 600],
                    ..Default::default()
                }),
                ..Default::default()
            }),
            ..Default::default()
        };
        assert!(framed(&huge).is_err());
    }

    #[test]
    fn packet_decoder_filters_private_channel_and_preserves_identity() {
        let packet = WirePacket {
            sender: 0x1234,
            destination: u32::MAX,
            id: 77,
            channel: 2,
            data: Some(WireData {
                port: 1,
                payload: b"ACK ABCD1234".to_vec(),
                ..Default::default()
            }),
            ..Default::default()
        };
        assert!(event(packet.clone(), 0, 9).is_none());
        assert!(event(packet.clone(), 2, 0x1234).is_none());
        match event(packet, 2, 9).unwrap() {
            RadioEvent::Text {
                packet_id,
                node,
                channel,
                text,
            } => {
                assert_eq!(packet_id, "77");
                assert_eq!(node, "!00001234");
                assert_eq!(channel, 2);
                assert_eq!(text, "ACK ABCD1234");
            }
            _ => panic!("text cannot become transport confirmation"),
        }
        let routing_packet = |routing: Routing| WirePacket {
            sender: 9,
            destination: 9,
            channel: 2,
            id: 102,
            data: Some(WireData {
                port: 5,
                request: 101,
                payload: routing.encode_to_vec(),
            }),
            ..Default::default()
        };
        assert!(
            matches!(event(routing_packet(Routing { error:Some(0), ..Default::default() }),2,9),Some(RadioEvent::Confirmation(id)) if id=="2:101")
        );
        assert!(
            event(
                routing_packet(Routing {
                    error: Some(1),
                    ..Default::default()
                }),
                2,
                9
            )
            .is_none()
        );
        assert!(
            event(
                routing_packet(Routing {
                    route_reply: Some(vec![]),
                    ..Default::default()
                }),
                2,
                9
            )
            .is_none()
        );
    }

    #[tokio::test]
    async fn routing_confirmation_requires_private_channel_and_local_destination() {
        let (_directory, store) = fixture();
        let item = message(&store, "Private transmission").await;
        assert!(
            store
                .transition(&item.id, TransmissionUpdate::Claim)
                .await
                .unwrap()
        );
        store
            .transition(
                &item.id,
                TransmissionUpdate::Submitted {
                    packet_id: "2:101".into(),
                },
            )
            .await
            .unwrap();
        let local_ack = WirePacket {
            sender: 9,
            destination: 9,
            channel: 2,
            id: 102,
            data: Some(WireData {
                port: 5,
                request: 101,
                payload: Routing {
                    error: Some(0),
                    ..Default::default()
                }
                .encode_to_vec(),
            }),
            ..Default::default()
        };
        for invalid in [
            WirePacket {
                channel: 0,
                ..local_ack.clone()
            },
            WirePacket {
                destination: 10,
                ..local_ack.clone()
            },
            WirePacket {
                destination: u32::MAX,
                ..local_ack.clone()
            },
            WirePacket {
                sender: 0,
                ..local_ack.clone()
            },
            WirePacket {
                id: 0,
                ..local_ack.clone()
            },
        ] {
            if let Some(value) = event(invalid, 2, 9) {
                apply(&store, value, 2).await.unwrap();
            }
            assert_eq!(
                record(&store, &item.id).await.state,
                TransmissionState::Submitted
            );
        }
        assert!(event(local_ack.clone(), 2, 0).is_none());
        // A peer acknowledgement and our firmware's implicit rebroadcast ACK
        // have the same destination/channel contract; neither is a human ACK.
        assert!(matches!(
            event(
                WirePacket {
                    sender: 10,
                    ..local_ack.clone()
                },
                2,
                9
            ),
            Some(RadioEvent::Confirmation(_))
        ));
        apply(&store, event(local_ack, 2, 9).unwrap(), 2)
            .await
            .unwrap();
        let confirmed = record(&store, &item.id).await;
        assert_eq!(confirmed.state, TransmissionState::Confirmed);
        assert!(confirmed.acknowledgements.is_empty());
        assert!(store.pending().await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn serial_handshake_waits_for_matching_completion_and_explicit_node() {
        let (_directory, store) = fixture();
        let (client, mut radio) = tokio::io::duplex(2048);
        let radio_task = tokio::spawn(async move {
            let request = EnvelopeOut::decode(read_message(&mut radio).await.as_slice()).unwrap();
            assert_ne!(request.config_request, 0);
            assert!(request.packet.is_none());
            radio
                .write_all(
                    &framed(&EnvelopeIn {
                        configured: request.config_request.wrapping_add(1),
                        ..Default::default()
                    })
                    .unwrap(),
                )
                .await
                .unwrap();
            radio
                .write_all(
                    &framed(&EnvelopeIn {
                        identity: Some(NodeIdentity { node: 0x1234 }),
                        ..Default::default()
                    })
                    .unwrap(),
                )
                .await
                .unwrap();
            // Matching completion is mandatory before the queued message can transmit.
            radio
                .write_all(
                    &framed(&EnvelopeIn {
                        configured: request.config_request,
                        ..Default::default()
                    })
                    .unwrap(),
                )
                .await
                .unwrap();
            let sent = EnvelopeOut::decode(read_message(&mut radio).await.as_slice())
                .unwrap()
                .packet
                .unwrap();
            assert_ne!(sent.id, 0);
            assert_eq!(sent.sender, 0x1234);
            assert_eq!(sent.destination, u32::MAX);
            assert_eq!(sent.channel, 2);
            assert!(sent.request_ack);
            assert_eq!(sent.data.unwrap().payload, b"USB submission".to_vec());
            sent.id
        });
        let mut connection = SerialConnection::connect(client, 2, &store).await.unwrap();
        let id = connection
            .send("USB submission", 2, "record-id")
            .await
            .unwrap();
        assert_eq!(id, format!("2:{}", radio_task.await.unwrap()));
    }

    #[tokio::test]
    async fn serial_configuration_without_completion_cannot_send() {
        let (_directory, store) = fixture();
        let (client, mut radio) = tokio::io::duplex(512);
        tokio::spawn(async move {
            let _ = read_message(&mut radio).await; /* close mid-handshake */
        });
        assert!(SerialConnection::connect(client, 2, &store).await.is_err());
    }

    #[tokio::test]
    async fn pacing_keeps_queue_durable_and_processes_duplicate_acks_without_echo() {
        let (_directory, store) = fixture();
        let first = message(&store, "First").await;
        let alert = store
            .create(
                NewRecord::alert(
                    MessageText::new("Find lead").unwrap(),
                    "staff",
                    StaffName::new("lead").unwrap(),
                    wall_time() + 100.0,
                    true,
                    wall_time(),
                )
                .unwrap(),
            )
            .await
            .unwrap();
        let mut config = simulator_config();
        config["radio"]["meshtastic"]["sendIntervalSeconds"] = json!(30);
        let mut worker = Worker::new(store.clone(), &config).unwrap();
        let mut transport = Simulator::default();
        worker.step(&mut transport, 0.0).await.unwrap();
        assert_eq!(transport.sent.len(), 1);
        assert_eq!(
            record(&store, &alert.id).await.state,
            TransmissionState::Queued
        );
        let ack = RadioEvent::Text {
            packet_id: "42".into(),
            node: "!radio".into(),
            channel: 2,
            text: format!("ACK {}", alert.id),
        };
        transport.inject(ack.clone());
        transport.inject(ack);
        transport.inject(RadioEvent::Confirmation(transport.sent[0].2.clone()));
        worker.step(&mut transport, 1.0).await.unwrap();
        assert_eq!(transport.sent.len(), 1);
        assert_eq!(
            record(&store, &first.id).await.state,
            TransmissionState::Confirmed
        );
        let acknowledged = record(&store, &alert.id).await;
        assert_eq!(acknowledged.acknowledgements.len(), 1);
        assert_eq!(acknowledged.acknowledgements[0].node, "!radio");
        assert_eq!(acknowledged.acknowledgements[0].channel, 2);
        worker.step(&mut transport, 30.0).await.unwrap();
        assert_eq!(transport.sent.len(), 2);
        assert!(transport.sent[1].0.contains(&alert.id));
        assert_eq!(
            record(&store, &alert.id).await.state,
            TransmissionState::Submitted
        );
    }

    struct Ambiguous {
        calls: usize,
    }
    impl Transport for Ambiguous {
        fn send<'a>(&'a mut self, _text: &'a str, _channel: u32, _id: &'a str) -> SendFuture<'a> {
            Box::pin(async move {
                self.calls += 1;
                bail!("USB disconnected after partial write")
            })
        }
        fn poll(&mut self) -> Result<Vec<RadioEvent>> {
            Ok(vec![])
        }
    }
    #[tokio::test]
    async fn ambiguous_write_is_uncertain_and_cannot_be_replayed() {
        let (_directory, store) = fixture();
        let item = message(&store, "Durable").await;
        let mut worker = Worker::new(store.clone(), &simulator_config()).unwrap();
        let mut transport = Ambiguous { calls: 0 };
        assert!(!worker.step(&mut transport, 0.0).await.unwrap());
        assert_eq!(
            record(&store, &item.id).await.state,
            TransmissionState::Uncertain
        );
        worker.step(&mut transport, 100.0).await.unwrap();
        assert_eq!(transport.calls, 1);
        assert!(
            !store
                .transition(&item.id, TransmissionUpdate::Claim)
                .await
                .unwrap()
        );
    }
    struct PartialWrite {
        accepted: Arc<Mutex<Vec<u8>>>,
    }
    impl AsyncRead for PartialWrite {
        fn poll_read(
            self: Pin<&mut Self>,
            _cx: &mut std::task::Context<'_>,
            _buffer: &mut tokio::io::ReadBuf<'_>,
        ) -> std::task::Poll<std::io::Result<()>> {
            std::task::Poll::Pending
        }
    }
    impl AsyncWrite for PartialWrite {
        fn poll_write(
            self: Pin<&mut Self>,
            _cx: &mut std::task::Context<'_>,
            buffer: &[u8],
        ) -> std::task::Poll<std::io::Result<usize>> {
            let mut accepted = self.accepted.lock().unwrap();
            if accepted.is_empty() {
                accepted.extend_from_slice(&buffer[..2]);
                std::task::Poll::Ready(Ok(2))
            } else {
                std::task::Poll::Ready(Err(std::io::ErrorKind::BrokenPipe.into()))
            }
        }
        fn poll_flush(
            self: Pin<&mut Self>,
            _cx: &mut std::task::Context<'_>,
        ) -> std::task::Poll<std::io::Result<()>> {
            std::task::Poll::Ready(Ok(()))
        }
        fn poll_shutdown(
            self: Pin<&mut Self>,
            _cx: &mut std::task::Context<'_>,
        ) -> std::task::Poll<std::io::Result<()>> {
            std::task::Poll::Ready(Ok(()))
        }
    }
    #[tokio::test]
    async fn actual_serial_partial_write_leaves_claim_uncertain() {
        let (_directory, store) = fixture();
        let item = message(&store, "USB partial").await;
        let accepted = Arc::new(Mutex::new(vec![]));
        let (_read, writer) = tokio::io::split(PartialWrite {
            accepted: accepted.clone(),
        });
        let (_tx, receiver) = mpsc::channel(INGRESS_CAPACITY);
        let mut transport = SerialConnection {
            writer,
            receiver,
            reader: tokio::spawn(std::future::pending()),
            own_node: 9,
            channel: 2,
            bindings: HashMap::new(),
        };
        let mut worker = Worker::new(store.clone(), &simulator_config()).unwrap();
        assert!(!worker.step(&mut transport, 0.0).await.unwrap());
        assert_eq!(*accepted.lock().unwrap(), vec![0x94, 0xc3]);
        assert_eq!(
            record(&store, &item.id).await.state,
            TransmissionState::Uncertain
        );
        worker.step(&mut transport, 60.0).await.unwrap();
        assert_eq!(*accepted.lock().unwrap(), vec![0x94, 0xc3]);
    }
    #[tokio::test]
    async fn expired_queued_alert_never_uses_radio_airtime() {
        let (directory, store) = fixture();
        let alert = store
            .create(
                NewRecord::alert(
                    MessageText::new("Expired").unwrap(),
                    "staff",
                    StaffName::new("lead").unwrap(),
                    wall_time() + 100.0,
                    true,
                    wall_time(),
                )
                .unwrap(),
            )
            .await
            .unwrap();
        let db = rusqlite::Connection::open(directory.path().join("events.sqlite")).unwrap();
        db.execute("UPDATE records SET expires=0 WHERE id=?", [&alert.id])
            .unwrap();
        let mut worker = Worker::new(store.clone(), &simulator_config()).unwrap();
        let mut transport = Simulator::default();
        worker.step(&mut transport, 0.0).await.unwrap();
        assert!(transport.sent.is_empty());
        assert_eq!(
            record(&store, &alert.id).await.state,
            TransmissionState::Expired
        );
    }
    #[tokio::test]
    async fn disabled_simulator_and_ingress_bounds_do_not_manufacture_success() {
        let (_directory, store) = fixture();
        let item = message(&store, "Queued").await;
        let mut worker = Worker::new(store.clone(), &json!({})).unwrap();
        let mut transport = Simulator::default();
        worker.step(&mut transport, 0.0).await.unwrap();
        assert!(transport.sent.is_empty());
        assert_eq!(
            record(&store, &item.id).await.state,
            TransmissionState::Queued
        );
        let mut worker = Worker::new(store.clone(), &simulator_config()).unwrap();
        worker.step(&mut transport, 0.0).await.unwrap();
        let item = record(&store, &item.id).await;
        assert_eq!(item.state, TransmissionState::Submitted);
        assert!(item.acknowledgements.is_empty());
        for _ in 0..INGRESS_CAPACITY {
            assert!(transport.inject(RadioEvent::Confirmation("unknown".into())));
        }
        assert!(!transport.inject(RadioEvent::Confirmation("overflow".into())));
    }
    #[tokio::test]
    async fn backoff_caps_at_sixty_and_full_rf_payload_is_private() {
        let (_directory, store) = fixture();
        let mut worker = Worker::new(store.clone(), &simulator_config()).unwrap();
        for expected in [1.0, 2.0, 4.0, 8.0, 16.0, 32.0, 60.0, 60.0] {
            worker.disconnected(100.0);
            assert_eq!(worker.retry_at, 100.0 + expected);
        }
        let mut transport = Simulator::default();
        transport.inject(RadioEvent::Text {
            packet_id: "10".into(),
            node: "!radio".into(),
            channel: 2,
            text: "x".repeat(200),
        });
        transport.inject(RadioEvent::Text {
            packet_id: "11".into(),
            node: "!radio".into(),
            channel: 0,
            text: "wrong channel".into(),
        });
        worker.step(&mut transport, 200.0).await.unwrap();
        assert!(transport.sent.is_empty());
        assert!(store.pending().await.unwrap().is_empty());
        let received = store.records().await.unwrap();
        assert_eq!(received.len(), 1);
        assert_eq!(received[0].text.len(), 200);
        assert!(store.public().await.unwrap().is_empty());
    }
    #[test]
    fn physical_adapter_requires_stable_device_and_positive_finite_pacing() {
        let (_directory, store) = fixture();
        for invalid in [json!(0), json!(-1), json!(true), json!("30")] {
            let config = json!({"radio":{"adapter":"meshtastic","meshtastic":{"device":"/dev/serial/by-id/test","sendIntervalSeconds":invalid}}});
            assert!(Worker::new(store.clone(), &config).is_err());
        }
        for device in [
            "/dev/ttyUSB0",
            "/dev/serial/by-id/../ttyUSB0",
            "/dev/serial/by-id/",
            "/dev/serial/by-id/a/b",
        ] {
            assert!(
                Worker::new(
                    store.clone(),
                    &json!({"radio":{"adapter":"meshtastic","meshtastic":{"device":device}}})
                )
                .is_err()
            );
        }
    }
}
