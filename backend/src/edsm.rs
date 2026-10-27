//! EDSM lookups. A body's name: `CarrierLocation` carries just a `BodyID`,
//! which EDSM keys bodies by (Spansh only has 64-bit ids); a call returns a
//! whole system, so callers cache it in `db::body_names`. And a system's
//! coordinates, for the route tritium estimate.

use std::time::Duration;

use serde::{Deserialize, Serialize};

/// EDSM has no per-body endpoint, so the system comes back whole and the caller
/// picks the body it wants.
const BODIES_URL: &str = "https://www.edsm.net/api-system-v1/bodies";

/// EDSM prefers to know who is calling.
const USER_AGENT: &str = "ED-Commander-Site (personal fleet-carrier dashboard)";

pub fn client() -> reqwest::Client {
    reqwest::Client::builder()
        .user_agent(USER_AGENT)
        // The ingest POST waits on this and the plugin gives up at 10s, so keep
        // the lookup well inside that. Measured latency is ~0.7s.
        .timeout(Duration::from_secs(5))
        .build()
        .expect("reqwest client configuration is valid")
}

#[derive(Debug, Deserialize)]
struct SystemResponse {
    /// Absent, or null, when EDSM has never seen the system — it answers `{}`.
    bodies: Option<Vec<SystemBody>>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SystemBody {
    #[serde(rename = "bodyId")]
    pub body_id: i64,
    pub name: String,
    /// Star / Planet / ... — the journal's `BodyType`.
    #[serde(rename = "type")]
    pub body_type: Option<String>,
}

/// Every body EDSM knows for a system. An unexplored system answers `{}` and
/// yields an empty list, which the caller records as a miss.
pub async fn fetch_bodies(
    client: &reqwest::Client,
    system_address: i64,
) -> Result<Vec<SystemBody>, reqwest::Error> {
    let response: SystemResponse = client
        .get(BODIES_URL)
        .query(&[("systemId64", system_address)])
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;

    Ok(response.bodies.unwrap_or_default())
}

const SYSTEM_URL: &str = "https://www.edsm.net/api-v1/system";

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct Coords {
    pub x: f64,
    pub y: f64,
    pub z: f64,
}

#[derive(Debug, Deserialize)]
struct SystemCoords {
    name: String,
    coords: Option<Coords>,
}

/// A system's name as EDSM spells it, and where it is. `None` when EDSM doesn't
/// know the system (it answers `[]`) or has no coordinates for it.
pub async fn fetch_coords(
    client: &reqwest::Client,
    name: &str,
) -> Result<Option<(String, Coords)>, reqwest::Error> {
    let body: serde_json::Value = client
        .get(SYSTEM_URL)
        .query(&[("systemName", name), ("showCoordinates", "1")])
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;

    Ok(serde_json::from_value::<SystemCoords>(body)
        .ok()
        .and_then(|s| Some((s.name, s.coords?))))
}
