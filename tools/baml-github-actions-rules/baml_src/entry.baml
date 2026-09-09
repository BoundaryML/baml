// All discovery, YAML parsing, policy, diagnostics and exit status run in BAML.
type Json = baml.json.json;

class Diagnostic {
    file: string,
    location: string,
    rule: string,
    message: string,
}

class Source {
    path: string,
    text: string,
}

class Report {
    checked: string[],
    excluded: string[],
    diagnostics: Diagnostic[],
}

function field(value: Json, key: string) -> Json {
    baml.json.field(value, key)
}

function string_value(value: Json) -> string {
    match (value) {
        let s: string => s,
        _ => "",
    }
}

function object(value: Json) -> map<string, Json> {
    match (value) {
        let m: map<string, Json> => m,
        _ => {
            let empty: map<string, Json> = {};
            empty
        },
    }
}

function array(value: Json) -> Json[] {
    match (value) {
        let a: Json[] => a,
        _ => [],
    }
}

function emit(
    ds: Diagnostic[],
    path: string,
    location: string,
    rule: string,
    message: string,
) -> void {
    ds.push(Diagnostic { file: path, location, rule, message });
}

function discover(root: string, directory: string, workflows: bool) -> Source[] {
    let result: Source[] = [];
    for (let entry in baml.fs.read_dir(`${root}/${directory}`)) {
        let path = `${directory}/${entry.name}`;
        if (entry.is_symlink) {
            throw baml.errors.InvalidArgument { message: `Refusing symlink in Actions tree: ${path}` };
        }
        if (entry.is_dir) {
            result = result.concat(discover(root, path, workflows));
        } else if (
            entry.is_file
                && (
                    (
                        workflows
                            && (entry.name.ends_with(".yaml") || entry.name.ends_with(".yml"))
                    )
                        || entry.name == "action.yml"
                        || entry.name == "action.yaml"
                )
        ) {
            result.push(Source { path, text: baml.fs.read(`${root}/${path}`) });
        }
    }
    result.sort_by_key((s) -> {
        s.path
    })
}

function lint_sources(sources: Source[], audit_exclusions: bool = false) -> Report {
    let report = Report { checked: [], excluded: [], diagnostics: [] };
    let exclusions = v0_exclusions();
    for (let source in sources) {
        if (exclusions.get(source.path) != null) {
            report.excluded.push(source.path);
        } else {
            report.checked.push(source.path);
            let _ = lint_document(source, report.diagnostics) catch (e) {
                baml.yaml.ParseError => {
                    emit(report.diagnostics, source.path, "$", "YAML", e.to_string())
                },
                baml.errors.InvalidArgument => {
                    emit(report.diagnostics, source.path, "$", "STRUCTURE", e.message)
                },
            };
        }
    }
    if (audit_exclusions) {
        audit_repository(sources, report.diagnostics);
        for (let path in exclusions.keys()) {
            if (!report.excluded.includes(path)) {
                emit(
                    report.diagnostics,
                    path,
                    "$",
                    "EXCLUSION",
                    "Stale exact v0 exclusion: remove it or correct the audited path.",
                );
            }
        }
    }
    report
}

function lint_document(source: Source, ds: Diagnostic[]) -> void {
    let doc = baml.yaml.parse(source.text);
    if (!(doc is map<string, Json>)) {
        throw baml.errors.InvalidArgument { message: "Expected a YAML mapping" };
    }
    validate_canonical(source.path, doc, ds);
    validate_cache_env(source.path, "env", field(doc, "env"), ds);
    if (("/" + source.path).ends_with("/action.yml") || ("/" + source.path).ends_with("/action.yaml")) {
        let runs = field(doc, "runs");
        let using = string_value(field(runs, "using"));
        if (using == "composite") {
            lint_steps(source.path, "runs.steps", field(runs, "steps"), ds);
        } else if (using == "") {
            throw baml.errors.InvalidArgument { message: "Action must declare runs.using" };
        }
    } else {
        let jobs = field(doc, "jobs");
        if (!(jobs is map<string, Json>) || object(jobs).length() == 0) {
            throw baml.errors.InvalidArgument { message: "Workflow must contain a nonempty jobs mapping" };
        }
        for (let name in object(jobs).keys()) {
            let job = field(jobs, name);
            let location = `jobs.${name}`;
            validate_cache_env(source.path, `${location}.env`, field(job, "env"), ds);
            if (field(job, "uses") != null) {
                lint_action(source.path, `${location}.uses`, string_value(field(job, "uses")), job, ds);
            } else {
                lint_steps(source.path, `${location}.steps`, field(job, "steps"), ds);
            }
        }
    }
}

function repository_report(repo_root: string = ".") -> Report {
    lint_sources(discover_repository(repo_root), audit_exclusions = true)
}

function main(repo_root: string = ".") -> void {
    let report = repository_report(repo_root = repo_root);
    for (let diagnostic in report.diagnostics) {
        baml.io.eprintln(annotation(diagnostic));
    }
    baml.io.println(
        `Checked ${report.checked.length()} workflows/actions; explicitly excluded ${report.excluded.length()} v0 files; ${report.diagnostics.length()} violations.`,
    );
    if (report.diagnostics.length() > 0) {
        baml.sys.exit(1);
    }
}

function gha_escape(s: string) -> string {
    s.replace_all("%", "%25")
        .replace_all("\r", "%0D")
        .replace_all("\n", "%0A")
        .replace_all(",", "%2C")
        .replace_all(":", "%3A")
}

function annotation(d: Diagnostic) -> string {
    `::error file=${gha_escape(d.file)},title=${gha_escape(d.rule)}::${gha_escape(d.location + ": " + d.message)}`
}

// Also discover tracked and new, untracked action manifests outside .github.
// Git's standard ignore rules omit generated dependencies; the .github trees
// above are always walked directly, even if somebody adds them to .gitignore.
function discover_repository(repo_root: string) -> Source[] {
    let sources = discover(repo_root, ".github/workflows", true).concat(
        discover(repo_root, ".github/actions", false),
    );
    let result = baml.sys.exec(
        "git",
        [
            "-C",
            repo_root,
            "ls-files",
            "-z",
            "--cached",
            "--others",
            "--exclude-standard",
            "--",
            "action.yml",
            "action.yaml",
            "**/action.yml",
            "**/action.yaml",
        ],
        baml.sys.ProcessOptions { cwd: null, env: null, timeout_ms: 30000, stdin: null },
    );
    if (!result.ok()) {
        throw baml.errors.Io { message: "Cannot enumerate repository action manifests with git ls-files" };
    }
    for (let path in baml.String.from_utf8(result.stdout).split("\0")) {
        if (
            path != ""
                && !sources.some((s) -> {
                    s.path == path
                })
                && baml.fs.exists(`${repo_root}/${path}`)
        ) {
            sources.push(Source { path, text: baml.fs.read(`${repo_root}/${path}`) });
        }
    }
    sources.sort_by_key((s) -> {
        s.path
    })
}
