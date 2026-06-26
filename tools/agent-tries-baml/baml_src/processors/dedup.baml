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
            // TODO: fetch real open issues for cross-run dedup; for now treat all as new.
            let open_json = "[]";

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

    // Class method (outside the implements block): create/merge one issue.
    function upsert(self, iss: DedupIssue, now: int) -> int {
        let evidence: IssueEvidence[] = [];
        for (let e in iss.evidence) {
            if (e.report_id != "") {
                evidence.push(IssueEvidence { trophyId: e.report_id, call_index: e.call_index });
            }
        }
        let doc = IssueCreate {
            kind: iss.kind,
            title: iss.title,
            description: iss.description,
            suggestion: iss.suggestion,
            category: iss.category,
            evidence: evidence,
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
