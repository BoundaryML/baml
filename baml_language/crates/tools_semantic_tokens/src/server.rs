//! Axum HTTP surface for the semantic-token viewer.
//!
//! Mirrors the pkg-grammar preview's vite middleware: list fixtures, fetch one
//! fixture's source + tokens, tokenize ad-hoc input, and accept a fixture's
//! snapshot. The frontend is a single embedded HTML page (no node build step).

use std::{
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::SystemTime,
};

use axum::{
    Json, Router,
    extract::{Query, State},
    http::StatusCode,
    response::Html,
    routing::{get, post},
};
use serde::{Deserialize, Serialize};

use crate::{
    analysis::{self, Token},
    staleness,
};

#[derive(Clone)]
pub(crate) struct AppState {
    fixtures_dir: Arc<Path>,
    /// mtime of this binary when the server started — identifies this build.
    started_exe_mtime: Option<SystemTime>,
    /// Set by the watcher while a rebuild is in flight.
    rebuilding: Arc<AtomicBool>,
}

pub(crate) fn router(
    fixtures_dir: PathBuf,
    started_exe_mtime: Option<SystemTime>,
    rebuilding: Arc<AtomicBool>,
) -> Router {
    let state = AppState {
        fixtures_dir: Arc::from(fixtures_dir),
        started_exe_mtime,
        rebuilding,
    };
    Router::new()
        .route("/", get(index))
        .route("/api/status", get(status))
        .route("/api/fixtures", get(fixtures))
        .route("/api/fixture", get(fixture))
        .route("/api/tokens", post(tokens))
        .route("/api/accept", post(accept))
        .with_state(state)
}

#[derive(Serialize)]
struct StatusResponse {
    /// Identifies this build; the frontend reloads when it changes (restart).
    build_id: u64,
    /// Whether a rebuild is currently running.
    rebuilding: bool,
}

async fn status(State(state): State<AppState>) -> Json<StatusResponse> {
    Json(StatusResponse {
        build_id: staleness::build_id(state.started_exe_mtime),
        rebuilding: state.rebuilding.load(Ordering::Relaxed),
    })
}

type ApiError = (StatusCode, String);

async fn index() -> Html<&'static str> {
    Html(include_str!("index.html"))
}

#[derive(Serialize)]
struct FixtureSummary {
    name: String,
    diff_count: usize,
}

#[derive(Serialize)]
struct FixturesResponse {
    fixtures: Vec<FixtureSummary>,
}

async fn fixtures(State(state): State<AppState>) -> Result<Json<FixturesResponse>, ApiError> {
    let dir = state.fixtures_dir.as_ref();
    let names = analysis::list_fixture_names(dir)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    // Each fixture builds its own ProjectDatabase + runs the compiler, so compute
    // the diff badges in parallel (one thread per fixture) rather than serially.
    let summaries = std::thread::scope(|scope| {
        let handles: Vec<_> = names
            .iter()
            .map(|name| {
                scope.spawn(move || {
                    let diff_count = analysis::load_fixture(&dir.join(name))
                        .map(|fx| analysis::diff_count(&fx.current, &fx.expected))
                        .unwrap_or(0);
                    FixtureSummary {
                        name: name.clone(),
                        diff_count,
                    }
                })
            })
            .collect();
        handles
            .into_iter()
            .map(|h| h.join().expect("fixture summary thread panicked"))
            .collect()
    });

    Ok(Json(FixturesResponse {
        fixtures: summaries,
    }))
}

#[derive(Deserialize)]
struct NameQuery {
    name: String,
}

#[derive(Serialize)]
struct FixtureResponse {
    source: String,
    current: Vec<Token>,
    expected: Vec<Token>,
}

async fn fixture(
    State(state): State<AppState>,
    Query(query): Query<NameQuery>,
) -> Result<Json<FixtureResponse>, ApiError> {
    let path = resolve_fixture(&state.fixtures_dir, &query.name)?;
    let fx = analysis::load_fixture(&path)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(Json(FixtureResponse {
        source: fx.source,
        current: fx.current,
        expected: fx.expected,
    }))
}

#[derive(Deserialize)]
struct SourceBody {
    source: String,
}

#[derive(Serialize)]
struct TokensResponse {
    tokens: Vec<Token>,
}

async fn tokens(Json(body): Json<SourceBody>) -> Json<TokensResponse> {
    Json(TokensResponse {
        tokens: analysis::compute_tokens(&body.source),
    })
}

#[derive(Deserialize)]
struct AcceptBody {
    name: String,
}

#[derive(Serialize)]
struct AcceptResponse {
    ok: bool,
}

async fn accept(
    State(state): State<AppState>,
    Json(body): Json<AcceptBody>,
) -> Result<Json<AcceptResponse>, ApiError> {
    let path = resolve_fixture(&state.fixtures_dir, &body.name)?;
    analysis::accept_fixture(&path)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    Ok(Json(AcceptResponse { ok: true }))
}

/// Resolve a fixture name to a path, rejecting traversal and non-`.baml` names.
fn resolve_fixture(dir: &Path, name: &str) -> Result<PathBuf, ApiError> {
    let is_basename = Path::new(name).file_name().map(|f| f.to_string_lossy())
        == Some(std::borrow::Cow::Borrowed(name));
    let is_baml = Path::new(name)
        .extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("baml"));
    if !is_basename || !is_baml {
        return Err((StatusCode::BAD_REQUEST, "invalid fixture name".to_string()));
    }
    let root = dir
        .canonicalize()
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    let path = dir.join(name);
    let meta = std::fs::symlink_metadata(&path)
        .map_err(|_| (StatusCode::NOT_FOUND, "unknown fixture".to_string()))?;
    if meta.file_type().is_symlink() {
        return Err((StatusCode::BAD_REQUEST, "invalid fixture name".to_string()));
    }
    let path = path
        .canonicalize()
        .map_err(|_| (StatusCode::NOT_FOUND, "unknown fixture".to_string()))?;
    if !path.starts_with(&root) || !path.is_file() {
        return Err((StatusCode::NOT_FOUND, "unknown fixture".to_string()));
    }
    Ok(path)
}
