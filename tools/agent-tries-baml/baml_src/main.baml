// Per-role entrypoints + supervisor. `baml run <Role>` runs one role as a
// process; `baml run Supervisor` spawns several in one process (mirrors
// services/supervisor.py). Api lives in servers/api.baml, Proxy in
// servers/proxy.baml.

// Build a ServiceClient from env.
function svc_from_env() -> ServiceClient {
    return ServiceClient { base_url: service_url(), token: service_token() };
}

function proxy_from_env() -> ProxyClient {
    return ProxyClient { urls: proxy_urls(), token: proxy_token() };
}

// ---- baml_worker -----------------------------------------------------------
function RunWorker() -> int {
    let p = WorkerProcessor {
        wid: "baml_worker-" + baml.id.new(),
        svc: svc_from_env(),
        proxy: proxy_from_env(),
        model: worker_model(),
        max_turns: worker_max_turns(),
        timeout_secs: worker_agent_timeout_secs(),
        quickstart_url: quickstart_url(),
    };
    let pp: Processor = p;
    baml.io.println("baml_worker " + p.wid + " starting");
    return pp.run();
}

// ---- baml_dedup ------------------------------------------------------------
function RunDedup() -> int {
    let p = DedupProcessor {
        wid: "baml_dedup-" + baml.id.new(),
        svc: svc_from_env(),
        batch_size: dedup_batch_size(),
    };
    let pp: Processor = p;
    baml.io.println("baml_dedup " + p.wid + " starting");
    return pp.run();
}

function linear_from_env() -> LinearClient {
    return LinearClient { token: linear_token(), team_id: linear_team_id() };
}

function RunRedraft() -> int {
    let p = RedraftProcessor {
        wid: "baml_redraft-" + baml.id.new(),
        svc: svc_from_env(),
        linear: linear_from_env(),
    };
    let pp: Processor = p;
    return pp.run();
}

function RunCohortCompare() -> int {
    let p = CohortCompareProcessor { wid: "cohort_compare-" + baml.id.new(), svc: svc_from_env() };
    let pp: Processor = p;
    return pp.run();
}

function RunChangelogWorker() -> int {
    let p = ChangelogWorkerProcessor {
        wid: "changelog_worker-" + baml.id.new(),
        svc: svc_from_env(),
        github_token: github_token(),
        repo_slug: baml_repo_slug(),
        max_attempts: changelog_max_attempts(),
    };
    let pp: Processor = p;
    return pp.run();
}

function RunBugVerify() -> int {
    let p = BugVerifyProcessor {
        wid: "bug_verify-" + baml.id.new(),
        svc: svc_from_env(),
        proxy: proxy_from_env(),
        model: bug_verify_model(),
        max_turns: bug_verify_max_turns(),
        timeout_secs: bug_verify_timeout_secs(),
    };
    let pp: Processor = p;
    return pp.run();
}

function RunBuilder() -> int {
    let p = BamlBuilderProcessor {
        wid: "baml_builder-" + baml.id.new(),
        svc: svc_from_env(),
        github_token: github_token(),
        repo_slug: baml_repo_slug(),
    };
    let pp: Processor = p;
    return pp.run();
}

function RunLinearPush() -> int {
    let p = LinearPushProcessor {
        wid: "linear_push-" + baml.id.new(),
        svc: svc_from_env(),
        linear: linear_from_env(),
    };
    let pp: Processor = p;
    return pp.run();
}

function RunFixDispatch() -> int {
    let p = FixDispatchProcessor {
        wid: "fix_dispatch-" + baml.id.new(),
        svc: svc_from_env(),
        cursor_key: cursor_api_key(),
        cursor_branch: cursor_branch(),
        repo_url: cursor_repo_url(),
    };
    let pp: Processor = p;
    return pp.run();
}

// linear_sync role runs BOTH its processors concurrently.
function RunLinearSync() -> int {
    let _push: Processor = LinearPushProcessor {
        wid: "linear_push-" + baml.id.new(),
        svc: svc_from_env(),
        linear: linear_from_env(),
    };
    let _fix: Processor = FixDispatchProcessor {
        wid: "fix_dispatch-" + baml.id.new(),
        svc: svc_from_env(),
        cursor_key: cursor_api_key(),
        cursor_branch: cursor_branch(),
        repo_url: cursor_repo_url(),
    };
    // The cursor PR tracker: a periodic sweep (not a claim loop) that follows each
    // dispatched fix agent through to a mergeable PR. Runs alongside the two claim
    // loops, mirroring the v1 linear_sync gather(LinearPush, FixDispatch, Tracker).
    let _ctx = TrackCtx {
        svc: svc_from_env(),
        linear: linear_from_env(),
        cursor_key: cursor_api_key(),
        github_token: github_token(),
        model: cursor_model(),
        slack_token: slack_bot_token(),
        fix_channel: slack_fix_channel(),
        max_attempts: cursor_max_fix_attempts(),
    };
    let _ = spawn { _push.run() };
    let _ = spawn { _fix.run() };
    let _ = spawn { run_cursor_tracker(_ctx, cursor_track_secs() * 1000, cursor_track_limit()) };
    while (true) {
        let _ = baml.sys.sleep(baml.time.Duration.from_milliseconds(3600000)) catch (_) {
            _ => null
        };
    }
    return 0;
}

function RunCron() -> int {
    baml.io.println("cron starting");
    return run_cron(svc_from_env(), cron_interval_secs() * 1000, gen_task_count(), true);
}

// ---- supervisor ------------------------------------------------------------
// Spawn a set of roles in one process. SUPERVISE is a comma list (name[:N]).
// Background spawns must swallow their own errors (BEP-034); each role's run()
// loops forever and already catches internally.
function Supervisor() -> int throws baml.errors.Io {
    let roles = env_list("SUPERVISE");
    let effective = match (roles.length()) {
        0 => ["web", "baml_worker", "baml_dedup"],
        _ => roles,
    };
    for (let spec in effective) {
        let name = role_name(spec);
        let n = role_count(spec);
        let i = 0;
        while (i < n) {
            spawn_role(name);
            i = i + 1;
        }
    }
    // Keep the supervisor process alive while the spawned roles run.
    baml.io
        .println(
            "supervisor: spawned " + baml.unstable.string(effective.length()) + " role spec(s)",
        );
    while (true) {
        let _ = baml.sys.sleep(baml.time.Duration.from_milliseconds(3600000)) catch (_) {
            _ => null
        };
    }
    return 0;
}

function spawn_role(name: string) -> int {
    if (name == "web" || name == "api") {
        let _ = spawn { Api() };
    } else if (name == "proxy") {
        let _ = spawn { Proxy() };
    } else if (name == "ingress") {
        let _ = spawn { Ingress() };
    } else if (name == "baml_worker") {
        let _ = spawn { RunWorker() };
    } else if (name == "baml_dedup") {
        let _ = spawn { RunDedup() };
    } else if (name == "baml_redraft") {
        let _ = spawn { RunRedraft() };
    } else if (name == "cohort_compare") {
        let _ = spawn { RunCohortCompare() };
    } else if (name == "changelog_worker") {
        let _ = spawn { RunChangelogWorker() };
    } else if (name == "bug_verify") {
        let _ = spawn { RunBugVerify() };
    } else if (name == "baml_builder") {
        let _ = spawn { RunBuilder() };
    } else if (name == "linear_sync") {
        let _ = spawn { RunLinearSync() };
    } else if (name == "cron") {
        let _ = spawn { RunCron() };
    } else {
        baml.io.eprintln("supervisor: unknown role '" + name + "'");
    }
    return 0;
}

// Parse "name:N" -> name.
function role_name(spec: string) -> string {
    return match (spec.split(":").at(0)) {
        let s: string => s,
        null => spec,
    };
}

// Parse "name:N" -> N (default 1).
function role_count(spec: string) -> int {
    let parts = spec.split(":");
    if (parts.length() < 2) {
        return 1;
    }
    return match (parts.at(1)) {
        let s: string => {
            baml.json.from_string<int>(s) catch (_) {
                _ => 1
            }
        },
        null => 1,
    };
}
