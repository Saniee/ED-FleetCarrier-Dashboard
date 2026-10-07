//! EDSM lookups.
//!
//! The only thing this project needs from EDSM is a body's name: `CarrierLocation`
//! carries a `BodyID` and nothing else. EDSM's system endpoint returns every body
//! keyed by that same `BodyID`, which is why it is the source and Spansh is not —
//! Spansh only exposes 64-bit body ids, so it cannot be keyed by the journal's.
//!
//! Every call is a whole system, so the caller is expected to cache the result
//! (`db::body_names`). At one call per new system this is nowhere near a rate
//! limit worth worrying about.

use std::time::Duration;

use serde::Deserialize;

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

/// Every body EDSM knows for a system.
///
/// An unexplored system is not an error: EDSM answers `{}` and this returns an
/// empty list, which the caller records as a miss so it is not asked again.
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
