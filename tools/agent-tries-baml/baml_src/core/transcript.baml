// Pure parsers for Claude Code session transcripts. Ports the critical metrics
// path of libs/bench_core/transcript.py (parse_claude_session): extract the final
// JSON summary line of `claude -p --output-format json` and normalize its
// turns/tool-calls/token counts. The terminal-render and per-turn structured log
// (render_terminal_transcript / parse_turn_log) are a later pass — they feed the
// transcript blob, not the claim-loop metrics.

// Raw summary line shape (extra fields like type/result/cost are ignored).
class ClaudeUsage {
    input_tokens: int?,
    output_tokens: int?,
    total_tokens: int?,
    cache_read_input_tokens: int?,
    cache_creation_input_tokens: int?,
}

class ClaudeSummary {
    num_turns: int?,
    num_tool_calls: int?,
    session_id: string?,
    usage: ClaudeUsage?,
}

// Normalized session metrics (matches the Python dict keys consumers expect).
class ClaudeSession {
    turns: int?,
    tool_calls: int?,
    session_id: string?,
    input_tokens: int?,
    output_tokens: int?,
    total_tokens: int?,
    cache_read_tokens: int?,
    cache_write_tokens: int?,
}

function parse_claude_session(stdout: string) -> ClaudeSession {
    // The summary is the LAST line that starts with { or [.
    let json_line = find_last_json_line(stdout);
    let summary = baml.json.from_string<ClaudeSummary>(json_line) catch (_) {
        _ => ClaudeSummary { num_turns: null, num_tool_calls: null, session_id: null, usage: null }
    };
    let usage = match (summary.usage) {
        let u: ClaudeUsage => u,
        null => {
            ClaudeUsage {
                input_tokens: null,
                output_tokens: null,
                total_tokens: null,
                cache_read_input_tokens: null,
                cache_creation_input_tokens: null,
            }
        },
    };
    // Claude's summary never emits a pre-summed total — synthesize it from the
    // components so downstream cost/stats aren't stranded on a null.
    let total = match (usage.total_tokens) {
        let t: int => t,
        null => {
            let sum = t_or_zero(usage.input_tokens)
                + t_or_zero(usage.output_tokens)
                + t_or_zero(usage.cache_read_input_tokens)
                + t_or_zero(usage.cache_creation_input_tokens);
            if (sum == 0) {
                null
            } else {
                sum
            }
        },
    };
    return ClaudeSession {
        turns: summary.num_turns,
        tool_calls: summary.num_tool_calls,
        session_id: summary.session_id,
        input_tokens: usage.input_tokens,
        output_tokens: usage.output_tokens,
        total_tokens: total,
        cache_read_tokens: usage.cache_read_input_tokens,
        cache_write_tokens: usage.cache_creation_input_tokens,
    };
}

function t_or_zero(v: int?) -> int {
    return match (v) {
        let n: int => n,
        null => 0,
    };
}

function find_last_json_line(stdout: string) -> string {
    let lines = stdout.split("\n");
    let json_line = "";
    for (let line in lines) {
        let t = line.trim();
        if (t.starts_with("{") || t.starts_with("[")) {
            json_line = line;
        }
    }
    return json_line;
}

// ---- session log path ------------------------------------------------------
// claude writes the full session jsonl to $HOME/.claude/projects/<encoded>/<sid>.jsonl
// where <encoded> is the staging path with every "/" replaced by "-".

function session_log_path(staging: string, session_id: string) -> string {
    let encoded = staging.replace_all("/", "-");
    return claude_config_dir() + "/projects/" + encoded + "/" + session_id + ".jsonl";
}

// Read claude's session jsonl for a run and render it to a terminal transcript.
// Falls back to `fallback` (the raw stdout) when the session file is missing/empty
// (e.g. no session_id, or claude wrote nowhere we can read).
function render_session(staging: string, session_id: string?, fallback: string) -> string {
    let sid = match (session_id) {
        let s: string => s,
        null => "",
    };
    if (sid == "") {
        return fallback;
    }
    let jsonl = baml.fs.read(session_log_path(staging, sid)) catch (_) {
        _ => ""
    };
    if (jsonl == "") {
        return fallback;
    }
    return render_terminal_transcript(jsonl);
}

// The structured per-turn log for a run's session jsonl, as raw json rows (the
// wire/type shape the dashboard's "Turns" tab reads; typed TurnRow[]? fields
// serialize elements to null, so the round-trip through json is deliberate).
// Null when the session file is missing or yields no turns.
function session_turn_log(staging: string, session_id: string?) -> json[]? {
    let sid = match (session_id) {
        let s: string => s,
        null => "",
    };
    if (sid == "") {
        return null;
    }
    let jsonl = baml.fs.read(session_log_path(staging, sid)) catch (_) {
        _ => ""
    };
    if (jsonl == "") {
        return null;
    }
    let rows = parse_turn_log(jsonl);
    if (rows.length() == 0) {
        return null;
    }
    let raw = baml.json.to_string(rows) catch (_) {
        _ => "[]"
    };
    let out: json[] = baml.json.from_string<json[]>(raw) catch (_) {
        _ => []
    };
    if (out.length() == 0) {
        return null;
    }
    return out;
}

// ---- terminal transcript renderer ------------------------------------------
// Ports render_terminal_transcript: render the session jsonl into a Claude-Code
// terminal-style transcript — user prompts `> `, assistant text/tools `⏺ `, tool
// results `⎿`, thinking under `✻ Thinking…`. Untruncated so it stays searchable.

class TToolInput {
    command: string?,
    file_path: string?,
    notebook_path: string?,
    pattern: string?,
}

class TResultPart {
    text: string?,
}

class TBlock {
    type: string?,
    text: string?,
    thinking: string?,
    name: string?,
    input: TToolInput?, // tool_use args (typed view; `json?` would poison LLM $parse)
    content: string?, // tool_result content (the common string form; list form degrades)
    is_error: bool?,
    id: string?, // tool_use id (assistant tool_use block)
    tool_use_id: string?, // ref back to the tool_use (user tool_result block)
}

class TMsg {
    content: TBlock[]?,
}

class TMsgStr {
    content: string?,
}

class TRow {
    type: string?,
    timestamp: string?,
    message: TMsg?,
}

// Minimal row shape that ALWAYS parses (no message) — used to read type/timestamp
// even on rows whose message.content is a bare string (which fails the TMsg union).
class TRowMeta {
    type: string?,
    timestamp: string?,
}

class TRowStr {
    type: string?,
    timestamp: string?,
    message: TMsgStr?,
}

// HH:MM:SS (UTC) from an ISO-8601 timestamp, or "".
function hhmmss(raw: string) -> string {
    if (raw.length() < 19) {
        return "";
    }
    return raw.substring(11, 19) catch (_) {
        _ => ""
    };
}

// Append a marked first line + indented continuation lines to `out`.
function emit_block(out: string[], text: string, marker: string, cont: string, ts: string) -> int {
    let parts = text.trim_end().split("\n");
    let stamp = if (ts == "") {
        ""
    } else {
        "[" + ts + "] "
    };
    let head = match (parts.at(0)) {
        let h: string => h,
        null => "",
    };
    out.push(stamp + marker + head);
    let i = 0;
    for (let p in parts) {
        if (i > 0) {
            out.push(cont + p);
        }
        i = i + 1;
    }
    out.push("");
    return 0;
}

// One-line argument for a tool-use header (Bash->command, Read/Write->path, …).
function tool_call_summary(name: string, input: TToolInput?) -> string {
    let ti = match (input) {
        let v: TToolInput => v,
        null => TToolInput { command: null, file_path: null, notebook_path: null, pattern: null },
    };
    if (name == "Bash") {
        return match (ti.command) {
            let c: string => c,
            null => "",
        };
    }
    if (
        name == "Read"
            || name == "Write"
            || name == "Edit"
            || name == "MultiEdit"
            || name == "NotebookEdit"
    ) {
        return match (ti.file_path) {
            let f: string => f,
            null => match (ti.notebook_path) {
                let n: string => n,
                null => "",
            },
        };
    }
    if (name == "Glob" || name == "Grep") {
        return match (ti.pattern) {
            let p: string => p,
            null => "",
        };
    }
    // Other tools: fall back to the first non-null typed field, else "".
    return match (ti.command) {
        let c: string => c,
        null => match (ti.file_path) {
            let f: string => f,
            null => "",
        },
    };
}

// A tool_result's text (the common string form; list-form content degrades to "").
function result_text(content: string?) -> string {
    return match (content) {
        let c: string => c,
        null => "",
    };
}

function render_terminal_transcript(jsonl: string) -> string {
    let out: string[] = [];
    for (let line in jsonl.split("\n")) {
        // type/timestamp from a minimal class (robust even when content is a string,
        // which makes the full TRow union fail).
        let meta = baml.json.from_string<TRowMeta>(line) catch (_) {
            _ => TRowMeta { type: null, timestamp: null }
        };
        let typ = match (meta.type) {
            let t: string => t,
            null => "",
        };
        let ts = match (meta.timestamp) {
            let r: string => hhmmss(r),
            null => "",
        };
        // blocks (best-effort): empty when content is a bare string.
        let blocks: TBlock[] = match (
            baml.json.from_string<TRow>(line) catch (_) {
                _ => null
            }
        ) {
            let r: TRow => match (r.message) {
                let m: TMsg => match (m.content) {
                    let b: TBlock[] => b,
                    null => [],
                },
                null => [],
            },
            null => [],
        };
        if (typ == "user") {
            // Decide by the actual blocks: a tool_result list renders results; anything
            // else is treated as a bare-string prompt (the SAP parser can coerce a string
            // into bogus blocks, so we key off "is there a real tool_result", not length).
            let has_tr = false;
            for (let c in blocks) {
                match (c.type) {
                    let v: string => {
                        if (v == "tool_result") {
                            has_tr = true;
                        }
                    },
                    null => 0,
                };
            }
            if (has_tr == false) {
                // content was a bare string (the initial prompt) -> render as `> `.
                let sr = baml.json.from_string<TRowStr>(line) catch (_) {
                    _ => TRowStr { type: null, timestamp: null, message: null }
                };
                let s = match (sr.message) {
                    let m: TMsgStr => match (m.content) {
                        let c: string => c,
                        null => "",
                    },
                    null => "",
                };
                if (s != "") {
                    let _ = emit_block(out, s, "> ", "  ", ts);
                }
            } else {
                for (let c in blocks) {
                    let ct = match (c.type) {
                        let v: string => v,
                        null => "",
                    };
                    if (ct == "tool_result") {
                        let body0 = result_text(c.content).trim();
                        let is_err = match (c.is_error) {
                            let e: bool => e,
                            null => false,
                        };
                        let body = if (is_err) {
                            "[error] " + body0
                        } else {
                            body0
                        };
                        let shown = if (body == "") {
                            "(no output)"
                        } else {
                            body
                        };
                        let _ = emit_block(out, shown, "  ⎿  ", "     ", "");
                    }
                }
            }
        } else if (typ == "assistant") {
            for (let c in blocks) {
                let ct = match (c.type) {
                    let v: string => v,
                    null => "",
                };
                if (ct == "thinking") {
                    let th = match (c.thinking) {
                        let t: string => t.trim(),
                        null => "",
                    };
                    if (th != "") {
                        let _ = emit_block(out, th, "✻ Thinking…\n", "  ", ts);
                    }
                } else if (ct == "text") {
                    let tx = match (c.text) {
                        let t: string => t.trim(),
                        null => "",
                    };
                    if (tx != "") {
                        let _ = emit_block(out, tx, "⏺ ", "  ", ts);
                    }
                } else if (ct == "tool_use") {
                    let name = match (c.name) {
                        let n: string => n,
                        null => "tool",
                    };
                    let stamp = if (ts == "") {
                        ""
                    } else {
                        "[" + ts + "] "
                    };
                    out.push(stamp + "⏺ " + name + "(" + tool_call_summary(name, c.input) + ")");
                }
            }
        }
    }
    return out.join("\n").trim_end() + "\n";
}

// ---- structured per-turn log (parse_turn_log) ------------------------------
// The trophy's `turnLog` powering the dashboard's "Turns" tab. Ports
// parse_turn_log: one row per assistant message with thinking/text previews and a
// tools list whose result previews are matched back from later tool_result rows.

class TurnTool {
    name: string?,
    input: TToolInput?,
    result_preview: string?,
    result_chars: int?,
    is_error: bool?,
}

class TurnRow {
    i: int,
    ts: string?,
    thinking_chars: int?,
    thinking_preview: string?,
    text_chars: int?,
    text_preview: string?,
    tools: TurnTool[],
}

class ToolResultInfo {
    text: string,
    is_error: bool,
}

// Truncate to a preview: non-error clips to 400 chars; an error keeps head 400 +
// tail 1600 with an elision marker (ports _preview).
function preview(s: string, is_error: bool) -> string {
    if (is_error == false) {
        if (s.length() <= 400) {
            return s;
        }
        return s.substring(0, 400) catch (_) {
            _ => s
        };
    }
    if (s.length() <= 2000) {
        return s;
    }
    let head = s.substring(0, 400) catch (_) {
        _ => s
    };
    let tail = s.substring(s.length() - 1600, s.length()) catch (_) {
        _ => ""
    };
    return head + "\n…[truncated]…\n" + tail;
}

// The content blocks of one session jsonl line ([] when content is a bare string).
function session_blocks(line: string) -> TBlock[] {
    let blocks: TBlock[] = match (
        baml.json.from_string<TRow>(line) catch (_) {
            _ => null
        }
    ) {
        let r: TRow => match (r.message) {
            let m: TMsg => match (m.content) {
                let b: TBlock[] => b,
                null => [],
            },
            null => [],
        },
        null => [],
    };
    return blocks;
}

function parse_turn_log(jsonl: string) -> TurnRow[] {
    let lines = jsonl.split("\n");

    // Pass 1: collect tool_results keyed by tool_use_id (so pass 2 needs no
    // in-place back-fill, which BAML's value semantics make awkward).
    let results: map<string, ToolResultInfo> = {};
    for (let line in lines) {
        let meta = baml.json.from_string<TRowMeta>(line) catch (_) {
            _ => TRowMeta { type: null, timestamp: null }
        };
        let typ = match (meta.type) {
            let t: string => t,
            null => "",
        };
        if (typ == "user") {
            for (let b in session_blocks(line)) {
                let bt = match (b.type) {
                    let v: string => v,
                    null => "",
                };
                let tid = match (b.tool_use_id) {
                    let v: string => v,
                    null => "",
                };
                if (bt == "tool_result" && tid != "") {
                    results
                        .set(
                            tid,
                            ToolResultInfo {
                                text: result_text(b.content),
                                is_error: match (b.is_error) {
                                    let e: bool => e,
                                    null => false,
                                },
                            },
                        );
                }
            }
        }
    }

    // Pass 2: one turn per assistant message.
    let turns: TurnRow[] = [];
    for (let line in lines) {
        let meta = baml.json.from_string<TRowMeta>(line) catch (_) {
            _ => TRowMeta { type: null, timestamp: null }
        };
        let typ = match (meta.type) {
            let t: string => t,
            null => "",
        };
        if (typ == "assistant") {
            let thinking = "";
            let text = "";
            let tools: TurnTool[] = [];
            for (let c in session_blocks(line)) {
                let ct = match (c.type) {
                    let v: string => v,
                    null => "",
                };
                if (ct == "thinking") {
                    thinking
                        = thinking
                            + match (c.thinking) {
                                let t: string => t,
                                null => "",
                            };
                } else if (ct == "text") {
                    text
                        = text
                            + match (c.text) {
                                let t: string => t,
                                null => "",
                            };
                } else if (ct == "tool_use") {
                    let res = match (
                        results
                            .get(
                                match (c.id) {
                                    let v: string => v,
                                    null => "",
                                },
                            )
                    ) {
                        let r: ToolResultInfo => r,
                        null => ToolResultInfo { text: "", is_error: false },
                    };
                    let has_res = res.text != "";
                    tools
                        .push(
                            TurnTool {
                                name: c.name,
                                input: c.input,
                                result_preview: if (has_res) {
                                    preview(res.text, res.is_error)
                                } else {
                                    null
                                },
                                result_chars: res.text.length(),
                                is_error: res.is_error,
                            },
                        );
                }
            }
            turns
                .push(
                    TurnRow {
                        i: turns.length() + 1,
                        ts: meta.timestamp,
                        thinking_chars: if (thinking == "") {
                            null
                        } else {
                            thinking.length()
                        },
                        thinking_preview: if (thinking == "") {
                            null
                        } else {
                            preview(thinking, false)
                        },
                        text_chars: if (text == "") {
                            null
                        } else {
                            text.length()
                        },
                        text_preview: if (text == "") {
                            null
                        } else {
                            preview(text, false)
                        },
                        tools: tools,
                    },
                );
        }
    }
    return turns;
}

// Read claude's session jsonl for a run and parse it into the structured turn log
// ([] when the session file is missing).
function parse_session_turns(staging: string, session_id: string?) -> TurnRow[] {
    let sid = match (session_id) {
        let s: string => s,
        null => "",
    };
    let empty: TurnRow[] = [];
    if (sid == "") {
        return empty;
    }
    let jsonl = baml.fs.read(session_log_path(staging, sid)) catch (_) {
        _ => ""
    };
    if (jsonl == "") {
        return empty;
    }
    return parse_turn_log(jsonl);
}
