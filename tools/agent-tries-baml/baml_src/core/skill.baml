// Latest BAML skill, pulled straight from the baml-skill GitHub repo (instead of
// `baml agent install`), so every bench run onboards from the freshest skill.
// Ports the spirit of services/baml_worker/skill_repo.py: shallow-clone (or pull)
// the repo into a cache, then concatenate every SKILL.md under it into one doc
// (each prefixed with its parent dir name), which the worker injects as SKILL.md.

function skill_repo_url() -> string {
    return env_or("ATB_SKILL_REPO_URL", "https://github.com/BoundaryML/baml-skill.git");
}

function skill_cache_dir() -> string {
    return env_or("BAML_SKILL_CACHE_DIR", "/tmp/baml-skill-cache");
}

// Refresh the local checkout to the latest default branch, then return the
// concatenated skill markdown ("" on any failure — runs degrade to no skill).
function fetch_latest_skill() -> string {
    let cache = skill_cache_dir();
    let has_git = baml.fs.exists(cache + "/.git") catch (_) {
        _ => false
    };
    if (has_git) {
        // Already cloned: fast-forward to the latest skill.
        let _ = run_capture(
            "git",
            ["-C", cache, "pull", "--ff-only", "--depth", "1"],
            "/tmp",
            null,
            60000,
            null,
            false,
            null,
        );
    } else {
        let _ = run_capture(
            "git",
            ["clone", "--depth", "1", skill_repo_url(), cache],
            "/tmp",
            null,
            120000,
            null,
            false,
            null,
        );
    }
    return concat_skills(cache);
}

// Walk the checkout and concatenate every SKILL.md (parent-dir-labeled).
function concat_skills(dir: string) -> string {
    let parts: string[] = [];
    collect_skill_md(dir, parts);
    return parts.join("\n\n---\n\n");
}

function collect_skill_md(dir: string, parts: string[]) -> int {
    let entries: baml.fs.DirEntry[] = baml.fs.read_dir(dir) catch (_) {
        _ => []
    };
    for (let e in entries) {
        if (e.is_dir && e.name != ".git") {
            let _ = collect_skill_md(dir + "/" + e.name, parts);
        } else if (e.is_file && e.name == "SKILL.md") {
            let content = baml.fs.read(dir + "/" + e.name) catch (_) {
                _ => ""
            };
            if (content != "") {
                parts.push("# BAML skill: " + last_path_seg(dir) + "\n\n" + content);
            }
        }
    }
    return 0;
}

function last_path_seg(path: string) -> string {
    let segs = path.split("/");
    let n = segs.length();
    if (n == 0) {
        return path;
    }
    return match (segs.at(n - 1)) {
        let s: string => s,
        null => path,
    };
}

// The baml-skill main HEAD sha via the GitHub API — identifies what
// `baml agent install` resolves to at dispatch time (the agent runs
// the install itself, so there's no local clone to ask). "" on API failure.
function skill_main_sha() -> string {
    let raw = github_get(github_token(), "/repos/BoundaryML/baml-skill/commits/main") catch (_) {
        _ => "{}"
    };
    let parsed = baml.json.from_string<GhCommitEntry>(raw) catch (_) {
        _ => GhCommitEntry { sha: null, commit: null, author: null }
    };
    return match (parsed.sha) {
        let v: string => head_str(v, 7),
        null => "",
    };
}

// Current baml-skill commit sha (so a run records exactly which skill it read).
function skill_version() -> string {
    let cache = skill_cache_dir();
    let res = run_capture("git", ["-C", cache, "rev-parse", "--short", "HEAD"], "/tmp", null, 15000, null, false, null);
    return res.stdout.trim();
}
