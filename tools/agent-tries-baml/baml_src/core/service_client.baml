// Typed HTTP client to the agent-tries-baml API gateway. Every processor and the
// ingress server talk to the service through this — they never touch Convex
// directly (the api server is the sole Convex gateway). Ports
// libs/bench_core/service_client.py over baml.http.send; routes + JSON bodies are
// byte-identical so the existing API contract is preserved.
//
// Doc bodies are passed as pre-serialized JSON strings (callers
// baml.json.stringify their typed docs); reads return the raw JSON text, which
// the caller decodes to its concrete type (baml.json.from_string<T>). This keeps
// the client non-generic while staying fully typed at the call sites.

class ServiceError {
    status: int,
    message: string,
}

// Helper request-body shapes (serialized to the exact JSON the API expects).
class ClaimBody {
    workerId: string,
    leaseMs: int,
    field: string,
    value: string,
    claimedValue: string,
    index: string,
}

class HeartbeatBody {
    leaseMs: int,
}

class WorkerHeartbeatBody {
    workerId: string,
    role: string,
    status: string,
    currentItemId: string?,
}

class IdResp {
    id: string,
}

class StorageResp {
    storageId: string,
}

class ServiceClient {
    base_url: string,
    token: string,

    function headers(self) -> map<string, string> {
        return { "Authorization": "Bearer " + self.token, "Content-Type": "application/json" };
    }

    function url(self, path: string) -> string {
        return self.base_url + path;
    }

    // Low-level send. Throws ServiceError on any non-2xx response.
    function send(
        self,
        method: string,
        path: string,
        body: string,
    ) -> baml.http.Response throws ServiceError | baml.errors.Io | baml.errors.Timeout {
        let req = baml.http.Request {
            method: method,
            url: self.url(path),
            headers: self.headers(),
            body: body,
        };
        let resp = baml.http.send(req);
        if (resp.ok() == false) {
            throw ServiceError { status: resp.status_code, message: resp.text() };
        }
        return resp;
    }

    // ---------- CRUD ----------

    // POST /{table} ; returns the new doc id.
    function create(
        self,
        table: string,
        doc_json: string,
    ) -> string throws ServiceError | baml.errors.Io | baml.errors.Timeout {
        let resp = self.send("POST", "/" + table, doc_json);
        let parsed = baml.json.from_string<IdResp>(resp.text()) catch (_) {
            _ => {
                throw ServiceError { status: 0, message: "bad create response" }
            }
        };
        return parsed.id;
    }

    // GET /{table}/{id} ; null on 404, raw doc JSON otherwise.
    function get(
        self,
        table: string,
        id: string,
    ) -> string? throws ServiceError | baml.errors.Io | baml.errors.Timeout {
        let req = baml.http.Request {
            method: "GET",
            url: self.url("/" + table + "/" + id),
            headers: self.headers(),
            body: "",
        };
        let resp = baml.http.send(req);
        if (resp.status_code == 404) {
            return null;
        }
        if (resp.ok() == false) {
            throw ServiceError { status: resp.status_code, message: resp.text() };
        }
        return resp.text();
    }

    // GET /{table}?field=&value=&index=&limit= ; raw JSON array of docs ("[]" on
    // error). Used by the cursor-tracker's periodic sweep over issue statuses.
    function list(
        self,
        table: string,
        field: string,
        value: string,
        index: string,
        limit: int,
    ) -> string throws ServiceError | baml.errors.Io | baml.errors.Timeout {
        let qs = "?field="
            + field
            + "&value="
            + value
            + "&index="
            + index
            + "&limit="
            + baml.unstable.string(limit);
        let req = baml.http.Request {
            method: "GET",
            url: self.url("/" + table + qs),
            headers: self.headers(),
            body: "",
        };
        let resp = baml.http.send(req);
        if (resp.ok() == false) {
            throw ServiceError { status: resp.status_code, message: resp.text() };
        }
        return resp.text();
    }

    // PATCH /{table}/{id} ; returns the updated doc JSON.
    function update(
        self,
        table: string,
        id: string,
        patch_json: string,
    ) -> string throws ServiceError | baml.errors.Io | baml.errors.Timeout {
        let resp = self.send("PATCH", "/" + table + "/" + id, patch_json);
        return resp.text();
    }

    // DELETE /{table}/{id}.
    function remove(
        self,
        table: string,
        id: string,
    ) -> int throws ServiceError | baml.errors.Io | baml.errors.Timeout {
        let resp = self.send("DELETE", "/" + table + "/" + id, "");
        return resp.status_code;
    }

    // ---------- queue verbs ----------

    // POST /{table}/claim ; null when the queue is empty, raw doc JSON otherwise.
    function claim(
        self,
        table: string,
        worker_id: string,
        lease_ms: int,
        value: string,
        claimed_value: string,
        field: string,
        index: string,
    ) -> string? throws ServiceError | baml.errors.Io | baml.errors.Timeout {
        let body = baml
            .json
            .to_string(
                ClaimBody {
                    workerId: worker_id,
                    leaseMs: lease_ms,
                    field: field,
                    value: value,
                    claimedValue: claimed_value,
                    index: index,
                },
            ) catch (_) {
            _ => {
                throw ServiceError { status: 0, message: "serialize claim failed" }
            }
        };
        let resp = self.send("POST", "/" + table + "/claim", body);
        let text = resp.text().trim();
        if (text == "" || text == "null") {
            return null;
        }
        return text;
    }

    // POST /{table}/{id}/transition ; transition the SAME field that was claimed.
    // `patch_json` is a raw JSON object string (e.g. "{}" or a serialized patch),
    // embedded as a real object in the body. `to`/`field` are controlled values.
    function transition(
        self,
        table: string,
        id: string,
        to: string,
        field: string,
        patch_json: string,
        release_claim: bool,
    ) -> string throws ServiceError | baml.errors.Io | baml.errors.Timeout {
        let rel = if (release_claim) {
            "true"
        } else {
            "false"
        };
        let body = "{\"to\":\""
            + to
            + "\",\"field\":\""
            + field
            + "\",\"patch\":"
            + patch_json
            + ",\"releaseClaim\":"
            + rel
            + "}";
        let resp = self.send("POST", "/" + table + "/" + id + "/transition", body);
        return resp.text();
    }

    // POST /{table}/{id}/heartbeat ; extend the lease on a long job.
    function heartbeat(
        self,
        table: string,
        id: string,
        lease_ms: int,
    ) -> int throws ServiceError | baml.errors.Io | baml.errors.Timeout {
        let body = baml.json.to_string(HeartbeatBody { leaseMs: lease_ms }) catch (_) {
            _ => {
                throw ServiceError { status: 0, message: "serialize heartbeat failed" }
            }
        };
        let resp = self.send("POST", "/" + table + "/" + id + "/heartbeat", body);
        return resp.status_code;
    }

    // ---------- presence ----------

    // POST /workers/heartbeat ; upsert this worker's roster row.
    function worker_heartbeat(
        self,
        worker_id: string,
        role: string,
        status: string,
        current_item_id: string?,
    ) -> int throws ServiceError | baml.errors.Io | baml.errors.Timeout {
        let body = baml
            .json
            .to_string(
                WorkerHeartbeatBody {
                    workerId: worker_id,
                    role: role,
                    status: status,
                    currentItemId: current_item_id,
                },
            ) catch (_) {
            _ => {
                throw ServiceError { status: 0, message: "serialize worker heartbeat failed" }
            }
        };
        let resp = self.send("POST", "/workers/heartbeat", body);
        return resp.status_code;
    }

    // ---------- promo codes ----------

    // POST /promo/claim ; returns the claimed code, or "" when out of stock.
    function promo_claim(
        self,
        claimed_by: string,
        claimed_by_user_id: string,
        notes: string,
    ) -> string throws ServiceError | baml.errors.Io | baml.errors.Timeout {
        let body = "{\"claimedBy\":"
            + jesc(claimed_by)
            + ",\"claimedByUserId\":"
            + jesc(claimed_by_user_id)
            + ",\"notes\":"
            + jesc(notes)
            + "}";
        let resp = self.send("POST", "/promo/claim", body);
        return extract_field(resp.text(), "code");
    }

    // ---------- blobs ----------

    // POST /{table}/{id}/transcript ; returns the storage id.
    function put_transcript(
        self,
        table: string,
        id: string,
        text: string,
    ) -> string throws ServiceError | baml.errors.Io | baml.errors.Timeout {
        let resp = self.send("POST", "/" + table + "/" + id + "/transcript", text);
        let parsed = baml.json.from_string<StorageResp>(resp.text()) catch (_) {
            _ => {
                throw ServiceError { status: 0, message: "bad transcript response" }
            }
        };
        return parsed.storageId;
    }

    // GET /transcripts/{storage_id} ; the transcript body.
    function get_transcript(
        self,
        storage_id: string,
    ) -> string throws ServiceError | baml.errors.Io | baml.errors.Timeout {
        let resp = self.send("GET", "/transcripts/" + storage_id, "");
        return resp.text();
    }

    // ---------- SSE wake stream ----------

    // GET /{table}/events ; long-lived SSE stream of the claimable count. The
    // processor pulls `.next()` (null => closed) and parses each `data:` line.
    function events(
        self,
        table: string,
        value: string,
        field: string,
        index: string,
    ) -> baml.http.SseStream throws baml.errors.Io | baml.errors.Timeout {
        let path = "/" + table + "/events?field=" + field + "&value=" + value + "&index=" + index;
        let req = baml.http.Request { method: "GET", url: self.url(path), headers: self.headers(), body: "" };
        return baml.http.fetch_sse(req);
    }
}
