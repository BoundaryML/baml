// mcp — MCP servers as ordinary BAML tools. A library over public
// primitives (baml.sys.start_process, root.raw_tool), not part of
// ai: the core namespace carries no protocol knowledge.
//
// One McpConnection is one server child process speaking JSON-RPC 2.0 over
// stdio, one message per line. tools() projects the server's catalog into
// root.Tool values whose handlers proxy tools/call, so MCP calls are
// executed by the runner, journaled as ToolRequested/ToolCompleted, and
// visible to on_event — with any client.
// Mirrors _planv2/pages/03_how_to/06_use_mcp_tools_with_any_client.md.

/// One JSON-RPC message as one line. Pure; tested with literal values.
function rpc_line(id: int?, method: string, params: map<string, unknown>?) -> string {
    let m: map<string, unknown> = { "jsonrpc": "2.0", "method": method };
    if let i: int = id {
        let _ = m.set("id", i);
    }
    if let p: map<string, unknown> = params {
        let _ = m.set("params", p);
    }
    baml.json.stringify(baml.json.to_json(m))
}

class McpConnection {
    server_name: string,
    p: baml.sys.Process,
    out: baml.sys.ReadPipeLines,
    timeout_ms: int,
    next_id: int,

    /// Spawn the server and perform the initialize handshake. The connection
    /// is live until close(); the caller owns the lifetime.
    function connect(
        name: string,
        command: string,
        args: string[],
        timeout_ms: int = 30000,
    ) -> McpConnection {
        let p = baml.sys.start_process(
            command,
            args,
            baml.sys.ProcessOptions { cwd: null, env: null, timeout_ms: timeout_ms, stdin: null },
        ) catch_all (e) {
            _ => {
                throw root.errors.NetworkFailure { status_code: null,
                    provider: `mcp/${name}`,
                    detail: `the MCP server could not start: ${e.to_string()}`,
                    raw_body: null,
                }
            },
        };
        let conn = McpConnection {
            server_name: name,
            p: p,
            out: p.stdout.lines(),
            timeout_ms: timeout_ms,
            next_id: 1,
        };
        let init: map<string, unknown> = {
            "protocolVersion": "2025-06-18",
            "capabilities": {},
            "clientInfo": { "name": "baml-mcp", "version": "0.1" },
        };
        let _ = conn._request("initialize", init);
        conn._send(rpc_line(null, "notifications/initialized", null));
        conn
    }

    function _send(self, line: string) -> void {
        // Template literals trim trailing whitespace, so `${line}\n` would
        // lose the newline the protocol frames on; the newline is a quoted
        // string, appended before the single write.
        let framed = [line, "\n"].join("");
        let _ = self.p.stdin.write(framed) catch_all (e) {
            _ => {
                throw root.errors.NetworkFailure { status_code: null,
                    provider: `mcp/${self.server_name}`,
                    detail: `writing to the MCP server failed: ${e.to_string()}`,
                    raw_body: null,
                }
            },
        };
    }

    /// (internal) One request/response round-trip. Server notifications and unrelated
    /// ids are skipped; a JSON-RPC error object throws.
    function _request(self, method: string, params: map<string, unknown>) -> json {
        let id = self.next_id;
        self.next_id += 1;
        self._send(rpc_line(id, method, params));
        baml.future.with_timeout(baml.time.Duration.from_milliseconds(self.timeout_ms), () -> {
            while (true) {
                let next = self.out.next() catch_all (e) {
                    _ => {
                        throw root.errors.NetworkFailure { status_code: null,
                            provider: `mcp/${self.server_name}`,
                            detail: `reading from the MCP server failed: ${e.to_string()}`,
                            raw_body: null,
                        }
                    },
                };
                match (next) {
                    let line: string => {
                        let trimmed = line.trim();
                        if (trimmed.length() > 0) {
                            // stdout is protocol-only per the spec; tolerate stray lines
                            let msg = baml.json.parse(trimmed) catch_all (e) {
                                _ => null,
                            };
                            if (baml.json.path_or<int>(msg, ".id", -1) == id) {
                                let err = baml.json.path_or<json>(msg, ".error", null);
                                if (err != null) {
                                    throw root.errors.InvalidRequest {
                                        provider: `mcp/${self.server_name}`,
                                        status_code: baml.json.path_or<int?>(err, ".code", null),
                                        detail: baml.json.path_or<string>(err, ".message", "MCP error"),
                                        raw_body: baml.json.stringify(err),
                                    }
                                }
                                return baml.json.path_or<json>(msg, ".result", null);
                            }
                        }
                    },
                    _ => {
                        throw root.errors.NetworkFailure { status_code: null,
                            provider: `mcp/${self.server_name}`,
                            detail: `the MCP server stream ended before answering ${method}`,
                            raw_body: null,
                        }
                    },
                };
            }
            // unreachable: the loop returns on a matching id or throws at EOF
            throw root.errors.NetworkFailure { status_code: null,
                provider: `mcp/${self.server_name}`,
                detail: `no response for ${method}`,
                raw_body: null,
            }
        }) catch (e) {
            baml.errors.Timeout => {
                throw root.errors.NetworkFailure { status_code: null,
                    provider: `mcp/${self.server_name}`,
                    detail: `reading from the MCP server timed out after ${self.timeout_ms}ms`,
                    raw_body: null,
                }
            },
        }
    }

    /// The server's catalog as ordinary tools. Each tool's inputSchema is
    /// already JSON Schema, so it carries into Tool.input_schema unchanged;
    /// the handler proxies tools/call over this connection.
    function tools(self) -> root.tools.Tool[] {
        let result = self._request("tools/list", {});
        let out: root.tools.Tool[] = [];
        for (let t in baml.json.path_or<json[]>(result, ".tools", [])) {
            let tool_name = baml.json.path_or<string>(t, ".name", "");
            let schema = baml.json.from_json<map<string, unknown>>(
                baml.json.path_or<json>(t, ".inputSchema", {}),
            ) catch_all (e) {
                _ => {
                    let empty: map<string, unknown> = {};
                    empty
                },
            };
            out.push(
                root.tools.raw_tool(
                    tool_name,
                    baml.json.path_or<string>(t, ".description", ""),
                    schema,
                    root.internal._proxy(self, tool_name),
                ),
            );
        }
        out
    }

    /// One tools/call. Text content items join as the tool's result; an
    /// isError result throws, which the runner journals as ToolFailed.
    function call_tool(self, name: string, args: map<string, unknown>) -> string {
        let params: map<string, unknown> = { "name": name, "arguments": args };
        let result = self._request("tools/call", params);
        let texts: string[] = [];
        for (let item in baml.json.path_or<json[]>(result, ".content", [])) {
            if (baml.json.path_or<string>(item, ".type", "") == "text") {
                texts.push(baml.json.path_or<string>(item, ".text", ""));
            }
        }
        let joined = texts.join("\n");
        if (baml.json.path_or<bool>(result, ".isError", false)) {
            throw baml.errors.InvalidArgument { message: `mcp tool ${name} failed: ${joined}` }
        }
        joined
    }

    function close(self) -> void {
        self.p.close();
    }
}
