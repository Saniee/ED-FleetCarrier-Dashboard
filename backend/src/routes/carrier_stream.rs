use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{
        Sse,
        sse::{Event, KeepAlive},
    },
};
use serde_json::Value;
use tokio_stream::{Stream, StreamExt, wrappers::BroadcastStream};

use crate::{
    app_state::{AppState, Update},
    auth::Viewer,
    db,
};

use super::{carriers::resolve_visible, market};

/// Server-sent stream for one carrier, addressed by callsign. Authorised like
/// the other reads; EventSource cannot send headers, so a session token may be
/// passed as `?access_token=`.
pub async fn stream_by_callsign(
    Path(callsign): Path<String>,
    State(state): State<AppState>,
    viewer: Viewer,
) -> Result<Sse<impl Stream<Item = Result<Event, axum::Error>>>, StatusCode> {
    let carrier_id = resolve_visible(&state, &callsign, &viewer).await?;

    let carrier = db::carriers::get(&state.db_pool, carrier_id)
        .await
        .ok()
        .flatten();

    Ok(open(state, carrier, Some(carrier_id)).await)
}

/// A new subscriber is sent the carrier and its market immediately, then a
/// fresh snapshot of whichever changed after every applied event. Two named
/// events travel this one connection — `carrier` and `market` — so the frontend
/// needs a single `EventSource`. `only` restricts updates to one carrier.
///
/// Visibility is checked once, when the stream opens; a carrier switched to
/// `owner_only` afterwards keeps feeding streams that were already open.
async fn open(
    state: AppState,
    carrier: Option<Value>,
    only: Option<i64>,
) -> Sse<impl Stream<Item = Result<Event, axum::Error>>> {
    // Subscribe before reading anything else so no update is missed in between.
    let rx = state.tx.subscribe();

    let mut initial: Vec<Result<Event, axum::Error>> = Vec::new();

    if let Some(snapshot) = &carrier {
        initial.push(Ok(carrier_event(snapshot)));
    }

    if let Some(carrier_id) = carrier_id_of(carrier.as_ref())
        && let Ok(Some(snapshot)) = market::snapshot(&state.db_pool, carrier_id).await
    {
        initial.push(Ok(market_event(&snapshot)));
    }

    let updates = BroadcastStream::new(rx).filter_map(move |msg| match msg {
        Ok(Update::Carrier(id, snapshot)) if only.is_none_or(|o| o == id) => {
            Some(Ok(carrier_event(&snapshot)))
        }
        Ok(Update::Market(id, snapshot)) if only.is_none_or(|o| o == id) => {
            Some(Ok(market_event(&snapshot)))
        }
        // Another carrier's update, or a subscriber that fell behind and skips
        // missed snapshots; each published snapshot is a full replacement, so
        // the next one is enough to catch up.
        _ => None,
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
