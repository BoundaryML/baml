/// (internal) Pure-BAML local HTTP mock for concurrency and JEV tests.
class MockServer {
    server: baml.http.Server,
    requests: baml.http.ServerRequest[],
    completed: bool[],
    overlap: int[],
    fail: bool,

    function base_url(self) -> string {
        "http://" + self.server.addr
    }

    function observed_peak(self) -> int {
        let peak = 0;
        for (let count in self.overlap) {
            if (count > peak) {
                peak = count;
            }
        }
        peak
    }

    function respond(self, req: baml.http.ServerRequest) -> baml.http.Response {
        baml.sys.sleep(
            baml.time.Duration.from_milliseconds(
                if (req.url.includes("timeout")) {
                    120
                } else {
                    60
                },
            ),
        );
        if (req.method != "POST") {
            return baml.http.Response.new(200, {}, "ok".to_utf8());
        }
        let state = baml.json.path<string>(baml.json.parse(req.body), ".state").trim();
        let applies = !state.starts_with("file:") || state.includes("src/timeout.rs");
        let body = if (self.fail) {
            `{"model":"jev-latest","answers":{},"usage":{"input_tokens":1,"output_tokens":1}}`
        } else if (applies) {
            `{"model":"jev-latest","answers":{"result":{"type":"noul","noul":0.91}},"usage":{"input_tokens":1,"output_tokens":1}}`
        } else {
            `{"model":"jev-latest","answers":{"result":{"type":"noul","noul":0.1}},"usage":{"input_tokens":1,"output_tokens":1}}`
        };
        baml.http.Response.new(200, { "content-type": "application/json" }, body.to_utf8())
    }

    function handle(self, req: baml.http.ServerRequest) -> baml.http.Response throws never {
        self.requests.push(req);
        // Append-only observations avoid read/modify/write races between handlers.
        self.overlap.push(self.requests.length() - self.completed.length());
        let response = self.respond(req) catch_all (_) {
            _ => baml.http.Response.new(500, {}, "mock error".to_utf8()),
        };
        self.completed.push(true);
        response
    }
}

function with_mock<T, E>(body: (mock: MockServer) -> T throws E) -> T throws E | baml.errors.Io {
    let mock = MockServer {
        server: baml.http.Server.bind("127.0.0.1:0"),
        requests: [],
        completed: [],
        overlap: [],
        fail: false,
    };
    let task = spawn { mock.server.serve(mock.handle) };
    defer { task.cancel(); }
    body(mock)
}

function io_sources(mock: MockServer) -> map<string, string> {
    let url = mock.base_url();
    {
        "slow.baml": "function slow(change: codeturtle.FileChange) -> codeturtle.Match {\n"
            + "let response = baml.http.fetch(\""
            + url
            + "/\" + change.path);\n"
            + "codeturtle.Match {path: change.path, verdict: codeturtle.Verdict.Applies, reason: \"Checked\", evidence: [response.text()]} }\n"
            + "function slow_files(changes: codeturtle.FileChange[]) -> codeturtle.Match[] {\n"
            + "let response = baml.http.fetch(\""
            + url
            + "/files\"); let text = response.text();\n"
            + "let results: codeturtle.Match[] = []; for (let change in changes) {\n"
            + "results.push(codeturtle.Match {path: change.path, verdict: codeturtle.Verdict.Applies, reason: \"Checked files\", evidence: [text]}); } results }\n"
            + "function slow_pr(pr: codeturtle.PullRequest) -> codeturtle.Match[] { slow_files(pr.changes) }\n"
            + "function faulty(change: codeturtle.FileChange) -> codeturtle.Match {\n"
            + "let result = slow(change); if (change.path == \"src/timeout.rs\") {throw \"Intentional failure\";} result }\n",
    }
}

function ai_sources(mock: MockServer) -> map<string, string> {
    let client_clause = "client: typesafeai.Client.new(api_key = \"test-key\", base_url = \"" + mock.base_url() + "\") ";
    let instructions = "${role(\"instructions\")} Does this change require human review? ${role(\"user\")} ";
    {
        "ai.baml": "function needs_file_review(change: codeturtle.FileChange) -> bool { "
            + client_clause
            + "prompt: `"
            + instructions
            + "file: ${change.path}` }\n"
            + "function needs_files_review(changes: codeturtle.FileChange[]) -> bool { "
            + client_clause
            + "prompt: `"
            + instructions
            + "files: ${changes.length()}` }\n"
            + "function needs_pr_review(pr: codeturtle.PullRequest) -> bool { "
            + client_clause
            + "prompt: `"
            + instructions
            + "pr: ${pr.title}` }\n",
    }
}
