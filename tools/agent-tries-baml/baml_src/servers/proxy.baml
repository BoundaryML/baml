// claude-proxy — http.Server wrapper around the Claude Code CLI. Ports
// services/claude_proxy/app.py. Routes:
//   GET  /healthz     liveness
//   POST /run-agent   stage files -> sys.exec claude -> parse session -> AgentResult
//   POST /check-baml  stage files -> run a baml command -> CheckBamlResult
//
// The handler is `(Request) -> Response throws never`, so every branch catches
// its own errors and returns a Response. Subprocesses go through core/exec.baml's
// run_capture (grandchild-safe, timeout-bounded).
//
// TODO(binary-cache): _ensure_baml (download the version-pinned baml CLI from the
// service into BAML_CACHE_DIR/<sha> and prepend to PATH) + `baml agent install`
// are deferred; for now baml resolves from the image PATH (baml_bin_dir = null).

// ---- HTTP helpers ----------------------------------------------------------

function path_only(url: string) -> string {
    return match (url.split("?").at(0)) {
        let p: string => p,
        null => "",
    };
}

function json_response(status: int, body: string) -> baml.http.Response {
    return baml.http.Response.new(status, { "Content-Type": "application/json" }, body.to_utf8());
}

function text_response(status: int, body: string) -> baml.http.Response {
    return baml.http.Response.new(status, { "Content-Type": "text/plain" }, body.to_utf8());
}

// Bearer check. Empty configured token => open (local dev).
function bearer_ok(req: baml.http.Request, token: string) -> bool {
    if (token == "") {
        return true;
    }
    let got = match (req.headers.get("authorization")) {
        let v: string => v,
        null => match (req.headers.get("Authorization")) {
            let v2: string => v2,
            null => "",
        },
    };
    return got == "Bearer " + token;
}

// ---- file staging ----------------------------------------------------------

// Reject empty/absolute/".."-bearing relative paths (path traversal guard).
function safe_rel(rel: string) -> bool {
    if (rel == "" || rel.starts_with("/")) {
        return false;
    }
    for (let part in rel.split("/")) {
        if (part == "..") {
            return false;
        }
    }
    return true;
}

// Write each file under `staging`, creating parent dirs. Returns false on any
// unsafe path or write error (best-effort; caller decides).
function stage_files(staging: string, files: map<string, string>) -> bool {
    let ok = baml.fs.mkdir(staging, baml.fs.MkdirOptions { recursive: true }) catch (_) {
        _ => null
    };
    let good = true;
    for (let rel in files.keys()) {
        if (safe_rel(rel) == false) {
            good = false;
        } else {
            let abs = staging + "/" + rel;
            let parent = parent_dir(abs);
            let _ = baml.fs.mkdir(parent, baml.fs.MkdirOptions { recursive: true }) catch (_) {
                _ => null
            };
            let content = match (files.get(rel)) {
                let c: string => c,
                null => "",
            };
            let _ = baml.fs.write(abs, content) catch (_) {
                _ => 0
            };
        }
    }
    return good;
}

function parent_dir(path: string) -> string {
    let parts = path.split("/");
    let n = parts.length();
    if (n <= 1) {
        return ".";
    }
    let out: string[] = [];
    let i = 0;
    for (let p in parts) {
        if (i < n - 1) {
            out.push(p);
        }
        i = i + 1;
    }
    return out.join("/");
}

// ---- post-file collection --------------------------------------------------
// Walk the staging tree and collect agent-authored artifacts (trophy.json + the
// project's .baml/.md files), skipping installed skills (.claude/.agents) and our
// own capture files. Minimal port of collect_post_files (no size caps/globs yet).

function collect_post_files(staging: string) -> map<string, string> {
    return collect_dir(staging, "");
}

function collect_dir(abs_dir: string, rel_prefix: string) -> map<string, string> {
    let out: map<string, string> = {};
    let entries: baml.fs.DirEntry[] = baml.fs.read_dir(abs_dir) catch (_) {
        _ => []
    };
    for (let e in entries) {
        if (e.name == ".claude" || e.name == ".agents" || e.name.starts_with(".atb_")) {
            // skip installed skills + our capture files
        } else if (e.is_dir) {
            let sub = collect_dir(abs_dir + "/" + e.name, rel_prefix + e.name + "/");
            for (let k in sub.keys()) {
                match (sub.get(k)) {
                    let v: string => out.set(k, v),
                    null => null,
                };
            }
        } else if (e.is_file && should_collect(e.name)) {
            let content = baml.fs.read(abs_dir + "/" + e.name) catch (_) {
                _ => ""
            };
            out.set(rel_prefix + e.name, content);
        }
    }
    return out;
}

function should_collect(name: string) -> bool {
    // SKILL.md is injected (onboarding input), not an agent-authored artifact.
    if (name == "SKILL.md") {
        return false;
    }
    return name == "trophy.json" || name.ends_with(".baml") || name.ends_with(".md");
}

// ---- /check-baml -----------------------------------------------------------

function handle_check_baml(req: baml.http.Request) -> baml.http.Response {
    let parsed = baml.json.from_string<CheckBamlRequest>(req.body) catch (_) {
        _ => null
    };
    return match (parsed) {
        null => json_response(400, "{\"error\":\"bad check-baml request\"}"),
        let cr: CheckBamlRequest => run_check(cr),
    };
}

function run_check(cr: CheckBamlRequest) -> baml.http.Response {
    let staging = staging_root() + "/check-" + cr.check_id;
    let _ = stage_files(staging, cr.files);

    // Provision a minimal project so `baml` exercises real compile behavior, not
    // the project gate — unless the repro ships its own baml.toml.
    if (cr.files.has("baml.toml") == false) {
        let _ = baml.fs.write(staging + "/baml.toml", "[package]\nname = \"repro\"\n") catch (_) {
            _ => 0
        };
    }

    let res = run_capture("sh", ["-c", cr.command], staging, null, cr.timeout_secs * 1000, null, false);
    let result = CheckBamlResult {
        check_id: cr.check_id,
        exit_code: res.exit_code,
        timed_out: res.timed_out,
        baml_on_path: false,
        stdout_tail: tail(res.stdout, 4000),
        stderr_tail: tail(res.stderr, 4000),
    };
    let body = baml.json.to_string(result) catch (_) {
        _ => "{\"check_id\":\"" + cr.check_id + "\",\"exit_code\":-1}"
    };
    return json_response(200, body);
}

// ---- /run-agent ------------------------------------------------------------

function handle_run_agent(req: baml.http.Request) -> baml.http.Response {
    let parsed = baml.json.from_string<RunAgentRequest>(req.body) catch (_) {
        _ => null
    };
    return match (parsed) {
        null => json_response(400, "{\"error\":\"bad run-agent request\"}"),
        let rr: RunAgentRequest => run_agent(rr),
    };
}

function run_agent(rr: RunAgentRequest) -> baml.http.Response {
    let staging = staging_root() + "/" + rr.cell_id;
    let _ = stage_files(staging, rr.files);

    // claude -p - --output-format json --max-turns N --model M
    //   --permission-mode bypassPermissions [--append-system-prompt SP]
    let base = [
        "-p",
        "-",
        "--output-format",
        "json",
        "--max-turns",
        baml.unstable.string(rr.max_turns),
        "--model",
        rr.model,
        "--permission-mode",
        "bypassPermissions",
    ];
    let args = match (rr.system_prompt) {
        let sp: string => {
            if (sp.trim() == "") {
                base
            } else {
                base.concat(["--append-system-prompt", sp])
            }
        },
        null => base,
    };

    let timeout_ms = match (rr.invocation_timeout_secs) {
        let t: int => t * 1000,
        null => invocation_timeout_secs() * 1000,
    };

    // OAuth mode: scrub ANTHROPIC_API_KEY so the CLI uses its persisted login.
    let scrub = anthropic_api_key() == "";
    let res = run_capture(claude_bin(), args, staging, null, timeout_ms, rr.prompt, scrub);

    let session = parse_claude_session(res.stdout);
    // The real transcript: read claude's session jsonl and render it terminal-style
    // (falls back to the raw stdout summary when the session file isn't found).
    let transcript = render_session(staging, session.session_id, res.stdout);
    let status = if (res.timed_out) {
        "timeout"
    } else if (res.exit_code == 0) {
        "ok"
    } else {
        "error"
    };

    let result = AgentResult {
        cell_id: rr.cell_id,
        status: status,
        exit_code: res.exit_code,
        transcript: transcript,
        stderr_tail: tail(res.stderr, 4000),
        turns: session.turns,
        tool_calls: session.tool_calls,
        api_calls: null,
        input_tokens: session.input_tokens,
        output_tokens: session.output_tokens,
        total_tokens: session.total_tokens,
        cache_read_tokens: session.cache_read_tokens,
        cache_write_tokens: session.cache_write_tokens,
        wall_clock_ms: null,
        estimated_cost_usd: cost_of(rr, session),
        turn_log: null,
        post_files: collect_post_files(staging),
        skill_installed: null,
    };
    let body = baml.json.to_string(result) catch (_) {
        _ => "{\"cell_id\":\"" + rr.cell_id + "\",\"status\":\"error\"}"
    };
    return json_response(200, body);
}

function cost_of(rr: RunAgentRequest, session: ClaudeSession) -> float? {
    return match (rr.prices) {
        let p: Prices => {
            compute_cost(
                or_zero(session.input_tokens),
                or_zero(session.output_tokens),
                or_zero(session.cache_read_tokens),
                or_zero(session.cache_write_tokens),
                p,
            )
        },
        null => null,
    };
}

function or_zero(v: int?) -> int {
    return match (v) {
        let n: int => n,
        null => 0,
    };
}

function tail(s: string, n: int) -> string {
    let len = s.length();
    if (len <= n) {
        return s;
    }
    return s.substring(len - n, len);
}

// ---- /v1/messages (LLM gateway) --------------------------------------------
// BAML's anthropic clients POST here (base_url -> proxy). The proxy is the single
// LLM gateway: it fulfills the Messages request via the `claude` CLI on its
// SUBSCRIPTION (OAuth), exactly like the old pipeline — no metered Anthropic API
// key needed in any service. It extracts the prompt from the messages, runs
// `claude -p`, and wraps the result back into a Messages response BAML can parse.
//
// LLM_FORWARD=1 instead forwards verbatim to api.anthropic.com (metered API),
// using the proxy's ANTHROPIC_API_KEY — handy when a real API key is available.

class MsgBlock {
    type: string,
    text: string?,
}

class ChatMsg {
    role: string,
    content: MsgBlock[],
}

class MessagesReq {
    model: string,
    system: string?,
    messages: ChatMsg[],
}

class ClaudeOut {
    result: string?,
}

function handle_messages(req: baml.http.Request) -> baml.http.Response {
    // Authorize via the x-api-key the client sent (the proxy token), when set.
    let tok = proxy_token();
    if (tok != "") {
        let got = match (req.headers.get("x-api-key")) {
            let v: string => v,
            null => "",
        };
        if (got != tok) {
            return json_response(401, "{\"error\":\"bad proxy token\"}");
        }
    }
    if (env_bool("LLM_FORWARD", false)) {
        return forward_to_anthropic(req);
    }
    return claude_messages(req);
}

// Forward verbatim to the metered Anthropic API (LLM_FORWARD=1).
function forward_to_anthropic(req: baml.http.Request) -> baml.http.Response {
    let fwd = baml.http.Request {
        method: "POST",
        url: "https://api.anthropic.com/v1/messages",
        headers: {
            "x-api-key": anthropic_api_key(),
            "anthropic-version": "2023-06-01",
            "content-type": "application/json",
        },
        body: req.body,
    };
    let resp = baml.http.send(fwd) catch (_) {
        _ => baml.http.Response.new(502, {}, "{\"error\":\"anthropic forward failed\"}".to_utf8())
    };
    let text = resp.text() catch (_) {
        _ => "{\"error\":\"anthropic read failed\"}"
    };
    return json_response(resp.status_code, text);
}

// Fulfill via the claude CLI subscription, then wrap as a Messages response.
function claude_messages(req: baml.http.Request) -> baml.http.Response {
    let parsed = baml.json.from_string<MessagesReq>(req.body) catch (_) {
        _ => null
    };
    return match (parsed) {
        null => {
            baml.io.println("proxy-msg: MessagesReq PARSE FAILED -> 400");
            json_response(400, "{\"error\":\"bad messages request\"}")
        },
        let mr: MessagesReq => {
            let prompt = render_messages(mr);
            let staging = staging_root() + "/msg-" + baml.id.new();
            let _ = baml.fs.mkdir(staging, baml.fs.MkdirOptions { recursive: true }) catch (_) {
                _ => null
            };
            // Single-shot text answer: `--tools ""` disables ALL tools so claude
            // can't reach for one (a tool_use first turn blows --max-turns 1 ->
            // error_max_turns -> garbage). OAuth (scrub key), schema-bearing prompt
            // on stdin. No --permission-mode (bypassPermissions is rejected as root).
            let args = ["-p", "-", "--output-format", "json", "--model", mr.model, "--max-turns", "1", "--tools", ""];
            let res = run_capture(claude_bin(), args, staging, null, 300000, prompt, anthropic_api_key() == "");
            // Surface a failed claude run (non-zero exit or an error result like
            // error_max_turns) instead of silently wrapping garbage back to BAML.
            if (res.exit_code != 0 || res.stdout.includes("\"is_error\":true")) {
                let head = res.stdout.substring(0, 200) catch (_) {
                    _ => res.stdout
                };
                baml.io
                    .println(
                        "proxy-claude FAILED: exit="
                            + baml.unstable.string(res.exit_code)
                            + " head="
                            + head,
                    );
            }
            let line = find_last_json_line(res.stdout);
            let out = baml.json.from_string<ClaudeOut>(line) catch (_) {
                _ => ClaudeOut { result: null }
            };
            let answer = match (out.result) {
                let r: string => r,
                null => res.stdout,
            };
            return json_response(200, messages_response_json(mr.model, answer));
        },
    };
}

// Concatenate the text blocks across messages (role-prefixed when more than one).
function render_messages(mr: MessagesReq) -> string {
    let system = match (mr.system) {
        let s: string => s + "\n\n",
        null => "",
    };
    let parts: string[] = [];
    for (let m in mr.messages) {
        for (let b in m.content) {
            match (b.text) {
                let t: string => parts.push(t),
                null => null,
            };
        }
    }
    return system + parts.join("\n\n");
}

// Wrap an assistant text answer as an Anthropic Messages response.
function messages_response_json(model: string, text: string) -> string {
    return "{\"id\":\"msg_proxy\",\"type\":\"message\",\"role\":\"assistant\",\"model\":"
        + jesc(model)
        + ",\"content\":[{\"type\":\"text\",\"text\":"
        + jesc(text)
        + "}],\"stop_reason\":\"end_turn\",\"stop_sequence\":null,\"usage\":{\"input_tokens\":0,\"output_tokens\":0}}";
}

// ---- router + entrypoint ---------------------------------------------------

function proxy_handler(req: baml.http.Request) -> baml.http.Response {
    let path = path_only(req.url);
    if (req.method == "GET" && path == "/healthz") {
        return text_response(200, "ok");
    }
    // LLM gateway is authorized by x-api-key (not Bearer), handled before the gate.
    if (req.method == "POST" && path == "/v1/messages") {
        return handle_messages(req);
    }
    if (bearer_ok(req, proxy_token()) == false) {
        return json_response(401, "{\"error\":\"unauthorized\"}");
    }
    if (req.method == "POST" && path == "/check-baml") {
        return handle_check_baml(req);
    }
    if (req.method == "POST" && path == "/run-agent") {
        return handle_run_agent(req);
    }
    return json_response(404, "{\"error\":\"not found\"}");
}

// `baml run Proxy` entrypoint.
function Proxy() -> int throws baml.errors.Io {
    let addr = "[::]:" + proxy_listen_port();
    let server = baml.http.Server.bind(addr);
    baml.io.println("claude-proxy listening on " + server.addr);
    return server.serve(proxy_handler);
}
