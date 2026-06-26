// Client for the claude-proxy /run-agent and /check-baml endpoints. Workers and
// dedup spread load across a pool of stateless proxies. Ports
// libs/bench_core/proxy_client.py.
//
// Note: BAML bans Math.random (determinism for resume), so load is spread with
// now_ms() % len instead of random.choice — equivalent for fan-out across N
// stateless proxies. Reuses ServiceError (core/service_client.baml) for a
// uniform error surface.

class ProxyClient {
    urls: string[],
    token: string,

    function headers(self) -> map<string, string> {
        let h: map<string, string> = { "Content-Type": "application/json" };
        if (self.token != "") {
            h.set("Authorization", "Bearer " + self.token);
        }
        return h;
    }

    // Pick a proxy URL, spreading load by wall-clock.
    function pick(self) -> string throws ServiceError {
        let n = self.urls.length();
        if (n == 0) {
            throw ServiceError { status: 0, message: "no proxy URLs configured" };
        }
        let idx = baml.sys.now_ms() % n;
        match (self.urls.at(idx)) {
            let u: string => u,
            null => {
                throw ServiceError { status: 0, message: "proxy index out of range" }
            },
        }
    }

    // POST a JSON body to `path` on a chosen proxy; returns the response text.
    function post(
        self,
        path: string,
        body: string,
    ) -> string throws ServiceError | baml.errors.Io | baml.errors.Timeout {
        let req = baml.http.Request {
            method: "POST",
            url: self.pick() + path,
            headers: self.headers(),
            body: body,
        };
        let resp = baml.http.send(req);
        if (resp.ok() == false) {
            throw ServiceError { status: resp.status_code, message: resp.text() };
        }
        return resp.text();
    }

    // POST /run-agent ; runs a claude agent against a staged cell.
    function run_agent(
        self,
        req: RunAgentRequest,
    ) -> AgentResult throws ServiceError | baml.errors.Io | baml.errors.Timeout {
        let body = baml.json.to_string(req) catch (_) {
            _ => {
                throw ServiceError { status: 0, message: "serialize run-agent failed" }
            }
        };
        let text = self.post("/run-agent", body);
        return baml.json.from_string<AgentResult>(text) catch (_) {
            _ => {
                throw ServiceError { status: 0, message: "bad run-agent response" }
            }
        };
    }

    // POST /check-baml ; compiles/runs a minimal repro (no agent).
    function check_baml(
        self,
        req: CheckBamlRequest,
    ) -> CheckBamlResult throws ServiceError | baml.errors.Io | baml.errors.Timeout {
        let body = baml.json.to_string(req) catch (_) {
            _ => {
                throw ServiceError { status: 0, message: "serialize check-baml failed" }
            }
        };
        let text = self.post("/check-baml", body);
        return baml.json.from_string<CheckBamlResult>(text) catch (_) {
            _ => {
                throw ServiceError { status: 0, message: "bad check-baml response" }
            }
        };
    }
}
