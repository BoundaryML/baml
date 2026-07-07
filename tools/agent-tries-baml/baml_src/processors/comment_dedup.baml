// Comment-dedup processor — a claim loop seeded by transcriptComments, so a
// human comment on a run transcript triggers issue creation on its own (the
// trophy-seeded DedupProcessor previously only picked comments up as ride-alongs
// when an unrelated trophy happened to be queued — a lone comment starved).
//
// The commented runs' trophies are fetched as CONTEXT (their findings were
// already deduped in an earlier batch); only the comments are new input. Shares
// the Dedup() call and the dedup_upsert path with DedupProcessor.

// A claimed transcript comment (Convex doc; numbers are floats).
class CommentRow {
    _id: string?,
    trophyId: string?,
    turnIndex: float?,
    author: string?,
    body: string?,
}

class CommentDedupProcessor {
    wid: string,
    svc: ServiceClient,
    batch_size: int,

    implements Processor {
        function config(self) -> ProcessorConfig {
            return ProcessorConfig {
                role: "comment_dedup",
                table: "transcriptComments",
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
                match (more) {
                    null => {
                        gathering = false;
                    },
                    let m: string => {
                        batch.push(m);
                    },
                };
            }

            let comments: CommentRow[] = [];
            for (let b in batch) {
                let parsed = baml.json.from_string<CommentRow>(b) catch (_) {
                    _ => null
                };
                match (parsed) {
                    let cr: CommentRow => {
                        comments.push(cr);
                    },
                    null => null,
                };
            }

            // Fetch the commented runs' trophies as context. A missing/unreadable
            // trophy is skipped (the comment still dedups with less context) — a
            // dead trophyId must not poison-pill the comment queue.
            let trophy_raws: string[] = [];
            for (let tid in distinct_trophy_ids(comments)) {
                let t = self.svc.get("trophies", tid) catch (_) {
                    _ => null
                };
                match (t) {
                    let tr: string => {
                        trophy_raws.push(tr);
                    },
                    null => null,
                };
            }

            let envs = env_map_of(trophy_raws);
            let context = if (trophy_raws.length() > 0) {
                "# context runs\nThese runs were already processed in an earlier batch — do NOT re-file their findings. They are included only so the human comments below make sense.\n\n"
                    + render_reports(trophy_raws)
            } else {
                ""
            };
            let reports = context + render_comments(comments);

            // Fail-LOUD on match-context fetch (same reasoning as DedupProcessor);
            // release the extra claims so only the seed eats the failure.
            let match_json = fetch_match_context(self.svc) catch (e) {
                _ => {
                    release_comments(self.svc, batch, raw);
                    throw ProcessError { message: "fetch match context failed: " + baml.unstable.string(e) }
                }
            };

            let result = Dedup(reports, match_json) catch (e) {
                _ => {
                    release_comments(self.svc, batch, raw);
                    throw ProcessError { message: "Dedup failed: " + baml.unstable.string(e) }
                }
            };

            let now = baml.sys.now_ms();
            for (let iss in result.issues) {
                if (iss.kind == "skill" || iss.kind == "language") {
                    let _ = dedup_upsert(self.svc, iss, now, envs);
                }
            }

            // Mark the whole comment batch done.
            for (let b in batch) {
                let id = comment_id_of(b);
                if (id != "") {
                    let _ = self.svc.transition("transcriptComments", id, "done", "status", "{}", true) catch (_) {
                        _ => ""
                    };
                }
            }
            return 0;
        }
    }
}

// Release every batch member EXCEPT the seed back to `queued` (best-effort).
function release_comments(svc: ServiceClient, batch: string[], seed: string) -> int {
    for (let b in batch) {
        if (b != seed) {
            let id = comment_id_of(b);
            if (id != "") {
                let _ = svc.transition("transcriptComments", id, "queued", "status", "{}", true) catch (_) {
                    _ => ""
                };
            }
        }
    }
    return 0;
}

function comment_id_of(raw: string) -> string {
    let parsed = baml.json.from_string<DocId>(raw) catch (_) {
        _ => DocId { _id: null }
    };
    return match (parsed._id) {
        let v: string => v,
        null => "",
    };
}

// The distinct trophy ids a comment batch references, in first-seen order.
function distinct_trophy_ids(comments: CommentRow[]) -> string[] {
    let seen: map<string, bool> = {};
    let out: string[] = [];
    for (let c in comments) {
        match (c.trophyId) {
            let tid: string => {
                if (tid != "") {
                    match (seen.get(tid)) {
                        let _b: bool => null,
                        null => {
                            seen.set(tid, true);
                            out.push(tid);
                        },
                    };
                }
            },
            null => null,
        };
    }
    return out;
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
