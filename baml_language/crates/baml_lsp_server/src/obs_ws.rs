//! `/api/obs` — the §9.3 binary observability WebSocket.
//!
//! The client sends small JSON text messages (`query` / `sub` / `unsub`);
//! the server replies with BQF1 binary frames whose `request_id` echoes the
//! message `id`. Errors are BQF1 Status frames (kind 6), never text.
//!
//! One [`ObserveEngine`] per connection: folds are KB-scale, so
//! per-connection state is cheap, and re-calling `open_run` on each
//! subscription tick (re-reading from disk) is how live tails observe
//! growth. A frame is only sent when the run's data epoch moved (for the
//! `runs` list: when the encoded bytes differ), and frames are sent
//! serially in the session task, so at most one frame per subscription is
//! ever in flight.

use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Duration;

use axum::extract::ws::{Message as AxumWsMsg, WebSocket};
use bex_query::{ObserveEngine, bqf1};
use serde::Deserialize;

/// Subscription re-evaluation cadence. §9.3 allows up to 30 Hz; 4 Hz is
/// the v1 cadence.
const SUB_TICK: Duration = Duration::from_millis(250);

/// Left-Heavy viewport width when the client omits `pixel_width`.
const DEFAULT_PIXEL_WIDTH: u32 = 1024;

/// `top_functions` row cap when the client omits `limit`.
const DEFAULT_LIMIT: u32 = 50;

// ---------------------------------------------------------------------------
// Wire messages (client -> server; all server output is BQF1 binary)
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct ObsRequest {
    op: String,
    #[serde(default)]
    id: u64,
    method: Option<String>,
    run: Option<String>,
    pixel_width: Option<u32>,
    limit: Option<u32>,
    /// BQL query string (method `bql` only).
    q: Option<String>,
}

/// Query/subscription methods. `run_meta` is query-only (the dictionary is
/// sent once per open; later frames reference functions by id).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ObsMethod {
    Runs,
    RunMeta,
    Timeline,
    LeftHeavy,
    TopFunctions,
    /// §9.4 exact-recency tier (live same-process sessions only; other
    /// keys get an honest empty frame).
    RecentCalls,
    /// §8 BQL v1 (query-only): `q` carries the pipeline, `run` optionally
    /// scopes `ctx()`. Results are `BqlTable` frames; typed BQL errors
    /// surface as Status 422.
    Bql,
}

impl ObsMethod {
    fn parse(name: &str) -> Option<ObsMethod> {
        match name {
            "runs" => Some(ObsMethod::Runs),
            "run_meta" => Some(ObsMethod::RunMeta),
            "timeline" => Some(ObsMethod::Timeline),
            "left_heavy" => Some(ObsMethod::LeftHeavy),
            "top_functions" => Some(ObsMethod::TopFunctions),
            "recent_calls" => Some(ObsMethod::RecentCalls),
            "bql" => Some(ObsMethod::Bql),
            _ => None,
        }
    }
}

/// A validated subscription request.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct SubSpec {
    pub(crate) id: u64,
    pub(crate) method: ObsMethod,
    /// `None` only for [`ObsMethod::Runs`].
    pub(crate) run: Option<String>,
    pub(crate) pixel_width: u32,
    pub(crate) limit: u32,
}

/// What one client text message asks the session to do.
#[derive(Debug)]
pub(crate) enum Reply {
    /// Send this BQF1 frame now (query result or error Status).
    Frame(Vec<u8>),
    /// Register (or replace) the subscription with this spec's id.
    Sub(SubSpec),
    /// Drop the subscription with this id.
    Unsub(u64),
}

// ---------------------------------------------------------------------------
// Pure dispatch (socket-free; unit-tested below)
// ---------------------------------------------------------------------------

/// Dispatch one client text message against the connection's engine.
/// Malformed JSON and unknown ops/methods come back as Status frames so
/// the client sees exactly one BQF1 reply shape.
pub(crate) fn handle_text(engine: &mut ObserveEngine, text: &str, now_ns: u64) -> Reply {
    let req: ObsRequest = match serde_json::from_str(text) {
        Ok(req) => req,
        Err(err) => {
            return Reply::Frame(bqf1::status_frame(
                0,
                400,
                &format!("malformed request: {err}"),
            ));
        }
    };
    match req.op.as_str() {
        "query" => Reply::Frame(dispatch_query(engine, &req, now_ns)),
        "sub" => dispatch_sub(&req),
        "unsub" => Reply::Unsub(req.id),
        other => Reply::Frame(bqf1::status_frame(
            req.id,
            400,
            &format!("unknown op {other}"),
        )),
    }
}

fn dispatch_query(engine: &mut ObserveEngine, req: &ObsRequest, now_ns: u64) -> Vec<u8> {
    let id = req.id;
    let method = match required_method(req) {
        Ok(method) => method,
        Err(frame) => return frame,
    };
    if method == ObsMethod::Runs {
        return engine.runs_frame(id, now_ns);
    }
    if method == ObsMethod::Bql {
        return bql_frame(engine, req);
    }
    let run = match required_run_key(req) {
        Ok(run) => run,
        Err(frame) => return frame,
    };
    if method == ObsMethod::RecentCalls {
        return recent_calls_frame(run, id);
    }
    // Open lazily: subscriptions re-open every tick, but a one-shot query
    // reuses the fold cached by a previous open on this connection.
    if engine.run_epoch(run).is_none()
        && let Err(err) = open_run_preferring_live(engine, run)
    {
        return bqf1::status_frame(id, 404, &err);
    }
    match method {
        ObsMethod::RunMeta => engine.run_meta_frame(run, id),
        ObsMethod::Timeline => engine.timeline_frame(run, id),
        ObsMethod::LeftHeavy => {
            engine.left_heavy_frame(run, req.pixel_width.unwrap_or(DEFAULT_PIXEL_WIDTH), id)
        }
        ObsMethod::TopFunctions => {
            engine.top_functions_frame(run, req.limit.unwrap_or(DEFAULT_LIMIT), id)
        }
        ObsMethod::Runs | ObsMethod::RecentCalls | ObsMethod::Bql => {
            unreachable!("handled above")
        }
    }
}

/// §8 BQL over `/api/obs`: run the pipeline in `q` (with `run` as the
/// optional ctx scope) and reply with one `BqlTable` frame. Typed BQL
/// errors become Status 422 `"{code}: {message} (remedy: {remedy})"`.
fn bql_frame(engine: &mut ObserveEngine, req: &ObsRequest) -> Vec<u8> {
    let id = req.id;
    let Some(query) = req.q.as_deref() else {
        return bqf1::status_frame(id, 400, "missing q (the BQL query string)");
    };
    let run = if req.run.is_some() {
        match required_run_key(req) {
            Ok(run) => Some(run),
            Err(frame) => return frame,
        }
    } else {
        None
    };
    match bex_query::bql::run(engine, run, query) {
        Ok(table) => table.to_frame(id),
        Err(err) => bqf1::status_frame(
            id,
            422,
            &format!("{}: {} (remedy: {})", err.code, err.message, err.remedy),
        ),
    }
}

fn dispatch_sub(req: &ObsRequest) -> Reply {
    let id = req.id;
    let method = match required_method(req) {
        Ok(method) => method,
        Err(frame) => return Reply::Frame(frame),
    };
    let run = match method {
        ObsMethod::Runs => None,
        ObsMethod::RunMeta => {
            return Reply::Frame(bqf1::status_frame(id, 400, "run_meta is not subscribable"));
        }
        ObsMethod::Bql => {
            return Reply::Frame(bqf1::status_frame(id, 400, "bql is not subscribable in v1"));
        }
        ObsMethod::Timeline
        | ObsMethod::LeftHeavy
        | ObsMethod::TopFunctions
        | ObsMethod::RecentCalls => match required_run_key(req) {
            Ok(run) => Some(run.to_string()),
            Err(frame) => return Reply::Frame(frame),
        },
    };
    Reply::Sub(SubSpec {
        id,
        method,
        run,
        pixel_width: req.pixel_width.unwrap_or(DEFAULT_PIXEL_WIDTH),
        limit: req.limit.unwrap_or(DEFAULT_LIMIT),
    })
}

fn required_method(req: &ObsRequest) -> Result<ObsMethod, Vec<u8>> {
    let Some(name) = req.method.as_deref() else {
        return Err(bqf1::status_frame(req.id, 400, "missing method"));
    };
    ObsMethod::parse(name)
        .ok_or_else(|| bqf1::status_frame(req.id, 400, &format!("unknown method {name}")))
}

/// A run key must be a plain dir name under `.baml/history/` or
/// `.baml/sessions/` — never a path.
fn required_run_key(req: &ObsRequest) -> Result<&str, Vec<u8>> {
    let Some(run) = req.run.as_deref() else {
        return Err(bqf1::status_frame(req.id, 400, "missing run key"));
    };
    let plain_name = !run.is_empty()
        && run.len() <= 512
        && !run.contains(['/', '\\'])
        && run != "."
        && run != "..";
    if plain_name {
        Ok(run)
    } else {
        Err(bqf1::status_frame(req.id, 400, "invalid run key"))
    }
}

// ---------------------------------------------------------------------------
// Subscription evaluation (socket-free; unit-tested below)
// ---------------------------------------------------------------------------

/// One live subscription: its spec plus what was last sent, so a tick only
/// emits when the underlying data moved.
struct SubState {
    spec: SubSpec,
    /// `run_epoch` at the last frame sent (run-scoped methods).
    last_epoch: Option<u64>,
    /// Bytes of the last `runs` frame sent. The runs list has no epoch;
    /// frames are small, so bytes-compare is the change signal.
    last_frame: Option<Vec<u8>>,
}

impl SubState {
    fn new(spec: SubSpec) -> SubState {
        SubState {
            spec,
            last_epoch: None,
            last_frame: None,
        }
    }
}

enum SubEval {
    /// Data moved (or first evaluation): send this frame.
    Send(Vec<u8>),
    /// Nothing changed since the last frame sent.
    Unchanged,
    /// The run failed to open: send this Status frame and drop the sub.
    Failed(Vec<u8>),
}

/// §9.2 live mirror: a session owned by THIS process folds from the
/// consumer's in-RAM state (`cct_live_segment`, ~0 latency, ahead of group
/// commit) instead of disk. Any other run key — other processes' sessions,
/// boundary dirs, engines the consumer no longer retains — falls back to
/// the disk path.
fn open_run_preferring_live(engine: &mut ObserveEngine, run: &str) -> Result<(), String> {
    if let Some(engine_id) = same_process_engine_id(run)
        && let Some(bytes) =
            bex_events::prof::cct_live_segment(engine_id, std::time::Duration::from_millis(200))
    {
        let revision = bex_events::prof::metadata::get_engine_metadata(engine_id)
            .and_then(|meta| meta.revision_id)
            .unwrap_or_default();
        engine.open_live(run, bytes, &revision);
        return Ok(());
    }
    engine.open_run(run)
}

/// §9.4 exact-recency tier: completed calls from the live engine's recent
/// rings. Only this process's own sessions have rings (RAM state); every
/// other key gets an honest empty frame rather than an error.
fn recent_calls_frame(run: &str, request_id: u64) -> Vec<u8> {
    let rows = same_process_engine_id(run)
        .and_then(|engine_id| bex_events::prof::recent_calls(engine_id, Duration::from_millis(200)))
        .unwrap_or_default();
    let thread: Vec<u64> = rows
        .iter()
        .map(|r| u64::from(r.partition) << 32 | u64::from(r.thread_idx))
        .collect();
    let call_id: Vec<u64> = rows.iter().map(|r| r.call_id).collect();
    let parent: Vec<u64> = rows.iter().map(|r| r.parent_call_id).collect();
    let function: Vec<u32> = rows.iter().map(|r| r.function).collect();
    let start: Vec<u64> = rows.iter().map(|r| r.start_ns).collect();
    let end: Vec<u64> = rows.iter().map(|r| r.end_ns).collect();
    let status: Vec<u32> = rows.iter().map(|r| u32::from(r.status)).collect();
    bqf1::encode_frame(
        bqf1::FrameKind::RecentCalls,
        0,
        request_id,
        0,
        &[
            bqf1::Col::U64(&thread),
            bqf1::Col::U64(&call_id),
            bqf1::Col::U64(&parent),
            bqf1::Col::U32(&function),
            bqf1::Col::U64(&start),
            bqf1::Col::U64(&end),
            bqf1::Col::U32(&status),
        ],
    )
}

/// Parse `<started_secs>-<euid_hex32>-e<engine_id>` and return the engine
/// id only when the euid segment is this process's.
fn same_process_engine_id(run: &str) -> Option<u64> {
    let mut parts = run.splitn(3, '-');
    let _started = parts.next()?;
    let euid = parts.next()?;
    let engine = parts.next()?.strip_prefix('e')?;
    if euid.len() != 32 || euid != bex_events::prof::process_euid_hex() {
        return None;
    }
    engine.parse().ok()
}

fn evaluate_sub(engine: &mut ObserveEngine, sub: &mut SubState, now_ns: u64) -> SubEval {
    if sub.spec.method == ObsMethod::Runs {
        let frame = engine.runs_frame(sub.spec.id, now_ns);
        if sub.last_frame.as_deref() == Some(frame.as_slice()) {
            return SubEval::Unchanged;
        }
        sub.last_frame = Some(frame.clone());
        return SubEval::Send(frame);
    }
    let run = sub.spec.run.clone().unwrap_or_default();
    if sub.spec.method == ObsMethod::RecentCalls {
        let frame = recent_calls_frame(&run, sub.spec.id);
        if sub.last_frame.as_deref() == Some(frame.as_slice()) {
            return SubEval::Unchanged;
        }
        sub.last_frame = Some(frame.clone());
        return SubEval::Send(frame);
    }
    // Re-open every tick — live-mirror for this process's own sessions,
    // else a disk re-read (KB-scale). Either way the tail stays fresh.
    if let Err(err) = open_run_preferring_live(engine, &run) {
        return SubEval::Failed(bqf1::status_frame(sub.spec.id, 404, &err));
    }
    let epoch = engine.run_epoch(&run);
    if epoch.is_some() && epoch == sub.last_epoch {
        return SubEval::Unchanged;
    }
    sub.last_epoch = epoch;
    let id = sub.spec.id;
    let frame = match sub.spec.method {
        ObsMethod::Timeline => engine.timeline_frame(&run, id),
        ObsMethod::LeftHeavy => engine.left_heavy_frame(&run, sub.spec.pixel_width, id),
        ObsMethod::TopFunctions => engine.top_functions_frame(&run, sub.spec.limit, id),
        ObsMethod::Runs | ObsMethod::RunMeta | ObsMethod::RecentCalls | ObsMethod::Bql => {
            unreachable!("handled above / not subscribable")
        }
    };
    SubEval::Send(frame)
}

// ---------------------------------------------------------------------------
// WebSocket session
// ---------------------------------------------------------------------------

fn now_epoch_ns() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| u64::try_from(d.as_nanos()).unwrap_or(u64::MAX))
        .unwrap_or(0)
}

/// One `/api/obs` connection: dispatch client messages and drive the
/// subscription tick. All sends go through this task's socket serially, so
/// per-subscription there is never more than one frame in flight.
pub(crate) async fn obs_ws_session(mut socket: WebSocket, baml_root: PathBuf) {
    tracing::info!(
        "Playground: /api/obs session started (root {})",
        baml_root.display()
    );
    let mut engine = ObserveEngine::new(baml_root);
    let mut subs: HashMap<u64, SubState> = HashMap::new();
    let mut tick = tokio::time::interval(SUB_TICK);
    tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

    loop {
        tokio::select! {
            msg = socket.recv() => {
                match msg {
                    Some(Ok(AxumWsMsg::Text(text))) => {
                        let text_str: &str = &text;
                        match handle_text(&mut engine, text_str, now_epoch_ns()) {
                            Reply::Frame(frame) => {
                                if socket.send(AxumWsMsg::Binary(frame.into())).await.is_err() {
                                    break;
                                }
                            }
                            Reply::Sub(spec) => {
                                let id = spec.id;
                                let mut sub = SubState::new(spec);
                                match evaluate_sub(&mut engine, &mut sub, now_epoch_ns()) {
                                    SubEval::Send(frame) => {
                                        if socket.send(AxumWsMsg::Binary(frame.into())).await.is_err() {
                                            break;
                                        }
                                        subs.insert(id, sub);
                                    }
                                    SubEval::Unchanged => {
                                        subs.insert(id, sub);
                                    }
                                    // Not registered: a run that cannot open
                                    // answers once instead of erroring 4x/s.
                                    SubEval::Failed(frame) => {
                                        if socket.send(AxumWsMsg::Binary(frame.into())).await.is_err() {
                                            break;
                                        }
                                    }
                                }
                            }
                            Reply::Unsub(id) => {
                                subs.remove(&id);
                            }
                        }
                    }
                    Some(Ok(AxumWsMsg::Close(_))) | None => break,
                    Some(Ok(_)) => {} // Binary/Ping/Pong: ignored.
                    Some(Err(err)) => {
                        tracing::warn!("Playground: /api/obs socket error: {err}");
                        break;
                    }
                }
            }
            _ = tick.tick() => {
                if subs.is_empty() {
                    continue;
                }
                let now = now_epoch_ns();
                let mut failed: Vec<u64> = Vec::new();
                let mut closed = false;
                for (id, sub) in &mut subs {
                    match evaluate_sub(&mut engine, sub, now) {
                        SubEval::Unchanged => {}
                        SubEval::Send(frame) => {
                            if socket.send(AxumWsMsg::Binary(frame.into())).await.is_err() {
                                closed = true;
                                break;
                            }
                        }
                        SubEval::Failed(frame) => {
                            failed.push(*id);
                            if socket.send(AxumWsMsg::Binary(frame.into())).await.is_err() {
                                closed = true;
                                break;
                            }
                        }
                    }
                }
                if closed {
                    break;
                }
                for id in failed {
                    subs.remove(&id);
                }
            }
        }
    }
    tracing::info!("Playground: /api/obs session closed");
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use bex_events::ids::{BexCallId, BexThreadId, FunctionId};
    use bex_events::prof::cct::CctEngine;
    use bex_events::prof::cct::session::{FsyncService, SessionWriter};
    use bex_events::prof::record::{FunctionEndStatus, MAX_RECORD_LEN, RawRecord, ThreadEndStatus};
    use bex_query::bqf1::{FrameKind, decode_frame};

    fn encode(records: &[RawRecord<'_>]) -> Vec<u8> {
        let mut out = Vec::new();
        let mut buf = [0u8; MAX_RECORD_LEN];
        for rec in records {
            let len = rec.encode(&mut buf);
            out.extend_from_slice(&buf[..len]);
        }
        out
    }

    /// main(fn16) -> leaf(fn17) on thread 1, fixed ticks.
    fn program(base_ts: u64) -> Vec<u8> {
        let t = |d: u64| base_ts + d;
        encode(&[
            RawRecord::StartThread {
                flags: 0,
                thread_id: BexThreadId(1),
                parent_thread_id: BexThreadId(0),
                parent_call_id: BexCallId(0),
                ts_ticks: t(0),
                name: b"",
            },
            RawRecord::CallFunction {
                flags: 0,
                thread_id: BexThreadId(1),
                call_id: BexCallId(1),
                parent_call_id: BexCallId(0),
                function_id: FunctionId(16),
                call_site: None,
                ts_ticks: t(1_000),
            },
            RawRecord::CallFunction {
                flags: 0,
                thread_id: BexThreadId(1),
                call_id: BexCallId(2),
                parent_call_id: BexCallId(1),
                function_id: FunctionId(17),
                call_site: None,
                ts_ticks: t(2_000),
            },
            RawRecord::EndFunction {
                status: FunctionEndStatus::Ok,
                thread_id: BexThreadId(1),
                call_id: BexCallId(2),
                ts_ticks: t(3_000),
            },
            RawRecord::EndFunction {
                status: FunctionEndStatus::Ok,
                thread_id: BexThreadId(1),
                call_id: BexCallId(1),
                ts_ticks: t(6_000),
            },
            RawRecord::EndThread {
                status: ThreadEndStatus::Completed,
                thread_id: BexThreadId(1),
                ts_ticks: t(7_000),
            },
        ])
    }

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("obs-ws-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// Write one session into `baml_dir` via the real bex_events writer
    /// (same pattern as bex_query's fold tests). `close_as_epoch` leaves an
    /// EPOCH_CLOSE marker so a later call re-mints into the same dir.
    fn write_session(baml_dir: &std::path::Path, base_ts: u64, close_as_epoch: bool) {
        let fsync = FsyncService::start();
        let mut writer = SessionWriter::create(
            baml_dir,
            [7; 16],
            1,
            1_700_000_000_000_000_000,
            (3, 1, 1, 1),
            [9; 32],
            "baml_rev_1_obs_ws_test",
            &fsync,
        )
        .unwrap();
        let mut engine = CctEngine::new(32);
        engine.consume(&program(base_ts), &mut |t| t);
        let flush = engine.flush_window();
        writer
            .write_window(&flush, engine.nodes(), base_ts, base_ts + 7_000, 8)
            .unwrap();
        if close_as_epoch {
            writer.close_epoch(engine.nodes(), base_ts + 7_000).unwrap();
        } else {
            writer.close(base_ts + 7_000, "test").unwrap();
        }
    }

    fn session_key(baml_dir: &std::path::Path) -> String {
        std::fs::read_dir(baml_dir.join("sessions"))
            .unwrap()
            .next()
            .unwrap()
            .unwrap()
            .path()
            .file_name()
            .unwrap()
            .to_string_lossy()
            .into_owned()
    }

    fn frame(reply: Reply) -> Vec<u8> {
        match reply {
            Reply::Frame(frame) => frame,
            other => panic!("expected Reply::Frame, got {other:?}"),
        }
    }

    #[test]
    fn malformed_json_replies_status_400() {
        let root = scratch("bad-json");
        let mut engine = ObserveEngine::new(root.clone());
        let bytes = frame(handle_text(&mut engine, "{not json", 0));
        let view = decode_frame(&bytes).unwrap();
        assert_eq!(view.kind, FrameKind::Status as u16);
        assert_eq!(view.request_id, 0);
        assert_eq!(view.col_u32(0).unwrap(), vec![400]);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn unknown_op_and_method_reply_status_400() {
        let root = scratch("bad-op");
        let mut engine = ObserveEngine::new(root.clone());
        let bytes = frame(handle_text(&mut engine, r#"{"op":"nope","id":7}"#, 0));
        let view = decode_frame(&bytes).unwrap();
        assert_eq!(view.kind, FrameKind::Status as u16);
        assert_eq!(view.request_id, 7);
        assert_eq!(view.col_u32(0).unwrap(), vec![400]);

        let bytes = frame(handle_text(
            &mut engine,
            r#"{"op":"query","id":8,"method":"wat"}"#,
            0,
        ));
        let view = decode_frame(&bytes).unwrap();
        assert_eq!(view.kind, FrameKind::Status as u16);
        assert_eq!(view.request_id, 8);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn query_runs_returns_runs_frame() {
        let root = scratch("query-runs");
        let mut engine = ObserveEngine::new(root.clone());
        let bytes = frame(handle_text(
            &mut engine,
            r#"{"op":"query","id":3,"method":"runs"}"#,
            1_700_000_000_000_000_000,
        ));
        let view = decode_frame(&bytes).unwrap();
        assert_eq!(view.kind, FrameKind::RunsList as u16);
        assert_eq!(view.request_id, 3);
        assert_eq!(view.nrows, 0);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn query_frames_over_written_session() {
        let root = scratch("query-frames");
        write_session(&root, 10_000, false);
        let key = session_key(&root);
        let mut engine = ObserveEngine::new(root.clone());

        let text = format!(r#"{{"op":"query","id":11,"method":"run_meta","run":"{key}"}}"#);
        let bytes = frame(handle_text(&mut engine, &text, 0));
        let view = decode_frame(&bytes).unwrap();
        assert_eq!(view.kind, FrameKind::RunMeta as u16);
        assert_eq!(view.request_id, 11);

        let text = format!(r#"{{"op":"query","id":12,"method":"timeline","run":"{key}"}}"#);
        let bytes = frame(handle_text(&mut engine, &text, 0));
        let view = decode_frame(&bytes).unwrap();
        assert_eq!(view.kind, FrameKind::Timeline as u16);
        assert_eq!(view.request_id, 12);
        assert_eq!(view.nrows, 1);

        // Default pixel_width applies when omitted.
        let text = format!(r#"{{"op":"query","id":13,"method":"left_heavy","run":"{key}"}}"#);
        let bytes = frame(handle_text(&mut engine, &text, 0));
        let view = decode_frame(&bytes).unwrap();
        assert_eq!(view.kind, FrameKind::LeftHeavy as u16);
        assert_eq!(view.request_id, 13);
        assert_eq!(view.nrows, 2, "main + leaf preorder rows");

        let text =
            format!(r#"{{"op":"query","id":14,"method":"top_functions","run":"{key}","limit":1}}"#);
        let bytes = frame(handle_text(&mut engine, &text, 0));
        let view = decode_frame(&bytes).unwrap();
        assert_eq!(view.kind, FrameKind::TopFunctions as u16);
        assert_eq!(view.request_id, 14);
        assert_eq!(view.nrows, 1, "limit=1 caps the table");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn query_rejects_unknown_and_invalid_run_keys() {
        let root = scratch("bad-run");
        let mut engine = ObserveEngine::new(root.clone());

        let bytes = frame(handle_text(
            &mut engine,
            r#"{"op":"query","id":21,"method":"timeline","run":"nope"}"#,
            0,
        ));
        let view = decode_frame(&bytes).unwrap();
        assert_eq!(view.kind, FrameKind::Status as u16);
        assert_eq!(view.request_id, 21);
        assert_eq!(view.col_u32(0).unwrap(), vec![404]);

        let bytes = frame(handle_text(
            &mut engine,
            r#"{"op":"query","id":22,"method":"timeline","run":"../escape"}"#,
            0,
        ));
        let view = decode_frame(&bytes).unwrap();
        assert_eq!(view.kind, FrameKind::Status as u16);
        assert_eq!(view.col_u32(0).unwrap(), vec![400]);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn sub_registers_and_unsub_unregisters() {
        let root = scratch("sub-basic");
        let mut engine = ObserveEngine::new(root.clone());

        let reply = handle_text(
            &mut engine,
            r#"{"op":"sub","id":31,"method":"timeline","run":"some-run"}"#,
            0,
        );
        match reply {
            Reply::Sub(spec) => {
                assert_eq!(
                    spec,
                    SubSpec {
                        id: 31,
                        method: ObsMethod::Timeline,
                        run: Some("some-run".to_string()),
                        pixel_width: DEFAULT_PIXEL_WIDTH,
                        limit: DEFAULT_LIMIT,
                    }
                );
            }
            other => panic!("expected Reply::Sub, got {other:?}"),
        }

        match handle_text(&mut engine, r#"{"op":"sub","id":32,"method":"runs"}"#, 0) {
            Reply::Sub(spec) => {
                assert_eq!(spec.method, ObsMethod::Runs);
                assert_eq!(spec.run, None);
            }
            other => panic!("expected Reply::Sub, got {other:?}"),
        }

        // run_meta is query-only.
        let bytes = frame(handle_text(
            &mut engine,
            r#"{"op":"sub","id":33,"method":"run_meta","run":"some-run"}"#,
            0,
        ));
        let view = decode_frame(&bytes).unwrap();
        assert_eq!(view.kind, FrameKind::Status as u16);
        assert_eq!(view.col_u32(0).unwrap(), vec![400]);

        match handle_text(&mut engine, r#"{"op":"unsub","id":31}"#, 0) {
            Reply::Unsub(31) => {}
            other => panic!("expected Reply::Unsub(31), got {other:?}"),
        }
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn sub_evaluation_sends_only_on_data_change() {
        let root = scratch("sub-eval");
        // EPOCH_CLOSE so the second write_session grows the same session.
        write_session(&root, 10_000, true);
        let key = session_key(&root);
        let mut engine = ObserveEngine::new(root.clone());

        let mut sub = SubState::new(SubSpec {
            id: 41,
            method: ObsMethod::Timeline,
            run: Some(key.clone()),
            pixel_width: DEFAULT_PIXEL_WIDTH,
            limit: DEFAULT_LIMIT,
        });
        let SubEval::Send(bytes) = evaluate_sub(&mut engine, &mut sub, 0) else {
            panic!("first evaluation must send");
        };
        let view = decode_frame(&bytes).unwrap();
        assert_eq!(view.kind, FrameKind::Timeline as u16);
        assert_eq!(view.request_id, 41);
        assert!(matches!(
            evaluate_sub(&mut engine, &mut sub, 0),
            SubEval::Unchanged
        ));

        // Growth (epoch re-mint into the same dir) bumps the data epoch.
        write_session(&root, 900_000, false);
        let SubEval::Send(bytes) = evaluate_sub(&mut engine, &mut sub, 0) else {
            panic!("growth must re-send");
        };
        assert_eq!(
            decode_frame(&bytes).unwrap().kind,
            FrameKind::Timeline as u16
        );

        // The runs list compares encoded bytes (empty history/ is stable).
        let mut runs_sub = SubState::new(SubSpec {
            id: 42,
            method: ObsMethod::Runs,
            run: None,
            pixel_width: DEFAULT_PIXEL_WIDTH,
            limit: DEFAULT_LIMIT,
        });
        assert!(matches!(
            evaluate_sub(&mut engine, &mut runs_sub, 0),
            SubEval::Send(_)
        ));
        assert!(matches!(
            evaluate_sub(&mut engine, &mut runs_sub, 0),
            SubEval::Unchanged
        ));

        // A run that cannot open fails once (the session drops the sub).
        let mut bad = SubState::new(SubSpec {
            id: 43,
            method: ObsMethod::TopFunctions,
            run: Some("missing".to_string()),
            pixel_width: DEFAULT_PIXEL_WIDTH,
            limit: DEFAULT_LIMIT,
        });
        let SubEval::Failed(bytes) = evaluate_sub(&mut engine, &mut bad, 0) else {
            panic!("missing run must fail");
        };
        let view = decode_frame(&bytes).unwrap();
        assert_eq!(view.kind, FrameKind::Status as u16);
        assert_eq!(view.request_id, 43);
        assert_eq!(view.col_u32(0).unwrap(), vec![404]);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn bql_query_round_trips_and_errors_as_status_422() {
        let root = scratch("bql");
        write_session(&root, 10_000, false);
        let key = session_key(&root);
        let mut engine = ObserveEngine::new(root.clone());

        // A valid pipeline replies with one BqlTable frame (kind 9): meta
        // row + one data row per fold node, footer JSON in the last column.
        let text = format!(
            r#"{{"op":"query","id":91,"method":"bql","run":"{key}","q":"ctx() | top(5, by=total_ns)"}}"#
        );
        let bytes = frame(handle_text(&mut engine, &text, 0));
        let view = decode_frame(&bytes).unwrap();
        assert_eq!(view.kind, FrameKind::BqlTable as u16);
        assert_eq!(view.request_id, 91);
        assert_eq!(view.nrows, 3, "meta row + main + leaf");
        let meta = view.col_str(view.cols.len() - 1).unwrap();
        assert!(
            meta[0].contains("\"footer\""),
            "footer JSON rides row 0: {}",
            meta[0]
        );
        assert!(meta[1].is_empty() && meta[2].is_empty());

        // Typed BQL errors surface as Status 422 with code + remedy inline.
        let text =
            format!(r#"{{"op":"query","id":92,"method":"bql","run":"{key}","q":"nonsense("}}"#);
        let bytes = frame(handle_text(&mut engine, &text, 0));
        let view = decode_frame(&bytes).unwrap();
        assert_eq!(view.kind, FrameKind::Status as u16);
        assert_eq!(view.request_id, 92);
        assert_eq!(view.col_u32(0).unwrap(), vec![422]);
        let msg = view.col_str(1).unwrap();
        assert!(
            msg[0].starts_with("E_PARSE:"),
            "message carries the code: {}",
            msg[0]
        );
        assert!(msg[0].contains("remedy:"));

        // Missing q, and sub attempts, are plain 400s.
        let bytes = frame(handle_text(
            &mut engine,
            r#"{"op":"query","id":93,"method":"bql"}"#,
            0,
        ));
        let view = decode_frame(&bytes).unwrap();
        assert_eq!(view.col_u32(0).unwrap(), vec![400]);
        let bytes = frame(handle_text(
            &mut engine,
            r#"{"op":"sub","id":94,"method":"bql","q":"ctx()"}"#,
            0,
        ));
        let view = decode_frame(&bytes).unwrap();
        assert_eq!(view.kind, FrameKind::Status as u16);
        assert_eq!(view.col_u32(0).unwrap(), vec![400]);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn same_process_engine_id_requires_matching_euid() {
        // Foreign euid: never routed to the live tap.
        assert_eq!(
            super::same_process_engine_id("1700000000-00000000000000000000000000000000-e7"),
            None
        );
        // Malformed keys: no tap.
        assert_eq!(super::same_process_engine_id("not-a-session"), None);
        assert_eq!(super::same_process_engine_id(""), None);
        // This process's euid parses through to the engine id.
        let ours = format!("1700000000-{}-e42", bex_events::prof::process_euid_hex());
        assert_eq!(super::same_process_engine_id(&ours), Some(42));
    }
}
