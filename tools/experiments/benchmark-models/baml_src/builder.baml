// The describe-only builder: one LLM agent whose ONLY documentation channel is
// `baml describe`. It never sees a syntax guide, examples, compiler output, or
// test results — it explores the language reference and writes source blind.
//
// The worker runs in its own process (the packed binary re-execs itself) so
// the env-driven builder client resolves to exactly one model per process.

class Describe {
    query: string, // one symbol or module name, e.g. "String" or "baml.fs"
}

class WriteFile {
    path: string, // must be a .baml file beneath baml_src/
    content: string,
}

class Done {
    summary: string,
}

type BuilderAction = Describe | WriteFile | Done;

// Both clients read the same BENCH_* env vars; the worker process sets them
// for exactly one model before these are ever constructed.
retry_policy BuilderRetry {
    max_retries: 2,
    initial_delay_ms: 2000,
    multiplier: 2.0,
    max_delay_ms: 30000,
}

client<llm> BuilderOpenAI {
    provider: openai,
    retry_policy: BuilderRetry,
    options: {
        model: env.BENCH_MODEL,
        base_url: env.BENCH_BASE_URL,
        api_key: env.BENCH_API_KEY,
        max_completion_tokens: 4096,
        supports_streaming: false,
    },
}

client<llm> BuilderAnthropic {
    provider: anthropic,
    retry_policy: BuilderRetry,
    options: {
        model: env.BENCH_MODEL,
        base_url: env.BENCH_BASE_URL,
        api_key: env.BENCH_API_KEY,
        max_tokens: 4096,
        supports_streaming: false,
    },
}

function NextActionOpenAI(
    task: string,
    transcript: string,
    turn: int,
    turn_limit: int,
) -> BuilderAction {
    client: BuilderOpenAI
    prompt: #"
        You are building a small project in a programming language you have
        never seen before, called BAML.

        Your ONLY documentation is the Describe action, which runs
        `baml describe <symbol>` — a reference explorer. Describing a class
        (e.g. String, Array, Map) lists its methods; describing a module
        (e.g. baml.fs, baml.json) lists its functions; describing with an
        empty query lists your project's own symbols. Do not rely on any
        remembered BAML syntax — discover it.

        You have NO compiler, NO tests, and NO examples. You cannot check
        your work; write carefully based on what Describe shows you.

        WriteFile may only create .baml files beneath baml_src/. Writing an
        existing path replaces the whole file. Choose Done when the project
        is complete.

        This is turn {{ turn }} of {{ turn_limit }}. Take exactly one action.

        Task:
        {{ task }}

        Transcript so far:
        {{ transcript }}

        {{ ctx.output_format }}
    "#
}

function NextActionAnthropic(
    task: string,
    transcript: string,
    turn: int,
    turn_limit: int,
) -> BuilderAction {
    client: BuilderAnthropic
    prompt: #"
        You are building a small project in a programming language you have
        never seen before, called BAML.

        Your ONLY documentation is the Describe action, which runs
        `baml describe <symbol>` — a reference explorer. Describing a class
        (e.g. String, Array, Map) lists its methods; describing a module
        (e.g. baml.fs, baml.json) lists its functions; describing with an
        empty query lists your project's own symbols. Do not rely on any
        remembered BAML syntax — discover it.

        You have NO compiler, NO tests, and NO examples. You cannot check
        your work; write carefully based on what Describe shows you.

        WriteFile may only create .baml files beneath baml_src/. Writing an
        existing path replaces the whole file. Choose Done when the project
        is complete.

        This is turn {{ turn }} of {{ turn_limit }}. Take exactly one action.

        Task:
        {{ task }}

        Transcript so far:
        {{ transcript }}

        {{ ctx.output_format }}
    "#
}

function next_action(
    provider: string,
    task: string,
    transcript: string,
    turn: int,
) -> BuilderAction {
    if (provider == "anthropic") {
        NextActionAnthropic(task, transcript, turn, max_turns())
    } else {
        NextActionOpenAI(task, transcript, turn, max_turns())
    }
}

// The same small task for every model, unchanged from the earlier pilots so
// results stay comparable.
function task_prompt() -> string {
    #"
        Build a small, self-contained BAML project for extracting structured
        data from receipt text.

        The finished project must contain:
        - structured receipt and line-item types,
        - one LLM function that extracts a receipt from raw text,
        - at least one pure helper function that computes a useful derived value,
        - deterministic tests for the pure helper,
        - no host-language source code.

        Keep the project intentionally small.
    "#
}

// ---- action validation ---------------------------------------------------------

function safe_source_path(path: string) -> bool {
    let p = path.trim();
    p.starts_with("baml_src/") && p.ends_with(".baml") && !p.includes("..") && !p.includes("\\")
}

// Models often echo the whole command; normalize to the bare symbol.
function describe_query(raw: string) -> string {
    let q = raw.trim();
    if (q.starts_with("baml describe ")) {
        q.substring(14, q.length()).trim()
    } else if (q.starts_with("describe ")) {
        q.substring(9, q.length()).trim()
    } else {
        q
    }
}

function valid_describe_query(q: string) -> bool {
    !q.includes(" ")
        && !q.includes("\t")
        && !q.includes("\n")
        && !q.includes(";")
        && !q.includes("&")
}

// ---- the worker loop -----------------------------------------------------------

class BuildStats {
    model: string,
    provider: string,
    turns_used: int,
    describes: int,
    writes: int,
    llm_errors: int,
    finished: bool,
    summary: string,
}

function run_describe(project: string, q: string) -> string {
    let args = if (q == "") {
        ["describe"]
    } else {
        ["describe", q]
    };
    let out = baml
        .sys
        .exec(
            "baml",
            args,
            baml.sys.ProcessOptions { cwd: project, env: null, timeout_ms: 120000, stdin: null },
        ) catch (e) {
        _ => {
            return "describe failed to execute: " + baml.unstable.string(e);
        }
    };
    command_text(out)
}

function append_transcript(project: string, transcript: string, entry: string) -> string {
    let next = transcript + "\n\n" + entry;
    baml.fs.write(project + "/builder-transcript.md", next) catch (_) {
        _ => null
    };
    next
}

// The worker entry: builds one project with one model, records a transcript,
// and writes BuildStats to <run_dir>/build.json. Never throws; the parent
// reads the stats file to learn what happened.
function run_worker(provider: string, model: string, project: string, run_dir: string) -> int {
    baml.fs.mkdir(project + "/baml_src", baml.fs.MkdirOptions { recursive: true }) catch (_) {
        _ => null
    };
    if (!baml.fs.exists(project + "/baml.toml")) {
        baml.fs.write(project + "/baml.toml", "[package]\nname = \"model-entry\"\n") catch (_) {
            _ => null
        };
    }

    let transcript = "# Builder transcript — " + model + "\n";
    baml.fs.write(project + "/builder-transcript.md", transcript) catch (_) {
        _ => null
    };

    let describes = 0;
    let writes = 0;
    let llm_errors = 0;
    let finished = false;
    let summary = "";
    let turn = 1;
    while (turn <= max_turns() && !finished) {
        baml.io.eprintln("[" + model + " turn " + i2s(turn) + "/" + i2s(max_turns()) + "]");
        let entry = "## Turn " + i2s(turn) + "\n";
        let action = next_action(provider, task_prompt(), recent(transcript, 12000), turn) catch (e) {
            _ => {
                llm_errors = llm_errors + 1;
                transcript
                    = append_transcript(
                        project,
                        transcript,
                        entry
                            + "LLM request failed:\n\n```\n"
                            + clip(baml.unstable.string(e), 2000)
                            + "\n```",
                    );
                turn = turn + 1;
                continue;
            }
        };
        match (action) {
            let a: Describe => {
                let q = describe_query(a.query);
                if (!valid_describe_query(q)) {
                    entry = entry + "Describe rejected: query must be one symbol or module name.";
                } else {
                    describes = describes + 1;
                    entry
                        = entry
                            + "Describe `"
                            + q
                            + "`\n\n```\n"
                            + run_describe(project, q)
                            + "\n```";
                }
            },
            let a: WriteFile => {
                if (!safe_source_path(a.path)) {
                    entry
                        = entry + "WriteFile rejected: path must be a .baml file beneath baml_src/.";
                } else {
                    writes = writes + 1;
                    baml.fs.write(project + "/" + a.path.trim(), a.content) catch (_) {
                        _ => null
                    };
                    entry
                        = entry
                            + "WriteFile `"
                            + a.path.trim()
                            + "`\n\n```baml\n"
                            + a.content
                            + "\n```";
                }
            },
            let a: Done => {
                finished = true;
                summary = a.summary;
                entry = entry + "Done: " + a.summary;
            },
        }
        transcript = append_transcript(project, transcript, entry);
        turn = turn + 1;
    }

    let stats = BuildStats {
        model: model,
        provider: provider,
        turns_used: turn - 1,
        describes: describes,
        writes: writes,
        llm_errors: llm_errors,
        finished: finished,
        summary: summary,
    };
    baml.fs
        .write(
            run_dir + "/build.json",
            baml.json.to_string(stats) catch (_) {
                _ => "{}"
            },
        ) catch (_) {
        _ => null
    };
    0
}

test "safe source paths" {
    assert.is_true(safe_source_path("baml_src/main.baml"));
    assert.equal(safe_source_path("../main.baml"), false);
    assert.equal(safe_source_path("baml_src/../../secret.baml"), false);
    assert.equal(safe_source_path("/tmp/main.baml"), false);
    assert.equal(safe_source_path("baml_src/readme.md"), false);
}

test "describe query normalization" {
    assert.equal(describe_query("String"), "String");
    assert.equal(describe_query("describe Array"), "Array");
    assert.equal(describe_query("baml describe baml.fs"), "baml.fs");
    assert.is_true(valid_describe_query("baml.json"));
    assert.equal(valid_describe_query("String; rm -rf /"), false);
    assert.equal(valid_describe_query("two words"), false);
    assert.is_true(valid_describe_query(""));
}
