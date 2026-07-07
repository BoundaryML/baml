// Redraft processor — claims issues a human moved to "redraft" on Linear, reads
// the reviewer's comments, and rewrites the issue via the Redraft() LLM function,
// then re-boards it (linearSyncStatus=dirty) and returns it to "confirmed". Ports
// services/baml_redraft. Single LLM call replaces the Python file-shuffling agent.

class RedraftPatch {
    title: string,
    kind: string,
    category: string,
    description: string,
    suggestion: string,
    linearSyncStatus: string,
}

class WorkerIssue {
    _id: string?,
    title: string?,
    kind: string?,
    category: string?,
    description: string?,
    suggestion: string?,
    repro: string?,
    linearIssueId: string?,
}

class RedraftProcessor {
    wid: string,
    svc: ServiceClient,
    linear: LinearClient,

    implements Processor {
        function config(self) -> ProcessorConfig {
            return ProcessorConfig {
                role: "baml_redraft",
                table: "issues",
                claim_value: "redraft",
                claim_into: "redrafting",
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
            let issue = baml.json.from_string<WorkerIssue>(raw) catch (_) {
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

            // Reviewer feedback = the Linear comments on the mirrored card.
            // Fail LOUD on a fetch error: redrafting with silently-empty feedback
            // blind-rewrites the issue the human just annotated (the reaper
            // requeues redrafting -> redraft for a retry).
            let feedback = match (issue.linearIssueId) {
                let lid: string => {
                    self.linear.get_comments(lid) catch (e) {
                        _ => {
                            throw ProcessError { message: "linear get_comments failed: " + baml.unstable.string(e) }
                        }
                    }
                },
                null => "",
            };

            // Genuinely no feedback (no card, or no comments on it): nothing to
            // redraft against — skip the LLM and re-board unchanged.
            if (feedback.trim() == "") {
                let _ = self.svc.transition("issues", id, "confirmed", "status", "{\"linearSyncStatus\":\"dirty\"}", true) catch (e) {
                    _ => {
                        throw ProcessError { message: "empty-feedback transition failed: " + baml.unstable.string(e) }
                    }
                };
                return 0;
            }

            let revised = Redraft(raw, feedback) catch (e) {
                _ => {
                    throw ProcessError { message: "Redraft failed: " + baml.unstable.string(e) }
                }
            };

            let patch = baml
                .json
                .to_string(
                    RedraftPatch {
                        title: revised.title,
                        kind: revised.kind,
                        category: revised.category,
                        description: revised.description,
                        suggestion: revised.suggestion,
                        linearSyncStatus: "dirty",
                    },
                ) catch (_) {
                _ => "{}"
            };
            let _ = self.svc.update("issues", id, patch) catch (e) {
                _ => {
                    throw ProcessError { message: "patch issue failed: " + baml.unstable.string(e) }
                }
            };
            // Return to the lifecycle's confirmed state (claimed on `status`).
            let _ = self.svc.transition("issues", id, "confirmed", "status", "{}", true) catch (e) {
                _ => {
                    throw ProcessError { message: "transition failed: " + baml.unstable.string(e) }
                }
            };
            return 0;
        }
    }
}
