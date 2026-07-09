// scenario 39 — harness extensibility: ../../../../../llm-provider/ideas/scenarios/39-harness-extensibility/README.md
//   implementation.baml — LIBRARY/std-lib-author code (offline fixtures).
//
// The design's implement.baml is a big net-new stdlib surface: a `HarnessTool`
// that extends `Tool` with a dispatch `source` (in-process handler vs MCP
// connection), a hook pipeline (`Hook`/`HookEvent`/`HookVerdict` +
// hook-gated `dispatch`), a `Skills` capability with its own `SkillError`
// channel, a `SubAgent` combinator, and an `A2aProvider`. Almost NONE of that
// ships on the real surface today — the real `ClaudeCode` is a marker Provider
// that `implements Realtime` over a subprocess (P8-stubbed), carrying the
// extension payload as bound config fields (`allowed_tools`). See harness.baml.
//
// So the extension flow is realized on what IS real (REALIZED.md, baml_gotchas):
//   - the tool table is `allowed_tools` (built-ins) + registered app tools,
//     merged into ONE `baml.ai.Tool[]` — the design's uniform `Tool[]` field
//     ("my function" and a remote tool ride one list), `allowed_tools` shaping it;
//   - the extension ROUND-TRIP (app tool registered INTO the table -> model
//     calls it -> dispatched result flows back) is proven by an OFFLINE,
//     deterministic `Tools` provider (`S39Harness`) — the design's subprocess
//     tool-table plumbing is P8, so the loop runs against a scripted fake here,
//     exactly as scenario 10's `ScriptedTools` does for the agentic loop;
//   - skills / sub-agents whose STRUCTURED output is SAP-parsed back are modeled
//     as LLM functions (per the "prefer LLM functions" directive) — the skill IS
//     an LLM function returning a typed class, SAP-parsed offline via a fake.
//     Those live in usage.baml (app-author code).
//
// DESIGN DIVERGENCES (noted, not built — stdlib-freeze constraint):
//   - `HarnessTool.source` (in-process handler / MCP conn / null built-in): the
//     real `baml.ai.Tool` carries NO dispatch source; dispatch is the app's
//     `run_tools(dispatch=…)` closure. We keep the source split as the app-side
//     `s39_dispatch` router (name -> handler) instead of a field on the tool.
//   - Hooks (`Hook`/`HookEvent`/`HookVerdict`), the `Skills` capability + its
//     `baml.errors.SkillError` channel, the `SubAgent` combinator, and
//     `A2aProvider` are net-new stdlib not present on the real surface.
//   - the real `ClaudeCode` does not `implement Tools`, so the loop cannot run
//     against the real subprocess offline — the P8 stub throws Unsupported.


// ── The custom tool's argument shape: an ordinary class -> first-class `type`.
// The design's GeoArgs; the app never hand-writes a schema — `Tool.from_type`
// lowers this class to the wire form and keeps it for `validate_args`.
class S39GeoArgs {
    latitude: float,
    longitude: float,
}

// ── The typed final value the scripted harness surfaces. A ONE-string-field
// class SAP-parses from almost any raw text (baml_gotchas D6), so the last tool
// result flows straight through into `tool_result` — that IS the round-trip
// proof: whatever the dispatcher produced comes back as the harness's value.
class S39HarnessAnswer {
    tool_result: string,
}


// ── The custom in-process tool (the design's `weather_tool()`), on the real
// `baml.ai.Tool`. `from_type` throws Unsupported if a type can't be lowered; we
// fold it into a placeholder so this is a throws-free tool factory (scenario 09).
function s39_weather_tool() -> baml.ai.Tool {
    baml.ai.Tool.from_type(
        "get_temperature",
        "Get the current temperature at a location.",
        reflect.type_of<S39GeoArgs>(),
    ) catch (e) {
        _ => baml.ai.Tool { name: "get_temperature", description: "", parameters: "{}" },
    }
}


// ── The tool table: `allowed_tools` (built-ins the harness resolves itself) +
// the registered app tools, merged into ONE flat `Tool[]`. This is the design's
// §4 "one Tool[] field" with `allowed_tools` shaping the surface. Built-ins are
// bare names (design's `source: null` — provider-resolved); app tools carry a
// real lowered schema. `log.info` marks each registration.
function s39_tool_table(allowed_tools: string[], app_tools: baml.ai.Tool[]) -> baml.ai.Tool[] throws never {
    let table: baml.ai.Tool[] = [];
    for (let name in allowed_tools) {
        log.info("39 register built-in tool into harness table: " + name);
        table.push(baml.ai.Tool { name: name, description: "built-in (provider-resolved)", parameters: "{}" });
    }
    for (let t in app_tools) {
        log.info("39 register app tool into harness table: " + t.name);
        table.push(t);
    }
    table
}


// ── The offline harness. A deterministic `Tools` provider (no subprocess, no
// network) that proves the extension ROUND-TRIP: it is handed the surfaced tool
// table at `begin`, it drives the model to CALL the registered app tool by name
// (failing loudly if that tool is not actually in the surfaced table), threads
// the dispatched result back through `submit`, and returns it as the final value.
// This is the design's ClaudeCode-implements-Tools, minus the subprocess (P8).
class S39Session {
    table: baml.ai.Tool[],
    app_tool: string,
    turns_wanted: int,
    turns_done: int,
    last_output: string,
}

class S39Harness {
    /// The built-in tools the harness resolves itself (design's `builtin_tools`
    /// / real `ClaudeCode.allowed_tools`).
    allowed_tools: string[],
    /// The registered app tool the scripted model will call.
    app_tool: string,
    /// How many tool turns the scripted model demands before answering.
    turns_wanted: int,

    implements baml.ai.Provider {}

    implements baml.ai.Tools {
        type Transcript = S39Session

        function begin<T>(self, prompt: string, tools: baml.ai.Tool[]) -> S39Session throws never {
            log.info("39 harness begin: surfaced tool table has " + tools.length().to_string() + " tools");
            S39Session {
                table: tools,
                app_tool: self.app_tool,
                turns_wanted: self.turns_wanted,
                turns_done: 0,
                last_output: "",
            }
        }

        function step<T>(self, t: S39Session) -> T | baml.ai.ToolCalls
            throws baml.errors.ToolError | baml.errors.UnknownError {
            if (t.turns_done < t.turns_wanted) {
                // The registered app tool MUST be in the surfaced table — that is
                // what "surfaced INTO the harness's tool table" means. If it is
                // not, the model could never call it (typed failure, not a silent
                // no-op).
                if (!s39_table_has(t.table, t.app_tool)) {
                    throw baml.errors.UnknownError {
                        data: null,
                        message: ["app tool '" + t.app_tool + "' was never surfaced into the harness table"],
                    };
                }
                log.info("39 harness step: model calls registered tool '" + t.app_tool + "'");
                return baml.ai.ToolCalls {
                    calls: [
                        baml.ai.ToolCall {
                            id: "call_1",
                            name: t.app_tool,
                            args: "{\"latitude\": 52.5, \"longitude\": 13.4}",
                        },
                    ],
                };
            }
            // Script exhausted: the dispatched result IS the answer (round-trip).
            // Wrap it in a valid JSON envelope so SAP decodes it into the fixture's
            // `S39HarnessAnswer { tool_result }` (a bare non-JSON string does not
            // trigger SAP's single-string-field passthrough — baml_gotchas D6).
            let final_json = "{\"tool_result\": \"" + t.last_output + "\"}";
            baml.sap.parse<T>(final_json) catch (e) {
                _ => throw baml.errors.UnknownError { data: e, message: ["harness final parse failed"] },
            }
        }

        function submit(self, t: S39Session, results: baml.ai.ToolResult[]) -> S39Session throws never {
            let out = match (results.at(0)) {
                null => t.last_output,
                let r: baml.ai.ToolResult => r.output,
            };
            log.info("39 harness submit: dispatched result threaded back = " + out);
            S39Session {
                table: t.table,
                app_tool: t.app_tool,
                turns_wanted: t.turns_wanted,
                turns_done: t.turns_done + 1,
                last_output: out,
            }
        }
    }
}

/// Is `name` present in the surfaced tool table? (Shared surfacing check.)
function s39_table_has(table: baml.ai.Tool[], name: string) -> bool throws never {
    for (let t in table) {
        if (t.name == name) {
            return true;
        }
    }
    false
}
