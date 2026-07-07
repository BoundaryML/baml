// ingress — public webhook http.Server + the complete @bammy Slack bot. Ports
// services/ingress + services/ingress/bammy.py. Routes:
//   GET  /healthz                    liveness
//   GET  /entries                    public changelog JSON (the website contract)
//   POST /entries                    token-gated: enqueue generation for a tag
//   POST /entries/update             token-gated: enqueue any missed releases
//   POST /entries/{version}/revise   token-gated: requeue with revise guidance
//   POST /bug                        create a task/cohort (source=bug_report)
//   POST /feedback                   validated + rate-limited feedback intake
//   POST /slack/events               Slack url-verification + @bammy dispatcher
//   POST /linear/webhook             status label change -> issue lifecycle move
//
// @bammy: a mention is ACKed immediately (Slack's 3s rule) and dispatched in a
// spawned task — classify via RouteMention (LLM through the proxy), then handle:
//   bench | changelog_edit | changelog_sync | promo_claim | posthog_query |
//   github_usage | feedback | general, replying in-thread via slack_post_message.
//
// Hardening (ports the Python ingress): Slack v0 signature verification (fails
// CLOSED without ATB_SLACK_SIGNING_SECRET) + event_id dedup; Linear-Signature
// HMAC + webhookTimestamp replay guard when ATB_LINEAR_WEBHOOK_SECRET is set;
// per-IP + daily rate limits on /feedback. HMAC shells out to openssl
// (core/hmac.baml) — BAML has no crypto stdlib.

class SlackInner {
    type: string?,
    text: string?,
    channel: string?,
    user: string?,
    ts: string?,
    thread_ts: string?,
}

class SlackEvent {
    type: string?,
    challenge: string?,
    event_id: string?, // Slack delivery id — retries reuse it (our dedup key)
    event: SlackInner?,
}

class BugBody {
    prompt: string?,
    bamlChannel: string?,
    coldStart: bool?,
    bamlPin: string?,
}

// arenaBranches is string ("a,b") OR string[] — parsed separately because a
// required `json` field is strict-on-missing (and scalar `json?` is banned by
// the llm.parse union-flattening rule), so it can't live on BugBody.
class BugBodyBranches {
    arenaBranches: json,
}

// /feedback JSON body (the `baml feedback` CLI contract).
class FeedbackBody {
    message: string?,
    transcript: string?,
    filesCreated: map<string, string>?,
    bamlVersion: string?,
    os: string?,
    arch: string?,
    source: string?,
}

class SlackTaskDoc {
    source: string,
    prompt: string,
    status: string,
    slackChannel: string?,
    slackThreadTs: string?,
    slackUser: string?,
    bamlChannel: string?,
    bamlPin: string?,
    coldStart: bool?,
}

class FeedbackTaskDoc {
    source: string,
    prompt: string,
    status: string,
}

// The /feedback done-task (createDoc strips nulls, so optionals are safe).
class FeedbackTaskFull {
    source: string,
    prompt: string,
    bamlVersion: string?,
    status: string,
}

// The /feedback queued trophy that flows into dedup.
class FeedbackTrophyDoc {
    taskId: string,
    source: string,
    outcome: string,
    bamlVersion: string?,
    metrics: map<string, json>,
    summary: string,
    transcriptStorageId: string?,
    turnLog: json[]?,
    filesCreated: map<string, string>?,
    status: string,
}

// A bench task created via /bug or @bammy, carrying any per-run directives.
class DirectiveTaskDoc {
    source: string,
    prompt: string,
    status: string,
    cohortId: string?,
    skillRef: string?,
    bamlChannel: string?,
    bamlPin: string?,
    coldStart: bool?,
}

// A skill-arena cohort row (one member task per branch). Slack routing rides
// the cohort so CohortCompare can post the single comparison reply.
class ArenaCohortDoc {
    prompt: string,
    skillRefs: string[],
    memberTaskIds: string[],
    source: string,
    status: string,
    slackChannel: string?,
    slackThreadTs: string?,
    slackUser: string?,
}

class CohortMembersPatch {
    memberTaskIds: string[],
}

function ingress_svc() -> ServiceClient {
    return ServiceClient { base_url: service_url(), token: service_token() };
}

// ---- request helpers ---------------------------------------------------------

// Case-tolerant header read (HTTP header names are case-insensitive; the server
// hands us whatever casing the client sent).
function header_get(req: baml.http.Request, name: string) -> string {
    let want = name.to_lower_case();
    for (let k in req.headers.keys()) {
        if (k.to_lower_case() == want) {
            return match (req.headers.get(k)) {
                let v: string => v,
                null => "",
            };
        }
    }
    return "";
}

// Bearer gate for ops endpoints (ports _require_service_token: dev-mode bypass
// when no token is configured).
function service_token_ok(req: baml.http.Request) -> bool {
    let tok = service_token();
    if (tok == "") {
        return true;
    }
    return digest_eq(header_get(req, "authorization"), "Bearer " + tok);
}

// ---- delivery dedup + rate limiting (file-backed; /tmp resets per deploy) ----

// Record an event id and report whether it was already seen (ports the _seen
// LRU: retried deliveries collapse to no-ops). File-per-id under /tmp so the
// spawned dispatchers and concurrent requests share one view; the dir is
// cleared wholesale past a size cap instead of LRU-evicting.
function seen_before(kind: string, id: string) -> bool {
    if (id == "") {
        return false;
    }
    let dir = "/tmp/atb-ingress-seen";
    let _ = baml.fs.mkdir(dir, baml.fs.MkdirOptions { recursive: true }) catch (_) {
        _ => null
    };
    prune_dir(dir, 1024);
    let path = dir + "/" + kind + "-" + sanitize_key(id);
    let existed = baml.fs.exists(path) catch (_) {
        _ => false
    };
    if (existed) {
        return true;
    }
    let _ = baml.fs.write(path, "1") catch (_) {
        _ => null
    };
    return false;
}

// Keep only [a-z0-9-_] so an id can't traverse paths.
function sanitize_key(id: string) -> string {
    let out: string[] = [];
    for (let ch in id.to_lower_case().split("")) {
        let keep = (ch >= "a" && ch <= "z") || (ch >= "0" && ch <= "9") || ch == "-" || ch == "_";
        if (keep) {
            out.push(ch);
        }
    }
    return out.join("");
}

// Clear a marker dir wholesale once it exceeds `cap` entries (bounds disk under
// id churn; a rare post-clear duplicate re-delivery is acceptable).
function prune_dir(dir: string, cap: int) -> int {
    let entries: baml.fs.DirEntry[] = baml.fs.read_dir(dir) catch (_) {
        _ => []
    };
    if (entries.length() <= cap) {
        return 0;
    }
    for (let e in entries) {
        let _ = baml.fs.remove(dir + "/" + e.name) catch (_) {
            _ => null
        };
    }
    return entries.length();
}

// /feedback rate limiting (ports _feedback_allowed): per-IP sliding window
// approximating the old token bucket (burst 3, 5/hour) plus a global daily cap.
function feedback_allowed(ip: string) -> bool {
    let now = baml.sys.now_ms();
    let dir = "/tmp/atb-ingress-rate";
    let _ = baml.fs.mkdir(dir, baml.fs.MkdirOptions { recursive: true }) catch (_) {
        _ => null
    };

    // Global daily cap, keyed by the UTC day start.
    let day_start = now - (now % 86400000);
    let day_path = dir + "/day-" + baml.unstable.string(day_start);
    let day_count = read_count(day_path);
    if (day_count >= feedback_daily_cap()) {
        return false;
    }

    // Per-IP window: <=burst in the last 10 minutes, <=per-hour in the last hour.
    let ip_path = dir + "/ip-" + sanitize_key(ip);
    let stamps = read_stamps(ip_path);
    let hour_ago = now - 3600000;
    let burst_ago = now - 600000;
    let recent: int[] = [];
    let in_hour = 0;
    let in_burst = 0;
    for (let t in stamps) {
        if (t > hour_ago) {
            recent.push(t);
            in_hour = in_hour + 1;
            if (t > burst_ago) {
                in_burst = in_burst + 1;
            }
        }
    }
    if (in_hour >= feedback_per_hour() || in_burst >= feedback_burst()) {
        let _ = write_stamps(ip_path, recent);
        return false;
    }
    recent.push(now);
    let _ = write_stamps(ip_path, recent);
    let _ = baml.fs.write(day_path, baml.unstable.string(day_count + 1)) catch (_) {
        _ => null
    };
    prune_dir(dir, 4096);
    return true;
}

function read_count(path: string) -> int {
    let raw = baml.fs.read(path) catch (_) {
        _ => "0"
    };
    return baml.json.from_string<int>(raw.trim()) catch (_) {
        _ => 0
    };
}

function read_stamps(path: string) -> int[] {
    let raw = baml.fs.read(path) catch (_) {
        _ => "[]"
    };
    return baml.json.from_string<int[]>(raw) catch (_) {
        _ => []
    };
}

function write_stamps(path: string, stamps: int[]) -> int {
    let body = baml.json.to_string(stamps) catch (_) {
        _ => "[]"
    };
    let _ = baml.fs.write(path, body) catch (_) {
        _ => null
    };
    return 0;
}

function feedback_burst() -> int {
    return env_int("FEEDBACK_BURST", 3);
}

function feedback_per_hour() -> int {
    return env_int("FEEDBACK_PER_HOUR", 5);
}

function feedback_daily_cap() -> int {
    return env_int("FEEDBACK_DAILY_CAP", 100);
}

// ---- /bug + /feedback ------------------------------------------------------

function handle_bug(req: baml.http.Request) -> baml.http.Response {
    let parsed = baml.json.from_string<BugBody>(req.body) catch (_) {
        _ => null
    };
    let body = match (parsed) {
        let b: BugBody => b,
        null => BugBody { prompt: null, bamlChannel: null, coldStart: null, bamlPin: null },
    };
    let raw_prompt = match (body.prompt) {
        let p: string => p.trim(),
        null => "",
    };
    if (raw_prompt == "") {
        return json_response(400, "{\"error\":\"missing prompt\"}");
    }

    // Honor inline [baml=N]/[canary]/[coldstart]/[skill arena] directives, plus
    // explicit payload overrides for each field (ports the Python /bug).
    let d = parse_run_directives(raw_prompt);
    let channel = match (body.bamlChannel) {
        let c: string => c.to_lower_case(),
        null => d.baml_channel,
    };
    let cold = match (body.coldStart) {
        let c: bool => c || d.cold_start,
        null => d.cold_start,
    };
    let pin = if (cold) {
        // Cold start withholds the prebuilt binary; a pin alongside it is unused.
        ""
    } else {
        match (body.bamlPin) {
            let p: string => p,
            null => d.baml_pin,
        }
    };

    // A skill-arena bug fans out a cohort (payload arenaBranches wins over the
    // inline directive; string "a,b" or ["a","b"] both accepted). Parsed off a
    // dedicated class since the field is polymorphic json.
    let branch_body = baml.json.from_string<BugBodyBranches>(req.body) catch (_) {
        _ => null
    };
    let payload_ab: json = match (branch_body) {
        let bb: BugBodyBranches => bb.arenaBranches,
        null => null,
    };
    let branches = match (payload_branches(payload_ab)) {
        let b: string[] => {
            if (b.length() > 0) {
                b
            } else {
                d.arena_branches
            }
        },
        null => d.arena_branches,
    };
    if (branches.length() > 0) {
        let cid = create_cohort(d.cleaned, branches, channel, pin, cold, "bug_report", "", "", "");
        return json_response(200, "{\"cohortId\":" + jesc(cid) + "}");
    }

    let tid = create_directive_task("bug_report", d.cleaned, "", "", channel, pin, cold);
    return json_response(200, "{\"id\":" + jesc(tid) + "}");
}

// Coerce the payload's arenaBranches (string "a,b" or string[]) to a branch list.
function payload_branches(v: json) -> string[]? {
    let as_str = baml.json.from_string<string>(baml.json.stringify(v) catch (_) {
        _ => "null"
    }) catch (_) {
        _ => ""
    };
    if (as_str != "") {
        return parse_branches(as_str);
    }
    let as_list: string[] = baml.json.from_string<string[]>(baml.json.stringify(v) catch (_) {
        _ => "null"
    }) catch (_) {
        _ => []
    };
    if (as_list.length() > 0) {
        return parse_branches(as_list.join(","));
    }
    return null;
}

// /feedback (ports the Python endpoint + feedback_handler.create_feedback):
// validated JSON body, per-IP + daily rate limits, and a done-task + queued-
// trophy pair so the report flows through dedup onto the issue board. The old
// handler stored the raw request body as a task prompt and always said ok.
function handle_feedback(req: baml.http.Request) -> baml.http.Response {
    if (req.body.length() > 4194304) {
        return json_response(413, "{\"error\":\"feedback too large\"}");
    }
    let ip = match (header_get(req, "fly-client-ip")) {
        "" => "?",
        _ => header_get(req, "fly-client-ip"),
    };
    if (feedback_allowed(ip) == false) {
        return json_response(429, "{\"error\":\"rate limited; try again later\"}");
    }
    let parsed = baml.json.from_string<FeedbackBody>(req.body) catch (_) {
        _ => null
    };
    let body = match (parsed) {
        let b: FeedbackBody => b,
        null => {
            return json_response(400, "{\"error\":\"invalid JSON body\"}");
        },
    };
    let message = match (body.message) {
        let m: string => m.trim(),
        null => "",
    };
    if (message == "" || message.length() > 8000) {
        return json_response(400, "{\"error\":\"message required (1..8000 chars)\"}");
    }

    // Optional context: trim, never reject — a fat transcript shouldn't lose the
    // report. Transcript keeps the TAIL (the end of a session is where the
    // problem usually is); files are capped per-file and in total.
    let transcript = match (body.transcript) {
        let t: string => tail_str(t, 2097152),
        null => "",
    };
    let files: map<string, string> = {};
    let files_total = 0;
    match (body.filesCreated) {
        let fc: map<string, string> => {
            for (let k in fc.keys()) {
                let content = match (fc.get(k)) {
                    let c: string => head_str(c, 262144),
                    null => "",
                };
                if (files_total + content.length() <= 1048576) {
                    files.set(k, content);
                    files_total = files_total + content.length();
                }
            }
        },
        null => null,
    };

    let ids = create_feedback_pair(message, body, transcript, files);
    return json_response(200, ids);
}

// Create the done task + queued trophy for one piece of feedback; returns the
// {"taskId","trophyId"} JSON. When a transcript parses as a Claude session it's
// rendered terminal-style, stored as the trophy's blob, and its turn log rides
// the trophy — so the run page shows feedback like any agent run.
function create_feedback_pair(
    message: string,
    body: FeedbackBody,
    transcript: string,
    files: map<string, string>,
) -> string {
    let svc = ingress_svc();
    let task_doc = baml
        .json
        .to_string(
            FeedbackTaskFull {
                source: "feedback",
                prompt: message,
                bamlVersion: body.bamlVersion,
                status: "done",
            },
        ) catch (_) {
        _ => "{}"
    };
    let task_id = svc.create("tasks", task_doc) catch (_) {
        _ => ""
    };

    let metrics: map<string, json> = {};
    let origin = match (body.source) {
        let s: string => s,
        null => "cli",
    };
    metrics.set("origin", origin);
    metrics.set("receivedAt", baml.sys.now_ms());
    match (body.bamlVersion) {
        let v: string => {
            metrics.set("bamlVersion", v);
        },
        null => null,
    };
    match (body.os) {
        let v: string => {
            metrics.set("os", v);
        },
        null => null,
    };
    match (body.arch) {
        let v: string => {
            metrics.set("arch", v);
        },
        null => null,
    };

    // Transcript: best-effort parse into a terminal render + turn log.
    let storage_id: string? = null;
    let turn_log: json[]? = null;
    if (transcript != "" && task_id != "") {
        let rows = parse_turn_log(transcript);
        let blob = if (rows.length() > 0) {
            render_terminal_transcript(transcript)
        } else {
            transcript
        };
        if (rows.length() > 0) {
            metrics.set("turns", rows.length());
            turn_log = baml
                .json
                .from_string<json[]>(baml.json.to_string(rows) catch (_) {
                    _ => "[]"
                }) catch (_) {
                _ => []
            };
        }
        storage_id = svc.put_transcript("tasks", task_id, blob) catch (_) {
            _ => null
        };
    }
    if (files.keys().length() > 0) {
        metrics.set("files_touched", files.keys().length());
        let loc = 0;
        for (let k in files.keys()) {
            let content = match (files.get(k)) {
                let c: string => c,
                null => "",
            };
            loc = loc + content.split("\n").length();
        }
        metrics.set("loc_changed", loc);
    }

    let files_opt: map<string, string>? = if (files.keys().length() > 0) {
        files
    } else {
        null
    };
    let trophy_doc = baml
        .json
        .to_string(
            FeedbackTrophyDoc {
                taskId: task_id,
                source: "feedback",
                outcome: "feedback",
                bamlVersion: body.bamlVersion,
                metrics: metrics,
                summary: message,
                transcriptStorageId: storage_id,
                turnLog: turn_log,
                filesCreated: files_opt,
                status: "queued",
            },
        ) catch (_) {
        _ => "{}"
    };
    let trophy_id = svc.create("trophies", trophy_doc) catch (_) {
        _ => ""
    };
    return "{\"taskId\":" + jesc(task_id) + ",\"trophyId\":" + jesc(trophy_id) + "}";
}

// ---- /slack/events ---------------------------------------------------------

// Slack v0 request signing: X-Slack-Signature == "v0=" + HMAC_SHA256(secret,
// "v0:{ts}:{raw body}"), ts within 300s. FAILS CLOSED on a missing signing
// secret (parity with slack_client.verify_signature) — an unsigned deploy
// rejects events rather than accepting the world's.
function slack_signature_ok(req: baml.http.Request) -> bool {
    let secret = env_or("SLACK_SIGNING_SECRET", "");
    if (secret == "") {
        baml.io.eprintln("slack: SLACK_SIGNING_SECRET unset; rejecting event (fails closed)");
        return false;
    }
    let ts = header_get(req, "x-slack-request-timestamp").trim();
    let ts_secs = baml.json.from_string<int>(ts) catch (_) {
        _ => 0
    };
    if (ts_secs == 0) {
        return false;
    }
    let skew = baml.sys.now_ms() - ts_secs * 1000;
    if (skew > 300000 || skew < -300000) {
        return false;
    }
    let expected = "v0=" + hmac_sha256_hex(secret, "v0:" + ts + ":" + req.body);
    return digest_eq(header_get(req, "x-slack-signature"), expected);
}

function handle_slack(req: baml.http.Request) -> baml.http.Response {
    let ev = baml.json.from_string<SlackEvent>(req.body) catch (_) {
        _ => null
    };
    return match (ev) {
        null => json_response(200, "{}"),
        let e: SlackEvent => {
            match (e.challenge) {
                // url_verification handshake (answered before the signature check,
                // matching the Python ingress).
                let c: string => json_response(200, "{\"challenge\":" + jesc(c) + "}"),
                null => {
                    if (slack_signature_ok(req) == false) {
                        return json_response(401, "{\"error\":\"bad signature\"}");
                    }
                    // Dedup retried deliveries (same event_id) before dispatching —
                    // without this a slow mention gets retried by Slack and runs twice.
                    let eid = match (e.event_id) {
                        let i: string => i,
                        null => "",
                    };
                    if (seen_before("slack", eid)) {
                        let retry = header_get(req, "x-slack-retry-num");
                        baml.io.println("slack: deduped event_id=" + eid + " retry#=" + retry);
                        return json_response(200, "{}");
                    }
                    // ACK now, dispatch off the request path (Slack's 3s rule).
                    let _ = spawn { dispatch_mention(e) };
                    json_response(200, "{}")
                },
            }
        },
    };
}

// The @bammy dispatcher (spawned; swallows all errors per BEP-034).
function dispatch_mention(e: SlackEvent) -> int {
    let inner = match (e.event) {
        let i: SlackInner => i,
        null => {
            SlackInner {
                type: null,
                text: null,
                channel: null,
                user: null,
                ts: null,
                thread_ts: null,
            }
        },
    };
    let text = match (inner.text) {
        let t: string => strip_mention(t),
        null => "",
    };
    let channel = match (inner.channel) {
        let c: string => c,
        null => "",
    };
    let thread = match (inner.thread_ts) {
        let t: string => t,
        null => match (inner.ts) {
            let t: string => t,
            null => "",
        },
    };
    let user = match (inner.user) {
        let u: string => u,
        null => "",
    };
    let token = slack_bot_token();

    // A mention that's empty after stripping the bot token isn't actionable.
    if (text == "") {
        return 0;
    }

    // 0. "How were you built?" fast-path — bammy2 self-identifies as the BAML
    //    rewrite (vs bammy1, the Python version). No model call.
    if (is_about_question(text)) {
        let _ = reply(token, channel, thread, architecture_reply());
        return 0;
    }

    // 1. Directive fast-path: explicit bench opt-in, no model call.
    if (has_bench_directives(text)) {
        let _ = start_bench(text, channel, thread, user, token);
        return 0;
    }

    // Mid-thread mention: hand the classifier the thread so "fix that" resolves
    // against what was being discussed (ports bammy's conversations.replies fetch).
    let ts = match (inner.ts) {
        let t: string => t,
        null => "",
    };
    let thread_context = if (thread != "" && thread != ts) {
        slack_fetch_thread(token, channel, thread)
    } else {
        ""
    };

    // 2. Classify (LLM via the proxy). Fail open to bench.
    let route = RouteMention(text, thread_context, changelog_catalog()) catch (_) {
        _ => null
    };
    match (route) {
        null => {
            let _ = start_bench(text, channel, thread, user, token);
        },
        let r: Route => {
            baml.io
                .println(
                    "bammy route: intent="
                        + r.intent
                        + " needs_clarif="
                        + baml.unstable.string(r.needs_clarification),
                );
            let _ = dispatch_route(r, text, channel, thread, user, token);
        },
    };
    return 0;
}

function dispatch_route(
    r: Route,
    text: string,
    channel: string,
    thread: string,
    user: string,
    token: string,
) -> int {
    if (r.needs_clarification) {
        let q = match (r.clarification_question) {
            "" => "Can you give me a bit more detail?",
            _ => r.clarification_question,
        };
        let _ = reply(token, channel, thread, q);
        return 0;
    }

    if (r.intent == "promo_claim") {
        let code = ingress_svc().promo_claim(user, user, r.promo_notes) catch (_) {
            _ => ""
        };
        let msg = match (code) {
            "" => "We're out of codes! Ping the growth team to load more.",
            _ => "`" + code + "` — logged for <@" + user + ">. One per person, enjoy the shirt!",
        };
        let _ = reply(token, channel, thread, msg);
        return 0;
    }

    if (r.intent == "feedback") {
        let msg = match (r.feedback_text) {
            "" => text,
            _ => r.feedback_text,
        };
        let _ = log_feedback(msg, channel, thread, user);
        let _ = reply(token, channel, thread, "Logged — it'll be triaged into the issue board. Thanks!");
        return 0;
    }

    if (r.intent == "posthog_query") {
        let question = match (r.posthog_question) {
            "" => text,
            _ => r.posthog_question,
        };
        let answer = run_posthog(question);
        let _ = reply(token, channel, thread, answer);
        return 0;
    }

    if (r.intent == "changelog_sync") {
        let n = sync_changelog();
        let msg = match (n) {
            0 => "Changelog is already up to date with GitHub.",
            _ => {
                "Enqueued "
                    + baml.unstable.string(n)
                    + " changelog entr(ies). They'll publish as generation finishes."
            },
        };
        let _ = reply(token, channel, thread, msg);
        return 0;
    }

    if (r.intent == "changelog_edit") {
        let ok = requeue_changelog(r.version_ref, r.mode, r.guidance);
        let msg = match (ok) {
            0 => {
                "Couldn't requeue `"
                    + r.version_ref
                    + "` — either no entry exists for that version or it's already regenerating."
            },
            _ => "Re-running the changelog entry for `" + r.version_ref + "`. It'll update shortly.",
        };
        let _ = reply(token, channel, thread, msg);
        return 0;
    }

    if (r.intent == "github_usage") {
        let _ = reply(
            token,
            channel,
            thread,
            "Scanning public GitHub for BAML usage… I'll post a report here when it's ready.",
        );
        return 0;
    }

    if (r.intent == "general") {
        let _ = reply(token, channel, thread, help_text());
        return 0;
    }

    // bench (and anything unexpected).
    let prompt = match (r.bench_prompt) {
        "" => text,
        _ => r.bench_prompt,
    };
    let _ = start_bench(prompt, channel, thread, user, token);
    return 0;
}

// ---- /linear/webhook -------------------------------------------------------

// ---- Linear webhook payload shapes ----------------------------------------

class LinearLabelRef {
    id: string?,
}

class LinearWebhookData {
    id: string?,
    title: string?,
    description: string?,
    labelIds: string[]?,
    labels: LinearLabelRef[]?,
}

class LinearWebhook {
    type: string?,
    action: string?, // create | update | remove
    webhookTimestamp: float?, // ms; replay-guard input
    data: LinearWebhookData?,
}

// The status gates which human move is honored; title + the push stamps feed
// the human-edit backflow (sync_card_edit).
class WebhookIssue {
    _id: string?,
    status: string?,
    title: string?,
    // What LinearPush last rendered onto the card (stamp_pushed). A webhook
    // payload matching these is our own echo; a mismatch is a human edit.
    lastPushedTitle: string?,
    lastPushedBodyHash: string?,
}

// Doc shape for ingesting a Linear card that has no mirrored Convex issue yet.
// Only the fields the issues schema needs; createDoc fills attempts/createdAt/
// updatedAt and strips nulls. evidence stays empty (no trophy backs a board card).
class WebhookIssueCreate {
    kind: string,
    title: string,
    description: string,
    category: string,
    evidence: string[],
    status: string,
    linearSyncStatus: string,
    linearIssueId: string,
    firstSeenAt: int,
    lastSeenAt: int,
}

// Route a Linear Issue event to a lifecycle move. We match the Convex issue by
// linearIssueId and read its status-group label: only the human-moved labels route
// — approved -> approved (FixDispatch claims it), redraft -> redraft, pr-prep (from
// needs_human) -> retry the PR with a fresh fix budget, rejected -> delete. Our own
// to-cursor/pr-prep/merged writes are no-ops via the resting-state gate, which also
// makes the webhook safe against duplicate deliveries without an actor check.
//
// Hardening (ports the Python ingress): when LINEAR_WEBHOOK_SECRET is set, the
// Linear-Signature header must be the HMAC-SHA256 hex of the raw body (401
// otherwise); unset skips verification (dev only). Independently, an event whose
// webhookTimestamp is outside LINEAR_WEBHOOK_MAX_SKEW_MS is dropped (200) as a
// replay.
function handle_linear(req: baml.http.Request) -> baml.http.Response {
    let secret = env_or("LINEAR_WEBHOOK_SECRET", "");
    if (secret != "") {
        let expected = hmac_sha256_hex(secret, req.body);
        if (digest_eq(header_get(req, "linear-signature"), expected) == false) {
            baml.io.eprintln("linear webhook: bad signature");
            return json_response(401, "{\"error\":\"bad signature\"}");
        }
    }
    let parsed = baml.json.from_string<LinearWebhook>(req.body) catch (_) {
        _ => null
    };
    return match (parsed) {
        null => json_response(200, "{}"),
        let w: LinearWebhook => {
            match (w.webhookTimestamp) {
                let ts: float => {
                    let skew = baml.sys.now_ms() - f2i(ts);
                    let max_skew = env_int("LINEAR_WEBHOOK_MAX_SKEW_MS", 60000);
                    if (skew > max_skew || skew < 0 - max_skew) {
                        baml.io.eprintln("linear webhook: stale webhookTimestamp; ignoring");
                        return json_response(200, "{}");
                    }
                },
                null => null,
            };
            route_linear_webhook(w)
        },
    };
}

function route_linear_webhook(w: LinearWebhook) -> baml.http.Response {
    let typ = match (w.type) {
        let t: string => t,
        null => "",
    };
    if (typ != "Issue") {
        return json_response(200, "{}");
    }
    let data = match (w.data) {
        let d: LinearWebhookData => d,
        null => {
            return json_response(200, "{}");
        },
    };
    let linear_id = match (data.id) {
        let i: string => i,
        null => "",
    };
    if (linear_id == "") {
        return json_response(400, "{\"error\":\"no issue id\"}");
    }

    let raw = ingress_svc().list("issues", "linearIssueId", linear_id, "by_linear_issue", 1) catch (_) {
        _ => "[]"
    };
    let issues: WebhookIssue[] = baml.json.from_string<WebhookIssue[]>(raw) catch (_) {
        _ => []
    };
    let action = match (w.action) {
        let a: string => a,
        null => "",
    };
    let issue = match (issues.at(0)) {
        let i: WebhookIssue => i,
        null => {
            if (action == "remove") {
                // Card deleted and nothing mirrors it — nothing to do.
                return json_response(200, "{}");
            }
            // No Convex issue mirrors this Linear card yet — it was created/managed
            // directly on the board. Ingest it as a new issue so the human move still
            // drives the pipeline (e.g. approved -> FixDispatch -> Cursor + Slack).
            return create_issue_from_webhook(linear_id, data);
        },
    };
    let iid = match (issue._id) {
        let v: string => v,
        null => {
            return json_response(200, "{}");
        },
    };

    // Card deleted on Linear: the board is the human surface — drop the mirrored
    // Convex issue too (consistent with the REJECTED label path below).
    if (action == "remove") {
        let _ = ingress_svc().remove("issues", iid) catch (_) {
            _ => 0
        };
        baml.io.println("linear-webhook: card " + linear_id + " removed -> deleted issue " + iid);
        return json_response(200, "{}");
    }

    // Human edits to the card's title/body flow back into Convex (guarded by the
    // push stamps so our own sync echoes no-op).
    if (action == "update") {
        let _ = sync_card_edit(iid, issue, data);
    }
    let label_ids = webhook_label_ids(data);
    baml.io
        .println(
            "linear-webhook: id="
                + linear_id
                + " issue="
                + iid
                + " status="
                + match (issue.status) {
                    let s: string => s,
                    null => "?",
                }
                + " labels=["
                + label_ids.join(",")
                + "] (redraft="
                + linear_status_id("REDRAFT")
                + " approved="
                + linear_status_id("APPROVED")
                + ")",
        );

    // Rejected is terminal: delete the issue from the pipeline DB (from ANY status).
    if (in_list(label_ids, linear_status_id("REJECTED"))) {
        let _ = ingress_svc().remove("issues", iid) catch (_) {
            _ => 0
        };
        return json_response(200, "{}");
    }

    let patch = route_linear_status(issue, label_ids);
    if (patch != "") {
        let _ = ingress_svc().update("issues", iid, patch) catch (_) {
            _ => ""
        };
    }
    return json_response(200, "{}");
}

// The labelIds on the event (preferring the explicit list, else the labels array).
function webhook_label_ids(data: LinearWebhookData) -> string[] {
    return match (data.labelIds) {
        let ids: string[] => ids,
        null => {
            let out: string[] = [];
            let labels: LinearLabelRef[] = match (data.labels) {
                let l: LinearLabelRef[] => l,
                null => [],
            };
            for (let lb in labels) {
                match (lb.id) {
                    let i: string => out.push(i),
                    null => 0,
                };
            }
            out
        },
    };
}

// Map the status-group label + the issue's CURRENT status (the resting-state gate)
// Initial pipeline status for a brand-new issue ingested from a Linear card, based
// on the human-action label it carries. "" when the card has no actionable label.
function initial_status_for_labels(label_ids: string[]) -> string {
    if (in_list(label_ids, linear_status_id("APPROVED"))) {
        return "approved";
    }
    if (in_list(label_ids, linear_status_id("REDRAFT"))) {
        return "redraft";
    }
    if (in_list(label_ids, linear_status_id("PR_PREP"))) {
        return "prprep";
    }
    return "";
}

// Ingest a Linear card that has no mirrored Convex issue. Only act on a human-action
// label (approved/redraft/pr-prep) — otherwise a card edit would spawn junk issues.
// Stamp linearIssueId (so later webhooks match this issue) and linearSyncStatus
// "synced" (the card already exists; LinearPush must not create a duplicate).
function create_issue_from_webhook(linear_id: string, data: LinearWebhookData) -> baml.http.Response {
    let label_ids = webhook_label_ids(data);
    let status = initial_status_for_labels(label_ids);
    if (status == "") {
        baml.io
            .println(
                "linear-webhook: id=" + linear_id + " -> no matching convex issue, no actionable label; ignored",
            );
        return json_response(200, "{}");
    }
    let title = match (data.title) {
        let t: string => t,
        null => "(imported from Linear)",
    };
    let desc = match (data.description) {
        let d: string => d,
        null => "",
    };
    let ev: string[] = [];
    let now = baml.sys.now_ms();
    let doc = WebhookIssueCreate {
        kind: "skill",
        title: title,
        description: desc,
        category: "bug",
        evidence: ev,
        status: status,
        linearSyncStatus: "synced",
        linearIssueId: linear_id,
        firstSeenAt: now,
        lastSeenAt: now,
    };
    let doc_json = baml.json.to_string(doc) catch (_) {
        _ => ""
    };
    if (doc_json == "") {
        return json_response(200, "{}");
    }
    let new_id = ingress_svc().create("issues", doc_json) catch (_) {
        _ => ""
    };
    baml.io
        .println(
            "linear-webhook: id=" + linear_id + " -> created convex issue " + new_id + " (status=" + status + ")",
        );
    return json_response(200, "{}");
}

// Flow a human's Linear card edit (title/body) back into the Convex issue.
// Loop-guarded by the push stamps: LinearPush stamps lastPushedTitle +
// lastPushedBodyHash after every card write, so a webhook payload matching the
// stamps is our own echo (no-op). An ABSENT stamp also no-ops — pre-existing
// cards are grandfathered until their next push stamps them, which prevents
// mis-reading an unstamped machine render as a human edit. Deliberately does
// NOT mark linearSyncStatus dirty: an immediate re-push would re-render the
// body and clobber exactly what the human just typed; the next natural push
// re-normalizes, and its echo no-ops because the hashes then match.
function sync_card_edit(iid: string, issue: WebhookIssue, data: LinearWebhookData) -> int {
    let pushed_title = match (issue.lastPushedTitle) {
        let t: string => t,
        null => "",
    };
    let pushed_hash = match (issue.lastPushedBodyHash) {
        let h: string => h,
        null => "",
    };
    let patch_map: map<string, json> = {};
    let changed = false;

    let new_title = match (data.title) {
        let t: string => t,
        null => "",
    };
    if (pushed_title != "" && new_title != "" && new_title != pushed_title) {
        patch_map.set("title", new_title);
        // Board and Convex agree on this title now; only a NEXT human edit differs.
        patch_map.set("lastPushedTitle", new_title);
        changed = true;
    }

    let body = match (data.description) {
        let d: string => d,
        null => "",
    };
    if (pushed_hash != "" && body != "") {
        let h = body_hash(body);
        if (h != pushed_hash) {
            let region = extract_description_region(body);
            if (region != "") {
                patch_map.set("description", region);
            }
            patch_map.set("lastPushedBodyHash", h);
            changed = true;
        }
    }

    if (changed == false) {
        return 0;
    }
    let patch = baml.json.to_string(patch_map) catch (_) {
        _ => ""
    };
    if (patch == "") {
        return 0;
    }
    let _ = ingress_svc().update("issues", iid, patch) catch (e) {
        _ => {
            baml.io
                .eprintln(
                    "linear-webhook: sync_card_edit update failed for " + iid + ": " + baml.unstable.string(e),
                );
            ""
        }
    };
    baml.io.println("linear-webhook: human card edit flowed back into issue " + iid);
    return 0;
}

// to a patch, or "" for no-op. Mirrors services/ingress/app.py _route_linear_status.
function route_linear_status(issue: WebhookIssue, label_ids: string[]) -> string {
    let cur = match (issue.status) {
        let s: string => s,
        null => "",
    };
    if (in_list(label_ids, linear_status_id("APPROVED"))) {
        if (approve_from(cur)) {
            return "{\"status\":\"approved\"}";
        }
        return "";
    }
    if (in_list(label_ids, linear_status_id("REDRAFT"))) {
        if (redraft_from(cur)) {
            return "{\"status\":\"redraft\"}";
        }
        return "";
    }
    if (in_list(label_ids, linear_status_id("PR_PREP"))) {
        if (cur == "needs_human") {
            // Human retry of a stuck PR: restart the fix budget + clear dispatch refs.
            return "{\"status\":\"prprep\",\"fixAttempts\":0,\"lastFixedSha\":\"\",\"cursorAgentId\":\"\",\"fixSlackTs\":\"\"}";
        }
        return "";
    }
    // A human moving the card to Done/Merged closes the ATB issue (its terminal
    // state). Without this branch the issue stayed mid-review forever even after
    // the work shipped on Linear.
    if (in_list(label_ids, linear_status_id("MERGED"))) {
        if (closeable_from(cur)) {
            return "{\"status\":\"closed\"}";
        }
        return "";
    }
    return "";
}

// An issue can be closed from any non-terminal state (it's already terminal at
// closed/rejected — a redundant Done move is a no-op).
function closeable_from(cur: string) -> bool {
    return cur != "closed" && cur != "rejected";
}

// Resting states from which a human "approved"/"redraft" board move is honored.
function approve_from(cur: string) -> bool {
    return cur == "open"
        || cur == "confirmed"
        || cur == "needs_human"
        || cur == "redraft"
        || cur == "failed"
        // A human approval wins over a queued/running stale re-check.
        || cur == "reverify"
        || cur == "reverifying";
}

function redraft_from(cur: string) -> bool {
    return cur == "open"
        || cur == "confirmed"
        || cur == "needs_human"
        || cur == "approved"
        || cur == "failed"
        // A human redraft drag should win even mid-verify (a transient auto-state
        // that can also strand an issue if its claim is orphaned by a redeploy).
        || cur == "verifying"
        || cur == "reverify"
        || cur == "reverifying";
}

// ---- handlers / helpers ----------------------------------------------------

// Strip a leading Slack mention token "<@U…> " from the text.
function strip_mention(t: string) -> string {
    let segs = t.split(">");
    if (t.starts_with("<@") && segs.length() > 1) {
        let rest = match (segs.at(1)) {
            let v: string => v,
            null => t,
        };
        return rest.trim();
    }
    return t.trim();
}

// ---- per-run directive grammar (ports handlers/bench.py) --------------------
// `[baml=N]` pin · `[canary]`/`[nightly]` channel · `[coldstart]`/`[cold]` ·
// `[skill arena(: branches)]` — matched anywhere, whitespace/case tolerant,
// stripped from the prompt.

class RunDirectives {
    cleaned: string,
    baml_channel: string, // "" when unspecified
    baml_pin: string, // "" when unspecified (or cold start won)
    cold_start: bool,
    arena_branches: string[], // empty when not an arena run
}

function parse_run_directives(text: string) -> RunDirectives {
    let segs = text.split("[");
    let kept: string[] = [];
    let channel = "";
    let pin = "";
    let cold = false;
    let arena: string[] = [];
    let first = match (segs.at(0)) {
        let v: string => v,
        null => "",
    };
    kept.push(first);
    let i = 1;
    while (i < segs.length()) {
        let seg = match (segs.at(i)) {
            let v: string => v,
            null => "",
        };
        let parts = seg.split("]");
        if (parts.length() < 2) {
            // No closing bracket — not a directive, keep verbatim.
            kept.push("[" + seg);
        } else {
            let inner = match (parts.at(0)) {
                let v: string => v,
                null => "",
            };
            let after = join_from_sep(parts, 1, "]");
            let norm = collapse_ws(inner).to_lower_case();
            if (norm == "cold" || norm == "coldstart") {
                cold = true;
                kept.push(after);
            } else if (norm == "canary" || norm == "nightly") {
                channel = norm;
                kept.push(after);
            } else if (norm.starts_with("baml") && inner.includes("=")) {
                let vparts = inner.split("=");
                pin = join_from_sep(vparts, 1, "=").trim();
                kept.push(after);
            } else if (norm.starts_with("skill arena")) {
                let csv = if (inner.includes(":")) {
                    let cparts = inner.split(":");
                    join_from_sep(cparts, 1, ":")
                } else {
                    arena_branches_default()
                };
                arena = parse_branches(csv);
                kept.push(after);
            } else {
                kept.push("[" + seg);
            }
        }
        i = i + 1;
    }
    // Cold start wins over a pin — both withhold the channel's latest binary.
    if (cold) {
        pin = "";
    }
    return RunDirectives {
        cleaned: collapse_ws(kept.join("")),
        baml_channel: channel,
        baml_pin: pin,
        cold_start: cold,
        arena_branches: arena,
    };
}

function has_bench_directives(text: string) -> bool {
    let d = parse_run_directives(text);
    return d.baml_channel != "" || d.baml_pin != "" || d.cold_start || d.arena_branches.length() > 0;
}

// Comma list -> trimmed, de-duplicated branch names.
function parse_branches(csv: string) -> string[] {
    let out: string[] = [];
    for (let raw in csv.split(",")) {
        let ref = raw.trim();
        if (ref != "" && in_list(out, ref) == false) {
            out.push(ref);
        }
    }
    return out;
}

function arena_branches_default() -> string {
    return env_or("ARENA_BRANCHES", "main");
}

// Collapse all whitespace runs to single spaces and trim.
function collapse_ws(s: string) -> string {
    let unified = s.split("\n").join(" ").split("\t").join(" ").split("\r").join(" ");
    let out: string[] = [];
    for (let w in unified.split(" ")) {
        if (w != "") {
            out.push(w);
        }
    }
    return out.join(" ");
}

// Join parts[start..] back together with `sep` (there's no array slice).
function join_from_sep(parts: string[], start: int, sep: string) -> string {
    let out: string[] = [];
    let i = start;
    while (i < parts.length()) {
        let p = match (parts.at(i)) {
            let v: string => v,
            null => "",
        };
        out.push(p);
        i = i + 1;
    }
    return out.join(sep);
}

// Fan out a skill-arena cohort: one pending cohort row plus one queued member
// task per branch, each inheriting the run directives; member ids recorded on
// the cohort for the fan-in reconciler (ports _create_cohort).
function create_cohort(
    prompt: string,
    branches: string[],
    channel: string,
    pin: string,
    cold: bool,
    source: string,
    slack_channel: string,
    slack_thread: string,
    slack_user: string,
) -> string {
    let svc = ingress_svc();
    let cohort_doc = baml
        .json
        .to_string(
            ArenaCohortDoc {
                prompt: prompt,
                skillRefs: branches,
                memberTaskIds: [],
                source: source,
                status: "pending",
                slackChannel: nz(slack_channel),
                slackThreadTs: nz(slack_thread),
                slackUser: nz(slack_user),
            },
        ) catch (_) {
        _ => "{}"
    };
    let cohort_id = svc.create("cohorts", cohort_doc) catch (_) {
        _ => ""
    };
    if (cohort_id == "") {
        return "";
    }
    let member_ids: string[] = [];
    for (let ref in branches) {
        let tid = create_directive_task(source, prompt, cohort_id, ref, channel, pin, cold);
        if (tid != "") {
            member_ids.push(tid);
        }
    }
    let patch = baml.json.to_string(CohortMembersPatch { memberTaskIds: member_ids }) catch (_) {
        _ => "{}"
    };
    let _ = svc.update("cohorts", cohort_id, patch) catch (_) {
        _ => ""
    };
    return cohort_id;
}

// One queued task carrying the parsed directives (createDoc strips nulls).
function create_directive_task(
    source: string,
    prompt: string,
    cohort_id: string,
    skill_ref: string,
    channel: string,
    pin: string,
    cold: bool,
) -> string {
    let doc = baml
        .json
        .to_string(
            DirectiveTaskDoc {
                source: source,
                prompt: prompt,
                status: "queued",
                cohortId: nz(cohort_id),
                skillRef: nz(skill_ref),
                bamlChannel: nz(channel),
                bamlPin: nz(pin),
                coldStart: if (cold) {
                    true
                } else {
                    null
                },
            },
        ) catch (_) {
        _ => ""
    };
    return create_doc("tasks", doc);
}

// Every reply carries the bammy2 signature so it's unmistakable you're talking
// to the BAML rewrite, not bammy1 (Python).
function bammy_signature() -> string {
    return "\n\n_🐑 bammy2 · pure BAML_";
}

function reply(token: string, channel: string, thread: string, msg: string) -> int {
    if (token == "" || channel == "") {
        return 0;
    }
    let _ = slack_post_message(token, channel, msg + bammy_signature(), thread);
    return 0;
}

// Detect "how were you built / what are you / are you the baml version" asks.
function is_about_question(text: string) -> bool {
    let t = text.to_lower_case();
    return t.includes("how were you built")
        || t.includes("how are you built")
        || t.includes("how were you made")
        || t.includes("how do you work")
        || t.includes("what are you made of")
        || t.includes("your architecture")
        || t.includes("are you baml")
        || t.includes("baml version")
        || t.includes("bammy2")
        || t.includes("bammy 2")
        || (t.includes("which version") && t.includes("bammy"));
}

// The bammy2 architecture pitch (Slack-formatted).
function architecture_reply() -> string {
    return "I'm *bammy2* — the pure-*BAML* rewrite of this pipeline. Unlike bammy1 (Python), every piece you're talking to is written in BAML (`.baml`), no Python in the path:\n\n• *This Slack handler* is a `baml.http.Server` (`servers/ingress.baml`) — it parses the event, classifies your mention, and replies.\n• *My brain* is `RouteMention(...)`, a typed BAML LLM function. It runs through a *claude-proxy* I call like an API (`/v1/messages`), which fulfills it on the Claude subscription — so no service holds an API key.\n• *The benchmark pipeline* (worker → trophy → dedup → issue) is BAML processors that all `implements Processor` — one claim-loop interface with default methods. The headline win: stages like dedup are now typed `Dedup(...) -> IssueList` LLM calls instead of agents writing JSON.\n• *State* lives in the same Convex DB, reached through a BAML `api` gateway (the sole Convex client) over its HTTP function API.\n• *Deployed on Fly* as separate apps: `bammy-2-service` (api + processors), `bammy-2-proxy` (LLM/agent gateway), `bammy-2-ingress` (me).\n\n42 `.baml` files, one pinned BAML nightly. Ask me `@bammy what can you do?` for the command list.";
}

// Create a bench task (or a skill-arena cohort) and reply with a dashboard link.
// Directives ([canary]/[baml=N]/[coldstart]/[skill arena]) are parsed off the
// prompt and ride the created task(s) — the old create_slack_task behavior.
function start_bench(
    text: string,
    channel: string,
    thread: string,
    user: string,
    token: string,
) -> int {
    let d = parse_run_directives(text);
    if (d.arena_branches.length() > 0) {
        let cid = create_cohort(
            d.cleaned,
            d.arena_branches,
            d.baml_channel,
            d.baml_pin,
            d.cold_start,
            "slack",
            channel,
            thread,
            user,
        );
        let msg = match (cid) {
            "" => "_On it, running that arena._",
            _ => {
                "_Arena run on "
                    + baml.unstable.string(d.arena_branches.length())
                    + " branches …_  <"
                    + ui_base_url()
                    + "/arena/"
                    + cid
                    + "|track on dashboard>"
            },
        };
        let _ = reply(token, channel, thread, msg);
        return 0;
    }
    let tid = create_slack_task(d.cleaned, channel, thread, user, d.baml_channel, d.baml_pin, d.cold_start);
    let msg = match (tid) {
        "" => "_On it, running that bench._",
        _ => {
            "_Running `"
                + head_str(d.cleaned, 120)
                + "` …_  <"
                + ui_base_url()
                + "/runs/"
                + tid
                + "|track on dashboard>"
        },
    };
    let _ = reply(token, channel, thread, msg);
    return 0;
}

function head_str(s: string, n: int) -> string {
    if (s.length() <= n) {
        return s;
    }
    return s.substring(0, n) catch (_) {
        _ => s
    };
}

function create_slack_task(
    prompt: string,
    channel: string,
    thread: string,
    user: string,
    baml_channel: string,
    baml_pin: string,
    cold: bool,
) -> string {
    let doc = baml
        .json
        .to_string(
            SlackTaskDoc {
                source: "slack",
                prompt: prompt,
                status: "queued",
                slackChannel: nz(channel),
                slackThreadTs: nz(thread),
                slackUser: nz(user),
                bamlChannel: nz(baml_channel),
                bamlPin: nz(baml_pin),
                coldStart: if (cold) {
                    true
                } else {
                    null
                },
            },
        ) catch (_) {
        _ => ""
    };
    return create_doc("tasks", doc);
}

// Feedback becomes a queued task (source=feedback) that flows into dedup.
function log_feedback(message: string, channel: string, thread: string, user: string) -> string {
    let doc = baml.json.to_string(FeedbackTaskDoc { source: "feedback", prompt: message, status: "queued" }) catch (_) {
        _ => ""
    };
    return create_doc("tasks", doc);
}

function create_doc(table: string, doc: string) -> string {
    if (doc == "") {
        return "";
    }
    return ingress_svc().create(table, doc) catch (_) {
        _ => ""
    };
}

// PostHog: NL -> HogQL -> run -> a compact reply.
function run_posthog(question: string) -> string {
    if (posthog_configured() == false) {
        return "PostHog isn't configured here.";
    }
    let q = WriteHogql(question, "", "") catch (_) {
        _ => null
    };
    return match (q) {
        null => "Couldn't translate that into a query.",
        let hq: Hogql => {
            let result = posthog_hogql(hq.hogql) catch (_) {
                _ => ""
            };
            match (result) {
                "" => "Query failed:\n```\n" + hq.hogql + "\n```",
                _ => "```sql\n" + hq.hogql + "\n```\n```\n" + tail_str(result, 1500) + "\n```",
            }
        },
    };
}

// ---- changelog: channel/version helpers (ports changelog_github) ------------
// Channels: nightly `baml-language-X.Y.Z-nightly.YYYYMMDD.<s>`, canary
// `baml-language-X.Y.Z`, legacy alpha `baml-language-X.Y.Z-alpha.N`, engine
// `v?X.Y.Z`. "" for unrelated tags (e.g. baml-wrapper-*).
function channel_of(tag: string) -> string {
    if (tag.starts_with("baml-language-")) {
        let ver = tag.substring(14, tag.length()) catch (_) {
            _ => ""
        };
        if (is_semverish(ver)) {
            return "canary";
        }
        if (ver.includes("-nightly.") && is_semverish(first_of(ver.split("-nightly.")))) {
            return "nightly";
        }
        if (ver.includes("-alpha.") && is_semverish(first_of(ver.split("-alpha.")))) {
            return "alpha";
        }
        return "";
    }
    let bare = if (tag.starts_with("v")) {
        tag.substring(1, tag.length()) catch (_) {
            _ => tag
        }
    } else {
        tag
    };
    if (is_semverish(bare)) {
        return "engine";
    }
    return "";
}

// Strip the `baml-language-` prefix (and a `v` prefix on engine tags).
function normalize_tag(tag: string) -> string {
    if (tag.starts_with("baml-language-")) {
        return tag.substring(14, tag.length()) catch (_) {
            _ => tag
        };
    }
    if (tag.starts_with("v")) {
        return tag.substring(1, tag.length()) catch (_) {
            _ => tag
        };
    }
    return tag;
}

// X.Y.Z — three dot-separated integer parts.
function is_semverish(s: string) -> bool {
    let parts = s.split(".");
    if (parts.length() != 3) {
        return false;
    }
    for (let p in parts) {
        if (p == "" || is_all_digits(p) == false) {
            return false;
        }
    }
    return true;
}

function is_all_digits(s: string) -> bool {
    if (s == "") {
        return false;
    }
    // split("") brackets the chars with empty strings — skip them.
    for (let ch in s.split("")) {
        if (ch != "" && (ch < "0" || ch > "9")) {
            return false;
        }
    }
    return true;
}

function first_of(parts: string[]) -> string {
    return match (parts.at(0)) {
        let v: string => v,
        null => "",
    };
}

// Minimal read of an existing entry row.
class EntryRow {
    _id: string?,
    version: string?,
    date: string?,
    title: string?,
    body: string?,
    authors: string[]?,
    channel: string?,
    status: string?,
    createdAt: float?,
}

class EntryRevisePatch {
    reviseMode: string,
    reviseGuidance: string,
}

class EntryRegenPatch {
    tag: string,
    fromRelease: string?,
    reviseMode: string,
}

class EntryCreateDoc {
    version: string,
    tag: string,
    channel: string,
    fromRelease: string?,
    status: string,
}

// changelog_sync (ports sync_missing_entries): list recent GitHub releases,
// skip versions that already have an entry row IN ANY STATUS, enqueue the rest
// oldest-first. Returns how many were enqueued.
function sync_changelog() -> int {
    let parts = baml_repo_slug().split("/");
    let owner = match (parts.at(0)) {
        let v: string => v,
        null => "BoundaryML",
    };
    let repo = match (parts.at(1)) {
        let v: string => v,
        null => "baml",
    };
    let releases = github_releases_wide(github_token(), owner, repo) catch (_) {
        _ => "[]"
    };
    let tags = extract_release_tags(releases);

    // Existing versions, any status (empty field => unfiltered newest-first list).
    let existing_raw = ingress_svc().list("changelogEntries", "", "", "", 1000) catch (_) {
        _ => "[]"
    };
    let existing: EntryRow[] = baml.json.from_string<EntryRow[]>(existing_raw) catch (_) {
        _ => []
    };
    let known: string[] = [];
    for (let row in existing) {
        match (row.version) {
            let v: string => {
                if (v != "") {
                    known.push(v);
                }
            },
            null => null,
        };
    }

    let n = 0;
    // Oldest-first (the GitHub listing is newest-first).
    let i = tags.length() - 1;
    while (i >= 0) {
        let tag = match (tags.at(i)) {
            let v: string => v,
            null => "",
        };
        i = i - 1;
        let channel = channel_of(tag);
        if (tag == "" || channel == "") {
            baml.io.println("changelog sync: skipping unrelated tag " + tag);
        } else {
            let version = normalize_tag(tag);
            if (in_list(known, version) == false) {
                let doc = baml
                    .json
                    .to_string(
                        EntryCreateDoc {
                            version: version,
                            tag: tag,
                            channel: channel,
                            fromRelease: null,
                            status: "queued",
                        },
                    ) catch (_) {
                    _ => ""
                };
                if (doc != "") {
                    let id = create_doc("changelogEntries", doc);
                    if (id != "") {
                        known.push(version);
                        n = n + 1;
                    }
                }
            }
        }
    }
    return n;
}

// Look up an entry row by (normalized) version. Null when absent.
function entry_by_version(version: string) -> EntryRow? {
    let raw = ingress_svc().list("changelogEntries", "version", version, "by_version", 1) catch (_) {
        _ => "[]"
    };
    let rows: EntryRow[] = baml.json.from_string<EntryRow[]>(raw) catch (_) {
        _ => []
    };
    return rows.at(0);
}

// Re-enqueue a published changelog entry for revise/regenerate (changelog_edit
// route + POST /entries/{version}/revise). Finds the row by version (trying the
// normalized form too), stamps reviseMode/guidance, and flips it back to queued
// — the worker's claim is the per-version lock. Returns 1 on success.
function requeue_changelog(version_ref: string, mode: string, guidance: string) -> int {
    let row = match (entry_by_version(version_ref.trim())) {
        let r: EntryRow => r,
        null => match (entry_by_version(normalize_tag(version_ref.trim()))) {
            let r: EntryRow => r,
            null => {
                baml.io.eprintln("requeue_changelog: no entry for " + version_ref);
                return 0;
            },
        },
    };
    let id = match (row._id) {
        let v: string => v,
        null => {
            return 0;
        },
    };
    let cur = match (row.status) {
        let s: string => s,
        null => "",
    };
    if (cur == "queued" || cur == "generating") {
        baml.io.eprintln("requeue_changelog: generation already in flight for " + version_ref);
        return 0;
    }
    let m = if (mode == "regenerate") {
        "regenerate"
    } else {
        "revise"
    };
    let patch = baml.json.to_string(EntryRevisePatch { reviseMode: m, reviseGuidance: guidance }) catch (_) {
        _ => "{}"
    };
    let _ = ingress_svc().update("changelogEntries", id, patch) catch (_) {
        _ => ""
    };
    let _ = ingress_svc().transition("changelogEntries", id, "queued", "status", "{}", true) catch (_) {
        _ => ""
    };
    return 1;
}

// Pull all release tag_names out of the GitHub releases JSON (the page size is
// the window; sync_changelog dedups by version so a wide window is safe).
function extract_release_tags(releases: string) -> string[] {
    let out: string[] = [];
    let segs = releases.split("\"tag_name\":\"");
    let i = 1;
    while (i < segs.length()) {
        let seg = match (segs.at(i)) {
            let v: string => v,
            null => "",
        };
        let tag = match (seg.split("\"").at(0)) {
            let v: string => v,
            null => "",
        };
        if (tag != "") {
            out.push(tag);
        }
        i = i + 1;
    }
    return out;
}

// Recent changelog versions for classifier grounding (best-effort; "" on failure).
function changelog_catalog() -> string {
    let raw = ingress_svc().get("changelogEntries?field=channel&value=nightly&index=by_channel", "list") catch (_) {
        _ => ""
    };
    return match (raw) {
        let r: string => r,
        null => "",
    };
}

function nz(s: string) -> string? {
    return match (s) {
        "" => null,
        _ => s,
    };
}

function help_text() -> string {
    return "Hi! Here's what I can do:\n- *Run a bench task*: `@bammy try building a recipe parser with baml` (directives: `[canary]` `[nightly]` `[baml=N]` `[coldstart]` `[skill arena]`)\n- *Edit the changelog*: `@bammy fix the title of the latest nightly`\n- *Catch the changelog up*: `@bammy add new changelog entries`\n- *T-shirt code*: `@bammy promo code please`\n- *Query PostHog*: `@bammy how many signups did we get this week?`\n- *Log feedback*: `@bammy feedback: baml fmt is slow on big files`";
}

// ---- /entries endpoints (public read + token-gated ops writes) -------------

// The website build's contract: {"entries":[{version,date,title,body,authors,
// channel}]} — published (status=done) entries, newest-first by (date, createdAt).
function handle_entries_list() -> baml.http.Response {
    let raw = ingress_svc().list("changelogEntries", "status", "done", "by_status_created", 1000) catch (_) {
        _ => "[]"
    };
    let rows: EntryRow[] = baml.json.from_string<EntryRow[]>(raw) catch (_) {
        _ => []
    };
    // Selection sort, newest-first by (date, createdAt) — n is small (<=1000).
    let keys: string[] = [];
    for (let r in rows) {
        keys.push(entry_sort_key(r));
    }
    let order: int[] = [];
    let used: int[] = [];
    let n = rows.length();
    let filled = 0;
    while (filled < n) {
        let best = 0 - 1;
        let best_key = "";
        let j = 0;
        while (j < n) {
            if (in_int_list(used, j) == false) {
                let k = match (keys.at(j)) {
                    let v: string => v,
                    null => "",
                };
                if (best < 0 || k > best_key) {
                    best = j;
                    best_key = k;
                }
            }
            j = j + 1;
        }
        order.push(best);
        used.push(best);
        filled = filled + 1;
    }
    let items: string[] = [];
    for (let idx in order) {
        match (rows.at(idx)) {
            let r: EntryRow => items.push(entry_json(r)),
            null => null,
        };
    }
    return json_response(200, "{\"entries\":[" + items.join(",") + "]}");
}

function in_int_list(xs: int[], v: int) -> bool {
    for (let x in xs) {
        if (x == v) {
            return true;
        }
    }
    return false;
}

// Sort key: date (YYYY-MM-DD, lexical) then zero-padded createdAt ms.
function entry_sort_key(r: EntryRow) -> string {
    let date = match (r.date) {
        let d: string => d,
        null => "",
    };
    let created = match (r.createdAt) {
        let c: float => f2i(c),
        null => 0,
    };
    return date + "|" + pad_left(baml.unstable.string(created), 15);
}

function pad_left(s: string, width: int) -> string {
    let out = s;
    while (out.length() < width) {
        out = "0" + out;
    }
    return out;
}

function entry_json(r: EntryRow) -> string {
    let authors: string[] = match (r.authors) {
        let a: string[] => a,
        null => [],
    };
    return "{\"version\":"
        + jesc(str_or(r.version, ""))
        + ",\"date\":"
        + jesc(str_or(r.date, ""))
        + ",\"title\":"
        + jesc(str_or(r.title, ""))
        + ",\"body\":"
        + jesc(str_or(r.body, ""))
        + ",\"authors\":"
        + json_str_array(authors)
        + ",\"channel\":"
        + jesc(str_or(r.channel, ""))
        + "}";
}

function str_or(v: string?, fallback: string) -> string {
    return match (v) {
        let s: string => s,
        null => fallback,
    };
}

class EntriesCreateBody {
    release: string?,
    from_release: string?,
}

class EntriesReviseBody {
    guidance: string?,
}

// POST /entries — enqueue generation for a release tag (409 when in flight;
// an existing done/failed row is flipped back to queued as a regenerate).
function handle_entries_create(req: baml.http.Request) -> baml.http.Response {
    if (service_token_ok(req) == false) {
        return json_response(401, "{\"error\":\"unauthorized\"}");
    }
    let body = baml.json.from_string<EntriesCreateBody>(req.body) catch (_) {
        _ => EntriesCreateBody { release: null, from_release: null }
    };
    let tag = match (body.release) {
        let t: string => t.trim(),
        null => "",
    };
    if (tag == "") {
        return json_response(400, "{\"error\":\"release required\"}");
    }
    let channel = channel_of(tag);
    if (channel == "") {
        return json_response(400, "{\"error\":\"unrecognized release tag: " + tag + "\"}");
    }
    let version = normalize_tag(tag);
    match (entry_by_version(version)) {
        let row: EntryRow => {
            let cur = str_or(row.status, "");
            if (cur == "queued" || cur == "generating") {
                return json_response(409, "{\"error\":\"generation already in flight for " + version + "\"}");
            }
            let id = str_or(row._id, "");
            let patch = baml
                .json
                .to_string(EntryRegenPatch { tag: tag, fromRelease: body.from_release, reviseMode: "regenerate" }) catch (_) {
                _ => "{}"
            };
            let _ = ingress_svc().update("changelogEntries", id, patch) catch (_) {
                _ => ""
            };
            let _ = ingress_svc().transition("changelogEntries", id, "queued", "status", "{}", true) catch (_) {
                _ => ""
            };
            return json_response(200, "{\"id\":" + jesc(id) + "}");
        },
        null => {
            let doc = baml
                .json
                .to_string(
                    EntryCreateDoc {
                        version: version,
                        tag: tag,
                        channel: channel,
                        fromRelease: body.from_release,
                        status: "queued",
                    },
                ) catch (_) {
                _ => ""
            };
            let id = create_doc("changelogEntries", doc);
            return json_response(200, "{\"id\":" + jesc(id) + "}");
        },
    };
}

// POST /entries/update — sync now: enqueue entries for any missed releases.
function handle_entries_update(req: baml.http.Request) -> baml.http.Response {
    if (service_token_ok(req) == false) {
        return json_response(401, "{\"error\":\"unauthorized\"}");
    }
    let n = sync_changelog();
    return json_response(200, "{\"enqueued\":" + baml.unstable.string(n) + "}");
}

// POST /entries/{version}/revise — requeue with revise guidance.
function handle_entries_revise(req: baml.http.Request, version: string) -> baml.http.Response {
    if (service_token_ok(req) == false) {
        return json_response(401, "{\"error\":\"unauthorized\"}");
    }
    let body = baml.json.from_string<EntriesReviseBody>(req.body) catch (_) {
        _ => EntriesReviseBody { guidance: null }
    };
    let guidance = match (body.guidance) {
        let g: string => g.trim(),
        null => "",
    };
    if (guidance == "") {
        return json_response(400, "{\"error\":\"guidance required\"}");
    }
    match (entry_by_version(version)) {
        null => {
            return json_response(404, "{\"error\":\"no entry for " + version + "\"}");
        },
        let row: EntryRow => {
            let cur = str_or(row.status, "");
            if (cur == "queued" || cur == "generating") {
                return json_response(409, "{\"error\":\"generation already in flight for " + version + "\"}");
            }
            let id = str_or(row._id, "");
            let patch = baml.json.to_string(EntryRevisePatch { reviseMode: "revise", reviseGuidance: guidance }) catch (_) {
                _ => "{}"
            };
            let _ = ingress_svc().update("changelogEntries", id, patch) catch (_) {
                _ => ""
            };
            let _ = ingress_svc().transition("changelogEntries", id, "queued", "status", "{}", true) catch (_) {
                _ => ""
            };
            return json_response(200, "{\"id\":" + jesc(id) + "}");
        },
    };
}

// ---- router + entrypoint ---------------------------------------------------

function ingress_handler(req: baml.http.Request) -> baml.http.Response {
    let path = path_only(req.url);
    if (req.method == "GET" && path == "/healthz") {
        return text_response(200, "ok");
    }
    if (req.method == "GET" && path == "/entries") {
        return handle_entries_list();
    }
    if (req.method == "POST" && path == "/entries") {
        return handle_entries_create(req);
    }
    if (req.method == "POST" && path == "/entries/update") {
        return handle_entries_update(req);
    }
    // POST /entries/{version}/revise
    if (req.method == "POST" && path.starts_with("/entries/") && path.ends_with("/revise")) {
        let inner = path.substring(9, path.length() - 7) catch (_) {
            _ => ""
        };
        if (inner != "" && inner.includes("/") == false) {
            return handle_entries_revise(req, inner);
        }
    }
    if (req.method == "POST" && path == "/bug") {
        return handle_bug(req);
    }
    if (req.method == "POST" && path == "/feedback") {
        return handle_feedback(req);
    }
    if (req.method == "POST" && path == "/slack/events") {
        return handle_slack(req);
    }
    if (req.method == "POST" && path == "/linear/webhook") {
        return handle_linear(req);
    }
    return json_response(404, "{\"error\":\"not found\"}");
}

function Ingress() -> int throws baml.errors.Io {
    let addr = "[::]:" + env_or("INGRESS_PORT", "8081");
    let server = baml.http.Server.bind(addr);
    baml.io.println("ingress listening on " + server.addr);
    return server.serve(ingress_handler);
}
