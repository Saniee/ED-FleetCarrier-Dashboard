//! Internal failures become a bare 500 for the client; the real error (sqlx
//! names tables and columns) goes to the server log.

use std::fmt::Display;

use axum::http::StatusCode;

pub fn internal<E: Display>(err: E) -> StatusCode {
    eprintln!("internal error: {err}");
    StatusCode::INTERNAL_SERVER_ERROR
}

pub fn internal_msg<E: Display>(err: E) -> (StatusCode, String) {
    (internal(err), "internal error".to_string())
}
