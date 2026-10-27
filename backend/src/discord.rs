//! Discord webhook delivery. Ingest queues messages and returns; one background
//! task posts them, so a slow or rate-limited Discord never holds up the plugin.

use std::time::Duration;

use chrono::DateTime;
use reqwest::{StatusCode, Url};
use serde_json::{Value, json};
use sqlx::PgPool;
use tokio::sync::mpsc;

use crate::{
    db::webhooks,
    notify::{Notice, Side},
};

/// Messages waiting to go out. Beyond this ingest drops new ones (and logs it)
/// rather than queue without bound.
const QUEUE: usize = 256;

/// Tries per message; a 429 waits as long as Discord asks, up to `MAX_WAIT`.
const ATTEMPTS: u32 = 3;
const MAX_WAIT: Duration = Duration::from_secs(30);

const HOSTS: [&str; 4] = ["discord.com", "discordapp.com", "ptb.discord.com", "canary.discord.com"];

/// The URL, normalised, if it is a Discord webhook. Anything else is refused so
/// the server can't be pointed at arbitrary addresses.
pub fn webhook_url(input: &str) -> Option<String> {
    let url = Url::parse(input.trim()).ok()?;
    if url.scheme() != "https" || url.port().is_some() || !url.username().is_empty() {
        return None;
    }
    if !HOSTS.contains(&url.host_str()?) {
        return None;
    }

    // /api[/v10]/webhooks/{id}/{token}
    let mut segments: Vec<&str> = url.path_segments()?.filter(|s| !s.is_empty()).collect();
    if segments.len() == 5 && segments[1].starts_with('v') && segments[1][1..].bytes().all(|b| b.is_ascii_digit()) {
        segments.remove(1);
    }
    let [api, hooks, id, token] = segments[..] else { return None };
    let token_ok = !token.is_empty()
        && token.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_');
    if api != "api" || hooks != "webhooks" || id.is_empty() || !id.bytes().all(|b| b.is_ascii_digit()) || !token_ok {
        return None;
    }
    Some(format!("https://discord.com/api/webhooks/{id}/{token}"))
}

/// The webhook with its token hidden, for showing back to the owner.
pub fn masked(url: &str) -> String {
    match url.rsplit_once('/') {
        Some((head, _)) => format!("{head}/••••••"),
        None => "••••••".to_string(),
    }
}

/// Which carrier a message is about, for its title and link.
pub struct Carrier<'a> {
    pub name: Option<&'a str>,
    pub callsign: Option<&'a str>,
    /// The site's public address (`PUBLIC_ORIGIN`), to link the carrier's page.
    pub origin: Option<&'a str>,
}

impl Carrier<'_> {
    pub fn from_row<'a>(row: &'a Value, origin: Option<&'a str>) -> Carrier<'a> {
        Carrier {
            name: row.get("name").and_then(Value::as_str),
            callsign: row.get("callsign").and_then(Value::as_str),
            origin,
        }
    }
}

fn credits(n: i64) -> String {
    let digits = n.unsigned_abs().to_string();
    let mut out = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    if n < 0 { format!("-{out} CR") } else { format!("{out} CR") }
}

/// Discord's own timestamp markup, which each reader sees in their time zone.
fn when(timestamp: &str) -> String {
    match DateTime::parse_from_rfc3339(timestamp) {
        Ok(t) => format!("<t:{0}:t> (<t:{0}:R>)", t.timestamp()),
        Err(_) => timestamp.to_string(),
    }
}

fn place(system: &str, body: Option<&str>) -> String {
    match body {
        Some(body) if !body.is_empty() && body != system => format!("**{body}**, {system}"),
        _ => format!("**{system}**"),
    }
}

/// The webhook body for one notice.
pub fn message(notice: &Notice, carrier: &Carrier) -> Value {
    const BLUE: u32 = 0x5865F2;
    const GREEN: u32 = 0x57F287;
    const YELLOW: u32 = 0xFEE75C;
    const RED: u32 = 0xED4245;
    const GREY: u32 = 0x99AAB5;

    let (title, description, color) = match notice {
        Notice::JumpScheduled { system, body, departure } => (
            "Jump scheduled",
            format!("Jumping to {}\nDeparts {}", place(system, body.as_deref()), when(departure)),
            BLUE,
        ),
        Notice::JumpCancelled { system } => ("Jump cancelled", format!("No longer jumping to **{system}**."), GREY),
        Notice::Arrived { system, body } => ("Arrived", format!("Now at {}", place(system, body.as_deref())), GREEN),
        Notice::OrderPosted { commodity, side, quantity, price } => {
            let verb = match side {
                Side::Buy => "Buying",
                Side::Sell => "Selling",
            };
            let price = price.map(|p| format!(" at {} each", credits(p))).unwrap_or_default();
            ("Trade order", format!("{verb} {quantity} t of **{commodity}**{price}"), YELLOW)
        }
        Notice::OrderCancelled { commodity } => ("Trade order cancelled", format!("**{commodity}** order removed."), GREY),
        Notice::OrderFilled { commodity, side } => {
            let what = match side {
                Side::Buy => "Buy order filled",
                Side::Sell => "Sold out",
            };
            (what, format!("**{commodity}**"), GREEN)
        }
        Notice::LowFuel { level, threshold } => (
            "Low tritium",
            format!("{level} t of tritium left (alert below {threshold} t)."),
            RED,
        ),
    };

    let name = carrier.name.unwrap_or("Fleet carrier");
    let author = match carrier.callsign {
        Some(cs) => format!("{name} ({cs})"),
        None => name.to_string(),
    };
    let mut author = json!({ "name": author });
    if let (Some(origin), Some(cs)) = (carrier.origin, carrier.callsign) {
        author["url"] = json!(format!("{}/carrier/{cs}", origin.trim_end_matches('/')));
    }

    json!({
        "embeds": [{
            "author": author,
            "title": title,
            "description": description,
            "color": color,
        }],
        // Names and commodities come from the journal; never let them ping.
        "allowed_mentions": { "parse": [] },
    })
}

pub fn test_message(carrier: &Carrier) -> Value {
    let mut body = message(
        &Notice::Arrived { system: "Sol".into(), body: None },
        carrier,
    );
    body["embeds"][0]["title"] = json!("Test message");
    body["embeds"][0]["description"] = json!("This channel will get this carrier's notifications.");
    body
}

#[derive(Debug)]
pub enum SendError {
    /// Discord says the webhook doesn't exist (deleted, or a bad token).
    Gone,
    RateLimited(Duration),
    Failed(String),
}

impl SendError {
    pub fn describe(&self) -> String {
        match self {
            SendError::Gone => "Discord says this webhook no longer exists".into(),
            SendError::RateLimited(_) => "rate limited by Discord".into(),
            SendError::Failed(msg) => msg.clone(),
        }
    }
}

pub async fn post(http: &reqwest::Client, url: &str, body: &Value) -> Result<(), SendError> {
    let res = http
        .post(url)
        .json(body)
        .send()
        .await
        .map_err(|e| SendError::Failed(format!("could not reach Discord: {}", e.without_url())))?;

    match res.status() {
        s if s.is_success() => Ok(()),
        StatusCode::NOT_FOUND | StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN => Err(SendError::Gone),
        StatusCode::TOO_MANY_REQUESTS => {
            let secs = res
                .json::<Value>()
                .await
                .ok()
                .and_then(|v| v.get("retry_after").and_then(Value::as_f64))
                .unwrap_or(1.0);
            Err(SendError::RateLimited(Duration::from_secs_f64(secs.clamp(0.0, 3600.0))))
        }
        s => Err(SendError::Failed(format!("Discord answered {s}"))),
    }
}

pub struct Delivery {
    pub carrier_id: i64,
    pub url: String,
    pub body: Value,
}

pub type Queue = mpsc::Sender<Delivery>;

/// Queue a message; never waits. A full queue drops it.
pub fn enqueue(queue: &Queue, delivery: Delivery) {
    if let Err(err) = queue.try_send(delivery) {
        eprintln!("discord: message dropped ({err})");
    }
}

/// Start the sender and return its queue. Messages go out one at a time, in
/// order, which also keeps a burst of events inside Discord's limits.
pub fn spawn(pool: PgPool, http: reqwest::Client) -> Queue {
    let (tx, mut rx) = mpsc::channel::<Delivery>(QUEUE);
    tokio::spawn(async move {
        while let Some(d) = rx.recv().await {
            deliver(&pool, &http, d).await;
        }
    });
    tx
}

async fn deliver(pool: &PgPool, http: &reqwest::Client, d: Delivery) {
    let mut last = None;
    for _ in 0..ATTEMPTS {
        match post(http, &d.url, &d.body).await {
            Ok(()) => {
                if let Err(err) = webhooks::record_sent(pool, d.carrier_id, &d.url).await {
                    eprintln!("discord: could not record delivery: {err}");
                }
                return;
            }
            Err(SendError::RateLimited(wait)) if wait <= MAX_WAIT => {
                tokio::time::sleep(wait).await;
                last = Some(SendError::RateLimited(wait));
            }
            Err(err @ SendError::Gone) => {
                last = Some(err);
                break;
            }
            Err(err) => {
                tokio::time::sleep(Duration::from_secs(2)).await;
                last = Some(err);
            }
        }
    }

    let Some(err) = last else { return };
    let gone = matches!(err, SendError::Gone);
    if let Err(e) = webhooks::record_error(pool, d.carrier_id, &d.url, &err.describe(), gone).await {
        eprintln!("discord: could not record failure: {e}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_discord_webhooks_only() {
        let canonical = "https://discord.com/api/webhooks/123/abc-DEF_9";
        assert_eq!(webhook_url(canonical).as_deref(), Some(canonical));
        assert_eq!(webhook_url("https://discordapp.com/api/webhooks/123/abc").as_deref(), Some("https://discord.com/api/webhooks/123/abc"));
        assert_eq!(webhook_url("https://ptb.discord.com/api/v10/webhooks/123/abc").as_deref(), Some("https://discord.com/api/webhooks/123/abc"));
        assert_eq!(webhook_url(" https://discord.com/api/webhooks/123/abc/ ").as_deref(), Some("https://discord.com/api/webhooks/123/abc"));

        for bad in [
            "http://discord.com/api/webhooks/123/abc",
            "https://discord.com:8443/api/webhooks/123/abc",
            "https://evil.example/api/webhooks/123/abc",
            "https://discord.com.evil.example/api/webhooks/123/abc",
            "https://user@discord.com/api/webhooks/123/abc",
            "https://discord.com/api/webhooks/abc/abc",
            "https://discord.com/api/webhooks/123",
            "https://discord.com/api/webhooks/123/abc/github",
            "https://discord.com/api/webhooks/123/a%2Fb",
            "not a url",
        ] {
            assert_eq!(webhook_url(bad), None, "{bad}");
        }
    }

    #[test]
    fn mask_hides_the_token() {
        assert_eq!(masked("https://discord.com/api/webhooks/123/secret"), "https://discord.com/api/webhooks/123/••••••");
    }

    #[test]
    fn formats_credits() {
        assert_eq!(credits(0), "0 CR");
        assert_eq!(credits(1234567), "1,234,567 CR");
        assert_eq!(credits(-1500), "-1,500 CR");
    }

    #[test]
    fn messages_never_ping() {
        let carrier = Carrier { name: Some("@everyone"), callsign: Some("ABC-123"), origin: Some("https://x.example/") };
        let body = message(&Notice::OrderCancelled { commodity: "Steel".into() }, &carrier);
        assert_eq!(body["allowed_mentions"]["parse"], json!([]));
        assert_eq!(body["embeds"][0]["author"]["url"], "https://x.example/carrier/ABC-123");
    }
}
