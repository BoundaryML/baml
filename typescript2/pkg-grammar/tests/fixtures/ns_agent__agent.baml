// bamlcode — a tiny Claude Code, written entirely in BAML.
//
// An agentic coding CLI: it reasons about your task, then takes ONE tool action
// at a time (read / write / edit files, list dirs, run shell commands), feeds the
// result back to the model, and loops until it answers you.
//
//   Interactive REPL:   baml run agent.main
//   One-shot:           baml run agent.ask --json-args '{"task": "summarize this repo"}'

// ---------------------------------------------------------------------------
// The model's decision for a single step.
//
// `action` selects a tool; the matching fields below carry its arguments.
// Only fill the fields relevant to the chosen action.
// ---------------------------------------------------------------------------
class Step {
    thought: string,
    action: "read_file" | "write_file" | "edit_file" | "list_dir" | "run_bash" | "respond",
    path: string?,
    content: string?,
    old_str: string?,
    new_str: string?,
    command: string?,
    message: string?,
}

class Turn {
    answer: string,
    transcript: string,
    steps: int,
    // Set when the user cancelled mid-turn with esc + enter. `queued` holds
    // any lines they typed while the turn ran, to run after it.
    interrupted: bool,
    queued: string[],
}

// The result of peeking at stdin during a turn: esc + enter requests a
// cancel; any other typed line is staged in `queued` to run afterwards.
class Interrupt {
    cancel: bool,
    queued: string,
}

// ---------------------------------------------------------------------------
// The brain: given the running transcript, decide the next single step.
// ---------------------------------------------------------------------------
function decide(transcript: string) -> Step {
    client: Brain
    prompt: #"
        You are bamlcode, a CLI coding agent (think "Claude Code") operating inside the
        user's current working directory. You complete software tasks by taking ONE tool
        action at a time. After each action you will see its result and decide what to do next.

        Available actions (set `action` and fill ONLY the matching fields):
          - read_file   → read a file.                fields: path
          - write_file  → create or overwrite a file. fields: path, content
          - edit_file   → replace exact text once.    fields: path, old_str, new_str
          - list_dir    → list a directory.           fields: path
          - run_bash    → run a /bin/sh command.      fields: command
          - respond     → talk to the user (you are done, or you need their input). fields: message

        Operating rules:
          - Inspect before you change: use list_dir / read_file before write_file / edit_file.
          - edit_file replaces the FIRST exact occurrence of old_str, so make old_str unique
            and copy it byte-for-byte from the file (including whitespace).
          - Use run_bash for things tools don't cover (grep, running tests, git, mkdir, etc.).
          - Keep `thought` to one short sentence.
          - When the task is complete, or you need a decision from the user, use `respond`.
          - Do NOT repeat an action that already succeeded; build on prior results.

        Transcript so far (USER messages, your THOUGHT/ACTION/RESULT trail):
        ----------------------------------------------------------------------
        {{ transcript }}
        ----------------------------------------------------------------------

        Decide the single next step.
        {{ ctx.output_format }}
    "#
}

// ---------------------------------------------------------------------------
// Tools — each returns a plain-text result the model can read.
// ---------------------------------------------------------------------------
function tool_read_file(path: string) -> string {
    if (path.trim().length() == 0) {
        return "ERROR: read_file needs a `path`.";
    };
    if (!baml.fs.exists(path)) {
        return "ERROR: no such file: " + path;
    };
    let body = baml.fs.read(path);
    let numbered = with_line_numbers(body);
    "Contents of " + path + ":\n" + numbered
}

function tool_write_file(path: string, content: string) -> string {
    if (path.trim().length() == 0) {
        return "ERROR: write_file needs a `path`.";
    };
    let n = baml.fs.write(path, content);
    "Wrote " + baml.unstable.string(n) + " bytes to " + path + "."
}

function tool_edit_file(path: string, old_str: string, new_str: string) -> string {
    if (!baml.fs.exists(path)) {
        return "ERROR: no such file: " + path;
    };
    let original = baml.fs.read(path);
    if (!original.includes(old_str)) {
        return "ERROR: old_str was not found in " + path + ". Read the file again and copy the exact text.";
    };
    let updated = original.replace(old_str, new_str);
    baml.fs.write(path, updated);
    "Edited " + path + " (replaced 1 occurrence)."
}

function tool_list_dir(path: string) -> string {
    let dir = if (path.trim().length() == 0) {
        "."
    } else {
        path
    };
    if (!baml.fs.exists(dir)) {
        return "ERROR: no such directory: " + dir;
    };
    let entries = baml.fs.read_dir(dir);
    let lines: string[] = [];
    for (let e in entries) {
        let mark = if (e.is_dir) {
            "/"
        } else {
            ""
        };
        lines.push("  " + e.name + mark);
    }
    if (lines.length() == 0) {
        return dir + " is empty.";
    };
    "Contents of " + dir + ":\n" + lines.join("\n")
}

function tool_run_bash(command: string) -> string {
    if (command.trim().length() == 0) {
        return "ERROR: run_bash needs a `command`.";
    };
    let out = baml.sys.shell(command, null);
    let stdout = out.stdout.to_string();
    let stderr = out.stderr.to_string();
    let header = "exit code: " + baml.unstable.string(out.exit_code);
    let with_out = if (stdout.trim().length() > 0) {
        header + "\n--- stdout ---\n" + stdout
    } else {
        header
    };
    if (stderr.trim().length() > 0) {
        with_out + "\n--- stderr ---\n" + stderr
    } else {
        with_out
    }
}

// Dispatch a decided Step to its tool, returning the tool result text.
function execute(step: Step) -> string {
    match (step.action) {
        "read_file" => tool_read_file(step.path ?? ""),
        "write_file" => tool_write_file(step.path ?? "", step.content ?? ""),
        "edit_file" => tool_edit_file(step.path ?? "", step.old_str ?? "", step.new_str ?? ""),
        "list_dir" => tool_list_dir(step.path ?? ""),
        "run_bash" => tool_run_bash(step.command ?? ""),
        "respond" => step.message ?? "",
    }
}

// Convenience constructors (handy in tests and host code).
function respond_step(message: string) -> Step {
    Step {
        thought: "",
        action: "respond",
        path: null,
        content: null,
        old_str: null,
        new_str: null,
        command: null,
        message: message,
    }
}

function bash_step(command: string) -> Step {
    Step {
        thought: "",
        action: "run_bash",
        path: null,
        content: null,
        old_str: null,
        new_str: null,
        command: command,
        message: null,
    }
}

// A short, human-readable label for a step's tool call.
function describe_call(step: Step) -> string {
    match (step.action) {
        "read_file" => "read " + (step.path ?? "?"),
        "write_file" => {
            "write "
                + (step.path ?? "?")
                + " ("
                + baml.unstable.string((step.content ?? "").length())
                + " chars)"
        },
        "edit_file" => "edit " + (step.path ?? "?"),
        "list_dir" => "ls " + (step.path ?? "."),
        "run_bash" => "$ " + (step.command ?? "?"),
        "respond" => "respond",
    }
}

// ---------------------------------------------------------------------------
// The agent loop for one user turn: decide → act → observe, until `respond`.
// Streams progress to the terminal and returns the answer + updated transcript.
// ---------------------------------------------------------------------------
function run_turn(history: string, user_msg: string) -> Turn {
    let transcript = history + "\nUSER: " + user_msg + "\n";
    let steps = 0;
    let answer = "";
    let max_steps = 30;

    while (steps < max_steps) {
        // Pure agent loop. Interruption is handled concurrently by the
        // supervisor that spawns this turn (see run_supervised), which can
        // cancel() the whole task mid-step — no in-loop polling needed.
        steps = steps + 1;
        let step = decide(transcript);

        if (step.action == "respond") {
            answer = step.message ?? "";
            say("\n" + style_assistant(answer) + "\n");
            transcript = transcript + "ASSISTANT: " + answer + "\n";
            return Turn {
                answer: answer,
                transcript: transcript,
                steps: steps,
                interrupted: false,
                queued: [],
            };
        };

        let call = describe_call(step);
        say("  " + dim(step.thought) + "\n");
        say("  " + arrow(call) + "\n");

        let result = execute(step);
        say(style_result(clip(result, 1200)) + "\n");

        transcript
            = transcript
                + "[THOUGHT] "
                + step.thought
                + "\n"
                + "[ACTION] "
                + call
                + "\n"
                + "[RESULT]\n"
                + clip(result, 6000)
                + "\n";
    }

    answer = "(stopped after " + baml.unstable.string(max_steps) + " steps without finishing)";
    say("\n" + style_assistant(answer) + "\n");
    Turn {
        answer: answer,
        transcript: transcript + "ASSISTANT: " + answer + "\n",
        steps: steps,
        interrupted: false,
        queued: [],
    }
}

// Build the Turn returned when a turn is cancelled, and tell the user.
function interrupt_turn(transcript: string, steps: int) -> Turn {
    let note = "⨂ interrupted.";
    say("\n" + root.ansi.yellow(note) + "\n");
    Turn {
        answer: note,
        transcript: transcript + "[INTERRUPTED by user]\n",
        steps: steps,
        interrupted: true,
        queued: [],
    }
}

// Peek at stdin (non-blocking, ~0.1s) for input. Esc + enter cancels the
// turn; any other line is staged to run after it.
function poll_interrupt() -> Interrupt {
    let out = baml.sys.shell("IFS= read -r -t 0.1 __l; rc=$?; printf '%s' \"$__l\"; exit $rc", null);
    if (out.exit_code != 0) {
        return Interrupt { cancel: false, queued: "" };
    };
    classify_interrupt(out.stdout.to_string())
}

// Classify a line typed during a turn: esc + enter (the line carries the raw
// ESC byte — trim() leaves it intact) cancels the turn; any other text is
// staged to run after it. A bare empty enter is ignored.
function classify_interrupt(raw: string) -> Interrupt {
    let line = raw.trim();
    if (line.includes(root.ansi.esc())) {
        Interrupt { cancel: true, queued: "" }
    } else {
        Interrupt { cancel: false, queued: line }
    }
}

// Run one turn concurrently and watch stdin. The agent turn runs in a spawned
// task (true parallelism — its blocking LLM/tool calls run alongside us),
// while we poll stdin. Pressing esc + enter cancels the future *mid-step* —
// even during an in-flight LLM call; any other line queues to run after the
// turn (returned on Turn.queued) without disturbing it.
function run_supervised(history: string, msg: string) -> Turn {
    let fut = spawn {
        run_turn(history, msg)
    };

    let cancelled = false;
    let queued: string[] = [];
    let watching = true;
    while (watching) {
        if (fut.is_settled()) {
            watching = false;
        } else {
            let probe = poll_interrupt();
            if (probe.cancel) {
                fut.cancel();
                cancelled = true;
                watching = false;
            } else if (probe.queued.length() > 0) {
                queued.push(probe.queued);
                say(
                    "\n  "
                        + root
                            .ansi
                            .yellow(
                                "✚ queued for after this turn ("
                                    + baml.unstable.string(queued.length())
                                    + " waiting): "
                                    + probe.queued,
                            )
                        + "\n",
                );
            };
        };
    }

    let turn = if (cancelled) {
        interrupt_turn(history, 0)
    } else {
        (await fut) catch (e) {
            _ => Turn {
                answer: "(the turn failed — see the error above)",
                transcript: history + "\n[turn errored]\n",
                steps: 0,
                interrupted: false,
                queued: [],
            }
        }
    };
    Turn {
        answer: turn.answer,
        transcript: turn.transcript,
        steps: turn.steps,
        interrupted: turn.interrupted,
        queued: queued,
    }
}

// ---------------------------------------------------------------------------
// Entry points
// ---------------------------------------------------------------------------

// Interactive REPL. Run with: baml run agent.main
//
// The message queue is just an in-memory array of strings, owned by this loop.
// Stage messages with /queue, then run them in order. (You can also type-ahead
// while a turn runs — the terminal buffers your keystrokes, and the next
// read_line picks each line up as its own turn. No queue file involved.)
function main() -> string {
    say(banner());
    let history = "";
    let queue: string[] = [];
    let running = true;

    while (running) {
        say(prompt_label(queue));
        let line = read_line();
        if (line.eof) {
            say("\n");
            running = false;
        } else {
            let msg = line.text.trim();
            if (msg == "exit" || msg == "quit" || msg == "/exit") {
                running = false;
            } else if (msg == "/queue" || msg == "/q") {
                say(show_queue(queue));
            } else if (msg == "/clear") {
                queue = [];
                say("  queue cleared.\n");
            } else if (msg.starts_with("/queue ") || msg.starts_with("/q ")) {
                queue.push(command_arg(msg));
                say(
                    "  ✚ queued — "
                        + baml.unstable.string(queue.length())
                        + " waiting. Queue more, or press enter to run them.\n",
                );
            } else if (msg.length() == 0) {
                // Empty line = "run whatever is queued now".
                if (queue.length() == 0) {
                    say("  (nothing queued)\n");
                } else {
                    history = run_queue(history, queue);
                    queue = [];
                };
            } else {
                // Run the typed task now, then drain anything staged with /queue
                // or typed while it ran. (Press esc + enter mid-turn to cancel;
                //  any other typed line queues to run after the turn.)
                let turn = run_supervised(history, msg);
                history = turn.transcript;
                for (let q in turn.queued) {
                    queue.push(q);
                }
                // If the user cancelled, leave the staged queue for later.
                if (!turn.interrupted && queue.length() > 0) {
                    history = run_queue(history, queue);
                    queue = [];
                };
            };
        };
    }

    say("\nbye 👋\n");
    "session ended"
}

// Run every queued message in order, threading history through each turn.
// Lines typed while a queued turn runs join the back of the same batch.
function run_queue(history: string, queue: string[]) -> string {
    let h = history;
    let i = 0;
    while (i < queue.length()) {
        let m = queue[i];
        i = i + 1;
        say(
            root.ansi
                .magenta(
                    "\n◆ queued ("
                        + baml.unstable.string(i)
                        + "/"
                        + baml.unstable.string(queue.length())
                        + ") › "
                        + m,
                )
                + "\n",
        );
        let t = run_supervised(h, m);
        h = t.transcript;
        for (let q in t.queued) {
            queue.push(q);
        }
        if (t.interrupted) {
            let left = queue.length() - i;
            say(
                root.ansi
                    .yellow(
                        "⨂ cancelled — skipped "
                            + baml.unstable.string(left)
                            + " queued message(s).",
                    )
                    + "\n",
            );
            return h;
        };
    }
    h
}

// One-shot mode for scripting/tests. Returns just the final answer.
// baml run agent.ask --json-args '{"task": "..."}'
function ask(task: string) -> string {
    let turn = run_turn("", task);
    turn.answer
}

// ---------------------------------------------------------------------------
// In-memory queue helpers
// ---------------------------------------------------------------------------

// The prompt, annotated with how many messages are staged.
function prompt_label(queue: string[]) -> string {
    if (queue.length() > 0) {
        "\n"
            + root.ansi.bold("you")
            + root.ansi.gray(" (" + baml.unstable.string(queue.length()) + " queued)")
            + root.ansi.bold(" › ")
    } else {
        "\n" + root.ansi.bold("you › ")
    }
}

// Pretty-print the staged queue.
function show_queue(queue: string[]) -> string {
    if (queue.length() == 0) {
        "  (queue empty)\n"
    } else {
        let lines: string[] = [];
        let i = 0;
        for (let m in queue) {
            i = i + 1;
            lines.push("  " + root.ansi.cyan(baml.unstable.string(i) + ".") + " " + m);
        }
        root.ansi.gray("  queued messages:") + "\n" + lines.join("\n") + "\n"
    }
}

// Strip the leading /queue or /q token from a command line.
function command_arg(msg: string) -> string {
    if (msg.starts_with("/queue ")) {
        msg.substring(7, msg.length()).trim()
    } else if (msg.starts_with("/q ")) {
        msg.substring(3, msg.length()).trim()
    } else {
        msg.trim()
    }
}

// ---------------------------------------------------------------------------
// Terminal I/O + presentation helpers
// ---------------------------------------------------------------------------
class Line {
    text: string,
    eof: bool,
}

// Read one line from the terminal (inherited stdin). eof=true at end of input.
function read_line() -> Line {
    let out = baml.sys.shell("IFS= read -r __l; rc=$?; printf '%s' \"$__l\"; exit $rc", null);
    Line { text: out.stdout.to_string(), eof: out.exit_code != 0 }
}

// Write directly to the controlling terminal so output streams live
// (baml captures normal stdout, so we bypass it via /dev/tty).
function say(text: string) -> void {
    let opts = baml.sys.ProcessOptions { cwd: null, env: null, timeout_ms: null, stdin: text };
    baml.sys.shell("cat > /dev/tty 2>/dev/null", opts);
}

function banner() -> string {
    root.ansi
        .cyan(
            "\n  ┌────────────────────────────────────────────┐\n"
                + "  │   bamlcode · a tiny Claude Code, in BAML     │\n"
                + "  └────────────────────────────────────────────┘\n",
        )
        + root.ansi.gray("  Ask me to read, edit, or build code in this dir.\n\n")
        + "  "
        + root.ansi.yellow("type a task")
        + root.ansi.gray("       run it now\n")
        + "  "
        + root.ansi.yellow("/queue <task>")
        + root.ansi.gray("     stage a task (then enter to run)\n")
        + "  "
        + root.ansi.yellow("/queue")
        + root.ansi.gray("            show the queue   ")
        + root.ansi.yellow("/clear")
        + root.ansi.gray("  empty it\n")
        + "  "
        + root.ansi.yellow("enter")
        + root.ansi.gray("             run the queue now\n")
        + "  "
        + root.ansi.yellow("exit")
        + root.ansi.gray("              quit\n\n")
        + root.ansi.gray("  while I'm working, press ")
        + root.ansi.yellow("esc + enter")
        + root.ansi.gray(" to stop — or type a new task to queue it for after.\n")
}

function style_assistant(msg: string) -> string {
    root.ansi.bold(root.ansi.green("● bamlcode")) + "\n" + indent(msg)
}

// A dim "· thought" line.
function dim(s: string) -> string {
    root.ansi.gray("· " + s)
}

// A cyan "→ tool call" line.
function arrow(s: string) -> string {
    root.ansi.cyan("→ " + s)
}

// Tool result: dim by default, red when it's an error.
function style_result(result: string) -> string {
    let body = indent(result);
    if (result.starts_with("ERROR")) {
        root.ansi.red(body)
    } else {
        root.ansi.gray(body)
    }
}

function indent(s: string) -> string {
    let lines = s.lines();
    let out: string[] = [];
    for (let l in lines) {
        out.push("    " + l);
    }
    out.join("\n")
}

function with_line_numbers(body: string) -> string {
    let lines = body.lines();
    let out: string[] = [];
    let i = 0;
    for (let l in lines) {
        i = i + 1;
        out.push(baml.unstable.string(i) + "\t" + l);
    }
    out.join("\n")
}

function clip(text: string, n: int) -> string {
    if (text.length() > n) {
        text.substring(0, n)
            + "\n… ("
            + baml.unstable.string(text.length() - n)
            + " more chars truncated)"
    } else {
        text
    }
}
