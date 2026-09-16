// The pipeline's own store: Supabase (PostgREST) tables for every model
// the pipeline passes around (see README, "Tables").
//
//   FEEDBACK_SUPABASE_URL   https://igraichzcidsylvzkjlc.supabase.co
//   FEEDBACK_SUPABASE_KEY   the service key (the pipeline writes)
//
// Same variables the eval dataset uses (create_issue.baml supabase_get);
// when they are unset every save is a no-op and the pipeline runs as
// before, from files under ~/.atb2. Row shapes are pure functions with
// token-free tests; the HTTP edge is thin.

// ==================== PostgREST ====================

/// The store could not be reached or refused a request. Thrown, not
/// panicked, so a loop can log it and go on with the next run.
class StoreError {
    message: string,
}

/// The store is configured: reads and writes go to Supabase.
function db_configured() -> bool {
    supabase_configured()
}

/// Which rows this process writes: "live" (real reports, the default) or
/// "eval" (run_tests.sh exports ATB2_DATASET=eval, so nothing an eval
/// writes can pass for a real user's report).
function dataset() -> string {
    if ((baml.env.get("ATB2_DATASET") ?? "live").trim().to_lower_case() == "eval") {
        "eval"
    } else {
        "live"
    }
}

/// One PostgREST request. `prefer` is the Prefer header (upsert / return
/// shape); the body is JSON text. Panics on transport or non-2xx, like
/// supabase_get: a store that is configured but broken must not be
/// silently skipped.
function supabase_send(
    method: string,
    path: string,
    body: string,
    prefer: string,
    timeout_s: int = 30,
) -> string {
    let base = baml.env.get("FEEDBACK_SUPABASE_URL") ?? "";
    if (!base.starts_with("https://")) {
        // the service key rides in two headers: never over plain http
        throw StoreError { message: "FEEDBACK_SUPABASE_URL must be https" };
    };
    let url = base + "/rest/v1/" + path;
    let key = baml.env.get("FEEDBACK_SUPABASE_KEY") ?? "";
    let headers: map<string, string> = {
        "apikey": key,
        "Authorization": "Bearer " + key,
        "Accept": "application/json",
        "Content-Type": "application/json",
    };
    if (prefer.length() > 0) {
        headers.set("Prefer", prefer);
    };
    let res = baml.http.send(
        baml.http.Request { method: method, url: url, headers: headers, body: body },
        timeout = baml.time.Duration.from_seconds(timeout_s),
    ) catch (e) {
        _ => {
            throw StoreError { message: method + " failed for " + url };
        },
    };
    let text = res.text() catch (_) {
        _ => "",
    };
    if (!res.ok()) {
        throw StoreError {
            message: res.status_code.to_string() + " for " + method + " " + url + ": " + tail(text, 3),
        };
    };
    text
}

/// Which dataset an id belongs to: eval rows are the reference issues
/// (GH-<number>); everything else (ISSUE-, PH-, SL-) is live. The database
/// enforces the same rule with a check constraint (see the PR), so an eval
/// run cannot upsert over a live row or the other way round.
function dataset_of_id(id: string) -> string {
    if (id.starts_with("GH-")) {
        "eval"
    } else {
        "live"
    }
}

/// Refuses a write whose id belongs to the other dataset.
function guard_dataset(id: string) -> null {
    if (dataset_of_id(id) != dataset()) {
        throw StoreError {
            message: "id " + id + " belongs to dataset " + dataset_of_id(id) + ", this run writes " + dataset(),
        };
    };
    null
}

/// Insert-or-update on the primary key. `rows` is a JSON array. Every read
/// filters on dataset and every write is guarded (guard_dataset), and the
/// database checks the id/dataset pairing too.
function upsert(table: string, rows: json) -> null {
    supabase_send(
        "POST",
        table + "?on_conflict=id",
        baml.json.stringify(rows),
        "resolution=merge-duplicates,return=minimal",
    );
    null
}

/// Insert one row; returns it as stored (with its generated id).
function insert(table: string, row: json) -> json {
    let text = supabase_send("POST", table, baml.json.stringify([row]), "return=representation");
    rows_of(text).at(0) ?? baml.json.parse("{}")
}

/// `GET table?<query>` as rows.
function select(table: string, query: string) -> json[] {
    rows_of(supabase_send("GET", table + "?" + query, "", ""))
}

/// A PostgREST body (a JSON array) as rows; anything else is no rows.
function rows_of(text: string) -> json[] {
    baml.json.from_string<json[]>(text) catch (_) {
        _ => [],
    }
}

/// `PATCH table?<filter>` with a partial row.
function update(table: string, filter: string, patch: json) -> null {
    supabase_send("PATCH", table + "?" + filter, baml.json.stringify(patch), "return=minimal");
    null
}

/// A value in a PostgREST filter: `eq.<value>` with reserved characters
/// escaped the way PostgREST wants (quoted).
function eq(value: string) -> string {
    "eq." + url_encode(value)
}

/// Percent-encodes the characters that would break a query string or a
/// PostgREST filter. Ids, states and PR urls are ASCII.
function url_encode(s: string) -> string {
    let out = s.replace_all("%", "%25");
    for (
        let pair in [
            [" ", "%20"],
            ["&", "%26"],
            ["=", "%3D"],
            ["?", "%3F"],
            ["#", "%23"],
            ["+", "%2B"],
            [",", "%2C"],
            ["/", "%2F"],
            [":", "%3A"],
            ["(", "%28"],
            [")", "%29"],
            ["\"", "%22"],
        ]
    ) {
        out = out.replace_all(pair.at(0) ?? "", pair.at(1) ?? "");
    }
    out
}

// ==================== Rows ====================

/// A `feedback` row.
function feedback_row(fb: Feedback, origin: json) -> json {
    {
        "id": fb.id,
        "title": fb.title,
        "body": fb.body,
        "source": fb.source.to_string(),
        "author_email": fb.author.email,
        "author_github": fb.author.github,
        "author_device_id": fb.author.device_id,
        "toolchain": fb.toolchain,
        "files": fb.files.to_json(),
        "comments": fb.comments.to_json(),
        "issue_ids": fb.issue_ids.to_json(),
        "origin": origin,
        "dataset": dataset(),
    }
        .to_json()
}

/// A Feedback from a `feedback` row.
function feedback_from_row(row: json) -> Feedback {
    Feedback {
        id: baml.json.path_or<string>(row, ".id", ""),
        title: baml.json.path_or<string>(row, ".title", ""),
        body: baml.json.path_or<string>(row, ".body", ""),
        source: source_from(baml.json.path_or<string>(row, ".source", "BamlFeedback")),
        author: Author {
            email: baml.json.path_or<string?>(row, ".author_email", null),
            github: baml.json.path_or<string?>(row, ".author_github", null),
            device_id: baml.json.path_or<string?>(row, ".author_device_id", null),
        },
        toolchain: baml.json.path_or<string?>(row, ".toolchain", null),
        files: baml.json.path_or<map<string, string>>(row, ".files", {}),
        comments: baml.json.path_or<Comment[]>(row, ".comments", []),
        issue_ids: baml.json.path_or<string[]>(row, ".issue_ids", []),
    }
}

/// An `issues` row. `state` is generated by the database from `status`.
function issue_row(issue: Issue) -> json {
    {
        "id": issue.id,
        "kind": issue_kind(issue).to_string().to_lower_case(),
        "title": issue.title,
        "description": issue.description,
        "shepherd": issue.shepherd,
        "subsystem": issue.subsystem.to_string(),
        "repros": issue.repros.to_json(),
        "version": issue.version,
        "feedback_ids": issue.feedback_ids.to_json(),
        "status": issue.status.to_json(),
        "comments": issue.comments.to_json(),
        "resolution_plan": issue.resolution_plan,
        "difficulty": match (issue.difficulty) {
            null => null,
            let d: Difficulty => d.to_string(),
        },
        "design_doc": issue.design_doc,
        "slack_ts": issue.slack_ts,
        "slack_channel": issue.slack_channel,
        "dataset": dataset(),
    }
        .to_json()
}

/// An Issue from an `issues` row (extra columns such as `state`,
/// `created_at` and a joined `outcome` are ignored).
function issue_from_row(row: json) -> Issue {
    Issue {
        id: baml.json.path_or<string>(row, ".id", ""),
        kind: if (baml.json.path_or<string>(row, ".kind", "bug") == "feature") {
            IssueKind.Feature
        } else {
            IssueKind.Bug
        },
        title: baml.json.path_or<string>(row, ".title", ""),
        description: baml.json.path_or<string>(row, ".description", ""),
        shepherd: baml.json.path_or<string?>(row, ".shepherd", null),
        subsystem: subsystem_from(baml.json.path_or<string>(row, ".subsystem", "Unknown")),
        repros: baml.json.path_or<Repro[]>(row, ".repros", []),
        version: baml.json.path_or<string>(row, ".version", ""),
        feedback_ids: baml.json.path_or<string[]>(row, ".feedback_ids", []),
        status: baml.json.path_or<IssueStatus>(row, ".status", Open { state: "open" }),
        comments: baml.json.path_or<Comment[]>(row, ".comments", []),
        resolution_plan: baml.json.path_or<string?>(row, ".resolution_plan", null),
        difficulty: difficulty_from(baml.json.path_or<string?>(row, ".difficulty", null)),
        design_doc: baml.json.path_or<string?>(row, ".design_doc", null),
        slack_ts: baml.json.path_or<string?>(row, ".slack_ts", null),
        slack_channel: baml.json.path_or<string?>(row, ".slack_channel", null),
    }
}

/// A `runs` row from a handle_issue outcome.
function run_row(issue_id: string, o: HandleOutcome, mode: HandleMode) -> json {
    {
        "issue_id": issue_id,
        "branch": o.branch,
        "kind": o.kind,
        "pr": o.pr,
        "turns": o.turns,
        "seconds": o.seconds,
        "timed_out": o.timed_out,
        "gate": match (o.gate) {
            null => null,
            let g: GateResult => g.to_json(),
        },
        "report": match (o.report) {
            null => null,
            let r: FixReport => r.to_json(),
        },
        "design_doc": o.design_doc,
        "reason": o.reason,
        "mode": mode.to_string(),
        "dataset": dataset(),
    }
        .to_json()
}

function source_from(s: string) -> FeedbackSource {
    match (s) {
        "Github" => FeedbackSource.Github,
        "Slack" => FeedbackSource.Slack,
        _ => FeedbackSource.BamlFeedback,
    }
}

function subsystem_from(s: string) -> Subsystem {
    match (s) {
        "Syntax" => Subsystem.Syntax,
        "Compiler" => Subsystem.Compiler,
        "Runtime" => Subsystem.Runtime,
        "StdLibrary" => Subsystem.StdLibrary,
        "Tooling" => Subsystem.Tooling,
        _ => Subsystem.Unknown,
    }
}

function difficulty_from(s: string?) -> Difficulty? {
    match (s) {
        "Trivial" => Difficulty.Trivial,
        "Easy" => Difficulty.Easy,
        "Medium" => Difficulty.Medium,
        "Hard" => Difficulty.Hard,
        _ => null,
    }
}

// ==================== Saves and loads ====================
// Every save is a no-op when the store is not configured, so the pipeline
// still runs from files alone.

function save_feedback(fb: Feedback, origin: json) -> null {
    if (!db_configured()) {
        return null;
    };
    guard_dataset(fb.id);
    upsert("feedback", [feedback_row(fb, origin)].to_json());
    null
}

/// Reports already in the store, by id: what an intake must not re-add.
/// The toolchains the linked reports said the bug fails on, oldest report
/// first, deduplicated; empty without the store or when no report said.
function reported_versions(issue: Issue) -> string[] {
    let out: string[] = [];
    if (!db_configured()) {
        return out;
    };
    for (let id in issue.feedback_ids) {
        let rows = select("feedback", "select=toolchain&id=" + eq(id) + "&dataset=" + eq(dataset()) + "&limit=1");
        match (rows.at(0)) {
            null => null,
            let r: json => {
                let v = (baml.json.path_or<string?>(r, ".toolchain", null) ?? "").trim();
                if (v.length() > 0 && !out.includes(v)) {
                    out.push(v);
                };
            },
        };
    }
    out
}

/// "fails on A, B · fixed on C" (pure). Falls back to the issue's own
/// last-seen-broken version when no report named one.
function fails_fixed_text(reported: string[], last_broken: string, fixed: string) -> string {
    let fails = if (reported.length() > 0) {
        reported.join(", ")
    } else {
        last_broken
    };
    "fails on " + fails + " · fixed on " + fixed
}

function known_feedback_ids() -> string[] {
    if (!db_configured()) {
        return [];
    };
    rows_field(select("feedback", "select=id&dataset=" + eq(dataset()) + "&order=received_at"), "id")
}

function save_issue(issue: Issue) -> null {
    if (!db_configured()) {
        return null;
    };
    guard_dataset(issue.id);
    let row = issue_row(issue);
    if (dataset() == "eval") {
        upsert("issues", [row].to_json());
        return null;
    };
    let inserted = rows_of(
        supabase_send(
            "POST",
            "issues?on_conflict=id",
            baml.json.stringify([row]),
            "resolution=ignore-duplicates,return=representation",
        ),
    );
    if (inserted.length() == 0) {
        // These collections can change while an agent holds an older snapshot.
        // Their writers append explicitly; lifecycle saves must not replace them.
        let fields = baml.json.from_json<map<string, json>>(row);
        let patch: map<string, json> = {};
        for (let key in fields.keys()) {
            if (!["comments", "repros", "feedback_ids", "slack_ts", "slack_channel"].includes(key)) {
                patch.set(key, fields.get(key) ?? baml.json.parse("null"));
            };
        }
        update(
            "issues",
            "id=" + eq(issue.id) + "&dataset=" + eq(dataset()) + "&state=neq.cancelled",
            patch.to_json(),
        );
    };
    null
}

function load_issue(id: string) -> Issue? {
    if (!db_configured()) {
        return null;
    };
    let rows = select("issues", "select=*&id=" + eq(id) + "&dataset=" + eq(dataset()) + "&limit=1");
    match (rows.at(0)) {
        null => null,
        let r: json => issue_from_row(r),
    }
}

/// Issues in a state ("open", "in_progress", ...), oldest first.
function load_issues_in(state: string) -> Issue[] {
    if (!db_configured()) {
        return [];
    };
    select("issues", "select=*&state=" + eq(state) + "&dataset=" + eq(dataset()) + "&order=created_at")
        .map((r: json) -> Issue {
            issue_from_row(r)
        })
}

function save_run(issue: Issue, o: HandleOutcome, mode: HandleMode) -> null {
    if (!db_configured()) {
        return null;
    };
    insert("runs", run_row(issue.id, o, mode));
    null
}

function save_merge_round(
    issue_id: string?,
    pr: string,
    round: int,
    kind: string,
    handled: int[],
    brief: string,
) -> null {
    if (!db_configured()) {
        return null;
    };
    insert(
        "merge_rounds",
        {
            "issue_id": issue_id,
            "pr": pr,
            "round": round,
            "kind": kind,
            "handled_ids": handled.to_json(),
            "brief": brief,
            "dataset": dataset(),
        }
            .to_json(),
    );
    null
}

/// Records what happened, and the Slack message about it if one was posted.
/// Returns the event id (0 when the store is off).
function log_event(
    kind: string,
    issue_id: string?,
    feedback_id: string?,
    payload: json,
    slack_channel: string?,
    slack_ts: string?,
) -> int {
    if (!db_configured()) {
        return 0;
    };
    let row = insert(
        "events",
        {
            "kind": kind,
            "issue_id": issue_id,
            "feedback_id": feedback_id,
            "payload": payload,
            "slack_channel": slack_channel,
            "slack_ts": slack_ts,
            "dataset": dataset(),
        }
            .to_json(),
    );
    baml.json.path_or<int>(row, ".id", 0)
}

/// One recorded decision: which step decided what, and why. The website
/// renders these as the issue's decision trail, so every field the pipeline
/// sets on an issue (subsystem, shepherd, difficulty, the repro, the
/// verdicts) has a reason next to it. `evidence` is anything a reviewer
/// would check the reason against: verdicts, candidates, unknowns.
function decide(
    step: string,
    issue_id: string?,
    feedback_id: string?,
    decision: string,
    reason: string,
    evidence: map<string, json> = {},
) -> int {
    log.info({ "step": step, "issue": issue_id, "feedback": feedback_id, "decision": decision, "reason": reason });
    log_event(
        "decision",
        issue_id,
        feedback_id,
        { "step": step, "decision": decision, "reason": reason, "evidence": evidence.to_json() }.to_json(),
        null,
        null,
    )
}

/// The Slack thread an issue's notifications live in: the first event about
/// it that was posted to Slack.
function issue_thread(issue_id: string) -> SlackRef? {
    if (!db_configured()) {
        return null;
    };
    let rows = select(
        "events",
        "select=slack_channel,slack_ts&issue_id="
            + eq(issue_id)
            + "&slack_ts=not.is.null&order=created_at&limit=1",
    );
    match (rows.at(0)) {
        null => null,
        let r: json => {
            SlackRef {
                channel: baml.json.path_or<string>(r, ".slack_channel", ""),
                ts: baml.json.path_or<string>(r, ".slack_ts", ""),
            }
        },
    }
}

class SlackRef {
    channel: string,
    ts: string,
}

/// Cursors are per dataset: an eval run never moves the live intakes.
function cursor_name(name: string) -> string {
    dataset() + ":" + name
}

function get_cursor(name: string) -> string? {
    if (!db_configured()) {
        return null;
    };
    let rows = select("cursors", "select=value&name=" + eq(cursor_name(name)) + "&limit=1");
    match (rows.at(0)) {
        null => null,
        let r: json => {
            // an empty value is a cleared cursor
            let v = baml.json.path_or<string>(r, ".value", "");
            if (v.length() == 0) {
                null
            } else {
                v
            }
        },
    }
}

function set_cursor(name: string, value: string) -> null {
    if (!db_configured()) {
        return null;
    };
    supabase_send(
        "POST",
        "cursors?on_conflict=name",
        baml.json.stringify([{ "name": cursor_name(name), "value": value }].to_json()),
        "resolution=merge-duplicates,return=minimal",
    );
    null
}

/// `rows[*][field]` as strings, for the ids of a select.
function rows_field(rows: json[], field: string) -> string[] {
    rows.map((r: json) -> string {
        baml.json.path_or<string>(r, "." + field, "")
    })
}

// ==================== Test fixtures (the tests live in metrics.baml) ====================

function sample_feedback() -> Feedback {
    Feedback {
        id: "PH-abc",
        title: "throws panics",
        body: "index out of bounds",
        source: FeedbackSource.BamlFeedback,
        author: Author { email: "a@b.c", github: null, device_id: null },
        toolchain: "0.17.0",
        files: { "main.baml": "function f() -> int throws Nope { 1 }" },
        comments: [],
        issue_ids: [],
    }
}

/// A HandleOutcome for tests, without a sandbox.
function outcome_for_test(
    kind: "fixed" | "hard" | "gate_failed" | "agent_stopped",
    gate: GateResult?,
    report: FixReport?,
) -> HandleOutcome {
    HandleOutcome {
        kind: kind,
        branch: "agent/gh-1-x",
        pr: null,
        turns: 3,
        seconds: 42,
        timed_out: false,
        gate: gate,
        report: report,
        design_doc: null,
        reason: null,
    }
}
