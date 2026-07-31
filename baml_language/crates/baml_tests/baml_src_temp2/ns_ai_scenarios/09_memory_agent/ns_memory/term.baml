// Terminal plumbing: line input, interrupt polling, ANSI styling, and the
// pretty-printed forms of tool calls and results.

class Line {
    text: string,
    eof: bool,
}

class Interrupt {
    cancel: bool,
    queued: string,
}

// baml.io.input cannot distinguish EOF from an empty line, and an empty line
// means "run the queue" in this REPL — so read via the shell, where EOF is a
// nonzero exit code.
function _read_line() -> Line {
    let output = baml
        .sys
        .shell(
            "IFS= read -r _memory_agent_line; rc=$?; printf '%s' \"$_memory_agent_line\"; exit $rc",
            null,
        ) catch_all (error) {
        _ => {
            return Line { text: "", eof: true };
        },
    };
    Line { text: output.stdout.to_string(), eof: output.exit_code != 0 }
}

// Non-blocking peek at stdin while a turn is running; `read -t 0.1` doubles as
// the poll interval.
function _poll_interrupt() -> Interrupt {
    let output = baml
        .sys
        .shell(
            "IFS= read -r -t 0.1 _memory_agent_line; rc=$?; printf '%s' \"$_memory_agent_line\"; exit $rc",
            null,
        ) catch_all (error) {
        _ => {
            return Interrupt { cancel: false, queued: "" };
        },
    };
    if (output.exit_code != 0) {
        return Interrupt { cancel: false, queued: "" };
    };
    _classify_interrupt(output.stdout.to_string())
}

function _classify_interrupt(raw: string) -> Interrupt {
    let line = raw.trim();
    if (line.includes(_escape())) {
        Interrupt { cancel: true, queued: "" }
    } else {
        Interrupt { cancel: false, queued: line }
    }
}

function _command_arg(message: string) -> string {
    if (message.starts_with("/queue ")) {
        message.substring(7, message.length()).trim()
    } else if (message.starts_with("/q ")) {
        message.substring(3, message.length()).trim()
    } else {
        message.trim()
    }
}

function _prompt_label(queue: string[]) -> string {
    if (queue.length() > 0) {
        "\n" + _bold("you") + _gray(` (${queue.length()} queued)`) + _bold(" › ")
    } else {
        "\n" + _bold("you › ")
    }
}

function _show_queue(queue: string[]) -> string {
    if (queue.length() == 0) {
        return "  (queue empty)\n";
    };
    let lines: string[] = [];
    let index = 0;
    for (let message in queue) {
        index = index + 1;
        lines.push("  " + _cyan(`${index}.`) + " " + message);
    }
    _gray("  queued messages:") + "\n" + lines.join("\n") + "\n"
}

function _call_summary(call: ai.tools.ToolCall) -> string {
    baml.json.path<string>(call.args, ".summary") catch_all (error) {
        _ => ""
    }
}

function _describe_call(call: ai.tools.ToolCall) -> string {
    match (call.name) {
        "read_file" => {
            "read "
                + (
                    baml.json.path<string>(call.args, ".path") catch_all (error) {
                        _ => "?"
                    }
                )
        },
        "write_file" => {
            let path = baml.json.path<string>(call.args, ".path") catch_all (error) {
                _ => "?"
            };
            let content = baml.json.path<string>(call.args, ".content") catch_all (error) {
                _ => ""
            };
            `write ${path} (${content.length()} chars)`
        },
        "edit_file" => {
            "edit "
                + (
                    baml.json.path<string>(call.args, ".path") catch_all (error) {
                        _ => "?"
                    }
                )
        },
        "list_dir" => {
            "ls "
                + (
                    baml.json.path<string>(call.args, ".path") catch_all (error) {
                        _ => "."
                    }
                )
        },
        "run_bash" => {
            "$ "
                + (
                    baml.json.path<string>(call.args, ".command") catch_all (error) {
                        _ => "?"
                    }
                )
        },
        let other => other,
    }
}

// A ToolOk's output is JSON — usually a bare string from these tools — while a
// ToolError already carries the model-facing failure message.
function _render_tool_result(result: ai.tools.ToolResult) -> string {
    match (result) {
        let ok: ai.tools.ToolOk => {
            baml.json.from_json<string>(ok.output) catch_all (decode_error) {
                _ => baml.json.stringify(ok.output),
            }
        },
        let failed: ai.tools.ToolError => failed.message,
    }
}

function _color(code: string, text: string) -> string {
    let no_color = baml.env.get("NO_COLOR") catch_all (error) {
        _ => null,
    };
    if (no_color != null) {
        return text;
    };
    let esc = _escape();
    `${esc}[${code}m${text}${esc}[0m`
}

function _escape() -> string {
    string.from_code_points([27]) catch_all (error) {
        _ => ""
    }
}

function _bold(text: string) -> string {
    _color("1", text)
}

function _gray(text: string) -> string {
    _color("90", text)
}

function _cyan(text: string) -> string {
    _color("36", text)
}

function _yellow(text: string) -> string {
    _color("33", text)
}

function _magenta(text: string) -> string {
    _color("35", text)
}

function _green(text: string) -> string {
    _color("32", text)
}

function _red(text: string) -> string {
    _color("31", text)
}

function _style_assistant(message: string) -> string {
    _bold(_green("● memory agent")) + "\n" + _indent(message)
}

function _dim(summary: string) -> string {
    _gray("· " + summary)
}

function _arrow(label: string) -> string {
    _cyan("→ " + label)
}

function _style_result(result: string, is_error: bool) -> string {
    let body = _indent(result);
    if (is_error) {
        _red(body)
    } else {
        _gray(body)
    }
}

function _indent(text: string) -> string {
    let output: string[] = [];
    for (let line in text.lines()) {
        output.push("    " + line);
    }
    output.join("\n")
}

function _banner(provider_label: string) -> string {
    _cyan(
        "\n  ┌────────────────────────────────────────────┐\n"
            + "  │  memory agent · coding agent in BAML       │\n"
            + "  └────────────────────────────────────────────┘\n",
    )
        // Newlines stay outside the backticks: multiline backtick literals are
        // dedented and trimmed, which would eat the indent and blank line.
        + _gray("  Provider: " + provider_label + ". Working in this process's current directory.\n\n")
        + "  "
        + _yellow("type a task")
        + _gray("       run it now\n")
        + "  "
        + _yellow("/queue <task>")
        + _gray("     stage a task (then enter to run)\n")
        + "  "
        + _yellow("/queue")
        + _gray("            show the queue   ")
        + _yellow("/clear")
        + _gray("  empty it\n")
        + "  "
        + _yellow("enter")
        + _gray("             run the queue now\n")
        + "  "
        + _yellow("/memory")
        + _gray("           list durable facts   ")
        + _yellow("/forget <name>")
        + _gray("  drop one\n")
        + "  "
        + _yellow("exit")
        + _gray("              quit\n\n")
        + _gray("  while working, press ")
        + _yellow("esc + enter")
        + _gray(" to stop, or type a task to queue it.\n")
        + _yellow("  This agent can run shell commands and edit files with your permissions.\n")
}
