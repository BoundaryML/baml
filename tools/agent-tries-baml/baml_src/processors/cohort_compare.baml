// Cohort-compare processor — claims a ready skill-arena cohort, renders its
// member-variant trophies into arena.md, runs the Compare() LLM function, and
// emits a single synthesized "cohort trophy" (status=queued) that re-enters dedup
// like any other. Ports services/cohort_compare. Single LLM call, no agent.

class CohortDoc {
    _id: string?,
    prompt: string?,
    skillRefs: string[]?,
    memberTaskIds: string[]?,
    slackChannel: string?,
    slackThreadTs: string?,
}

class CohortTrophyDoc {
    taskId: string,
    outcome: string,
    metrics: json,
    summary: string,
    whatWentWell: string[],
    whatFailed: string[],
    reportMd: string,
    findings: json,
    suggestions: json,
    isCohortReport: bool,
    cohortId: string,
    status: string,
}

class CohortCompareProcessor {
    wid: string,
    svc: ServiceClient,

    implements Processor {
        function config(self) -> ProcessorConfig {
            return ProcessorConfig {
                role: "cohort_compare",
                table: "cohorts",
                claim_value: "queued",
                claim_into: "comparing",
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
            let cohort = baml.json.from_string<CohortDoc>(raw) catch (_) {
                _ => {
                    throw ProcessError { message: "bad cohort json" }
                }
            };
            let id = match (cohort._id) {
                let v: string => v,
                null => {
                    throw ProcessError { message: "cohort missing _id" }
                },
            };

            // Render arena.md from the cohort's member (variant) trophies.
            let members = self.svc.get("cohorts", id) catch (_) {
                _ => null
            };
            let arena = render_arena(cohort, self.svc, id);

            let comparison = Compare(arena) catch (e) {
                _ => {
                    throw ProcessError { message: "Compare failed: " + baml.unstable.string(e) }
                }
            };

            // Emit the synthesized cohort trophy (enters dedup).
            let findings_json = baml.json.to_string(comparison.findings) catch (_) {
                _ => "[]"
            };
            let suggestions_json = baml.json.to_string(comparison.suggestions) catch (_) {
                _ => "[]"
            };
            let doc = CohortTrophyDoc {
                taskId: id,
                outcome: "feedback",
                metrics: empty_obj(),
                summary: comparison.summary,
                whatWentWell: comparison.what_went_well,
                whatFailed: comparison.what_failed,
                reportMd: comparison.report_md,
                findings: parse_json_or_empty(findings_json),
                suggestions: parse_json_or_empty(suggestions_json),
                isCohortReport: true,
                cohortId: id,
                status: "queued",
            };
            let doc_json = baml.json.to_string(doc) catch (_) {
                _ => {
                    throw ProcessError { message: "serialize cohort trophy failed" }
                }
            };
            let _ = self.svc.create("trophies", doc_json) catch (e) {
                _ => {
                    throw ProcessError { message: "create cohort trophy failed: " + baml.unstable.string(e) }
                }
            };
            let _ = self.svc.transition("cohorts", id, "done", "status", "{}", true) catch (_) {
                _ => ""
            };
            return 0;
        }
    }
}

// Build arena.md: one block per member variant trophy.
function render_arena(cohort: CohortDoc, svc: ServiceClient, cohort_id: string) -> string {
    let prompt = match (cohort.prompt) {
        let p: string => p,
        null => "",
    };
    let header = "# Skill-arena task\n" + prompt + "\n\n";
    let body = svc.get("trophies?field=cohortId&value=" + cohort_id, "list") catch (_) {
        _ => ""
    };
    // The member trophies carry per-variant report + findings; pass them through.
    return header + "# Variants\n" + body;
}

function empty_obj() -> json {
    return baml.json.parse("{}") catch (_) {
        _ => {
            baml.json.parse("null") catch (_) {
                _ => ""
            }
        }
    };
}

function parse_json_or_empty(s: string) -> json {
    return baml.json.parse(s) catch (_) {
        _ => empty_obj()
    };
}
