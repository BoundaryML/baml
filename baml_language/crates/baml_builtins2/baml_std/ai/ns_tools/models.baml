class Tool {
    name: string,
    description: string,
    input_schema: json,
    handler: baml.AnyFunction<Returns = unknown>,
    handoff: bool,

    function as_handoff(self) -> Tool throws never {
        Tool {
            name: self.name,
            description: self.description,
            input_schema: self.input_schema,
            handler: self.handler,
            handoff: true,
        }
    }
}

// Public tool inputs accept ordinary BAML functions. Tool is the normalized
// representation used after configuration crosses into the runner/provider
// machinery.
type ToolInput = Tool | baml.AnyFunction<Returns = unknown>;

function _coarse_schema(type_name: string) -> json throws never {
    let schema: json = { "type": type_name };
    schema
}

function _schema_for_function(handler: baml.AnyFunction) -> json throws never {
    let signature = reflect.signature(handler);
    let properties: map<string, json> = {};
    let required: string[] = [];
    //# Derive required positional parameters
    for (let arg in signature.args) {
        let schema = baml.schema.json_schema(arg.type) catch (e) {
            _ => _coarse_schema(arg.type.to_string()),
        };
        let _ = properties.set(arg.name, schema);
        required.push(arg.name);
    }
    //# Derive defaulted/named parameters without marking them required
    for (let name in signature.opts.keys()) {
        match (signature.opts.get(name)) {
            let arg: reflect.Arg => {
                let schema = baml.schema.json_schema(arg.type) catch (e) {
                    _ => _coarse_schema(arg.type.to_string()),
                };
                let _ = properties.set(name, schema);
            },
            null => {},
        }
    }

    {
        "type": "object",
        "properties": properties,
        "required": baml.json.to_json(required) catch (e) {
            _ => null
        },
        "additionalProperties": false,
    }
}

// Function-backed application tool. AnyFunction preserves the callable's
// declared name, docstring, and signature. Explicit metadata is only needed
// for anonymous callables or when the external tool contract intentionally
// differs from the underlying function declaration. reflect.call_any performs
// checked named argument dispatch, including ordinary BAML default arguments.
function tool(
    handler: baml.AnyFunction<Returns = unknown>,
    name: string? = null,
    description: string? = null,
) -> Tool throws never {
    let signature = reflect.signature(handler);
    Tool {
        name: name ?? signature.name ?? "",
        description: description ?? signature.docstring ?? "",
        input_schema: _schema_for_function(handler),
        handler: handler,
        handoff: false,
    }
}

function normalize_tool(input: ToolInput) -> Tool throws never {
    match (input) {
        let existing: Tool => existing,
        let handler: baml.AnyFunction<Returns = unknown> => {
            tool(handler)
        },
    }
}

function normalize_tools(inputs: ToolInput[]) -> Tool[] throws never {
    inputs.map((input: ToolInput) -> Tool {
        normalize_tool(input)
    })
}

/// Widens a normalized `Tool[]` back to the public `ToolInput[]` boundary.
/// Arrays are invariant, so the widening is element-by-element. Useful when
/// handing a task's tools to another lifecycle, e.g.
/// `ai.realtime.with_automatic_tools(session, ai.tools.as_inputs(task.tools))`.
function as_inputs(tools: Tool[]) -> ToolInput[] throws never {
    tools.map((tool: Tool) -> ToolInput {
        tool
    })
}

// Escape hatch for MCP and other dynamically described tools. Runtime
// argument validation is still performed by the supplied handler.
function tool_from_json_schema(
    name: string,
    description: string,
    input_schema: json,
    handler: baml.AnyFunction<Returns = unknown>,
) -> Tool throws never {
    Tool {
        name: name,
        description: description,
        input_schema: input_schema,
        handler: handler,
        handoff: false,
    }
}

// A provider-owned tool is configured and executed by the provider adapter.
// It is deliberately not an application Tool and has no application handler.
class ProviderTool {
    name: string,
    kind: string,
    config: json,
}

class ToolCall {
    id: string,
    name: string,
    args: json,
}

/// The correlated outcome of one application tool call.
///
/// A ToolResult is conversation DATA, not program control flow. Providers
/// require every ToolCall id they emit to receive exactly one correlated
/// result — success or failure — because the model is the party that recovers
/// from a failed tool: it reads what happened and retries, picks another
/// tool, or reports the problem. That is why a failed tool is a value that
/// flows back into the conversation, not an error in the application's
/// `throws` channel. (Tool functions themselves fail by throwing
/// `ai.Failure`; `invoke_tool` is the total boundary that reifies the throw.)
///
/// The two cases are a union rather than one class with an `is_error` flag,
/// following the stdlib idiom for multi-case outcomes — compare the Agent's
/// `Done | Stopped | Handoff | Interrupted` — so consumers match on the
/// variant instead of branching on a boolean and decoding a magic
/// `{"error": ...}` payload. Provider adapters map the variants onto each
/// wire format at the serialization edge (e.g. Anthropic's `is_error`
/// boolean, OpenAI's stringified function output); `result_payload` is the
/// shared canonical JSON they submit.
type ToolResult = ToolOk | ToolError

/// A tool ran to completion; `output` is its JSON-encoded return value,
/// correlated to the requesting call by `id`.
class ToolOk {
    id: string,
    output: json,

    /// Correlate a successful output with the provider call that requested it.
    function of(call: ToolCall, output: json) -> ToolOk throws never {
        ToolOk { id: call.id, output: output }
    }
}

/// A tool could not produce an output. `message` is model-facing text: it is
/// submitted into the conversation so the model can self-correct (fix its
/// arguments, choose another tool, or report failure). `cause` carries the
/// TYPED original error for the application channel — `after_tool_call`
/// callbacks and observers match on it instead of parsing the rendered
/// message. It replaces the former `is_error: true` + `output: {"error":
/// ...}` convention with typed fields.
class ToolError {
    id: string,
    message: string,
    cause: (ai.Failure | baml.errors.UnknownError)?,

    /// Correlate a failure message with the provider call that requested it.
    function of(
        call: ToolCall,
        message: string,
        cause: (ai.Failure | baml.errors.UnknownError)? = null,
    ) -> ToolError throws never {
        ToolError { id: call.id, message: message, cause: cause }
    }
}

/// The provider correlation id of either variant. Field access through a
/// union requires a shared interface, so plumbing that only needs the id
/// uses this helper instead of matching at every site.
function result_id(result: ToolResult) -> string throws never {
    match (result) {
        let ok: ToolOk => ok.id,
        let failed: ToolError => failed.id,
    }
}

/// True for the `ToolError` variant. Prefer matching when you need the
/// variant's fields; this exists for wire formats and assertions that only
/// need the flag.
function result_is_error(result: ToolResult) -> bool throws never {
    match (result) {
        let failed: ToolError => true,
        _ => false,
    }
}

/// The JSON payload a provider adapter submits for either variant: the
/// output for `ToolOk`, the canonical `{"error": message}` object for
/// `ToolError`. Every adapter shares this shape so the model sees the same
/// error convention regardless of provider.
function result_payload(result: ToolResult) -> json throws never {
    match (result) {
        let ok: ToolOk => ok.output,
        let failed: ToolError => {
            let payload: json = { "error": failed.message };
            payload
        },
    }
}

function invoke_tool(tool: Tool, call: ToolCall) -> ToolResult throws never {
    //# Require a JSON object so named arguments can be checked by call_any
    let args = baml.json.from_json<map<string, unknown>>(call.args) catch (e) {
        _ => {
            return ToolError.of(call, "tool arguments must be an object");
        },
    };
    //# Invoke the original BAML function with signature-aware named
    //# arguments. Handlers may be ordinary fallible functions — including
    //# LLM functions (agent-as-tool) — so the boundary catches EVERYTHING:
    //# the model sees a rendered message, and the typed original rides in
    //# `cause` for the application channel.
    let output = reflect.call_any(tool.handler, args) catch_all (e) {
        let invalid: reflect.InvalidArgumentError => {
            return ToolError
                .of(
                    call,
                    `invalid arguments: expected ${invalid.expected}, received ${invalid.got}`,
                );
        },
        let failure: ai.Failure => {
            return ToolError.of(call, `tool failed: ${failure}`, cause = failure);
        },
        let unknown: baml.errors.UnknownError => {
            return ToolError.of(call, `tool failed: ${unknown.message}`, cause = unknown);
        },
        _ => {
            let wrapped = baml.errors.UnknownError.with_message<never>(e, "tool handler threw");
            return ToolError.of(call, `tool failed: ${wrapped.message}`, cause = wrapped);
        },
    };
    //# Normalize arbitrary typed function results at the provider boundary
    let encoded = baml.json.to_json(output) catch (e) {
        _ => {
            return ToolError.of(call, "tool result could not be serialized");
        },
    };
    ToolOk.of(call, encoded)
}

class ToolCalls {
    calls: ToolCall[],
}

/// The batch validation the stock Agent applies before any application
/// effect: non-empty batch, non-empty unique correlation IDs, non-empty
/// names. Public so custom runners enforce the same envelope instead of
/// copying it.
function check_calls(
    calls: ToolCalls,
    provider_name: string,
) -> null throws ai.InvalidRequest {
    ai.internal._validate_tool_calls(calls, provider_name)
}

/// "Every pending call answered exactly once, and nothing else" — the
/// correlation rule providers enforce in submit and sessions enforce for
/// handoffs. Public so custom runners and adapters share one check with
/// one message.
function check_results(
    pending: ToolCall[],
    results: ToolResult[],
    provider_name: string,
) -> null throws ai.InvalidRequest {
    ai.internal._require_exact_correlation(pending, results, provider_name)
}

class ToolRegistryError {
    name: string,
    message: string,
}

implements ai.Failure for ToolRegistryError {
  function effects(self) -> ai.Effects throws never { ai.Effects.None }
}

class ToolRegistry {
    roster: Tool[],
    version: int,

    function new(tools: ToolInput[]) -> ToolRegistry throws ai.Failure {
        let registry = ToolRegistry { roster: [], version: 0 };
        //# Seed the registry through validated insertion
        for (let input in tools) {
            registry.add(input);
        }
        registry
    }

    function add(self, input: ToolInput) -> null throws ai.Failure {
        let tool = normalize_tool(input);
        //# Reject duplicate tool names
        for (let existing in self.roster) {
            if (existing.name == tool.name) {
                throw ToolRegistryError {
          name: tool.name,
          message: "tool name is already registered: " + tool.name,
        };
            }
        }
        self.roster.push(tool);
        self.version = self.version + 1;
        null
    }

    function add_all(self, tools: ToolInput[]) -> null throws ai.Failure {
        let combined: ToolInput[] = [];
        for (let existing in self.roster) {
            combined.push(existing);
        }
        //# Stage the combined tool roster
        for (let input in tools) {
            combined.push(input);
        }
        //# Validate before replacing live registry state
        let replacement = ToolRegistry.new(combined);
        self.roster = replacement.roster;
        self.version = self.version + 1;
        null
    }

    function contains(self, name: string) -> bool throws never {
        self.roster
            .filter((tool: Tool) -> bool {
                tool.name == name
            })
            .length()
            == 1
    }

    function get(self, name: string) -> Tool? throws never {
        self.roster.find((tool: Tool) -> bool {
            tool.name == name
        })
    }

    function replace(self, input: ToolInput) -> bool throws never {
        let tool = normalize_tool(input);
        let replaced = false;
        //# Replace the matching tool without reordering
        self.roster
            = self.roster.map((existing: Tool) -> Tool {
                if (existing.name == tool.name) {
                    replaced = true;
                    tool
                } else {
                    existing
                }
            });
        if (replaced) {
            self.version = self.version + 1;
        }
        replaced
    }

    function replace_all(self, tools: ToolInput[]) -> null throws ai.Failure {
        //# Validate the complete replacement roster
        let replacement = ToolRegistry.new(tools);
        self.roster = replacement.roster;
        self.version = self.version + 1;
        null
    }

    function remove(self, name: string) -> bool throws never {
        let before = self.roster.length();
        self.roster
            = self.roster.filter((tool: Tool) -> bool {
                tool.name != name
            });
        let removed = self.roster.length() != before;
        if (removed) {
            self.version = self.version + 1;
        }
        removed
    }

    function clear(self) -> null throws never {
        if (self.roster.length() > 0) {
            self.roster = [];
            self.version = self.version + 1;
        }
        null
    }

    function snapshot(self) -> Tool[] throws never {
        self.roster.slice(0, self.roster.length())
    }
}
