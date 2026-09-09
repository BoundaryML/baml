// Export issues to Linear. One Linear issue per atb2 issue, assigned to
// the issue's shepherd, with the full description (ticket, repros, status,
// difficulty, plan, design doc, comments, linked feedback).
//
//   baml run -e 'export_issue_to_linear(load_issue("ISSUE-…") ?? baml.sys.panic("no such issue"))'
//   baml run -e 'export_issues_to_linear()'              # every open issue
//   baml run -e 'export_issues_to_linear(state = "in_progress")'
//
// Idempotent: every exported issue carries an `atb2: <id>` marker line in
// its Linear description, and an export first looks that marker up. A
// second export of the same issue updates the Linear issue in place
// (description and assignee) instead of opening a duplicate.
//
// Wiring (Infisical, boundary-tools):
//
//   ATB2_LINEAR_API_KEY    a personal API key (Settings > API); sent as-is
//                          in the Authorization header, the way Linear
//                          expects for personal keys
//   ATB2_LINEAR_TEAM       the team key ("ENG") or team id the issues go to
//   ATB2_LINEAR_ASSIGNEES  optional "github_login:linear-user-id,…" mapping,
//                          the shape of ATB2_SHEPHERDS. Shepherds not in the
//                          mapping are matched against Linear's user list by
//                          display name, then by the local part of the email.
//
// Description and assignee resolution are pure functions with token-free
// tests; only the GraphQL calls touch the network.

class LinearError {
    message: string,
}

/// A Linear issue we exported to.
class LinearExport {
    /// Linear's uuid for the issue.
    id: string,
    /// "ENG-123".
    identifier: string,
    url: string,
    /// Linear user id the issue was assigned to; null when the shepherd
    /// could not be resolved to a Linear user (the issue is still exported).
    assignee_id: string?,
    /// True when an existing Linear issue was updated rather than created.
    updated: bool,
}

function linear_key() -> string {
    (baml.env.get("ATB2_LINEAR_API_KEY") ?? baml.env.get("ATB_LINEAR_TOKEN") ?? "").trim()
}

function linear_configured() -> bool {
    linear_key().length() > 0 && (baml.env.get("ATB2_LINEAR_TEAM") ?? "").trim().length() > 0
}

// ==================== GraphQL ====================

/// One Linear GraphQL call. Throws on transport failure, non-2xx, or any
/// entry in `errors`: Linear that is configured but broken should be
/// noticed, not skipped.
function linear_api(query: string, variables: json) -> json {
    let key = linear_key();
    if (key.length() == 0) {
        throw LinearError { message: "ATB2_LINEAR_API_KEY unset" };
    };
    let body: map<string, json> = { "query": query.to_json(), "variables": variables };
    let res = baml.http.send(
        baml.http.Request {
            method: "POST",
            url: "https://api.linear.app/graphql",
            headers: { "Authorization": key, "Content-Type": "application/json" },
            body: baml.json.stringify(body.to_json()),
        },
        timeout = baml.time.Duration.from_seconds(30),
    ) catch (_) {
        _ => {
            throw LinearError { message: "linear: request failed" };
        },
    };
    let text = res.text() catch (_) {
        _ => "",
    };
    if (!res.ok()) {
        throw LinearError { message: "linear: " + res.status_code.to_string() };
    };
    let out = baml.json.parse(text) catch (_) {
        _ => {
            throw LinearError { message: "linear: response is not JSON" };
        },
    };
    let errors = baml.json.path_or<json[]>(out, ".errors", []);
    match (errors.at(0)) {
        null => null,
        let e: json => {
            throw LinearError { message: "linear: GraphQL request failed" };
        },
    };
    baml.json.path_or<json>(out, ".data", baml.json.parse("{}"))
}

/// The team id for ATB2_LINEAR_TEAM: a uuid is used as-is, anything else
/// is looked up as a team key ("ENG").
function linear_team_id() -> string {
    let team = (baml.env.get("ATB2_LINEAR_TEAM") ?? "").trim();
    if (team.length() == 0) {
        throw LinearError { message: "ATB2_LINEAR_TEAM unset" };
    };
    if (team.length() == 36 && team.split("-").length() == 5) {
        return team;
    };
    let data = linear_api(
        "query($key: String!) { teams(filter: { key: { eq: $key } }) { nodes { id key } } }",
        { "key": team.to_json() }.to_json(),
    );
    match (baml.json.path_or<json[]>(data, ".teams.nodes", []).at(0)) {
        null => {
            throw LinearError { message: "linear: no team with key " + team };
        },
        let t: json => baml.json.path_or<string>(t, ".id", ""),
    }
}

// ==================== Assignee ====================

/// A Linear user, as much of it as assignee matching needs.
class LinearUser {
    id: string,
    name: string,
    display_name: string,
    email: string,
}

/// The explicit mapping, ATB2_SHEPHERDS-style: "login:linear-user-id,…".
/// Case-insensitive on the login; a login mapped twice, or to an empty
/// id, resolves to null so a misconfiguration never assigns the wrong person.
function linear_assignee_in(login: string?, config: string) -> string? {
    if (login == null) {
        return null;
    };
    let found: string? = null;
    for (let entry in config.split(",")) {
        let pair = entry.trim().split(":");
        if (
            pair.length() == 2
                && (pair.at(0) ?? "").trim().to_lower_case() == (login ?? "").trim().to_lower_case()
        ) {
            let id = (pair.at(1) ?? "").trim();
            if (found != null || id.length() == 0) {
                return null;
            };
            found = id;
        };
    }
    found
}

/// Match a GitHub login against Linear's users: display name first (the
/// handle most people reuse), then the local part of the email. Exact,
/// case-insensitive; an ambiguous match resolves to null.
function linear_user_matching(login: string?, users: LinearUser[]) -> string? {
    if (login == null) {
        return null;
    };
    let want = (login ?? "").trim().to_lower_case();
    if (want.length() == 0) {
        return null;
    };
    let by_handle = users.filter((u: LinearUser) -> bool {
        u.display_name.trim().to_lower_case() == want
    });
    if (by_handle.length() == 1) {
        return by_handle.at(0)?.id;
    };
    if (by_handle.length() > 1) {
        return null;
    };
    let by_email = users.filter((u: LinearUser) -> bool {
        (u.email.split("@").at(0) ?? "").trim().to_lower_case() == want
    });
    if (by_email.length() == 1) {
        return by_email.at(0)?.id;
    };
    null
}

/// The active users of the workspace.
function linear_users() -> LinearUser[] {
    let data = linear_api(
        "query { users(filter: { active: { eq: true } }, first: 250) { nodes { id name displayName email } } }",
        baml.json.parse("{}"),
    );
    baml.json.path_or<json[]>(data, ".users.nodes", []).map((u: json) -> LinearUser {
        LinearUser {
            id: baml.json.path_or<string>(u, ".id", ""),
            name: baml.json.path_or<string>(u, ".name", ""),
            display_name: baml.json.path_or<string>(u, ".displayName", ""),
            email: baml.json.path_or<string>(u, ".email", ""),
        }
    })
}

/// The Linear user an issue's shepherd maps to: the explicit mapping wins,
/// then a match against the workspace's users. Null when nothing fits.
function linear_assignee(shepherd: string?) -> string? {
    if (shepherd == null) {
        return null;
    };
    match (linear_assignee_in(shepherd, baml.env.get("ATB2_LINEAR_ASSIGNEES") ?? "")) {
        let id: string => id,
        null => linear_user_matching(shepherd, linear_users()),
    }
}

// ==================== Description ====================

/// The marker line that ties a Linear issue back to its atb2 issue; what
/// `find_linear_issue` looks for.
function linear_marker(issue_id: string) -> string {
    "atb2: " + issue_id
}

function status_text(s: IssueStatus) -> string {
    match (s) {
        let x: Open => "open",
        let x: AwaitingApproval => "awaiting approval",
        let x: Approved => "approved by " + x.by,
        let x: InProgress => match (x.pr) {
            null => "in progress (a human took it)",
            let pr: string => "in progress: " + pr,
        },
        let x: Rejected => "rejected: " + x.reason,
        let x: Deferred => match (x.workaround) {
            null => "deferred: " + x.reason,
            let w: string => "deferred: " + x.reason + " (workaround: " + w + ")",
        },
        let x: Merged => "merged: " + x.pr,
        let x: Shipped => "shipped in " + x.version + " on " + x.date,
    }
}

function linear_repro_md(i: int, r: Repro) -> string {
    let files = r.files.keys()
        .map((name: string) -> string {
            "`" + name + "`\n```\n" + (r.files.get(name) ?? "") + "\n```"
        })
        .join("\n\n");
    "### Repro "
        + (i + 1).to_string()
        + "\n\n```sh\n"
        + r.command
        + "\n```\n\n"
        + (
            match (r.setup) {
                null => "",
                let s: string => "Setup: " + s + "\n\n",
            }
        )
        + files
        + "\n\nExpected: `"
        + baml.json.stringify(r.expectation.to_json())
        + "`"
}

/// The whole issue as Linear markdown: the ticket verbatim, then every
/// repro, then the pipeline's state and documents, then the trail back
/// (feedback ids, the atb2 marker).
function linear_description(issue: Issue) -> string {
    let parts: string[] = [issue.description.trim()];
    parts.push(
        "## Details\n\n"
            + "- Subsystem: "
            + issue.subsystem.to_string()
            + "\n"
            + "- Version: "
            + issue.version
            + "\n"
            + "- Status: "
            + status_text(issue.status)
            + "\n"
            + "- Difficulty: "
            + (
                match (issue.difficulty) {
                    null => "not gauged",
                    let d: Difficulty => d.to_string(),
                }
            )
            + "\n"
            + "- Shepherd: "
            + (issue.shepherd ?? "unassigned"),
    );
    if (issue.repros.length() > 0) {
        let i = 0;
        let repros: string[] = [];
        for (let r in issue.repros) {
            repros.push(linear_repro_md(i, r));
            i = i + 1;
        }
        parts.push("## Repros\n\n" + repros.join("\n\n"));
    };
    match (issue.resolution_plan) {
        null => null,
        let p: string => parts.push("## Resolution plan\n\n" + p.trim()),
    };
    match (issue.design_doc) {
        null => null,
        let d: string => parts.push("## Design doc\n\n" + d.trim()),
    };
    if (issue.comments.length() > 0) {
        parts.push(
            "## Comments\n\n"
                + issue.comments
                    .map((c: Comment) -> string {
                        "- **" + c.author + "**: " + c.body
                    })
                    .join("\n"),
        );
    };
    let trail: string[] = [];
    if (issue.feedback_ids.length() > 0) {
        trail.push("Feedback: " + issue.feedback_ids.join(", "));
    };
    match (baml.env.get("ATB2_UI_URL")) {
        null => null,
        let base: string => trail.push("Dashboard: " + base.trim() + "/issues/" + issue.id),
    };
    trail.push(linear_marker(issue.id));
    parts.push("---\n" + trail.join("\n"));
    parts.join("\n\n")
}

// ==================== Export ====================

/// The Linear issue already exported for `issue_id`, by its marker line.
function find_linear_issue(issue_id: string) -> LinearExport? {
    let data = linear_api(
        "query($marker: String!, $team: ID!) { issues(filter: { description: { contains: $marker }, team: { id: { eq: $team } } }, first: 250) { pageInfo { hasNextPage } nodes { id identifier url description assignee { id } } } }",
        { "marker": linear_marker(issue_id).to_json(), "team": linear_team_id().to_json() }.to_json(
        ),
    );
    if (baml.json.path_or<bool>(data, ".issues.pageInfo.hasNextPage", true)) {
        throw LinearError { message: "linear: incomplete issue lookup; refusing to create a possible duplicate" };
    };
    let nodes = baml.json.path_or<json[]>(data, ".issues.nodes", []);
    // "atb2: ISSUE-1" is also contained by "atb2: ISSUE-10": keep the
    // issue whose marker is a whole line
    let marker = linear_marker(issue_id);
    let exact = nodes.filter((n: json) -> bool {
        let desc = baml.json.path_or<string>(n, ".description", "");
        desc.lines().includes(marker)
    });
    if (exact.length() > 1) {
        throw LinearError { message: "linear: duplicate export markers; refusing an ambiguous update" };
    };
    match (exact.at(0)) {
        null => null,
        let n: json => {
            LinearExport {
                id: baml.json.path_or<string>(n, ".id", ""),
                identifier: baml.json.path_or<string>(n, ".identifier", ""),
                url: baml.json.path_or<string>(n, ".url", ""),
                assignee_id: baml.json.path_or<string?>(n, ".assignee.id", null),
                updated: true,
            }
        },
    }
}

/// Create or update the Linear issue for `issue`, assigned to its shepherd.
function export_issue_to_linear(issue: Issue) -> LinearExport {
    if (!linear_configured()) {
        throw LinearError { message: "ATB2_LINEAR_API_KEY and ATB2_LINEAR_TEAM must be set" };
    };
    //# resolve the shepherd to a Linear user
    let assignee = linear_assignee(issue.shepherd);
    let input: map<string, json> = { "title": issue.title.to_json(), "description": linear_description(issue).to_json() };
    match (assignee) {
        null => null,
        let id: string => input.set("assigneeId", id.to_json()),
    };
    //# update the existing Linear issue when there is one
    match (find_linear_issue(issue.id)) {
        let existing: LinearExport => {
            let data = linear_api(
                "mutation($id: String!, $input: IssueUpdateInput!) { issueUpdate(id: $id, input: $input) { success issue { id identifier url } } }",
                { "id": existing.id.to_json(), "input": input.to_json() }.to_json(),
            );
            if (!baml.json.path_or<bool>(data, ".issueUpdate.success", false)) {
                throw LinearError { message: "linear: issueUpdate failed for " + existing.identifier };
            };
            LinearExport {
                id: existing.id,
                identifier: existing.identifier,
                url: existing.url,
                assignee_id: assignee,
                updated: true,
            }
        },
        //# otherwise create it in the team
        null => {
            input.set("teamId", linear_team_id().to_json());
            let data = linear_api(
                "mutation($input: IssueCreateInput!) { issueCreate(input: $input) { success issue { id identifier url } } }",
                { "input": input.to_json() }.to_json(),
            );
            if (!baml.json.path_or<bool>(data, ".issueCreate.success", false)) {
                throw LinearError { message: "linear: issueCreate failed for " + issue.id };
            };
            LinearExport {
                id: baml.json.path_or<string>(data, ".issueCreate.issue.id", ""),
                identifier: baml.json.path_or<string>(data, ".issueCreate.issue.identifier", ""),
                url: baml.json.path_or<string>(data, ".issueCreate.issue.url", ""),
                assignee_id: assignee,
                updated: false,
            }
        },
    }
}

/// What a bulk export did.
class LinearExportReport {
    created: string[],
    updated: string[],
    /// Issues whose shepherd has no Linear user; exported unassigned.
    unassigned: string[],
    /// "<issue id>: <error>" for each issue that could not be exported.
    failed: string[],
}

/// Export every issue in `state` ("open", "in_progress", …) from the store.
function export_issues_to_linear(state: string = "open") -> LinearExportReport {
    let report = LinearExportReport { created: [], updated: [], unassigned: [], failed: [] };
    if (!db_configured()) {
        report.failed.push("store: FEEDBACK_SUPABASE_URL/KEY unset; nothing to export");
        return report;
    };
    for (let issue in load_issues_in(state)) {
        let out = export_issue_to_linear(issue) catch (e) {
            let err: LinearError => {
                report.failed.push(issue.id + ": " + err.message);
                continue;
            },
        };
        if (out.updated) {
            report.updated.push(issue.id + " -> " + out.identifier);
        } else {
            report.created.push(issue.id + " -> " + out.identifier);
        };
        if (out.assignee_id == null) {
            report.unassigned.push(issue.id + " (shepherd: " + (issue.shepherd ?? "none") + ")");
        };
    }
    report
}

// ==================== Tests ====================

function linear_fixture() -> Issue {
    let row = baml.json.from_json<map<string, json>>(issue_row(gh_issue(7, Difficulty.Medium)));
    row.set("shepherd", "codeshaunted".to_json());
    row.set("resolution_plan", "Fix the parser.".to_json());
    row.set("feedback_ids", ["PH-1", "PH-2"].to_json());
    row.set("comments", [Comment { author: "hellovai", body: "seen in the wild" }].to_json());
    issue_from_row(row.to_json())
}

testset "linear_export" {
    test "assignee mapping is explicit and refuses ambiguity" {
        assert.equal(linear_assignee_in("CodeShaunted", "other:u2, codeshaunted:u1") ?? "", "u1");
        assert.equal(
            linear_assignee_in("codeshaunted", "codeshaunted:u1,codeshaunted:u2") == null,
            true,
        );
        assert.equal(linear_assignee_in("codeshaunted", "codeshaunted:") == null, true);
        assert.equal(linear_assignee_in(null, "codeshaunted:u1") == null, true);
        assert.equal(linear_assignee_in("nobody", "codeshaunted:u1") == null, true);
    }
    test "shepherds match Linear users by handle, then email" {
        let users = [
            LinearUser {
                id: "a",
                name: "Shaun",
                display_name: "codeshaunted",
                email: "shaun@boundaryml.com",
            },
            LinearUser {
                id: "b",
                name: "Vaibhav",
                display_name: "vaibhav",
                email: "hellovai@boundaryml.com",
            },
            LinearUser { id: "c", name: "Sam", display_name: "sam", email: "sam@boundaryml.com" },
            LinearUser { id: "d", name: "Sam 2", display_name: "sam", email: "sam2@boundaryml.com" },
        ];
        assert.equal(linear_user_matching("CodeShaunted", users) ?? "", "a");
        assert.equal(linear_user_matching("hellovai", users) ?? "", "b");
        assert.equal(linear_user_matching("sam", users) == null, true);
        assert.equal(linear_user_matching("stranger", users) == null, true);
        assert.equal(linear_user_matching(null, users) == null, true);
    }
    test "description carries the ticket, repros, state, documents and the marker" {
        let issue = linear_fixture();
        let md = linear_description(issue);
        assert.equal(md.starts_with(issue.description.trim()), true);
        assert.equal(md.index_of("## Details") != null, true);
        assert.equal(md.index_of("- Difficulty: Medium") != null, true);
        assert.equal(md.index_of("- Shepherd: codeshaunted") != null, true);
        assert.equal(md.index_of("## Repros") != null == (issue.repros.length() > 0), true);
        assert.equal(md.index_of("## Resolution plan\n\nFix the parser.") != null, true);
        assert.equal(md.index_of("- **hellovai**: seen in the wild") != null, true);
        assert.equal(md.index_of("Feedback: PH-1, PH-2") != null, true);
        assert.equal(md.ends_with(linear_marker(issue.id)), true);
    }
    test "every status renders" {
        assert.equal(status_text(Open { state: "open" }), "open");
        assert.equal(status_text(Approved { state: "approved", by: "U1" }), "approved by U1");
        assert.equal(
            status_text(InProgress { state: "in_progress", pr: null }),
            "in progress (a human took it)",
        );
        assert.equal(
            status_text(Deferred { state: "deferred", reason: "later", workaround: "use x" }),
            "deferred: later (workaround: use x)",
        );
        assert.equal(
            status_text(Shipped { state: "shipped", version: "0.18.0", date: "2026-09-09" }),
            "shipped in 0.18.0 on 2026-09-09",
        );
    }
}
