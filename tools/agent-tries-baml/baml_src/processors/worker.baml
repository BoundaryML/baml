// BAML worker processor — claims a task, runs the Claude Code agent (via the
// proxy) with the worker system prompt, parses the agent's trophy.json, and
// creates a trophy doc + finishes the task. Ports services/baml_worker.
// Implements the Processor interface (core/processor.baml).
//
// Focused port: skill-arena injection, repro verification, mined-error backstop,
// baml-version blocking, and Slack notify are deferred (TODOs) — the core
// task -> agent -> trophy flow is complete.

class TrophyFindingAnchor {
    call_index: int?,
    turn_index: int?,
}

class TrophyFinding {
    kind: string,
    title: string,
    description: string,
    anchor: TrophyFindingAnchor,
    suggestion: string,
    repro: TrophyRepro?,
}

class TrophyDoc {
    taskId: string,
    outcome: string, // success | partial | failed
    bamlVersion: string?,
    metrics: Metrics,
    summary: string,
    whatWentWell: string[],
    whatFailed: string[],
    reportMd: string,
    findings: TrophyFinding[],
    suggestions: TrophySuggestion[],
    filesCreated: map<string, string>, // {path: content} of agent-authored artifacts
    skillUsed: string?, // the skill source this run onboarded from (repo URL)
    skillVersion: string?, // baml-skill commit sha (repo@sha = exact skill identity)
    transcriptStorageId: string?, // blob id of the raw transcript (GET /transcripts/<id>)
    turnLog: json[]?, // raw turn objects (typed TurnRow[]? serializes elements to null)
    cohortId: string?,
    status: string, // queued (enters dedup) | cohort_member (held)
}

class WorkerProcessor {
    wid: string,
    svc: ServiceClient,
    proxy: ProxyClient,
    model: string,
    max_turns: int,
    timeout_secs: int,
    quickstart_url: string,

    implements Processor {
        function config(self) -> ProcessorConfig {
            return ProcessorConfig {
                role: "baml_worker",
                table: "tasks",
                claim_value: "queued",
                claim_into: "running",
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
            let task = baml.json.from_string<Task>(raw) catch (_) {
                _ => {
                    throw ProcessError { message: "bad task json" }
                }
            };
            let task_id = match (task._id) {
                let id: string => id,
                null => {
                    throw ProcessError { message: "task missing _id" }
                },
            };

            let cold = match (task.coldStart) {
                true => true,
                _ => false,
            };
            let system_prompt = if (cold) {
                cold_start_system_prompt(self.quickstart_url)
            } else {
                worker_system_prompt()
            };
            // Warm intent: a per-task pin wins; otherwise install the LATEST of the
            // configured channel (baml_channel(), default "nightly") so the agent
            // picks up BAML fixes with no image rebuild. The proxy resolves this to a
            // concrete version and returns it on the result (recorded on the trophy).
            // Cold runs onboard a frozen baml (the onboarding flow is under test).
            let baml_version = if (cold) {
                null
            } else {
                match (task.bamlVersion) {
                    let v: string => v,
                    null => baml_channel(),
                }
            };

            // Warm runs install the official skills THEMSELVES (`baml agent install
            // --latest`, per the system prompt) — the install is part of the
            // benchmarked behavior. We still record the exact skill identity
            // (repo@sha): --latest resolves to baml-skill@main, so the main HEAD
            // sha at dispatch time identifies it to the minute. Cold runs install
            // nothing (the onboarding experience itself is under test).
            let skill_ver = if (cold) {
                ""
            } else {
                skill_main_sha()
            };
            let skill_src = if (cold) {
                ""
            } else {
                skill_repo_url()
            };
            let files: map<string, string> = {};

            let req = RunAgentRequest {
                cell_id: task_id,
                model: self.model,
                max_turns: self.max_turns,
                prompt: task.prompt,
                system_prompt: system_prompt,
                files: files,
                prices: prices_for(self.model),
                baml_version: baml_version,
                install_skill: cold == false,
                post_file_patterns: ["**/*.baml", "*.md", "trophy.json"],
                max_file_bytes: 262144,
                max_total_file_bytes: 4194304,
                invocation_timeout_secs: self.timeout_secs,
            };

            let result = self.proxy.run_agent(req) catch (e) {
                _ => {
                    throw ProcessError { message: "proxy run_agent failed: " + baml.unstable.string(e) }
                }
            };

            // Stash the transcript blob on the volume; its storage id goes on the
            // trophy so the dashboard's terminal view can fetch it (GET /transcripts/<id>).
            let transcript_sid = match (result.transcript) {
                let t: string => {
                    self.svc.put_transcript("tasks", task_id, t) catch (_) {
                        _ => ""
                    }
                },
                null => "",
            };

            // Parse the agent's self-reported trophy.json.
            let report = parse_trophy(result);
            let outcome = derive_outcome(result.status, report.task_completed);
            let findings = findings_of(report);

            let doc = TrophyDoc {
                taskId: task_id,
                outcome: outcome,
                // Prefer the CONCRETE version the proxy resolved + ran the agent on;
                // fall back to the requested intent (channel/pin) if absent.
                bamlVersion: match (result.baml_version) {
                    let v: string => v,
                    null => baml_version,
                },
                metrics: metrics_of(result, self.model),
                summary: report.summary,
                whatWentWell: report.what_went_well,
                whatFailed: report.what_failed,
                reportMd: report.report_md,
                findings: findings,
                suggestions: report.suggestions,
                filesCreated: result.post_files,
                skillUsed: nz_str(skill_src),
                skillVersion: nz_str(skill_ver),
                transcriptStorageId: nz_str(transcript_sid),
                // The structured per-turn log the dashboard's "Turns" tab renders,
                // parsed by the proxy from the session jsonl. When the proxy couldn't
                // produce one but a transcript blob exists, stamp the single
                // placeholder turn so the transcript section still shows (defaulting
                // to the Terminal tab).
                turnLog: match (result.turn_log) {
                    let tl: json[] => tl,
                    null => {
                        if (transcript_sid == "") {
                            null
                        } else {
                            terminal_only_turnlog()
                        }
                    },
                },
                cohortId: task.cohortId,
                status: match (task.cohortId) {
                    let c: string => "cohort_member",
                    null => "queued",
                },
            };
            let doc_json = baml.json.to_string(doc) catch (_) {
                _ => {
                    throw ProcessError { message: "serialize trophy failed" }
                }
            };

            let trophy_id = self.svc.create("trophies", doc_json) catch (e) {
                _ => {
                    throw ProcessError { message: "create trophy failed: " + baml.unstable.string(e) }
                }
            };
            let _ = self.svc.transition("tasks", task_id, "done", "status", "{}", true) catch (e) {
                _ => {
                    throw ProcessError { message: "transition task failed: " + baml.unstable.string(e) }
                }
            };
            // Slack completion notify: reply in the bench thread with the result +
            // a trophy link (no-op for non-Slack tasks / when the bot token is unset).
            let _ = notify_slack_result(task, outcome, report.summary, result, findings.length(), trophy_id);
            return 0;
        }
    }
}

// Read trophy.json from the agent's post_files (fallback: an empty report).
function parse_trophy(result: AgentResult) -> TrophyReport {
    let raw = match (result.post_files.get("trophy.json")) {
        let t: string => t,
        null => "",
    };
    return baml.json.from_string<TrophyReport>(raw) catch (_) {
        _ => {
            TrophyReport {
                report_md: "Agent wrote no parseable trophy.json.",
                task_completed: "false",
                summary: "No trophy report.",
                what_went_well: [],
                what_failed: [],
                issues: [],
                suggestions: [],
            }
        }
    };
}

function derive_outcome(status: string, task_completed: string) -> string {
    if (status != "ok") {
        return "failed";
    }
    if (task_completed == "partial") {
        return "partial";
    }
    if (task_completed == "true") {
        return "success";
    }
    return "failed";
}

// Reply in the bench's Slack thread with the run result + a trophy link. No-op for
// non-Slack tasks or when the bot token is unset. Ports baml_worker's completion
// notify (services/baml_worker/__main__.py): "*Result: <outcome>.* <summary>" + a
// stats line (turns · api · cost [· N issues flagged]) + a <link|view trophy>.
function notify_slack_result(
    task: Task,
    outcome: string,
    summary: string,
    result: AgentResult,
    findings_count: int,
    trophy_id: string,
) -> int {
    let channel = match (task.slackChannel) {
        let c: string => c,
        null => "",
    };
    let token = slack_bot_token();
    if (channel == "" || token == "") {
        return 0;
    }
    let thread = match (task.slackThreadTs) {
        let t: string => t,
        null => "",
    };
    let turns = match (result.turns) {
        let n: int => baml.unstable.string(n),
        null => "-",
    };
    let api = match (result.tool_calls) {
        let n: int => baml.unstable.string(n),
        null => "-",
    };
    let cost = match (result.estimated_cost_usd) {
        let c: float => "$" + baml.unstable.string((c * 100.0).round() / 100.0),
        null => "$-",
    };
    let flagged = if (findings_count > 0) {
        " · " + baml.unstable.string(findings_count) + " issue(s) flagged"
    } else {
        ""
    };
    let link = ui_base_url() + "/runs/" + trophy_id;
    let text = "*Result: "
        + outcome
        + ".* "
        + summary
        + "\n"
        + turns
        + " turns · "
        + api
        + " api · "
        + cost
        + flagged
        + "\n<"
        + link
        + "|view trophy>";
    let _ = slack_post_message(token, channel, text, thread);
    return 0;
}

// A single placeholder turn (as a raw json value) so the dashboard renders the
// transcript section, then defaults to the Terminal tab — the full rendered
// transcript. Built via json.parse so it serializes as a real object (a typed
// TurnRow in an optional-array field serializes to null). Interim until the
// structured parse_turn_log is wired back in.
function terminal_only_turnlog() -> json[] {
    let turn: json = baml
        .json
        .parse(
            #"{"i":1,"text_preview":"Open the Terminal tab for the full transcript (structured turn view is coming soon).","tools":[]}"#,
        ) catch (_) {
        _ => 0
    };
    return [turn];
}

// Build the trophy metric bag from the agent result. Derived stats the proxy
// can't reliably supply are computed HERE, where the model (hence the rate card)
// is known: total_tokens (sum of components when the session has no pre-summed
// value), estimated_cost_usd (worker-side, robust to the rr.prices round-trip),
// and files_touched / loc_changed (from the collected post_files).
function metrics_of(result: AgentResult, model: string) -> Metrics {
    return Metrics {
        turns: result.turns,
        tool_calls: result.tool_calls,
        api_calls: result.api_calls,
        invocations: null,
        input_tokens: result.input_tokens,
        output_tokens: result.output_tokens,
        total_tokens: total_tokens_of(result),
        cache_read_tokens: result.cache_read_tokens,
        cache_write_tokens: result.cache_write_tokens,
        wall_clock_ms: result.wall_clock_ms,
        setup_build_ms: null,
        agent_thinking_ms: null,
        debug_retries: null,
        estimated_cost_usd: cost_for(result, model),
        files_touched: files_touched_of(result),
        loc_changed: loc_of(result),
        skill_installed: result.skill_installed,
    };
}

function m_or_zero(v: int?) -> int {
    return match (v) {
        let n: int => n,
        null => 0,
    };
}

// Claude sessions don't emit a pre-summed total; sum the components when absent.
function total_tokens_of(result: AgentResult) -> int? {
    return match (result.total_tokens) {
        let t: int => t,
        null => {
            let sum = m_or_zero(result.input_tokens)
                + m_or_zero(result.output_tokens)
                + m_or_zero(result.cache_read_tokens)
                + m_or_zero(result.cache_write_tokens);
            if (sum == 0) {
                null
            } else {
                sum
            }
        },
    };
}

// Prefer the proxy's estimate; else compute it worker-side from the components and
// the model's rate card (null only when the model has no known prices).
function cost_for(result: AgentResult, model: string) -> float? {
    return match (result.estimated_cost_usd) {
        let c: float => c,
        null => match (prices_for(model)) {
            let p: Prices => {
                compute_cost(
                    m_or_zero(result.input_tokens),
                    m_or_zero(result.output_tokens),
                    m_or_zero(result.cache_read_tokens),
                    m_or_zero(result.cache_write_tokens),
                    p,
                )
            },
            null => null,
        },
    };
}

// Count of agent-authored artifacts (the self-report trophy.json doesn't count).
function files_touched_of(result: AgentResult) -> int? {
    let n = 0;
    for (let k in result.post_files.keys()) {
        if (k != "trophy.json") {
            n = n + 1;
        }
    }
    if (n == 0) {
        return null;
    }
    return n;
}

// Lines across the collected artifacts (a post_files-based proxy for loc_changed;
// we keep full content, not a diff, so this is total authored lines).
function loc_of(result: AgentResult) -> int? {
    let total = 0;
    for (let k in result.post_files.keys()) {
        if (k != "trophy.json") {
            let content = match (result.post_files.get(k)) {
                let c: string => c,
                null => "",
            };
            total = total + content.split("\n").length();
        }
    }
    if (total == 0) {
        return null;
    }
    return total;
}

function findings_of(report: TrophyReport) -> TrophyFinding[] {
    let out: TrophyFinding[] = [];
    for (let iss in report.issues) {
        if (iss.kind == "skill" || iss.kind == "language") {
            out.push(
                TrophyFinding {
                    kind: iss.kind,
                    title: iss.title,
                    description: iss.description,
                    anchor: TrophyFindingAnchor { call_index: iss.call_index, turn_index: iss.call_index },
                    suggestion: iss.suggestion,
                    repro: iss.repro,
                },
            );
        }
    }
    return out;
}

// Empty string -> null (for optional doc fields).
function nz_str(s: string) -> string? {
    return match (s) {
        "" => null,
        _ => s,
    };
}
