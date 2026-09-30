use crate::state::State;
use axum::extract::State as AxumState;
use axum::response::{Html, IntoResponse};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use std::sync::atomic::Ordering;
use std::sync::Arc;

pub async fn run(state: Arc<State>) -> std::io::Result<()> {
    let app = Router::new()
        .route("/", get(index))
        .route("/api/status", get(status))
        .route("/api/backend", post(action))
        .with_state(state.clone());
    let listener = tokio::net::TcpListener::bind(state.admin).await?;
    axum::serve(listener, app)
        .await
        .map_err(std::io::Error::other)
}

async fn index() -> Html<&'static str> {
    Html(include_str!("../ui/index.html"))
}

#[derive(Serialize)]
struct Status {
    listen: String,
    admin: String,
    active: u64,
    opened: u64,
    failed: u64,
    rejected: u64,
    bytes: u64,
    max_conns: u64,
    backends: Vec<BackendJson>,
}

#[derive(Serialize)]
struct BackendJson {
    addr: String,
    healthy: bool,
    draining: bool,
    disabled: bool,
    active: u64,
    total: u64,
    connect_fail: u64,
}

async fn status(AxumState(s): AxumState<Arc<State>>) -> Json<Status> {
    Json(Status {
        listen: s.listen.to_string(),
        admin: s.admin.to_string(),
        active: s.active.load(Ordering::Relaxed),
        opened: s.opened.load(Ordering::Relaxed),
        failed: s.failed.load(Ordering::Relaxed),
        rejected: s.rejected.load(Ordering::Relaxed),
        bytes: s.bytes.load(Ordering::Relaxed),
        max_conns: crate::state::MAX_CONNS,
        backends: s
            .backends
            .iter()
            .map(|b| BackendJson {
                addr: b.addr.to_string(),
                healthy: b.healthy.load(Ordering::Relaxed),
                draining: b.draining.load(Ordering::Relaxed),
                disabled: b.disabled.load(Ordering::Relaxed),
                active: b.active.load(Ordering::Relaxed),
                total: b.total.load(Ordering::Relaxed),
                connect_fail: b.connect_fail.load(Ordering::Relaxed),
            })
            .collect(),
    })
}

#[derive(Deserialize)]
struct Action {
    addr: String,
    action: String,
}

async fn action(
    AxumState(s): AxumState<Arc<State>>,
    Json(body): Json<Action>,
) -> impl IntoResponse {
    let addr: SocketAddr = match body.addr.parse() {
        Ok(a) => a,
        Err(_) => return (axum::http::StatusCode::BAD_REQUEST, "bad addr").into_response(),
    };
    let Some(b) = s.by_addr(addr) else {
        return (axum::http::StatusCode::NOT_FOUND, "unknown backend").into_response();
    };
    match body.action.as_str() {
        "drain" => {
            b.draining.store(true, Ordering::Relaxed);
        }
        "enable" => {
            b.disabled.store(false, Ordering::Relaxed);
            b.draining.store(false, Ordering::Relaxed);
            b.consec_ok.store(0, Ordering::Relaxed);
            b.consec_fail.store(0, Ordering::Relaxed);
            b.healthy.store(false, Ordering::Relaxed);
        }
        "disable" => {
            b.disabled.store(true, Ordering::Relaxed);
            b.healthy.store(false, Ordering::Relaxed);
        }
        _ => return (axum::http::StatusCode::BAD_REQUEST, "bad action").into_response(),
    }
    axum::http::StatusCode::NO_CONTENT.into_response()
}
