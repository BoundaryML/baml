// BAML dedup processor — batches queued trophies and runs the classify/dedup
// LLM call, upserting the issues DB. Ports services/baml_dedup. The headline win:
// where Python ran an AGENT that wrote issues.json, this calls the typed Dedup()
// function directly — no proxy, no JSON-parse glue.
//
// Focused port: open-issues fetch (cross-run dedup) + repro verification are
// deferred (TODOs); new findings become new issues. batch mode.

class IssueEvidence {
    trophyId: string,
    call_index: int?,
    turn_index: int?, // anchors the dashboard run link (/runs/<id>#turn-N)
}

class IssueCreate {
    kind: string,
    title: string,
    description: string,
    suggestion: string,
    category: string,
    evidence: IssueEvidence[],
    repro: string?,
    status: string,
    linearSyncStatus: string,
    firstSeenAt: int,
    lastSeenAt: int,
}

// One currently-open issue, as handed to the Dedup LLM ({id, kind, title}) so it
// can match a re-found finding to an existing issue and emit it with that `id`.
class OpenIssueRef {
    _id: string?,
    kind: string?,
    title: string?,
}

class OpenIssueOut {
    id: string,
    kind: string,
    title: string,
}

// Minimal read of an existing issue's evidence (to append to, not replace). Read
// as raw json[] — Convex serializes every number as a float, so a typed read with
// `call_index: int?` would fail to parse the stored 0.0 and silently drop ALL the
// existing evidence. json[] round-trips it verbatim.
class IssueEvidenceRead {
    evidence: json[]?,
}

// Patch applied when extending a matched issue: append evidence, refresh the
// write-up, bump lastSeenAt, and mark the Linear mirror dirty so the card
// re-renders (ports _upsert's merge path — without the dirty flip an extended
// issue never re-syncs to the board).
class ExtendPatch {
    evidence: json[],
    lastSeenAt: int,
    linearSyncStatus: string,
    description: string?,
    suggestion: string?,
    repro: string?,
}

class DedupProcessor {
    wid: string,
    svc: ServiceClient,
    batch_size: int,

    implements Processor {
        function config(self) -> ProcessorConfig {
            return ProcessorConfig {
                role: "baml_dedup",
                table: "trophies",
                claim_value: "queued",
                claim_into: "deduping",
                claim_field: "status",
                claim_index: "by_status_created",
                lease_ms: 1800000,
                heartbeat_ms: 60000,
                poll_backstop_ms: 3000,
                presence_ms: 15000,
                batch: true,
            };
        }

        function service(self) -> ServiceClient {
            return self.svc;
        }

        function worker_id(self) -> string {
            return self.wid;
        }

        function process(self, raw: string) -> int throws ProcessError {
            // Gather a window: `raw` is the seed; claim up to batch_size-1 more.
            let batch: string[] = [raw];
            let gathering = true;
            while (gathering && batch.length() < self.batch_size) {
                let more = self
                    .svc
                    .claim(
                        "trophies",
                        self.wid,
                        1800000,
                        "queued",
                        "deduping",
                        "status",
                        "by_status_created",
                    ) catch (_) {
                    _ => null
                };
                match (more) {
                    null => {
                        gathering = false;
                    },
                    let m: string => {
                        batch.push(m);
                    },
                };
            }

            let reports = render_reports(batch);
            // Real cross-run dedup: hand the LLM the currently-open issues so a
            // re-found finding extends the existing issue (emitted with its `id`)
            // instead of spawning a near-duplicate (cf. the 5 backtick-bug dupes).
            let open_json = fetch_open_issues(self.svc, 200);

            let result = Dedup(reports, open_json) catch (e) {
                _ => {
                    throw ProcessError { message: "Dedup failed: " + baml.unstable.string(e) }
                }
            };

            let now = baml.sys.now_ms();
            for (let iss in result.issues) {
                if (iss.kind == "skill" || iss.kind == "language") {
                    self.upsert(iss, now);
                }
            }

            // Mark the whole batch done.
            for (let b in batch) {
                let id = trophy_id_of(b);
                let _ = self.svc.transition("trophies", id, "done", "status", "{}", true) catch (_) {
                    _ => ""
                };
            }
            return 0;
        }
    }

    // Class method (outside the implements block): create OR extend one issue. When
    // the Dedup LLM matched the finding to an open issue it carries that issue's
    // `id` -> extend it; otherwise it's genuinely new -> create.
    function upsert(self, iss: DedupIssue, now: int) -> int {
        let existing = match (iss.id) {
            let v: string => v,
            null => "",
        };
        if (existing != "") {
            return self.extend(existing, iss, now);
        }
        return self.create_doc(iss, now);
    }

    // Evidence entries from a finding (skips entries with no report id).
    function evidence_of(self, iss: DedupIssue) -> IssueEvidence[] {
        let evidence: IssueEvidence[] = [];
        for (let e in iss.evidence) {
            if (e.report_id != "") {
                evidence
                    .push(
                        IssueEvidence {
                            trophyId: e.report_id,
                            call_index: e.call_index,
                            turn_index: e.turn_index,
                        },
                    );
            }
        }
        return evidence;
    }

    function create_doc(self, iss: DedupIssue, now: int) -> int {
        let doc = IssueCreate {
            kind: iss.kind,
            title: iss.title,
            description: iss.description,
            suggestion: iss.suggestion,
            category: iss.category,
            evidence: self.evidence_of(iss),
            repro: iss.repro,
            status: "open",
            linearSyncStatus: "dirty",
            firstSeenAt: now,
            lastSeenAt: now,
        };
        let doc_json = baml.json.to_string(doc) catch (_) {
            _ => ""
        };
        if (doc_json != "") {
            let _ = self.svc.create("issues", doc_json) catch (_) {
                _ => ""
            };
        }
        return 0;
    }

    // Extend a matched open issue: append this batch's evidence and bump lastSeenAt.
    // If the issue vanished (e.g. rejected mid-batch) fall back to a fresh create so
    // the finding isn't silently dropped.
    function extend(self, id: string, iss: DedupIssue, now: int) -> int {
        let raw = self.svc.get("issues", id) catch (_) {
            _ => null
        };
        let cur_raw = match (raw) {
            let r: string => r,
            null => "",
        };
        if (cur_raw == "") {
            return self.create_doc(iss, now);
        }
        let cur = baml.json.from_string<IssueEvidenceRead>(cur_raw) catch (_) {
            _ => IssueEvidenceRead { evidence: null }
        };
        // Keep the existing evidence verbatim (json[]) and append this batch's new
        // entries (each typed entry round-tripped through json so the array stays
        // homogeneous and Convex-float-safe).
        let merged: json[] = match (cur.evidence) {
            let ev: json[] => ev,
            null => [],
        };
        for (let e in self.evidence_of(iss)) {
            let j = baml.json.parse(baml.json.to_string(e) catch (_) { _ => "null" }) catch (_) {
                _ => 0
            };
            merged.push(j);
        }
        let patch = baml
            .json
            .to_string(
                ExtendPatch {
                    evidence: merged,
                    lastSeenAt: now,
                    linearSyncStatus: "dirty",
                    description: nz_str(iss.description),
                    suggestion: nz_str(iss.suggestion),
                    repro: iss.repro,
                },
            ) catch (_) {
            _ => ""
        };
        if (patch != "") {
            let _ = self.svc.update("issues", id, patch) catch (_) {
                _ => ""
            };
        }
        return 0;
    }
}

// Statuses an issue counts as open/in-flight for cross-run dedup matching (every
// non-terminal state; closed/rejected are terminal and excluded).
function dedup_open_statuses() -> string[] {
    return [
        "open",
        "confirmed",
        "approved",
        "dispatching",
        "tocursor",
        "prprep",
        "pr_ready",
        "verifying",
        "redraft",
        "redrafting",
        "needs_human",
    ];
}

// Fetch the currently-open issues as the {id, kind, title} JSON the Dedup prompt
// expects (per status, capped at `limit`). "[]" when nothing is open.
function fetch_open_issues(svc: ServiceClient, limit: int) -> string {
    let outs: OpenIssueOut[] = [];
    for (let status in dedup_open_statuses()) {
        let raw = svc.list("issues", "status", status, "by_status_created", limit) catch (_) {
            _ => "[]"
        };
        let issues: OpenIssueRef[] = baml.json.from_string<OpenIssueRef[]>(raw) catch (_) {
            _ => []
        };
        for (let iss in issues) {
            let id = match (iss._id) {
                let v: string => v,
                null => "",
            };
            if (id != "") {
                outs
                    .push(
                        OpenIssueOut {
                            id: id,
                            kind: match (iss.kind) {
                                let k: string => k,
                                null => "",
                            },
                            title: match (iss.title) {
                                let t: string => t,
                                null => "",
                            },
                        },
                    );
            }
        }
    }
    return baml.json.to_string(outs) catch (_) {
        _ => "[]"
    };
}

// Render a batch of trophy JSONs into the reports.md the Dedup prompt expects.
function render_reports(batch: string[]) -> string {
    let parts: string[] = [];
    for (let raw in batch) {
        let t = baml.json.from_string<Trophy>(raw) catch (_) {
            _ => null
        };
        match (t) {
            let tr: Trophy => {
                let id = match (tr._id) {
                    let v: string => v,
                    null => "unknown",
                };
                let summary = match (tr.summary) {
                    let s: string => s,
                    null => "",
                };
                let report = match (tr.reportMd) {
                    let r: string => r,
                    null => "",
                };
                let findings = match (tr.findings) {
                    let f: json[] => baml.json.stringify(f),
                    null => "[]",
                };
                parts
                    .push(
                        "--- report_id: "
                            + id
                            + " ---\n## Summary\n"
                            + summary
                            + "\n\n"
                            + report
                            + "\n\n## Findings (json)\n"
                            + findings
                            + "\n",
                    );
            },
            null => null,
        };
    }
    return parts.join("\n");
}

function trophy_id_of(raw: string) -> string {
    let parsed = baml.json.from_string<DocId>(raw) catch (_) {
        _ => DocId { _id: null }
    };
    return match (parsed._id) {
        let v: string => v,
        null => "",
    };
}
