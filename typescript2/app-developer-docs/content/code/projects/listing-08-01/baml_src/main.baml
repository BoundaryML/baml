class ToolUnavailable {
    tool: string,
    retry_after_ms: int,
}

// docs:start excerpt-01
// docs:start excerpt-15
class BadToolInput {
    tool: string,
    message: string,
}
// docs:end excerpt-15

function check_tool_name(tool: string) -> string {
    if (tool == "") {
        throw BadToolInput { tool, message: "tool name is empty" };
    }
    tool
}
// docs:end excerpt-01



// docs:start excerpt-02
function display_tool_name(tool: string) -> string throws never {
    `Tool: ${check_tool_name(tool)}` catch (e) {
        BadToolInput => e.message,
    }
}
// docs:end excerpt-02



// docs:start excerpt-04
function invoke_tool(tool: string) -> string {
    if (tool == "") {
        throw BadToolInput { tool, message: "tool name is empty" };
    }
    if (int.random(0, 100) < 20) {
        throw ToolUnavailable { tool, retry_after_ms: 500 };
    }
    `result from ${tool}`
}
// docs:end excerpt-04



// docs:start excerpt-07
function run_step(tool: string) -> string throws never {
    invoke_tool(tool) catch_all (e) {
        ToolUnavailable => `retry ${e.tool} in ${e.retry_after_ms} ms`,
        BadToolInput => `invalid tool input: ${e.message}`,
        baml.errors.InvalidArgument => e.message,
    }
}
// docs:end excerpt-07



// docs:start excerpt-08
function run_step_with_fallback(tool: string) -> string throws never {
    invoke_tool(tool) catch_all (e) {
        BadToolInput => `invalid tool input: ${e.message}`,
        _ => "tool failed",
    }
}
// docs:end excerpt-08



// docs:start excerpt-06
function handle_bad_input(
    tool: string,
) -> string throws ToolUnavailable | baml.errors.InvalidArgument {
    invoke_tool(tool) catch (e) {
        BadToolInput => `invalid tool input: ${e.message}`,
    }
}
// docs:end excerpt-06



// docs:start excerpt-14
function with_context() -> string throws never {
    invoke_tool("search") catch (e, tb) {
        ToolUnavailable => `${e.tool}: ${tb.stack_trace.frames.length()} frame(s)`,
        BadToolInput => `${e.tool}: ${tb.stack_trace.frames.length()} frame(s)`,
        baml.errors.InvalidArgument => e.message,
    }
}
// docs:end excerpt-14



function fail_with_error() -> string {
    throw BadToolInput { tool: "search", message: "query is empty" }
}

function catch_every_error() -> string {
    fail_with_error() catch_all (e) {
        _ => "caught error",
    }
}

function catch_all_with_context() -> string {
    fail_with_error() catch_all (e, tb) {
        _ => tb.error.to_string(),
    }
}


// docs:start excerpt-10
function load_tool_config(tool: string) -> string {
    if (tool == "") {
        throw BadToolInput { tool, message: "tool name is empty" };
    }
    baml.sys.panic(`TODO: load config for ${tool}`)
}
// docs:end excerpt-10


// docs:start excerpt-12
function handle_web_request(tool: string) -> int {
    {
        load_tool_config(tool);
        200
    } catch_all_panics (e) {
        _ => 500,
    }
}
// docs:end excerpt-12


// docs:start excerpt-13
function run_step_with_logging(tool: string) -> string throws never {
    invoke_tool(tool) catch_all (e) {
        _ => {
            log.error(e);
            "tool failed"
        },
    }
}
// docs:end excerpt-13


function handler_kinds() -> string[] {
    [
        catch_every_error(),
        handle_web_request("search").to_string(),
        run_step_with_logging("search"),
        with_context(),
    ]
}

// docs:start excerpt-11
function panic_escapes_catch_all(tool: string) -> string {
    load_tool_config(tool) catch_all (e) {
        _ => "caught error",
    }
}
// docs:end excerpt-11



// docs:start excerpt-09
function invoke_tool_checked(
    tool: string,
) -> string throws ToolUnavailable | BadToolInput | baml.errors.InvalidArgument {
    invoke_tool(tool)
}
// docs:end excerpt-09



// docs:start excerpt-16
function define_validator() -> string throws never {
    let validate = (score: int) -> int {
        if (score < 0) {
            throw BadToolInput { tool: "rank", message: "score is negative" };
        }
        score
    };
    "validator ready"
}
// docs:end excerpt-16


// docs:start excerpt-17
function apply_validator(validate: (score: int) -> int, score: int) -> int {
    validate(score)
}

function validate_score(score: int) -> int {
    apply_validator(
        (value: int) -> int {
            if (value < 0) {
                throw BadToolInput { tool: "rank", message: "score is negative" };
            }
            value
        },
        score,
    )
}
// docs:end excerpt-17


function lambda_examples() -> string[] throws never {
    [
        define_validator(),
        validate_score(-1).to_string() catch (e) {
            BadToolInput => e.message,
        },
    ]
}


// docs:start excerpt-05
function uncaught() -> string {
    invoke_tool("search")
}
// docs:end excerpt-05



function catch_all_panics_with_context(tool: string) -> int {
    {
        load_tool_config(tool);
        200
    } catch_all_panics (e, tb) {
        _ => 500,
    }
}

function main() -> string[] {
    [run_step("search"), run_step(""), run_step("search")]
}

// docs:start excerpt-propagation
function checked_tool_name(tool: string) -> string {
    `Tool: ${check_tool_name(tool)}`
}
// docs:end excerpt-propagation
