// Configuration, the in-memory run table, the run store on disk, and the SSE
// fan-out.

/// One entry of the site registry (contract section 8.1).
class SiteEntry {
    name: string,
    url: string,
}

class Config {
    site: string,
    port: string,
    /// The registry from SITES, in the order of the JSON object.
    sites: SiteEntry[],
    /// REMOTE_POOL: the sites that may host `remote_` calls, in order.
    remote_pool: string[],
    worker_cmd: string[],
    worker_cwd: string,
    /// The environment of worker processes: the server's own environment plus
    /// BAML_CLI_ALLOW_DIRECT=1. Null when the environment could not be read;
    /// workers inherit the server's environment unchanged in that case.
    worker_env: map<string, string>?,
    program_dir: string,
    /// `program_dir` with symbolic links resolved. A worker may report
    /// absolute source paths in this form.
    program_real_dir: string,
    runs_dir: string,
    /// PROGRAMS_DIR: the content-addressed program store of this site
    /// (contract section 9.5).
    programs_dir: string,
    /// SLEEP_SUSPEND_MS: passed to workers as `--sleep-suspend-ms`. Null leaves
    /// the worker's default in place.
    sleep_suspend_ms: string?,
    /// WORKER_LEGACY=1: the worker predates contract section 9. It gets none of
    /// the flags `--program-store`, `--program-hash`, and `--sleep-suspend-ms`,
    /// and always `--project`.
    legacy_worker: bool,
    /// CHAOS: the test knobs of contract section 9.3.
    chaos: Chaos,
}

/// The test knobs from the environment variable CHAOS (contract section 9.3).
/// Each delay is applied by the site that sends the named site-to-site request,
/// before it sends it. Every field is zero or false when CHAOS is not set.
class Chaos {
    dispatch_delay_ms: int,
    result_delay_ms: int,
    cancel_delay_ms: int,
    import_delay_ms: int,
    program_fetch_delay_ms: int,
    /// Send every `remote_result` twice.
    duplicate_results: bool,
    /// Hold one result until the next one was sent.
    reorder_results: bool,
    /// Extension: the longest time that `reorder_results` holds a result when no
    /// other result follows. Default 4000.
    reorder_hold_ms: int,
}

/// One SSE subscriber. Frames are queued and written by one drain thread per
/// subscriber, so a slow browser never blocks the thread that reads a worker.
class Subscriber {
    res: baml.http.Response,
    queue: uint8array[],
    draining: bool,
    dead: bool,
}

/// Coalesces concurrent writes of one `meta.json`.
class MetaWriter {
    writing: bool,
    dirty: bool,
}

class App {
    cfg: Config,
    runs: Run[],
    procs: map<string, baml.sys.Process>,
    subs: Subscriber[],
    writers: map<string, MetaWriter>,
    /// Call ids whose dispatch to a remote site is in flight, mapped to the id
    /// of the run that made the call.
    dispatching: map<string, string>,
    /// Forwards of a result to a fork that migrated, keyed by
    /// `<run id>/<call id>`, while they are in flight.
    forwarding: map<string, bool>,
    /// Run ids whose import is in progress on this site.
    importing: map<string, bool>,
    functions: FunctionInfo[],
    /// Wake timers of `sleeping` runs: run id to the sequence number of the
    /// timer thread that may wake the run. A timer thread whose number is not
    /// the one in this map does nothing.
    timers: map<string, int>,
    timer_seq: int,
    /// Placement counter (contract section 9.3): the number of remote calls
    /// that this site placed so far.
    placed: int,
    /// Program hashes that were verified in the program store of this site.
    programs: map<string, bool>,
    /// Program hashes whose fetch from another site is in flight.
    fetching: map<string, bool>,
    /// CHAOS `reorder_results`: true while one result is held back, and the
    /// number of results that this site sent so far.
    holding_result: bool,
    results_sent: int,
    /// `<run id>/<call id>` of results that were discarded, so that a second
    /// delivery of the same result is not announced again.
    discarded: map<string, bool>,
    /// Run ids with a worker process that were asked to cancel, mapped to the
    /// reason. A worker that has already written its snapshot and is about to
    /// exit with code 75 cannot act on the command any more, so the exit
    /// handler finishes the cancellation.
    cancel_requested: map<string, string>,
    /// The runtime build of the workers of this site, from the first `hello`
    /// or from the first program entry that a worker of this site stored.
    /// Empty until it is known.
    worker_build: string,
    /// The state lock: a task group with the limit 1. See `locked`.
    lock: baml.spawn.TaskGroup,
}

// ───────────────────────────── the state lock ─────────────────────────────
// BAML threads run in parallel and are preempted between any two instructions:
// eight threads that each check a field and then set it all see the old value
// (measured with toolchain 0.19.1). Every HTTP request, every worker reader, and
// every timer of this server is its own thread, so a check that is followed by a
// change needs mutual exclusion. The stdlib has no mutex. A task group with the
// limit 1 admits one task at a time in FIFO order, which is the same guarantee.
//
// Rules for a body that runs under the lock:
// - It performs no sys-op (no file, no HTTP, no process, no sleep, no print),
//   so the lock is held for microseconds.
// - It does not call `locked` again. The second task would wait for the first
//   one, which waits for the second.

/// Runs `body` while no other `locked` body of this server runs.
function locked<T>(app: App, body: () -> T throws never) -> T throws never {
    await spawn with baml.spawn.options(group = app.lock) { body() }
}

// ───────────────────────────── diagnostics ─────────────────────────────
// `log.warn` prints nothing when it runs on a thread that descends from an
// HTTP request handler (toolchain 0.19.1-nightly), so diagnostics go to stderr.

function warn(message: string) -> void throws never {
    baml.io.eprintln(`[warn] ${message}`);
}

// ───────────────────────────── configuration ─────────────────────────────

function env_or(name: string, default: string) -> string throws never {
    let v = baml.env.get(name) catch_all (e) {
        _ => null
    };
    if (v == null) {
        return default;
    }
    let s: string = v;
    if (s.trim().length() == 0) {
        default
    } else {
        s
    }
}

/// The working directory of this process. The PWD variable is only a fallback:
/// a parent process that started the server with a `cwd` option leaves its own
/// PWD in the environment.
function current_dir() -> string throws never {
    let out = baml.sys.exec("pwd", [], null) catch_all (e) {
        _ => null
    };
    if (out != null) {
        let dir = out.stdout.to_string().trim();
        if (dir.starts_with("/")) {
            return dir;
        }
    }
    env_or("PWD", ".")
}

/// Resolves `.` and `..` segments. A relative path is resolved against `base`.
function absolute_path(base: string, path: string) -> string throws never {
    let full = if (path.starts_with("/")) {
        path
    } else {
        `${base}/${path}`
    };
    let out: string[] = [];
    for (let seg in full.split("/")) {
        if (seg.length() == 0 || seg == ".") {
            continue;
        }
        if (seg == "..") {
            out.pop();
            continue;
        }
        out.push(seg);
    }
    "/" + out.join("/")
}

/// The environment of this process, read from `env -0`. `ProcessOptions.env`
/// replaces the whole environment of a child, and the stdlib has no call that
/// lists the variables, so the server asks `env` once at startup. Returns an
/// empty map when that fails.
function own_environment() -> map<string, string> throws never {
    let out: map<string, string> = {};
    let res = baml.sys.exec("env", ["-0"], null) catch_all (e) {
        _ => null
    };
    if (res == null) {
        return out;
    }
    let bytes = res.stdout.to_array();
    let n = bytes.length();
    let start = 0;
    let i = 0;
    while (i <= n) {
        if (i == n || bytes[i] == 0) {
            if (i > start) {
                let entry = baml.Uint8Array.from_array(bytes.slice(start, i)) catch_all (e) {
                    _ => null
                };
                if (entry != null) {
                    let text = entry.to_string();
                    let eq = text.index_of("=");
                    if (eq != null) {
                        if (eq > 0) {
                            out.set(text.slice(0, eq), text.slice(eq + 1, text.length()));
                        }
                    }
                }
            }
            start = i + 1;
        }
        i += 1;
    }
    out
}

/// The environment for worker processes. `baml-cli worker` prints a warning
/// on stderr when it is started directly unless BAML_CLI_ALLOW_DIRECT=1 is set.
function worker_environment() -> map<string, string>? throws never {
    let env = own_environment();
    if (!env.has("PATH")) {
        warn("could not read the environment; workers inherit it without BAML_CLI_ALLOW_DIRECT=1");
        return null;
    }
    env.set("BAML_CLI_ALLOW_DIRECT", "1");
    env
}

/// `dir` with symbolic links resolved (`pwd -P`), or `dir` when that fails.
function real_dir(dir: string) -> string throws never {
    let out = baml.sys.exec(
        "pwd",
        ["-P"],
        baml.sys.ProcessOptions { cwd: dir, env: null, timeout_ms: null, stdin: null, stderr: null },
    ) catch_all (e) {
        _ => null
    };
    if (out != null) {
        let resolved = out.stdout.to_string().trim();
        if (resolved.starts_with("/")) {
            return resolved;
        }
    }
    dir
}

/// The default of SITES (contract section 8.1).
function default_sites() -> string throws never {
    `{"local":"http://127.0.0.1:8787","cloud":"http://127.0.0.1:8788","cloud2":"http://127.0.0.1:8789"}`
}

/// The default of REMOTE_POOL (contract section 8.1).
function default_remote_pool() -> string throws never {
    `["cloud","cloud2"]`
}

/// True when `name` matches `[a-z0-9]+`.
function valid_site_name(name: string) -> bool throws never {
    if (name.length() == 0) {
        return false;
    }
    let allowed = "abcdefghijklmnopqrstuvwxyz0123456789";
    for (let ch in name.chars()) {
        if (!allowed.includes(ch)) {
            return false;
        }
    }
    true
}

/// The registry in `text`, a JSON object that maps site names to base URLs.
/// Entries with an invalid name or a value that is not a string are skipped.
/// A trailing `/` of a URL is removed.
function parse_sites(text: string) -> SiteEntry[] throws never {
    let out: SiteEntry[] = [];
    if let m: map<string, json> = jparse(text) {
        for (let name in m.keys()) {
            if let url: string = m.get(name) {
                if (!valid_site_name(name)) {
                    warn(`SITES: ignoring the site name ${name}, which does not match [a-z0-9]+`);
                    continue;
                }
                let base = url.trim();
                while (base.ends_with("/")) {
                    base = base.slice(0, base.length() - 1);
                }
                out.push(SiteEntry { name: name, url: base });
            } else {
                warn(`SITES: the URL of site ${name} is not a string`);
            }
        }
    }
    out
}

/// The port in a base URL such as `http://127.0.0.1:8787`, or null.
function url_port(url: string) -> string? throws never {
    let rest = url;
    let scheme = rest.index_of("://");
    if (scheme != null) {
        rest = rest.slice(scheme + 3, rest.length());
    }
    let slash = rest.index_of("/");
    if (slash != null) {
        rest = rest.slice(0, slash);
    }
    let colon = rest.last_index_of(":");
    if (colon == null) {
        return null;
    }
    let digits = rest.slice(colon + 1, rest.length());
    let port = baml.Int.parse(digits) catch_all (e) {
        _ => -1
    };
    if (port <= 0) {
        return null;
    }
    digits
}

/// The base URL of `site` from the registry, or null for an unknown site.
function site_url(cfg: Config, site: string) -> string? throws never {
    let entry = cfg.sites.find((s) -> {
        s.name == site
    });
    if (entry == null) {
        return null;
    }
    entry.url
}

/// The sites that may host a remote call of this site (contract section 8.2):
/// REMOTE_POOL in its order without this site.
function eligible_sites(cfg: Config) -> string[] throws never {
    cfg.remote_pool.filter((name) -> {
        name != cfg.site
    })
}

/// Placement of a remote call (contract section 9.3): the eligible sites in
/// round-robin order with one counter per caller site, or null when the pool
/// has no other site. The caller holds the state lock (`plan_remote_call`), so
/// two calls that are placed at the same time get different sites.
function pick_remote_site(app: App) -> string? throws never {
    let eligible = eligible_sites(app.cfg);
    if (eligible.length() == 0) {
        return null;
    }
    let target = eligible[app.placed % eligible.length()];
    app.placed += 1;
    target
}

/// The CHAOS knobs. A value that is not a JSON object leaves every knob off.
function parse_chaos(text: string) -> Chaos throws never {
    let chaos = Chaos {
        dispatch_delay_ms: 0,
        result_delay_ms: 0,
        cancel_delay_ms: 0,
        import_delay_ms: 0,
        program_fetch_delay_ms: 0,
        duplicate_results: false,
        reorder_results: false,
        reorder_hold_ms: 4000,
    };
    if (text.trim().length() == 0) {
        return chaos;
    }
    let parsed = jparse(text);
    if (!(parsed is map<string, json>)) {
        warn(`CHAOS is not a JSON object: ${text}`);
        return chaos;
    }
    chaos.dispatch_delay_ms = jget_int(parsed, "dispatch_delay_ms", 0);
    chaos.result_delay_ms = jget_int(parsed, "result_delay_ms", 0);
    chaos.cancel_delay_ms = jget_int(parsed, "cancel_delay_ms", 0);
    chaos.import_delay_ms = jget_int(parsed, "import_delay_ms", 0);
    chaos.program_fetch_delay_ms = jget_int(parsed, "program_fetch_delay_ms", 0);
    chaos.duplicate_results = baml.json.path_or<bool>(parsed, ".duplicate_results", false);
    chaos.reorder_results = baml.json.path_or<bool>(parsed, ".reorder_results", false);
    chaos.reorder_hold_ms = jget_int(parsed, "reorder_hold_ms", 4000);
    chaos
}

/// True when any CHAOS knob is on.
function chaos_active(c: Chaos) -> bool throws never {
    c.dispatch_delay_ms > 0
        || c.result_delay_ms > 0
        || c.cancel_delay_ms > 0
        || c.import_delay_ms > 0
        || c.program_fetch_delay_ms > 0
        || c.duplicate_results
        || c.reorder_results
}

/// Sleeps `ms` milliseconds. Used for the CHAOS delays and for short waits.
function pause_ms(ms: int) -> void throws never {
    if (ms <= 0) {
        return;
    }
    baml.sys.sleep(baml.time.Duration.from_milliseconds(ms)) catch_all (e) {
        _ => null
    };
}

function load_config() -> Config throws never {
    let site = env_or("SITE", "local");
    let sites_text = env_or("SITES", default_sites());
    let sites = parse_sites(sites_text);
    if (sites.length() == 0) {
        warn(`SITES is not a JSON object of site names and URLs: ${sites_text}`);
        sites = parse_sites(default_sites());
    }
    let own = sites.find((s) -> {
        s.name == site
    });
    let own_port: string? = null;
    if (own == null) {
        warn(`site ${site} has no entry in SITES, so other sites cannot call it back`);
    } else {
        own_port = url_port(own.url);
    }
    let port = env_or("PORT", own_port ?? "8787");
    let pool_text = env_or("REMOTE_POOL", default_remote_pool());
    let pool_parsed = baml.json.from_string<string[]>(pool_text) catch_all (e) {
        _ => {
            warn(`REMOTE_POOL is not a JSON array of site names: ${pool_text}`);
            baml.json.from_string<string[]>(default_remote_pool()) catch_all (e2) {
                _ => {
                    let none: string[] = [];
                    none
                }
            }
        }
    };
    // A pool member without a registry entry cannot be reached.
    let remote_pool: string[] = [];
    for (let name in pool_parsed) {
        let known = sites.some((s) -> {
            s.name == name
        });
        if (known) {
            remote_pool.push(name);
        } else {
            warn(`REMOTE_POOL: ignoring the site ${name}, which has no entry in SITES`);
        }
    }
    let cwd = current_dir();
    // The default worker command is relative to demo_durable/, the parent of
    // the server directory, so workers start there unless WORKER_CWD says otherwise.
    let worker_cwd = absolute_path(cwd, env_or("WORKER_CWD", ".."));
    let cmd_text = env_or("WORKER_CMD", `["node","scripts/mock-worker.mjs"]`);
    let cmd = baml.json.from_string<string[]>(cmd_text) catch_all (e) {
        _ => {
            warn(`WORKER_CMD is not a JSON array of strings: ${cmd_text}`);
            ["node", "scripts/mock-worker.mjs"]
        }
    };
    if (cmd.length() == 0) {
        cmd = ["node", "scripts/mock-worker.mjs"];
    }
    let program_dir = absolute_path(cwd, env_or("PROGRAM_DIR", "../program"));
    let suspend_text = env_or("SLEEP_SUSPEND_MS", "").trim();
    let sleep_suspend_ms: string? = null;
    if (suspend_text.length() > 0) {
        let parsed_ms = baml.Int.parse(suspend_text) catch_all (e) {
            _ => -1
        };
        if (parsed_ms < 0) {
            warn(`SLEEP_SUSPEND_MS is not a number of milliseconds: ${suspend_text}`);
        } else {
            sleep_suspend_ms = parsed_ms.to_string();
        }
    }
    Config {
        site: site,
        port: port,
        sites: sites,
        remote_pool: remote_pool,
        worker_cmd: cmd,
        worker_cwd: worker_cwd,
        worker_env: worker_environment(),
        program_dir: program_dir,
        program_real_dir: real_dir(program_dir),
        runs_dir: absolute_path(cwd, env_or("RUNS_DIR", ".baml/runs")),
        programs_dir: absolute_path(cwd, env_or("PROGRAMS_DIR", `.baml/programs-${site}`)),
        sleep_suspend_ms: sleep_suspend_ms,
        legacy_worker: env_or("WORKER_LEGACY", "0") == "1",
        chaos: parse_chaos(env_or("CHAOS", "")),
    }
}

// ───────────────────────────── run table ─────────────────────────────

function find_run(app: App, id: string) -> Run? throws never {
    app.runs.find((r) -> {
        r.id == id
    })
}

function run_dir(app: App, id: string) -> string throws never {
    `${app.cfg.runs_dir}/${id}`
}

function new_run_id(app: App) -> string throws never {
    let alphabet = "abcdefghijklmnopqrstuvwxyz0123456789";
    while (true) {
        let bytes = baml.random.SystemRandom.get().random(6);
        let id = "r-";
        for (let b in bytes.to_array()) {
            let i = b % 36;
            id = id + alphabet.slice(i, i + 1);
        }
        let taken = baml.fs.exists(run_dir(app, id)) catch_all (e) {
            _ => false
        };
        if (find_run(app, id) == null && !taken) {
            return id;
        }
    }
    ""
}

function is_live(status: string) -> bool throws never {
    status == "starting" || status == "running" || status == "pausing"
}

function is_terminal(status: string) -> bool throws never {
    status == "completed" || status == "failed" || status == "lost" || status == "cancelled"
}

/// A run without a process whose state lives in its latest snapshot.
function is_suspended(status: string) -> bool throws never {
    status == "paused" || status == "sleeping"
}

/// The statuses whose outstanding remote children are cancelled (contract
/// section 9.3).
function abandons_children(status: string) -> bool throws never {
    status == "cancelled" || status == "failed" || status == "lost"
}

function latest_snapshot(run: Run) -> SnapshotInfo? throws never {
    run.snapshots.at(run.snapshots.length() - 1)
}

// ───────────────────────────── persistence ─────────────────────────────

function ensure_dir(path: string) -> void throws never {
    baml.fs.mkdir(path, baml.fs.MkdirOptions { recursive: true }) catch_all (e) {
        _ => null
    };
}

/// Writes `meta.json`. Concurrent callers coalesce into one writer per run, so
/// the file always ends with the newest record.
function persist(app: App, run: Run) -> void throws never {
    let w = locked(app, () -> {
        let existing = app.writers.get(run.id);
        if (existing == null) {
            let created = MetaWriter { writing: false, dirty: false };
            app.writers.set(run.id, created);
            created
        } else {
            existing
        }
    });
    let writer = w;
    // One thread writes the file. A caller that finds a writer at work leaves a
    // mark, and the writer writes once more with the newest record.
    let mine = locked(app, () -> {
        if (writer.writing) {
            writer.dirty = true;
            false
        } else {
            writer.writing = true;
            writer.dirty = false;
            true
        }
    });
    if (!mine) {
        return;
    }
    let again = true;
    while (again) {
        let text = baml.json.stringify_pretty(run_json(run));
        baml.fs.write(`${run_dir(app, run.id)}/meta.json`, text) catch_all (e) {
            _ => {
                warn(`could not write meta.json of ${run.id}: ${e}`);
                0
            }
        };
        again
            = locked(app, () -> {
                if (writer.dirty) {
                    writer.dirty = false;
                    true
                } else {
                    writer.writing = false;
                    false
                }
            });
    }
}

function append_event(app: App, run_id: string, event: json) -> void throws never {
    let path = `${run_dir(app, run_id)}/events.jsonl`;
    let line = baml.json.stringify(event) + "\n";
    append_text(path, line) catch_all (e) {
        _ => {
            warn(`could not append to ${path}: ${e}`);
        }
    };
}

function append_text(
    path: string,
    text: string,
) -> void throws baml.errors.Io | baml.errors.InvalidArgument {
    let f = baml.fs.open(path, "a");
    defer {
        f.close() catch_all (e) {
            _ => null
        };
    }
    f.write(text);
}

/// Records a change of a run: bumps `updated_ts`, writes `meta.json`, and
/// sends the `run` event.
function touch(app: App, run: Run) -> void throws never {
    run.updated_ts = now_ms();
    broadcast(app, { "type": "run", "ts": run.updated_ts, "site": app.cfg.site, "run": run_json(run) });
    persist(app, run);
}

/// Sends a server-originated event about `run_id` to subscribers and to the
/// run's `events.jsonl`.
function emit_run_event(app: App, run_id: string, event: map<string, json>) -> void throws never {
    event.set("ts", now_ms());
    event.set("site", app.cfg.site);
    broadcast(app, event);
    append_event(app, run_id, event);
}

/// Sends a server-originated event that belongs to no run (for example
/// `program_fetched`) to the subscribers.
function emit_event(app: App, event: map<string, json>) -> void throws never {
    event.set("ts", now_ms());
    event.set("site", app.cfg.site);
    broadcast(app, event);
}

/// Loads every run from RUNS_DIR. A run that had a process when the server
/// stopped has none now: it becomes `paused` if it is durable and has a
/// snapshot, and `lost` otherwise.
function load_runs(app: App) -> void throws never {
    ensure_dir(app.cfg.runs_dir);
    let entries = baml.fs.read_dir(app.cfg.runs_dir) catch_all (e) {
        _ => {
            let none: baml.fs.DirEntry[] = [];
            none
        }
    };
    let loaded: Run[] = [];
    for (let entry in entries) {
        if (!entry.is_dir) {
            continue;
        }
        let meta_path = `${app.cfg.runs_dir}/${entry.name}/meta.json`;
        let text = baml.fs.read(meta_path) catch_all (e) {
            _ => ""
        };
        if (text.length() == 0) {
            continue;
        }
        let parsed = run_from_json(jparse(text)) catch_all (e) {
            _ => {
                warn(`ignoring unreadable ${meta_path}: ${e}`);
                null
            }
        };
        if (parsed == null) {
            continue;
        }
        let run: Run = parsed;
        if (is_live(run.status)) {
            run.pid = null;
            run.blocked = null;
            if (run.durable && run.snapshots.length() > 0) {
                // `wake_at` is set when the worker reports a suspend for a sleep
                // and cleared when the next segment starts. A live status with
                // `wake_at` means that the server stopped between that event and
                // the exit of the worker, so the latest snapshot is the suspend.
                if (run.wake_at != null) {
                    run.status = "sleeping";
                } else {
                    run.status = "paused";
                    reset_acks(run);
                }
            } else {
                run.status = "lost";
                run.error = "the site server restarted while the worker was running";
            }
            run.updated_ts = now_ms();
            loaded.push(run);
            persist(app, run);
        } else {
            loaded.push(run);
        }
    }
    app.runs
        = loaded.sort_by_key((r) -> {
            r.created_ts
        });
}

/// After a recovery from an older snapshot the worker state may predate a
/// delivered result, so every stored result is offered again.
function reset_acks(run: Run) -> void throws never {
    for (let r in run.remote_results) {
        r.acked = false;
    }
}

// ───────────────────────────── SSE fan-out ─────────────────────────────

function sse_frame(event: json) -> uint8array throws never {
    `data: ${baml.json.stringify(event)}\n\n`.to_utf8()
}

function enqueue(app: App, sub: Subscriber, frame: uint8array) -> void throws never {
    // The frame joins the queue and the drain thread is claimed in one step, so
    // frames keep their order and one thread writes to the response.
    let start = locked(app, () -> {
        if (sub.dead) {
            false
        } else {
            sub.queue.push(frame);
            if (sub.draining) {
                false
            } else {
                sub.draining = true;
                true
            }
        }
    });
    if (start) {
        spawn with baml.spawn.options(detach = true) { drain(app, sub) };
    }
}

function drain(app: App, sub: Subscriber) -> void throws never {
    while (true) {
        // Taking the next frame and giving up the drain role are one step, so a
        // frame that arrives at the end of the loop is never left behind.
        let frame = locked(app, () -> {
            if (sub.dead) {
                sub.queue.clear();
            }
            let next = sub.queue.shift();
            if (next == null) {
                sub.draining = false;
            }
            next
        });
        if (frame == null) {
            return;
        }
        let bytes: uint8array = frame;
        sub.res.write(bytes) catch_all (e) {
            _ => {
                sub.dead = true;
                null
            }
        };
    }
}

function broadcast(app: App, event: json) -> void throws never {
    let frame = sse_frame(event);
    broadcast_frame(app, frame);
}

function broadcast_frame(app: App, frame: uint8array) -> void throws never {
    // The list is replaced, never changed in place, so this snapshot is stable.
    let subs = locked(app, () -> {
        let any_dead = app.subs.some((s) -> {
            s.dead
        });
        if (any_dead) {
            app.subs
                = app.subs.filter((s) -> {
                    !s.dead
                });
        }
        app.subs
    });
    for (let sub in subs) {
        enqueue(app, sub, frame);
    }
}

/// Registers a new SSE connection. Its first message is `init`.
function subscribe(app: App) -> baml.http.Response throws never {
    let res = baml.http.Response.new_streaming(
        200,
        {
            "content-type": "text/event-stream",
            "cache-control": "no-cache",
            "x-accel-buffering": "no",
            "access-control-allow-origin": "*",
        },
    );
    let sub = Subscriber { res: res, queue: [], draining: false, dead: false };
    // `init` is built and queued, and the subscriber is registered, in one step
    // under the lock. Every other frame is queued under the lock too. `init` is
    // therefore the first frame of the connection, and a change of a run is
    // either part of `init` or reaches the client as a `run` event.
    let ts = now_ms();
    locked(app, () -> {
        let init: map<string, json> = { "type": "init", "ts": ts, "site": app.cfg.site, "runs": runs_json(app.runs.reverse()) };
        sub.queue.push(sse_frame(init));
        sub.draining = true;
        app.subs = app.subs.concat([sub]);
        null
    });
    spawn with baml.spawn.options(detach = true) { drain(app, sub) };
    res
}
