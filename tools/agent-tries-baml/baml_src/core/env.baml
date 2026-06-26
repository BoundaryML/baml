// The env contract for the whole fleet.
//
// Prod (Infisical) injects SECRETS under an `ATB_` prefix; plain config lives in
// the fly [env] blocks unprefixed. Local docker-compose maps unprefixed .env vars
// to ATB_*. A single resolver — try `ATB_<NAME>`, then `<NAME>` — handles every
// case (secret via ATB_, plain config via the unprefixed fallback, and the
// `ATB_GITHUB_TOKEN ?? GITHUB_TOKEN` dual-source pattern). Mirrors the Python
// os.environ.get("ATB_<NAME>") reads in libs/bench_core + services/*.

// ---- resolvers -------------------------------------------------------------

// Raw lookup that swallows the (rare) Io/ParseError into null.
function env_raw(name: string) -> string? {
    baml.env.get(name) catch (_) {
        _ => null
    }
}

// ATB_<name> first, then <name>. Null if neither is set.
function env_opt(name: string) -> string? {
    let prefixed = env_raw("ATB_" + name);
    match (prefixed) {
        let v: string => v,
        null => env_raw(name),
    }
}

function env_or(name: string, fallback: string) -> string {
    match (env_opt(name)) {
        let v: string => v,
        null => fallback,
    }
}

function env_int(name: string, fallback: int) -> int {
    match (env_opt(name)) {
        let v: string => {
            baml.json.from_string<int>(v) catch (_) {
                _ => fallback
            }
        },
        null => fallback,
    }
}

function env_bool(name: string, fallback: bool) -> bool {
    match (env_opt(name)) {
        let v: string => {
            let lo = v.trim().to_lower_case();
            lo == "1" || lo == "true" || lo == "yes"
        },
        null => fallback,
    }
}

// Comma-separated list (e.g. CLAUDE_PROXY_URLS). Empty -> [].
function env_list(name: string) -> string[] {
    match (env_opt(name)) {
        let v: string => {
            let out: string[] = [];
            for (let part in v.split(",")) {
                let t = part.trim();
                if (t.length() > 0) {
                    out.push(t);
                }
            }
            out
        },
        null => [],
    }
}

// ---- core auth / service wiring -------------------------------------------

function service_url() -> string {
    return env_or("SERVICE_URL", "http://api:8080");
}

function service_token() -> string {
    return env_or("SERVICE_TOKEN", "devservicetoken");
}

function proxy_urls() -> string[] {
    let urls = env_list("CLAUDE_PROXY_URLS");
    match (urls.length()) {
        0 => ["http://claude-proxy:9090"],
        _ => urls,
    }
}

function proxy_token() -> string {
    return env_or("CLAUDE_PROXY_TOKEN", "devproxytoken");
}

function anthropic_api_key() -> string {
    return env_or("ANTHROPIC_API_KEY", "");
}

// ---- convex (only the api server uses these) ------------------------------

function convex_url() -> string {
    return env_or("CONVEX_URL", "http://convex:3210");
}

// Promo codes live in their OWN Convex project/deployment (separate inventory DB).
// Defaults to the main Convex when unset (back-compat).
function promo_convex_url() -> string {
    return env_or("PROMO_CONVEX_URL", convex_url());
}

function convex_admin_key() -> string? {
    return env_opt("CONVEX_ADMIN_KEY");
}

// ---- model defaults (overridable per service via env) ---------------------

function worker_model() -> string {
    return env_or("CLAUDE_MODEL", "claude-opus-4-8");
}

function worker_max_turns() -> int {
    return env_int("CLAUDE_MAX_TURNS", 40);
}

function dedup_model() -> string {
    return env_or("BAML_DEDUP_MODEL", "claude-sonnet-4-6");
}

function redraft_model() -> string {
    return env_or("BAML_REDRAFT_MODEL", "claude-sonnet-4-6");
}

function arena_model() -> string {
    return env_or("ARENA_MODEL", "claude-sonnet-4-6");
}

function changelog_model() -> string {
    return env_or("CHANGELOG_MODEL", "claude-opus-4-8");
}

function bug_verify_model() -> string {
    return env_or("BUG_VERIFY_MODEL", "claude-sonnet-4-6");
}

function intent_model() -> string {
    return env_or("BAMMY_INTENT_MODEL", "claude-haiku-4-5");
}

function hogql_model() -> string {
    return env_or("BAMMY_HOGQL_MODEL", "claude-sonnet-4-6");
}

function gen_task_model() -> string {
    return env_or("GEN_TASK_MODEL", "claude-sonnet-4-6");
}

// ---- claude-proxy ----------------------------------------------------------

function staging_root() -> string {
    return env_or("STAGING_ROOT", "/tmp/staging");
}

// On-volume transcript/blob store (the api server writes + serves these).
function blob_dir() -> string {
    return env_or("BLOB_DIR", "/data/blobs");
}

function baml_cache_dir() -> string {
    return env_or("BAML_CACHE_DIR", "/var/baml-cache");
}

function claude_bin() -> string {
    return env_or("CLAUDE_BIN", "claude");
}

function proxy_listen_port() -> string {
    return env_or("PROXY_LISTEN_PORT", "9090");
}

function invocation_timeout_secs() -> int {
    return env_int("CLAUDE_INVOCATION_TIMEOUT_SECS", 1800);
}

function worker_agent_timeout_secs() -> int {
    return env_int("WORKER_AGENT_TIMEOUT_SECS", 3600);
}

function quickstart_url() -> string {
    return env_or("QUICKSTART_URL", "https://new.boundaryml.com/quickstart");
}

function dedup_batch_size() -> int {
    return env_int("BATCH_SIZE", 20);
}

// ---- integrations / processor config --------------------------------------

function linear_token() -> string {
    return env_or("LINEAR_TOKEN", "");
}

function linear_team_id() -> string {
    return env_or("LINEAR_TEAM_ID", "179250f3-902f-491e-94f8-8256292e8386");
}

function github_token() -> string {
    return env_or("GITHUB_TOKEN", "");
}

function cursor_api_key() -> string {
    return env_or("CURSOR_API_KEY", "");
}

function cursor_branch() -> string {
    return env_or("CURSOR_BRANCH", "canary");
}

function cursor_repo_url() -> string {
    return env_or("CURSOR_REPO_URL", "https://github.com/BoundaryML/baml");
}

// ---- cursor-tracker (PR-monitor sweep) ------------------------------------

function cursor_model() -> string {
    return env_or("CURSOR_MODEL", "");
}

// Seconds between tracker sweeps.
function cursor_track_secs() -> int {
    return env_int("CURSOR_TRACK_SECS", 60);
}

// Max issues swept per status per tick.
function cursor_track_limit() -> int {
    return env_int("CURSOR_TRACK_LIMIT", 100);
}

// Bounded refix attempts before escalating an issue to needs_human.
function cursor_max_fix_attempts() -> int {
    return env_int("CURSOR_MAX_FIX_ATTEMPTS", 3);
}

// Channel for the tracker's per-issue fix-saga Slack thread ("" = no notify).
function slack_fix_channel() -> string {
    return env_or("SLACK_FIX_CHANNEL", "");
}

function changelog_max_attempts() -> int {
    return env_int("CHANGELOG_MAX_ATTEMPTS", 3);
}

function bug_verify_max_turns() -> int {
    return env_int("BUG_VERIFY_MAX_TURNS", 16);
}

function bug_verify_timeout_secs() -> int {
    return env_int("BUG_VERIFY_TIMEOUT_SECS", 900);
}

function cron_interval_secs() -> int {
    return env_int("CRON_INTERVAL_SECS", 86400);
}

function gen_task_count() -> int {
    return env_int("GEN_TASK_COUNT", 3);
}

function slack_bot_token() -> string {
    return env_or("SLACK_BOT_TOKEN", "");
}

function ui_base_url() -> string {
    return env_or("UI_BASE_URL", "https://new.boundaryml.com/atb");
}

// Containment for LinearPush: when non-empty, ONLY these issue ids are ever
// mirrored to a Linear card (every other dirty issue is drained without touching
// the board). Empty => unrestricted (full production mirroring). A flood guard so
// turning the Linear integration on can't spray the shared board with cards.
function linear_push_allowlist() -> string[] {
    return env_list("LINEAR_PUSH_ALLOWLIST");
}

function changelog_allowlist() -> string[] {
    return env_list("SLACK_CHANGELOG_ALLOWLIST");
}

// ---- baml version management ----------------------------------------------

function baml_repo_slug() -> string {
    return env_or("BAML_REPO_SLUG", "BoundaryML/baml");
}

function baml_ref() -> string {
    return env_or("BAML_REF", "canary");
}

function baml_channel() -> string {
    return env_or("BAML_CHANNEL", "nightly");
}
