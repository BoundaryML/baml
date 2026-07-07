// BAML dedup processor — batches queued trophies and runs the classify/dedup
// LLM call, upserting the issues DB. Ports services/baml_dedup. The headline win:
// where Python ran an AGENT that wrote issues.json, this calls the typed Dedup()
// function directly — no proxy, no JSON-parse glue.
//
// Cross-run dedup hands the LLM the open board (id/kind/category/title/snippet)
// plus the newest closed issues, so a re-found finding extends (or reopens) the
// existing issue instead of spawning a near-duplicate. Human transcript comments
// are handled by the separate CommentDedupProcessor (comment_dedup.baml), which
// shares the dedup_upsert path below.

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
    brokeIn: string?, // baml version of the first evidence run
    status: string,
    linearSyncStatus: string,
    firstSeenAt: int,
    lastSeenAt: int,
}

// One board issue, as read for the Dedup LLM's match context. Extra doc fields
// are ignored by the lenient decode; description feeds the snippet.
class OpenIssueRef {
    _id: string?,
    kind: string?,
    category: string?,
    title: string?,
    description: string?,
}

// What the Dedup prompt sees per matchable issue. `state` is "open" or
// "closed" — a closed match means the bug re-appeared (extend => reopen).
class MatchIssueOut {
    id: string,
    kind: string,
    category: string,
    title: string,
    snippet: string,
    state: string,
}

// Minimal read of an existing issue's evidence + lifecycle state (to append to,
// not replace). Evidence is read as raw json[] — Convex serializes every number
// as a float, so a typed read with `call_index: int?` would fail to parse the
// stored 0.0 and silently drop ALL the existing evidence. json[] round-trips it.
class IssueEvidenceRead {
    evidence: json[]?,
    status: string?,
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

            // trophyId -> run environment, for stamping issues/evidence.
            let envs = env_map_of(batch);
            let reports = render_reports(batch);

            // Real cross-run dedup context. Fail-LOUD: an api hiccup swallowed to
            // "[]" would make every finding look brand-new and burst duplicates
            // across the whole batch. Release the extra claims before rethrowing so
            // only the seed eats the failure.
            let match_json = fetch_match_context(self.svc) catch (e) {
                _ => {
                    release_trophies(self.svc, batch, raw);
                    throw ProcessError { message: "fetch match context failed: " + baml.unstable.string(e) }
                }
            };

            let result = Dedup(reports, match_json) catch (e) {
                _ => {
                    release_trophies(self.svc, batch, raw);
                    throw ProcessError { message: "Dedup failed: " + baml.unstable.string(e) }
                }
            };

            let now = baml.sys.now_ms();
            for (let iss in result.issues) {
                if (iss.kind == "skill" || iss.kind == "language") {
                    let _ = dedup_upsert(self.svc, iss, now, envs);
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
}

// Release every batch member EXCEPT the seed back to `queued` (best-effort).
// Called before a batch-level rethrow: the base loop fails only the seed row, so
// without this the extra claims would sit in `deduping` until the lease reaper,
// each stranding burning one of their max attempts.
function release_trophies(svc: ServiceClient, batch: string[], seed: string) -> int {
    for (let b in batch) {
        if (b != seed) {
            let id = trophy_id_of(b);
            if (id != "") {
                let _ = svc.transition("trophies", id, "queued", "status", "{}", true) catch (_) {
                    _ => ""
                };
            }
        }
    }
    return 0;
}

// ---- shared upsert path (used by DedupProcessor + CommentDedupProcessor) ----

// Create OR extend one issue. When the Dedup LLM matched the finding to a board
// issue it carries that issue's `id` -> extend it; otherwise it's genuinely new.
function dedup_upsert(svc: ServiceClient, iss: DedupIssue, now: int, envs: map<string, RunEnv>) -> int throws ProcessError {
    let existing = match (iss.id) {
        let v: string => v,
        null => "",
    };
    if (existing != "") {
        return dedup_extend(svc, existing, iss, now, envs);
    }
    return dedup_create_doc(svc, iss, now, envs);
}

// Evidence entries from a finding (skips entries with no report id), each
// stamped with the environment of the run it came from.
function dedup_evidence_of(iss: DedupIssue, envs: map<string, RunEnv>) -> IssueEvidence[] {
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

function dedup_create_doc(svc: ServiceClient, iss: DedupIssue, now: int, envs: map<string, RunEnv>) -> int throws ProcessError {
    let evidence = dedup_evidence_of(iss, envs);
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
        brokeIn: renv.bamlVersion,
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
    let _ = svc.create("issues", doc_json) catch (e) {
        _ => {
            throw ProcessError { message: "create issue failed: " + baml.unstable.string(e) }
        }
    };
    return 0;
}

// Extend a matched issue: append this batch's evidence and bump lastSeenAt. A
// closed/rejected match means the bug re-appeared -> reopen it. If the issue
// vanished entirely fall back to a fresh create so the finding isn't dropped.
function dedup_extend(svc: ServiceClient, id: string, iss: DedupIssue, now: int, envs: map<string, RunEnv>) -> int throws ProcessError {
    let raw = svc.get("issues", id) catch (_) {
        _ => null
    };
    let cur_raw = match (raw) {
        let r: string => r,
        null => "",
    };
    if (cur_raw == "") {
        return dedup_create_doc(svc, iss, now, envs);
    }
    let cur = baml.json.from_string<IssueEvidenceRead>(cur_raw) catch (_) {
        _ => IssueEvidenceRead { evidence: null, status: null }
    };
    // Keep the existing evidence verbatim (json[]) and append this batch's new
    // entries (each typed entry round-tripped through json so the array stays
    // homogeneous and Convex-float-safe).
    let merged: json[] = match (cur.evidence) {
        let ev: json[] => ev,
        null => [],
    };
    let fresh = dedup_evidence_of(iss, envs);
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
    // Re-found on a closed/rejected issue: the bug is back — reopen the same
    // card rather than duplicating it. Clearing fixedIn uses "" (a serialized
    // null in a patch is stripped, so null would be a no-op).
    let cur_status = match (cur.status) {
        let s: string => s,
        null => "",
    };
    if (cur_status == "closed" || cur_status == "rejected") {
        patch_map.set("status", "open");
        patch_map.set("fixedIn", "");
    }
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
    let _ = svc.update("issues", id, patch) catch (e) {
        _ => {
            throw ProcessError { message: "extend issue failed: " + baml.unstable.string(e) }
        }
    };
    return 0;
}

// Statuses an issue counts as open/in-flight for cross-run dedup matching (every
// non-terminal state; closed/rejected are terminal and handled separately).
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
        "reverify",
        "reverifying",
        "redraft",
        "redrafting",
        "needs_human",
    ];
}

// One-line match snippet from an issue description: newlines collapsed,
// truncated to `max` chars.
function snippet_of(desc: string, max: int) -> string {
    let flat = desc.split("\n").join(" ").trim();
    if (flat.length() <= max) {
        return flat;
    }
    return flat.substring(0, max);
}

// Fetch the board as the {id, kind, category, title, snippet, state} JSON the
// Dedup prompt matches against: every open/in-flight issue (capped per status)
// plus the 30 newest closed ones (a re-found closed issue reopens instead of
// duplicating). THROWS on any list failure — a silent "[]" here is what turned
// one api hiccup into a batch-wide duplicate burst.
function fetch_match_context(svc: ServiceClient) -> string throws ProcessError {
    let outs: MatchIssueOut[] = [];
    for (let status in dedup_open_statuses()) {
        if (outs.length() < 150) {
            let raw = svc.list("issues", "status", status, "by_status_created", 40) catch (e) {
                _ => {
                    throw ProcessError { message: "list issues (" + status + ") failed: " + baml.unstable.string(e) }
                }
            };
            for (let m in parse_match_outs(raw, "open")) {
                outs.push(m);
            }
        }
    }
    let closed_raw = svc.list("issues", "status", "closed", "by_status_created", 30) catch (e) {
        _ => {
            throw ProcessError { message: "list closed issues failed: " + baml.unstable.string(e) }
        }
    };
    for (let m in parse_match_outs(closed_raw, "closed")) {
        outs.push(m);
    }
    return baml.json.to_string(outs) catch (_) {
        _ => "[]"
    };
}

// Decode one list() payload into match entries tagged with `state`.
function parse_match_outs(raw: string, state: string) -> MatchIssueOut[] {
    let outs: MatchIssueOut[] = [];
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
                    MatchIssueOut {
                        id: id,
                        kind: match (iss.kind) {
                            let k: string => k,
                            null => "",
                        },
                        category: match (iss.category) {
                            let c: string => c,
                            null => "",
                        },
                        title: match (iss.title) {
                            let t: string => t,
                            null => "",
                        },
                        snippet: snippet_of(
                            match (iss.description) {
                                let d: string => d,
                                null => "",
                            },
                            160,
                        ),
                        state: state,
                    },
                );
        }
    }
    return outs;
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
