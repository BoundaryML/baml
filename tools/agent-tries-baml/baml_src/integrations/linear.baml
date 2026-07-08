// Linear GraphQL client — the board layer. Ports libs/bench_core/linear_client.py.
// Creates/updates issue cards with a Markdown body + one status-group label, and
// reads status/comments back. Labels REPLACE on update and a group allows one
// label, so "set status" is a read-modify-write over labelIds.
//
// Status-group label ids and workflow-state ids are deployment secrets read from
// env (LINEAR_STATUS_* / LINEAR_STATE_*); a misconfigured deploy fails at call
// time (empty id -> Linear rejects), matching the Python fail-loud intent.

class LinearError {
    message: string,
}

// GraphQL response envelope, decoded leniently (extra fields like `data` are
// ignored). `errors` is absent on success and a non-empty array on failure —
// a typed read, NOT a substring test, so a card whose title/body happens to
// contain `"errors"` doesn't get misclassified as an API failure.
class GqlEnvelope {
    errors: json[]?,
}

// Typed shapes for the queries whose data we actually walk (from_string is
// lenient on extra fields, so these list only what's read).
class FindIssuesResp {
    data: FindIssuesData?,
}

class FindIssuesData {
    issues: FindIssuesNodes,
}

class FindIssuesNodes {
    nodes: FindIssueNode[],
}

class FindIssueNode {
    id: string,
    labels: FindIssueLabels,
}

class FindIssueLabels {
    nodes: FindLabelNode[],
}

class FindLabelNode {
    id: string,
}

class CommentsResp {
    data: CommentsData?,
}

class CommentsData {
    issue: CommentsIssue?,
}

class CommentsIssue {
    comments: CommentsPage,
}

class CommentsPage {
    nodes: json[],
    pageInfo: CommentsPageInfo,
}

class CommentsPageInfo {
    hasNextPage: bool,
    endCursor: string?,
}

class LinearClient {
    token: string,
    team_id: string,

    function headers(self) -> map<string, string> {
        // Linear sends a personal API key verbatim as Authorization (not Bearer).
        return { "Authorization": self.token, "Content-Type": "application/json" };
    }

    // Execute one GraphQL op; returns the `data` object as a JSON string. Throws
    // LinearError on a GraphQL `errors` array or HTTP failure.
    function gql(
        self,
        query: string,
        variables_json: string,
    ) -> string throws LinearError | baml.errors.Io | baml.errors.Timeout {
        let body = "{\"query\":" + jesc(query) + ",\"variables\":" + variables_json + "}";
        let req = baml.http.Request {
            method: "POST",
            url: linear_api_base(),
            headers: self.headers(),
            body: body,
        };
        let resp = baml.http.send(req);
        if (resp.ok() == false) {
            throw LinearError { message: "linear http " + baml.unstable.string(resp.status_code) };
        }
        let text = resp.text();
        if (gql_has_errors(text)) {
            throw LinearError { message: "linear graphql errors: " + tail_str(text, 300) };
        }
        return text;
    }

    // issueCreate: title + Markdown description + initial status label/state.
    function create_issue(
        self,
        title: string,
        status_label_id: string,
        body_md: string,
    ) -> string throws LinearError | baml.errors.Io | baml.errors.Timeout {
        let vars = "{\"input\":{\"teamId\":"
            + jesc(self.team_id)
            + ",\"title\":"
            + jesc(title)
            + ",\"description\":"
            + jesc(body_md)
            + ",\"labelIds\":["
            + jesc(status_label_id)
            + "],\"stateId\":"
            + jesc(state_id_for_label(status_label_id))
            + "}}";
        let data = self
            .gql(
                "mutation($input: IssueCreateInput!) { issueCreate(input: $input) { success issue { id } } }",
                vars,
            );
        if (mutation_ok(data) == false) {
            throw LinearError { message: "issueCreate not successful: " + tail_str(data, 300) };
        }
        let id = extract_field(data, "id");
        if (id == "") {
            throw LinearError { message: "issueCreate returned no id: " + tail_str(data, 300) };
        }
        return id;
    }

    // issueUpdate: swap the status-group label (read-modify-write) + state.
    function set_status(
        self,
        issue_id: string,
        status_label_id: string,
    ) -> int throws LinearError | baml.errors.Io | baml.errors.Timeout {
        let current = self.current_label_ids(issue_id);
        let label_ids = swap_status_label(current, status_label_id);
        let vars = "{\"id\":"
            + jesc(issue_id)
            + ",\"input\":{\"labelIds\":"
            + json_str_array(label_ids)
            + ",\"stateId\":"
            + jesc(state_id_for_label(status_label_id))
            + "}}";
        let data = self
            .gql(
                "mutation($id: String!, $input: IssueUpdateInput!) { issueUpdate(id: $id, input: $input) { success } }",
                vars,
            );
        if (mutation_ok(data) == false) {
            throw LinearError { message: "issueUpdate not successful: " + tail_str(data, 300) };
        }
        return 0;
    }

    // issueUpdate that also rewrites the card's title + body, not just the status
    // label/state. LinearPush uses this so a (re)drafted issue's new write-up
    // actually lands on the Linear card instead of only its status moving.
    function update_card(
        self,
        issue_id: string,
        title: string,
        body_md: string,
        status_label_id: string,
    ) -> int throws LinearError | baml.errors.Io | baml.errors.Timeout {
        let current = self.current_label_ids(issue_id);
        let label_ids = swap_status_label(current, status_label_id);
        let vars = "{\"id\":"
            + jesc(issue_id)
            + ",\"input\":{\"title\":"
            + jesc(title)
            + ",\"description\":"
            + jesc(body_md)
            + ",\"labelIds\":"
            + json_str_array(label_ids)
            + ",\"stateId\":"
            + jesc(state_id_for_label(status_label_id))
            + "}}";
        let data = self
            .gql(
                "mutation($id: String!, $input: IssueUpdateInput!) { issueUpdate(id: $id, input: $input) { success } }",
                vars,
            );
        if (mutation_ok(data) == false) {
            throw LinearError { message: "issueUpdate not successful: " + tail_str(data, 300) };
        }
        return 0;
    }

    function add_comment(
        self,
        issue_id: string,
        body_md: string,
    ) -> int throws LinearError | baml.errors.Io | baml.errors.Timeout {
        let vars = "{\"input\":{\"issueId\":" + jesc(issue_id) + ",\"body\":" + jesc(body_md) + "}}";
        let data = self
            .gql(
                "mutation($input: CommentCreateInput!) { commentCreate(input: $input) { success } }",
                vars,
            );
        if (mutation_ok(data) == false) {
            throw LinearError { message: "commentCreate not successful: " + tail_str(data, 300) };
        }
        return 0;
    }

    // Returns ALL comments on an issue as a JSON array string (paginated — long
    // review threads must not truncate the redraft feedback at one page).
    function get_comments(
        self,
        issue_id: string,
    ) -> string throws LinearError | baml.errors.Io | baml.errors.Timeout {
        let nodes: json[] = [];
        let after = "";
        let more = true;
        while (more) {
            let after_json = if (after == "") {
                "null"
            } else {
                jesc(after)
            };
            let vars = "{\"id\":" + jesc(issue_id) + ",\"after\":" + after_json + "}";
            let data = self
                .gql(
                    "query($id: String!, $after: String) { issue(id: $id) { comments(first: 100, after: $after) { nodes { body createdAt user { name } } pageInfo { hasNextPage endCursor } } } }",
                    vars,
                );
            let page = baml.json.from_string<CommentsResp>(data) catch (_) {
                _ => CommentsResp { data: null }
            };
            more = false;
            match (page.data) {
                let d: CommentsData => {
                    match (d.issue) {
                        let iss: CommentsIssue => {
                            for (let n in iss.comments.nodes) {
                                nodes.push(n);
                            }
                            if (iss.comments.pageInfo.hasNextPage) {
                                after = match (iss.comments.pageInfo.endCursor) {
                                    let c: string => c,
                                    null => "",
                                };
                                more = after != "";
                            }
                        },
                        null => null,
                    };
                },
                null => null,
            };
        }
        return baml.json.to_string(nodes) catch (_) {
            _ => "[]"
        };
    }

    // Exact-title lookup under the team, restricted to cards that already carry a
    // status-group label (i.e. cards this pipeline manages / hand-imported ones).
    // Returns the id ONLY on exactly one match; ambiguity (>1) skips with a
    // warning rather than guessing, and 0 matches returns "".
    function find_issue_by_title(
        self,
        title: string,
    ) -> string throws LinearError | baml.errors.Io | baml.errors.Timeout {
        let vars = "{\"filter\":{\"team\":{\"id\":{\"eq\":"
            + jesc(self.team_id)
            + "}},\"title\":{\"eq\":"
            + jesc(title)
            + "}}}";
        let data = self
            .gql(
                "query($filter: IssueFilter!) { issues(filter: $filter, first: 50) { nodes { id labels { nodes { id } } } } }",
                vars,
            );
        let resp = baml.json.from_string<FindIssuesResp>(data) catch (_) {
            _ => FindIssuesResp { data: null }
        };
        let candidates: string[] = [];
        match (resp.data) {
            let d: FindIssuesData => {
                for (let node in d.issues.nodes) {
                    let label_ids: string[] = [];
                    for (let l in node.labels.nodes) {
                        label_ids.push(l.id);
                    }
                    if (has_status_group_label(label_ids)) {
                        candidates.push(node.id);
                    }
                }
            },
            null => null,
        };
        if (candidates.length() == 1) {
            return match (candidates.at(0)) {
                let v: string => v,
                null => "",
            };
        }
        if (candidates.length() > 1) {
            baml.io
                .eprintln(
                    "linear find_issue_by_title: " + baml.unstable.string(candidates.length())
                        + " status-bearing cards titled " + jesc(title) + "; skipping adopt",
                );
        }
        return "";
    }

    function current_label_ids(
        self,
        issue_id: string,
    ) -> string[] throws LinearError | baml.errors.Io | baml.errors.Timeout {
        let vars = "{\"id\":" + jesc(issue_id) + "}";
        let data = self.gql("query($id: String!) { issue(id: $id) { labels { nodes { id } } } }", vars);
        // Extract label-node ids from the data JSON.
        return extract_label_ids(data);
    }
}

// ---- env-configured ids + label/state mapping -----------------------------

function linear_api_base() -> string {
    return env_or("LINEAR_API_BASE", "https://api.linear.app/graphql");
}

function linear_status_id(name: string) -> string {
    return env_or("LINEAR_STATUS_" + name, "");
}

function linear_state_id(name: string) -> string {
    return env_or("LINEAR_STATE_" + name, "");
}

// Internal lifecycle status -> Linear status-group label id. Mirrors
// services/linear_sync LinearPush._map_status: a re-render mid-lifecycle (redraft
// rewrite, body/PR update) must land the label matching the issue's CURRENT state,
// not reset it. `redraft`/`redrafting` keep the human's redraft label; anything
// unrecognized rests at not-started.
function label_for_issue_status(status: string) -> string {
    let name = if (status == "open" || status == "confirmed") {
        "NOT_STARTED"
    } else if (status == "approved" || status == "dispatching") {
        "APPROVED"
    } else if (status == "fixing" || status == "tocursor") {
        "TO_CURSOR"
    } else if (status == "prprep") {
        "PR_PREP"
    } else if (status == "pr_ready") {
        "READY_TO_MERGE"
    } else if (status == "needs_human") {
        "NEEDS_HUMAN"
    } else if (status == "closed" || status == "merged") {
        "MERGED"
    } else if (status == "redraft" || status == "redrafting") {
        "REDRAFT"
    } else if (status == "rejected") {
        // A rejected issue's card shows the rejected label + Canceled state
        // (previously fell to NOT_STARTED, which re-opened the card on re-sync).
        "REJECTED"
    } else if (status == "verifying" || status == "reverify" || status == "reverifying") {
        // Auto-verify states are invisible on the board — the card rests at
        // not-started until the verdict moves it (closed -> MERGED).
        "NOT_STARTED"
    } else {
        "NOT_STARTED"
    };
    return linear_status_id(name);
}

// ATB status-group label id -> Linear workflow state id (unmapped -> In Review).
function state_id_for_label(status_label_id: string) -> string {
    if (status_label_id == linear_status_id("NOT_STARTED")) {
        return linear_state_id("UNCOMMITTED");
    }
    if (status_label_id == linear_status_id("APPROVED")) {
        return linear_state_id("IN_PROGRESS");
    }
    if (status_label_id == linear_status_id("TO_CURSOR")) {
        return linear_state_id("IN_PROGRESS");
    }
    if (status_label_id == linear_status_id("REDRAFT")) {
        return linear_state_id("IN_REVIEW");
    }
    if (status_label_id == linear_status_id("NEEDS_HUMAN")) {
        return linear_state_id("CANCELED");
    }
    if (status_label_id == linear_status_id("MERGED")) {
        return linear_state_id("DONE");
    }
    if (status_label_id == linear_status_id("REJECTED")) {
        return linear_state_id("CANCELED");
    }
    return linear_state_id("IN_REVIEW");
}

// All status-group label ids (to strip before adding a new one).
function status_group_label_ids() -> string[] {
    let names = [
        "NOT_STARTED",
        "APPROVED",
        "TO_CURSOR",
        "PR_PREP",
        "READY_TO_MERGE",
        "NEEDS_HUMAN",
        "REDRAFT",
        "MERGED",
        "REJECTED",
    ];
    let out: string[] = [];
    for (let n in names) {
        let id = linear_status_id(n);
        if (id != "") {
            out.push(id);
        }
    }
    return out;
}

// True when any of the given label ids is a status-group label.
function has_status_group_label(label_ids: string[]) -> bool {
    let group = status_group_label_ids();
    for (let lid in label_ids) {
        if (in_list(group, lid)) {
            return true;
        }
    }
    return false;
}

// Drop any existing status-group label, add the new one, keep the rest.
function swap_status_label(current: string[], status_label_id: string) -> string[] {
    let group = status_group_label_ids();
    let kept: string[] = [];
    for (let lid in current) {
        if (in_list(group, lid) == false) {
            kept.push(lid);
        }
    }
    kept.push(status_label_id);
    return kept;
}

// ---- Markdown body rendering (issue_body_md) ------------------------------

function issue_body_md(
    body: string,
    evidence_links: string[],
    env_line: string,
    suggestion: string,
    category: string,
    repro: string,
    issue_link: string,
    pr_url: string,
) -> string {
    let parts: string[] = [];
    let links: string[] = [];
    if (issue_link != "") {
        links.push("[Issue](" + issue_link + ")");
    }
    if (pr_url != "") {
        links.push("[PR](" + pr_url + ")");
    }
    if (links.length() > 0) {
        parts.push("**Links:** " + links.join(" · "));
    }
    if (category != "") {
        parts.push("**Category:** " + category);
    }
    if (env_line != "") {
        parts.push("**Environment:** " + env_line);
    }
    if (body != "") {
        parts.push(body);
    }
    if (repro != "") {
        parts.push("## Reproduction\n\n```typescript\n" + repro + "\n```");
    }
    if (suggestion != "") {
        parts.push("## Suggested fix\n\n" + suggestion);
    }
    if (evidence_links.length() > 0) {
        let items: string[] = [];
        let i = 1;
        for (let link in evidence_links) {
            // A link may carry a "|<annotation>" suffix (e.g. "|baml 0.13.1-...").
            let segs = link.split("|");
            let url = match (segs.at(0)) {
                let u: string => u,
                null => link,
            };
            let note = if (segs.length() > 1) {
                " · " + join_from_sep(segs, 1, "|")
            } else {
                ""
            };
            items.push("- [run " + baml.unstable.string(i) + "](" + url + ")" + note);
            i = i + 1;
        }
        parts.push("## Evidence\n\n" + items.join("\n"));
    }
    return parts.join("\n\n");
}

// ---- two-way sync helpers ---------------------------------------------------

// Content hash of a rendered card body, stamped as issues.lastPushedBodyHash
// after every push. A webhook body whose hash matches is our own echo; a
// mismatch is a human edit on Linear. hmac_sha256_hex with a fixed key is just
// a convenient stdlib-free SHA hash — the key carries no secret meaning.
function body_hash(body: string) -> string {
    return hmac_sha256_hex("atb-linear-body", body);
}

// True when a Linear GraphQL error means the referenced card no longer exists
// (deleted/merged away) — LinearPush then drops the dead linearIssueId and
// adopts-by-title / creates fresh instead of failing forever.
function is_missing_entity(msg: string) -> bool {
    let m = msg.to_lower_case();
    return m.includes("entity not found") || m.includes("could not find");
}

// Inverse of issue_body_md for the human-owned region: given a card body,
// return just the free-text description — skip the machine header lines
// (**Links:** / **Category:** / **Environment:**) and stop at the first
// machine-owned section (## Reproduction / ## Suggested fix / ## Evidence).
// The description's own `##` subheadings survive (only the three machine
// headings terminate the region).
function extract_description_region(body: string) -> string {
    let out: string[] = [];
    let done = false;
    for (let line in body.split("\n")) {
        if (done == false) {
            let t = line.trim();
            if (t.starts_with("## Reproduction")
                || t.starts_with("## Suggested fix")
                || t.starts_with("## Evidence")) {
                done = true;
            } else if (t.starts_with("**Links:**")
                || t.starts_with("**Category:**")
                || t.starts_with("**Environment:**")) {
                // machine header line — skip
            } else {
                out.push(line);
            }
        }
    }
    return out.join("\n").trim();
}

// ---- GraphQL response inspection -------------------------------------------

// True when the response carries a non-empty GraphQL `errors` array. Falls back
// to false on an undecodable body (the HTTP layer already screened status).
function gql_has_errors(text: string) -> bool {
    let envelope = baml.json.from_string<GqlEnvelope>(text) catch (_) {
        _ => GqlEnvelope { errors: null }
    };
    let errs: json[] = match (envelope.errors) {
        let e: json[] => e,
        null => [],
    };
    return errs.length() > 0;
}

// True when a mutation payload reports `"success": true` (Linear mutations can
// return 200 + errors:false without a GraphQL error; the old client raised on
// !success and so do we).
function mutation_ok(data: string) -> bool {
    let segs = data.split("\"success\":");
    if (segs.length() < 2) {
        return false;
    }
    let after = match (segs.at(1)) {
        let v: string => v,
        null => "",
    };
    return after.trim().starts_with("true");
}

// ---- small JSON/string helpers --------------------------------------------

function json_str_array(xs: string[]) -> string {
    let parts: string[] = [];
    for (let x in xs) {
        parts.push(jesc(x));
    }
    return "[" + parts.join(",") + "]";
}

function in_list(xs: string[], v: string) -> bool {
    for (let x in xs) {
        if (x == v) {
            return true;
        }
    }
    return false;
}

function tail_str(s: string, n: int) -> string {
    let len = s.length();
    if (len <= n) {
        return s;
    }
    return s.substring(len - n, len);
}

// Float -> int (truncating). Convex serializes every number as a float, so int
// fields read off a doc must be declared float? and converted through this.
function f2i(f: float) -> int {
    let whole = match (baml.unstable.string(f).split(".").at(0)) {
        let v: string => v,
        null => "0",
    };
    return baml.json.from_string<int>(whole) catch (_) {
        _ => 0
    };
}

// Non-throwing JSON string escaper (to_string throws JsonSerializationError).
function jesc(s: string) -> string {
    return baml.json.to_string(s) catch (_) {
        _ => "\"\""
    };
}

// Pull the first "key":"..." value out of a GraphQL data JSON (split-based; BAML
// has no String.indexOf).
function extract_field(data: string, key: string) -> string {
    let marker = "\"" + key + "\":\"";
    let segs = data.split(marker);
    if (segs.length() < 2) {
        return "";
    }
    let after = match (segs.at(1)) {
        let v: string => v,
        null => "",
    };
    return match (after.split("\"").at(0)) {
        let v: string => v,
        null => "",
    };
}

// Pull all "id":"..." values from the labels-query data.
function extract_label_ids(data: string) -> string[] {
    let out: string[] = [];
    let segs = data.split("\"id\":\"");
    let i = 1;
    while (i < segs.length()) {
        let seg = match (segs.at(i)) {
            let v: string => v,
            null => "",
        };
        let id = match (seg.split("\"").at(0)) {
            let v: string => v,
            null => "",
        };
        if (id != "") {
            out.push(id);
        }
        i = i + 1;
    }
    return out;
}
