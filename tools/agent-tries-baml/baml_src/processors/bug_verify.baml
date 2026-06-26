// Bug-verify processor — claims an open issue, runs the verify AGENT (proxy) with
// the issue staged + verify_system_prompt, parses verdict.json, and either closes
// the issue (fixed, high confidence) or stamps verifiedAt/brokeIn. Ports
// services/bug_verify. Agent stage (it runs the `baml` CLI), so it goes through
// the proxy like the worker; the Verdict is decoded with from_string.

class VerifyIssue {
    _id: string?,
    title: string?,
    kind: string?,
    description: string?,
    repro: string?,
    linearIssueId: string?,
}

class VerifyPatch {
    verifiedAt: int,
    verifyBamlVersion: string,
    fixedIn: string?,
    status: string?,
}

class BugVerifyProcessor {
    wid: string,
    svc: ServiceClient,
    proxy: ProxyClient,
    model: string,
    max_turns: int,
    timeout_secs: int,

    implements Processor {
        function config(self) -> ProcessorConfig {
            return ProcessorConfig {
                // Verify NEW issues once (open -> verifying -> confirmed). Do NOT
                // claim "confirmed": redrafted issues land there, and re-verifying
                // a confirmed issue (which transitions back to confirmed when not
                // "fixed") re-claims it forever -- a tight loop that strands the
                // issue in `verifying` and blocks human redraft/approve.
                role: "bug_verify",
                table: "issues",
                claim_value: "open",
                claim_into: "verifying",
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
            let issue = baml.json.from_string<VerifyIssue>(raw) catch (_) {
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

            // Stage issue.json (+ repro.baml when present) for the agent.
            let files: map<string, string> = { "issue.json": raw };
            match (issue.repro) {
                let r: string => files.set("baml_src/main.baml", r),
                null => null,
            };

            let req = RunAgentRequest {
                cell_id: "verify-" + id,
                model: self.model,
                max_turns: self.max_turns,
                prompt: "Verify whether the bug described in issue.json still exists in the baml CLI on PATH. Reproduce it (or build a minimal repro), observe the current behavior, and write verdict.json.",
                system_prompt: verify_system_prompt(),
                files: files,
                prices: prices_for(self.model),
                baml_version: null,
                install_skill: false,
                post_file_patterns: ["verdict.json"],
                max_file_bytes: 262144,
                max_total_file_bytes: 4194304,
                invocation_timeout_secs: self.timeout_secs,
            };
            let result = self.proxy.run_agent(req) catch (e) {
                _ => {
                    throw ProcessError { message: "verify agent failed: " + baml.unstable.string(e) }
                }
            };

            let verdict = parse_verdict(result);
            let now = baml.sys.now_ms();
            // Fixed with high confidence -> close; otherwise re-stamp and re-confirm.
            let still = verdict.still_broken;
            let fixed = still == false && verdict.confidence == "high";
            let patch = baml
                .json
                .to_string(
                    VerifyPatch {
                        verifiedAt: now,
                        verifyBamlVersion: "current",
                        fixedIn: if (fixed) {
                            "current"
                        } else {
                            null
                        },
                        status: null,
                    },
                ) catch (_) {
                _ => "{}"
            };
            let _ = self.svc.update("issues", id, patch) catch (_) {
                _ => ""
            };
            let to = if (fixed) {
                "closed"
            } else {
                "confirmed"
            };
            let _ = self.svc.transition("issues", id, to, "status", "{}", true) catch (_) {
                _ => ""
            };
            return 0;
        }
    }
}

function parse_verdict(result: AgentResult) -> Verdict {
    let raw = match (result.post_files.get("verdict.json")) {
        let t: string => t,
        null => "",
    };
    return baml.json.from_string<Verdict>(raw) catch (_) {
        _ => {
            Verdict {
                still_broken: true,
                confidence: "low",
                evidence: "no parseable verdict.json",
                fixed_behavior: null,
            }
        }
    };
}
