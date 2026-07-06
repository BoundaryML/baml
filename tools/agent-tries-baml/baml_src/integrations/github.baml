// GitHub REST client. Ports libs/bench_core/github_client.py. The methods the
// builder (releases) + cursor-tracker (PR/CI/review state) use. Bearer auth.
// HTTP methods return raw JSON; the ci_state/coderabbit_state derivers do
// lightweight string heuristics over that JSON (the Python parses structurally).

function github_api_base() -> string {
    return env_or("GITHUB_API_BASE", "https://api.github.com");
}

function github_headers(token: string) -> map<string, string> {
    let h: map<string, string> = {
        "Accept": "application/vnd.github+json",
        "X-GitHub-Api-Version": "2022-11-28",
        "User-Agent": "agent-tries-baml-v2",
    };
    if (token != "") {
        h.set("Authorization", "Bearer " + token);
    }
    return h;
}

function github_get(
    token: string,
    path: string,
) -> string throws baml.errors.Io | baml.errors.Timeout {
    let req = baml.http.Request {
        method: "GET",
        url: github_api_base() + path,
        headers: github_headers(token),
        body: "",
    };
    let resp = baml.http.send(req);
    return resp.text() catch (_) {
        _ => "[]"
    };
}

// GET /repos/{o}/{r}/releases (newest first); raw JSON. Used by the builder to
// resolve the latest nightly/canary release + its asset.
function github_releases(
    token: string,
    owner: string,
    repo: string,
) -> string throws baml.errors.Io | baml.errors.Timeout {
    return github_get(token, "/repos/" + owner + "/" + repo + "/releases?per_page=20");
}

// Wider window for the changelog sweep (a multi-day backlog of nightlies can
// exceed 20; the old sync scanned 100).
function github_releases_wide(
    token: string,
    owner: string,
    repo: string,
) -> string throws baml.errors.Io | baml.errors.Timeout {
    return github_get(token, "/repos/" + owner + "/" + repo + "/releases?per_page=100");
}

function github_get_pr(
    token: string,
    owner: string,
    repo: string,
    number: int,
) -> string throws baml.errors.Io | baml.errors.Timeout {
    return github_get(token, "/repos/" + owner + "/" + repo + "/pulls/" + baml.unstable.string(number));
}

function github_check_runs(
    token: string,
    owner: string,
    repo: string,
    sha: string,
) -> string throws baml.errors.Io | baml.errors.Timeout {
    return github_get(token, "/repos/" + owner + "/" + repo + "/commits/" + sha + "/check-runs");
}

function github_pr_reviews(
    token: string,
    owner: string,
    repo: string,
    number: int,
) -> string throws baml.errors.Io | baml.errors.Timeout {
    return github_get(token, "/repos/" + owner + "/" + repo + "/pulls/" + baml.unstable.string(number) + "/reviews");
}

function github_issue_comments(
    token: string,
    owner: string,
    repo: string,
    number: int,
) -> string throws baml.errors.Io | baml.errors.Timeout {
    return github_get(
        token,
        "/repos/" + owner + "/" + repo + "/issues/" + baml.unstable.string(number) + "/comments",
    );
}

// ---- typed PR state (for the cursor-tracker) -------------------------------

class GhRef {
    sha: string?,
    ref: string?,
}

class GhPr {
    merged: bool?,
    state: string?, // open | closed
    mergeable_state: string?, // clean | dirty | blocked | behind | unstable | unknown
    head: GhRef?,
    base: GhRef?,
}

// Fetch + parse a PR into its merge/CI-relevant fields. Null on a fetch/parse
// failure (the tracker keeps waiting rather than mis-acting on a bad read).
function github_pr_state(token: string, owner: string, repo: string, number: int) -> GhPr? {
    let raw = github_get_pr(token, owner, repo, number) catch (_) {
        _ => "{}"
    };
    return baml.json.from_string<GhPr>(raw) catch (_) {
        _ => null
    };
}

// The github.com clone URL for an owner/repo (what Cursor wants as `repository`).
function repo_url_from_pr(owner: string, repo: string) -> string {
    return "https://github.com/" + owner + "/" + repo;
}

// owner/repo from a repo URL (https://github.com/o/r[.git]); [] if not a repo URL.
function parse_repo_url(url: string) -> string[] {
    let segs = url.split("github.com/");
    if (segs.length() < 2) {
        return [];
    }
    let rest = match (segs.at(1)) {
        let v: string => v,
        null => "",
    };
    let parts = rest.split("/");
    if (parts.length() < 2) {
        return [];
    }
    let owner = match (parts.at(0)) {
        let v: string => v,
        null => "",
    };
    let repo_raw = match (parts.at(1)) {
        let v: string => v,
        null => "",
    };
    // Strip a trailing ".git" and any trailing slash/query.
    let repo = match (repo_raw.split("?").at(0)) {
        let v: string => v.replace(".git", ""),
        null => repo_raw,
    };
    if (owner == "" || repo == "") {
        return [];
    }
    return [owner, repo];
}

class GhPrRef {
    html_url: string?,
}

// Find the open PR whose head branch is `branch` (head=owner:branch). The fallback
// when Cursor's API doesn't surface a PR it opened. Returns the html_url or "".
function github_open_pr_for_branch(
    token: string,
    owner: string,
    repo: string,
    branch: string,
) -> string {
    let raw = github_get(
        token,
        "/repos/"
            + owner
            + "/"
            + repo
            + "/pulls?state=open&head="
            + owner
            + ":"
            + branch
            + "&per_page=5",
    ) catch (_) {
        _ => "[]"
    };
    let prs: GhPrRef[] = baml.json.from_string<GhPrRef[]>(raw) catch (_) {
        _ => []
    };
    return match (prs.at(0)) {
        let p: GhPrRef => match (p.html_url) {
            let u: string => u,
            null => "",
        },
        null => "",
    };
}

// A PR's inline review comments (CodeRabbit per-line + human inline feedback).
function github_pr_review_comments(
    token: string,
    owner: string,
    repo: string,
    number: int,
) -> string throws baml.errors.Io | baml.errors.Timeout {
    return github_get(
        token,
        "/repos/"
            + owner
            + "/"
            + repo
            + "/pulls/"
            + baml.unstable.string(number)
            + "/comments?per_page=100",
    );
}

// Comment shapes (for human-vs-bot detection on a ready PR).
class GhUser {
    login: string?,
    type: string?, // "User" (human) vs "Bot"
}

class GhComment {
    user: GhUser?,
    body: string?,
    created_at: string?,
    state: string?, // review state (e.g. CHANGES_REQUESTED) — null for inline comments
}

// ---- lightweight state derivers (string heuristics over the check-runs JSON) --

// pending | passing | failing — from a check-runs payload.
function ci_state(check_runs_json: string) -> string {
    if (
        check_runs_json.includes("\"conclusion\":\"failure\"")
            || check_runs_json.includes("\"conclusion\":\"timed_out\"")
            || check_runs_json.includes("\"conclusion\":\"cancelled\"")
    ) {
        return "failing";
    }
    if (
        check_runs_json.includes("\"status\":\"in_progress\"")
            || check_runs_json.includes("\"status\":\"queued\"")
    ) {
        return "pending";
    }
    return "passing";
}

// none | blocking | clear — does CodeRabbit have an open/blocking review?
function coderabbit_state(reviews_json: string) -> string {
    if (reviews_json.includes("coderabbit") == false) {
        return "none";
    }
    if (reviews_json.includes("\"state\":\"CHANGES_REQUESTED\"")) {
        return "blocking";
    }
    return "clear";
}

// owner/repo/number from a PR URL (https://github.com/o/r/pull/N).
function parse_pr_url(url: string) -> string[] {
    let marker = "github.com/";
    let segs = url.split(marker);
    if (segs.length() < 2) {
        return [];
    }
    let rest = match (segs.at(1)) {
        let v: string => v,
        null => "",
    };
    let parts = rest.split("/");
    if (parts.length() < 4) {
        return [];
    }
    let owner = match (parts.at(0)) {
        let v: string => v,
        null => "",
    };
    let repo = match (parts.at(1)) {
        let v: string => v,
        null => "",
    };
    let num = match (parts.at(3)) {
        let v: string => v,
        null => "",
    };
    return [owner, repo, num];
}
