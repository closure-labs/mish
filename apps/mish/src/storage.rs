//! Compatibility adapter for opt-in local simulation and GeoPack files.
use crate::{invalid, validation};
use anyhow::Result;
use base64::{Engine, engine::general_purpose::STANDARD};
use hmac::{Hmac, KeyInit, Mac};
use rand::{TryRng, rngs::SysRng};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::Write,
    os::unix::fs::{DirBuilderExt, OpenOptionsExt},
    path::{Path, PathBuf},
};

pub fn read_json(path: &Path) -> Result<Value> {
    Ok(serde_json::from_slice(&fs::read(path)?)?)
}
pub fn hash(value: &Value) -> String {
    hex::encode(Sha256::digest(
        serde_json::to_vec(value).expect("JSON value is serializable"),
    ))
}
pub fn utc_now() -> String {
    std::env::var("MISH_FIXED_TIME")
        .unwrap_or_else(|_| chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true))
}
pub fn default_config(state: Option<&Path>) -> Value {
    let state = state.map(PathBuf::from).unwrap_or_else(|| {
        std::env::var_os("MISH_STATE_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("/var/lib/mish"))
    });
    json!({"version":env!("CARGO_PKG_VERSION"),"profile":"mini-pc","domain":"mish.local","nixDistribution":"nixos","stateDir":state,
        "api":{"bindHost":"127.0.0.1","port":8787},"queueLimit":1000,"retentionDays":7,
        "matrix":{"provider":"tuwunel","domain":"mish.local","port":6167,"homeserverUrl":"http://127.0.0.1:6167","releaseGated":true},
        "openziti":{"enable":false,"controllerUrl":"http://127.0.0.1:1280","edgeRouterUrl":"http://127.0.0.1:3022","privilegedMfaRequired":true,"serviceIdentities":["matrix","geopack","admin-api"]},
        "geopack":{"enable":false},"nostr":{"relays":[]},"radio":{"adapter":"disabled","providers":[],"simulator":{"enable":false},"reticulumRnode":{"enable":false,"interface":"Mish RNode","port":"/dev/ttyACM0","mode":"access_point","frequency":915000000,"bandwidth":125000,"txPower":7,"spreadingFactor":8,"codingRate":5}},"catalog":{"path":""}})
}
fn merge(base: &mut Value, overlay: Value) {
    if let (Some(base), Some(overlay)) = (base.as_object_mut(), overlay.as_object()) {
        for (key, value) in overlay {
            if let Some(existing) = base.get_mut(key) {
                merge(existing, value.clone());
            } else {
                base.insert(key.clone(), value.clone());
            }
        }
    } else {
        *base = overlay;
    }
}
pub fn load_config(path: Option<&Path>, state: Option<&Path>) -> Result<Value> {
    let configured = std::env::var_os("MISH_CONFIG")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/etc/mish/config.json"));
    let path = path.unwrap_or(&configured);
    let mut value = default_config(state);
    if path.exists() {
        let overlay = read_json(path)?;
        if !overlay.is_object() {
            return Err(invalid("config object required"));
        }
        merge(&mut value, overlay);
    }
    if let Some(state) = state {
        value["stateDir"] = json!(state);
    }
    Ok(value)
}
pub fn atomic_json(path: &Path, value: &Value) -> Result<()> {
    let parent = path.parent().ok_or_else(|| invalid("parent required"))?;
    fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(parent)?;
    let mut file = tempfile::NamedTempFile::new_in(parent)?;
    serde_json::to_writer_pretty(file.as_file_mut(), value)?;
    writeln!(file)?;
    file.as_file().sync_all()?;
    file.persist(path)?;
    fs::File::open(parent)?.sync_all()?;
    Ok(())
}
#[derive(Clone)]
pub struct MishStore {
    pub state_dir: PathBuf,
    pub config: Value,
}
impl MishStore {
    pub fn new(state_dir: &Path, config: Value) -> Result<Self> {
        for dir in [
            "waypoints",
            "geopacks",
            "nostr/publications",
            "keys",
            "openziti",
        ] {
            fs::DirBuilder::new()
                .recursive(true)
                .mode(0o700)
                .create(state_dir.join(dir))?;
        }
        Ok(Self {
            state_dir: state_dir.to_owned(),
            config,
        })
    }
    fn path(&self, directory: &str, id: &str) -> Result<PathBuf> {
        Ok(self
            .state_dir
            .join(directory)
            .join(format!("{}.json", validation::safe_identifier(id)?)))
    }
    fn list(&self, directory: &str) -> Result<Vec<Value>> {
        let mut paths = fs::read_dir(self.state_dir.join(directory))?
            .map(|v| v.map(|v| v.path()))
            .collect::<std::io::Result<Vec<_>>>()?;
        paths.sort();
        paths
            .into_iter()
            .filter(|p| p.extension().is_some_and(|e| e == "json"))
            .map(|p| read_json(&p))
            .collect()
    }
    pub fn list_waypoints(&self) -> Result<Vec<Value>> {
        self.list("waypoints")
    }
    pub fn list_geopacks(&self) -> Result<Vec<Value>> {
        self.list("geopacks")
    }
    // Keep the opt-in simulation adapter's existing CLI/JSON fields together.
    #[allow(clippy::too_many_arguments)]
    pub fn add_simulated_waypoint(
        &self,
        lat: f64,
        lon: f64,
        label: Option<&str>,
        body: Option<&str>,
        visibility: &str,
        id: Option<&str>,
        payload: Value,
    ) -> Result<(Value, bool)> {
        validation::position(lat, lon)?;
        validation::visibility(visibility)?;
        if !payload.is_object() {
            return Err(invalid("payload object required"));
        }
        let generated = format!(
            "wp-{}",
            &hash(
                &json!({"body":body,"label":label,"lat":(lat*1e7).round()/1e7,"lon":(lon*1e7).round()/1e7,"payload":payload,"source":"meshtastic-simulated"})
            )[..24]
        );
        let id = id.unwrap_or(&generated);
        let path = self.path("waypoints", id)?;
        if path.exists() {
            return Ok((read_json(&path)?, false));
        }
        let time = utc_now();
        let mut value = json!({"id":id,"createdAt":time,"source":"meshtastic-simulated","visibility":visibility,"position":{"lat":lat,"lon":lon},"provenance":{"adapter":"radio.simulator","format":"meshtastic-position","receivedAt":time,"payload":payload}});
        if let Some(label) = label.filter(|v| !v.is_empty()) {
            value["label"] = json!(label);
        }
        if let Some(body) = body.filter(|v| !v.is_empty()) {
            value["body"] = json!(body);
        }
        atomic_json(&path, &value)?;
        Ok((value, true))
    }
    fn key(&self) -> Result<Vec<u8>> {
        let path = self.state_dir.join("keys/geopack-signing.key");
        if path.exists() {
            return Ok(STANDARD.decode(fs::read_to_string(path)?.trim())?);
        }
        let mut key = vec![0; 32];
        SysRng.try_fill_bytes(&mut key)?;
        match fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&path)
        {
            Ok(mut file) => {
                writeln!(file, "{}", STANDARD.encode(&key))?;
                file.sync_all()?;
                Ok(key)
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                Ok(STANDARD.decode(fs::read_to_string(path)?.trim())?)
            }
            Err(error) => Err(error.into()),
        }
    }
    fn sign(&self, mut value: Value) -> Result<Value> {
        value
            .as_object_mut()
            .ok_or_else(|| invalid("manifest object required"))?
            .remove("signature");
        let mut mac = Hmac::<Sha256>::new_from_slice(&self.key()?)?;
        mac.update(&serde_json::to_vec(&value)?);
        value["signature"] = json!(format!(
            "hmac-sha256:{}",
            hex::encode(mac.finalize().into_bytes())
        ));
        Ok(value)
    }
    pub fn create_geopack(
        &self,
        name: &str,
        mut ids: Vec<String>,
        visibility: &str,
        id: Option<&str>,
    ) -> Result<(Value, bool)> {
        if name.trim().is_empty() {
            return Err(invalid("GeoPack name required"));
        }
        validation::visibility(visibility)?;
        ids.sort();
        ids.dedup();
        if ids.is_empty() {
            return Err(invalid("at least one waypoint required"));
        }
        let mut projections = Vec::new();
        for id in &ids {
            let waypoint = read_json(&self.path("waypoints", id)?)?;
            if visibility == "public" && waypoint["visibility"] != "public" {
                return Err(invalid("public GeoPacks require public waypoints"));
            }
            let mut projected = json!({"id":waypoint["id"],"source":waypoint["source"],"visibility":waypoint["visibility"],"position":waypoint["position"],"provenance":{"adapter":waypoint["provenance"]["adapter"],"format":waypoint["provenance"]["format"],"payload":waypoint["provenance"]["payload"]}});
            for optional in ["label", "body"] {
                if let Some(value) = waypoint.get(optional) {
                    projected[optional] = value.clone();
                }
            }
            projections.push(projected);
        }
        let content_hash = format!(
            "sha256:{}",
            hash(&json!({"format":"mish-geopack-content","waypoints":projections}))
        );
        let slug = name
            .chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() {
                    c.to_ascii_lowercase()
                } else {
                    '-'
                }
            })
            .collect::<String>();
        let slug = slug
            .split('-')
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
            .join("-");
        let slug = if slug.is_empty() { "geopack" } else { &slug };
        let generated = format!(
            "{}-{}",
            slug,
            &hash(&json!({"name":name,"waypoints":ids,"hash":content_hash}))[..12]
        );
        let id = id.unwrap_or(&generated);
        let path = self.path("geopacks", id)?;
        if path.exists() {
            return Ok((read_json(&path)?, false));
        }
        let value = self.sign(json!({"version":"0.1","id":id,"name":name,"createdAt":utc_now(),"waypointIds":ids,"contentHash":content_hash,"signer":format!("mish:{}",&hex::encode(Sha256::digest(self.key()?))[..32]),"publication":{"visibility":visibility,"publishedAt":null,"relays":[],"eventId":null}}))?;
        atomic_json(&path, &value)?;
        Ok((value, true))
    }
    pub fn publish_geopack(&self, id: &str, relays: Vec<String>) -> Result<Value> {
        let path = self.path("geopacks", id)?;
        let mut manifest = read_json(&path)?;
        if manifest["publication"]["visibility"] != "public" {
            return Err(invalid("GeoPack must be public before publication"));
        }
        for id in manifest["waypointIds"]
            .as_array()
            .ok_or_else(|| invalid("waypoint list required"))?
        {
            if read_json(
                &self.path(
                    "waypoints",
                    id.as_str()
                        .ok_or_else(|| invalid("waypoint identifier required"))?,
                )?,
            )?["visibility"]
                != "public"
            {
                return Err(invalid("private waypoints cannot be published"));
            }
        }
        let relays = if relays.is_empty() {
            self.config["nostr"]["relays"]
                .as_array()
                .map(|a| {
                    a.iter()
                        .filter_map(|v| v.as_str().map(String::from))
                        .collect()
                })
                .unwrap_or_default()
        } else {
            relays
        };
        let relays: Vec<String> = if relays.is_empty() {
            vec!["mock://local".into()]
        } else {
            relays
        };
        let unsigned = json!({"pubkey":manifest["signer"],"created_at":std::env::var("MISH_FIXED_UNIX_TIME").ok().and_then(|s| s.parse::<i64>().ok()).unwrap_or_else(|| chrono::Utc::now().timestamp()),"kind":30078,"tags":[["d",id],["application","mish"],["format","geopack-manifest"],["contentHash",manifest["contentHash"].as_str().unwrap_or_default()]],"content":serde_json::to_string(&manifest)?});
        let mut mac = Hmac::<Sha256>::new_from_slice(&self.key()?)?;
        mac.update(&serde_json::to_vec(&unsigned)?);
        let mut event = unsigned.clone();
        event["id"] = json!(hash(&unsigned));
        event["sig"] = json!(format!(
            "hmac-sha256:{}",
            hex::encode(mac.finalize().into_bytes())
        ));
        let mut statuses = Vec::new();
        for relay in &relays {
            let (target, status) = if let Some(file) = relay.strip_prefix("file://") {
                let target = PathBuf::from(file);
                if target.extension().is_some() {
                    if let Some(parent) = target.parent() {
                        fs::create_dir_all(parent)?;
                    }
                    let mut output = fs::OpenOptions::new()
                        .create(true)
                        .append(true)
                        .mode(0o600)
                        .open(&target)?;
                    writeln!(output, "{}", serde_json::to_string(&event)?)?;
                    output.sync_all()?;
                    (target, "appended")
                } else {
                    let target = target.join(format!("{id}.json"));
                    atomic_json(&target, &json!({"relay":relay,"event":event}))?;
                    (target, "stored")
                }
            } else {
                let queued = !relay.starts_with("mock://");
                let target = self
                    .state_dir
                    .join("nostr/publications")
                    .join(format!("{id}{}.json", if queued { ".outbox" } else { "" }));
                atomic_json(&target, &json!({"relay":relay,"event":event}))?;
                (target, if queued { "queued" } else { "stored" })
            };
            statuses.push(json!({"relay":relay,"path":target,"status":status}));
        }
        manifest["publication"]["publishedAt"] = json!(utc_now());
        manifest["publication"]["relays"] = json!(relays);
        manifest["publication"]["eventId"] = event["id"].clone();
        manifest = self.sign(manifest)?;
        atomic_json(&path, &manifest)?;
        Ok(json!({"pack":manifest,"event":event,"relays":statuses}))
    }
    pub fn bootstrap_operator(&self) -> Result<(Value, bool)> {
        let path = self.state_dir.join("openziti/operator-enrollment.json");
        if path.exists() {
            return Ok((read_json(&path)?, false));
        }
        let mut token = [0; 32];
        SysRng.try_fill_bytes(&mut token)?;
        let value = json!({"version":"0.1","identity":"operator","createdAt":utc_now(),"enrollmentToken":base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(token),"mfaRequired":self.config["openziti"]["privilegedMfaRequired"].as_bool().unwrap_or(true),"policies":["privileged-admin","service-dial","operator-bootstrap"]});
        atomic_json(&path, &value)?;
        Ok((value, true))
    }
}
pub async fn status_payload(config: &Value) -> Value {
    let mut value = json!({"ok":true,"version":config["version"],"profile":config["profile"],"domain":config["domain"],"nixDistribution":config["nixDistribution"],"stateDir":config["stateDir"],"matrix":{"provider":config["matrix"]["provider"],"domain":config["matrix"]["domain"],"releaseGated":config["matrix"]["releaseGated"]},"openziti":{"enabled":config["openziti"]["enable"],"privilegedMfaRequired":config["openziti"]["privilegedMfaRequired"],"serviceIdentities":config["openziti"]["serviceIdentities"]},"geopack":{"enabled":config["geopack"]["enable"]},"nostr":config["nostr"],"radio":config["radio"]});
    if let Some(url) = config["matrix"]["homeserverUrl"].as_str() {
        value["matrix"]["health"] = probe(url).await;
    }
    if config["openziti"]["enable"] == true {
        for (field, url) in [
            ("controllerHealth", "controllerUrl"),
            ("edgeRouterHealth", "edgeRouterUrl"),
        ] {
            if let Some(url) = config["openziti"][url].as_str() {
                value["openziti"][field] = probe(url).await;
            }
        }
    }
    value
}
async fn probe(url: &str) -> Value {
    let result = async {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_millis(500))
            .build()?;
        client
            .get(format!("{}/health", url.trim_end_matches('/')))
            .send()
            .await?
            .error_for_status()?
            .json::<Value>()
            .await
    }
    .await;
    match result {
        Ok(response) => json!({"ok":true,"status":"reachable","response":response}),
        Err(error) => json!({"ok":false,"status":"unreachable","error":error.to_string()}),
    }
}

#[cfg(test)]
mod compatibility_tests {
    use super::*;

    #[test]
    fn persisted_geopack_key_preserves_signature_bytes() {
        let state = tempfile::tempdir().unwrap();
        let store = MishStore::new(state.path(), default_config(Some(state.path()))).unwrap();
        fs::write(
            state.path().join("keys/geopack-signing.key"),
            "AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8=\n",
        )
        .unwrap();
        // Independent Python stdlib HMAC fixture: key bytes 0..31, JSON {"name":"Fixture"}.
        let signed = store
            .sign(json!({"name":"Fixture", "signature":"replaced"}))
            .unwrap();
        assert_eq!(
            signed["signature"],
            "hmac-sha256:fb000a9eed121004eda305c62ae07901451cd98b4f706d442747f97e4e67df36"
        );
    }
}
