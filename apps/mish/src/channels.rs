//! Independently implemented Meshtastic channel sharing wire format.
//! Protocol references: https://github.com/meshtastic/protobufs/blob/master/meshtastic/channel.proto
//! and https://github.com/meshtastic/protobufs/blob/master/meshtastic/apponly.proto.
//! Opaque settings and LoRa bytes are retained so future fields survive import/export.

use anyhow::{Context, Result, bail};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use prost::Message;
use qrcode::{QrCode, render::svg};
use serde::{Deserialize, Serialize};
use std::io::Read;

pub const MAX_CHANNELS: usize = 8;
const MAX_SETTINGS: usize = 1024;
const MAX_LORA: usize = 4096;
const MAX_SET: usize = 12_288;

/// Secret-bearing types intentionally have no Debug implementation.
#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MeshChannel {
    pub name: String,
    pub settings: Vec<u8>,
}

#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ChannelSet {
    pub settings: Vec<Vec<u8>>,
    pub lora: Option<Vec<u8>>,
}

pub struct SettingsView {
    pub name: String,
    pub psk: Vec<u8>,
    pub uplink_enabled: bool,
    pub downlink_enabled: bool,
    pub use_aead: bool,
}

#[derive(Clone, PartialEq, Message)]
struct SettingsWire {
    #[prost(bytes = "vec", tag = "2")]
    psk: Vec<u8>,
    #[prost(string, tag = "3")]
    name: String,
    #[prost(bool, tag = "5")]
    uplink: bool,
    #[prost(bool, tag = "6")]
    downlink: bool,
    #[prost(bool, tag = "8")]
    use_aead: bool,
}

// Bytes and embedded messages have the same length-delimited wire encoding.
#[derive(Clone, PartialEq, Message)]
struct SetWire {
    #[prost(bytes = "vec", repeated, tag = "1")]
    settings: Vec<Vec<u8>>,
    #[prost(bytes = "vec", optional, tag = "2")]
    lora: Option<Vec<u8>>,
}

#[derive(Clone, PartialEq, Message)]
struct OpaqueWire {}

pub fn validate_lora(bytes: &[u8]) -> Result<()> {
    if bytes.len() > MAX_LORA {
        bail!("LoRa configuration exceeds size limit");
    }
    OpaqueWire::decode(bytes).context("malformed LoRa configuration")?;
    Ok(())
}

pub fn validate_name(name: &str) -> Result<()> {
    if name.is_empty()
        || name.len() > 11
        || name.trim() != name
        || name.chars().any(char::is_control)
    {
        bail!(
            "channel name must contain 1–11 UTF-8 bytes without surrounding whitespace or control characters"
        );
    }
    if name.eq_ignore_ascii_case("admin") {
        bail!("admin is a reserved radio channel name");
    }
    Ok(())
}

/// First region word supplies the short radio name; full region remains display metadata.
/// Long names require an explicit short name to avoid accidental collisions.
pub fn mesh_name(region: &str) -> Result<String> {
    let first = region
        .split_whitespace()
        .next()
        .context("region name is empty")?;
    let stem: String = first.chars().filter(char::is_ascii_alphanumeric).collect();
    if stem.is_empty() {
        bail!("region name needs an ASCII letter or digit for its radio name");
    }
    let name = format!("{stem}Mesh");
    validate_name(&name)?;
    Ok(name)
}

pub fn generated(name: &str) -> Result<MeshChannel> {
    validate_name(name)?;
    let mut psk = vec![0u8; 32];
    // Native Linux OS entropy, failing closed if the source is unavailable.
    std::fs::File::open("/dev/urandom")
        .context("opening OS random source")?
        .read_exact(&mut psk)
        .context("generating channel key")?;
    let settings = SettingsWire {
        psk,
        name: name.into(),
        uplink: false,
        downlink: false,
        use_aead: false,
    }
    .encode_to_vec();
    Ok(MeshChannel {
        name: name.into(),
        settings,
    })
}

pub fn settings_view(bytes: &[u8]) -> Result<SettingsView> {
    if bytes.len() > MAX_SETTINGS {
        bail!("channel settings exceed size limit");
    }
    let wire = SettingsWire::decode(bytes).context("malformed channel settings")?;
    // Empty names and default single-byte keys are valid existing Meshtastic channels.
    if wire.name.len() > 11 || wire.name.chars().any(char::is_control) {
        bail!("imported radio channel name exceeds firmware limit or contains control characters");
    }
    if !matches!(wire.psk.len(), 0 | 1 | 16 | 32) {
        bail!("invalid Meshtastic PSK length");
    }
    Ok(SettingsView {
        name: wire.name,
        psk: wire.psk,
        uplink_enabled: wire.uplink,
        downlink_enabled: wire.downlink,
        use_aead: wire.use_aead,
    })
}

impl ChannelSet {
    pub fn validate(&self) -> Result<()> {
        if self.settings.is_empty() || self.settings.len() > MAX_CHANNELS {
            bail!("channel set must contain 1–8 channels");
        }
        let mut names = std::collections::HashSet::new();
        for settings in &self.settings {
            let view = settings_view(settings)?;
            if !names.insert(view.name) {
                bail!("channel set contains duplicate names");
            }
        }
        if let Some(bytes) = &self.lora {
            validate_lora(bytes)?;
        }
        Ok(())
    }
}

pub fn import_url(url: &str) -> Result<ChannelSet> {
    if url.len() > MAX_SET * 2 {
        bail!("channel URL exceeds size limit");
    }
    let (prefix, data) = url
        .trim()
        .split_once('#')
        .context("channel URL has no payload")?;
    if !matches!(
        prefix,
        "https://meshtastic.org/e/"
            | "https://www.meshtastic.org/e/"
            | "https://meshtastic.org/d/"
            | "https://www.meshtastic.org/d/"
            | "https://meshtastic.org/e/?add=true"
            | "https://www.meshtastic.org/e/?add=true"
    ) {
        bail!("unsupported Meshtastic channel URL");
    }
    let data = data.trim_end_matches('=');
    let bytes = URL_SAFE_NO_PAD
        .decode(data)
        .context("invalid channel URL encoding")?;
    if bytes.len() > MAX_SET {
        bail!("channel set exceeds size limit");
    }
    let wire = SetWire::decode(bytes.as_slice()).context("malformed channel set")?;
    let set = ChannelSet {
        settings: wire.settings,
        lora: wire.lora,
    };
    set.validate()?;
    Ok(set)
}

/// Caller supplies exactly the authorized channels. Never walks a registry or all slots.
pub fn share_url(primary: &[u8], selected: &[Vec<u8>], lora: Option<&[u8]>) -> Result<String> {
    let set = ChannelSet {
        settings: std::iter::once(primary.to_vec())
            .chain(selected.iter().cloned())
            .collect(),
        lora: lora.map(<[u8]>::to_vec),
    };
    set.validate()?;
    for settings in &set.settings {
        if settings_view(settings)?.name.eq_ignore_ascii_case("admin") {
            bail!("administrative radio channels cannot be exported for enrollment");
        }
    }
    let bytes = SetWire {
        settings: set.settings,
        lora: set.lora,
    }
    .encode_to_vec();
    if bytes.len() > MAX_SET {
        bail!("channel set exceeds size limit");
    }
    Ok(format!(
        "https://meshtastic.org/e/#{}",
        URL_SAFE_NO_PAD.encode(bytes)
    ))
}

pub fn qr_svg(url: &str) -> Result<String> {
    // Parse first: arbitrary URLs or HTML cannot enter enrollment artifacts.
    import_url(url)?;
    let code = QrCode::new(url.as_bytes())
        .context("channel enrollment payload is too large for a QR code")?;
    Ok(code.render::<svg::Color>().min_dimensions(320, 320).build())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generation_uses_full_independent_keys_and_short_names() {
        assert_eq!(mesh_name("Astral Valley").unwrap(), "AstralMesh");
        let a = generated("AstralMesh").unwrap();
        let b = generated("AstralMesh").unwrap();
        let view = settings_view(&a.settings).unwrap();
        assert_eq!(view.psk.len(), 32);
        assert_ne!(a.settings, b.settings);
        assert!(!view.uplink_enabled && !view.downlink_enabled);
        assert!(generated("ABCDEFGHIJKL").is_err());
        assert!(generated("ADMIN").is_err());
    }

    #[test]
    fn scoped_enrollment_preserves_unknown_fields_and_never_includes_other_groups() {
        let mut public = generated("AstralMesh").unwrap();
        // Future settings field 99, varint 1: preserved exactly, not interpreted.
        public.settings.extend([0x98, 0x06, 0x01]);
        let staff = generated("Staff").unwrap();
        let medical = generated("Medical").unwrap();
        let lora = vec![0x08, 0x01, 0x98, 0x06, 0x01];
        let url = share_url(
            &public.settings,
            std::slice::from_ref(&medical.settings),
            Some(&lora),
        )
        .unwrap();
        let imported = import_url(&url).unwrap();
        assert_eq!(imported.settings, vec![public.settings, medical.settings]);
        assert_eq!(imported.lora, Some(lora));
        assert!(!imported.settings.contains(&staff.settings));
        assert!(qr_svg(&url).unwrap().contains("<svg"));
    }

    #[test]
    fn malformed_and_excessive_imports_fail_closed() {
        assert!(import_url("https://example.com/e/#AA").is_err());
        assert!(import_url("https://meshtastic.org/e/#!").is_err());
        let invalid = SetWire {
            settings: vec![vec![0x12, 0x02, 0x01, 0x02]],
            lora: None,
        };
        assert!(
            import_url(&format!(
                "https://meshtastic.org/e/#{}",
                URL_SAFE_NO_PAD.encode(invalid.encode_to_vec())
            ))
            .is_err()
        );
        let settings = generated("Group").unwrap().settings;
        assert!(share_url(&settings, &vec![settings.clone(); 8], None).is_err());
        assert!(share_url(&settings, std::slice::from_ref(&settings), None).is_err());
    }
}
