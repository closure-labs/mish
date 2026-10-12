//! Bounded commands to the authoritative SQLite owner. Database I/O never blocks Tokio.
use crate::channel_store::{ChannelMetadata, EnrollmentExport, RadioChannel};
use crate::{appliance::EventStore, domain::*};
use anyhow::{Result, anyhow};
use tokio::sync::{mpsc, oneshot};

type Reply<T> = oneshot::Sender<Result<T>>;
enum Command {
    Community(
        crate::community::Credentials,
        crate::community::Action,
        Reply<serde_json::Value>,
    ),
    Authenticate(String, Reply<Option<StaffIdentity>>),
    Enroll(String, Role, bool, Reply<Enrollment>),
    Revoke(String, Reply<()>),
    Create(NewRecord, Reply<Record>),
    Record(String, Reply<Option<Record>>),
    Records(Reply<Vec<Record>>),
    RecordsFor(StaffIdentity, Reply<Vec<Record>>),
    RecordsForChannel(StaffIdentity, String, Reply<Vec<Record>>),
    CreateAs(StaffIdentity, NewRecord, Reply<Record>),
    Channels(Option<StaffIdentity>, Reply<Vec<ChannelMetadata>>),
    ChannelEnrollment(String, Option<StaffIdentity>, Reply<EnrollmentExport>),
    ChannelAccess(String, String, String, bool, Reply<()>),
    RadioChannels(Reply<Vec<RadioChannel>>),
    Binding(String, String, Reply<ChannelMetadata>),
    RadioRoute(u32, Reply<Option<ChannelMetadata>>),
    Acknowledge(
        String,
        String,
        AcknowledgementSource,
        Option<String>,
        Option<i64>,
        Reply<Acknowledgement>,
    ),
    Publish(String, Reply<Announcement>),
    Public(Reply<Vec<Announcement>>),
    Pending(Reply<Vec<PendingRecord>>),
    Transition(String, TransmissionUpdate, Reply<bool>),
    Confirm(String, Reply<bool>),
    Receive(String, String, String, i64, Reply<bool>),
    Prune(Reply<usize>),
}

#[derive(Clone)]
pub struct Coordinator {
    sender: mpsc::Sender<Command>,
}
impl Coordinator {
    pub async fn community(
        &self,
        credentials: crate::community::Credentials,
        action: crate::community::Action,
    ) -> Result<serde_json::Value> {
        self.request(|r| Command::Community(credentials, action, r))
            .await
    }
    pub fn start(store: EventStore, retention_days: i64) -> Self {
        let (sender, mut receiver) = mpsc::channel(128);
        std::thread::spawn(move || {
            while let Some(command) = receiver.blocking_recv() {
                // A successful response always follows the durable database operation.
                match command {
                    Command::Community(credentials, action, reply) => {
                        let _ = reply.send(store.community(credentials, action));
                    }
                    Command::Authenticate(token, reply) => {
                        let _ = reply.send(store.authenticate(&token));
                    }
                    Command::Enroll(name, role, first, reply) => {
                        let _ = reply.send(store.enroll(&name, role, first));
                    }
                    Command::Revoke(id, reply) => {
                        let _ = reply.send(store.revoke(&id));
                    }
                    Command::Create(value, reply) => {
                        let _ = reply.send(store.create(value));
                    }
                    Command::Record(id, reply) => {
                        let _ = reply.send(store.record(&id));
                    }
                    Command::Records(reply) => {
                        let _ = reply.send(store.records());
                    }
                    Command::RecordsFor(actor, reply) => {
                        let _ = reply.send(store.records_for(&actor));
                    }
                    Command::RecordsForChannel(actor, id, reply) => {
                        let _ = reply.send(store.records_for_channel(&actor, &id));
                    }
                    Command::CreateAs(actor, value, reply) => {
                        let _ = reply.send(store.create_as(&actor, value));
                    }
                    Command::Channels(actor, reply) => {
                        let _ = reply.send(store.channel_catalog(actor.as_ref()));
                    }
                    Command::ChannelEnrollment(id, actor, reply) => {
                        let _ = reply.send(store.enrollment(&id, actor.as_ref()));
                    }
                    Command::ChannelAccess(operator, staff, channel, grant, reply) => {
                        let _ = reply.send(if grant {
                            store.grant_channel_access(&operator, &staff, &channel)
                        } else {
                            store.revoke_channel_access(&operator, &staff, &channel)
                        });
                    }
                    Command::RadioChannels(reply) => {
                        let _ = reply.send(store.radio_channels());
                    }
                    Command::Binding(id, fingerprint, reply) => {
                        let _ = reply.send(store.validate_binding(&id, &fingerprint));
                    }
                    Command::RadioRoute(slot, reply) => {
                        let _ = reply.send(store.route_radio_channel(slot));
                    }
                    Command::Acknowledge(id, actor, source, node, channel, reply) => {
                        let _ = reply.send(store.acknowledge(
                            &id,
                            &actor,
                            source,
                            node.as_deref(),
                            channel,
                        ));
                    }
                    Command::Publish(text, reply) => {
                        let _ = reply.send(store.publish(&text));
                    }
                    Command::Public(reply) => {
                        let _ = reply.send(store.public());
                    }
                    Command::Pending(reply) => {
                        let _ = reply.send(store.pending());
                    }
                    Command::Transition(id, update, reply) => {
                        let _ = reply.send(store.transition(&id, &update));
                    }
                    Command::Confirm(packet, reply) => {
                        let _ = reply.send(store.transport_confirmation(&packet));
                    }
                    Command::Receive(packet, node, text, channel, reply) => {
                        let _ = reply.send(store.receive(&packet, &node, &text, channel));
                    }
                    Command::Prune(reply) => {
                        let _ = reply.send(store.prune(retention_days));
                    }
                }
            }
        });
        Self { sender }
    }
    async fn request<T>(&self, command: impl FnOnce(Reply<T>) -> Command) -> Result<T> {
        let (reply, response) = oneshot::channel();
        self.sender
            .try_send(command(reply))
            .map_err(|error| anyhow!("coordinator unavailable or busy: {error}"))?;
        response
            .await
            .map_err(|_| anyhow!("coordinator stopped before confirming persistence"))?
    }
    pub async fn authenticate(&self, token: &str) -> Result<Option<StaffIdentity>> {
        self.request(|r| Command::Authenticate(token.into(), r))
            .await
    }
    pub async fn enroll(&self, name: &str, role: Role, first: bool) -> Result<Enrollment> {
        self.request(|r| Command::Enroll(name.into(), role, first, r))
            .await
    }
    pub async fn revoke(&self, id: &str) -> Result<()> {
        self.request(|r| Command::Revoke(id.into(), r)).await
    }
    pub async fn create(&self, value: NewRecord) -> Result<Record> {
        self.request(|r| Command::Create(value, r)).await
    }
    pub async fn record(&self, id: &str) -> Result<Option<Record>> {
        self.request(|r| Command::Record(id.into(), r)).await
    }
    pub async fn records(&self) -> Result<Vec<Record>> {
        self.request(Command::Records).await
    }
    pub async fn records_for(&self, actor: StaffIdentity) -> Result<Vec<Record>> {
        self.request(|r| Command::RecordsFor(actor, r)).await
    }
    pub async fn records_for_channel(&self, actor: StaffIdentity, id: &str) -> Result<Vec<Record>> {
        self.request(|r| Command::RecordsForChannel(actor, id.into(), r))
            .await
    }
    pub async fn create_as(&self, actor: StaffIdentity, value: NewRecord) -> Result<Record> {
        self.request(|r| Command::CreateAs(actor, value, r)).await
    }
    pub async fn channels(&self, actor: Option<StaffIdentity>) -> Result<Vec<ChannelMetadata>> {
        self.request(|r| Command::Channels(actor, r)).await
    }
    pub async fn channel_enrollment(
        &self,
        id: &str,
        actor: Option<StaffIdentity>,
    ) -> Result<EnrollmentExport> {
        self.request(|r| Command::ChannelEnrollment(id.into(), actor, r))
            .await
    }
    pub async fn channel_access(
        &self,
        operator: &str,
        staff: &str,
        channel: &str,
        grant: bool,
    ) -> Result<()> {
        self.request(|r| {
            Command::ChannelAccess(operator.into(), staff.into(), channel.into(), grant, r)
        })
        .await
    }
    pub async fn radio_channels(&self) -> Result<Vec<RadioChannel>> {
        self.request(Command::RadioChannels).await
    }
    pub async fn validate_binding(&self, id: &str, fingerprint: &str) -> Result<ChannelMetadata> {
        self.request(|r| Command::Binding(id.into(), fingerprint.into(), r))
            .await
    }
    pub async fn route_radio_channel(&self, slot: u32) -> Result<Option<ChannelMetadata>> {
        self.request(|r| Command::RadioRoute(slot, r)).await
    }
    pub async fn acknowledge(
        &self,
        id: &str,
        actor: &str,
        source: AcknowledgementSource,
        node: Option<&str>,
        channel: Option<i64>,
    ) -> Result<Acknowledgement> {
        self.request(|r| {
            Command::Acknowledge(
                id.into(),
                actor.into(),
                source,
                node.map(Into::into),
                channel,
                r,
            )
        })
        .await
    }
    pub async fn publish(&self, text: &str) -> Result<Announcement> {
        self.request(|r| Command::Publish(text.into(), r)).await
    }
    pub async fn public(&self) -> Result<Vec<Announcement>> {
        self.request(Command::Public).await
    }
    pub async fn pending(&self) -> Result<Vec<PendingRecord>> {
        self.request(Command::Pending).await
    }
    pub async fn transition(&self, id: &str, update: TransmissionUpdate) -> Result<bool> {
        self.request(|r| Command::Transition(id.into(), update, r))
            .await
    }
    pub async fn transport_confirmation(&self, packet: &str) -> Result<bool> {
        self.request(|r| Command::Confirm(packet.into(), r)).await
    }
    pub async fn receive(
        &self,
        packet: &str,
        node: &str,
        text: &str,
        channel: i64,
    ) -> Result<bool> {
        self.request(|r| Command::Receive(packet.into(), node.into(), text.into(), channel, r))
            .await
    }
    pub async fn prune(&self) -> Result<usize> {
        self.request(Command::Prune).await
    }
}
