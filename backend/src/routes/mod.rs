mod carrier;
mod carrier_event;
mod carrier_stream;
mod healthz;

use axum::{Router, routing::{get, post}};
use crate::app_state::AppState;

pub fn router(app: AppState) -> Router {
    let api = Router::new()
        .route("/api/healthz", get(healthz::healthz))
        .route("/api/carrier", get(carrier::get_current))
        .route("/api/carrier/{carrier_id}", get(carrier::get_by_id))
        .route("/api/carrier/event", post(carrier_event::post))
        .route("/api/carrier/stream", get(carrier_stream::stream));

    Router::new()
        .merge(api)
        .with_state(app)
}
