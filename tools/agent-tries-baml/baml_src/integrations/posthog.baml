// PostHog client. Ports libs/bench_core/posthog_client.py. Runs a HogQL query and
// returns the raw response JSON (the @bammy posthog_query handler formats it).

class PosthogError {
    message: string,
}

function posthog_configured() -> bool {
    return env_opt("POSTHOG_API_KEY") != null && env_opt("POSTHOG_PROJECT_ID") != null;
}

function posthog_host() -> string {
    return env_or("POSTHOG_HOST", "https://us.posthog.com");
}

// POST /api/projects/{project}/query/ with a HogQLQuery; returns raw result JSON.
function posthog_hogql(
    query: string,
) -> string throws PosthogError | baml.errors.Io | baml.errors.Timeout {
    let key = env_or("POSTHOG_API_KEY", "");
    let project = env_or("POSTHOG_PROJECT_ID", "");
    if (key == "" || project == "") {
        throw PosthogError { message: "posthog not configured" };
    }
    let body = "{\"query\":{\"kind\":\"HogQLQuery\",\"query\":" + jesc(query) + "}}";
    let req = baml.http.Request {
        method: "POST",
        url: posthog_host() + "/api/projects/" + project + "/query/",
        headers: { "Authorization": "Bearer " + key, "Content-Type": "application/json" },
        body: body,
    };
    let resp = baml.http.send(req);
    if (resp.ok() == false) {
        throw PosthogError { message: "posthog rejected query (" + baml.unstable.string(resp.status_code) + "): " + tail_str(resp.text(), 300) };
    }
    return resp.text();
}
