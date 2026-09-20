// Worker processes: start, the stdout and stderr readers, exit handling,
// commands on stdin, and remote call routing.

/// Starts the worker of the current segment of `run`. `resume_path` is null
/// for segment 1.
function launch(app: App, run: Run, resume_path: string?) -> void throws never {
    let cfg = app.cfg;
    let args: string[] = cfg.worker_cmd.slice(1, cfg.worker_cmd.length());
    // Contract section 9.5: a run that came from another site (a remote child
    // or an imported run) executes the program with its hash from the store of
    // this site, which `ensure_program` filled before this point. It gets no
    // `--project`, so this site never compiles that program.
    let by_hash = runs_by_hash(app, run);
    if (by_hash) {
        // The store of this site may have lost the entry since the run was
        // accepted (a collector, an operator, a torn write). The site that
        // sent the run can serve the program again, so a missing entry is
        // fetched before a worker is spent on it.
        let from = run.origin?.site ?? run.parent?.site ?? "";
        let missing = ensure_program(app, run.program_hash, from);
        if (missing != null) {
            let reason: string = missing;
            fail_before_start(app, run, resume_path != null, reason);
            return;
        }
    }
    // A cancel that arrived while the run was `starting` without a process
    // (the program was being fetched) is honoured before a worker is spent.
    let cancelled = locked(app, () -> {
        let reason = app.cancel_requested.get(run.id);
        app.cancel_requested.delete(run.id);
        reason
    });
    if (cancelled != null) {
        run.status = "cancelled";
        run.error = cancelled;
        emit_run_event(
            app,
            run.id,
            { "type": "run_cancelled", "run": run.id, "was": "starting", "reason": cancelled },
        );
        touch(app, run);
        cancel_children(app, run, `the parent run ${run.id} was cancelled`);
        finish_child(app, run);
        return;
    }
    if (!by_hash) {
        args = args.concat(["--project", cfg.program_dir]);
    }
    args
        = args.concat(
            [
                "--run",
                run.id,
                "--segment",
                run.segment.to_string(),
                "--snapshot-dir",
                run_dir(app, run.id),
            ],
        );
    if (!cfg.legacy_worker) {
        args = args.concat(["--program-store", cfg.programs_dir]);
        let suspend = cfg.sleep_suspend_ms;
        if (suspend != null) {
            args = args.concat(["--sleep-suspend-ms", suspend]);
        }
        if (by_hash && resume_path == null) {
            args = args.concat(["--program-hash", run.program_hash ?? ""]);
        }
    }
    let offered: string[] = [];
    if (resume_path == null) {
        args = args.concat(["--start", run.func, "--json-args", baml.json.stringify(run.args)]);
    } else {
        let p: string = resume_path;
        args = args.concat(["--resume", p]);
        // Results that arrived while the run had no process, with the time
        // each one arrived: the worker replays them in that order.
        for (let r in run.remote_results) {
            if (!r.acked) {
                offered.push(r.call_id);
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
    run.wake_at = null;
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
        cancel_children(app, run, `the parent run ${run.id} ended as ${run.status}`);
        finish_child(app, run);
        return;
    }
    let proc: baml.sys.Process = started;
    // A result that arrived after the argument list was built found no
    // process to write to. The registration of the process and the list of
    // such results are one step under the state lock, so every result is
    // either in the arguments, in this list, or written by
    // `accept_remote_result` itself.
    let late = locked(app, () -> {
        app.procs.set(run.id, proc);
        run.remote_results.filter((r) -> {
            !r.acked && !offered.includes(r.call_id)
        })
    });
    for (let r in late) {
        send_command(app, run, result_message(r, true));
    }
    if (app.cancel_requested.has(run.id)) {
        // The cancel arrived between the check above and this registration.
        send_command(app, run, { "type": "cancel" });
    }
    touch(app, run);
    let segment = run.segment;
    //# One supervisor thread per worker process
    spawn with baml.spawn.options(detach = true) { supervise(app, run, proc, segment) };
}

/// A run whose worker cannot be started because its program is missing. A
/// resume leaves the run suspended with the reason, because its snapshot is
/// intact and a later resume can succeed. A start ends the run as failed.
function fail_before_start(
    app: App,
    run: Run,
    resuming: bool,
    reason: string,
) -> void throws never {
    warn(`run ${run.id}: ${reason}`);
    if (resuming) {
        locked(app, () -> {
            run.status = "paused";
            run.segment -= 1;
            run.error = reason;
            null
        });
        touch(app, run);
        return;
    }
    run.status = "failed";
    run.error = reason;
    touch(app, run);
    cancel_children(app, run, `the parent run ${run.id} ended as ${run.status}`);
    finish_child(app, run);
}

/// True when the worker of `run` loads its program from the store by hash and
/// gets no `--project`: the run came from another site, and its program hash
/// is known.
function runs_by_hash(app: App, run: Run) -> bool throws never {
    if (app.cfg.legacy_worker || run.program_hash == null) {
        return false;
    }
    run.parent != null || run.origin != null
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
            // A pause request may change the status at the same moment.
            locked(app, () -> {
                if (run.status == "starting") {
                    run.status = "running";
                }
                null
            });
            let hash = jget_str(ev, "program_hash");
            if (hash.length() > 0) {
                run.program_hash = hash;
            }
            let build = jget_str(ev, "runtime_build");
            if (build.length() > 0) {
                app.worker_build = build;
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
                // Contract section 9.3: the worker reports how long the sleep
                // still lasts, and this server computes the deadline on its own
                // clock at the receipt of the event.
                let remaining = wake_remaining_ms(ev);
                if (remaining != null) {
                    run.wake_at = now_ms() + remaining;
                }
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
    if (kind == "remote_wait") {
        //# A restored thread waits on a call that this process never announced
        let waited = jget_str(ev, "call_id");
        let has_result = baml.json.path_or<bool>(ev, ".has_result", false);
        if (waited.length() > 0) {
            spawn with baml.spawn.options(detach = true) {
                reconcile_remote_wait(app, run, waited, jget_str(ev, "function"), has_result)
            };
        }
    }
    if (kind == "remote_call") {
        record_call(app, run, ev);
        // A worker that recovered from a snapshot that predates a cancellation
        // reaches the call again. Events arrive in order, so a `remote_cancel`
        // that was recorded before this event belongs to the earlier attempt,
        // and the call is wanted again.
        let again = jget_str(ev, "call_id");
        //# A remote_ call: place it on a site of the remote pool without blocking the reader
        let plan = locked(app, () -> {
            if (run.cancelled_calls.includes(again)) {
                run.cancelled_calls
                    = run.cancelled_calls.filter((c) -> {
                        c != again
                    });
                app.discarded.delete(`${run.id}/${again}`);
            }
            plan_remote_call(app, run, again)
        });
        spawn with baml.spawn.options(detach = true) { route_remote_call(app, run, ev, plan) };
    }
    if (kind == "remote_cancel") {
        //# The worker no longer waits on a remote call: cancel the child on its site
        let call_id = jget_str(ev, "call_id");
        if (call_id.length() > 0) {
            // Recorded before the next event of this worker is read, so a
            // dispatch that is still in flight sees it when it returns.
            locked(app, () -> {
                abandon_call(run, call_id);
                null
            });
            touch(app, run);
            spawn with baml.spawn.options(detach = true) { route_remote_cancel(app, run, call_id) };
        }
    }
}

/// `wake.remaining_ms` of a `paused` event as a whole number of milliseconds
/// that is not negative, or null when the event has no `wake`.
function wake_remaining_ms(ev: json) -> int? throws never {
    let wake = jget(ev, "wake");
    if (wake == null) {
        return null;
    }
    let whole = baml.json.path_or<int>(wake, ".remaining_ms", -1);
    if (whole >= 0) {
        return whole;
    }
    let fraction = baml.json.path_or<float>(wake, ".remaining_ms", -1.0);
    if (fraction >= 0.0) {
        return fraction.iceil() catch_all (e) {
            _ => 0
        };
    }
    0
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
    // A `cancel` or a `pause` that the API accepted after the worker had
    // written its self-suspend snapshot never reached the worker: its stdin
    // reader was gone. The request is honoured here instead. A requested pause
    // leaves the run `paused`, so no timer wakes a run that the user paused.
    let was_pausing = run.status == "pausing";
    let cancel_reason = locked(app, () -> {
        let reason = app.cancel_requested.get(run.id);
        app.cancel_requested.delete(run.id);
        reason
    });
    if (code == 75 && paused_now) {
        // A suspend for a sleep set `wake_at` at the `paused` event.
        if (run.wake_at != null && !was_pausing && cancel_reason == null) {
            run.status = "sleeping";
        } else {
            run.status = "paused";
        }
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
    if (run.status != "sleeping") {
        run.wake_at = null;
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
    if (run.status == "sleeping") {
        schedule_wake(app, run);
    }
    touch(app, run);
    if (cancel_reason != null && run.status == "paused") {
        //# A cancel that arrived while the worker was suspending itself
        let why: string = cancel_reason;
        cancel_without_process(app, run, why);
        return;
    }
    //# A run that ends as cancelled, failed, or lost cancels its outstanding children
    cancel_children(app, run, `the parent run ${run.id} ended as ${run.status}`);
    finish_child(app, run);
}

// ───────────────────────────── durable sleep ─────────────────────────────

/// Starts the wake timer of a `sleeping` run and announces the deadline. The
/// timer of an earlier suspend of the same run loses its entry in `app.timers`
/// and does nothing when it fires.
function schedule_wake(app: App, run: Run) -> void throws never {
    let wake_at = run.wake_at ?? now_ms();
    let seq = new_timer(app, run.id);
    emit_run_event(app, run.id, { "type": "sleep_scheduled", "run": run.id, "wake_at": wake_at });
    let run_id = run.id;
    spawn with baml.spawn.options(detach = true) { wake_timer(app, run_id, seq, wake_at, "timer") };
}

/// Registers a new wake timer for `run_id` and returns its sequence number. The
/// timer that was registered before loses its claim.
function new_timer(app: App, run_id: string) -> int throws never {
    locked(app, () -> {
        app.timer_seq += 1;
        app.timers.set(run_id, app.timer_seq);
        app.timer_seq
    })
}

/// Waits until `wake_at` on this server's clock and resumes the run. The
/// deadline is an absolute time, so a timer that is rebuilt after a restart
/// fires at the same moment as the one that was lost.
function wake_timer(
    app: App,
    run_id: string,
    seq: int,
    wake_at: int,
    reason: string,
) -> void throws never {
    let left = wake_at - now_ms();
    while (left > 0) {
        pause_ms(left);
        if (app.timers.get(run_id) != seq) {
            return;
        }
        left = wake_at - now_ms();
    }
    if (app.timers.get(run_id) != seq) {
        return;
    }
    let found = find_run(app, run_id);
    if (found == null) {
        return;
    }
    let run: Run = found;
    if (run.status != "sleeping") {
        return;
    }
    let snap = latest_snapshot(run);
    if (snap == null) {
        return;
    }
    if (begin_resume(app, run, reason)) {
        launch(app, run, snap.snapshot_path);
    }
}

/// The compare-and-set of contract section 9.3. Takes a run from `paused` or
/// `sleeping` to `starting` and advances its segment. Returns false, and
/// changes nothing, when the run is in any other status. The check and the
/// change happen under the state lock, so of two callers (two resume requests,
/// a timer and a request, a late timer, a resume and a cancel) exactly one gets
/// true, and only that one starts a worker.
function begin_resume(app: App, run: Run, reason: string) -> bool throws never {
    let was = locked(app, () -> {
        if (is_suspended(run.status)) {
            let before = run.status;
            run.status = "starting";
            run.wake_at = null;
            run.segment += 1;
            app.timers.delete(run.id);
            before
        } else {
            ""
        }
    });
    if (was.length() == 0) {
        return false;
    }
    if (was == "sleeping") {
        emit_run_event(app, run.id, { "type": "woken", "run": run.id, "reason": reason });
    }
    true
}

/// At the start of the server: rebuilds the wake timers from the run store.
/// A run whose deadline passed while the server was down resumes at once.
function rebuild_timers(app: App) -> void throws never {
    for (let run in app.runs) {
        if (run.status != "sleeping") {
            continue;
        }
        let wake_at = run.wake_at ?? 0;
        let overdue = wake_at <= now_ms();
        let seq = new_timer(app, run.id);
        if (!overdue) {
            emit_run_event(app, run.id, { "type": "sleep_scheduled", "run": run.id, "wake_at": wake_at });
        }
        let run_id = run.id;
        let reason = if (overdue) {
            "restart"
        } else {
            "timer"
        };
        spawn with baml.spawn.options(detach = true) {
            wake_timer(app, run_id, seq, wake_at, reason)
        };
    }
}

/// Ends a run that has no process (`paused` or `sleeping`) as `cancelled`
/// (contract section 9.3). Returns false when the run is in another status.
/// Like `begin_resume`, the check and the change happen under the state lock,
/// so a cancel and a resume of the same run cannot both succeed.
function cancel_without_process(app: App, run: Run, reason: string) -> bool throws never {
    let was = locked(app, () -> {
        if (is_suspended(run.status)) {
            let before = run.status;
            run.status = "cancelled";
            run.wake_at = null;
            run.error = reason;
            app.timers.delete(run.id);
            before
        } else {
            ""
        }
    });
    if (was.length() == 0) {
        return false;
    }
    emit_run_event(app, run.id, { "type": "run_cancelled", "run": run.id, "was": was, "reason": reason });
    touch(app, run);
    cancel_children(app, run, `the parent run ${run.id} was cancelled`);
    finish_child(app, run);
    true
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
    // When the result arrived here. A resumed worker orders the results it
    // gets at its start by this time.
    out.set("ts", r.ts);
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
    post_json_within(url, body, 10)
}

function post_json_within(
    url: string,
    body: json,
    seconds: int,
) -> baml.http.Response throws baml.errors.Io | baml.errors.Timeout | baml.errors.InvalidArgument {
    baml.http.send(
        baml.http.Request {
            method: "POST",
            url: url,
            headers: { "content-type": "application/json" },
            body: baml.json.stringify(body),
        },
        timeout = baml.time.Duration.from_seconds(seconds),
    )
}

/// What the site server does with one `remote_call` event.
class RemotePlan {
    /// `answer`: the result is stored already. `skip`: the call is dispatched,
    /// in flight, or no longer wanted. `fail`: the pool has no other site.
    /// `dispatch`: start a child on `target`.
    action: string,
    target: string,
}

/// Decides what happens with a `remote_call` event. The caller holds the state
/// lock and runs on the thread that reads the worker's events, so calls are
/// placed in the order in which the worker made them (a fan-out from `local`
/// alternates between the pool sites), and a `remote_cancel` that follows at
/// once finds the dispatch registered.
function plan_remote_call(app: App, run: Run, call_id: string) -> RemotePlan throws never {
    if (call_id.length() == 0) {
        return RemotePlan { action: "skip", target: "" };
    }
    // A resumed worker may reach a call again whose result is already here.
    let stored = run.remote_results.some((r) -> {
        r.call_id == call_id
    });
    if (stored) {
        return RemotePlan { action: "answer", target: "" };
    }
    let already = run.waiting_on.some((w) -> {
        w.call_id == call_id
    });
    if (already || app.dispatching.has(call_id) || call_abandoned(run, call_id)) {
        return RemotePlan { action: "skip", target: "" };
    }
    //# Placement: the eligible sites of REMOTE_POOL in round-robin order
    let target = pick_remote_site(app);
    if (target == null) {
        return RemotePlan { action: "fail", target: "" };
    }
    // `resume {site}` is refused while this entry exists, so the child is
    // always recorded on the record that travels with the run.
    app.dispatching.set(call_id, run.id);
    RemotePlan { action: "dispatch", target: target }
}

/// The parent side of a remote call: starts the child run on the site that
/// `plan_remote_call` picked (contract sections 8.2 and 9.3).
function route_remote_call(
    app: App,
    run: Run,
    ev: map<string, json>,
    plan: RemotePlan,
) -> void throws never {
    let call_id = jget_str(ev, "call_id");
    let func = jget_str(ev, "function");
    if (plan.action == "skip") {
        return;
    }
    if (plan.action == "answer") {
        let stored = run.remote_results.find((r) -> {
            r.call_id == call_id
        });
        if (stored != null) {
            send_command(app, run, result_message(stored, true));
        }
        return;
    }
    if (plan.action == "fail") {
        // The worker receives the error as a `remote_result`, so the user code
        // sees a failed call instead of waiting forever.
        warn(`remote call ${call_id}: REMOTE_POOL has no site other than ${app.cfg.site}`);
        accept_remote_result(app, run, call_id, null, "no remote site available", "");
        return;
    }
    let target_site = plan.target;
    let target_url = site_url(app.cfg, target_site) ?? "";
    // Contract section 9.5: the child runs the program of its parent. The other
    // site fetches the program from this site when its store does not hold it.
    let request: map<string, json> = {
        "function": func,
        "args": jget(ev, "args"),
        "parent": { "site": app.cfg.site, "run": run.id, "call_id": call_id },
        "program_hash": run.program_hash,
        "from_site": app.cfg.site,
    };
    let failure: string? = null;
    let child: json = null;
    pause_ms(app.cfg.chaos.dispatch_delay_ms);
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
    if (failure != null) {
        app.dispatching.delete(call_id);
        let message: string = failure;
        warn(message);
        accept_remote_result(app, run, call_id, null, message, "");
        return;
    }
    let waiting = Waiting {
        call_id: call_id,
        child_site: baml.json.path_or<string>(child, ".site", target_site),
        child_run: jget_str(child, "id"),
        func: func,
        inherited: false,
    };
    // A very fast child may have answered before this point. Or the call was
    // cancelled, or the run ended, while the dispatch was in flight: the child
    // exists now, so it is cancelled right away. The decision and the entry in
    // `waiting_on` are one step, so a cancel that arrives now either sees the
    // entry or was seen here.
    let outcome = locked(app, () -> {
        // The dispatch is over. `resume {site}` is allowed again from here on,
        // and the record that travels names the child.
        app.dispatching.delete(call_id);
        let has_answer = run.remote_results.some((r) -> {
            r.call_id == call_id
        });
        if (has_answer) {
            "answered"
        } else if (call_abandoned(run, call_id)) {
            abandon_call(run, call_id);
            "abandoned"
        } else {
            run.waiting_on.push(waiting);
            "waiting"
        }
    });
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
    if (outcome == "abandoned") {
        cancel_child(app, run, waiting, "the call was cancelled while its dispatch was in flight");
    }
}

// ───────────────────────────── remote cancellation ─────────────────────────────

/// Records that `run` no longer waits on `call_id`. A result that arrives for
/// the call later is discarded.
function abandon_call(run: Run, call_id: string) -> void throws never {
    if (!run.cancelled_calls.includes(call_id)) {
        run.cancelled_calls.push(call_id);
    }
}

/// True when a result for `call_id` has no taker: the worker cancelled the
/// call, or the run ended in a status that cancels its children.
function call_abandoned(run: Run, call_id: string) -> bool throws never {
    run.cancelled_calls.includes(call_id) || abandons_children(run.status)
}

/// A worker `remote_cancel` event (contract section 9.3): removes the
/// `waiting_on` entry and cancels the child on its site. When the dispatch of
/// the call is still in flight, `route_remote_call` cancels the child as soon
/// as the other site answered.
function route_remote_cancel(app: App, run: Run, call_id: string) -> void throws never {
    let entry = locked(app, () -> {
        let found = run.waiting_on.find((w) -> {
            w.call_id == call_id
        });
        if (found != null) {
            run.waiting_on
                = run.waiting_on.filter((w) -> {
                    w.call_id != call_id
                });
        }
        found
    });
    if (entry == null) {
        return;
    }
    let waiting: Waiting = entry;
    touch(app, run);
    cancel_child(app, run, waiting, "the caller cancelled the remote call");
}

/// Cancels every outstanding remote child of a run that ended as `cancelled`,
/// `failed`, or `lost` (contract section 9.3). Does nothing in another status.
function cancel_children(app: App, run: Run, reason: string) -> void throws never {
    let entries = locked(app, () -> {
        let none: Waiting[] = [];
        if (!abandons_children(run.status)) {
            none
        } else {
            let taken = run.waiting_on;
            run.waiting_on = [];
            for (let w in taken) {
                abandon_call(run, w.call_id);
            }
            taken
        }
    });
    if (entries.length() == 0) {
        return;
    }
    touch(app, run);
    for (let w in entries) {
        let waiting: Waiting = w;
        spawn with baml.spawn.options(detach = true) { cancel_child(app, run, waiting, reason) };
    }
}

/// Announces the cancellation of one remote child and sends the cancel request
/// to the child's site.
function cancel_child(app: App, run: Run, waiting: Waiting, reason: string) -> void throws never {
    //# A child that another run still waits on, or that another run dispatched, is not cancelled
    let shared_with = locked(app, () -> {
        other_waiter(app, run, waiting)
    });
    if (waiting.inherited || shared_with.length() > 0) {
        emit_run_event(
            app,
            run.id,
            {
                "type": "remote_released",
                "run": run.id,
                "call_id": waiting.call_id,
                "child_site": waiting.child_site,
                "child_run": waiting.child_run,
                "inherited": waiting.inherited,
                "shared_with": shared_with,
            },
        );
        return;
    }
    emit_run_event(
        app,
        run.id,
        {
            "type": "remote_cancelled",
            "run": run.id,
            "call_id": waiting.call_id,
            "child_site": waiting.child_site,
            "child_run": waiting.child_run,
        },
    );
    let body: map<string, json> = {
        "reason": reason,
        "parent": { "site": app.cfg.site, "run": run.id, "call_id": waiting.call_id },
    };
    send_cancel(app, waiting.child_site, waiting.child_run, body);
}

/// The id of another run of this site that still waits on the child of
/// `waiting`, or an empty string. A fork and its source wait on the same child
/// runs. The record of a fork that moved to another site counts while it holds
/// the entry, because the result is forwarded through it. The caller holds the
/// state lock.
function other_waiter(app: App, run: Run, waiting: Waiting) -> string throws never {
    for (let other in app.runs) {
        if (other.id == run.id || abandons_children(other.status) || other.status == "completed") {
            continue;
        }
        let waits = other.waiting_on.some((w) -> {
            w.call_id == waiting.call_id && w.child_run == waiting.child_run
        });
        if (waits) {
            return other.id;
        }
    }
    ""
}

/// Keeps what is needed to dispatch a call again (see `CallRecord`).
function record_call(app: App, run: Run, ev: map<string, json>) -> void throws never {
    let call_id = jget_str(ev, "call_id");
    if (call_id.length() == 0) {
        return;
    }
    let added = locked(app, () -> {
        let known = run.calls.some((c) -> {
            c.call_id == call_id
        });
        if (!known) {
            run.calls.push(
                CallRecord {
                    call_id: call_id,
                    func: jget_str(ev, "function"),
                    args: jget(ev, "args"),
                },
            );
        }
        !known
    });
    if (added) {
        touch(app, run);
    }
}

/// A resumed worker reported that a restored thread waits on `call_id`
/// (`remote_wait`). The worker does not announce such a call again, so without
/// this step a run whose record does not list the call would wait forever: a
/// fork that was taken before the dispatch of the call had returned, a fork of
/// a snapshot that is not the latest one, or a fork of a run whose calls were
/// cancelled later.
///
/// The call is settled from what this site knows, in this order: a result that
/// is stored on this run or on a run that shares the call id (the source of a
/// fork, or one of its forks); a child that still runs for such a run, to
/// which this run attaches itself; a dispatch that is in flight, which is
/// looked at again when it has returned; and otherwise a new dispatch from the
/// recorded arguments.
function reconcile_remote_wait(
    app: App,
    run: Run,
    call_id: string,
    func: string,
    has_result: bool,
) -> void throws never {
    let attempts = 0;
    while (attempts < 600) {
        attempts += 1;
        let step = locked(app, () -> {
            plan_remote_wait(app, run, call_id, has_result)
        });
        if (step.action == "wait") {
            pause_ms(100);
            continue;
        }
        if (step.action == "answer") {
            let stored = step.stored;
            if (stored != null) {
                touch(app, run);
                send_command(app, run, result_message(stored, true));
            }
            return;
        }
        if (step.action == "attached") {
            emit_run_event(
                app,
                run.id,
                {
                    "type": "remote_attached",
                    "run": run.id,
                    "call_id": call_id,
                    "child_site": step.entry?.child_site ?? "",
                    "child_run": step.entry?.child_run ?? "",
                    "function": func,
                },
            );
            touch(app, run);
            return;
        }
        if (step.action == "dispatch") {
            let record = step.record;
            if (record == null) {
                accept_remote_result(
                    app,
                    run,
                    call_id,
                    null,
                    `the run waits on remote call ${call_id}, which this site has no record of`,
                    "",
                );
                return;
            }
            let ev: map<string, json> = { "call_id": call_id, "function": record.func, "args": record.args };
            route_remote_call(app, run, ev, RemotePlan { action: "dispatch", target: step.target });
            return;
        }
        return;
    }
}

class WaitStep {
    /// `done`, `answer`, `attached`, `wait`, or `dispatch`.
    action: string,
    stored: StoredResult?,
    entry: Waiting?,
    record: CallRecord?,
    target: string,
}

/// One look at the state for `reconcile_remote_wait`. The caller holds the
/// state lock.
function plan_remote_wait(
    app: App,
    run: Run,
    call_id: string,
    has_result: bool,
) -> WaitStep throws never {
    let done = WaitStep { action: "done", stored: null, entry: null, record: null, target: "" };
    if (!app.procs.has(run.id) || abandons_children(run.status)) {
        return done;
    }
    let own = run.remote_results.find((r) -> {
        r.call_id == call_id
    });
    if (own != null) {
        if (has_result) {
            return done;
        }
        return WaitStep { ...done, action: "answer", stored: own };
    }
    if (
        run.waiting_on.some((w) -> {
            w.call_id == call_id
        })
    ) {
        return done;
    }
    // The snapshot predates a cancellation of the call: it is wanted again.
    if (run.cancelled_calls.includes(call_id)) {
        run.cancelled_calls
            = run.cancelled_calls.filter((c) -> {
                c != call_id
            });
        app.discarded.delete(`${run.id}/${call_id}`);
    }
    // A result or a live child of a run that shares the call id.
    for (let other in app.runs) {
        if (other.id == run.id) {
            continue;
        }
        let result = other.remote_results.find((r) -> {
            r.call_id == call_id
        });
        if (result != null) {
            let copy = StoredResult { ...result, acked: false };
            run.remote_results.push(copy);
            return WaitStep { ...done, action: "answer", stored: copy };
        }
    }
    for (let other in app.runs) {
        if (other.id == run.id || abandons_children(other.status)) {
            continue;
        }
        let entry = other.waiting_on.find((w) -> {
            w.call_id == call_id
        });
        if (entry != null) {
            let attached = Waiting { ...entry, inherited: true };
            run.waiting_on.push(attached);
            return WaitStep { ...done, action: "attached", entry: attached };
        }
    }
    if (app.dispatching.has(call_id)) {
        return WaitStep { ...done, action: "wait" };
    }
    let record = run.calls.find((c) -> {
        c.call_id == call_id
    });
    if (record == null) {
        return WaitStep { ...done, action: "dispatch" };
    }
    let target = pick_remote_site(app);
    if (target == null) {
        return WaitStep { ...done, action: "dispatch" };
    }
    app.dispatching.set(call_id, run.id);
    WaitStep { ...done, action: "dispatch", record: record, target: target }
}

/// Posts `cancel` for a run on another site. Retries while that site is
/// unreachable or answers with a 5xx status. A 4xx answer is final: the run
/// has ended already (409) or is not known there (404).
function send_cancel(app: App, site: string, run_id: string, body: json) -> bool throws never {
    if (run_id.length() == 0) {
        return false;
    }
    if (site == app.cfg.site) {
        let found = find_run(app, run_id);
        if (found == null) {
            return false;
        }
        cancel_here(app, found, jget_str(body, "reason"));
        return true;
    }
    let base = site_url(app.cfg, site);
    if (base == null) {
        warn(`cannot cancel run ${run_id}: site ${site} has no entry in SITES`);
        return false;
    }
    pause_ms(app.cfg.chaos.cancel_delay_ms);
    let attempt = 0;
    while (attempt < 10) {
        attempt += 1;
        let status = post_json(`${base}/api/runs/${run_id}/cancel`, body).status_code catch_all (e) {
            _ => 0
        };
        if (status >= 200 && status < 300) {
            return true;
        }
        if (status >= 400 && status < 500) {
            return false;
        }
        warn(`could not cancel run ${run_id} on site ${site} (attempt ${attempt}, status ${status})`);
        pause_ms(1500);
    }
    false
}

/// Cancels `run` on this site. Returns a message when the run cannot be
/// cancelled, and null otherwise. A run with a worker gets the `cancel`
/// command. A `paused` or `sleeping` run is cancelled without a process. A
/// `migrated` record passes the request on to the site that holds the run.
function cancel_here(app: App, run: Run, reason: string) -> string? throws never {
    let why = if (reason.length() == 0) {
        "cancelled by request"
    } else {
        reason
    };
    // The request is recorded before the command is written. A worker that is
    // between its `paused` event and its exit does not read the command, and
    // `on_exit` then completes the cancellation.
    let target = locked(app, () -> {
        if (app.procs.has(run.id)) {
            app.cancel_requested.set(run.id, why);
            "process"
        } else if (run.status == "starting") {
            // No process yet: `launch` is fetching the program. It looks at
            // the request before it starts the worker.
            app.cancel_requested.set(run.id, why);
            "starting"
        } else {
            "none"
        }
    });
    if (target == "starting") {
        return null;
    }
    if (target == "process") {
        if (!send_command(app, run, { "type": "cancel" })) {
            // The pipe is closed: the process is ending, and `on_exit` acts on
            // the recorded request when the run suspended itself.
            if (app.procs.has(run.id)) {
                return null;
            }
            return `could not send cancel to the worker of ${run.id}`;
        }
        return null;
    }
    if (cancel_without_process(app, run, why)) {
        return null;
    }
    if (run.status == "migrated") {
        let to = run.migrated_to ?? "";
        let run_id = run.id;
        spawn with baml.spawn.options(detach = true) {
            send_cancel(app, to, run_id, { "reason": why })
        };
        return null;
    }
    `run ${run.id} is ${run.status} and has no worker process`
}

/// What `accept_remote_result` decided for one run that a result concerns.
class Delivery {
    target: Run,
    /// `store`: the result was stored and goes to the worker. `discard`: the
    /// run cancelled the call. `stale`: the result comes from a child that the
    /// run does not wait on (the first attempt of a call that was requested
    /// again). `forward`: the run is a fork that migrated away, and the result
    /// is sent after it.
    action: string,
    stored: StoredResult?,
    entry: Waiting?,
    /// For `discard`: false when the same discard was announced before.
    announce: bool,
    /// For `forward`: true when the result was stored on the record just now.
    changed: bool,
}

/// The parent side of a remote result. The result is stored in `meta.json`
/// first. It is written to the worker's stdin if a worker is running, and
/// passed as `--remote-result` at the next resume otherwise. Forks that wait
/// on the same call receive it too. A fork that migrated to another site gets
/// the result forwarded: the result is stored on the record that stayed here,
/// and the `waiting_on` entry of that record is removed once the other site
/// accepted the result. `run` itself is skipped when it is `migrated`: the
/// caller forwards the result in that case.
///
/// A result for a call that the run cancelled is discarded (contract section
/// 9.3). A result can arrive twice, and two deliveries can arrive at the same
/// time, so every decision and the change that belongs to it are made under
/// the state lock. Events, files, and the worker's stdin follow after it.
function accept_remote_result(
    app: App,
    run: Run,
    call_id: string,
    value: json,
    error: string?,
    child_run: string,
) -> void throws never {
    let ts = now_ms();
    let deliveries = locked(app, () -> {
        plan_deliveries(app, run, call_id, value, error, child_run, ts)
    });
    for (let d in deliveries) {
        let target = d.target;
        if (d.action == "forward") {
            // The fork lives on another site now. The entry stays until the
            // other site accepted the result, so a forward that fails is tried
            // again at the next delivery or at the next start of this server.
            if (d.changed) {
                touch(app, target);
            }
            let moved_id = target.id;
            spawn with baml.spawn.options(detach = true) {
                forward_to_moved_fork(app, moved_id, call_id, value, error)
            };
            continue;
        }
        if (d.action == "discard" || d.action == "stale") {
            if (d.announce) {
                let ev: map<string, json> = {
                    "type": "remote_result_discarded",
                    "run": target.id,
                    "call_id": call_id,
                    "ok": error == null,
                };
                if (d.action == "stale") {
                    ev.set("child_run", child_run);
                }
                emit_run_event(app, target.id, ev);
            }
            continue;
        }
        let stored = d.stored;
        if (stored == null) {
            continue;
        }
        emit_run_event(
            app,
            target.id,
            {
                "type": "remote_returned",
                "run": target.id,
                "call_id": call_id,
                "child_site": d.entry?.child_site ?? "",
                "child_run": d.entry?.child_run ?? "",
                "ok": error == null,
            },
        );
        touch(app, target);
        send_command(app, target, result_message(stored, true));
    }
}

/// The bookkeeping of `accept_remote_result`. The caller holds the state lock.
function plan_deliveries(
    app: App,
    run: Run,
    call_id: string,
    value: json,
    error: string?,
    child_run: string,
    ts: int,
) -> Delivery[] throws never {
    let out: Delivery[] = [];
    let candidates: Run[] = [];
    if (run.status != "migrated") {
        candidates.push(run);
    }
    for (let other in app.runs) {
        let waits = other.id != run.id
            && other.waiting_on.some((w) -> {
                w.call_id == call_id
            });
        if (waits) {
            candidates.push(other);
        }
    }
    for (let target in candidates) {
        //# A result for a call that the run cancelled is discarded
        if (target.cancelled_calls.includes(call_id)) {
            let key = `${target.id}/${call_id}`;
            let first = !app.discarded.has(key);
            app.discarded.set(key, true);
            out.push(
                Delivery {
                    target: target,
                    action: "discard",
                    stored: null,
                    entry: null,
                    announce: first,
                    changed: false,
                },
            );
            continue;
        }
        if (target.status == "migrated") {
            // `forwarding` keeps a repeated delivery from forwarding twice.
            let key = `${target.id}/${call_id}`;
            if (app.forwarding.has(key)) {
                continue;
            }
            app.forwarding.set(key, true);
            let kept = target.remote_results.some((r) -> {
                r.call_id == call_id
            });
            if (!kept) {
                target.remote_results.push(
                    StoredResult {
                        call_id: call_id,
                        value: value,
                        error: error,
                        acked: false,
                        ts: ts,
                    },
                );
            }
            out.push(
                Delivery {
                    target: target,
                    action: "forward",
                    stored: null,
                    entry: null,
                    announce: false,
                    changed: !kept,
                },
            );
            continue;
        }
        let known = target.remote_results.some((r) -> {
            r.call_id == call_id
        });
        if (known) {
            continue;
        }
        let entry = target.waiting_on.find((w) -> {
            w.call_id == call_id
        });
        // A call that was cancelled and then requested again runs as a second
        // child under the same call id. The late answer of the first child must
        // not settle the second attempt. `child_run` is empty for a result that
        // this server built itself and for a sender that does not name the child.
        if (child_run.length() > 0 && entry != null) {
            if (entry.child_run != child_run) {
                out.push(
                    Delivery {
                        target: target,
                        action: "stale",
                        stored: null,
                        entry: entry,
                        announce: true,
                        changed: false,
                    },
                );
                continue;
            }
        }
        let stored = StoredResult { call_id: call_id, value: value, error: error, acked: false, ts: ts };
        target.remote_results.push(stored);
        target.waiting_on
            = target.waiting_on.filter((w) -> {
                w.call_id != call_id
            });
        out.push(
            Delivery {
                target: target,
                action: "store",
                stored: stored,
                entry: entry,
                announce: false,
                changed: true,
            },
        );
    }
    out
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
                    locked(app, () -> {
                        record.waiting_on
                            = record.waiting_on.filter((w) -> {
                                w.call_id != call_id
                            });
                        null
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
    // `child_run` lets the parent's site tell the answer of a cancelled first
    // attempt from the answer of the child that it waits on now.
    let body: map<string, json> = { "call_id": p.call_id, "child_run": run.id };
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
            accept_remote_result(app, local_parent, p.call_id, jget(body, "value"), run_error(body), run.id);
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
    if (send_result(app, url, body)) {
        run.result_delivered = true;
        touch(app, run);
    }
}

/// Posts the result of a child run to the parent's site, with the CHAOS knobs
/// of contract section 9.3 applied: a delay, a hold that lets the next result
/// overtake this one, and a second delivery of the same result.
function send_result(app: App, url: string, body: json) -> bool throws never {
    let chaos = app.cfg.chaos;
    pause_ms(chaos.result_delay_ms);
    let hold = chaos.reorder_results
        && locked(app, () -> {
            if (app.holding_result) {
                false
            } else {
                app.holding_result = true;
                true
            }
        });
    if (hold) {
        let seen = app.results_sent;
        let held = 0;
        while (app.results_sent == seen && held < chaos.reorder_hold_ms) {
            pause_ms(50);
            held += 50;
        }
        app.holding_result = false;
    }
    let ok = post_result(url, body, false);
    locked(app, () -> {
        app.results_sent += 1;
        null
    });
    if (ok && chaos.duplicate_results) {
        post_result(url, body, false);
    }
    ok
}

function run_error(body: json) -> string? throws never {
    let e = jget(body, "error");
    if let s: string = e {
        s
    } else {
        null
    }
}
