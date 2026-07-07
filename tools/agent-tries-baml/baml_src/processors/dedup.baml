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
    bamlVersion: string?, // env of the run behind this evidence
    skillVersion: string?,
}

// The environment one run executed under (stamped onto issues + evidence so
// Linear cards can say exactly which baml + skill produced a finding).
class RunEnv {
    bamlVersion: string?,
    skillUsed: string?,
    skillVersion: string?,
}

// A claimed transcript comment (Convex doc; numbers are floats).
class CommentRow {
    _id: string?,
    trophyId: string?,
    turnIndex: float?,
    author: string?,
    body: string?,
}

class IssueCreate {
    kind: string,
    title: string,
    description: string,
    suggestion: string,
    category: string,
    evidence: IssueEvidence[],
    repro: string?,
    bamlVersion: string?, // env of the (first) run behind the issue
    skillUsed: string?,
    skillVersion: string?,
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

            // Human transcript comments ride along with the trophy batch: claim up
            // to 20 queued ones, feed them to the LLM as reviewer findings, and
            // mark them done with the batch (a crashed run leaves them claimed for
            // the reaper, same as trophies).
            let comments: CommentRow[] = [];
            let comment_ids: string[] = [];
            let c_gathering = true;
            while (c_gathering && comments.length() < 20) {
                let more_c = self
                    .svc
                    .claim(
                        "transcriptComments",
                        self.wid,
                        1800000,
                        "queued",
                        "deduping",
                        "status",
                        "by_status_created",
                    ) catch (_) {
                    _ => null
                };
                match (more_c) {
                    null => {
                        c_gathering = false;
                    },
                    let m: string => {
                        let parsed_c = baml.json.from_string<CommentRow>(m) catch (_) {
                            _ => null
                        };
                        match (parsed_c) {
                            let cr: CommentRow => {
                                comments.push(cr);
                                match (cr._id) {
                                    let cid: string => {
                                        comment_ids.push(cid);
                                    },
                                    null => null,
                                };
                            },
                            null => null,
                        };
                    },
                };
            }

            // trophyId -> run environment, for stamping issues/evidence.
            let envs = env_map_of(batch);

            let reports = render_reports(batch) + render_comments(comments);
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
                    self.upsert(iss, now, envs);
                }
            }

            // Mark the whole batch (trophies + comments) done.
            for (let b in batch) {
                let id = trophy_id_of(b);
                let _ = self.svc.transition("trophies", id, "done", "status", "{}", true) catch (_) {
                    _ => ""
                };
            }
            for (let cid in comment_ids) {
                let _ = self.svc.transition("transcriptComments", cid, "done", "status", "{}", true) catch (_) {
                    _ => ""
                };
            }
            return 0;
        }
    }

    // Class method (outside the implements block): create OR extend one issue. When
    // the Dedup LLM matched the finding to an open issue it carries that issue's
    // `id` -> extend it; otherwise it's genuinely new -> create.
    function upsert(self, iss: DedupIssue, now: int, envs: map<string, RunEnv>) -> int throws ProcessError {
        let existing = match (iss.id) {
            let v: string => v,
            null => "",
        };
        if (existing != "") {
            return self.extend(existing, iss, now, envs);
        }
        return self.create_doc(iss, now, envs);
    }

    // Evidence entries from a finding (skips entries with no report id), each
    // stamped with the environment of the run it came from.
    function evidence_of(self, iss: DedupIssue, envs: map<string, RunEnv>) -> IssueEvidence[] {
        let evidence: IssueEvidence[] = [];
        for (let e in iss.evidence) {
            if (e.report_id != "") {
                let renv = env_of(envs, e.report_id);
                evidence
                    .push(
                        IssueEvidence {
                            trophyId: e.report_id,
                            call_index: e.call_index,
                            turn_index: e.turn_index,
                            bamlVersion: renv.bamlVersion,
                            skillVersion: renv.skillVersion,
                        },
                    );
            }
        }
        return evidence;
    }

    function create_doc(self, iss: DedupIssue, now: int, envs: map<string, RunEnv>) -> int throws ProcessError {
        let evidence = self.evidence_of(iss, envs);
        let renv = first_env(evidence, envs);
        let doc = IssueCreate {
            kind: iss.kind,
            title: iss.title,
            description: iss.description,
            suggestion: iss.suggestion,
            category: iss.category,
            evidence: evidence,
            repro: iss.repro,
            bamlVersion: renv.bamlVersion,
            skillUsed: renv.skillUsed,
            skillVersion: renv.skillVersion,
            status: "open",
            linearSyncStatus: "dirty",
            firstSeenAt: now,
            lastSeenAt: now,
        };
        let doc_json = baml.json.to_string(doc) catch (_) {
            _ => ""
        };
        if (doc_json == "") {
            throw ProcessError { message: "serialize issue failed" };
        }
        // Fail LOUD: a swallowed create silently drops the finding (the trophy
        // still goes done). A throw fails the batch -> reaper requeues it.
        let _ = self.svc.create("issues", doc_json) catch (e) {
            _ => {
                throw ProcessError { message: "create issue failed: " + baml.unstable.string(e) }
            }
        };
        return 0;
    }

    // Extend a matched open issue: append this batch's evidence and bump lastSeenAt.
    // If the issue vanished (e.g. rejected mid-batch) fall back to a fresh create so
    // the finding isn't silently dropped.
    function extend(self, id: string, iss: DedupIssue, now: int, envs: map<string, RunEnv>) -> int throws ProcessError {
        let raw = self.svc.get("issues", id) catch (_) {
            _ => null
        };
        let cur_raw = match (raw) {
            let r: string => r,
            null => "",
        };
        if (cur_raw == "") {
            return self.create_doc(iss, now, envs);
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
        let fresh = self.evidence_of(iss, envs);
        for (let e in fresh) {
            let j = baml.json.parse(baml.json.to_string(e) catch (_) { _ => "null" }) catch (_) {
                _ => 0
            };
            merged.push(j);
        }
        // Build the patch as a map so absent fields are OMITTED — a serialized
        // null in a Convex patch overwrites the stored value (would clobber an
        // existing repro/env whenever this batch lacks one).
        let patch_map: map<string, json> = {};
        patch_map.set("evidence", merged);
        patch_map.set("lastSeenAt", now);
        patch_map.set("linearSyncStatus", "dirty");
        if (iss.description != "") {
            patch_map.set("description", iss.description);
        }
        if (iss.suggestion != "") {
            patch_map.set("suggestion", iss.suggestion);
        }
        match (iss.repro) {
            let rp: string => {
                patch_map.set("repro", rp);
            },
            null => null,
        };
        // Last-seen-on environment: update only when this batch knows it.
        let renv = first_env(fresh, envs);
        match (renv.bamlVersion) {
            let bv: string => {
                patch_map.set("bamlVersion", bv);
            },
            null => null,
        };
        match (renv.skillUsed) {
            let su: string => {
                patch_map.set("skillUsed", su);
            },
            null => null,
        };
        match (renv.skillVersion) {
            let sv: string => {
                patch_map.set("skillVersion", sv);
            },
            null => null,
        };
        let patch = baml.json.to_string(patch_map) catch (_) {
            _ => ""
        };
        if (patch == "") {
            throw ProcessError { message: "serialize extend patch failed" };
        }
        // Fail LOUD (this exact failure was silently eaten when the schema lacked
        // the env fields — the evidence merge vanished while trophies went done).
        let _ = self.svc.update("issues", id, patch) catch (e) {
            _ => {
                throw ProcessError { message: "extend issue failed: " + baml.unstable.string(e) }
            }
        };
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

// trophyId -> environment (baml version + skill) for every trophy in the batch.
function env_map_of(batch: string[]) -> map<string, RunEnv> {
    let envs: map<string, RunEnv> = {};
    for (let raw in batch) {
        let t = baml.json.from_string<Trophy>(raw) catch (_) {
            _ => null
        };
        match (t) {
            let tr: Trophy => {
                match (tr._id) {
                    let id: string => {
                        envs
                            .set(
                                id,
                                RunEnv {
                                    bamlVersion: tr.bamlVersion,
                                    skillUsed: tr.skillUsed,
                                    skillVersion: tr.skillVersion,
                                },
                            );
                    },
                    null => null,
                };
            },
            null => null,
        };
    }
    return envs;
}

function env_of(envs: map<string, RunEnv>, trophy_id: string) -> RunEnv {
    return match (envs.get(trophy_id)) {
        let e: RunEnv => e,
        null => RunEnv { bamlVersion: null, skillUsed: null, skillVersion: null },
    };
}

// The environment of the first evidence entry the batch knows about.
function first_env(evidence: IssueEvidence[], envs: map<string, RunEnv>) -> RunEnv {
    for (let e in evidence) {
        let renv = env_of(envs, e.trophyId);
        match (renv.bamlVersion) {
            let _v: string => {
                return renv;
            },
            null => null,
        };
    }
    return RunEnv { bamlVersion: null, skillUsed: null, skillVersion: null };
}

// Render claimed transcript comments as a reviewer-findings section appended to
// reports.md. "" when there are none.
function render_comments(comments: CommentRow[]) -> string {
    if (comments.length() == 0) {
        return "";
    }
    let lines: string[] = [];
    for (let c in comments) {
        let author = match (c.author) {
            let a: string => a,
            null => "anonymous",
        };
        let trophy = match (c.trophyId) {
            let t: string => t,
            null => "unknown",
        };
        let turn = match (c.turnIndex) {
            let t: float => " (turn " + baml.unstable.string(f2i(t)) + ")",
            null => "",
        };
        let body = match (c.body) {
            let b: string => b,
            null => "",
        };
        lines.push("- [" + author + "] on run " + trophy + turn + ": " + body);
    }
    return "\n\n# human transcript comments\nHuman reviewers left these directly on run transcripts. Treat each as a finding from a trusted reviewer: dedup it against the reports and open issues like any other finding. For evidence, cite the run id shown here as `report_id` (and its turn number as `turn_index` when given).\n"
        + lines.join("\n")
        + "\n";
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
