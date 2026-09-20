// Data model of the site server (contract section 3.2).
//
// `function` is a BAML keyword, so the classes store it as `func`. The helpers
// `run_json` and `run_from_json` translate between the two spellings. Nothing
// else in the server reads or writes a run record as JSON.

class ParentRef {
    site: string,
    run: string,
    call_id: string,
}

class OriginRef {
    site: string,
    run: string,
}

class ForkRef {
    run: string,
    n: int,
}

class Position {
    thread: int,
    func: string,
    file: string,
    line: int,
}

class Waiting {
    call_id: string,
    child_site: string,
    child_run: string,
    func: string,
}

class SnapshotInfo {
    n: int,
    snapshot_path: string,
    state_path: string,
    bytes: int,
    ts: int,
    stats: json,
    /// Extension: the segment that wrote the snapshot.
    segment: int,
    /// Extension: true for a snapshot that was not requested with `pause`.
    automatic: bool,
}

/// Extension: a remote result that reached this site. It stays in `meta.json`
/// so that a later segment can receive it with `--remote-result`.
class StoredResult {
    call_id: string,
    value: json,
    error: string?,
    /// True once a worker reported `remote_result_received` for it.
    acked: bool,
    ts: int,
}

/// Extension: the latest `blocked` answer to a pause request that has not
/// succeeded yet.
class PauseBlocked {
    reason: string,
    path: string[],
    ts: int,
    /// Number of `blocked` events since the pause request.
    attempts: int,
}

class Run {
    id: string,
    site: string,
    func: string,
    args: json,
    durable: bool,
    status: string,
    parent: ParentRef?,
    origin: OriginRef?,
    segment: int,
    pid: int?,
    position: Position?,
    waiting_on: Waiting[],
    snapshots: SnapshotInfo[],
    result: json,
    error: string?,
    created_ts: int,
    updated_ts: int,
    // Extensions. Readers that follow the contract ignore them.
    remote_results: StoredResult[],
    forked_from: ForkRef?,
    /// For a child run: the result reached the parent's site.
    result_delivered: bool,
    /// Set while the status is `pausing` and the worker answered with `blocked`.
    blocked: PauseBlocked?,
    /// Contract section 8.3: on a record with status `migrated`, the site that
    /// the run moved to. Null on every other record.
    migrated_to: string?,
}

/// Returns a copy of `m` in which the key `from` is called `to`. The key order is kept.
function renamed(m: map<string, json>, from: string, to: string) -> map<string, json> throws never {
    let out: map<string, json> = {};
    for (let k in m.keys()) {
        let key = if (k == from) {
            to
        } else {
            k
        };
        out.set(key, m.get(k));
    }
    out
}

/// Renames `from` to `to` in a run record, its position, and its waiting_on entries.
function rename_function_keys(value: json, from: string, to: string) -> json throws never {
    if let m: map<string, json> = value {
        let out = renamed(m, from, to);
        if let p: map<string, json> = out.get("position") {
            out.set("position", renamed(p, from, to));
        }
        if let ws: json[] = out.get("waiting_on") {
            let entries: json[] = [];
            for (let w in ws) {
                if let wm: map<string, json> = w {
                    entries.push(renamed(wm, from, to));
                } else {
                    entries.push(w);
                }
            }
            out.set("waiting_on", entries);
        }
        out
    } else {
        value
    }
}

/// The contract JSON of a run record.
function run_json(run: Run) -> json throws never {
    rename_function_keys(j(run), "func", "function")
}

function run_from_json(value: json) -> Run throws baml.json.DecodeError {
    let copy = rename_function_keys(value, "function", "func");
    if let m: map<string, json> = copy {
        // Fields that another site or an older meta.json may not have.
        if (!m.has("remote_results")) {
            m.set("remote_results", []);
        }
        if (!m.has("forked_from")) {
            m.set("forked_from", null);
        }
        if (!m.has("result_delivered")) {
            m.set("result_delivered", false);
        }
        if (!m.has("blocked")) {
            m.set("blocked", null);
        }
        if (!m.has("migrated_to")) {
            m.set("migrated_to", null);
        }
        if let snaps: json[] = m.get("snapshots") {
            for (let s in snaps) {
                if let sm: map<string, json> = s {
                    if (!sm.has("segment")) {
                        sm.set("segment", 1);
                    }
                    if (!sm.has("automatic")) {
                        sm.set("automatic", false);
                    }
                }
            }
        }
    }
    baml.json.from_json<Run>(copy)
}

function runs_json(runs: Run[]) -> json throws never {
    let out: json[] = [];
    for (let r in runs) {
        out.push(run_json(r));
    }
    out
}

// ───────────────────────────── json helpers (never throw) ─────────────────────────────

/// Any BAML value as json (null if it has no JSON form).
function j(value: unknown) -> json throws never {
    baml.json.to_json(value) catch_all (e) {
        _ => null
    }
}

/// JSON text as json (null if malformed).
function jparse(s: string) -> json throws never {
    baml.json.parse(s) catch_all (e) {
        _ => null
    }
}

function jget_str(body: json, key: string) -> string throws never {
    baml.json.path_or<string>(body, `.${key}`, "")
}

function jget_int(body: json, key: string, default: int) -> int throws never {
    baml.json.path_or<int>(body, `.${key}`, default)
}

function jget(body: json, key: string) -> json throws never {
    baml.json.field(body, key)
}

function now_ms() -> int throws never {
    baml.time.Instant.now().to_timestamp_milliseconds().to_int() catch_all (e) {
        _ => 0
    }
}
