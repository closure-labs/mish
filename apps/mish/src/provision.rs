//! Explicit native USB commissioning; the messaging reconnect path never calls apply.
//! Independently authored protocol fields, referenced from official admin/channel/mesh
//! schemas: https://github.com/meshtastic/protobufs/tree/master/meshtastic.
//! Local administration uses sender zero, not the runtime's radio node sender.

use crate::channels::{MAX_CHANNELS, MeshChannel, settings_view, validate_lora};
use anyhow::{Context, Result, bail};
use prost::Message;
use serde::{Deserialize, Serialize};
use std::{io::Write, path::Path, time::Duration};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio_serial::SerialPortBuilderExt;

const MAX_FRAME: usize = 512;

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeviceChannel {
    pub index: u32,
    pub role: i32,
    pub settings: Option<Vec<u8>>,
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RadioSnapshot {
    pub own_node: u32,
    pub channels: Vec<DeviceChannel>,
    pub lora: Vec<u8>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ProvisionMode {
    Join,
    Create,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct ProvisionPlan {
    pub before: RadioSnapshot,
    pub changes: Vec<DeviceChannel>,
    /// Local radio slot for each desired channel in the caller's original order.
    pub slots: Vec<u32>,
}

impl RadioSnapshot {
    pub fn validate(&self) -> Result<()> {
        if self.own_node == 0 || self.channels.len() != MAX_CHANNELS || self.lora.len() > MAX_FRAME
        {
            bail!("incomplete radio snapshot");
        }
        let mut disabled_seen = false;
        let mut primary = 0;
        for (index, channel) in self.channels.iter().enumerate() {
            if channel.index as usize != index || !(0..=2).contains(&channel.role) {
                bail!("invalid radio channel table");
            }
            if channel.role == 0 {
                disabled_seen = true;
                continue;
            }
            if disabled_seen {
                bail!("radio enabled channels must be consecutive");
            }
            if channel.role == 1 {
                primary += 1;
            }
            settings_view(
                channel
                    .settings
                    .as_deref()
                    .context("active channel has no settings")?,
            )?;
        }
        if primary != 1 || self.channels[0].role != 1 {
            bail!("radio must have one primary channel in slot zero");
        }
        validate_lora(&self.lora)?;
        Ok(())
    }
}

pub fn plan(
    snapshot: &RadioSnapshot,
    desired: &[MeshChannel],
    mode: ProvisionMode,
) -> Result<ProvisionPlan> {
    snapshot.validate()?;
    if desired.is_empty() || desired.len() > MAX_CHANNELS {
        bail!("request must contain 1–8 channels");
    }
    if mode == ProvisionMode::Join {
        let current = settings_view(snapshot.channels[0].settings.as_deref().unwrap())?;
        let community = settings_view(&desired[0].settings)?;
        if current.name != community.name
            || current.psk != community.psk
            || current.use_aead != community.use_aead
        {
            bail!(
                "joining requires the community channel to match the radio primary; configure the radio first to preserve RF settings"
            );
        }
    }
    let mut after = snapshot.channels.clone();
    let mut slots = Vec::new();
    let mut desired_names = std::collections::HashSet::new();
    for (position, material) in desired.iter().enumerate() {
        let target = settings_view(&material.settings)?;
        if target.name != material.name || !desired_names.insert(target.name.clone()) {
            bail!("channel names are inconsistent or duplicated");
        }
        if target.name.eq_ignore_ascii_case("admin") {
            bail!("reserved admin channel cannot be commissioned for participation");
        }
        let matching = after
            .iter()
            .filter(|channel| channel.role != 0)
            .find(|channel| {
                channel
                    .settings
                    .as_ref()
                    .and_then(|bytes| settings_view(bytes).ok())
                    .is_some_and(|view| view.name == target.name)
            });
        if let Some(existing) = matching {
            let view = settings_view(existing.settings.as_deref().unwrap())?;
            if view.psk != target.psk || view.use_aead != target.use_aead {
                bail!("existing channel name has a different key or encryption mode");
            }
            if mode == ProvisionMode::Create && position == 0 && existing.index != 0 {
                bail!("requested community channel already occupies a secondary slot");
            }
            slots.push(existing.index);
            continue;
        }
        let index = if mode == ProvisionMode::Create && position == 0 {
            // Explicit Create is the sole operation authorized to replace primary.
            0
        } else {
            after
                .iter()
                .position(|channel| channel.role == 0)
                .context("radio has no free channel slots")?
        };
        after[index] = DeviceChannel {
            index: index as u32,
            role: if index == 0 { 1 } else { 2 },
            settings: Some(material.settings.clone()),
        };
        slots.push(index as u32);
    }
    let changes = after
        .into_iter()
        .zip(&snapshot.channels)
        .filter_map(|(new, old)| (new != *old).then_some(new))
        .collect();
    Ok(ProvisionPlan {
        before: snapshot.clone(),
        changes,
        slots,
    })
}

/// Registry deployment uses pinned slots instead of reallocating after a restart.
pub fn plan_at(
    snapshot: &RadioSnapshot,
    desired: &[(u32, MeshChannel)],
    mode: ProvisionMode,
) -> Result<ProvisionPlan> {
    snapshot.validate()?;
    if desired.is_empty() || desired.len() > MAX_CHANNELS {
        bail!("request must contain 1–8 channels");
    }
    let mut after = snapshot.channels.clone();
    let mut slots = Vec::new();
    let mut names = std::collections::HashSet::new();
    for (slot, material) in desired {
        if *slot >= MAX_CHANNELS as u32 || slots.contains(slot) {
            bail!("invalid or duplicated desired slot");
        }
        let view = settings_view(&material.settings)?;
        if view.name != material.name
            || view.name.eq_ignore_ascii_case("admin")
            || !names.insert(view.name.clone())
        {
            bail!("invalid or duplicated desired channel name");
        }
        let existing = &after[*slot as usize];
        if existing.role != 0 {
            let old = settings_view(existing.settings.as_deref().unwrap())?;
            if old.name == view.name && old.psk == view.psk && old.use_aead == view.use_aead {
                slots.push(*slot);
                continue;
            }
            if *slot != 0 || mode != ProvisionMode::Create {
                bail!("desired radio slot is occupied by another channel");
            }
        }
        after[*slot as usize] = DeviceChannel {
            index: *slot,
            role: if *slot == 0 { 1 } else { 2 },
            settings: Some(material.settings.clone()),
        };
        slots.push(*slot);
    }
    let expected = RadioSnapshot {
        channels: after.clone(),
        ..snapshot.clone()
    };
    expected.validate()?;
    let changes = after
        .into_iter()
        .zip(&snapshot.channels)
        .filter_map(|(new, old)| (new != *old).then_some(new))
        .collect();
    Ok(ProvisionPlan {
        before: snapshot.clone(),
        changes,
        slots,
    })
}

/// Conservative convenience policy for deployment of stored slots onto a fresh
/// default radio. A custom primary must already match the stored community.
pub fn plan_slots(
    snapshot: &RadioSnapshot,
    desired: &[(u32, MeshChannel)],
) -> Result<ProvisionPlan> {
    let primary = snapshot.channels.first().context("radio has no primary")?;
    let view = settings_view(
        primary
            .settings
            .as_deref()
            .context("radio primary has no settings")?,
    )?;
    let default_key = [
        0xd4, 0xf1, 0xbb, 0x3a, 0x20, 0x29, 0x07, 0x59, 0xf0, 0xbc, 0xff, 0xab, 0xcf, 0x4e, 0x69,
        0x01,
    ];
    let stock = (view.name.is_empty() || view.name == "LongFast")
        && (view.psk.is_empty() || view.psk == [1] || view.psk == default_key);
    plan_at(
        snapshot,
        desired,
        if stock {
            ProvisionMode::Create
        } else {
            ProvisionMode::Join
        },
    )
}

pub fn decode_channel(bytes: &[u8]) -> Result<DeviceChannel> {
    let channel: DeviceChannel = ChannelWire::decode(bytes)
        .context("malformed channel readback")?
        .into();
    if channel.index >= MAX_CHANNELS as u32 || !(0..=2).contains(&channel.role) {
        bail!("invalid channel readback");
    }
    if let Some(settings) = &channel.settings {
        settings_view(settings)?;
    }
    Ok(channel)
}

pub fn decode_lora_config(bytes: &[u8]) -> Result<Option<Vec<u8>>> {
    let config = Config::decode(bytes).context("malformed configuration readback")?;
    if let Some(lora) = &config.lora {
        validate_lora(lora)?;
    }
    Ok(config.lora)
}

#[derive(Clone, PartialEq, Message)]
struct ChannelWire {
    #[prost(int32, tag = "1")]
    index: i32,
    #[prost(bytes = "vec", optional, tag = "2")]
    settings: Option<Vec<u8>>,
    #[prost(int32, tag = "3")]
    role: i32,
}
impl From<ChannelWire> for DeviceChannel {
    fn from(wire: ChannelWire) -> Self {
        Self {
            index: wire.index as u32,
            role: wire.role,
            settings: wire.settings,
        }
    }
}
impl From<&DeviceChannel> for ChannelWire {
    fn from(channel: &DeviceChannel) -> Self {
        Self {
            index: channel.index as i32,
            role: channel.role,
            settings: channel.settings.clone(),
        }
    }
}
#[derive(Clone, PartialEq, Message)]
struct Admin {
    #[prost(uint32, optional, tag = "1")]
    get_channel: Option<u32>,
    #[prost(message, optional, tag = "2")]
    channel: Option<ChannelWire>,
    #[prost(uint32, optional, tag = "5")]
    get_config: Option<u32>,
    #[prost(message, optional, tag = "6")]
    config: Option<Config>,
    #[prost(message, optional, tag = "33")]
    set_channel: Option<ChannelWire>,
    #[prost(bytes = "vec", tag = "101")]
    passkey: Vec<u8>,
}
#[derive(Clone, PartialEq, Message)]
struct Config {
    #[prost(bytes = "vec", optional, tag = "6")]
    lora: Option<Vec<u8>>,
}
#[derive(Clone, PartialEq, Message)]
struct Data {
    #[prost(uint32, tag = "1")]
    port: u32,
    #[prost(bytes = "vec", tag = "2")]
    payload: Vec<u8>,
    #[prost(bool, tag = "3")]
    want_response: bool,
    #[prost(fixed32, tag = "6")]
    request: u32,
}
#[derive(Clone, PartialEq, Message)]
struct Packet {
    #[prost(fixed32, tag = "1")]
    sender: u32,
    #[prost(fixed32, tag = "2")]
    destination: u32,
    #[prost(uint32, tag = "3")]
    channel: u32,
    #[prost(message, optional, tag = "4")]
    data: Option<Data>,
    #[prost(fixed32, tag = "6")]
    id: u32,
    #[prost(bool, tag = "10")]
    want_ack: bool,
}
#[derive(Clone, PartialEq, Message)]
struct ToRadio {
    #[prost(message, optional, tag = "1")]
    packet: Option<Packet>,
    #[prost(uint32, tag = "3")]
    nonce: u32,
}
#[derive(Clone, PartialEq, Message)]
struct FromRadio {
    #[prost(message, optional, tag = "2")]
    packet: Option<Packet>,
    #[prost(message, optional, tag = "3")]
    identity: Option<Identity>,
    #[prost(uint32, tag = "7")]
    complete: u32,
}
#[derive(Clone, PartialEq, Message)]
struct Identity {
    #[prost(uint32, tag = "1")]
    node: u32,
}

fn id() -> u32 {
    loop {
        let value = rand::random();
        if value != 0 {
            return value;
        }
    }
}
fn frame(message: &impl Message) -> Result<Vec<u8>> {
    let bytes = message.encode_to_vec();
    if bytes.len() > MAX_FRAME {
        bail!("provisioning frame exceeds serial limit");
    }
    let mut result = vec![0x94, 0xc3, (bytes.len() >> 8) as u8, bytes.len() as u8];
    result.extend(bytes);
    Ok(result)
}

struct Session<S> {
    stream: S,
    own_node: u32,
    passkey: Vec<u8>,
}
impl<S: AsyncRead + AsyncWrite + Unpin> Session<S> {
    async fn write(&mut self, message: &impl Message) -> Result<()> {
        let bytes = frame(message)?;
        tokio::time::timeout(Duration::from_secs(5), async {
            self.stream.write_all(&bytes).await?;
            self.stream.flush().await
        })
        .await
        .context("USB provisioning write timed out")??;
        Ok(())
    }
    async fn read(&mut self) -> Result<FromRadio> {
        loop {
            // Resynchronize over console output and malformed header lengths.
            let mut magic = false;
            loop {
                let byte = self
                    .stream
                    .read_u8()
                    .await
                    .context("USB provisioning disconnected")?;
                if magic && byte == 0xc3 {
                    break;
                }
                magic = byte == 0x94;
            }
            let length = self.stream.read_u16().await? as usize;
            if length > MAX_FRAME {
                continue;
            }
            let mut bytes = vec![0; length];
            self.stream.read_exact(&mut bytes).await?;
            if let Ok(message) = FromRadio::decode(bytes.as_slice()) {
                return Ok(message);
            }
        }
    }
    async fn connect(stream: S) -> Result<Self> {
        let mut session = Self {
            stream,
            own_node: 0,
            passkey: vec![],
        };
        let nonce = id();
        session
            .write(&ToRadio {
                nonce,
                ..Default::default()
            })
            .await?;
        tokio::time::timeout(Duration::from_secs(15), async {
            loop {
                let message = session.read().await?;
                if let Some(identity) = message.identity {
                    session.own_node = identity.node;
                }
                if message.complete == nonce && session.own_node != 0 {
                    return Ok::<_, anyhow::Error>(());
                }
            }
        })
        .await
        .context("USB provisioning configuration timed out")??;
        Ok(session)
    }
    async fn request(&mut self, admin: Admin) -> Result<Admin> {
        let request_id = id();
        self.write(&ToRadio {
            packet: Some(Packet {
                sender: 0,
                destination: self.own_node,
                id: request_id,
                want_ack: true,
                data: Some(Data {
                    port: 6,
                    payload: admin.encode_to_vec(),
                    want_response: true,
                    ..Default::default()
                }),
                ..Default::default()
            }),
            ..Default::default()
        })
        .await?;
        tokio::time::timeout(Duration::from_secs(15), async {
            loop {
                let Some(packet) = self.read().await?.packet else {
                    continue;
                };
                if packet.sender != self.own_node || packet.destination != self.own_node {
                    continue;
                }
                let Some(data) = packet.data else {
                    continue;
                };
                if data.request != request_id {
                    continue;
                }
                if data.port == 5 {
                    continue;
                } // Transport ACK is not configuration readback.
                if data.port != 6 {
                    continue;
                }
                let response =
                    Admin::decode(data.payload.as_slice()).context("malformed admin response")?;
                if !response.passkey.is_empty() {
                    self.passkey = response.passkey.clone();
                }
                return Ok(response);
            }
        })
        .await
        .context("USB admin response timed out")?
    }
    async fn snapshot(&mut self) -> Result<RadioSnapshot> {
        let mut channels = Vec::new();
        for index in 0..MAX_CHANNELS {
            let response = self
                .request(Admin {
                    get_channel: Some(index as u32 + 1),
                    ..Default::default()
                })
                .await?;
            let channel: DeviceChannel = response
                .channel
                .context("admin response has no channel")?
                .into();
            if channel.index != index as u32 {
                bail!("admin response returned wrong channel index");
            }
            channels.push(channel);
        }
        let response = self
            .request(Admin {
                get_config: Some(5),
                ..Default::default()
            })
            .await?;
        let lora = response
            .config
            .and_then(|config| config.lora)
            .context("admin response has no LoRa config")?;
        let snapshot = RadioSnapshot {
            own_node: self.own_node,
            channels,
            lora,
        };
        snapshot.validate()?;
        Ok(snapshot)
    }
    async fn set(&mut self, channel: &DeviceChannel) -> Result<()> {
        // Do not wait for an ACK as success: the next explicit read verifies the write.
        let admin = Admin {
            set_channel: Some(channel.into()),
            passkey: self.passkey.clone(),
            ..Default::default()
        };
        self.write(&ToRadio {
            packet: Some(Packet {
                sender: 0,
                destination: self.own_node,
                id: id(),
                data: Some(Data {
                    port: 6,
                    payload: admin.encode_to_vec(),
                    ..Default::default()
                }),
                ..Default::default()
            }),
            ..Default::default()
        })
        .await
    }
}

fn open(device: &str) -> Result<tokio_serial::SerialStream> {
    let path = Path::new(device);
    if path.parent() != Some(Path::new("/dev/serial/by-id")) || path.file_name().is_none() {
        bail!("USB commissioning requires a stable /dev/serial/by-id device path");
    }
    tokio_serial::new(device, 115_200)
        .open_native_async()
        .context("opening USB radio for commissioning")
}

pub async fn inspect(device: &str) -> Result<RadioSnapshot> {
    Session::connect(open(device)?).await?.snapshot().await
}

/// Caller must stop the live worker before opening the commissioning serial port.
/// Recovery file is exclusive, mode 0600, and durable before the first radio write.
pub async fn apply(
    device: &str,
    plan: &ProvisionPlan,
    recovery_path: &Path,
) -> Result<RadioSnapshot> {
    apply_stream(open(device)?, plan, recovery_path).await
}

// The complete production sequence also runs against bounded duplex USB fixtures.
async fn apply_stream<S: AsyncRead + AsyncWrite + Unpin>(
    stream: S,
    plan: &ProvisionPlan,
    recovery_path: &Path,
) -> Result<RadioSnapshot> {
    plan.before.validate()?;
    let mut expected = plan.before.clone();
    let mut changed = std::collections::HashSet::new();
    for channel in &plan.changes {
        if channel.index >= MAX_CHANNELS as u32
            || !changed.insert(channel.index)
            || channel.role == 0
        {
            bail!("planned radio slots are invalid");
        }
        let view = settings_view(
            channel
                .settings
                .as_deref()
                .context("planned channel has no settings")?,
        )?;
        if view.name.eq_ignore_ascii_case("admin") {
            bail!("reserved administrative channel in deployment plan");
        }
        expected.channels[channel.index as usize] = channel.clone();
    }
    expected.validate()?;
    let mut session = Session::connect(stream).await?;
    let before = session.snapshot().await?;
    if before != plan.before {
        bail!("radio changed after planning; inspect and plan again");
    }
    persist_recovery(recovery_path, &before)?;
    for channel in &plan.changes {
        if channel.index >= MAX_CHANNELS as u32 {
            bail!("planned radio slot exceeds capacity");
        }
        session.set(channel).await?;
        let read = session
            .request(Admin {
                get_channel: Some(channel.index + 1),
                ..Default::default()
            })
            .await?;
        let actual: DeviceChannel = read
            .channel
            .context("missing channel verification response")?
            .into();
        if actual != *channel {
            bail!("radio channel readback mismatch; stop and inspect recovery snapshot");
        }
    }
    let after = session.snapshot().await?;
    if after != expected {
        bail!(
            "radio verification failed: unrelated settings changed or requested settings were not retained"
        );
    }
    Ok(after)
}

fn persist_recovery(path: &Path, snapshot: &RadioSnapshot) -> Result<()> {
    use std::os::unix::fs::OpenOptionsExt;
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)
        .context("creating protected radio recovery snapshot")?;
    file.write_all(&serde_json::to_vec(snapshot)?)?;
    file.sync_all()?;
    if let Some(parent) = path.parent() {
        std::fs::File::open(parent)?.sync_all()?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::channels::generated;

    fn snapshot() -> RadioSnapshot {
        let mut channels: Vec<_> = (0..8)
            .map(|index| DeviceChannel {
                index,
                role: 0,
                settings: None,
            })
            .collect();
        channels[0] = DeviceChannel {
            index: 0,
            role: 1,
            settings: Some(vec![0x12, 0x01, 0x01]),
        };
        RadioSnapshot {
            own_node: 9,
            channels,
            lora: vec![0x08, 0x01],
        }
    }

    #[test]
    fn planning_preserves_slots_and_refuses_conflicting_keys_or_full_radios() {
        let public = generated("AstralMesh").unwrap();
        let staff = generated("Staff").unwrap();
        let base = snapshot();
        let creation = plan(
            &base,
            &[public.clone(), staff.clone()],
            ProvisionMode::Create,
        )
        .unwrap();
        assert_eq!(creation.slots, vec![0, 1]);
        assert_eq!(creation.before.lora, base.lora);
        let mut installed = base.clone();
        for channel in creation.changes {
            let index = channel.index as usize;
            installed.channels[index] = channel;
        }
        assert!(
            plan(&installed, &[public], ProvisionMode::Join)
                .unwrap()
                .changes
                .is_empty()
        );
        assert!(
            plan(
                &installed,
                &[generated("Staff").unwrap()],
                ProvisionMode::Join
            )
            .is_err()
        );
        for index in 2..8 {
            installed.channels[index] = DeviceChannel {
                index: index as u32,
                role: 2,
                settings: Some(generated(&format!("Group{index}")).unwrap().settings),
            };
        }
        assert!(
            plan(
                &installed,
                &[generated("Extra").unwrap()],
                ProvisionMode::Join
            )
            .is_err()
        );
    }

    #[test]
    fn recovery_snapshot_is_private_exclusive_and_contains_exact_bytes() {
        use std::os::unix::fs::PermissionsExt;
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("radio.json");
        let before = snapshot();
        persist_recovery(&path, &before).unwrap();
        assert_eq!(
            std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );
        let restored: RadioSnapshot =
            serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        assert!(restored == before);
        assert!(persist_recovery(&path, &before).is_err());
    }

    #[tokio::test]
    async fn local_usb_admin_ignores_wrong_correlation_and_transport_ack() {
        let (client, mut radio) = tokio::io::duplex(2048);
        let device = tokio::spawn(async move {
            // Reuse the session's bounded reader on the simulated device side.
            let mut endpoint = Session {
                stream: &mut radio,
                own_node: 9,
                passkey: vec![],
            };
            let request = endpoint.read_raw_to_radio().await;
            endpoint
                .write(&FromRadio {
                    identity: Some(Identity { node: 9 }),
                    ..Default::default()
                })
                .await
                .unwrap();
            endpoint
                .write(&FromRadio {
                    complete: request.nonce,
                    ..Default::default()
                })
                .await
                .unwrap();
            let request = endpoint.read_raw_to_radio().await.packet.unwrap();
            assert_eq!(request.sender, 0);
            assert_eq!(request.destination, 9);
            let data = request.data.unwrap();
            assert_eq!(data.port, 6);
            assert!(data.want_response);
            assert_eq!(
                Admin::decode(data.payload.as_slice()).unwrap().get_channel,
                Some(1)
            );
            for (port, correlation) in [
                (6, request.id.wrapping_add(1)),
                (5, request.id),
                (6, request.id),
            ] {
                endpoint
                    .write(&FromRadio {
                        packet: Some(Packet {
                            sender: 9,
                            destination: 9,
                            id: 7,
                            data: Some(Data {
                                port,
                                request: correlation,
                                payload: Admin {
                                    channel: Some(ChannelWire {
                                        index: 0,
                                        role: 1,
                                        settings: Some(vec![0x12, 0x01, 0x01]),
                                    }),
                                    passkey: vec![42; 8],
                                    ..Default::default()
                                }
                                .encode_to_vec(),
                                ..Default::default()
                            }),
                            ..Default::default()
                        }),
                        ..Default::default()
                    })
                    .await
                    .unwrap();
            }
        });
        let mut session = Session::connect(client).await.unwrap();
        let response = session
            .request(Admin {
                get_channel: Some(1),
                ..Default::default()
            })
            .await
            .unwrap();
        assert_eq!(response.channel.unwrap().index, 0);
        assert_eq!(session.passkey, vec![42; 8]);
        device.await.unwrap();
    }

    impl<S: AsyncRead + AsyncWrite + Unpin> Session<S> {
        async fn read_raw_to_radio(&mut self) -> ToRadio {
            assert_eq!(self.stream.read_u8().await.unwrap(), 0x94);
            assert_eq!(self.stream.read_u8().await.unwrap(), 0xc3);
            let length = self.stream.read_u16().await.unwrap() as usize;
            let mut bytes = vec![0; length];
            self.stream.read_exact(&mut bytes).await.unwrap();
            ToRadio::decode(bytes.as_slice()).unwrap()
        }
    }

    #[derive(Clone, Copy, PartialEq, Eq)]
    enum DeviceBehavior {
        Success,
        WrongReadback,
        DisconnectAfterWrite,
        ChangedRf,
    }

    async fn simulated_deployment(behavior: DeviceBehavior) {
        use std::os::unix::fs::PermissionsExt;
        let directory = tempfile::tempdir().unwrap();
        let recovery_path = directory.path().join("radio-recovery.json");
        let mut before = snapshot();
        // Existing independent private slot and an opaque RF field must survive.
        before.channels[1] = DeviceChannel {
            index: 1,
            role: 2,
            settings: Some(generated("Existing").unwrap().settings),
        };
        before.lora.extend([0x98, 0x06, 0x01]);
        let desired = [
            generated("AstralMesh").unwrap(),
            generated("Staff").unwrap(),
        ];
        let deployment = plan(&before, &desired, ProvisionMode::Create).unwrap();
        assert_eq!(deployment.slots, vec![0, 2]);
        let expected_writes: Vec<_> = deployment
            .changes
            .iter()
            .map(|channel| channel.index)
            .collect();
        let mut expected = before.clone();
        for channel in &deployment.changes {
            expected.channels[channel.index as usize] = channel.clone();
        }
        let (client, radio) = tokio::io::duplex(4096);
        let device_before = before.clone();
        let device_recovery = recovery_path.clone();
        let device = tokio::spawn(async move {
            let mut endpoint = Session {
                stream: radio,
                own_node: device_before.own_node,
                passkey: vec![],
            };
            let handshake = endpoint.read_raw_to_radio().await;
            assert_ne!(handshake.nonce, 0);
            endpoint
                .write(&FromRadio {
                    identity: Some(Identity {
                        node: device_before.own_node,
                    }),
                    ..Default::default()
                })
                .await
                .unwrap();
            endpoint
                .write(&FromRadio {
                    complete: handshake.nonce,
                    ..Default::default()
                })
                .await
                .unwrap();
            let mut live = device_before.clone();
            let mut writes = Vec::new();
            let mut rf_reads = 0;
            loop {
                let request = endpoint.read_raw_to_radio().await.packet.unwrap();
                assert_eq!(request.sender, 0);
                assert_eq!(request.destination, live.own_node);
                let data = request.data.unwrap();
                assert_eq!(data.port, 6);
                let admin = Admin::decode(data.payload.as_slice()).unwrap();
                if let Some(channel) = admin.set_channel {
                    // Durable recovery must precede even the first USB mutation.
                    assert_eq!(
                        std::fs::metadata(&device_recovery)
                            .unwrap()
                            .permissions()
                            .mode()
                            & 0o777,
                        0o600
                    );
                    let saved: RadioSnapshot =
                        serde_json::from_slice(&std::fs::read(&device_recovery).unwrap()).unwrap();
                    assert!(saved == device_before);
                    assert_eq!(admin.passkey, vec![42; 8]);
                    let channel: DeviceChannel = channel.into();
                    writes.push(channel.index);
                    let index = channel.index as usize;
                    live.channels[index] = channel;
                    if behavior == DeviceBehavior::DisconnectAfterWrite {
                        return (live, writes);
                    }
                    continue;
                }
                assert!(data.want_response);
                let mut stop_after_response = false;
                let response = if let Some(index_plus_one) = admin.get_channel {
                    assert!((1..=8).contains(&index_plus_one));
                    let index = index_plus_one as usize - 1;
                    let mut observed = live.channels[index].clone();
                    if behavior == DeviceBehavior::WrongReadback
                        && writes.last() == Some(&(index as u32))
                    {
                        observed.settings = Some(generated("WrongKey").unwrap().settings);
                        stop_after_response = true;
                    }
                    Admin {
                        channel: Some((&observed).into()),
                        passkey: vec![42; 8],
                        ..Default::default()
                    }
                } else {
                    assert_eq!(admin.get_config, Some(5));
                    rf_reads += 1;
                    if behavior == DeviceBehavior::ChangedRf && rf_reads == 2 {
                        live.lora = vec![0x08, 0x00];
                    }
                    stop_after_response = rf_reads == 2;
                    Admin {
                        config: Some(Config {
                            lora: Some(live.lora.clone()),
                        }),
                        passkey: vec![42; 8],
                        ..Default::default()
                    }
                };
                endpoint
                    .write(&FromRadio {
                        packet: Some(Packet {
                            sender: live.own_node,
                            destination: live.own_node,
                            id: id(),
                            data: Some(Data {
                                port: 6,
                                request: request.id,
                                payload: response.encode_to_vec(),
                                ..Default::default()
                            }),
                            ..Default::default()
                        }),
                        ..Default::default()
                    })
                    .await
                    .unwrap();
                if stop_after_response {
                    return (live, writes);
                }
            }
        });
        let result = tokio::time::timeout(
            Duration::from_secs(3),
            apply_stream(client, &deployment, &recovery_path),
        )
        .await
        .expect("simulated commissioning must terminate promptly");
        let (actual_radio, writes) = tokio::time::timeout(Duration::from_secs(3), device)
            .await
            .expect("simulated radio must terminate promptly")
            .unwrap();
        if behavior == DeviceBehavior::Success {
            assert!(result.unwrap() == expected);
            assert!(actual_radio == expected);
            assert_eq!(writes, expected_writes);
        } else {
            assert!(
                result.is_err(),
                "partial or mismatched provisioning must not return a verified snapshot"
            );
            if behavior == DeviceBehavior::ChangedRf {
                assert_eq!(writes, expected_writes);
            } else {
                assert_eq!(writes.len(), 1);
            }
        }
        let saved: RadioSnapshot =
            serde_json::from_slice(&std::fs::read(&recovery_path).unwrap()).unwrap();
        assert!(saved == before);
        assert!(actual_radio.channels[1] == before.channels[1]);
    }

    #[tokio::test]
    async fn apply_usb_writes_only_planned_slots_and_verifies_complete_radio() {
        simulated_deployment(DeviceBehavior::Success).await;
    }

    #[tokio::test]
    async fn apply_usb_wrong_readback_never_returns_verified_snapshot() {
        simulated_deployment(DeviceBehavior::WrongReadback).await;
    }

    #[tokio::test]
    async fn apply_usb_disconnect_after_mutation_preserves_recovery_and_fails() {
        simulated_deployment(DeviceBehavior::DisconnectAfterWrite).await;
    }

    #[tokio::test]
    async fn apply_usb_unrelated_rf_change_fails_whole_radio_verification() {
        simulated_deployment(DeviceBehavior::ChangedRf).await;
    }
}
