use axum::{
    extract::State,
    response::{
        Sse,
        sse::{Event, KeepAlive},
    },
};
use serde_json::Value;
use tokio_stream::{Stream, StreamExt, wrappers::BroadcastStream};

use crate::{
    app_state::{AppState, Update},
    db,
};

use super::market;

/// Server-sent stream of dashboard state.
///
/// A new subscriber is sent the current carrier and its market immediately, then
/// a fresh snapshot of whichever changed after every applied event. Two named
/// events travel this one connection — `carrier` and `market` — so the frontend
/// needs a single `EventSource`.
pub async fn stream(
    State(state): State<AppState>,
) -> Sse<impl Stream<Item = Result<Event, axum::Error>>> {
    let rx = state.tx.subscribe();

    let carrier = db::carriers::get_current(&state.db_pool)
        .await
        .ok()
        .flatten();

    let mut initial: Vec<Result<Event, axum::Error>> = Vec::new();

    if let Some(snapshot) = &carrier {
        initial.push(Ok(carrier_event(snapshot)));
    }

    // The market belongs to the current carrier, and there may not be one yet.
    if let Some(carrier_id) = carrier_id_of(carrier.as_ref())
        && let Ok(Some(snapshot)) = market::snapshot(&state.db_pool, carrier_id).await
    {
        initial.push(Ok(market_event(&snapshot)));
    }

    let updates = BroadcastStream::new(rx).filter_map(|msg| match msg {
        Ok(Update::Carrier(snapshot)) => Some(Ok(carrier_event(&snapshot))),
        Ok(Update::Market(snapshot)) => Some(Ok(market_event(&snapshot))),
        // A subscriber that fell behind skips missed snapshots; each published
        // snapshot is a full replacement, so the next one is enough to catch up.
        Err(_lagged) => None,
    });

    Sse::new(tokio_stream::iter(initial).chain(updates)).keep_alive(KeepAlive::default())
}

/// The `carrier_id` of a carrier snapshot. Rows are serialized with `to_jsonb`,
/// so the key is the column name.
fn carrier_id_of(snapshot: Option<&Value>) -> Option<i64> {
    snapshot?.get("carrier_id")?.as_i64()
}

fn carrier_event(snapshot: &Value) -> Event {
    Event::default()
        .event("carrier")
        .json_data(snapshot)
        .expect("carrier snapshot is always serializable")
}

fn market_event(snapshot: &Value) -> Event {
    Event::default()
        .event("market")
        .json_data(snapshot)
        .expect("market snapshot is always serializable")
}
