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
    kind: string?,
    category: string?,
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
                [],
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
                "",
                match (issue.prUrl) {
                    let v: string => v,
                    null => "",
                },
            );
            let approved_label = linear_status_id("NOT_STARTED");

            // Create on first sync; otherwise update in place.
            match (issue.linearIssueId) {
                let lid: string => {
                    let _ = self.linear.set_status(lid, approved_label) catch (_) {
                        _ => 0
                    };
                },
                null => {
                    let new_id = self.linear.create_issue(title, approved_label, body) catch (_) {
                        _ => ""
                    };
                    if (new_id != "") {
                        let patch = baml.json.to_string(LinearIdPatch { linearIssueId: new_id }) catch (_) {
                            _ => "{}"
                        };
                        let _ = self.svc.update("issues", id, patch) catch (_) {
                            _ => ""
                        };
                    }
                },
            };

            // Mark synced on the SAME field we claimed.
            let _ = self.svc.transition("issues", id, "synced", "linearSyncStatus", "{}", true) catch (e) {
                _ => {
                    throw ProcessError { message: "mark synced failed: " + baml.unstable.string(e) }
                }
            };
            return 0;
        }
    }
}

// ---- FixDispatch -----------------------------------------------------------

class FixDispatchProcessor {
    wid: string,
    svc: ServiceClient,
    cursor_key: string,
    cursor_branch: string,
    repo_url: string,

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
                self.repo_url,
                self.cursor_branch,
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
                + self.repo_url
                + " @ "
                + self.cursor_branch;
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
            return 0;
        }
    }
}

// Mint a Cursor-valid agent id (`bc-<uuid>`) from the kernel UUID source. BAML has
// no uuid stdlib + Math.random is unavailable, so we read /proc/sys/kernel/random/uuid.
// Returns "" if unavailable (caller then lets Cursor assign the id).
function mint_agent_id() -> string {
    let res = run_capture("cat", ["/proc/sys/kernel/random/uuid"], "/tmp", null, 5000, null, false);
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
