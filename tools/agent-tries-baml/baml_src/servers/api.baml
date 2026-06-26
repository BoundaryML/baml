// The api gateway — the SOLE Convex client. Translates REST over http.Server
// into Convex HTTP function calls (core/convex.baml). Ports services/api. Every
// processor + ingress talks only to this. Reuses the HTTP helpers defined in
// servers/proxy.baml (json_response / text_response / path_only / bearer_ok).
//
// Routes (REST -> Convex function):
//   GET    /healthz                       liveness (open)
//   POST   /{table}                       {table}:create   {doc}
//   GET    /{table}?params                {table}:list     params
//   GET    /{table}/{id}                  {table}:get      {id}      (404 on null)
//   PATCH  /{table}/{id}                  {table}:update   {id,patch}
//   DELETE /{table}/{id}                  {table}:remove   {id}
//   POST   /{table}/claim                 {table}:claim    (body forwarded)
//   POST   /{table}/{id}/transition       {table}:transition {id,...}
//   POST   /{table}/{id}/heartbeat        {table}:heartbeat  {id,leaseMs}
//   POST   /workers/heartbeat             workers:upsert   (body forwarded)
//   GET    /{table}/events                SSE stream of {table}:countClaimable
//                                         (instant processor wakes; nightly only)
//
// TODO: /baml/* (version mgmt) and /ingest/* (local run upload) + blob storage.

// ---- path helpers ----------------------------------------------------------

function path_segments(path: string) -> string[] {
    let out: string[] = [];
    for (let p in path.split("/")) {
        if (p != "") {
            out.push(p);
        }
    }
    return out;
}

function seg(segs: string[], i: int) -> string {
    return match (segs.at(i)) {
        let s: string => s,
        null => "",
    };
}

// Insert "id" as the first key of a JSON object body (for transition/heartbeat,
// where Convex wants id alongside the posted fields).
function merge_id(id: string, body: string) -> string {
    let t = body.trim();
    if (t == "" || t == "{}") {
        return "{\"id\":\"" + id + "\"}";
    }
    let rest = t.substring(1, t.length());
    return "{\"id\":\"" + id + "\"," + rest;
}

// Parse a URL query string into a JSON object, coercing all-digit values to
// numbers (e.g. limit) and quoting the rest.
function query_to_json(url: string) -> string {
    let parts = url.split("?");
    if (parts.length() < 2) {
        return "{}";
    }
    let qs = seg_at(parts, 1);
    let pieces: string[] = [];
    for (let pair in qs.split("&")) {
        let kv = pair.split("=");
        if (kv.length() == 2) {
            let k = seg_at(kv, 0);
            let v = seg_at(kv, 1);
            let val = if (is_digits(v)) {
                v
            } else {
                "\"" + v + "\""
            };
            pieces.push("\"" + k + "\":" + val);
        }
    }
    return "{" + pieces.join(",") + "}";
}

function seg_at(xs: string[], i: int) -> string {
    return match (xs.at(i)) {
        let s: string => s,
        null => "",
    };
}

function is_digits(s: string) -> bool {
    if (s == "") {
        return false;
    }
    // NB: "5".split("") -> ["", "5", ""] (boundary empties); skip them.
    for (let ch in s.split("")) {
        if (ch != "" && (ch < "0" || ch > "9")) {
            return false;
        }
    }
    return true;
}

// ---- Convex-call handlers (each catches and returns 502 on failure) ---------

function api_create(gw: ConvexGateway, table: string, body: string) -> baml.http.Response {
    let res = gw.mutation(table + ":create", "{\"doc\":" + body + "}") catch (_) {
        _ => null
    };
    // Convex create returns the bare id (a JSON string); ServiceClient.create
    // expects {"id": ...}, so wrap it to match the contract.
    return match (res) {
        let v: string => json_response(200, "{\"id\":" + v + "}"),
        null => json_response(502, "{\"error\":\"convex create failed\"}"),
    };
}

function api_get(gw: ConvexGateway, table: string, id: string) -> baml.http.Response {
    let res = gw.query(table + ":get", "{\"id\":\"" + id + "\"}") catch (_) {
        _ => null
    };
    return match (res) {
        let v: string => {
            if (v.trim() == "null") {
                json_response(404, "{\"error\":\"not found\"}")
            } else {
                json_response(200, v)
            }
        },
        null => json_response(502, "{\"error\":\"convex get failed\"}"),
    };
}

function api_list(gw: ConvexGateway, table: string, url: string) -> baml.http.Response {
    let res = gw.query(table + ":list", query_to_json(url)) catch (_) {
        _ => null
    };
    return convex_ok(res);
}

function api_update(
    gw: ConvexGateway,
    table: string,
    id: string,
    body: string,
) -> baml.http.Response {
    let args = "{\"id\":\"" + id + "\",\"patch\":" + body + "}";
    let res = gw.mutation(table + ":update", args) catch (_) {
        _ => null
    };
    return convex_ok(res);
}

function api_remove(gw: ConvexGateway, table: string, id: string) -> baml.http.Response {
    let res = gw.mutation(table + ":remove", "{\"id\":\"" + id + "\"}") catch (_) {
        _ => null
    };
    return convex_ok(res);
}

function api_claim(gw: ConvexGateway, table: string, body: string) -> baml.http.Response {
    let res = gw.mutation(table + ":claim", body) catch (_) {
        _ => null
    };
    return convex_ok(res);
}

function api_transition(
    gw: ConvexGateway,
    table: string,
    id: string,
    body: string,
) -> baml.http.Response {
    let res = gw.mutation(table + ":transition", merge_id(id, body)) catch (_) {
        _ => null
    };
    return convex_ok(res);
}

function api_heartbeat(
    gw: ConvexGateway,
    table: string,
    id: string,
    body: string,
) -> baml.http.Response {
    let res = gw.mutation(table + ":heartbeat", merge_id(id, body)) catch (_) {
        _ => null
    };
    return convex_ok(res);
}

function api_worker_upsert(gw: ConvexGateway, body: string) -> baml.http.Response {
    let res = gw.mutation("workers:upsert", body) catch (_) {
        _ => null
    };
    return convex_ok(res);
}

function convex_ok(res: string?) -> baml.http.Response {
    return match (res) {
        let v: string => json_response(200, v),
        null => json_response(502, "{\"error\":\"convex call failed\"}"),
    };
}

// ---- SSE wake stream (GET /{table}/events) ---------------------------------
// Streaming response; a spawned writer polls {table}:countClaimable and emits
// `data: {"count":N}` on every change, so processors wake instantly instead of
// waiting on their poll backstop. Mirrors convex_gateway.subscribe_counts.

function api_events(gw: ConvexGateway, table: string, url: string) -> baml.http.Response {
    let resp = baml
        .http
        .Response
        .new_streaming(200, { "Content-Type": "text/event-stream", "Cache-Control": "no-cache" });
    let args = query_to_json(url);
    let w = resp;
    let _ = spawn { events_writer(gw, table, args, w) };
    return resp;
}

function events_writer(
    gw: ConvexGateway,
    table: string,
    args: string,
    resp: baml.http.Response,
) -> int {
    let last = -1;
    let alive = true;
    while (alive) {
        let count = poll_count(gw, table, args);
        if (count >= 0 && count != last) {
            last = count;
            if (sse_write(resp, "data: {\"count\":" + baml.unstable.string(count) + "}\n\n") == false) {
                alive = false;
            }
        }
        if (alive) {
            let _ = baml.sys.sleep(baml.time.Duration.from_milliseconds(2000)) catch (_) {
                _ => null
            };
        }
    }
    return 0;
}

function poll_count(gw: ConvexGateway, table: string, args: string) -> int {
    let res = gw.query(table + ":countClaimable", args) catch (_) {
        _ => "-1"
    };
    return baml.json.from_string<int>(res.trim()) catch (_) {
        _ => -1
    };
}

// Write one SSE chunk; false when the client has disconnected (write throws).
function sse_write(resp: baml.http.Response, s: string) -> bool {
    let ok = true;
    let _ = resp.write(s.to_utf8()) catch (_) {
        _ => {
            ok = false;
            null
        }
    };
    return ok;
}

// ---- router + entrypoint ---------------------------------------------------

function api_handler(req: baml.http.Request) -> baml.http.Response {
    let path = path_only(req.url);
    if (req.method == "GET" && path == "/healthz") {
        return text_response(200, "ok");
    }
    if (bearer_ok(req, service_token()) == false) {
        return json_response(401, "{\"error\":\"unauthorized\"}");
    }

    let gw = convex_gateway_from_env();
    let segs = path_segments(path);
    let n = segs.length();
    let m = req.method;

    // Transcript blob serving: GET /transcripts/<storage-id> (the id is a path like
    // "tasks/<id>.txt", so it spans multiple segments). The website's transcript
    // proxy reads this; the worker writes via POST /{table}/{id}/transcript below.
    if (m == "GET" && n >= 2 && seg(segs, 0) == "transcripts") {
        return api_serve_transcript(join_from(segs, 1));
    }

    if (n == 1) {
        let table = seg(segs, 0);
        if (m == "POST") {
            return api_create(gw, table, req.body);
        }
        if (m == "GET") {
            return api_list(gw, table, req.url);
        }
    }

    if (n == 2) {
        let table = seg(segs, 0);
        let second = seg(segs, 1);
        if (table == "workers" && second == "heartbeat" && m == "POST") {
            return api_worker_upsert(gw, req.body);
        }
        if (table == "promo" && second == "claim" && m == "POST") {
            // Promo inventory is a SEPARATE Convex deployment (PROMO_CONVEX_URL).
            // claimNext returns the JSON-encoded code ("CODE") or null. Wrap it as
            // {"code": ...} so ServiceClient.promo_claim's extract_field("code") finds
            // it (a bare "CODE" string has no "code" field -> always read as no-code).
            let res = promo_gateway_from_env().mutation("promoCodes:claimNext", req.body) catch (_) {
                _ => null
            };
            let code = match (res) {
                let v: string => v,
                null => "null",
            };
            return json_response(200, "{\"code\":" + code + "}");
        }
        if (second == "claim" && m == "POST") {
            return api_claim(gw, table, req.body);
        }
        if (second == "events" && m == "GET") {
            return api_events(gw, table, req.url);
        }
        // else [table, id]
        if (m == "GET") {
            return api_get(gw, table, second);
        }
        if (m == "PATCH") {
            return api_update(gw, table, second, req.body);
        }
        if (m == "DELETE") {
            return api_remove(gw, table, second);
        }
    }

    if (n == 3) {
        let table = seg(segs, 0);
        let id = seg(segs, 1);
        let action = seg(segs, 2);
        if (action == "transition" && m == "POST") {
            return api_transition(gw, table, id, req.body);
        }
        if (action == "heartbeat" && m == "POST") {
            return api_heartbeat(gw, table, id, req.body);
        }
        // Store a transcript blob on the volume; returns its storage id.
        if (action == "transcript" && m == "POST") {
            return api_store_transcript(table, id, req.body);
        }
    }

    return json_response(404, "{\"error\":\"not found\"}");
}

// ---- transcript blob store (on-volume; Convex's 1MB cap can't hold transcripts) -

// Reject empty/absolute/".."-bearing keys (path-traversal guard).
function safe_blob_key(key: string) -> bool {
    if (key == "" || key.starts_with("/")) {
        return false;
    }
    for (let part in key.split("/")) {
        if (part == "..") {
            return false;
        }
    }
    return true;
}

// Join path segments from `start` onward with "/".
function join_from(segs: string[], start: int) -> string {
    let parts: string[] = [];
    let i = 0;
    for (let s in segs) {
        if (i >= start) {
            parts.push(s);
        }
        i = i + 1;
    }
    return parts.join("/");
}

// POST /{table}/{id}/transcript -> write the body to BLOB_DIR/{table}/{id}.txt,
// return {"storageId": "{table}/{id}.txt"} (the worker records it on the trophy).
function api_store_transcript(table: string, id: string, body: string) -> baml.http.Response {
    let sid = table + "/" + id + ".txt";
    if (safe_blob_key(sid) == false) {
        return json_response(400, "{\"error\":\"bad key\"}");
    }
    let _ = baml.fs.mkdir(blob_dir() + "/" + table, baml.fs.MkdirOptions { recursive: true }) catch (_) {
        _ => null
    };
    let _ = baml.fs.write(blob_dir() + "/" + sid, body) catch (_) {
        _ => 0
    };
    return json_response(200, "{\"storageId\":" + jesc(sid) + "}");
}

// GET /transcripts/{key} -> the stored blob as text/plain, or 404.
function api_serve_transcript(key: string) -> baml.http.Response {
    if (safe_blob_key(key) == false) {
        return text_response(400, "bad key");
    }
    let content = baml.fs.read(blob_dir() + "/" + key) catch (_) {
        _ => null
    };
    return match (content) {
        let c: string => text_response(200, c),
        null => text_response(404, "not found"),
    };
}

// `baml run Api` entrypoint.
function Api() -> int throws baml.errors.Io {
    let addr = "[::]:" + env_or("PORT", "8080");
    let server = baml.http.Server.bind(addr);
    baml.io.println("api listening on " + server.addr);
    return server.serve(api_handler);
}
