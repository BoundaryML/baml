// Worker processes: start, the stdout and stderr readers, exit handling,
// commands on stdin, and remote call routing.

/// Starts the worker of the current segment of `run`. `resume_path` is null
/// for segment 1.
function launch(app: App, run: Run, resume_path: string?) -> void throws never {
    let cfg = app.cfg;
    let args: string[] = cfg.worker_cmd.slice(1, cfg.worker_cmd.length());
    args
        = args.concat(
            [
                "--project",
                cfg.program_dir,
                "--run",
                run.id,
                "--segment",
                run.segment.to_string(),
                "--snapshot-dir",
                run_dir(app, run.id),
            ],
        );
    if (resume_path == null) {
        args = args.concat(["--start", run.func, "--json-args", baml.json.stringify(run.args)]);
    } else {
        let p: string = resume_path;
        args = args.concat(["--resume", p]);
        // Results that arrived while the run had no process.
        for (let r in run.remote_results) {
            if (!r.acked) {
                args
                    = args.concat(
                        ["--remote-result", baml.json.stringify(result_message(r, false))],
                    );
            }
        }
    }
    run.status = "starting";
    run.pid = null;
    run.error = null;
    run.blocked = null;
    let started = baml.sys.start_process(
        cfg.worker_cmd[0],
        args,
        baml.sys.ProcessOptions {
            cwd: cfg.worker_cwd,
            // The server's environment plus BAML_CLI_ALLOW_DIRECT=1.
            env: cfg.worker_env,
            timeout_ms: null,
            stdin: null,
            stderr: baml.sys.StderrMode.Pipe,
        },
    ) catch_all (e, ctx) {
        _ => {
            run.status = "failed";
            run.error = `could not start the worker (${cfg.worker_cmd.join(" ")}): ${e}`;
            null
        }
    };
    if (started == null) {
        touch(app, run);
        finish_child(app, run);
        return;
    }
    let proc: baml.sys.Process = started;
    app.procs.set(run.id, proc);
    touch(app, run);
    let segment = run.segment;
    //# One supervisor thread per worker process
    spawn with baml.spawn.options(detach = true) { supervise(app, run, proc, segment) };
}

function supervise(app: App, run: Run, proc: baml.sys.Process, segment: int) -> void throws never {
    let out = spawn { read_stdout(app, run, proc, segment) };
    let err = spawn { read_stderr(app, run, proc, segment) };
    let exit = proc.wait() catch_all (e) {
        _ => baml.sys.ProcessExit { exit_code: -1, signal: "wait failed" }
    };
    // Both pipes reach EOF after the exit, so every event is handled before the
    // exit code is.
    await out;
    await err;
    on_exit(app, run, segment, exit);
}

function read_stdout(
    app: App,
    run: Run,
    proc: baml.sys.Process,
    segment: int,
) -> void throws never {
    read_stdout_lines(app, run, proc, segment) catch_all (e) {
        _ => {
            warn(`stdout of ${run.id} ended with an error: ${e}`);
        }
    };
}

function read_stdout_lines(app: App, run: Run, proc: baml.sys.Process, segment: int) -> void {
    for (let line in proc.stdout.lines()) {
        if (line.trim().length() == 0) {
            continue;
        }
        let ev = jparse(line);
        if let m: map<string, json> = ev {
            on_worker_event(app, run, segment, m);
        } else {
            // Not a JSON object. The contract forbids it, so report it as a log line.
            forward_log(app, run, segment, "worker_stdout", line);
        }
    }
}

function read_stderr(
    app: App,
    run: Run,
    proc: baml.sys.Process,
    segment: int,
) -> void throws never {
    let pipe = proc.stderr;
    if (pipe == null) {
        return;
    }
    let p: baml.sys.ReadPipe = pipe;
    read_stderr_lines(app, run, p, segment) catch_all (e) {
        _ => {
            warn(`stderr of ${run.id} ended with an error: ${e}`);
        }
    };
}

function read_stderr_lines(app: App, run: Run, pipe: baml.sys.ReadPipe, segment: int) -> void {
    for (let line in pipe.lines()) {
        forward_log(app, run, segment, "worker_stderr", line);
    }
}

function forward_log(
    app: App,
    run: Run,
    segment: int,
    stream: string,
    text: string,
) -> void throws never {
    let ev: map<string, json> = {
        "v": 1,
        "type": "log",
        "ts": now_ms(),
        "run": run.id,
        "segment": segment,
        "pid": run.pid,
        "stream": stream,
        "text": text,
        "thread": null,
        "site": app.cfg.site,
    };
    broadcast(app, ev);
    append_event(app, run.id, ev);
}

/// Handles one event from a worker: updates the run record, forwards the event
/// with `site` added, and appends it to `events.jsonl`.
function on_worker_event(
    app: App,
    run: Run,
    segment: int,
    ev: map<string, json>,
) -> void throws never {
    ev.set("site", app.cfg.site);
    let kind = jget_str(ev, "type");
    let changed = false;
    match (kind) {
        "hello" => {
            let pid = jget_int(ev, "pid", 0);
            if (pid > 0) {
                run.pid = pid;
            }
            if (run.status == "starting") {
                run.status = "running";
            }
            changed = true;
        },
        "position" => {
            run.position
                = Position {
                    thread: jget_int(ev, "thread", 0),
                    func: jget_str(ev, "function"),
                    file: jget_str(ev, "file"),
                    line: jget_int(ev, "line", 0),
                };
            changed = true;
        },
        "paused" | "snapshot" => {
            add_snapshot(run, segment, ev, kind == "snapshot");
            if (kind == "paused") {
                run.blocked = null;
            }
            changed = true;
        },
        "blocked" => {
            // The pause request stays open: the worker retries at its next
            // yield. The record keeps the latest reason for the app.
            if (run.status == "pausing") {
                let attempts = 1;
                let before = run.blocked;
                if (before != null) {
                    attempts = before.attempts + 1;
                }
                run.blocked
                    = PauseBlocked {
                        reason: jget_str(ev, "reason"),
                        path: baml.json.path_or<string[]>(ev, ".path", []),
                        ts: jget_int(ev, "ts", now_ms()),
                        attempts: attempts,
                    };
                changed = true;
            }
        },
        "completed" => {
            run.result = jget(ev, "value");
            changed = true;
        },
        "failed" => {
            run.error = jget_str(ev, "error");
            changed = true;
        },
        "remote_result_received" => {
            let call_id = jget_str(ev, "call_id");
            for (let r in run.remote_results) {
                if (r.call_id == call_id && !r.acked) {
                    r.acked = true;
                    changed = true;
                }
            }
        },
        _ => {},
    }
    broadcast(app, ev);
    append_event(app, run.id, ev);
    if (changed) {
        touch(app, run);
    }
    if (kind == "remote_call") {
        //# A remote_ call: place it on a site of the remote pool without blocking the reader
        spawn with baml.spawn.options(detach = true) { route_remote_call(app, run, ev) };
    }
}

function add_snapshot(
    run: Run,
    segment: int,
    ev: map<string, json>,
    automatic: bool,
) -> void throws never {
    let snapshot_path = jget_str(ev, "snapshot_path");
    let bytes = baml.fs.size(snapshot_path) catch_all (e) {
        _ => 0
    };
    // A worker names its files snap-<n> with n = highest existing + 1 in the
    // snapshot directory. The record uses that number when it keeps `n`
    // increasing, and counts on otherwise.
    let last = latest_snapshot(run);
    let floor = if (last == null) {
        0
    } else {
        last.n
    };
    let named = snapshot_number(snapshot_path);
    let n = if (named > floor) {
        named
    } else {
        floor + 1
    };
    run.snapshots.push(
        SnapshotInfo {
            n: n,
            snapshot_path: snapshot_path,
            state_path: jget_str(ev, "state_path"),
            bytes: bytes,
            ts: jget_int(ev, "ts", now_ms()),
            stats: jget(ev, "stats"),
            segment: segment,
            automatic: automatic,
        },
    );
}

/// The `<n>` of a path that ends in `snap-<n>.<extension>`, or 0.
function snapshot_number(path: string) -> int throws never {
    let slash = path.last_index_of("/");
    let name = if (slash == null) {
        path
    } else {
        path.slice(slash + 1, path.length())
    };
    if (!name.starts_with("snap-")) {
        return 0;
    }
    let dot = name.index_of(".");
    let digits = if (dot == null) {
        name.slice(5, name.length())
    } else {
        name.slice(5, dot)
    };
    baml.Int.parse(digits) catch_all (e) {
        _ => 0
    }
}

/// Maps the exit of a worker process to a run status (contract section 2.2).
function on_exit(
    app: App,
    run: Run,
    segment: int,
    exit: baml.sys.ProcessExit,
) -> void throws never {
    app.procs.delete(run.id);
    let code = exit.exit_code;
    let pid = run.pid;
    let has_snapshot = run.snapshots.length() > 0;
    let paused_now = run.snapshots.some((s) -> {
        s.segment == segment && !s.automatic
    });
    if (code == 75 && paused_now) {
        run.status = "paused";
    } else if (code == 0) {
        run.status = "completed";
    } else if (code == 1) {
        run.status = "failed";
        if (run.error == null) {
            run.error = "the worker exited with code 1";
        }
    } else if (code == 130) {
        run.status = "cancelled";
    } else if (run.durable && has_snapshot) {
        // The process was lost, but the state lives in the latest snapshot.
        run.status = "paused";
        reset_acks(run);
    } else {
        run.status = "lost";
        run.error
            = `the worker process was lost (exit code ${code}, signal ${exit.signal ?? "none"})`;
    }
    run.pid = null;
    run.blocked = null;
    emit_run_event(
        app,
        run.id,
        {
            "type": "worker_exit",
            "run": run.id,
            "segment": segment,
            "pid": pid,
            "exit_code": code,
            "signal": exit.signal,
            "status": run.status,
        },
    );
    touch(app, run);
    finish_child(app, run);
}

// ───────────────────────────── commands on stdin ─────────────────────────────

function send_command(app: App, run: Run, command: json) -> bool throws never {
    let proc = app.procs.get(run.id);
    if (proc == null) {
        return false;
    }
    let p: baml.sys.Process = proc;
    write_line(p, baml.json.stringify(command) + "\n") catch_all (e) {
        _ => {
            warn(`could not write to the worker of ${run.id}: ${e}`);
            false
        }
    }
}

function write_line(p: baml.sys.Process, line: string) -> bool throws baml.errors.Io {
    p.stdin.write(line);
    p.stdin.flush();
    true
}

/// The `remote_result` command or `--remote-result` value for a stored result.
function result_message(r: StoredResult, with_type: bool) -> json throws never {
    let out: map<string, json> = {};
    if (with_type) {
        out.set("type", "remote_result");
    }
    out.set("call_id", r.call_id);
    if (r.error == null) {
        out.set("value", r.value);
    } else {
        out.set("error", r.error);
    }
    out
}

// ───────────────────────────── remote calls ─────────────────────────────

function post_json(
    url: string,
    body: json,
) -> baml.http.Response throws baml.errors.Io | baml.errors.Timeout | baml.errors.InvalidArgument {
    baml.http.send(
        baml.http.Request {
            method: "POST",
            url: url,
            headers: { "content-type": "application/json" },
            body: baml.json.stringify(body),
        },
        timeout = baml.time.Duration.from_seconds(10n),
    )
}

/// The parent side of a remote call: picks a site of the remote pool (contract
/// section 8.2) and starts the child run there.
function route_remote_call(app: App, run: Run, ev: map<string, json>) -> void throws never {
    let call_id = jget_str(ev, "call_id");
    let func = jget_str(ev, "function");
    if (call_id.length() == 0) {
        return;
    }
    // A resumed worker may reach a call again whose result is already here.
    let stored = run.remote_results.find((r) -> {
        r.call_id == call_id
    });
    if (stored != null) {
        send_command(app, run, result_message(stored, true));
        return;
    }
    let already = run.waiting_on.some((w) -> {
        w.call_id == call_id
    });
    if (already || app.dispatching.has(call_id)) {
        return;
    }
    //# Placement: the first site of REMOTE_POOL that is not this site
    let target = pick_remote_site(app.cfg);
    if (target == null) {
        // The worker receives the error as a `remote_result`, so the user code
        // sees a failed call instead of waiting forever.
        warn(`remote call ${call_id}: REMOTE_POOL has no site other than ${app.cfg.site}`);
        accept_remote_result(app, run, call_id, null, "no remote site available");
        return;
    }
    let target_site: string = target;
    let target_url = site_url(app.cfg, target_site) ?? "";
    // `resume {site}` is refused while this entry exists, so the child is
    // always recorded on the record that travels with the run.
    app.dispatching.set(call_id, run.id);
    let request: map<string, json> = {
        "function": func,
        "args": jget(ev, "args"),
        "parent": { "site": app.cfg.site, "run": run.id, "call_id": call_id },
    };
    let failure: string? = null;
    let child: json = null;
    let res = post_json(`${target_url}/api/remote/runs`, request) catch_all (e, ctx) {
        _ => {
            failure = `could not reach site ${target_site} at ${target_url}: ${ctx.to_string()}`;
            null
        }
    };
    if (res != null) {
        let text = res.text() catch_all (e) {
            _ => ""
        };
        if (res.ok()) {
            child = jparse(text);
        } else {
            failure = `site ${target_site} refused the remote call (${res.status_code}): ${text}`;
        }
    }
    app.dispatching.delete(call_id);
    if (failure != null) {
        let message: string = failure;
        warn(message);
        accept_remote_result(app, run, call_id, null, message);
        return;
    }
    let waiting = Waiting {
        call_id: call_id,
        child_site: baml.json.path_or<string>(child, ".site", target_site),
        child_run: jget_str(child, "id"),
        func: func,
    };
    // A very fast child may have answered before this point.
    let answered = run.remote_results.some((r) -> {
        r.call_id == call_id
    });
    if (!answered) {
        run.waiting_on.push(waiting);
    }
    emit_run_event(
        app,
        run.id,
        {
            "type": "remote_dispatched",
            "run": run.id,
            "call_id": call_id,
            "child_site": waiting.child_site,
            "child_run": waiting.child_run,
            "function": func,
        },
    );
    touch(app, run);
}

/// The parent side of a remote result. The result is stored in `meta.json`
/// first. It is written to the worker's stdin if a worker is running, and
/// passed as `--remote-result` at the next resume otherwise. Forks that wait
/// on the same call receive it too. A fork that migrated to another site gets
/// the result forwarded: the result is stored on the record that stayed here,
/// and the `waiting_on` entry of that record is removed once the other site
/// accepted the result. `run` itself is skipped when it is `migrated`: the
/// caller forwards the result in that case.
function accept_remote_result(
    app: App,
    run: Run,
    call_id: string,
    value: json,
    error: string?,
) -> void throws never {
    let targets: Run[] = [];
    if (run.status != "migrated") {
        targets.push(run);
    }
    for (let other in app.runs) {
        let waits = other.id != run.id
            && other.waiting_on.some((w) -> {
                w.call_id == call_id
            });
        if (!waits) {
            continue;
        }
        if (other.status == "migrated") {
            // The fork lives on another site now. The entry stays until the
            // other site accepted the result, so a forward that fails is tried
            // again at the next delivery or at the next start of this server.
            // `forwarding` keeps a repeated delivery from forwarding twice.
            let key = `${other.id}/${call_id}`;
            if (app.forwarding.has(key)) {
                continue;
            }
            app.forwarding.set(key, true);
            let kept = other.remote_results.some((r) -> {
                r.call_id == call_id
            });
            if (!kept) {
                other.remote_results.push(
                    StoredResult {
                        call_id: call_id,
                        value: value,
                        error: error,
                        acked: false,
                        ts: now_ms(),
                    },
                );
                touch(app, other);
            }
            let moved_id = other.id;
            spawn with baml.spawn.options(detach = true) {
                forward_to_moved_fork(app, moved_id, call_id, value, error)
            };
        } else {
            targets.push(other);
        }
    }
    for (let target in targets) {
        let known = target.remote_results.some((r) -> {
            r.call_id == call_id
        });
        if (known) {
            continue;
        }
        let stored = StoredResult { call_id: call_id, value: value, error: error, acked: false, ts: now_ms() };
        target.remote_results.push(stored);
        let entry = target.waiting_on.find((w) -> {
            w.call_id == call_id
        });
        target.waiting_on
            = target.waiting_on.filter((w) -> {
                w.call_id != call_id
            });
        emit_run_event(
            app,
            target.id,
            {
                "type": "remote_returned",
                "run": target.id,
                "call_id": call_id,
                "child_site": entry?.child_site ?? "",
                "child_run": entry?.child_run ?? "",
                "ok": error == null,
            },
        );
        touch(app, target);
        send_command(app, target, result_message(stored, true));
    }
}

/// The body of `POST /api/runs/:id/remote_result`.
function result_body(
    call_id: string,
    value: json,
    error: string?,
) -> map<string, json> throws never {
    let body: map<string, json> = { "call_id": call_id };
    if (error == null) {
        body.set("value", value);
    } else {
        body.set("error", error);
    }
    body
}

/// The sites that a forwarded result already passed (the field `via` of a
/// `remote_result` body). A site adds its own name before it forwards.
function via_of(body: json) -> string[] throws never {
    baml.json.path_or<string[]>(body, ".via", [])
}

/// The body of a forwarded `remote_result` request: the result plus `via`
/// with this site added.
function forward_body(
    app: App,
    call_id: string,
    value: json,
    error: string?,
    via: string[],
) -> map<string, json> throws never {
    let body = result_body(call_id, value, error);
    body.set("via", j(via.concat([app.cfg.site])));
    body
}

/// The sites that may hold `run`, whose record here is `migrated`: the site in
/// `migrated_to`. A record that was written before `migrated_to` existed, or
/// that names a site outside the registry, is answered with every other site of
/// the registry that the result did not pass yet. A site that does not know the
/// run answers 404.
function forward_sites(app: App, run: Run, via: string[]) -> SiteEntry[] throws never {
    let to = run.migrated_to;
    if (to != null) {
        let entry = app.cfg.sites.find((s) -> {
            s.name == to
        });
        if (entry != null) {
            return [entry];
        }
    }
    app.cfg.sites.filter((s) -> {
        s.name != app.cfg.site && !via.includes(s.name)
    })
}

/// Posts a forwarded `remote_result` body once to the site that `run` migrated
/// to. A site in `via` is not asked again. Returns the name of the site that
/// accepted the result, or null. The caller of the HTTP route retries, so this
/// function does not.
function forward_once(app: App, run: Run, body: json, via: string[]) -> string? throws never {
    for (let target in forward_sites(app, run, via)) {
        if (via.includes(target.name)) {
            warn(
                `the result for run ${run.id} already passed site ${target.name}, so it is not sent there again`,
            );
            continue;
        }
        let status = post_json(`${target.url}/api/runs/${run.id}/remote_result`, body).status_code catch_all (e) {
            _ => 0
        };
        if (status >= 200 && status < 300) {
            return target.name;
        }
        warn(`site ${target.name} did not accept the result for run ${run.id} (status ${status})`);
    }
    null
}

/// Posts a remote result for `run`, which migrated away from this site, to the
/// site that holds it now. Retries while that site is unreachable, answers with
/// a 5xx status, or does not know the run yet (404 while the import is still in
/// progress). Returns true once a site accepted the result.
function forward_result(
    app: App,
    run: Run,
    call_id: string,
    value: json,
    error: string?,
) -> bool throws never {
    let body = forward_body(app, call_id, value, error, []);
    let targets = forward_sites(app, run, []);
    // With one known destination a 404 means "not imported yet". With the
    // fallback list a 404 means "ask the next site".
    let retry_404 = targets.length() == 1;
    for (let target in targets) {
        if (post_result(`${target.url}/api/runs/${run.id}/remote_result`, body, retry_404)) {
            return true;
        }
    }
    warn(`the result of ${call_id} did not reach run ${run.id} on site ${run.migrated_to ?? "?"}`);
    false
}

/// Forwards a result to the fork `run_id`, which migrated away from this site,
/// and removes the `waiting_on` entry of the record that stayed here once the
/// other site accepted the result. The entry stays after a failure.
function forward_to_moved_fork(
    app: App,
    run_id: string,
    call_id: string,
    value: json,
    error: string?,
) -> void throws never {
    let key = `${run_id}/${call_id}`;
    let found = find_run(app, run_id);
    if (found != null) {
        let fork: Run = found;
        if (fork.status == "migrated" && forward_result(app, fork, call_id, value, error)) {
            // An import may have replaced the record while the request was in flight.
            let current = find_run(app, run_id);
            if (current != null) {
                let record: Run = current;
                if (record.status == "migrated") {
                    record.waiting_on
                        = record.waiting_on.filter((w) -> {
                            w.call_id != call_id
                        });
                    touch(app, record);
                }
            }
        }
    }
    app.forwarding.delete(key);
}

/// At the start of the server: forwards every result that is stored on a
/// `migrated` record whose `waiting_on` still names the call. Such a pair
/// exists only for a fork whose forward did not succeed before the server
/// stopped.
function resume_forwards(app: App) -> void throws never {
    for (let run in app.runs) {
        if (run.status != "migrated") {
            continue;
        }
        for (let w in run.waiting_on) {
            let stored = run.remote_results.find((r) -> {
                r.call_id == w.call_id
            });
            if (stored == null) {
                continue;
            }
            let result: StoredResult = stored;
            let key = `${run.id}/${result.call_id}`;
            if (app.forwarding.has(key)) {
                continue;
            }
            app.forwarding.set(key, true);
            let run_id = run.id;
            spawn with baml.spawn.options(detach = true) {
                forward_to_moved_fork(app, run_id, result.call_id, result.value, result.error)
            };
        }
    }
}

/// Posts a remote result and retries while the other site is unreachable or
/// answers with a 5xx status. With `retry_404` a 404 is retried too: the other
/// site may still be importing the run. Returns true once the other site
/// accepted the result.
function post_result(url: string, body: json, retry_404: bool) -> bool throws never {
    let attempt = 0;
    while (attempt < 20) {
        attempt += 1;
        let status = post_json(url, body).status_code catch_all (e) {
            _ => 0
        };
        if (status >= 200 && status < 300) {
            return true;
        }
        let final_answer = status >= 400 && status < 500 && !(retry_404 && status == 404);
        if (final_answer) {
            // The other site answered and does not want the result (for
            // example it does not know the run). Retrying cannot help.
            warn(`${url} rejected the result with status ${status}`);
            return false;
        }
        warn(`could not deliver a result to ${url} (attempt ${attempt}, status ${status})`);
        baml.sys.sleep(baml.time.Duration.from_milliseconds(1500n)) catch_all (e) {
            _ => null
        };
    }
    false
}

/// The child side of a remote result: when a run with `parent` ends, its site
/// posts the result to the parent's site.
function finish_child(app: App, run: Run) -> void throws never {
    if (run.parent == null || run.result_delivered || !is_terminal(run.status)) {
        return;
    }
    spawn with baml.spawn.options(detach = true) { deliver_to_parent(app, run) };
}

function deliver_to_parent(app: App, run: Run) -> void throws never {
    let parent = run.parent;
    if (parent == null) {
        return;
    }
    let p: ParentRef = parent;
    let body: map<string, json> = { "call_id": p.call_id };
    if (run.status == "completed") {
        body.set("value", run.result);
    } else {
        body.set("error", run.error ?? `remote run ${run.id} ended with status ${run.status}`);
    }
    if (p.site == app.cfg.site) {
        // Parent and child share this site. Placement never does that, but a
        // parent can migrate to the site of its child and come back.
        let found = find_run(app, p.run);
        if (found != null) {
            let local_parent: Run = found;
            // For a migrated parent this only reaches forks that stayed here.
            accept_remote_result(app, local_parent, p.call_id, jget(body, "value"), run_error(body));
            if (local_parent.status == "migrated") {
                // The parent moved on. Follow `migrated_to` from here.
                if (forward_result(app, local_parent, p.call_id, jget(body, "value"), run_error(body))) {
                    run.result_delivered = true;
                    touch(app, run);
                }
                return;
            }
            run.result_delivered = true;
            touch(app, run);
            return;
        }
        warn(`run ${run.id}: the parent ${p.run} is not known on site ${app.cfg.site}`);
        return;
    }
    //# Call back the parent's site. The URL comes from SITES (contract section 8.3)
    let parent_url = site_url(app.cfg, p.site);
    if (parent_url == null) {
        warn(
            `run ${run.id}: the parent's site ${p.site} has no entry in SITES, so the result cannot be delivered`,
        );
        return;
    }
    let url = `${parent_url}/api/runs/${p.run}/remote_result`;
    if (post_result(url, body, false)) {
        run.result_delivered = true;
        touch(app, run);
    }
}

function run_error(body: json) -> string? throws never {
    let e = jget(body, "error");
    if let s: string = e {
        s
    } else {
        null
    }
}
