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

/// Placement of a remote call (contract section 8.2): the first site of
/// REMOTE_POOL that is not this site, or null when the pool has no other site.
function pick_remote_site(cfg: Config) -> string? throws never {
    for (let name in cfg.remote_pool) {
        if (name != cfg.site) {
            return name;
        }
    }
    null
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
    let w = app.writers.get(run.id);
    if (w == null) {
        let created = MetaWriter { writing: false, dirty: false };
        app.writers.set(run.id, created);
        w = created;
    }
    let writer: MetaWriter = w;
    if (writer.writing) {
        writer.dirty = true;
        return;
    }
    writer.writing = true;
    let again = true;
    while (again) {
        writer.dirty = false;
        let text = baml.json.stringify_pretty(run_json(run));
        baml.fs.write(`${run_dir(app, run.id)}/meta.json`, text) catch_all (e) {
            _ => {
                warn(`could not write meta.json of ${run.id}: ${e}`);
                0
            }
        };
        again = writer.dirty;
    }
    writer.writing = false;
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
                run.status = "paused";
                reset_acks(run);
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

function enqueue(sub: Subscriber, frame: uint8array) -> void throws never {
    if (sub.dead) {
        return;
    }
    sub.queue.push(frame);
    if (!sub.draining) {
        sub.draining = true;
        spawn with baml.spawn.options(detach = true) { drain(sub) };
    }
}

function drain(sub: Subscriber) -> void throws never {
    while (sub.queue.length() > 0 && !sub.dead) {
        let frame = sub.queue.shift();
        if (frame == null) {
            break;
        }
        let bytes: uint8array = frame;
        sub.res.write(bytes) catch_all (e) {
            _ => {
                sub.dead = true;
                null
            }
        };
    }
    if (sub.dead) {
        sub.queue.clear();
    }
    sub.draining = false;
}

function broadcast(app: App, event: json) -> void throws never {
    let frame = sse_frame(event);
    broadcast_frame(app, frame);
}

function broadcast_frame(app: App, frame: uint8array) -> void throws never {
    let any_dead = false;
    for (let sub in app.subs) {
        if (sub.dead) {
            any_dead = true;
        } else {
            enqueue(sub, frame);
        }
    }
    if (any_dead) {
        app.subs
            = app.subs.filter((s) -> {
                !s.dead
            });
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
    // No sys-op runs between building `init` and registering the subscriber, so
    // the client misses no event and sees none twice.
    let init: map<string, json> = { "type": "init", "ts": now_ms(), "site": app.cfg.site, "runs": runs_json(app.runs.reverse()) };
    app.subs.push(sub);
    enqueue(sub, sse_frame(init));
    res
}
