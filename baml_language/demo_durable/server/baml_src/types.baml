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
    /// True on an entry that a fork copied from its source, or that a run
    /// attached to a child that another run dispatched. The child belongs to
    /// the run that dispatched it, so a run never cancels a child through an
    /// inherited entry.
    inherited: bool,
    /// Contract section 10.2: the call site that the worker reported with the
    /// `remote_call` event, so the cause of the call survives a resume and a
    /// page reload. Null when the worker could not attribute the call.
    file: string?,
    line: int?,
    /// Contract section 10.3: the function that made the call, as the
    /// `remote_call` event reported it. The app names the caller instead of
    /// guessing it from the run's entry function, which is a different
    /// function whenever the call is written in a helper.
    caller: string?,
}

/// One remote call that a worker of the run announced: what is needed to
/// dispatch the call again. A restored thread that waits for the result of a
/// call does not announce it again and no longer holds its arguments, so the
/// site server keeps them. A fork copies the list of its source.
class CallRecord {
    call_id: string,
    func: string,
    args: json,
    /// Contract section 10.2: the call site of the `remote_call` event.
    file: string?,
    line: int?,
    /// Contract section 10.3: the function that made the call.
    caller: string?,
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
    /// Contract section 9.3: the time at which a `sleeping` run is resumed, in
    /// Unix milliseconds on this server's clock. Null in every other status.
    wake_at: int?,
    /// Contract section 9.3: the hash of the program that the run executes, from
    /// the worker's `hello` event or from the request that created the run.
    program_hash: string?,
    /// Extension: the remote calls that the run no longer waits on (a worker
    /// `remote_cancel` event, or the end of the run as `cancelled`, `failed`, or
    /// `lost`). A result that arrives for one of them is discarded.
    cancelled_calls: string[],
    /// Extension: every remote call that a worker of the run announced.
    calls: CallRecord[],
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
        // Contract section 9.7: `calls: [{ call_id, function, args }]`.
        if let cs: json[] = out.get("calls") {
            let records: json[] = [];
            for (let c in cs) {
                if let cm: map<string, json> = c {
                    records.push(renamed(cm, from, to));
                } else {
                    records.push(c);
                }
            }
            out.set("calls", records);
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
        if (!m.has("wake_at")) {
            m.set("wake_at", null);
        }
        if (!m.has("program_hash")) {
            m.set("program_hash", null);
        }
        if (!m.has("cancelled_calls")) {
            m.set("cancelled_calls", []);
        }
        if (!m.has("calls")) {
            m.set("calls", []);
        }
        if let waits: json[] = m.get("waiting_on") {
            for (let w in waits) {
                if let wm: map<string, json> = w {
                    if (!wm.has("inherited")) {
                        wm.set("inherited", false);
                    }
                    // Contract section 10.2. A record that an older server wrote names no call site.
                    if (!wm.has("file")) {
                        wm.set("file", null);
                    }
                    if (!wm.has("line")) {
                        wm.set("line", null);
                    }
                    if (!wm.has("caller")) {
                        wm.set("caller", null);
                    }
                }
            }
        }
        if let recorded: json[] = m.get("calls") {
            for (let c in recorded) {
                if let cm: map<string, json> = c {
                    if (!cm.has("file")) {
                        cm.set("file", null);
                    }
                    if (!cm.has("line")) {
                        cm.set("line", null);
                    }
                    if (!cm.has("caller")) {
                        cm.set("caller", null);
                    }
                }
            }
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

/// A string field that may be absent or null (contract section 10.1).
function jget_str_opt(body: json, key: string) -> string? throws never {
    if let s: string = baml.json.field(body, key) {
        if (s.length() > 0) {
            s
        } else {
            null
        }
    } else {
        null
    }
}

/// A 1-based line number that may be absent or null (contract section 10.1).
function jget_line_opt(body: json, key: string) -> int? throws never {
    let n = baml.json.path_or<int>(body, `.${key}`, 0);
    if (n > 0) {
        n
    } else {
        null
    }
}

function now_ms() -> int throws never {
    baml.time.Instant.now().to_timestamp_milliseconds().to_int() catch_all (e) {
        _ => 0
    }
}
