function load_sources(directory: string, prefix: string = "") -> map<string, string> {
    let sources: map<string, string> = {};
    for (let entry in baml.fs.read_dir(directory)) {
        if (entry.is_symlink) {
            continue;
        }
        let relative = prefix + entry.name;
        let path = directory + "/" + entry.name;
        if (entry.is_dir) {
            let nested = load_sources(path, prefix = relative + "/");
            for (let key in nested.keys()) {
                sources[key] = nested[key];
            }
        } else if (entry.is_file && entry.name.ends_with(".baml")) {
            sources[relative] = baml.fs.read(path);
        }
    }
    sources
}

// Filesystem adapters. The policy engine itself accepts typed values and source maps.
function lint_policy(repo: string) -> LintReport {
    lint_sources(baml.fs.read(repo + "/.github/codeturtle/OWNERS"), load_sources(repo + "/.github/codeturtle"))
}

function run(repo: string, changes_path: string, concurrency: int) -> Report {
    let pr = baml.json.from_string<PullRequest>(baml.fs.read(changes_path));
    check_sources(
        pr,
        baml.fs.read(repo + "/.github/codeturtle/OWNERS"),
        load_sources(repo + "/.github/codeturtle"),
        concurrency = concurrency,
    )
}
