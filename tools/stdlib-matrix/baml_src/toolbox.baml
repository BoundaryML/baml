// What a model may look up while it works.
//
// The old pipeline handed the model a lexically narrowed shortlist and asked it
// to pick. That was an approximation of looking something up, forced by the
// model having no way to ask — and it was the approximation, not the model,
// that put a ceiling on the answers: a menu built by string similarity cannot
// contain `baml.sys.exec` when the question is `child_process.spawn`.
//
// So the model gets to ask instead. Four tools, each read-only and each
// answering in one shape:
//
//   describe  one symbol in full — `baml describe`, the real thing
//   search    free text over names and docstrings
//   examples  real call sites, from the repository
//   docs      one section of TYPE_SYSTEM.md
//
// `describe` and `search` are both the compiler: `baml describe NAME` and
// `baml describe --search NAME`. Neither reads the export document this run
// already holds, which was not the first design — the export is the same data,
// and a subprocess per lookup sounded expensive. Measured, it is under 200ms,
// which is nothing beside the model call it serves.
//
// The deciding argument was not cost but ownership. "How do I find something in
// a BAML codebase" is a question the compiler should answer for everyone, not
// one this tool should answer privately and better. The first version of
// `search` here scored docstrings in BAML; it now lives in `baml_cli` as
// `--search`, where every agent and every user gets it, and where it is tested
// against the real surface rather than against this tool's fixtures.
//
// Nothing here interpolates into a shell. Every argument is one element of an
// argv array passed to `baml.sys.exec`, and the ones that take free text are
// guarded against being read as options.

/// Everything the tools answer from, assembled once per run.
class Toolbox {
    /// TYPE_SYSTEM.md, split on its `##` headings.
    doc_headings: string[],
    doc_bodies: map<string, string>,
    /// Where the subprocess tools run.
    repo_root: string,
    /// The compiler to ask. Passed in rather than found on `PATH`, so a run
    /// describes the stdlib it is comparing rather than whichever `baml` a
    /// machine happens to have installed.
    baml_bin: string,
}

function build_toolbox(repo_root: string, type_system_file: string, baml_bin: string) -> Toolbox {
    let sections = load_doc_sections(type_system_file);
    Toolbox {
        doc_headings: sections.headings,
        doc_bodies: sections.bodies,
        repo_root: repo_root,
        baml_bin: baml_bin,
    }
}

// ── search ──────────────────────────────────────────────────────────────────

/// How many results a search returns. Enough to show a real menu, few enough
/// that a wrong query is obvious rather than buried.
function search_limit() -> string {
    "25"
}

/// Free text over names and docstrings.
///
/// The question a TypeScript reader actually has is "what runs a subprocess",
/// not "what is `baml.sys.exec`" — they do not know that name yet. `describe`
/// cannot answer that; it resolves names, and `baml describe subprocess` says
/// "no symbol found". This is the tool for the other half.
function tool_search(box: Toolbox, query: string) -> string {
    let text = query.trim();
    if (text.length() == 0) {
        return "search needs something to look for";
    }
    if (text.starts_with("-")) {
        return `"${text}" is not something to search for`;
    }
    run_baml(box, ["describe", "--search", "--limit", search_limit(), "--color", "never", text])
}

// ── describe ────────────────────────────────────────────────────────────────

/// One symbol, as the compiler describes it.
///
/// The budget is raised well above the CLI's default of 30 lines because a
/// namespace or a large class is exactly what gets asked about, and a truncated
/// answer reads as a complete one.
function describe_budget() -> string {
    "200"
}

function tool_describe(box: Toolbox, query: string) -> string {
    let name = query.trim();
    if (name.length() == 0) {
        return "describe needs a symbol name";
    }
    // One argv element, so there is no shell to escape — but `describe` would
    // still read a leading dash as one of its own options.
    if (name.starts_with("-")) {
        return `"${name}" is not a symbol name`;
    }
    run_baml(box, ["describe", "--budget", describe_budget(), "--color", "never", name])
}

/// Advice to whoever started the run, dropped from what the model is shown.
///
/// Stderr is kept because the most useful failure lives there — `describe`
/// prints "no symbol found" on stdout and its suggestions on stderr — but it
/// also carries warnings about how the compiler was invoked, which answer a
/// question nobody asked and which the model cannot act on.
function strip_warnings(text: string) -> string {
    let kept: string[] = [];
    for (let line in text.lines()) {
        if (!line.starts_with("warning:")) {
            kept.push(line);
        }
    }
    kept.join("\n").trim()
}

/// Runs the compiler and returns whatever it said.
///
/// Both streams are returned on failure: `describe` reports "no symbol found"
/// and its suggestions on stdout while exiting non-zero, and dropping that
/// would turn the single most useful failure — a near miss with alternatives —
/// into a bare error.
function run_baml(box: Toolbox, args: string[]) -> string {
    let result = baml.sys.exec(
        box.baml_bin,
        args,
        baml.sys.ProcessOptions { cwd: box.repo_root, env: null, timeout_ms: 30000, stdin: null },
    ) catch_all (error) {
        _ => {
            return `could not run the compiler: ${error}`;
        },
    };
    let out = baml.String.from_utf8(result.stdout) catch_all (_) {
        _ => "",
    };
    let err = baml.String.from_utf8(result.stderr) catch_all (_) {
        _ => "",
    };
    let text = `${out.trim()}\n${strip_warnings(err)}`.trim();
    if (text.length() == 0) {
        `the compiler said nothing (exit ${result.exit_code})`
    } else {
        text
    }
}

// ── examples ────────────────────────────────────────────────────────────────

function examples_limit() -> int {
    20
}

/// Where a call site is worth showing from: the standard library itself and the
/// compiler's own test corpus. The rest of the repository is generated output,
/// fixtures for parser errors, and this tool's own source.
function examples_paths() -> string[] {
    ["baml_language/crates/baml_builtins2/*.baml", "baml_language/crates/baml_tests/*.baml"]
}

/// Real call sites for a symbol.
///
/// `git` rather than a walk of the tree, because it is already a dependency and
/// already knows which files are checked in. The needle is one argv element and
/// never touches a shell: `--fixed-strings` stops it being read as a pattern,
/// `-e` stops a leading dash being read as an option, and `--` ends the options
/// before the pathspecs.
function tool_examples(box: Toolbox, query: string) -> string {
    let needle = query.trim();
    if (needle.length() < 3) {
        return "examples needs at least three characters to search for";
    }
    let args = ["grep", "--no-color", "-n", "--fixed-strings", "-e", needle, "--"];
    for (let path in examples_paths()) {
        args.push(path);
    }
    let result = baml.sys.exec(
        "git",
        args,
        baml.sys.ProcessOptions { cwd: box.repo_root, env: null, timeout_ms: 20000, stdin: null },
    ) catch_all (error) {
        _ => {
            return `could not search for examples: ${error}`;
        },
    };
    // `git grep` exits 1 when it finds nothing, which is an answer rather than
    // a failure. Anything else is a real error and is reported as one.
    if (result.exit_code == 1) {
        return `no call sites for "${needle}" in the standard library or its tests`;
    }
    if (result.exit_code != 0) {
        return `could not search for examples: git grep exited ${result.exit_code}`;
    }
    let text = baml.String.from_utf8(result.stdout) catch_all (_) {
        _ => {
            return "could not search for examples: output was not valid UTF-8";
        },
    };
    let out: string[] = [];
    let total = 0;
    for (let line in text.lines()) {
        if (line.trim().length() > 0) {
            total += 1;
            if (out.length() < examples_limit()) {
                out.push(line.trim());
            }
        }
    }
    let header = if (total > out.length()) {
        `${total} call sites, first ${out.length()}:`
    } else {
        `${total} call sites:`
    };
    `${header}\n${out.join("\n")}`
}

// ── docs ────────────────────────────────────────────────────────────────────

class DocSections {
    headings: string[],
    bodies: map<string, string>,
}

/// TYPE_SYSTEM.md, split on its `##` headings.
///
/// Served a section at a time rather than whole because it is 40KB: putting all
/// of it in every session's prompt would cost more than the sessions do, and
/// most sessions need none of it.
function load_doc_sections(path: string) -> DocSections {
    let out = DocSections { headings: [], bodies: {} };
    let text = baml.fs.read(path) catch_all (_) {
        _ => {
            return out;
        },
    };
    let current = "";
    let buffer: string[] = [];
    for (let line in text.lines()) {
        // `### ` does not match: the third character is a hash, not a space, so
        // subsections stay inside the section that owns them.
        if (line.starts_with("## ")) {
            if (current.length() > 0) {
                let _ = out.bodies.set(current, buffer.join("\n"));
            }
            current = line.substring(3, line.length()).trim();
            out.headings.push(current);
            buffer = [];
        } else if (current.length() > 0) {
            buffer.push(line);
        }
    }
    if (current.length() > 0) {
        let _ = out.bodies.set(current, buffer.join("\n"));
    }
    out
}

function tool_docs(box: Toolbox, query: string) -> string {
    if (box.doc_headings.length() == 0) {
        return "the type system documentation was not found";
    }
    let wanted = query.trim().to_lower_case();
    if (wanted.length() == 0) {
        return `sections: ${box.doc_headings.join(" · ")}`;
    }
    let found = "";
    for (let heading in box.doc_headings) {
        if (found.length() == 0 && heading.to_lower_case().includes(wanted)) {
            found = heading;
        }
    }
    if (found.length() == 0) {
        return `no section matches "${query}" — sections: ${box.doc_headings.join(" · ")}`;
    }
    `## ${found}\n${box.doc_bodies.get(found) ?? ""}`
}

// ── dispatch ────────────────────────────────────────────────────────────────

function tool_names() -> string[] {
    ["describe", "search", "examples", "docs"]
}

/// Runs one lookup. An unknown tool is answered rather than thrown: a model
/// that invents a name should be told what exists and get another turn, not
/// take the session down with it.
function run_tool(box: Toolbox, name: string, argument: string) -> string {
    match (name.trim().to_lower_case()) {
        "describe" => tool_describe(box, argument),
        "search" => tool_search(box, argument),
        "examples" => tool_examples(box, argument),
        "docs" => tool_docs(box, argument),
        _ => `no tool is called "${name}" — the tools are ${tool_names().join(", ")}`,
    }
}

// ── the skill ───────────────────────────────────────────────────────────────

/// Where the BAML skill is published. Fetched rather than vendored so a run
/// teaches the model what the skill says today, not what it said when this file
/// was written.
function skill_url() -> string {
    "https://raw.githubusercontent.com/BoundaryML/baml-skill/refs/heads/main/plugins/baml/skills/core/SKILL.md"
}

/// The skill text, from the network or from the last run that reached it.
///
/// The cache is not an optimization. A run that quietly loses its documentation
/// produces worse judgements that look exactly like good ones, so the order is:
/// fetch, fall back to the cache and say so, and fail if there is neither.
/// Failing is the right outcome — the alternative is paying for a whole run to
/// get answers nobody can tell are degraded.
function load_skill(cache_file: string) -> string {
    let fetched = fetch_skill();
    if (fetched.length() > 0) {
        let _ = baml.fs.write(cache_file, fetched) catch_all (_) {
            _ => null,
        };
        return fetched;
    }
    if (baml.fs.exists(cache_file)) {
        report_progress("could not fetch the BAML skill; using the cached copy");
        return baml.fs.read(cache_file) catch_all (_) {
            _ => "",
        };
    }
    throw baml.errors.InvalidArgument {
        message: `could not fetch the BAML skill from ${skill_url()} and no cached copy exists at ${cache_file}`,
    };
}

/// The skill, or an empty string if it could not be fetched.
function fetch_skill() -> string {
    let response = baml.http.fetch(skill_url()) catch_all (_) {
        _ => {
            return "";
        },
    };
    if (!response.ok()) {
        return "";
    }
    response.text() catch_all (_) {
        _ => "",
    }
}
