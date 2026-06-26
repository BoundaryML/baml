// Slack client. Ports libs/bench_core/slack_client.py. post_message (the one the
// pipeline uses for acks/results), fetch_thread, users_info. Bearer auth; no-ops
// quietly when token/channel are missing (best-effort, like the Python).

function slack_post_message(
    token: string,
    channel: string,
    text: string,
    thread_ts: string,
) -> string {
    if (token == "" || channel == "") {
        return "";
    }
    let body = match (thread_ts) {
        "" => "{\"channel\":" + jesc(channel) + ",\"text\":" + jesc(text) + "}",
        _ => {
            "{\"channel\":"
                + jesc(channel)
                + ",\"text\":"
                + jesc(text)
                + ",\"thread_ts\":"
                + jesc(thread_ts)
                + "}"
        },
    };
    let req = baml.http.Request {
        method: "POST",
        url: "https://slack.com/api/chat.postMessage",
        headers: { "Authorization": "Bearer " + token, "Content-Type": "application/json; charset=utf-8" },
        body: body,
    };
    let resp = baml.http.send(req) catch (_) {
        _ => baml.http.Response.new(0, {}, "".to_utf8())
    };
    let text2 = resp.text() catch (_) {
        _ => ""
    };
    // Return the posted message ts (or "" on failure).
    return extract_field(text2, "ts");
}

// Fetch a thread's messages (conversations.replies); returns the raw JSON.
function slack_fetch_thread(token: string, channel: string, thread_ts: string) -> string {
    if (token == "" || channel == "") {
        return "{}";
    }
    let url = "https://slack.com/api/conversations.replies?channel="
        + channel
        + "&ts="
        + thread_ts
        + "&limit=50";
    let req = baml.http.Request {
        method: "GET",
        url: url,
        headers: { "Authorization": "Bearer " + token },
        body: "",
    };
    let resp = baml.http.send(req) catch (_) {
        _ => baml.http.Response.new(0, {}, "{}".to_utf8())
    };
    return resp.text() catch (_) {
        _ => "{}"
    };
}
