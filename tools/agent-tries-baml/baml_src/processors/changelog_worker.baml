// Changelog worker — claims a changelogEntries row and runs the draft<->critique
// loop (DraftEntry / CritiqueEntry LLM functions) until approval or an attempt
// cap, then writes title/body/date/authors. Ports services/changelog_worker.
//
// Context parity with v1 (bench_core.changelog_github.collect_context): the
// release date, the predecessor release on the SAME channel, the commit log for
// the range, and the actual file-level diff (skip-listed, per-file + total char
// budgets). The per-block `baml` CODE CHECK (running fenced blocks via the
// proxy) is still deferred (passed as "no checks").

class ChangelogEntryDoc {
    _id: string?,
    version: string?,
    tag: string?,
    channel: string?,
    fromRelease: string?,
    reviseMode: string?,
    reviseGuidance: string?,
}

class ChangelogPatch {
    title: string,
    body: string,
    date: string,
    authors: string[],
    status: string,
}

class ChangelogWorkerProcessor {
    wid: string,
    svc: ServiceClient,
    github_token: string,
    repo_slug: string,
    max_attempts: int,

    implements Processor {
        function config(self) -> ProcessorConfig {
            return ProcessorConfig {
                role: "changelog_worker",
                table: "changelogEntries",
                claim_value: "queued",
                claim_into: "generating",
                claim_field: "status",
                claim_index: "by_status_created",
                lease_ms: 1800000,
                heartbeat_ms: 60000,
                poll_backstop_ms: 3000,
                presence_ms: 15000,
                batch: false,
            };
        }

        function service(self) -> ServiceClient {
            return self.svc;
        }

        function worker_id(self) -> string {
            return self.wid;
        }

        function process(self, raw: string) -> int throws ProcessError {
            let entry = baml.json.from_string<ChangelogEntryDoc>(raw) catch (_) {
                _ => {
                    throw ProcessError { message: "bad changelog entry json" }
                }
            };
            let id = match (entry._id) {
                let v: string => v,
                null => {
                    throw ProcessError { message: "entry missing _id" }
                },
            };

            let context = self.build_context(entry);

            // Draft -> critique loop.
            let draft = DraftEntry(context) catch (e) {
                _ => {
                    throw ProcessError { message: "DraftEntry failed: " + baml.unstable.string(e) }
                }
            };
            let best = draft;
            let attempt = 1;
            let approved = false;
            while (attempt < self.max_attempts && approved == false) {
                let draft_json = baml.json.to_string(best) catch (_) {
                    _ => "{}"
                };
                let critique = CritiqueEntry(context, draft_json, "CODE CHECK: no blocks run") catch (_) {
                    _ => null
                };
                match (critique) {
                    let c: Critique => {
                        if (c.verdict == "approve") {
                            approved = true;
                        } else {
                            best
                                = DraftEntry(context + "\n\n# REVISE\n" + c.rewrite_hints) catch (_) {
                                    _ => best
                                };
                        }
                    },
                    null => {
                        approved = true;
                    },
                };
                attempt = attempt + 1;
            }

            let patch = baml
                .json
                .to_string(
                    ChangelogPatch {
                        title: best.title,
                        body: best.body,
                        date: best.date,
                        authors: best.authors,
                        status: "done",
                    },
                ) catch (_) {
                _ => "{}"
            };
            let _ = self.svc.update("changelogEntries", id, patch) catch (e) {
                _ => {
                    throw ProcessError { message: "write changelog failed: " + baml.unstable.string(e) }
                }
            };
            let _ = self.svc.transition("changelogEntries", id, "done", "status", "{}", true) catch (_) {
                _ => ""
            };
            return 0;
        }
    }

    // Full release context (ports collect_context): version/date/predecessor on
    // the same channel + commit log + file diff. Degrades gracefully — a failed
    // GitHub call yields an honest "no diff available" context, never a throw.
    function build_context(self, entry: ChangelogEntryDoc) -> string {
        let version = match (entry.version) {
            let v: string => v,
            null => "unknown",
        };
        let channel = match (entry.channel) {
            let c: string => c,
            null => channel_of(version),
        };
        let tag = match (entry.tag) {
            let t: string => t,
            null => to_tag(version, channel),
        };
        let parts = self.repo_slug.split("/");
        let owner = match (parts.at(0)) {
            let v: string => v,
            null => "BoundaryML",
        };
        let repo = match (parts.at(1)) {
            let v: string => v,
            null => "baml",
        };

        // Release date.
        let rel_raw = github_release_by_tag(self.github_token, owner, repo, tag) catch (_) {
            _ => "{}"
        };
        let rel = baml.json.from_string<GhReleaseInfo>(rel_raw) catch (_) {
            _ => GhReleaseInfo { tag_name: null, published_at: null, created_at: null, draft: null }
        };
        let date = release_date_of(rel);

        // Predecessor on the same channel (fromRelease override wins).
        let base_tag = match (entry.fromRelease) {
            let f: string => f,
            null => {
                let releases_raw = github_releases_wide(self.github_token, owner, repo) catch (_) {
                    _ => "[]"
                };
                previous_release_tag(releases_raw, tag, channel)
            },
        };

        // Commit log + diff for base...head.
        let commit_log = "";
        let authors = "";
        let diff = "";
        if (base_tag != "") {
            let cmp_raw = github_compare(self.github_token, owner, repo, base_tag, tag) catch (_) {
                _ => "{}"
            };
            let cmp = baml.json.from_string<GhCompareInfo>(cmp_raw) catch (_) {
                _ => GhCompareInfo { commits: null, files: null }
            };
            let rendered = render_commit_log(cmp);
            commit_log = rendered.log;
            authors = rendered.authors.join(", ");
            diff = render_diff_for_prompt(cmp, 80000);
        }

        return "# release context\nversion: "
            + version
            + "\nfrom_version: "
            + match (base_tag) {
                "" => "(none — oldest release on this channel, or lookup failed)",
                _ => normalize_tag(base_tag),
            }
            + "\ndate: "
            + date
            + "\nchannel: "
            + channel
            + "\n\n# authors (GitHub logins from the commit range)\n"
            + match (authors) {
                "" => "(unknown)",
                _ => authors,
            }
            + "\n\n# commit log ("
            + match (base_tag) {
                "" => "no base",
                _ => base_tag + ".." + tag,
            }
            + ")\n"
            + match (commit_log) {
                "" => "(no commit log available for this range)",
                _ => commit_log,
            }
            + "\n\n# file diff\n"
            + match (diff) {
                "" => "(no file diff available for this range)",
                _ => diff,
            };
    }
}

// ---- GitHub release/compare parsing (lenient typed reads) -------------------

class GhReleaseInfo {
    tag_name: string?,
    published_at: string?,
    created_at: string?,
    draft: bool?,
}

class GhCompareInfo {
    commits: json[]?,
    files: json[]?,
}

class GhCommitEntry {
    sha: string?,
    commit: GhCommitInner?,
    author: GhAuthorRef?,
}

class GhCommitInner {
    message: string?,
    author: GhNameRef?,
}

class GhNameRef {
    name: string?,
}

class GhAuthorRef {
    login: string?,
}

class GhFileEntry {
    filename: string?,
    status: string?,
    additions: float?,
    deletions: float?,
    patch: string?,
}

class RenderedLog {
    log: string,
    authors: string[],
}

// Reconstruct the GitHub tag from a stored (normalized) version + channel
// (ports changelog_github.to_tag).
function to_tag(version: string, channel: string) -> string {
    if (channel == "nightly" || channel == "canary" || channel == "alpha") {
        return "baml-language-" + version;
    }
    return version;
}

// YYYY-MM-DD from published_at (fallback created_at).
function release_date_of(rel: GhReleaseInfo) -> string {
    let ts = match (rel.published_at) {
        let p: string => p,
        null => match (rel.created_at) {
            let c: string => c,
            null => "",
        },
    };
    if (ts.length() >= 10) {
        return ts.substring(0, 10) catch (_) {
            _ => ts
        };
    }
    return ts;
}

// The tag of the release immediately before `tag` on the same channel, by
// created_at (newest-first listing). "" when none / unknown.
function previous_release_tag(releases_raw: string, tag: string, channel: string) -> string {
    let rels: GhReleaseInfo[] = baml.json.from_string<GhReleaseInfo[]>(releases_raw) catch (_) {
        _ => []
    };
    let target_ts = "";
    for (let r in rels) {
        let rtag = match (r.tag_name) {
            let t: string => t,
            null => "",
        };
        if (rtag == tag) {
            target_ts = match (r.created_at) {
                let c: string => c,
                null => "",
            };
        }
    }
    if (target_ts == "") {
        return "";
    }
    // Newest-first: the first same-channel release strictly older IS the predecessor.
    for (let r in rels) {
        let is_draft = match (r.draft) {
            let d: bool => d,
            null => false,
        };
        let rtag = match (r.tag_name) {
            let t: string => t,
            null => "",
        };
        let rts = match (r.created_at) {
            let c: string => c,
            null => "",
        };
        if (is_draft == false && rtag != "" && channel_of(rtag) == channel && rts < target_ts) {
            return rtag;
        }
    }
    return "";
}

// One line per commit: `sha7 subject (author)`; authors deduped, bots dropped,
// capped at 200 commits (ports collect_context).
function render_commit_log(cmp: GhCompareInfo) -> RenderedLog {
    let lines: string[] = [];
    let authors: string[] = [];
    let commits: json[] = match (cmp.commits) {
        let c: json[] => c,
        null => [],
    };
    for (let cj in commits) {
        if (lines.length() < 200) {
            let raw = baml.json.stringify(cj) catch (_) {
                _ => "{}"
            };
            let e = baml.json.from_string<GhCommitEntry>(raw) catch (_) {
                _ => GhCommitEntry { sha: null, commit: null, author: null }
            };
            let sha = match (e.sha) {
                let v: string => head_str(v, 7),
                null => "",
            };
            let subject = match (e.commit) {
                let inner: GhCommitInner => match (inner.message) {
                    let m: string => first_of(m.split("\n")),
                    null => "",
                },
                null => "",
            };
            let who = commit_author_of(e);
            if (who != "" && in_list(authors, who) == false && who.ends_with("[bot]") == false) {
                authors.push(who);
            }
            let line = sha + " " + subject;
            if (who != "") {
                line = line + " (" + who + ")";
            }
            lines.push(line);
        }
    }
    return RenderedLog { log: lines.join("\n"), authors: authors };
}

function commit_author_of(e: GhCommitEntry) -> string {
    let login = match (e.author) {
        let a: GhAuthorRef => match (a.login) {
            let l: string => l,
            null => "",
        },
        null => "",
    };
    if (login != "") {
        return login;
    }
    return match (e.commit) {
        let inner: GhCommitInner => match (inner.author) {
            let n: GhNameRef => match (n.name) {
                let v: string => v,
                null => "",
            },
            null => "",
        },
        null => "",
    };
}

// Concatenated per-file patches up to `budget` chars, each file capped at 6000
// so one huge file can't eat the budget; lockfiles/generated/binary paths are
// skipped (ports _diff_for_prompt).
function render_diff_for_prompt(cmp: GhCompareInfo, budget: int) -> string {
    let parts: string[] = [];
    let used = 0;
    let files: json[] = match (cmp.files) {
        let f: json[] => f,
        null => [],
    };
    let truncated = false;
    for (let fj in files) {
        if (truncated == false && used < budget) {
            let raw = baml.json.stringify(fj) catch (_) {
                _ => "{}"
            };
            let f = baml.json.from_string<GhFileEntry>(raw) catch (_) {
                _ => GhFileEntry { filename: null, status: null, additions: null, deletions: null, patch: null }
            };
            let name = match (f.filename) {
                let n: string => n,
                null => "",
            };
            let patch = match (f.patch) {
                let pt: string => pt,
                null => "",
            };
            if (name != "" && patch != "" && diff_skip_file(name) == false) {
                let capped = if (patch.length() > 6000) {
                    head_str(patch, 6000) + "\n[...patch truncated...]"
                } else {
                    patch
                };
                let adds = match (f.additions) {
                    let a: float => f2i(a),
                    null => 0,
                };
                let dels = match (f.deletions) {
                    let d: float => f2i(d),
                    null => 0,
                };
                let status = match (f.status) {
                    let st: string => st,
                    null => "?",
                };
                let block = "\n=== "
                    + name
                    + " ("
                    + status
                    + ": +"
                    + baml.unstable.string(adds)
                    + "/-"
                    + baml.unstable.string(dels)
                    + ") ===\n"
                    + capped
                    + "\n";
                if (used + block.length() > budget) {
                    parts.push("\n[diff truncated at ~" + baml.unstable.string(budget) + " chars]\n");
                    truncated = true;
                } else {
                    parts.push(block);
                    used = used + block.length();
                }
            }
        }
    }
    return parts.join("");
}

// Lockfiles / generated code / binaries never belong in the prompt.
function diff_skip_file(name: string) -> bool {
    let lowered = name.to_lower_case();
    let ext_skip = lowered.ends_with(".lock")
        || lowered.ends_with(".snap")
        || lowered.ends_with(".min.js")
        || lowered.ends_with(".map")
        || lowered.ends_with(".svg")
        || lowered.ends_with(".png")
        || lowered.ends_with(".jpg")
        || lowered.ends_with(".jpeg")
        || lowered.ends_with(".gif")
        || lowered.ends_with(".ico")
        || lowered.ends_with(".woff")
        || lowered.ends_with(".woff2")
        || lowered.ends_with(".ttf")
        || lowered.ends_with(".otf")
        || lowered.ends_with(".pdf")
        || lowered.ends_with(".zip")
        || lowered.ends_with("package-lock.json");
    if (ext_skip) {
        return true;
    }
    return lowered.includes("node_modules/")
        || lowered.includes("_generated")
        || lowered.includes("__snapshots__")
        || lowered.includes(".next/")
        || lowered.includes("dist/")
        || lowered.includes("build/")
        || lowered.includes("target/");
}
