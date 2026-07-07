// Bug-verify processors — run the verify AGENT (proxy) with the issue staged +
// verify_system_prompt, parse verdict.json, and act on it. Two claim loops share
// one body (run_verify_agent):
//
//   BugVerifyProcessor  first-pass verify of NEW issues (open -> verifying ->
//                       confirmed | closed). Conservative: ignores `stale`.
//   ReverifyProcessor   stale re-check against the newest nightly (reverify ->
//                       reverifying -> confirmed | closed). Rows enter `reverify`
//                       via the hourly requeueReverify Convex sweep. Auto-closes
//                       when the repro demonstrably passes (fixed) OR when no
//                       repro can be constructed at all (stale).
//
// The agent runs against the LATEST NIGHTLY (baml_version: "nightly" — the proxy
// installs it per-run and returns the resolved concrete version), and that real
// version is stamped into verifyBamlVersion/fixedIn (no more "current").

class VerifyIssue {
    _id: string?,
    title: string?,
    kind: string?,
    description: string?,
    repro: string?,
    linearIssueId: string?,
}


// Minimal read of a bamlBuilds row (only the sha is needed).
class BuildShaRead {
    sha: string?,
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
                // issue in `verifying` and blocks human redraft/approve. Periodic
                // re-checks are the ReverifyProcessor's job (gated by the capped
                // requeueReverify sweep, so no loop).
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
            return run_verify_agent(
                self.svc,
                self.proxy,
                self.model,
                self.max_turns,
                self.timeout_secs,
                raw,
                false,
            );
        }
    }
}

class ReverifyProcessor {
    wid: string,
    svc: ServiceClient,
    proxy: ProxyClient,
    model: string,
    max_turns: int,
    timeout_secs: int,

    implements Processor {
        function config(self) -> ProcessorConfig {
            return ProcessorConfig {
                role: "bug_reverify",
                table: "issues",
                claim_value: "reverify",
                claim_into: "reverifying",
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
            return run_verify_agent(
                self.svc,
                self.proxy,
                self.model,
                self.max_turns,
                self.timeout_secs,
                raw,
                true,
            );
        }
    }
}

// Shared verify body. `reverify` selects the stale re-check semantics:
//   - fixed (still_broken=false, high confidence): close + mark the board dirty
//     (LinearPush maps closed -> Merged/Done, so the card auto-closes).
//   - reverify-only: verdict.stale==true closes as stale (no fixedIn), with a
//     note appended to the description so the card says why.
//   - otherwise: re-stamp verifiedAt/verifyBamlVersion and rest at confirmed.
function run_verify_agent(svc: ServiceClient, proxy: ProxyClient, model: string, max_turns: int, timeout_secs: int, raw: string, reverify: bool) -> int throws ProcessError {
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
        model: model,
        max_turns: max_turns,
        prompt: "Verify whether the bug described in issue.json still exists in the baml CLI on PATH. Reproduce it (or build a minimal repro), observe the current behavior, and write verdict.json.",
        system_prompt: verify_system_prompt(),
        files: files,
        prices: prices_for(model),
        // Latest nightly, installed per-run by the proxy; the result carries the
        // resolved concrete version for stamping.
        baml_version: "nightly",
        install_skill: false,
        post_file_patterns: ["verdict.json"],
        max_file_bytes: 262144,
        max_total_file_bytes: 4194304,
        invocation_timeout_secs: timeout_secs,
    };
    let result = proxy.run_agent(req) catch (e) {
        _ => {
            throw ProcessError { message: "verify agent failed: " + baml.unstable.string(e) }
        }
    };

    let verdict = parse_verdict(result);
    let version = verify_version_of(svc, result);
    let now = baml.sys.now_ms();

    let outcome = verify_outcome(verdict, reverify);
    let fixed = outcome == "fixed";
    let stale = outcome == "stale";

    // Build the patch as a map so absent fields are OMITTED (a serialized null
    // in a Convex patch overwrites the stored value).
    let patch_map: map<string, json> = {};
    patch_map.set("verifiedAt", now);
    patch_map.set("verifyBamlVersion", version);
    if (fixed) {
        patch_map.set("fixedIn", version);
    }
    if (fixed || stale) {
        // The board must follow the close: mark dirty so LinearPush re-renders
        // the card into the Merged/Done state (previously a closed issue's card
        // silently stayed open).
        patch_map.set("linearSyncStatus", "dirty");
    }
    if (stale) {
        let desc = match (issue.description) {
            let d: string => d,
            null => "",
        };
        patch_map
            .set(
                "description",
                desc
                    + "\n\n> Closed as stale: no reproduction could be constructed against baml `"
                    + version
                    + "`. "
                    + verdict.evidence,
            );
    }
    let patch = baml.json.to_string(patch_map) catch (_) {
        _ => "{}"
    };
    // Fail LOUD: a swallowed stamp/transition would strand the issue in the
    // in-flight state until the reaper, silently losing the verdict.
    let _ = svc.update("issues", id, patch) catch (e) {
        _ => {
            throw ProcessError { message: "verify patch failed: " + baml.unstable.string(e) }
        }
    };
    let to = if (fixed || stale) {
        "closed"
    } else {
        "confirmed"
    };
    let _ = svc.transition("issues", id, to, "status", "{}", true) catch (e) {
        _ => {
            throw ProcessError { message: "verify transition failed: " + baml.unstable.string(e) }
        }
    };
    return 0;
}

// Pure verdict -> outcome mapping (unit-tested in tests/verify_test.baml):
//   "fixed"  — demonstrably gone (still_broken=false, high confidence) -> close.
//   "stale"  — reverify only: no repro could be constructed at all -> close.
//   "broken" — anything else -> rest at confirmed.
function verify_outcome(verdict: Verdict, reverify: bool) -> string {
    if (verdict.still_broken == false && verdict.confidence == "high") {
        return "fixed";
    }
    let verdict_stale = match (verdict.stale) {
        let s: bool => s,
        null => false,
    };
    if (reverify && verdict_stale) {
        return "stale";
    }
    return "broken";
}

// The concrete baml version this verify ran against: the proxy's resolved
// agent toolchain, falling back to the newest ready build's sha, then "current".
function verify_version_of(svc: ServiceClient, result: AgentResult) -> string {
    let resolved = match (result.baml_version) {
        let v: string => v,
        null => "",
    };
    if (resolved != "") {
        return resolved;
    }
    let newest = newest_ready_build(svc);
    if (newest != "") {
        return newest;
    }
    return "current";
}

// Sha of the newest ready bamlBuilds row ("" when none / unreadable). list()
// returns newest-first (convex/lib.ts orders desc).
function newest_ready_build(svc: ServiceClient) -> string {
    let raw = svc.list("bamlBuilds", "status", "ready", "by_status_created", 1) catch (_) {
        _ => "[]"
    };
    let rows: BuildShaRead[] = baml.json.from_string<BuildShaRead[]>(raw) catch (_) {
        _ => []
    };
    for (let r in rows) {
        let sha = match (r.sha) {
            let s: string => s,
            null => "",
        };
        if (sha != "") {
            return sha;
        }
    }
    return "";
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
                stale: null,
            }
        }
    };
}
