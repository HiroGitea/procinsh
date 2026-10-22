use super::super::AppState;
use axum::{Json, extract::State};
use serde_json::{Value, json};
use std::sync::Arc;

/// `{"local": name, "hosts": [summary...]}` with the local host first.
pub(super) async fn hosts(State(s): State<Arc<AppState>>) -> Json<Value> {
    Json(json!({
        "local": s.hosts.local().name,
        "hosts": s.hosts.summaries(s.system_monitor.captured_at()),
    }))
}
