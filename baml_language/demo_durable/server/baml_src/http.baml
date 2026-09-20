// HTTP API of the site server (contract section 3.3).

class ErrorBody {
    error: string,
}

class ParamInfo {
    name: string,
    type: string,
}

class FunctionInfo {
    name: string,
    params: ParamInfo[],
    durable: bool,
    remote: bool,
}

function cors() -> map<string, string> throws never {
    {
        "content-type": "application/json",
        "access-control-allow-origin": "*",
        "access-control-allow-methods": "GET, POST, OPTIONS",
        "access-control-allow-headers": "content-type",
    }
}

function respond(status: int, body: json) -> baml.http.Response throws never {
    baml.http.Response.new(status, cors(), baml.json.stringify(body).to_utf8())
}

function respond_text(status: int, body: string) -> baml.http.Response throws never {
    baml.http.Response.new(status, cors(), body.to_utf8())
}

function fail(status: int, message: string) -> baml.http.Response throws never {
    respond(status, { "error": message })
}

function respond_run(run: Run) -> baml.http.Response throws never {
    respond(200, run_json(run))
}

// ───────────────────────────── routing ─────────────────────────────

function handle(app: App, req: baml.http.Request) -> baml.http.Response {
    let method = req.method.to_upper_case();
    if (method == "OPTIONS") {
        return baml.http.Response.new(204, cors(), "".to_utf8());
    }
    let q = req.url.index_of("?");
    let path = req.url;
    let query = "";
    if (q != null) {
        path = req.url.slice(0, q);
        query = req.url.slice(q + 1, req.url.length());
    }
    let parts = path.split("/").filter((p) -> {
        p.length() > 0
    });
    let n = parts.length();
    if (n < 2 || parts[0] != "api") {
        return fail(404, `no route ${method} ${path}`);
    }
    let body = jparse(req.body);

    if (method == "GET") {
        if (n == 2 && parts[1] == "info") {
            return respond(200, info_json(app));
        }
        if (n == 2 && parts[1] == "events") {
            return subscribe(app);
        }
        if (n == 2 && parts[1] == "source") {
            return get_source(app, query);
        }
        if (n == 2 && parts[1] == "runs") {
            return respond(200, runs_json(app.runs.reverse()));
        }
        if (n >= 3 && parts[1] == "runs") {
            let found = find_run(app, parts[2]);
            if (found == null) {
                return fail(404, `no run ${parts[2]} on site ${app.cfg.site}`);
            }
            let run: Run = found;
            if (n == 3) {
                return respond_run(run);
            }
            if (n == 4 && parts[3] == "events") {
                return get_events(app, run);
            }
            if (n == 6 && parts[3] == "snapshots" && parts[5] == "state") {
                return get_state(run, parts[4]);
            }
        }
        return fail(404, `no route ${method} ${path}`);
    }

    if (method == "POST") {
        if (n == 2 && parts[1] == "runs") {
            return start_run(app, body, null);
        }
        if (n == 3 && parts[1] == "remote" && parts[2] == "runs") {
            return start_remote_run(app, body);
        }
        if (n == 3 && parts[1] == "runs" && parts[2] == "import") {
            return import_run(app, body);
        }
        if (n == 4 && parts[1] == "runs") {
            let found = find_run(app, parts[2]);
            if (found == null) {
                return fail(404, `no run ${parts[2]} on site ${app.cfg.site}`);
            }
            let run: Run = found;
            match (parts[3]) {
                "pause" => {
                    return pause_run(app, run);
                },
                "resume" => {
                    return resume_run(app, run, body);
                },
                "kill" => {
                    return kill_run(app, run);
                },
                "cancel" => {
                    return cancel_run(app, run);
                },
                "fork" => {
                    return fork_run(app, run, body);
                },
                "remote_result" => {
                    return remote_result(app, run, body, req.body);
                },
                _ => {},
            }
        }
        return fail(404, `no route ${method} ${path}`);
    }
    fail(405, `method ${method} is not supported`)
}

// ───────────────────────────── info and source ─────────────────────────────

function info_json(app: App) -> json throws never {
    {
        "site": app.cfg.site,
        "program_dir": app.cfg.program_dir,
        "functions": j(app.functions),
        // Contract section 7.6.
        "worker_cmd": j(app.cfg.worker_cmd),
        "runs_dir": app.cfg.runs_dir,
        // Contract section 8.3: the registry in its order, and the remote pool.
        "sites": j(app.cfg.sites),
        "remote_pool": j(app.cfg.remote_pool),
    }
}

/// Lists the top-level functions of the program. The server has no reflection
/// over another project, so it scans the source text for `function` headers.
function scan_functions(program_dir: string) -> FunctionInfo[] throws never {
    let out: FunctionInfo[] = [];
    let dirs = [`${program_dir}/baml_src`, program_dir];
    for (let dir in dirs) {
        let entries = baml.fs.read_dir(dir) catch_all (e) {
            _ => {
                let none: baml.fs.DirEntry[] = [];
                none
            }
        };
        for (let entry in entries) {
            if (!entry.is_file || !entry.name.ends_with(".baml")) {
                continue;
            }
            let text = baml.fs.read(`${dir}/${entry.name}`) catch_all (e) {
                _ => ""
            };
            for (let line in text.lines()) {
                if (!line.starts_with("function ")) {
                    continue;
                }
                let f = parse_function_header(line);
                if (f != null) {
                    out.push(f);
                }
            }
        }
        if (out.length() > 0) {
            break;
        }
    }
    out
}

function parse_function_header(line: string) -> FunctionInfo? throws never {
    let open = line.index_of("(");
    let close = line.last_index_of(")");
    if (open == null || close == null) {
        return null;
    }
    let name = line.slice(9, open).trim();
    if (name.length() == 0 || name.includes("<")) {
        return null;
    }
    let params: ParamInfo[] = [];
    let depth = 0;
    let current = "";
    let pieces: string[] = [];
    for (let ch in line.slice(open + 1, close).chars()) {
        if (ch == "<" || ch == "(" || ch == "[") {
            depth += 1;
        }
        if (ch == ">" || ch == ")" || ch == "]") {
            depth -= 1;
        }
        if (ch == "," && depth == 0) {
            pieces.push(current);
            current = "";
        } else {
            current = current + ch;
        }
    }
    pieces.push(current);
    for (let piece in pieces) {
        let colon = piece.index_of(":");
        if (colon == null) {
            continue;
        }
        params.push(
            ParamInfo {
                name: piece.slice(0, colon).trim(),
                type: piece.slice(colon + 1, piece.length()).trim(),
            },
        );
    }
    FunctionInfo {
        name: name,
        params: params,
        durable: name.includes("durable"),
        remote: name.starts_with("remote_"),
    }
}

function percent_decode(s: string) -> string throws never {
    if (!s.includes("%") && !s.includes("+")) {
        return s;
    }
    let bytes: int[] = [];
    let src = s.to_utf8().to_array();
    let i = 0;
    while (i < src.length()) {
        let b = src[i];
        if (b == 37 && i + 2 < src.length()) {
            let hex = baml.String.from_utf8(baml.Uint8Array.from_array([src[i + 1], src[i + 2]])) catch_all (e) {
                _ => ""
            };
            let decoded = baml.Uint8Array.from_hex(hex) catch_all (e) {
                _ => null
            };
            if (decoded != null) {
                if (decoded.length() == 1) {
                    bytes.push(decoded.at(0) ?? 0);
                    i += 3;
                    continue;
                }
            }
        }
        if (b == 43) {
            bytes.push(32);
        } else {
            bytes.push(b);
        }
        i += 1;
    }
    let arr = baml.Uint8Array.from_array(bytes) catch_all (e) {
        _ => null
    };
    if (arr == null) {
        return s;
    }
    arr.to_string()
}

function query_param(query: string, key: string) -> string? throws never {
    for (let pair in query.split("&")) {
        let eq = pair.index_of("=");
        if (eq == null) {
            continue;
        }
        if (percent_decode(pair.slice(0, eq)) == key) {
            return percent_decode(pair.slice(eq + 1, pair.length()));
        }
    }
    null
}

/// `GET /api/source?file=`. The `file` value of a `position` event is accepted
/// as it is: a path relative to PROGRAM_DIR, or, when the worker could not
/// make it relative, an absolute path inside PROGRAM_DIR. Every other path is
/// rejected.
function get_source(app: App, query: string) -> baml.http.Response throws never {
    let file = query_param(query, "file");
    let f: string = file ?? "";
    if (f.length() == 0) {
        return fail(400, "the query parameter `file` is required");
    }
    let escapes = f.includes("\\")
        || f.split("/").some((seg) -> {
            seg == ".."
        });
    if (escapes) {
        return fail(400, `the path ${f} escapes the program directory`);
    }
    let full = absolute_path(app.cfg.program_dir, f);
    let inside = full.starts_with(app.cfg.program_dir + "/") || full.starts_with(app.cfg.program_real_dir + "/");
    if (!inside) {
        return fail(400, `the path ${f} escapes the program directory`);
    }
    let text = baml.fs.read(full) catch_all (e) {
        _ => null
    };
    if (text == null) {
        return fail(404, `no source file ${f} in ${app.cfg.program_dir}`);
    }
    respond(200, { "file": f, "text": text })
}

// ───────────────────────────── run reads ─────────────────────────────

function get_events(app: App, run: Run) -> baml.http.Response throws never {
    let text = baml.fs.read(`${run_dir(app, run.id)}/events.jsonl`) catch_all (e) {
        _ => ""
    };
    let lines = text.lines().filter((l) -> {
        l.starts_with("{") && l.ends_with("}")
    });
    respond_text(200, "[" + lines.join(",") + "]")
}

function get_state(run: Run, n_text: string) -> baml.http.Response throws never {
    let n = baml.Int.parse(n_text) catch_all (e) {
        _ => -1
    };
    let snap = run.snapshots.find((s) -> {
        s.n == n
    });
    if (snap == null) {
        return fail(404, `run ${run.id} has no snapshot ${n_text}`);
    }
    let text = baml.fs.read(snap.state_path) catch_all (e) {
        _ => null
    };
    if (text == null) {
        return fail(404, `the state dump ${snap.state_path} is missing`);
    }
    let t: string = text;
    if (jparse(t) == null) {
        return fail(500, `the state dump ${snap.state_path} is not JSON`);
    }
    respond_text(200, t)
}

// ───────────────────────────── run commands ─────────────────────────────

function start_run(app: App, body: json, parent: ParentRef?) -> baml.http.Response throws never {
    let func = jget_str(body, "function").trim();
    if (func.length() == 0) {
        return fail(400, "`function` is required");
    }
    let known = app.functions.length() == 0
        || app.functions.some((f) -> {
            f.name == func
        });
    if (!known) {
        return fail(400, `no function ${func} in ${app.cfg.program_dir}`);
    }
    let args = jget(body, "args");
    if (args == null) {
        let empty: map<string, json> = {};
        args = empty;
    }
    if (!(args is map<string, json>)) {
        return fail(400, "`args` must be a JSON object keyed by parameter name");
    }
    let now = now_ms();
    let run = Run {
        id: new_run_id(app),
        site: app.cfg.site,
        func: func,
        args: args,
        durable: func.includes("durable"),
        status: "starting",
        parent: parent,
        origin: null,
        segment: 1,
        pid: null,
        position: null,
        waiting_on: [],
        snapshots: [],
        result: null,
        error: null,
        created_ts: now,
        updated_ts: now,
        remote_results: [],
        forked_from: null,
        result_delivered: false,
        blocked: null,
        migrated_to: null,
    };
    ensure_dir(run_dir(app, run.id));
    app.runs.push(run);
    //# Start segment 1 in a new worker process
    launch(app, run, null);
    respond_run(run)
}

function start_remote_run(app: App, body: json) -> baml.http.Response throws never {
    let parent = baml.json.from_json<ParentRef>(jget(body, "parent")) catch_all (e) {
        _ => null
    };
    if (parent == null) {
        return fail(400, "`parent` must be {site, run, call_id}");
    }
    // The result is posted to the URL of `parent.site` in SITES. Without an
    // entry it could never be delivered, and the parent would wait forever.
    if (site_url(app.cfg, parent.site) == null) {
        return fail(400, `unknown parent site ${parent.site}`);
    }
    start_run(app, body, parent)
}

function pause_run(app: App, run: Run) -> baml.http.Response throws never {
    if (!run.durable) {
        return fail(409, `run ${run.id} is not durable (${run.func}), so it cannot be paused`);
    }
    if (!(run.status == "running" || run.status == "starting") || !app.procs.has(run.id)) {
        return fail(409, `run ${run.id} is ${run.status}; only a running run can be paused`);
    }
    run.status = "pausing";
    run.blocked = null;
    touch(app, run);
    if (!send_command(app, run, { "type": "pause" })) {
        if (run.status == "pausing") {
            run.status = "running";
            touch(app, run);
        }
        return fail(500, `could not send pause to the worker of ${run.id}`);
    }
    respond_run(run)
}

function cancel_run(app: App, run: Run) -> baml.http.Response throws never {
    if (!app.procs.has(run.id)) {
        return fail(409, `run ${run.id} is ${run.status} and has no worker process`);
    }
    if (!send_command(app, run, { "type": "cancel" })) {
        return fail(500, `could not send cancel to the worker of ${run.id}`);
    }
    respond_run(run)
}

function kill_run(app: App, run: Run) -> baml.http.Response throws never {
    let proc = app.procs.get(run.id);
    if (proc == null) {
        return fail(409, `run ${run.id} is ${run.status} and has no worker process`);
    }
    //# End the worker process without a snapshot
    let killed = kill_process(proc) catch_all (e, ctx) {
        _ => ctx.to_string()
    };
    if (killed.length() > 0) {
        return fail(500, `could not end the worker of ${run.id}: ${killed}`);
    }
    // The supervisor thread maps the exit to `paused` or `lost`. Wait for it
    // so that the response carries the new status.
    let waited = 0;
    while (app.procs.has(run.id) && waited < 3000) {
        baml.sys.sleep(baml.time.Duration.from_milliseconds(20n)) catch_all (e) {
            _ => null
        };
        waited += 20;
    }
    respond_run(run)
}

function kill_process(proc: baml.sys.Process) -> string throws baml.errors.Io {
    proc.kill();
    ""
}

function resume_run(app: App, run: Run, body: json) -> baml.http.Response throws never {
    // The site name is checked before the state of the run, so a bad name is
    // always a 400 (contract section 8.3). A missing `site`, JSON null, and the
    // empty string mean this site.
    let site = "";
    let site_value = jget(body, "site");
    if (site_value != null) {
        if let name: string = site_value {
            site = name;
        } else {
            return fail(400, "`site` must be a string: the name of a site in SITES");
        }
    }
    let here = site.length() == 0 || site == app.cfg.site;
    let target_url: string? = null;
    if (!here) {
        target_url = site_url(app.cfg, site);
        if (target_url == null) {
            return fail(400, `unknown site ${site}`);
        }
    }
    if (run.status != "paused") {
        return fail(409, `run ${run.id} is ${run.status}; only a paused run can be resumed`);
    }
    let snap = latest_snapshot(run);
    if (snap == null) {
        return fail(409, `run ${run.id} has no snapshot`);
    }
    let s: SnapshotInfo = snap;
    if (here) {
        //# Resume here: next segment, new process, latest snapshot
        // `launch` sets the status before its first sys-op, so a second resume
        // request finds a run that is not paused.
        run.segment += 1;
        launch(app, run, s.snapshot_path);
        return respond_run(run);
    }
    //# Resume on another site: any name of the registry (contract section 8.3)
    // A remote call whose dispatch is still in flight is recorded on this
    // record when the other site answers. The run must not leave before that,
    // or the new site would dispatch the call a second time.
    if (app.dispatching.values().includes(run.id)) {
        return fail(409, `run ${run.id} has a remote call whose dispatch is in flight; try again in a moment`);
    }
    migrate_out(app, run, s, site, target_url ?? "")
}

/// Sends the latest snapshot to the site `to_site` and marks the run
/// `migrated`. The record that stays behind names the new site in `migrated_to`.
function migrate_out(
    app: App,
    run: Run,
    snap: SnapshotInfo,
    to_site: string,
    to_url: string,
) -> baml.http.Response throws never {
    // Guard against a second resume while the request is in flight. Both
    // fields are set before the first sys-op of this function, so no other
    // request sees the run as `paused` from here on. `migrated_to` is set
    // together with the status, so a result that arrives during the transfer is
    // forwarded to the destination. The destination answers 404 until the
    // import is done, this site answers 502, and the child's site retries.
    run.status = "migrated";
    run.migrated_to = to_site;
    let bytes = read_bytes(snap.snapshot_path) catch_all (e) {
        _ => null
    };
    if (bytes == null) {
        run.status = "paused";
        run.migrated_to = null;
        touch(app, run);
        return fail(500, `cannot read ${snap.snapshot_path}`);
    }
    let state_text = baml.fs.read(snap.state_path) catch_all (e) {
        _ => "null"
    };
    let request: map<string, json> = { "run": run_json(run), "snapshot_base64": bytes.to_base64(), "state": jparse(state_text) };
    let failure: string? = null;
    let res = post_json(`${to_url}/api/runs/import`, request) catch_all (e, ctx) {
        _ => {
            failure = `could not reach site ${to_site} at ${to_url}: ${ctx.to_string()}`;
            null
        }
    };
    if (res != null) {
        if (!res.ok()) {
            let text = res.text() catch_all (e) {
                _ => ""
            };
            failure = `site ${to_site} refused the import (${res.status_code}): ${text}`;
        }
    }
    if (failure != null) {
        run.status = "paused";
        run.migrated_to = null;
        touch(app, run);
        return fail(502, failure ?? "");
    }
    emit_run_event(app, run.id, { "type": "migrated_out", "run": run.id, "to_site": to_site });
    touch(app, run);
    respond_run(run)
}

function read_bytes(
    path: string,
) -> uint8array throws baml.errors.Io | baml.errors.InvalidArgument {
    let f = baml.fs.open(path, "r");
    defer {
        f.close() catch_all (e) {
            _ => null
        };
    }
    f.bytes()
}

/// Site to site: receives a migrating run. Same id, next segment on this site.
function import_run(app: App, body: json) -> baml.http.Response throws never {
    let decoded = run_from_json(jget(body, "run")) catch_all (e) {
        _ => null
    };
    if (decoded == null) {
        return fail(400, "`run` is not a run record");
    }
    let run: Run = decoded;
    // The id becomes a directory name inside RUNS_DIR.
    if (!valid_run_id(run.id)) {
        return fail(400, "the run id must match [a-z0-9-]+");
    }
    let latest = latest_snapshot(run);
    if (latest == null) {
        return fail(400, "the run record has no snapshot");
    }
    let snap: SnapshotInfo = latest;
    let bytes = baml.Uint8Array.from_base64(jget_str(body, "snapshot_base64")) catch_all (e) {
        _ => b""
    };
    if (bytes.length() == 0) {
        return fail(400, "`snapshot_base64` is missing or is not base64");
    }
    let existing = find_run(app, run.id);
    if (existing != null) {
        let old: Run = existing;
        if (old.status != "migrated") {
            return fail(409, `run ${run.id} already exists on site ${app.cfg.site} with status ${old.status}`);
        }
    }
    // The id is reserved before the first sys-op of this function, so a second
    // import of the same run cannot pass the check above while this one writes
    // its files.
    if (app.importing.has(run.id)) {
        return fail(409, `an import of run ${run.id} is in progress on site ${app.cfg.site}`);
    }
    app.importing.set(run.id, true);
    let from_site = run.site;
    let dir = run_dir(app, run.id);
    ensure_dir(dir);
    let snapshot_path = `${dir}/snap-${snap.n}.bamlsnap`;
    let state_path = `${dir}/snap-${snap.n}.json`;
    let wrote = write_snapshot_files(snapshot_path, bytes, state_path, baml.json.stringify_pretty(jget(body, "state"))) catch_all (e, ctx) {
        _ => ctx.to_string()
    };
    if (wrote.length() > 0) {
        app.importing.delete(run.id);
        return fail(500, `cannot store the snapshot: ${wrote}`);
    }
    // Only the transferred snapshot exists on this site.
    snap.snapshot_path = snapshot_path;
    snap.state_path = state_path;
    snap.bytes = bytes.length();
    run.snapshots = [snap];
    run.site = app.cfg.site;
    run.origin = OriginRef { site: from_site, run: run.id };
    run.status = "paused";
    run.pid = null;
    run.blocked = null;
    run.migrated_to = null;
    app.runs
        = app.runs.filter((r) -> {
            r.id != run.id
        });
    app.runs.push(run);
    app.importing.delete(run.id);
    emit_run_event(app, run.id, { "type": "migrated_in", "run": run.id, "from_site": from_site });
    //# Migration: continue with the next segment on this site
    run.segment += 1;
    launch(app, run, snapshot_path);
    respond_run(run)
}

function valid_run_id(id: string) -> bool throws never {
    if (id.length() == 0) {
        return false;
    }
    let allowed = "abcdefghijklmnopqrstuvwxyz0123456789-";
    for (let ch in id.chars()) {
        if (!allowed.includes(ch)) {
            return false;
        }
    }
    true
}

function write_snapshot_files(
    snapshot_path: string,
    bytes: uint8array,
    state_path: string,
    state_text: string,
) -> string throws baml.errors.Io {
    baml.fs.write_bytes(snapshot_path, bytes);
    baml.fs.write(state_path, state_text);
    ""
}

/// Copies snapshot `n` (default: the latest) into a new paused run.
function fork_run(app: App, run: Run, body: json) -> baml.http.Response throws never {
    let latest = latest_snapshot(run);
    if (latest == null) {
        return fail(409, `run ${run.id} has no snapshot to fork`);
    }
    let last: SnapshotInfo = latest;
    let n = jget_int(body, "n", last.n);
    let found = run.snapshots.find((s) -> {
        s.n == n
    });
    if (found == null) {
        return fail(404, `run ${run.id} has no snapshot ${n}`);
    }
    let source: SnapshotInfo = found;
    let id = new_run_id(app);
    let dir = run_dir(app, id);
    ensure_dir(dir);
    let snapshot_path = `${dir}/snap-${source.n}.bamlsnap`;
    let state_path = `${dir}/snap-${source.n}.json`;
    let copied = copy_snapshot(source, snapshot_path, state_path) catch_all (e, ctx) {
        _ => ctx.to_string()
    };
    if (copied.length() > 0) {
        return fail(500, `cannot copy snapshot ${n} of ${run.id}: ${copied}`);
    }
    let now = now_ms();
    // The fork of the latest snapshot waits on the same remote calls as its
    // source. Stored results are offered to the fork at its first resume.
    let waiting: Waiting[] = [];
    if (source.n == last.n) {
        for (let w in run.waiting_on) {
            waiting.push(Waiting { ...w });
        }
    }
    let results: StoredResult[] = [];
    for (let r in run.remote_results) {
        results.push(StoredResult { ...r, acked: false });
    }
    let fork = Run {
        id: id,
        site: app.cfg.site,
        func: run.func,
        args: run.args,
        durable: run.durable,
        status: "paused",
        parent: null,
        origin: null,
        segment: source.segment,
        pid: null,
        position: run.position,
        waiting_on: waiting,
        snapshots: [SnapshotInfo { ...source, snapshot_path: snapshot_path, state_path: state_path }],
        result: null,
        error: null,
        created_ts: now,
        updated_ts: now,
        remote_results: results,
        forked_from: ForkRef { run: run.id, n: source.n },
        result_delivered: false,
        blocked: null,
        migrated_to: null,
    };
    app.runs.push(fork);
    emit_run_event(app, id, { "type": "forked", "run": id, "from_run": run.id, "n": source.n });
    touch(app, fork);
    respond_run(fork)
}

function copy_snapshot(
    source: SnapshotInfo,
    snapshot_path: string,
    state_path: string,
) -> string throws baml.errors.Io | baml.errors.InvalidArgument | baml.errors.ParseError {
    baml.fs.write_bytes(snapshot_path, read_bytes(source.snapshot_path));
    baml.fs.write(state_path, baml.fs.read(source.state_path));
    ""
}

/// Site to site: the result of a remote call of `run`.
function remote_result(
    app: App,
    run: Run,
    body: json,
    raw_body: string,
) -> baml.http.Response throws never {
    let call_id = jget_str(body, "call_id");
    if (call_id.length() == 0) {
        return fail(400, "`call_id` is required");
    }
    if (run.status == "migrated") {
        // Forks of the run that stayed on this site still wait for the result.
        accept_remote_result(app, run, call_id, jget(body, "value"), run_error(body));
        //# The run lives on another site now: pass the result on along migrated_to
        // The next site does the same when the run moved on from there, so the
        // chain is followed until a site holds a record that is not `migrated`.
        // Every site adds its name to `via`. A chain never visits a site twice,
        // because a site holds one record per run. A result that comes back is
        // in a loop (two `migrated` records that name each other, for example
        // while a migration is in flight), and it is refused with 502 so that
        // the sender retries later.
        let via = via_of(body);
        if (via.includes(app.cfg.site) || via.length() >= app.cfg.sites.length()) {
            return fail(
                502,
                `the result for run ${run.id} already passed ${via.join(" -> ")} and reached site ${app.cfg.site} again; it is not forwarded in a loop`,
            );
        }
        let forwarded = forward_body(app, call_id, jget(body, "value"), run_error(body), via);
        let to_site = forward_once(app, run, forwarded, via);
        if (to_site == null) {
            return fail(
                502,
                `run ${run.id} migrated to ${run.migrated_to ?? "another site"}, and forwarding the result failed`,
            );
        }
        return respond(200, { "ok": true, "forwarded_to": to_site });
    }
    accept_remote_result(app, run, call_id, jget(body, "value"), run_error(body));
    respond(200, { "ok": true })
}
