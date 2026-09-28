use axum::{
    extract::State,
    response::{
        Sse,
        sse::{Event, KeepAlive},
    },
};
use serde_json::Value;
use tokio_stream::{Stream, StreamExt, wrappers::BroadcastStream};

use crate::{app_state::AppState, db};

/// Server-sent stream of the carrier table. A new subscriber is sent the current
/// table immediately, then a fresh full snapshot after every applied event.
pub async fn stream(
    State(state): State<AppState>,
) -> Sse<impl Stream<Item = Result<Event, axum::Error>>> {
    let rx = state.tx.subscribe();

    let current = db::carriers::get_current(&state.db_pool)
        .await
        .ok()
        .flatten();
    let initial = tokio_stream::iter(current.map(|snapshot| Ok(snapshot_event(&snapshot))));

    let updates = BroadcastStream::new(rx).filter_map(|msg| match msg {
        Ok(snapshot) => Some(Ok(snapshot_event(&snapshot))),
        // A subscriber that fell behind skips missed snapshots; each published
        // table is a full replacement, so the next one is enough to catch up.
        Err(_lagged) => None,
    });

    Sse::new(initial.chain(updates)).keep_alive(KeepAlive::default())
}

fn snapshot_event(snapshot: &Value) -> Event {
    Event::default()
        .event("carrier")
        .json_data(snapshot)
        .expect("carrier snapshot is always serializable")
}
