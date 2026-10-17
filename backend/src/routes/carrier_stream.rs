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
/// the other reads, via the `Authorization` header (the frontend reads the
/// stream with `fetch`, since `EventSource` cannot send headers).
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
    let who = viewer.0.as_ref().map(|u| (u.id, u.is_admin()));

    Ok(open(state, carrier, carrier_id, who).await)
}

/// What one broadcast message means for a stream.
enum Next {
    Send(Event),
    Skip,
    /// The viewer may no longer read the carrier.
    End,
}

/// A new subscriber gets the carrier and its market immediately, then a fresh
/// snapshot after every applied event, as named events (`carrier`, `market`) on
/// one connection. Each carrier snapshot re-checks what `resolve_visible`
/// checked at open, so a carrier made `owner_only`, hidden or given away ends
/// the streams that may no longer read it (`viewer` is `(user id, is admin)`).
async fn open(
    state: AppState,
    carrier: Option<Value>,
    carrier_id: i64,
    viewer: Option<(i64, bool)>,
) -> Sse<impl Stream<Item = Result<Event, axum::Error>>> {
    // Subscribe before reading anything else so no update is missed in between.
    let rx = state.tx.subscribe();

    let mut initial: Vec<Result<Event, axum::Error>> = Vec::new();

    if let Some(snapshot) = &carrier {
        initial.push(Ok(carrier_event(snapshot)));
    }

    if let Ok(Some(snapshot)) = market::snapshot(&state.db_pool, carrier_id).await {
        initial.push(Ok(market_event(&snapshot)));
    }

    let updates = BroadcastStream::new(rx)
        .map(move |msg| match msg {
            Ok(Update::Carrier(id, snapshot)) if id == carrier_id => {
                if still_visible(&snapshot, viewer) {
                    Next::Send(carrier_event(&snapshot))
                } else {
                    Next::End
                }
            }
            Ok(Update::Market(id, snapshot)) if id == carrier_id => Next::Send(market_event(&snapshot)),
            // Another carrier's update, or a subscriber that fell behind and skips
            // missed snapshots; each published snapshot is a full replacement, so
            // the next one is enough to catch up.
            _ => Next::Skip,
        })
        .take_while(|next| !matches!(next, Next::End))
        .filter_map(|next| match next {
            Next::Send(event) => Some(Ok(event)),
            _ => None,
        });

    Sse::new(tokio_stream::iter(initial).chain(updates)).keep_alive(KeepAlive::default())
}

/// `resolve_visible`'s owner-only and hidden rules, judged from a fresh carrier
/// snapshot (a `to_jsonb` row). A disabled owner isn't visible in the row; their
/// tokens stop working, so only visitors' reports still reach open streams, and
/// the page answers 404 on its next load.
fn still_visible(snapshot: &Value, viewer: Option<(i64, bool)>) -> bool {
    let owner = snapshot.get("owner_id").and_then(Value::as_i64);
    let is_owner = owner.is_some() && owner == viewer.map(|(id, _)| id);
    let is_admin = viewer.is_some_and(|(_, admin)| admin);
    let hidden = snapshot.get("moderation_hidden_at").is_some_and(|v| !v.is_null());
    let owner_only = snapshot.get("visibility").and_then(Value::as_str) == Some("owner_only");
    !(hidden && !is_owner && !is_admin) && !(owner_only && !is_owner)
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

#[cfg(test)]
mod tests {
    use super::still_visible;
    use serde_json::json;

    #[test]
    fn visibility_is_rechecked_from_the_snapshot() {
        let public = json!({ "owner_id": 1, "visibility": "public", "moderation_hidden_at": null });
        let owner_only = json!({ "owner_id": 1, "visibility": "owner_only", "moderation_hidden_at": null });
        let hidden = json!({ "owner_id": 1, "visibility": "public", "moderation_hidden_at": "2026-10-17T00:00:00Z" });
        let unowned_only = json!({ "owner_id": null, "visibility": "owner_only", "moderation_hidden_at": null });
        let (anon, owner, other, admin) = (None, Some((1, false)), Some((2, false)), Some((3, true)));

        assert!([anon, owner, other, admin].into_iter().all(|v| still_visible(&public, v)));
        assert!(still_visible(&owner_only, owner));
        assert!(![anon, other, admin].into_iter().any(|v| still_visible(&owner_only, v)));
        assert!(still_visible(&hidden, owner) && still_visible(&hidden, admin));
        assert!(!still_visible(&hidden, anon) && !still_visible(&hidden, other));
        // Released while owner-only: nobody is the owner any more.
        assert!(!still_visible(&unowned_only, anon));
    }
}
