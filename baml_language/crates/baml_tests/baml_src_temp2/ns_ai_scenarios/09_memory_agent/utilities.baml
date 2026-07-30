// Internal implementation for the memory-agent scenario. Public, zero-argument
// examples live in example.baml; assertions live in tests/memory_agent.baml.

// ---------------------------------------------------------------------------
// Durable memory
// ---------------------------------------------------------------------------

class MemoryEntry {
    name: string,
    keywords: string[],
    body: string,

    function score(self, tokens: string[]) -> int {
        let keys = _memory_agent_tokenize(`${self.name} ${self.keywords.join(" ")}`);
        let body_tokens = _memory_agent_tokenize(self.body);
        let total = 0;
        for (let token in tokens) {
            if (
                keys.some((candidate: string) -> bool {
                    _memory_agent_token_matches(candidate, token)
                })
            ) {
                total = total + 3;
            };
            if (
                body_tokens.some((candidate: string) -> bool {
                    _memory_agent_token_matches(candidate, token)
                })
            ) {
                total = total + 1;
            };
        }
        total
    }

    function marker(self) -> string {
        `[memory:${self.name}]`
    }

    function render(self) -> string {
        let hints = if (self.keywords.length() > 0) {
            ` (${self.keywords.join(", ")})`
        } else {
            ""
        };
        `${self.marker()}${hints} ${self.body}`
    }

    function to_file(self) -> string {
        `---\nname: ${self.name}\nkeywords: ${self.keywords.join(", ")}\n---\n${self.body}` + "\n"
    }
}

class MemoryAgentRecall {
    block: string,
    names: string[],
}

class MemoryAgentStore {
    dir: string,

    function ensure(self) -> void {
        if (!baml.fs.exists(self.dir)) {
            baml.fs.mkdir(self.dir, baml.fs.MkdirOptions { recursive: true });
        };
    }

    function path_for(self, name: string) -> string {
        `${self.dir}/${_memory_agent_slugify(name)}.md`
    }

    function remember(self, name: string, keywords: string[], body: string) -> MemoryEntry {
        //# Persist one durable fact as readable Markdown
        self.ensure();
        let memory = MemoryEntry {
            name: _memory_agent_slugify(name),
            keywords: _memory_agent_normalize_keywords(keywords),
            body: body.trim(),
        };
        baml.fs.write(self.path_for(memory.name), memory.to_file());
        memory
    }

    function get(self, name: string) -> MemoryEntry? {
        let path = self.path_for(name);
        if (!baml.fs.exists(path)) {
            return null;
        };
        _memory_agent_parse_memory(_memory_agent_slugify(name), baml.fs.read(path))
    }

    function forget(self, name: string) -> bool {
        let path = self.path_for(name);
        if (!baml.fs.exists(path)) {
            return false;
        };
        baml.fs.remove(path);
        true
    }

    function all(self) -> MemoryEntry[] {
        if (!baml.fs.exists(self.dir)) {
            return [];
        };
        let memories: MemoryEntry[] = [];
        for (let entry in baml.fs.read_dir(self.dir)) {
            if (entry.is_file && entry.name.ends_with(".md")) {
                let slug = entry.name.substring(0, entry.name.length() - 3);
                memories
                    .push(
                        _memory_agent_parse_memory(slug, baml.fs.read(`${self.dir}/${entry.name}`)),
                    );
            };
        }
        memories
    }

    function search(self, query: string, limit: int = 3) -> MemoryEntry[] {
        //# Score all memories against the current user message
        let tokens = _memory_agent_tokenize(query);
        if (tokens.length() == 0) {
            return [];
        };
        let hits = self.all().filter((memory: MemoryEntry) -> bool {
            memory.score(tokens) > 0
        });
        hits.sort_by_key((memory: MemoryEntry) -> int {
            0 - memory.score(tokens)
        });
        hits.slice(0, _memory_agent_min_int(limit, hits.length()))
    }

    function inject(self, query: string, history: string, limit: int = 3) -> MemoryAgentRecall {
        //# Inject relevant facts only once per application transcript
        let fresh = self.search(query, limit = limit).filter((memory: MemoryEntry) -> bool {
            !history.includes(memory.marker())
        });
        if (fresh.length() == 0) {
            return MemoryAgentRecall { block: "", names: [] };
        };
        let lines = fresh.map((memory: MemoryEntry) -> string {
            memory.render()
        });
        MemoryAgentRecall {
            block: `\n[RECALLED MEMORY — durable facts from earlier sessions; trust them unless the user contradicts them]\n${lines.join("\n")}`
                + "\n",
            names: fresh.map((memory: MemoryEntry) -> string {
                memory.name
            }),
        }
    }

    function index(self) -> string {
        let memories = self.all();
        if (memories.length() == 0) {
            return "(memory is empty)";
        };
        memories
            .map((memory: MemoryEntry) -> string {
                `- ${memory.name} (${memory.keywords.join(", ")}): ${memory.body}`
            })
            .join("\n")
    }

    function listing(self) -> string {
        let memories = self.all();
        if (memories.length() == 0) {
            return `  (no memories yet — stored under ${self.dir}/)` + "\n";
        };
        let lines = memories.map((memory: MemoryEntry) -> string {
            `  • ${memory.name} (${memory.keywords.join(", ")})\n    ${memory.body}`
        });
        `  ${memories.length()} memor${_memory_agent_plural(memories.length())} in ${self.dir}/\n${lines.join("\n")}`
            + "\n"
    }
}

function _memory_agent_store() -> MemoryAgentStore {
    let configured = baml.env.get("BAML_MEMORY_AGENT_DIR") ?? baml.env.get("BAMLCODE_MEMORY_DIR")
        ?? ".bamlcode/memory";
    MemoryAgentStore { dir: configured }
}

function _memory_agent_memory_enabled() -> bool {
    let configured = baml.env.get("BAML_MEMORY_AGENT") ?? baml.env.get("BAMLCODE_MEMORY") ?? "on";
    configured.trim().to_lower_case() != "off"
}

function _memory_agent_fresh_store(case_name: string) -> MemoryAgentStore {
    let store = MemoryAgentStore { dir: `/tmp/baml_memory_agent_test/${case_name}` };
    if (baml.fs.exists(store.dir)) {
        baml.fs.remove_dir_all(store.dir);
    };
    store.ensure();
    store
}

function _memory_agent_parse_memory(slug: string, raw: string) -> MemoryEntry {
    let lines = raw.lines();
    if ((lines.at(0) ?? "").trim() != "---") {
        return MemoryEntry { name: slug, keywords: [], body: raw.trim() };
    };
    let name = slug;
    let keywords: string[] = [];
    let body: string[] = [];
    let in_header = true;
    let index = 1;
    while (index < lines.length()) {
        let line = lines[index];
        index = index + 1;
        if (in_header) {
            if (line.trim() == "---") {
                in_header = false;
            } else if (line.starts_with("name:")) {
                let value = _memory_agent_header_value(line, "name:");
                if (value.length() > 0) {
                    name = value;
                };
            } else if (line.starts_with("keywords:")) {
                keywords
                    = _memory_agent_split_keywords(_memory_agent_header_value(line, "keywords:"));
            };
        } else {
            body.push(line);
        };
    }
    MemoryEntry { name: name, keywords: keywords, body: body.join("\n").trim() }
}

function _memory_agent_header_value(line: string, key: string) -> string {
    line.substring(key.length(), line.length()).trim()
}

function _memory_agent_split_keywords(raw: string) -> string[] {
    _memory_agent_normalize_keywords(raw.split(","))
}

function _memory_agent_normalize_keywords(raw: string[]) -> string[] {
    raw.map((keyword: string) -> string {
            keyword.trim().to_lower_case()
        })
        .filter((keyword: string) -> bool {
            keyword.length() > 0
        })
}

function _memory_agent_slugify(name: string) -> string {
    let output = "";
    for (let character in name.to_lower_case().chars()) {
        if (character.is_ascii_alphanumeric()) {
            output = output + character;
        } else if (output.length() > 0 && !output.ends_with("-")) {
            output = output + "-";
        };
    }
    let trimmed = if (output.ends_with("-")) {
        output.substring(0, output.length() - 1)
    } else {
        output
    };
    if (trimmed.length() == 0) {
        "memory"
    } else {
        trimmed
    }
}

function _memory_agent_tokenize(text: string) -> string[] {
    let tokens: string[] = [];
    let current = "";
    for (let character in text.to_lower_case().chars()) {
        if (character.is_ascii_alphanumeric()) {
            current = current + character;
        } else {
            if (_memory_agent_keep_token(current)) {
                tokens.push(current);
            };
            current = "";
        };
    }
    if (_memory_agent_keep_token(current)) {
        tokens.push(current);
    };
    tokens
}

function _memory_agent_keep_token(token: string) -> bool {
    token.length() >= 2 && !_memory_agent_stopwords().includes(token)
}

function _memory_agent_stopwords() -> string[] {
    [
        "am",
        "an",
        "as",
        "at",
        "be",
        "by",
        "do",
        "if",
        "in",
        "is",
        "it",
        "me",
        "my",
        "no",
        "of",
        "on",
        "or",
        "so",
        "to",
        "up",
        "us",
        "we",
        "the",
        "and",
        "for",
        "you",
        "your",
        "our",
        "with",
        "that",
        "this",
        "from",
        "have",
        "has",
        "are",
        "was",
        "were",
        "can",
        "will",
        "how",
        "what",
        "when",
        "why",
        "who",
        "all",
        "any",
        "but",
        "not",
        "get",
        "got",
        "let",
        "use",
        "into",
        "onto",
        "its",
        "his",
        "her",
        "them",
        "they",
        "than",
        "then",
        "there",
        "here",
        "some",
        "just",
        "like",
        "make",
        "made",
        "does",
        "did",
    ]
}

function _memory_agent_token_matches(left: string, right: string) -> bool {
    if (left == right) {
        return true;
    };
    let shorter = if (left.length() < right.length()) {
        left
    } else {
        right
    };
    let longer = if (left.length() < right.length()) {
        right
    } else {
        left
    };
    shorter.length() >= 4 && longer.starts_with(shorter)
}

function _memory_agent_min_int(left: int, right: int) -> int {
    if (left < right) {
        left
    } else {
        right
    }
}

function _memory_agent_plural(count: int) -> string {
    if (count == 1) {
        "y"
    } else {
        "ies"
    }
}

// ---------------------------------------------------------------------------
// Curator write side
// ---------------------------------------------------------------------------

class MemoryAgentEdit {
    op: "save" | "delete",
    name: string,
    keywords: string[],
    body: string,
}

function _memory_agent_save_edit(
    name: string,
    keywords: string[],
    body: string,
) -> MemoryAgentEdit {
    MemoryAgentEdit { op: "save", name: name, keywords: keywords, body: body }
}

function _memory_agent_delete_edit(name: string) -> MemoryAgentEdit {
    MemoryAgentEdit { op: "delete", name: name, keywords: [], body: "" }
}

function _memory_agent_max_edits_per_turn() -> int {
    5
}

function _memory_agent_apply_edits(edits: MemoryAgentEdit[], store: MemoryAgentStore) -> string[] {
    //# Apply only validated, bounded curator decisions
    let applied: string[] = [];
    let capped = edits.slice(0, _memory_agent_min_int(_memory_agent_max_edits_per_turn(), edits.length()));
    for (let edit in capped) {
        match (edit.op) {
            "save" => {
                if (edit.name.trim().length() > 0 && edit.body.trim().length() > 0) {
                    applied.push(`+${store.remember(edit.name, edit.keywords, edit.body).name}`);
                };
            },
            "delete" => {
                if (store.forget(edit.name)) {
                    applied.push(`-${_memory_agent_slugify(edit.name)}`);
                };
            },
        };
    }
    applied
}

// ---------------------------------------------------------------------------
// Coding tools
// ---------------------------------------------------------------------------

/// Read a UTF-8 text file and return it with one-based line numbers.
function _memory_agent_read_file(path: string, summary: string = "") -> string throws never {
    if (path.trim().length() == 0) {
        return "ERROR: read_file needs a `path`.";
    };
    let exists = baml.fs.exists(path) catch_all (error) {
        _ => {
            return `ERROR: could not inspect ${path}: ${error}`;
        },
    };
    if (!exists) {
        return "ERROR: no such file: " + path;
    };
    let body = baml.fs.read(path) catch_all (error) {
        _ => {
            return `ERROR: could not read ${path}: ${error}`;
        },
    };
    "Contents of " + path + ":\n" + _memory_agent_with_line_numbers(body)
}

/// Create or overwrite a UTF-8 text file.
function _memory_agent_write_file(
    path: string,
    content: string,
    summary: string = "",
) -> string throws never {
    if (path.trim().length() == 0) {
        return "ERROR: write_file needs a `path`.";
    };
    let bytes = baml.fs.write(path, content) catch_all (error) {
        _ => {
            return `ERROR: could not write ${path}: ${error}`;
        },
    };
    `Wrote ${bytes} bytes to ${path}.`
}

/// Replace the first exact occurrence of old_str in a UTF-8 text file.
function _memory_agent_edit_file(
    path: string,
    old_str: string,
    new_str: string,
    summary: string = "",
) -> string throws never {
    let exists = baml.fs.exists(path) catch_all (error) {
        _ => {
            return `ERROR: could not inspect ${path}: ${error}`;
        },
    };
    if (!exists) {
        return "ERROR: no such file: " + path;
    };
    let original = baml.fs.read(path) catch_all (error) {
        _ => {
            return `ERROR: could not read ${path}: ${error}`;
        },
    };
    if (!original.includes(old_str)) {
        return "ERROR: old_str was not found in " + path + ". Read the file again and copy the exact text.";
    };
    let updated = original.replace(old_str, new_str);
    let _ = baml.fs.write(path, updated) catch_all (error) {
        _ => {
            return `ERROR: could not write ${path}: ${error}`;
        },
    };
    "Edited " + path + " (replaced 1 occurrence)."
}

/// List one directory. Directory names end in a slash.
function _memory_agent_list_dir(path: string, summary: string = "") -> string throws never {
    let directory = if (path.trim().length() == 0) {
        "."
    } else {
        path
    };
    let exists = baml.fs.exists(directory) catch_all (error) {
        _ => {
            return `ERROR: could not inspect ${directory}: ${error}`;
        },
    };
    if (!exists) {
        return "ERROR: no such directory: " + directory;
    };
    let entries = baml.fs.read_dir(directory) catch_all (error) {
        _ => {
            return `ERROR: could not list ${directory}: ${error}`;
        },
    };
    let lines: string[] = [];
    for (let entry in entries) {
        let marker = if (entry.is_dir) {
            "/"
        } else {
            ""
        };
        lines.push("  " + entry.name + marker);
    }
    if (lines.length() == 0) {
        return directory + " is empty.";
    };
    "Contents of " + directory + ":\n" + lines.join("\n")
}

/// Run a command with /bin/sh and return its exit code, stdout, and stderr.
function _memory_agent_run_bash(command: string, summary: string = "") -> string throws never {
    if (command.trim().length() == 0) {
        return "ERROR: run_bash needs a `command`.";
    };
    let output = baml.sys.shell(command, null) catch_all (error) {
        _ => {
            return `ERROR: could not start shell command: ${error}`;
        },
    };
    let stdout = output.stdout.to_string();
    let stderr = output.stderr.to_string();
    let rendered = `exit code: ${output.exit_code}`;
    if (stdout.trim().length() > 0) {
        rendered = rendered + "\n--- stdout ---\n" + stdout;
    };
    if (stderr.trim().length() > 0) {
        rendered = rendered + "\n--- stderr ---\n" + stderr;
    };
    rendered
}

function _memory_agent_main_tools() -> ai.tools.ToolInput[] throws never {
    [
        ai
            .tools
            .tool(
                _memory_agent_read_file,
                name = "read_file",
                description = "Read a text file with line numbers. Set summary to one short sentence explaining why.",
            ),
        ai
            .tools
            .tool(
                _memory_agent_write_file,
                name = "write_file",
                description = "Create or overwrite a text file. Set summary to one short sentence explaining why.",
            ),
        ai
            .tools
            .tool(
                _memory_agent_edit_file,
                name = "edit_file",
                description = "Replace the first exact old_str occurrence. Set summary to one short sentence explaining why.",
            ),
        ai
            .tools
            .tool(
                _memory_agent_list_dir,
                name = "list_dir",
                description = "List a directory, using an empty path for the current directory. Set summary to one short sentence explaining why.",
            ),
        ai
            .tools
            .tool(
                _memory_agent_run_bash,
                name = "run_bash",
                description = "Run a /bin/sh command. Set summary to one short sentence explaining why.",
            ),
    ]
}

function _memory_agent_with_line_numbers(body: string) -> string throws never {
    let output: string[] = [];
    let line_number = 0;
    for (let line in body.lines()) {
        line_number = line_number + 1;
        output.push(`${line_number}\t${line}`);
    }
    output.join("\n")
}

function _memory_agent_clip(text: string, limit: int) -> string throws never {
    if (text.length() > limit) {
        text.substring(0, limit) + `\n… (${text.length() - limit} more chars truncated)`
    } else {
        text
    }
}

// ---------------------------------------------------------------------------
// Providers and Tasks
// ---------------------------------------------------------------------------

function _memory_agent_openai_provider() -> root.openai.OpenAIProvider throws never {
    root.openai
        .responses(
            model = "gpt-5.6-luna",
            parallel_tool_calls = false,
            extra_body = { "max_output_tokens": 4096 },
        )
}

function _memory_agent_google_provider() -> root.google.ai.Gemini throws never {
    root.google
        .ai
        .gemini(
            model = "gemini-2.5-flash",
            parallel_tool_calls = false,
            extra_body = { "generationConfig": { "maxOutputTokens": 4096 } },
        )
}

function _memory_agent_live_google_provider() -> root.google.ai.Gemini {
    root.google
        .ai
        .gemini(
            model = "gemini-2.5-flash",
            api_key = baml.env.get_or_panic("GOOGLE_API_KEY"),
            parallel_tool_calls = false,
            extra_body = { "generationConfig": { "maxOutputTokens": 4096 } },
        )
}

function _MemoryAgentTurn(transcript: string) -> string {
    provider: _memory_agent_openai_provider()
    tools: _memory_agent_main_tools()
    prompt: `
        You are bamlcode, a CLI coding agent operating inside the user's current working directory.

        Complete software tasks by taking one application tool action at a time.

        Available tools:
          - read_file: read a file with line numbers
          - write_file: create or overwrite a file
          - edit_file: replace the first exact text occurrence
          - list_dir: list a directory
          - run_bash: run a /bin/sh command

        Put a short action summary in each tool call's \`summary\` argument.

        Memory:
          - Durable facts may appear under [RECALLED MEMORY].
          - Trust recalled facts unless the user contradicts them.
          - You do not manage memory. A separate curator runs after the turn.

        Operating rules:
          - Inspect before changing files.
          - Make old_str unique and copy it byte-for-byte before edit_file.
          - Use run_bash for search, tests, git, mkdir, and unsupported operations.
          - Do not repeat an action that already succeeded.
          - When complete, or when user input is required, return the final answer.
          - Never call more than one tool in a model step.

        Application transcript:
        ----------------------------------------------------------------------
        ${transcript}
        ----------------------------------------------------------------------

        ${ctx.output_format}
    `
}

function _memory_agent_curator_provider() -> root.openai.OpenAIProvider throws never {
    root.openai
        .responses(
            model = "gpt-5.6-luna",
            parallel_tool_calls = false,
            extra_body = { "max_output_tokens": 1024 },
        )
}

function _MemoryAgentCurator(
    turn: string,
    index: string,
) -> MemoryAgentEdit[] {
    provider: _memory_agent_curator_provider()
    prompt: `
        You maintain the long-term memory of a coding agent. You are not the coding agent.
        Read only the completed turn below and decide what, if anything, is durable enough
        to affect future sessions.

        Save only confirmed preferences, project conventions and invariants, commands known
        to work, important locations, and decisions with their reasons.

        Never save transient task state, progress, code that can be reread, secrets, personal
        data, or guesses. Use delete when an existing memory is obsolete. Correct a memory by
        saving under the same name. Empty output is normally correct. Return at most five edits.

        Existing memory index:
        ----------------------------------------------------------------------
        ${index}
        ----------------------------------------------------------------------

        Just-completed turn:
        ----------------------------------------------------------------------
        ${turn}
        ----------------------------------------------------------------------

        ${ctx.output_format}
    `
}

function _memory_agent_google_curator_provider() -> root.google.ai.Gemini throws never {
    root.google
        .ai
        .gemini(
            model = "gemini-2.5-flash",
            parallel_tool_calls = false,
            extra_body = { "generationConfig": { "maxOutputTokens": 1024 } },
        )
}

function _memory_agent_live_google_curator_provider() -> root.google.ai.Gemini {
    root.google
        .ai
        .gemini(
            model = "gemini-2.5-flash",
            api_key = baml.env.get_or_panic("GOOGLE_API_KEY"),
            parallel_tool_calls = false,
            extra_body = { "generationConfig": { "maxOutputTokens": 1024 } },
        )
}

function _memory_agent_main_budget() -> ai.Budget throws never {
    ai.Budget { max_steps: 30, max_cost_usd: null }
}

function _memory_agent_curator_budget() -> ai.Budget throws never {
    ai.Budget { max_steps: 1, max_cost_usd: null }
}

function _memory_agent_runner(
    conversation: ai.Conversation? = null,
    cancel: baml.spawn.CancelToken? = null,
    observers: ai.observe.AgentObserver[] = [],
) -> ai.run.Agent<string> throws never {
    ai
        .run
        .Agent<string>
        .new(
            budget = _memory_agent_main_budget(),
            max_tool_calls_per_step = 1,
            observers = observers,
            conversation = conversation,
            cancel = cancel,
        )
}

// ---------------------------------------------------------------------------
// One Agent run
// ---------------------------------------------------------------------------

enum MemoryAgentBackend {
    OpenAI,
    Google,
}

class MemoryAgentTurn {
    answer: string,
    transcript: string,
    conversation: ai.Conversation?,
    steps: int,
    interrupted: bool,
    queued: string[],
}

class MemoryAgentSession {
    transcript: string,
    conversation: ai.Conversation?,
}

class MemoryAgentInterrupt {
    cancel: bool,
    queued: string,
}

class MemoryAgentLine {
    text: string,
    eof: bool,
}

class MemoryAgentTrace {
    transcript: string,
    steps: int,
}

class MemoryAgentObserver {
    trace: MemoryAgentTrace,
    live: bool,

    implements ai.observe.AgentObserver {
        function on_event(self, event: ai.observe.AgentEvent) -> null throws never {
            match (event) {
                let started: ai.observe.ModelStartedEvent => {
                    self.trace.steps = self.trace.steps + 1;
                },
                let tool_event: ai.observe.ToolCallEvent => {
                    if (tool_event.phase == "proposed") {
                        let summary = _memory_agent_call_summary(tool_event.call);
                        let label = _memory_agent_describe_call(tool_event.call);
                        if (summary.length() > 0) {
                            self.trace.transcript
                                = self.trace.transcript + "[THOUGHT] " + summary + "\n";
                            if (self.live) {
                                _memory_agent_say("  " + _memory_agent_dim(summary) + "\n");
                            };
                        };
                        self.trace.transcript = self.trace.transcript + "[ACTION] " + label + "\n";
                        if (self.live) {
                            _memory_agent_say("  " + _memory_agent_arrow(label) + "\n");
                        };
                    } else if (tool_event.phase == "finished" || tool_event.phase == "blocked") {
                        match (tool_event.result) {
                            let result: ai.tools.ToolResult => {
                                let rendered = _memory_agent_render_tool_result(result);
                                self.trace.transcript
                                    = self.trace.transcript
                                        + "[RESULT]\n"
                                        + _memory_agent_clip(rendered, 6000)
                                        + "\n";
                                if (self.live) {
                                    _memory_agent_say(
                                        _memory_agent_style_result(_memory_agent_clip(rendered, 1200), result.is_error)
                                            + "\n",
                                    );
                                };
                            },
                            null => {},
                        }
                    };
                },
                _ => {},
            }
            null
        }
    }
}

function _memory_agent_call_summary(call: ai.tools.ToolCall) -> string throws never {
    baml.json.path<string>(call.args, ".summary") catch_all (error) {
        _ => ""
    }
}

function _memory_agent_describe_call(call: ai.tools.ToolCall) -> string throws never {
    match (call.name) {
        "read_file" => {
            "read "
                + (
                    baml.json.path<string>(call.args, ".path") catch_all (error) {
                        _ => "?"
                    }
                )
        },
        "write_file" => {
            let path = baml.json.path<string>(call.args, ".path") catch_all (error) {
                _ => "?"
            };
            let content = baml.json.path<string>(call.args, ".content") catch_all (error) {
                _ => ""
            };
            `write ${path} (${content.length()} chars)`
        },
        "edit_file" => {
            "edit "
                + (
                    baml.json.path<string>(call.args, ".path") catch_all (error) {
                        _ => "?"
                    }
                )
        },
        "list_dir" => {
            "ls "
                + (
                    baml.json.path<string>(call.args, ".path") catch_all (error) {
                        _ => "."
                    }
                )
        },
        "run_bash" => {
            "$ "
                + (
                    baml.json.path<string>(call.args, ".command") catch_all (error) {
                        _ => "?"
                    }
                )
        },
        let other => other,
    }
}

function _memory_agent_render_tool_result(result: ai.tools.ToolResult) -> string throws never {
    baml.json.from_json<string>(result.output) catch_all (decode_error) {
        _ => baml.json.stringify(result.output),
    }
}

function _memory_agent_opening_transcript(
    history: string,
    recall_block: string,
    user_msg: string,
) -> string throws never {
    history + recall_block + "\nUSER: " + user_msg + "\n"
}

function _memory_agent_turn_delta(history: string, transcript: string) -> string throws never {
    if (transcript.starts_with(history)) {
        transcript.substring(history.length(), transcript.length())
    } else {
        transcript
    }
}

function _memory_agent_continuation_text(
    recall_block: string,
    user_msg: string,
) -> string throws never {
    if (recall_block.trim().length() == 0) {
        user_msg
    } else {
        recall_block.trim() + "\n\n" + user_msg
    }
}

function _memory_agent_append_user_message(
    conversation: ai.Conversation,
    text: string,
) -> ai.Conversation throws ai.Failure | baml.errors.UnknownError | baml.errors.Unsupported {
    //# Preserve opaque provider state while adding the next application turn
    conversation.append_message(ai.ChatMessage.user(text))
}

function _memory_agent_run_turn(
    history: string,
    conversation: ai.Conversation?,
    user_msg: string,
    backend: MemoryAgentBackend,
    cancel: baml.spawn.CancelToken? = null,
) -> MemoryAgentTurn {
    //# Recall durable facts before creating the model Task
    let recall = if (_memory_agent_memory_enabled()) {
        _memory_agent_store().inject(user_msg, history)
    } else {
        MemoryAgentRecall { block: "", names: [] }
    };
    if (recall.names.length() > 0) {
        _memory_agent_say("  " + _memory_agent_gray(`⟳ recalled: ${recall.names.join(", ")}`) + "\n");
    };
    let opening = _memory_agent_opening_transcript(history, recall.block, user_msg);
    let trace = MemoryAgentTrace { transcript: opening, steps: 0 };
    let observer = MemoryAgentObserver { trace: trace, live: true };
    let continued = match (conversation) {
        let existing: ai.Conversation => {
            _memory_agent_append_user_message(
                existing,
                _memory_agent_continuation_text(recall.block, user_msg),
            )
        },
        null => null,
    };
    let base_task = _MemoryAgentTurn@task(opening);
    let task = match (backend) {
        MemoryAgentBackend.OpenAI => base_task,
        MemoryAgentBackend.Google => {
            //# Override the Task provider; Agent and tools stay unchanged
            base_task.with_provider(_memory_agent_live_google_provider())
        },
    };
    //# Run the standard Agent loop; no scenario-specific Runner is involved
    let outcome = task
        .run(
            runner = _memory_agent_runner(
                conversation = continued,
                cancel = cancel,
                observers = [observer],
            ),
        );
    let answer = "";
    let final_conversation: ai.Conversation? = null;
    let interrupted = false;
    match (outcome) {
        let done: ai.Done<string> => {
            answer = done.value;
            final_conversation = done.conversation;
        },
        let budget: ai.BudgetReached => {
            answer = `(stopped after ${budget.steps_taken} steps: ${budget.reason})`;
            final_conversation = budget.conversation;
        },
        let handoff: ai.Handoff => {
            answer = `(unexpected handoff to ${handoff.call.name})`;
            final_conversation = handoff.conversation;
        },
        let stopped: ai.Interrupted => {
            answer = "⨂ interrupted.";
            final_conversation = stopped.conversation;
            interrupted = true;
        },
    }
    if (interrupted) {
        _memory_agent_say("\n" + _memory_agent_yellow(answer) + "\n");
        trace.transcript = trace.transcript + "[INTERRUPTED by user]\n";
    } else {
        _memory_agent_say("\n" + _memory_agent_style_assistant(answer) + "\n");
        trace.transcript = trace.transcript + "ASSISTANT: " + answer + "\n";
    };
    MemoryAgentTurn {
        answer: answer,
        transcript: trace.transcript,
        conversation: final_conversation,
        steps: trace.steps,
        interrupted: interrupted,
        queued: [],
    }
}

function _memory_agent_curate_turn_unchecked(
    turn: string,
    store: MemoryAgentStore,
    backend: MemoryAgentBackend,
) -> string[] {
    if (turn.trim().length() == 0) {
        return [];
    };
    let task = match (backend) {
        MemoryAgentBackend.OpenAI => {
            _MemoryAgentCurator@task(turn, store.index())
        },
        MemoryAgentBackend.Google => {
            //# Apply the Google override to the independent curator Task too
            _MemoryAgentCurator@task(turn, store.index())
                .with_provider(_memory_agent_live_google_curator_provider())
        },
    };
    let outcome = task
        .run(
            runner = ai
                .run
                .Agent<MemoryAgentEdit[]>
                .new(
                    budget = _memory_agent_curator_budget(),
                    max_tool_calls_per_step = 0,
                ),
        );
    let edits: MemoryAgentEdit[] = match (outcome) {
        let done: ai.Done<MemoryAgentEdit[]> => done.value,
        let budget: ai.BudgetReached => {
            let empty: MemoryAgentEdit[] = [];
            empty
        },
        let handoff: ai.Handoff => {
            let empty: MemoryAgentEdit[] = [];
            empty
        },
        let stopped: ai.Interrupted => {
            let empty: MemoryAgentEdit[] = [];
            empty
        },
    };
    _memory_agent_apply_edits(edits, store)
}

function _memory_agent_curate_turn(
    turn: string,
    store: MemoryAgentStore,
    backend: MemoryAgentBackend,
) -> string[] throws never {
    _memory_agent_curate_turn_unchecked(turn, store, backend) catch_all (error) {
        _ => []
    }
}

// ---------------------------------------------------------------------------
// Interactive supervisor, queue, and cancellation
// ---------------------------------------------------------------------------

function _memory_agent_forget(
    name: string,
    store: MemoryAgentStore = _memory_agent_store(),
) -> string {
    if (name.trim().length() == 0) {
        return "ERROR: forget needs a `memory_name`.";
    };
    if (store.forget(name)) {
        `Forgot "${name}".`
    } else {
        `ERROR: no memory named "${name}". Use /memory to see what's stored.`
    }
}

function _memory_agent_poll_interrupt() -> MemoryAgentInterrupt throws never {
    let output = baml
        .sys
        .shell(
            "IFS= read -r -t 0.1 __memory_agent_line; rc=$?; printf '%s' \"$__memory_agent_line\"; exit $rc",
            null,
        ) catch_all (error) {
        _ => {
            return MemoryAgentInterrupt { cancel: false, queued: "" };
        },
    };
    if (output.exit_code != 0) {
        return MemoryAgentInterrupt { cancel: false, queued: "" };
    };
    _memory_agent_classify_interrupt(output.stdout.to_string())
}

function _memory_agent_classify_interrupt(raw: string) -> MemoryAgentInterrupt throws never {
    let line = raw.trim();
    if (line.includes(_memory_agent_escape())) {
        MemoryAgentInterrupt { cancel: true, queued: "" }
    } else {
        MemoryAgentInterrupt { cancel: false, queued: line }
    }
}

function _memory_agent_run_supervised(
    history: string,
    conversation: ai.Conversation?,
    message: string,
    backend: MemoryAgentBackend,
) -> MemoryAgentTurn {
    //# Request a resumable checkpoint while the application watches terminal input
    let cancel = baml.spawn.CancelToken.new();
    let future = spawn {
        _memory_agent_run_turn(history, conversation, message, backend, cancel = cancel)
    };
    let cancellation_requested = false;
    let queued: string[] = [];
    let watching = true;
    while (watching) {
        if (future.is_settled()) {
            watching = false;
        } else {
            let probe = _memory_agent_poll_interrupt();
            if (probe.cancel) {
                if (!cancellation_requested) {
                    let _ = cancel.cancel();
                    cancellation_requested = true;
                    _memory_agent_say(
                        "\n"
                            + _memory_agent_yellow(
                                "⨂ stopping after the current model/tool checkpoint…",
                            )
                            + "\n",
                    );
                };
            } else if (probe.queued.length() > 0) {
                queued.push(probe.queued);
                _memory_agent_say(
                    "\n  "
                        + _memory_agent_yellow(
                            `✚ queued for after this turn (${queued.length()} waiting): ${probe.queued}`,
                        )
                        + "\n",
                );
            };
        };
    }
    let turn = (await future) catch_all (error) {
        _ => {
            _memory_agent_say("\n" + _memory_agent_red(`turn failed: ${error}`) + "\n");
            MemoryAgentTurn {
                answer: "(the turn failed — see the error above)",
                transcript: history + "\n[turn errored]\n",
                conversation: conversation,
                steps: 0,
                interrupted: false,
                queued: [],
            }
        },
    };
    //# Curate only completed turns in an isolated Task
    if (!turn.interrupted && _memory_agent_memory_enabled()) {
        let stored = _memory_agent_curate_turn(_memory_agent_turn_delta(history, turn.transcript), _memory_agent_store(), backend);
        if (stored.length() > 0) {
            _memory_agent_say("  " + _memory_agent_gray(`⛁ memory ${stored.join(" ")}`) + "\n");
        };
    };
    MemoryAgentTurn {
        answer: turn.answer,
        transcript: turn.transcript,
        conversation: turn.conversation,
        steps: turn.steps,
        interrupted: turn.interrupted,
        queued: queued,
    }
}

function _memory_agent_run_queue(
    session: MemoryAgentSession,
    queue: string[],
    backend: MemoryAgentBackend,
) -> MemoryAgentSession {
    let transcript = session.transcript;
    let conversation = session.conversation;
    let index = 0;
    while (index < queue.length()) {
        let message = queue[index];
        index = index + 1;
        _memory_agent_say(_memory_agent_magenta(`\n◆ queued (${index}/${queue.length()}) › ${message}`) + "\n");
        let turn = _memory_agent_run_supervised(transcript, conversation, message, backend);
        transcript = turn.transcript;
        conversation = turn.conversation;
        for (let typed in turn.queued) {
            queue.push(typed);
        }
        if (turn.interrupted) {
            let remaining = queue.length() - index;
            _memory_agent_say(
                _memory_agent_yellow(`⨂ cancelled — skipped ${remaining} queued message(s).`)
                    + "\n",
            );
            return MemoryAgentSession { transcript: transcript, conversation: conversation };
        };
    }
    MemoryAgentSession { transcript: transcript, conversation: conversation }
}

function _memory_agent_ask(task: string, backend: MemoryAgentBackend) -> string {
    let turn = _memory_agent_run_turn("", null, task, backend);
    if (_memory_agent_memory_enabled()) {
        let _ = _memory_agent_curate_turn(turn.transcript, _memory_agent_store(), backend);
    };
    turn.answer
}

function _memory_agent_repl(backend: MemoryAgentBackend) -> string {
    _memory_agent_say(_memory_agent_banner(backend));
    let session = MemoryAgentSession { transcript: "", conversation: null };
    let queue: string[] = [];
    let running = true;
    while (running) {
        //# Read and classify the next application command
        _memory_agent_say(_memory_agent_prompt_label(queue));
        let line = _memory_agent_read_line();
        if (line.eof) {
            _memory_agent_say("\n");
            running = false;
        } else {
            let message = line.text.trim();
            match (message) {
                "exit" | "quit" | "/exit" => {
                    running = false;
                },
                "/queue" | "/q" => {
                    _memory_agent_say(_memory_agent_show_queue(queue));
                },
                "/memory" | "/mem" => {
                    let listing = _memory_agent_store().listing() catch_all (error) {
                        _ => `  ERROR: could not list memory: ${error}` + "\n",
                    };
                    _memory_agent_say(listing);
                },
                let command if command.starts_with("/forget ") => {
                    let result = _memory_agent_forget(command.substring(8, command.length()).trim()) catch_all (error) {
                        _ => `ERROR: could not forget memory: ${error}`,
                    };
                    _memory_agent_say("  " + result + "\n");
                },
                "/clear" => {
                    queue = [];
                    _memory_agent_say("  queue cleared.\n");
                },
                let command if command.starts_with("/queue ") || command.starts_with("/q ") => {
                    queue.push(_memory_agent_command_arg(command));
                    _memory_agent_say(
                        `  ✚ queued — ${queue.length()} waiting. Queue more, or press enter to run them.\n`,
                    );
                },
                "" => {
                    if (queue.length() == 0) {
                        _memory_agent_say("  (nothing queued)\n");
                    } else {
                        session = _memory_agent_run_queue(session, queue, backend);
                        queue = [];
                    };
                },
                let immediate => {
                    //# Run now, then drain type-ahead and staged work
                    let turn = _memory_agent_run_supervised(
                        session.transcript,
                        session.conversation,
                        immediate,
                        backend,
                    );
                    session = MemoryAgentSession {
                        transcript: turn.transcript,
                        conversation: turn.conversation,
                    };
                    for (let typed in turn.queued) {
                        queue.push(typed);
                    }
                    if (!turn.interrupted && queue.length() > 0) {
                        session = _memory_agent_run_queue(session, queue, backend);
                        queue = [];
                    };
                },
            };
        };
    }
    _memory_agent_say("\nbye 👋\n");
    "session ended"
}

function _memory_agent_command_arg(message: string) -> string throws never {
    if (message.starts_with("/queue ")) {
        message.substring(7, message.length()).trim()
    } else if (message.starts_with("/q ")) {
        message.substring(3, message.length()).trim()
    } else {
        message.trim()
    }
}

function _memory_agent_prompt_label(queue: string[]) -> string throws never {
    if (queue.length() > 0) {
        "\n"
            + _memory_agent_bold("you")
            + _memory_agent_gray(` (${queue.length()} queued)`)
            + _memory_agent_bold(" › ")
    } else {
        "\n" + _memory_agent_bold("you › ")
    }
}

function _memory_agent_show_queue(queue: string[]) -> string throws never {
    if (queue.length() == 0) {
        return "  (queue empty)\n";
    };
    let lines: string[] = [];
    let index = 0;
    for (let message in queue) {
        index = index + 1;
        lines.push("  " + _memory_agent_cyan(`${index}.`) + " " + message);
    }
    _memory_agent_gray("  queued messages:") + "\n" + lines.join("\n") + "\n"
}

// ---------------------------------------------------------------------------
// Terminal presentation
// ---------------------------------------------------------------------------

function _memory_agent_read_line() -> MemoryAgentLine throws never {
    let output = baml
        .sys
        .shell(
            "IFS= read -r __memory_agent_line; rc=$?; printf '%s' \"$__memory_agent_line\"; exit $rc",
            null,
        ) catch_all (error) {
        _ => {
            return MemoryAgentLine { text: "", eof: true };
        },
    };
    MemoryAgentLine { text: output.stdout.to_string(), eof: output.exit_code != 0 }
}

function _memory_agent_say(text: string) -> void throws never {
    let options = baml.sys.ProcessOptions { cwd: null, env: null, timeout_ms: null, stdin: text };
    let _ = baml.sys.shell("cat > /dev/tty 2>/dev/null", options) catch_all (error) {
        _ => null,
    };
}

function _memory_agent_color(code: string, text: string) -> string throws never {
    let no_color = baml.env.get("NO_COLOR") catch_all (error) {
        _ => null,
    };
    if (no_color != null) {
        return text;
    };
    let escape = _memory_agent_escape();
    `${escape}[${code}m${text}${escape}[0m`
}

function _memory_agent_escape() -> string throws never {
    string.from_code_points([27]) catch_all (error) {
        _ => ""
    }
}

function _memory_agent_bold(text: string) -> string throws never {
    _memory_agent_color("1", text)
}

function _memory_agent_gray(text: string) -> string throws never {
    _memory_agent_color("90", text)
}

function _memory_agent_cyan(text: string) -> string throws never {
    _memory_agent_color("36", text)
}

function _memory_agent_yellow(text: string) -> string throws never {
    _memory_agent_color("33", text)
}

function _memory_agent_magenta(text: string) -> string throws never {
    _memory_agent_color("35", text)
}

function _memory_agent_green(text: string) -> string throws never {
    _memory_agent_color("32", text)
}

function _memory_agent_red(text: string) -> string throws never {
    _memory_agent_color("31", text)
}

function _memory_agent_style_assistant(message: string) -> string throws never {
    _memory_agent_bold(_memory_agent_green("● memory agent"))
        + "\n"
        + _memory_agent_indent(message)
}

function _memory_agent_dim(summary: string) -> string throws never {
    _memory_agent_gray("· " + summary)
}

function _memory_agent_arrow(label: string) -> string throws never {
    _memory_agent_cyan("→ " + label)
}

function _memory_agent_style_result(result: string, is_error: bool) -> string throws never {
    let body = _memory_agent_indent(result);
    if (is_error || result.starts_with("ERROR")) {
        _memory_agent_red(body)
    } else {
        _memory_agent_gray(body)
    }
}

function _memory_agent_indent(text: string) -> string throws never {
    let output: string[] = [];
    for (let line in text.lines()) {
        output.push("    " + line);
    }
    output.join("\n")
}

function _memory_agent_banner(backend: MemoryAgentBackend) -> string throws never {
    let provider = match (backend) {
        MemoryAgentBackend.OpenAI => "OpenAI Responses",
        MemoryAgentBackend.Google => "Google AI Gemini",
    };
    _memory_agent_cyan(
        "\n  ┌────────────────────────────────────────────┐\n"
            + "  │   memory agent · coding agent in BAML       │\n"
            + "  └────────────────────────────────────────────┘\n",
    )
        + _memory_agent_gray(`  Provider: ${provider}. Working in this process's current directory.\n\n`)
        + "  "
        + _memory_agent_yellow("type a task")
        + _memory_agent_gray("       run it now\n")
        + "  "
        + _memory_agent_yellow("/queue <task>")
        + _memory_agent_gray("     stage a task (then enter to run)\n")
        + "  "
        + _memory_agent_yellow("/queue")
        + _memory_agent_gray("            show the queue   ")
        + _memory_agent_yellow("/clear")
        + _memory_agent_gray("  empty it\n")
        + "  "
        + _memory_agent_yellow("enter")
        + _memory_agent_gray("             run the queue now\n")
        + "  "
        + _memory_agent_yellow("/memory")
        + _memory_agent_gray("           list durable facts   ")
        + _memory_agent_yellow("/forget <name>")
        + _memory_agent_gray("  drop one\n")
        + "  "
        + _memory_agent_yellow("exit")
        + _memory_agent_gray("              quit\n\n")
        + _memory_agent_gray("  while working, press ")
        + _memory_agent_yellow("esc + enter")
        + _memory_agent_gray(" to stop, or type a task to queue it.\n")
        + _memory_agent_yellow("  This agent can run shell commands and edit files with your permissions.\n")
}
