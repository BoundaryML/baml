// Cursor Cloud Agents client. Ports libs/bench_core/cursor_client.py. Launches a
// background fix agent (idempotent via a stable agentId), polls its state, finds
// its PR, and adds follow-ups. HTTP Basic auth: the api key as username, empty
// password. Methods return raw JSON (the tracker/FixDispatch parse what they need).

function cursor_base() -> string {
    return env_or("CURSOR_API_BASE", "https://api.cursor.com");
}

// "Basic base64(api_key:)" — Cursor's HTTP Basic convention.
function cursor_auth(api_key: string) -> string {
    return "Basic " + (api_key + ":").to_utf8().to_base64();
}

// POST /v1/agents — launch (or idempotently re-attach via agent_id). Returns raw
// JSON; a 409 agent_id_conflict body is returned as-is ("already launched").
//
// Body shape mirrors the production client exactly (libs/bench_core/cursor_client.py):
//   { prompt:{text}, repos:[{url, startingRef, prUrl?}], autoCreatePR:true,
//     workOnCurrentBranch?:true, agentId?, model?:{id} }
// `pr_url` targets an existing PR (commits land on its head branch); pair it with
// `work_on_current_branch` so a refix updates that PR instead of cutting a new one.
function cursor_launch_agent(
    api_key: string,
    prompt_text: string,
    repo_url: string,
    ref: string,
    agent_id: string,
    model: string,
    pr_url: string,
    work_on_current_branch: bool,
) -> string throws baml.errors.Io | baml.errors.Timeout {
    let pr_part = match (pr_url) {
        "" => "",
        _ => ",\"prUrl\":" + jesc(pr_url),
    };
    let repo = "{\"url\":" + jesc(repo_url) + ",\"startingRef\":" + jesc(ref) + pr_part + "}";
    let wob_part = if (work_on_current_branch) {
        ",\"workOnCurrentBranch\":true"
    } else {
        ""
    };
    let id_part = match (agent_id) {
        "" => "",
        _ => ",\"agentId\":" + jesc(agent_id),
    };
    let model_part = match (model) {
        "" => "",
        _ => ",\"model\":{\"id\":" + jesc(model) + "}",
    };
    let body = "{\"prompt\":{\"text\":"
        + jesc(prompt_text)
        + "},\"repos\":["
        + repo
        + "],\"autoCreatePR\":true"
        + wob_part
        + id_part
        + model_part
        + "}";
    let req = baml.http.Request {
        method: "POST",
        url: cursor_base() + "/v1/agents",
        headers: { "Authorization": cursor_auth(api_key), "Content-Type": "application/json" },
        body: body,
    };
    let resp = baml.http.send(req);
    return resp.text() catch (_) {
        _ => "{}"
    };
}

// ---- run / PR resolution (for the cursor-tracker) --------------------------
// Terminal run statuses — the agent has stopped working (so a missing PR is final
// and a refix is safe to dispatch).
function cursor_run_terminal(status: string) -> bool {
    return status == "FINISHED" || status == "ERROR" || status == "CANCELLED" || status == "EXPIRED";
}

class CursorAgentState {
    latestRunId: string?,
    status: string?,
    createdAt: string?,
}

class CursorBranch {
    repoUrl: string?,
    branch: string?,
    prUrl: string?,
}

class CursorGit {
    branches: CursorBranch[]?,
}

class CursorRunState {
    status: string?,
    git: CursorGit?,
}

// What the tracker needs to advance an issue: the PR (if opened yet), the branch
// the agent pushed, and whether the run has stopped.
class CursorPr {
    pr_url: string?,
    branch: string?,
    repo_url: string?,
    run_terminal: bool,
}

// GET /v1/agents/{id}/runs/{run_id} — the run carries git.branches[] with prUrls.
function cursor_get_run(
    api_key: string,
    agent_id: string,
    run_id: string,
) -> string throws baml.errors.Io | baml.errors.Timeout {
    let req = baml.http.Request {
        method: "GET",
        url: cursor_base() + "/v1/agents/" + agent_id + "/runs/" + run_id,
        headers: { "Authorization": cursor_auth(api_key) },
        body: "",
    };
    let resp = baml.http.send(req);
    return resp.text() catch (_) {
        _ => "{}"
    };
}

// Resolve an agent into its PR state: read the agent (for latestRunId + status),
// then its latest run, and return the first pushed branch carrying a prUrl. Null
// only when the agent has no run yet (caller keeps waiting).
function cursor_pr_for_agent(api_key: string, agent_id: string) -> CursorPr? {
    let agent_raw = cursor_get_agent(api_key, agent_id) catch (_) {
        _ => "{}"
    };
    let agent = baml.json.from_string<CursorAgentState>(agent_raw) catch (_) {
        _ => CursorAgentState { latestRunId: null, status: null, createdAt: null }
    };
    let agent_terminal = match (agent.status) {
        let s: string => cursor_run_terminal(s),
        null => false,
    };
    let run_id = match (agent.latestRunId) {
        let r: string => r,
        null => {
            return null;
        },
    };
    let run_raw = cursor_get_run(api_key, agent_id, run_id) catch (_) {
        _ => "{}"
    };
    let run = baml.json.from_string<CursorRunState>(run_raw) catch (_) {
        _ => CursorRunState { status: null, git: null }
    };
    let run_terminal = match (run.status) {
        let s: string => cursor_run_terminal(s),
        null => agent_terminal,
    }
        || agent_terminal;
    let branches: CursorBranch[] = match (run.git) {
        let g: CursorGit => match (g.branches) {
            let b: CursorBranch[] => b,
            null => [],
        },
        null => [],
    };
    // First branch with a prUrl wins; else surface the first branch (no PR yet).
    for (let b in branches) {
        match (b.prUrl) {
            let u: string => {
                return CursorPr {
                    pr_url: u,
                    branch: b.branch,
                    repo_url: b.repoUrl,
                    run_terminal: run_terminal,
                };
            },
            null => null,
        };
    }
    let first = match (branches.at(0)) {
        let b: CursorBranch => b,
        null => CursorBranch { repoUrl: null, branch: null, prUrl: null },
    };
    return CursorPr {
        pr_url: null,
        branch: first.branch,
        repo_url: first.repoUrl,
        run_terminal: run_terminal,
    };
}

// GET /v1/agents/{id} — current state.
function cursor_get_agent(
    api_key: string,
    agent_id: string,
) -> string throws baml.errors.Io | baml.errors.Timeout {
    let req = baml.http.Request {
        method: "GET",
        url: cursor_base() + "/v1/agents/" + agent_id,
        headers: { "Authorization": cursor_auth(api_key) },
        body: "",
    };
    let resp = baml.http.send(req);
    return resp.text() catch (_) {
        _ => "{}"
    };
}

// POST /v1/agents/{id}/followup — add a follow-up instruction.
function cursor_add_followup(
    api_key: string,
    agent_id: string,
    text: string,
) -> string throws baml.errors.Io | baml.errors.Timeout {
    let req = baml.http.Request {
        method: "POST",
        url: cursor_base() + "/v1/agents/" + agent_id + "/followup",
        headers: { "Authorization": cursor_auth(api_key), "Content-Type": "application/json" },
        body: "{\"prompt\":{\"text\":" + jesc(text) + "}}",
    };
    let resp = baml.http.send(req);
    return resp.text() catch (_) {
        _ => "{}"
    };
}
