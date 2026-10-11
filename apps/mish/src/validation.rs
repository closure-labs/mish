use crate::{domain::RecordId, invalid};
use anyhow::Result;
use serde_json::Value;
pub fn safe_identifier(value: &str) -> Result<&str> {
    RecordId::new(value)?;
    Ok(value)
}
pub fn visibility(value: &str) -> Result<&str> {
    if !matches!(value, "public" | "friends" | "private") {
        return Err(invalid("invalid visibility"));
    }
    Ok(value)
}
pub fn position(lat: f64, lon: f64) -> Result<()> {
    if !lat.is_finite()
        || !lon.is_finite()
        || !(-90.0..=90.0).contains(&lat)
        || !(-180.0..=180.0).contains(&lon)
    {
        return Err(invalid("position out of range"));
    }
    Ok(())
}
fn fields(value: &Value, required: &[&str]) -> Result<()> {
    let object = value
        .as_object()
        .ok_or_else(|| invalid("JSON object required"))?;
    for field in required {
        if !object.contains_key(*field) {
            return Err(invalid(format!("missing {field}")));
        }
    }
    Ok(())
}
pub fn waypoint(value: &Value) -> Result<()> {
    fields(
        value,
        &[
            "id",
            "createdAt",
            "source",
            "visibility",
            "position",
            "provenance",
        ],
    )?;
    safe_identifier(
        value["id"]
            .as_str()
            .ok_or_else(|| invalid("identifier required"))?,
    )?;
    visibility(value["visibility"].as_str().unwrap_or_default())?;
    position(
        value["position"]["lat"]
            .as_f64()
            .ok_or_else(|| invalid("latitude required"))?,
        value["position"]["lon"]
            .as_f64()
            .ok_or_else(|| invalid("longitude required"))?,
    )
}
pub fn geopack(value: &Value) -> Result<()> {
    fields(
        value,
        &[
            "version",
            "id",
            "name",
            "createdAt",
            "waypointIds",
            "contentHash",
            "signer",
            "publication",
            "signature",
        ],
    )?;
    if value["version"] != "0.1"
        || !value["contentHash"]
            .as_str()
            .unwrap_or_default()
            .starts_with("sha256:")
        || !value["signature"]
            .as_str()
            .unwrap_or_default()
            .starts_with("hmac-sha256:")
        || value["waypointIds"].as_array().is_none_or(|a| a.is_empty())
    {
        return Err(invalid("invalid GeoPack manifest"));
    }
    visibility(
        value["publication"]["visibility"]
            .as_str()
            .unwrap_or_default(),
    )?;
    Ok(())
}
pub fn catalog(value: &Value) -> Result<()> {
    fields(
        value,
        &[
            "version",
            "signedAt",
            "signer",
            "profiles",
            "capabilities",
            "policies",
            "signature",
        ],
    )?;
    for profile in ["mini-pc", "sbc-rpi4", "sbc-rpi5-nvme", "home-server"] {
        if !value["profiles"][profile].is_object() {
            return Err(invalid("catalog profile required"));
        }
    }
    let p = &value["profiles"];
    if p["mini-pc"]["defaultMatrixProvider"] != "tuwunel"
        || p["sbc-rpi4"]["defaultMatrixProvider"] != "conduit"
        || p["mini-pc"]["releaseGated"] != true
        || p["sbc-rpi4"]["releaseGated"] != true
        || p["sbc-rpi5-nvme"]["releaseGated"] != false
        || p["sbc-rpi5-nvme"]["experimental"] != true
        || p["home-server"]["releaseGated"] != false
        || value["policies"]["matrix"]["denyDualBackend"] != true
    {
        return Err(invalid("invalid catalog policy"));
    }
    for (field, provider) in [
        ("providers", "meshtastic-simulated"),
        ("providers", "reticulum-rnode"),
        ("releaseGatedProviders", "meshtastic-simulated"),
        ("experimentalProviders", "reticulum-rnode"),
    ] {
        if !value["capabilities"]["radio"][field]
            .as_array()
            .is_some_and(|items| items.iter().any(|v| v == provider))
        {
            return Err(invalid("invalid radio capability"));
        }
    }
    Ok(())
}
