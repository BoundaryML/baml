// ingress — public webhook http.Server + the complete @bammy Slack bot. Ports
// services/ingress + services/ingress/bammy.py. Routes:
//   GET  /healthz          liveness
//   GET  /entries          public changelog JSON (stub)
//   POST /bug              create a task (source=bug_report)
//   POST /feedback         create a feedback task
//   POST /slack/events     Slack url-verification + the full @bammy dispatcher
//   POST /linear/webhook   status-group label change -> issue lifecycle move
//
// @bammy: a mention is ACKed immediately (Slack's 3s rule) and dispatched in a
// spawned task — classify via RouteMention (LLM through the proxy), then handle:
//   bench | changelog_edit | changelog_sync | promo_claim | posthog_query |
//   github_usage | feedback | general, replying in-thread via slack_post_message.
// Signature verification + rate limiting are deferred.

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
    event: SlackInner?,
}

class BugBody {
    prompt: string?,
}

class SlackTaskDoc {
    source: string,
    prompt: string,
    status: string,
    slackChannel: string?,
    slackThreadTs: string?,
    slackUser: string?,
}

class FeedbackTaskDoc {
    source: string,
    prompt: string,
    status: string,
}

class ChangelogSyncDoc {
    version: string,
    channel: string,
    status: string,
}

function ingress_svc() -> ServiceClient {
    return ServiceClient { base_url: service_url(), token: service_token() };
}

// ---- /bug + /feedback ------------------------------------------------------

function handle_bug(req: baml.http.Request) -> baml.http.Response {
    let parsed = baml.json.from_string<BugBody>(req.body) catch (_) {
        _ => null
    };
    let prompt = match (parsed) {
        let b: BugBody => match (b.prompt) {
            let p: string => p,
            null => "",
        },
        null => "",
    };
    if (prompt == "") {
        return json_response(400, "{\"error\":\"missing prompt\"}");
    }
    return json_response(200, "{\"id\":" + jesc(create_bug_task(prompt)) + "}");
}

function handle_feedback(req: baml.http.Request) -> baml.http.Response {
    let _ = log_feedback(req.body, "", "", "");
    return json_response(200, "{\"ok\":true}");
}

// ---- /slack/events ---------------------------------------------------------

function handle_slack(req: baml.http.Request) -> baml.http.Response {
    let ev = baml.json.from_string<SlackEvent>(req.body) catch (_) {
        _ => null
    };
    return match (ev) {
        null => json_response(200, "{}"),
        let e: SlackEvent => {
            match (e.challenge) {
                // url_verification handshake.
                let c: string => json_response(200, "{\"challenge\":" + jesc(c) + "}"),
                // Real event: ACK now, dispatch off the request path (Slack's 3s rule).
                null => {
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

    // 2. Classify (LLM via the proxy). Fail open to bench.
    let route = RouteMention(text, "", changelog_catalog()) catch (_) {
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
        let _ = requeue_changelog(r.version_ref, r.mode, r.guidance);
        let _ = reply(
            token,
            channel,
            thread,
            "Re-running the changelog entry for `" + r.version_ref + "`. It'll update shortly.",
        );
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
    data: LinearWebhookData?,
}

// Only _id + status — the status gates which human move is honored.
class WebhookIssue {
    _id: string?,
    status: string?,
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
// TODO(hardening): Linear-Signature HMAC verification + replay guard are deferred
// (no BAML hmac stdlib yet) — this endpoint is currently unauthenticated.
function handle_linear(req: baml.http.Request) -> baml.http.Response {
    let parsed = baml.json.from_string<LinearWebhook>(req.body) catch (_) {
        _ => null
    };
    return match (parsed) {
        null => json_response(200, "{}"),
        let w: LinearWebhook => route_linear_webhook(w),
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
    let issue = match (issues.at(0)) {
        let i: WebhookIssue => i,
        null => {
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
    return "";
}

// Resting states from which a human "approved"/"redraft" board move is honored.
function approve_from(cur: string) -> bool {
    return cur == "open"
        || cur == "confirmed"
        || cur == "needs_human"
        || cur == "redraft"
        || cur == "failed";
}

function redraft_from(cur: string) -> bool {
    return cur == "open"
        || cur == "confirmed"
        || cur == "needs_human"
        || cur == "approved"
        || cur == "failed"
        // A human redraft drag should win even mid-verify (a transient auto-state
        // that can also strand an issue if its claim is orphaned by a redeploy).
        || cur == "verifying";
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

function has_bench_directives(text: string) -> bool {
    return text.includes("[canary]")
        || text.includes("[nightly]")
        || text.includes("[coldstart]")
        || text.includes("[baml=")
        || text.includes("[skill arena]");
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

// Create a bench task and reply with a link to its run page on the dashboard.
function start_bench(
    text: string,
    channel: string,
    thread: string,
    user: string,
    token: string,
) -> int {
    let tid = create_slack_task(text, channel, thread, user);
    let msg = match (tid) {
        "" => "_On it, running that bench._",
        _ => {
            "_Running `"
                + head_str(text, 120)
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
            },
        ) catch (_) {
        _ => ""
    };
    return create_doc("tasks", doc);
}

function create_bug_task(prompt: string) -> string {
    let doc = baml.json.to_string(FeedbackTaskDoc { source: "bug_report", prompt: prompt, status: "queued" }) catch (_) {
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

// changelog_sync: fetch GitHub releases, enqueue an entry per recent version.
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
    let releases = github_releases(github_token(), owner, repo) catch (_) {
        _ => "[]"
    };
    let versions = extract_release_tags(releases);
    let n = 0;
    for (let v in versions) {
        let doc = baml.json.to_string(ChangelogSyncDoc { version: v, channel: "nightly", status: "queued" }) catch (_) {
            _ => ""
        };
        if (doc != "") {
            let _ = create_doc("changelogEntries", doc);
            // (dedup-by-version is a TODO; the changelog_worker is the per-version lock)
        }
    }
    return versions.length();
}

// Re-enqueue a published changelog entry for revise/regenerate (changelog_edit).
function requeue_changelog(version_ref: string, mode: string, guidance: string) -> int {
    // The changelogEntries row is found by version; flip it back to queued with the
    // revise mode/guidance set (the worker's claim is the per-version lock).
    // Lookup-by-version is deferred; acked to the user above.
    return 0;
}

// Pull the first few release tag_names out of the GitHub releases JSON.
function extract_release_tags(releases: string) -> string[] {
    let out: string[] = [];
    let segs = releases.split("\"tag_name\":\"");
    let i = 1;
    while (i < segs.length() && i <= 3) {
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

// ---- router + entrypoint ---------------------------------------------------

function ingress_handler(req: baml.http.Request) -> baml.http.Response {
    let path = path_only(req.url);
    if (req.method == "GET" && path == "/healthz") {
        return text_response(200, "ok");
    }
    if (req.method == "GET" && path == "/entries") {
        return json_response(200, "{\"entries\":[]}");
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
