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
        if (text.includes("\"errors\"")) {
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
        return extract_field(data, "id");
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
        let _ = self
            .gql(
                "mutation($id: String!, $input: IssueUpdateInput!) { issueUpdate(id: $id, input: $input) { success } }",
                vars,
            );
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
        let _ = self
            .gql(
                "mutation($id: String!, $input: IssueUpdateInput!) { issueUpdate(id: $id, input: $input) { success } }",
                vars,
            );
        return 0;
    }

    function add_comment(
        self,
        issue_id: string,
        body_md: string,
    ) -> int throws LinearError | baml.errors.Io | baml.errors.Timeout {
        let vars = "{\"input\":{\"issueId\":" + jesc(issue_id) + ",\"body\":" + jesc(body_md) + "}}";
        let _ = self
            .gql(
                "mutation($input: CommentCreateInput!) { commentCreate(input: $input) { success } }",
                vars,
            );
        return 0;
    }

    // Returns all comment bodies on an issue, joined (newest context for redraft).
    function get_comments(
        self,
        issue_id: string,
    ) -> string throws LinearError | baml.errors.Io | baml.errors.Timeout {
        let vars = "{\"id\":" + jesc(issue_id) + "}";
        let data = self
            .gql(
                "query($id: String!) { issue(id: $id) { comments { nodes { body createdAt user { name } } } } }",
                vars,
            );
        return data;
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
            items.push("- [run " + baml.unstable.string(i) + "](" + link + ")");
            i = i + 1;
        }
        parts.push("## Evidence\n\n" + items.join("\n"));
    }
    return parts.join("\n\n");
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
