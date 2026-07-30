// linear-sync — two processors over the issues table's two independent queues.
// Ports services/linear_sync.
//   LinearPush:   claims `linearSyncStatus` dirty -> syncing, mirrors the issue to
//                 a Linear card (create or update), marks synced. Transitions on
//                 the SAME field it claimed (fail-on-same-field) so it never
//                 corrupts the lifecycle `status`.
//   FixDispatch:  claims `status` approved -> dispatching, launches a Cursor fix
//                 agent, stamps cursorAgentId, moves the card to to-cursor.
//
// Focused port: the full cursor-tracker (drive a PR to green via CI/CodeRabbit)
// is a later pass; dispatch + card mirroring are complete.

class SyncIssue {
    _id: string?,
    title: string?,
    status: string?, // lifecycle state; picks the mirrored Linear status label
    kind: string?,
    category: string?,
    evidence: json[]?, // {trophyId, turn_index?, call_index?, bamlVersion?} entries -> run links
    bamlVersion: string?, // env of the run(s) behind the issue (dedup-stamped)
    skillUsed: string?,
    skillVersion: string?,
    description: string?,
    suggestion: string?,
    repro: string?,
    linearIssueId: string?,
    prUrl: string?,
    fixSlackTs: string?, // dispatch ref / "pending:<id>" idempotency marker
    fixAttempts: float?, // Convex serializes numbers as floats
}

class LinearIdPatch {
    linearIssueId: string,
}

class CursorPatch {
    cursorAgentId: string,
}

// ---- LinearPush ------------------------------------------------------------

class LinearPushProcessor {
    wid: string,
    svc: ServiceClient,
    linear: LinearClient,

    implements Processor {
        function config(self) -> ProcessorConfig {
            return ProcessorConfig {
                role: "linear_push",
                table: "issues",
                claim_value: "dirty",
                claim_into: "syncing",
                claim_field: "linearSyncStatus",
                claim_index: "by_linear_sync",
                lease_ms: 600000,
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
            let issue = baml.json.from_string<SyncIssue>(raw) catch (_) {
                _ => {
                    throw ProcessError { message: "bad issue json" }
                }
            };
            let id = match (issue._id) {
                let v: string => v,
                null => {
                    throw ProcessError { message: "issue missing _id" }
                },
            };

            // No Linear token -> no-op sync (ports the v1 token-less client): mark
            // synced and still run the open->confirmed boarding so the pipeline
            // flows in dev. Without this guard the fail-loud path below would
            // retry-loop every dirty issue against an unauthenticated API forever.
            if (self.linear.token == "") {
                let _ = self.svc.transition("issues", id, "synced", "linearSyncStatus", "{}", true) catch (e) {
                    _ => {
                        throw ProcessError { message: "tokenless mark synced failed: " + baml.unstable.string(e) }
                    }
                };
                let was_open_nl = match (issue.status) {
                    let s: string => s == "open",
                    null => false,
                };
                if (was_open_nl) {
                    let _ = self.svc.transition("issues", id, "confirmed", "status", "{}", false) catch (_) {
                        _ => ""
                    };
                }
                return 0;
            }

            // Containment: with an allowlist set, mirror ONLY listed issues; drain
            // every other dirty issue (mark synced) without writing to the board.
            let allow = linear_push_allowlist();
            if (allow.length() > 0 && in_list(allow, id) == false) {
                let _ = self.svc.transition("issues", id, "synced", "linearSyncStatus", "{}", true) catch (e) {
                    _ => {
                        throw ProcessError { message: "contained-skip mark synced failed: " + baml.unstable.string(e) }
                    }
                };
                baml.io
                    .println(
                        "linear_push: issue " + id + " not in allowlist; drained without a card",
                    );
                return 0;
            }

            let title = match (issue.title) {
                let v: string => v,
                null => "(untitled)",
            };
            let body = issue_body_md(
                match (issue.description) {
                    let v: string => v,
                    null => "",
                },
                evidence_links_of(issue.evidence),
                env_line_of(issue),
                match (issue.suggestion) {
                    let v: string => v,
                    null => "",
                },
                match (issue.category) {
                    let v: string => v,
                    null => "",
                },
                match (issue.repro) {
                    let v: string => v,
                    null => "",
                },
                ui_base_url() + "/issues/" + id,
                match (issue.prUrl) {
                    let v: string => v,
                    null => "",
                },
            );
            // Mirror the label matching the issue's CURRENT lifecycle state — NOT a
            // hardcoded not-started, which dragged every re-synced card (post-dispatch,
            // redraft, etc.) back to the start.
            let status_label = label_for_issue_status(
                match (issue.status) {
                    let s: string => s,
                    null => "open",
                },
            );

            // Resolve the target card: recorded linearIssueId -> adopt an existing
            // same-title card (the hand-imported board / a create whose id-recording
            // failed) -> create fresh. Linear API failures are NOT swallowed: they
            // throw ProcessError, fail_item marks linearSyncStatus failed, and the
            // reaper requeues to dirty — the old retry semantics. Marking synced on
            // a failed write would lose the card forever.
            //
            // Self-heal: a recorded id whose card no longer exists (deleted on
            // Linear) previously retry-looped forever; now the missing-entity error
            // clears the dead pointer and falls through to adopt/create.
            let recorded = match (issue.linearIssueId) {
                let lid: string => lid,
                null => "",
            };
            let wrote = if (recorded != "") {
                self.try_update_card(recorded, title, body, status_label)
            } else {
                false
            };
            if (wrote == false) {
                if (recorded != "") {
                    let _ = self.svc.update("issues", id, "{\"linearIssueId\":\"\"}") catch (_) {
                        _ => ""
                    };
                    baml.io
                        .println(
                            "linear_push: issue " + id + " had a dead card " + recorded + "; re-resolving",
                        );
                }
                let target = self.linear.find_issue_by_title(title) catch (e) {
                    _ => {
                        throw ProcessError { message: "linear find_issue_by_title failed: " + baml.unstable.string(e) }
                    }
                };
                if (target != "") {
                    let _ = self.linear.update_card(target, title, body, status_label) catch (e) {
                        _ => {
                            throw ProcessError { message: "linear update_card failed: " + baml.unstable.string(e) }
                        }
                    };
                    self.record_linear_id(id, target);
                } else {
                    let new_id = self.linear.create_issue(title, status_label, body) catch (e) {
                        _ => {
                            throw ProcessError { message: "linear create_issue failed: " + baml.unstable.string(e) }
                        }
                    };
                    self.record_linear_id(id, new_id);
                }
            }

            // Stamp what we just pushed (two-way-sync loop guard): a webhook whose
            // title/body matches these is our own echo; a mismatch is a human edit
            // to flow back into Convex. Fail-loud — an unstamped push would let the
            // next human-edit check misread our own render as an edit.
            self.stamp_pushed(id, title, body);

            // Mark synced on the SAME field we claimed.
            let _ = self.svc.transition("issues", id, "synced", "linearSyncStatus", "{}", true) catch (e) {
                _ => {
                    throw ProcessError { message: "mark synced failed: " + baml.unstable.string(e) }
                }
            };

            // Boarding confirmation (ports _confirm): a freshly-opened issue that
            // just landed on the board advances open -> confirmed. Claim is on
            // linearSyncStatus, so don't release on this status transition.
            let was_open = match (issue.status) {
                let s: string => s == "open",
                null => false,
            };
            if (was_open) {
                let _ = self.svc.transition("issues", id, "confirmed", "status", "{}", false) catch (e) {
                    _ => {
                        throw ProcessError { message: "confirm open->confirmed failed: " + baml.unstable.string(e) }
                    }
                };
            }
            return 0;
        }
    }

    // Update the recorded card. True on success; false when the card no longer
    // exists (dead pointer — caller re-resolves); throws on any other failure.
    function try_update_card(self, target: string, title: string, body: string, status_label: string) -> bool throws ProcessError {
        let ok = true;
        let _ = self.linear.update_card(target, title, body, status_label) catch (e) {
            _ => {
                let msg = baml.unstable.string(e);
                if (is_missing_entity(msg) == false) {
                    throw ProcessError { message: "linear update_card failed: " + msg };
                }
                ok = false;
                0
            }
        };
        return ok;
    }

    // Record what this push rendered onto the card, so the ingress webhook can
    // tell our own echo from a human edit (see sync_card_edit in ingress.baml).
    function stamp_pushed(self, id: string, title: string, body: string) -> int throws ProcessError {
        let patch = "{\"lastPushedTitle\":"
            + jesc(title)
            + ",\"lastPushedBodyHash\":"
            + jesc(body_hash(body))
            + "}";
        let _ = self.svc.update("issues", id, patch) catch (e) {
            _ => {
                throw ProcessError { message: "stamp pushed failed: " + baml.unstable.string(e) }
            }
        };
        return 0;
    }

    // Persist the resolved Linear card id; a failure here throws so the reaper
    // retries — adopt-by-title makes the retry idempotent (no duplicate card).
    function record_linear_id(self, id: string, linear_id: string) -> int throws ProcessError {
        let patch = baml.json.to_string(LinearIdPatch { linearIssueId: linear_id }) catch (_) {
            _ => "{}"
        };
        let _ = self.svc.update("issues", id, patch) catch (e) {
            _ => {
                throw ProcessError { message: "recording linearIssueId failed: " + baml.unstable.string(e) }
            }
        };
        return 0;
    }
}

// Evidence entries ({trophyId, turn_index?, ...} as raw json — Convex floats) ->
// dashboard run links. Shape matches the v2 website: /runs/<trophyId>#turn-<N>
// (hash anchor; the site has no ?call= route). Entries without a trophyId are
// skipped; without a turn_index the link just has no anchor.
class EvidenceRef {
    trophyId: string?,
    turn_index: float?,
    bamlVersion: string?,
}

// The card's Environment line: exactly which baml + skill produced the issue.
// "" when dedup didn't know (pre-feature issues) — issue_body_md omits it.
function env_line_of(issue: SyncIssue) -> string {
    let parts: string[] = [];
    match (issue.bamlVersion) {
        let bv: string => {
            if (bv != "") {
                parts.push("baml `" + bv + "`");
            }
        },
        null => null,
    };
    let repo = match (issue.skillUsed) {
        let r: string => r,
        null => "",
    };
    let sha = match (issue.skillVersion) {
        let v: string => v,
        null => "",
    };
    if (repo != "" || sha != "") {
        let label = if (repo != "" && sha != "") {
            repo + "@" + sha
        } else {
            repo + sha
        };
        parts.push("skill `" + label + "`");
    }
    return parts.join(" · ");
}

function evidence_links_of(evidence: json[]?) -> string[] {
    let out: string[] = [];
    let entries: json[] = match (evidence) {
        let e: json[] => e,
        null => [],
    };
    for (let e in entries) {
        let raw = baml.json.stringify(e) catch (_) {
            _ => "{}"
        };
        let ref = baml.json.from_string<EvidenceRef>(raw) catch (_) {
            _ => EvidenceRef { trophyId: null, turn_index: null }
        };
        match (ref.trophyId) {
            let tid: string => {
                if (tid != "") {
                    let anchor = match (ref.turn_index) {
                        let t: float => "#turn-" + baml.unstable.string(f2i(t)),
                        null => "",
                    };
                    let suffix = match (ref.bamlVersion) {
                        let bv: string => {
                            if (bv != "") {
                                "|baml " + bv
                            } else {
                                ""
                            }
                        },
                        null => "",
                    };
                    out.push(ui_base_url() + "/runs/" + tid + anchor + suffix);
                }
            },
            null => null,
        };
    }
    return out;
}

// ---- FixDispatch -----------------------------------------------------------

class FixDispatchProcessor {
    wid: string,
    svc: ServiceClient,
    cursor_key: string,

    implements Processor {
        function config(self) -> ProcessorConfig {
            return ProcessorConfig {
                role: "fix_dispatch",
                table: "issues",
                claim_value: "approved",
                claim_into: "dispatching",
                claim_field: "status",
                claim_index: "by_status_created",
                lease_ms: 600000,
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
            let issue = baml.json.from_string<SyncIssue>(raw) catch (_) {
                _ => {
                    throw ProcessError { message: "bad issue json" }
                }
            };
            let id = match (issue._id) {
                let v: string => v,
                null => {
                    throw ProcessError { message: "issue missing _id" }
                },
            };
            if (self.cursor_key == "") {
                throw ProcessError { message: "no cursor key configured; cannot dispatch" };
            }
            // Route the fix to the repo that owns this issue's kind: skill issues ->
            // the baml-skill repo, language issues -> the baml language repo (each on
            // its own default branch). No hard-pinned target.
            let kind = match (issue.kind) {
                let k: string => k,
                null => "skill",
            };
            let repo = cursor_repo_for_kind(kind);
            let branch = cursor_branch_for_kind(kind);

            let existing = match (issue.fixSlackTs) {
                let v: string => v,
                null => "",
            };
            // Already dispatched (a real ref, not a pending marker): if that agent is
            // still alive (working or already opened a PR) just ensure tocursor — don't
            // launch a duplicate. A dead prior agent means a human re-approval, so clear
            // the stale ref and fall through to launch a genuinely fresh one.
            if (existing != "" && existing.starts_with("pending:") == false) {
                if (prior_dispatch_alive(self.cursor_key, existing)) {
                    let fa = match (issue.fixAttempts) {
                        let n: float => f2i(n),
                        null => 0,
                    };
                    let p = "{\"cursorAgentId\":"
                        + jesc(existing)
                        + ",\"fixAttempts\":"
                        + baml.unstable.string(fa)
                        + "}";
                    let _ = self.svc.transition("issues", id, "tocursor", "status", p, true) catch (_) {
                        _ => ""
                    };
                    let _ = dispatch_set_linear(issue.linearIssueId);
                    return 0;
                }
                let _ = self.svc.update("issues", id, "{\"fixSlackTs\":\"\"}") catch (_) {
                    _ => ""
                };
            }

            // The Cursor agent id must be `bc-<uuid>` and STABLE within a dispatch (so a
            // crash-retry 409s instead of spawning a duplicate) but FRESH across
            // re-approvals. Resume a pending marker if present; else mint + persist one
            // BEFORE launching. An empty id (uuid mint failed) lets Cursor assign one.
            let pending_id = if (existing.starts_with("pending:")) {
                existing.substring(8, existing.length()) catch (_) {
                    _ => ""
                }
            } else {
                ""
            };
            let agent_id = if (pending_id != "") {
                pending_id
            } else {
                mint_agent_id()
            };
            if (pending_id == "" && agent_id != "") {
                let _ = self
                    .svc
                    .update("issues", id, "{\"fixSlackTs\":" + jesc("pending:" + agent_id) + "}") catch (_) {
                    _ => ""
                };
            }

            let prompt = cursor_fix_prompt(issue);
            let resp = cursor_launch_agent(
                self.cursor_key,
                prompt,
                repo,
                branch,
                agent_id,
                cursor_model(),
                "",
                false,
            ) catch (_) {
                _ => "{}"
            };
            // Success = the response carries an agent "id" (or a 409 agent_id_conflict,
            // meaning our stable id already launched). Anything else is a real failure.
            let id_field = extract_field(resp, "id");
            let ref = if (id_field != "") {
                id_field
            } else if (resp.includes("agent_id_conflict") && agent_id != "") {
                agent_id
            } else {
                ""
            };
            if (ref == "") {
                // Clear the pending marker so a retry mints a clean id, then fail loud.
                let _ = self.svc.update("issues", id, "{\"fixSlackTs\":\"\"}") catch (_) {
                    _ => ""
                };
                throw ProcessError { message: "cursor launch failed: " + tail_str(resp, 200) };
            }

            // Post the launch note to the fix channel; its ts roots the per-issue fix
            // thread (the cursor-tracker threads every later update under it). No-op
            // when the bot token / SLACK_FIX_CHANNEL aren't set.
            let title = match (issue.title) {
                let t: string => t,
                null => "(issue)",
            };
            let note = "Started Cursor agent for *"
                + title
                + "*\n<https://cursor.com/agents/"
                + ref
                + "|view agent>  ·  "
                + repo
                + " @ "
                + branch;
            let thread_ts = slack_post_message(slack_bot_token(), slack_fix_channel(), note, "");
            let patch = "{\"cursorAgentId\":"
                + jesc(ref)
                + ",\"fixAttempts\":0,\"fixSlackTs\":"
                + jesc(ref)
                + ",\"fixThreadTs\":"
                + jesc(thread_ts)
                + "}";
            let _ = self.svc.transition("issues", id, "tocursor", "status", patch, true) catch (e) {
                _ => {
                    throw ProcessError { message: "dispatch transition failed: " + baml.unstable.string(e) }
                }
            };
            // Mirror the board immediately (ports v1 FixDispatch's best-effort
            // TO_CURSOR swap) — without this the card rests on `approved` until the
            // tracker's next transition, which reads as "nothing happened".
            let _ = dispatch_set_linear(issue.linearIssueId);
            return 0;
        }
    }
}

// Best-effort to-cursor label swap after a successful dispatch. No-ops without a
// Linear token / mirrored card; failures only log (a board hiccup must never
// fail a dispatch that already launched).
function dispatch_set_linear(linear_issue_id: string?) -> int {
    let tok = linear_token();
    if (tok == "") {
        return 0;
    }
    match (linear_issue_id) {
        let lid: string => {
            let lc = LinearClient { token: tok, team_id: linear_team_id() };
            let label = linear_status_id("TO_CURSOR");
            if (label != "") {
                let _ = lc.set_status(lid, label) catch (e) {
                    _ => {
                        baml.io
                            .eprintln(
                                "fix_dispatch: set to-cursor label failed for " + lid + ": " + baml.unstable.string(e),
                            );
                        0
                    }
                };
            }
        },
        null => null,
    };
    return 0;
}

// Mint a Cursor-valid agent id (`bc-<uuid>`) from the kernel UUID source. BAML has
// no uuid stdlib + Math.random is unavailable, so we read /proc/sys/kernel/random/uuid.
// Returns "" if unavailable (caller then lets Cursor assign the id).
function mint_agent_id() -> string {
    let res = run_capture("cat", ["/proc/sys/kernel/random/uuid"], "/tmp", null, 5000, null, false, null, null);
    let u = res.stdout.trim();
    if (u == "") {
        return "";
    }
    return "bc-" + u;
}

// Whether a prior fix agent is still worth keeping (vs. re-dispatching fresh): true
// when it is still working OR already opened a PR; false when terminal-with-no-PR or
// unreachable (so a human re-approval launches a brand-new agent).
function prior_dispatch_alive(cursor_key: string, ref: string) -> bool {
    return match (cursor_pr_for_agent(cursor_key, ref)) {
        null => false,
        let p: CursorPr => match (p.pr_url) {
            let u: string => true,
            null => p.run_terminal == false,
        },
    };
}

// The Cursor agent fix prompt, built from the issue.
function cursor_fix_prompt(issue: SyncIssue) -> string {
    let title = match (issue.title) {
        let v: string => v,
        null => "",
    };
    let desc = match (issue.description) {
        let v: string => v,
        null => "",
    };
    let repro = match (issue.repro) {
        let v: string => v,
        null => "(none)",
    };
    let suggestion = match (issue.suggestion) {
        let v: string => v,
        null => "",
    };
    return "Fix this BAML issue and open a PR.\n\n# Problem\n"
        + title
        + "\n\n"
        + desc
        + "\n\n# Reproduction\n"
        + repro
        + "\n\n# Suggested fix\n"
        + suggestion
        + "\n\nBuild, run the targeted tests, and request a CodeRabbit review on the PR.";
}
