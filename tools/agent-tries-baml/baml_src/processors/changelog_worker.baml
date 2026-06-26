// Changelog worker — claims a changelogEntries row and runs the draft<->critique
// loop (DraftEntry / CritiqueEntry LLM functions) until approval or an attempt
// cap, then writes title/body/date/authors. Ports services/changelog_worker.
//
// Focused port: the GitHub commit-log + file-diff context is fetched minimally
// (release list); the per-block `baml` CODE CHECK (running fenced blocks via the
// proxy) is deferred (passed as "no checks") — the draft/critique LLM loop is the
// headline and is complete.

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

    // Minimal release context: version + recent releases from GitHub.
    function build_context(self, entry: ChangelogEntryDoc) -> string {
        let version = match (entry.version) {
            let v: string => v,
            null => "unknown",
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
        let releases = github_releases(self.github_token, owner, repo) catch (_) {
            _ => "[]"
        };
        return "# Release\nversion: "
            + version
            + "\n\n# Recent releases (GitHub)\n"
            + tail_str(releases, 4000);
    }
}
