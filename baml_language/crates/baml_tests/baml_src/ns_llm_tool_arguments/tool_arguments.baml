// A reflected tool aligns a model's arguments to its handler's parameters.
//
// A provider hands the arguments over as parsed JSON: a class is a map, an
// enum a string, a list a list of json. `reflect.call_any` takes values of the
// parameter types, so `Tool.call` first reads the JSON as one class whose
// fields are the parameters (`baml.sap.coerce`), under the names the tool
// schema shows. Before this, only a tool whose parameters were all scalars
// could be called from a model's output.
//
// Per the corpus convention every case body is a top-level `function` and the
// `test` block only asserts.

enum ToolArgGymType {
    COMMERCIAL
    GARAGE @alias("garage-gym")
}

class ToolArgGym {
    name: string @alias("label"),
    gym_type: ToolArgGymType,
    tags: string[],
    notes: string @skip,
}

class ToolArgPlan {
    title: string,
    gyms: ToolArgGym[],
}

// The same fields as `ToolArgGym`, and a different class.
class ToolArgLookalike {
    name: string,
    gym_type: ToolArgGymType,
    tags: string[],
    notes: string,
}

function tool_arg_kind(kind: ToolArgGymType) -> string {
    kind.to_string()
}

function tool_arg_lists(tags: string[], counts: map<string, int>) -> string {
    `${tags.join("+")}|${counts.get("a") ?? -1}`
}

function tool_arg_gym(gym: ToolArgGym) -> string {
    `${gym.name}|${gym.gym_type.to_string()}|${gym.tags.join("+")}|[${gym.notes}]`
}

function tool_arg_plan(plan: ToolArgPlan) -> string {
    `${plan.title}|${plan.gyms.map((gym) -> { gym.name }).join("+")}`
}

function tool_arg_scalars(q: string, n: int, x: float, on: bool, note: string?) -> string {
    `${q}|${n}|${x}|${on}|${note ?? "none"}`
}

function tool_arg_limit(q: string, limit: int = 10) -> string {
    `${q}|${limit}`
}

function tool_arg_open(n: int, extra: unknown) -> string {
    let written = baml.json.to_string(extra) catch_all (e) {
        _ => "unserializable",
    };
    `${n}|${written.length()}`
}

/// The arguments as a provider hands them over: parsed JSON.
function tool_arg_from_model(text: string) -> map<string, unknown> {
    let parsed = baml.json.parse(text) catch (e) {
        _ => baml.sys.panic("the test arguments are JSON"),
    };
    baml.json.from_json<map<string, unknown>>(parsed) catch (e) {
        _ => baml.sys.panic("the test arguments are an object"),
    }
}

/// What a call of the reflected tool for `handler` returns, or how it fails.
function tool_arg_call(handler: reflect.AnyFunction, args: map<string, unknown>) -> string {
    ai.tools.tool(handler).call(args) catch_all (e) {
        let bad: baml.errors.InvalidArgument => `InvalidArgument: ${bad.message}`,
        _ => "another error",
    }
}

function tool_arg_model_call(handler: reflect.AnyFunction, text: string) -> string {
    tool_arg_call(handler, tool_arg_from_model(text))
}

test "an enum argument is read under the name the schema shows" {
    assert.equal(tool_arg_model_call(tool_arg_kind, `{"kind": "garage-gym"}`), `"GARAGE"`);
    assert.equal(tool_arg_model_call(tool_arg_kind, `{"kind": "COMMERCIAL"}`), `"COMMERCIAL"`)
}

test "list and map arguments take their element types" {
    assert.equal(
        tool_arg_model_call(tool_arg_lists, `{"tags": ["a", "b"], "counts": {"a": 2}}`),
        `"a+b|2"`,
    )
}

test "a class argument is read with its aliases, and its skipped field is not read" {
    assert.equal(
        tool_arg_model_call(
            tool_arg_gym,
            `{"gym": {"label": "Home", "gym_type": "garage-gym", "tags": ["barbell"], "notes": "sent"}}`,
        ),
        `"Home|GARAGE|barbell|[]"`,
    )
}

test "a class argument reaches the classes inside it" {
    assert.equal(
        tool_arg_model_call(
            tool_arg_plan,
            `{"plan": {"title": "Week", "gyms": [{"label": "Home", "gym_type": "garage-gym", "tags": []}, {"label": "Work", "gym_type": "COMMERCIAL", "tags": []}]}}`,
        ),
        `"Week|Home+Work"`,
    )
}

test "scalar arguments keep their values" {
    assert.equal(
        tool_arg_model_call(tool_arg_scalars, `{"q": "hi", "n": 3, "x": 2, "on": true, "note": null}`),
        `"hi|3|2.0|true|none"`,
    )
}

test "a defaulted parameter keeps its default when the model leaves it out" {
    assert.equal(tool_arg_model_call(tool_arg_limit, `{"q": "hi"}`), `"hi|10"`);
    assert.equal(tool_arg_model_call(tool_arg_limit, `{"q": "hi", "limit": 3}`), `"hi|3"`)
}

test "an unknown parameter receives the argument as it arrived" {
    assert.equal(
        tool_arg_model_call(tool_arg_open, `{"n": 7, "extra": {"a": [1, 2]}}`),
        `"7|11"`,
    )
}

// BAML code that calls a tool passes values it already holds. They go to the
// handler as they are: nothing is written out and read back.

function tool_arg_typed_value() -> string {
    let gym = ToolArgGym { name: "Typed", gym_type: ToolArgGymType.GARAGE, tags: ["t"], notes: "kept" };
    tool_arg_call(tool_arg_gym, { "gym": gym })
}

test "an argument that is already a value of its parameter's type is passed as it is" {
    assert.equal(tool_arg_typed_value(), `"Typed|GARAGE|t|[kept]"`)
}

// A value that is not JSON and not of the parameter's type is not converted,
// whatever its shape.

function tool_arg_lookalike() -> string {
    let other = ToolArgLookalike { name: "Other", gym_type: ToolArgGymType.GARAGE, tags: [], notes: "" };
    tool_arg_call(tool_arg_gym, { "gym": other })
}

class ToolArgNode {
    name: string,
    next: ToolArgNode?,
}

function tool_arg_nodes(nodes: ToolArgNode[]) -> string {
    `${nodes.length()}`
}

function tool_arg_untyped_list() -> string {
    let node = ToolArgNode { name: "a", next: null };
    node.next = node;
    let untyped: unknown[] = [node];
    tool_arg_call(tool_arg_nodes, { "nodes": untyped })
}

test "a value of another class is rejected, not converted" {
    assert.is_true(tool_arg_lookalike().starts_with("InvalidArgument: invalid arguments for tool_arg_gym: "))
}

test "a list that is not JSON and not of the parameter's type is rejected" {
    assert.is_true(tool_arg_untyped_list().starts_with("InvalidArgument: invalid arguments for tool_arg_nodes: "))
}

// An argument that does not align is an `InvalidArgument` that names the tool.
// Its text goes back to the model, so it is bounded whatever was sent.

function tool_arg_long_message() -> int {
    let long = "x".repeat(20000);
    tool_arg_call(tool_arg_kind, { "kind": long }).length()
}

test "an argument that does not align is an InvalidArgument" {
    assert.is_true(
        tool_arg_model_call(tool_arg_gym, `{"gym": {"label": "Home", "gym_type": "nowhere", "tags": []}}`)
            .starts_with("InvalidArgument: invalid arguments for tool_arg_gym: "),
    );
    // The schema shows the alias, so the declared name is not read.
    assert.is_true(
        tool_arg_model_call(tool_arg_gym, `{"gym": {"name": "Home", "gym_type": "garage-gym", "tags": []}}`)
            .starts_with("InvalidArgument: invalid arguments for tool_arg_gym: "),
    );
    assert.is_true(
        tool_arg_model_call(tool_arg_gym, `{}`).starts_with("InvalidArgument: invalid arguments for tool_arg_gym: "),
    )
}

test "the message of a failed alignment is bounded" {
    assert.is_true(tool_arg_long_message() < 1200)
}

// The runner's path: a model's tool call reaches a handler that takes a class.

class ToolArgProbe {
    saved: string[],
}

class ToolArgScriptedClient {
    turns: int,

    implements ai.Client {
        function id(self) -> string {
            "tool-arguments-client"
        }

        function render(self, input: ai.ModelTurnInput) -> baml.http.Request {
            let _ = input;
            baml.http.Request { method: "POST", url: "https://example.invalid/tool-arguments", headers: {}, body: "{}" }
        }

        function invoke(self, input: ai.ModelTurnInput) -> ai.ModelTurn {
            let _ = input;
            self.turns += 1;
            let content: ai.content.ModelBlock[] = [];
            if (self.turns == 1) {
                content.push(
                    ai.content.ToolUse {
                        id: "call_1",
                        name: "save_gym",
                        args: tool_arg_from_model(`{"gym": {"label": "Home", "gym_type": "garage-gym", "tags": ["barbell"]}}`),
                    },
                );
            } else {
                content.push(ai.content.Text { text: "saved" });
            }
            ai.ModelTurn { calls: [], content: content, stop_reason: ai.content.StopReason.Complete, usage: null }
        }
    }
}

function ToolArgSaveGym(request: string) -> string {
    client: "openai/gpt-4o-mini"
    prompt: `${request}`
}

function tool_arg_agent_run() -> string {
    let probe = ToolArgProbe { saved: [] };
    let save_gym = (gym: ToolArgGym) -> string {
        probe.saved.push(`${gym.name}:${gym.gym_type.to_string()}`);
        "ok"
    };
    let spec = ToolArgSaveGym@spec("Save my home gym.");
    spec.toolbox = ai.tools.Toolbox.new([ai.tools.tool(save_gym, name = "save_gym", description = "Saves a gym.")]);
    let reply = ai.Agent.new(client = ToolArgScriptedClient { turns: 0 }).run(spec).value;
    `${reply}|${probe.saved.join(",")}`
}

test "the agent runs a tool whose parameter is a class" {
    assert.equal(tool_arg_agent_run(), "saved|Home:GARAGE")
}

// `baml.sap.coerce` is schema-aligned parsing for a value that is already
// parsed: it aligns, and has no text to find or repair.

function tool_arg_coerce_gym() -> string {
    let sent: baml.json.json = { "label": "Home", "gym_type": "garage-gym", "tags": ["barbell"] };
    let gym = baml.sap.coerce<ToolArgGym>(sent) catch (e) {
        baml.errors.ParseError => {
            return "ParseError";
        },
    };
    `${gym.name}|${gym.gym_type.to_string()}|${gym.tags.join("+")}`
}

function tool_arg_coerce_mismatch() -> string {
    let sent: baml.json.json = { "label": "Home", "gym_type": "nowhere", "tags": [] };
    let gym = baml.sap.coerce<ToolArgGym>(sent) catch (e) {
        baml.errors.ParseError => {
            return "ParseError";
        },
    };
    gym.name
}

// The type of a coercion can be data: `unreflect` names it.
function tool_arg_coerce_runtime_type(t: reflect.Type) -> string {
    type T = unreflect(t);
    let sent: baml.json.json = ["a", "b"];
    let aligned = baml.sap.coerce<T>(sent) catch (e) {
        baml.errors.ParseError => {
            return "ParseError";
        },
    };
    if (aligned is string[]) {
        "string[]"
    } else {
        "another type"
    }
}

test "coerce aligns a parsed value to a type" {
    assert.equal(tool_arg_coerce_gym(), "Home|GARAGE|barbell");
    assert.equal(tool_arg_coerce_mismatch(), "ParseError");
    assert.equal(tool_arg_coerce_runtime_type(reflect.Type.of<string[]>()), "string[]")
}

// A type the parser cannot model is a thrown `ParseError` from `coerce`, where
// `parse` panics: a caller that holds its type as data gets an answer.

type ToolArgHandler = (string) -> string throws never;

class ToolArgSkipsImage {
    label: string,
    picture: image @skip,
}

function tool_arg_coerce_unmodelled() -> string {
    [
        tool_arg_coerce_runtime_type(reflect.Type.of<unknown[]>()),
        tool_arg_coerce_runtime_type(reflect.Type.of<uint8array>()),
        tool_arg_coerce_runtime_type(reflect.Type.of<ToolArgHandler>()),
        tool_arg_coerce_runtime_type(reflect.Type.of<ToolArgSkipsImage>()),
    ]
        .join(",")
}

test "coerce throws for a type the parser cannot model" {
    assert.equal(tool_arg_coerce_unmodelled(), "ParseError,ParseError,ParseError,ParseError")
}

// So a tool with such a parameter fails as an `InvalidArgument` when a model
// calls it, and does not panic.

function tool_arg_skips_image(item: ToolArgSkipsImage) -> string {
    item.label
}

test "an argument of a type the parser cannot model is an InvalidArgument, not a panic" {
    assert.is_true(
        tool_arg_model_call(tool_arg_skips_image, `{"item": {"label": "x"}}`)
            .starts_with("InvalidArgument: invalid arguments for tool_arg_skips_image: "),
    )
}

// The parser's integer is wider than an `int`: a number that fits the one and
// not the other does not align, and fails as any other mismatch does.

function tool_arg_coerce_wide_int() -> string {
    let sent = baml.json.parse("4611686018427387904") catch (e) {
        _ => baml.sys.panic("the test value is JSON"),
    };
    let n = baml.sap.coerce<int>(sent) catch (e) {
        baml.errors.ParseError => {
            return "ParseError";
        },
    };
    `${n}`
}

test "an integer too wide for an int does not align" {
    assert.equal(tool_arg_coerce_wide_int(), "ParseError");
    assert.is_true(
        tool_arg_model_call(tool_arg_limit, `{"q": "x", "limit": 4611686018427387904}`)
            .starts_with("InvalidArgument: invalid arguments for tool_arg_limit: "),
    )
}
