// Site server of the durable functions proof of concept.
//
//   SITE=local baml run main
//
// The server never executes user code. It starts one worker process per run
// segment, relays the worker's JSON events to SSE subscribers, keeps the run
// store, places remote calls on a site of the remote pool, and sends migrating
// snapshots to the named site (contract section 8).

function main() -> void {
    //# Boot: configuration, run store, function list
    let cfg = load_config();
    let app = App {
        cfg: cfg,
        runs: [],
        procs: {},
        subs: [],
        writers: {},
        dispatching: {},
        forwarding: {},
        importing: {},
        functions: scan_functions(cfg.program_dir),
        timers: {},
        timer_seq: 0,
        placed: 0,
        programs: {},
        fetching: {},
        holding_result: false,
        results_sent: 0,
        discarded: {},
        cancel_requested: {},
        worker_build: "",
        lock: baml.spawn.TaskGroup.new(1, name = "state"),
    };
    load_runs(app);
    let server = baml.http.Server.bind(`127.0.0.1:${cfg.port}`);
    baml.io.eprintln(`site ${cfg.site} listening on http://${server.addr}`);
    let registry = cfg.sites.map((s) -> {
        `${s.name}=${s.url}`
    });
    baml.io.eprintln(`  sites ${registry.join(" ")}`);
    let eligible = eligible_sites(cfg);
    let placement = if (eligible.length() == 0) {
        "no site"
    } else {
        eligible.join(", ") + " in round-robin order"
    };
    baml.io.eprintln(
        `  remote pool ${cfg.remote_pool.join(", ")} (remote calls of this site go to ${placement})`,
    );
    baml.io.eprintln(`  worker ${cfg.worker_cmd.join(" ")} (cwd ${cfg.worker_cwd})`);
    baml.io.eprintln(`  program ${cfg.program_dir}`);
    baml.io.eprintln(`  runs ${cfg.runs_dir} (${app.runs.length()} loaded)`);
    baml.io.eprintln(`  programs ${cfg.programs_dir}`);
    if (chaos_active(cfg.chaos)) {
        baml.io.eprintln(`  CHAOS ${baml.json.stringify(j(cfg.chaos))}`);
    }
    ensure_dir(cfg.programs_dir);
    //# Child runs that ended while their parent's site was unreachable
    for (let run in app.runs) {
        finish_child(app, run);
    }
    //# Results for forks that migrated, whose forward did not succeed before the restart
    resume_forwards(app);
    //# Children of runs that ended without a live server to cancel them
    for (let run in app.runs) {
        cancel_children(app, run, `the parent run ${run.id} ended as ${run.status}`);
    }
    //# Wake timers of sleeping runs. An overdue run resumes at once
    rebuild_timers(app);
    //# SSE keepalive every 10 seconds
    spawn with baml.spawn.options(detach = true) {
        let frame = ": keepalive\n\n".to_utf8();
        while (true) {
            baml.sys.sleep(baml.time.Duration.from_seconds(10n)) catch_all (e) {
                _ => null
            };
            broadcast_frame(app, frame);
        }
    };
    //# Serve HTTP until the process is stopped
    server.serve((req) -> {
        handle(app, req) catch_all (e, ctx) {
            _ => {
                warn(`request failed: ${ctx.to_string()}`);
                fail(500, ctx.to_string())
            }
        }
    })
}
