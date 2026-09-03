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
function supabase_send(method: string, path: string, body: string, prefer: string) -> string {
    let url = (baml.env.get("FEEDBACK_SUPABASE_URL") ?? "") + "/rest/v1/" + path;
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
        timeout = baml.time.Duration.from_seconds(30),
    ) catch (e) {
        _ => baml.sys.panic("supabase: " + method + " failed for " + url),
    };
    let text = res.text() catch (_) {
        _ => "",
    };
    if (!res.ok()) {
        baml.sys.panic(
            "supabase: "
                + res.status_code.to_string()
                + " for "
                + method
                + " "
                + url
                + ": "
                + tail(text, 3),
        );
    };
    text
}

/// Insert-or-update on the primary key. `rows` is a JSON array.
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
        "dataset": dataset(),
    }
        .to_json()
}

/// An Issue from an `issues` row (extra columns such as `state`,
/// `created_at` and a joined `outcome` are ignored).
function issue_from_row(row: json) -> Issue {
    Issue {
        id: baml.json.path_or<string>(row, ".id", ""),
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
    upsert("feedback", [feedback_row(fb, origin)].to_json());
    null
}

/// Reports already in the store, by id: what an intake must not re-add.
function known_feedback_ids() -> string[] {
    if (!db_configured()) {
        return [];
    };
    rows_field(select("feedback", "select=id&order=received_at"), "id")
}

function save_issue(issue: Issue) -> null {
    if (!db_configured()) {
        return null;
    };
    upsert("issues", [issue_row(issue)].to_json());
    null
}

function load_issue(id: string) -> Issue? {
    if (!db_configured()) {
        return null;
    };
    let rows = select("issues", "select=*&id=" + eq(id) + "&limit=1");
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

function get_cursor(name: string) -> string? {
    if (!db_configured()) {
        return null;
    };
    let rows = select("cursors", "select=value&name=" + eq(name) + "&limit=1");
    match (rows.at(0)) {
        null => null,
        let r: json => baml.json.path_or<string?>(r, ".value", null),
    }
}

function set_cursor(name: string, value: string) -> null {
    if (!db_configured()) {
        return null;
    };
    supabase_send(
        "POST",
        "cursors?on_conflict=name",
        baml.json.stringify([{ "name": name, "value": value }].to_json()),
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

// ==================== Unit tests (token-free) ====================

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

testset "store" {
    test "a feedback row round-trips" {
        let fb = sample_feedback();
        let row = feedback_row(fb, { "kind": "posthog", "event": "e1" }.to_json());
        assert.equal(baml.json.path_or<string>(row, ".source", ""), "BamlFeedback");
        assert.equal(baml.json.path_or<string>(row, ".author_email", ""), "a@b.c");
        assert.equal(baml.json.path_or<string>(row, ".origin.kind", ""), "posthog");
        let back = feedback_from_row(row);
        assert.equal(back.id, fb.id);
        assert.equal(back.files.get("main.baml") ?? "", "function f() -> int throws Nope { 1 }");
        assert.equal(back.toolchain ?? "", "0.17.0");
        assert.equal(back.author.email ?? "", "a@b.c");
        assert.equal(back.author.github == null, true);
    }

    test "an issue row round-trips, status and repros included" {
        let issue = with_difficulty(gh_issue(4587, null), Difficulty.Easy);
        let taken = with_status(issue, InProgress { state: "in_progress", pr: "https://github.com/x/y/pull/1" });
        let row = issue_row(taken);
        assert.equal(baml.json.path_or<string>(row, ".subsystem", ""), "Compiler");
        assert.equal(baml.json.path_or<string>(row, ".difficulty", ""), "Easy");
        assert.equal(baml.json.path_or<string>(row, ".status.state", ""), "in_progress");
        let back = issue_from_row(row);
        assert.equal(back.id, taken.id);
        assert.equal(back.subsystem, Subsystem.Compiler);
        assert.equal(back.difficulty ?? Difficulty.Hard, Difficulty.Easy);
        assert.equal(back.repros.length(), taken.repros.length());
        match (back.status) {
            let s: InProgress => assert.equal(s.pr ?? "", "https://github.com/x/y/pull/1"),
            _ => assert.is_true(false),
        };
    }

    test "an issue row without a difficulty reads back as ungauged" {
        let back = issue_from_row(issue_row(gh_issue(1, null)));
        assert.equal(back.difficulty == null, true);
        assert.equal(subsystem_from("nope"), Subsystem.Unknown);
        assert.equal(difficulty_from(null) == null, true);
    }

    test "a run row carries the outcome" {
        let o = outcome_for_test("gate_failed", sample_gate(), sample_report());
        let row = run_row("ISSUE-1", o, HandleMode.DryRun);
        assert.equal(baml.json.path_or<string>(row, ".kind", ""), "gate_failed");
        assert.equal(baml.json.path_or<string>(row, ".mode", ""), "DryRun");
        assert.equal(baml.json.path_or<bool>(row, ".gate.ok", true), sample_gate().ok);
        assert.equal(baml.json.path_or<string>(row, ".issue_id", ""), "ISSUE-1");
    }

    test "rows carry the dataset; live unless ATB2_DATASET=eval" {
        let row = issue_row(gh_issue(1, null));
        assert.equal(baml.json.path_or<string>(row, ".dataset", ""), dataset());
        assert.is_true(dataset() == "live" || dataset() == "eval");
        assert.equal(
            baml.json.path_or<string>(
                feedback_row(sample_feedback(), baml.json.parse("{}")),
                ".dataset",
                "",
            ),
            dataset(),
        );
    }

    test "filters are url-safe" {
        assert.equal(eq("ISSUE-01j9"), "eq.ISSUE-01j9");
        assert.equal(eq("a b&c"), "eq.a%20b%26c");
        assert.equal(
            url_encode("https://github.com/x/y/pull/1"),
            "https%3A%2F%2Fgithub.com%2Fx%2Fy%2Fpull%2F1",
        );
    }

    test "the store is off without its variables" {
        // the token-free tests run without FEEDBACK_SUPABASE_*; every save is a no-op
        if (!db_configured()) {
            save_issue(gh_issue(1, null));
            assert.equal(known_feedback_ids().length(), 0);
            assert.equal(load_issue("nope") == null, true);
            assert.equal(get_cursor("x") == null, true);
        };
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
