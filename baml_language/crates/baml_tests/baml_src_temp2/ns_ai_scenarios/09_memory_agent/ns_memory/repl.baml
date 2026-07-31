// The interactive session: REPL command loop, message queue, and the
// supervisor that watches the terminal for interrupts while a turn runs.

function env_task() -> string {
    baml.env.get("BAML_MEMORY_AGENT_TASK")
        ?? "Summarize this repository and report the most important entry points."
}

// One non-interactive turn; the completed turn is still offered to the curator.
function ask(task: string, backend: Backend) -> string {
    let turn = run_turn("", null, task, backend);
    if (enabled()) {
        let _ = curate_turn(turn.transcript, default_store(), backend);
    };
    turn.answer
}

function _report_curation(report: EditReport) -> null {
    if (report.applied.length() > 0) {
        baml.io.print("  " + _gray(`⛁ memory ${report.applied.join(" ")}`) + "\n");
    };
    if (report.rejected.length() > 0) {
        baml.io.print("  " + _yellow(`⛁ memory edits rejected: ${report.rejected.join("; ")}`) + "\n");
    };
    null
}

function forget_command(name: string, store: Store = default_store()) -> string {
    if (name.trim().length() == 0) {
        return "ERROR: forget needs a `memory_name`.";
    };
    if (store.forget(name)) {
        `Forgot "${name}".`
    } else {
        `ERROR: no memory named "${name}". Use /memory to see what's stored.`
    }
}

function run_supervised(
    history: string,
    continuation: ai.run.AgentSession<string>?,
    message: string,
    backend: Backend,
) -> SupervisedTurn {
    //# Request a resumable checkpoint while the application watches terminal input
    let cancel = baml.spawn.CancelToken.new();
    let future = spawn {
        run_turn(history, continuation, message, backend, cancel = cancel)
    };
    let cancellation_requested = false;
    let queued: string[] = [];
    let watching = true;
    while (watching) {
        if (future.is_settled()) {
            watching = false;
        } else {
            let probe = _poll_interrupt();
            if (probe.cancel) {
                if (!cancellation_requested) {
                    let _ = cancel.cancel();
                    cancellation_requested = true;
                    baml.io.print(
                        "\n"
                            + _yellow("⨂ stopping after the current model/tool checkpoint…")
                            + "\n",
                    );
                };
            } else if (probe.queued.length() > 0) {
                queued.push(probe.queued);
                baml.io.print(
                    "\n  "
                        + _yellow(
                            `✚ queued for after this turn (${queued.length()} waiting): ${probe.queued}`,
                        )
                        + "\n",
                );
            };
        };
    }
    let turn = (await future) catch_all (error) {
        _ => {
            baml.io.print("\n" + _red(`turn failed: ${error}`) + "\n");
            AgentTurn {
                answer: "(the turn failed — see the error above)",
                transcript: history + "\n[turn errored]\n",
                continuation: continuation,
                steps: 0,
                interrupted: false,
            }
        },
    };
    //# Curate only completed turns in an isolated Task
    if (!turn.interrupted && enabled()) {
        let report = curate_turn(_turn_delta(history, turn.transcript), default_store(), backend);
        let _ = _report_curation(report);
    };
    SupervisedTurn { turn: turn, queued: queued }
}

function run_queue(session: Session, queue: string[], backend: Backend) -> Session {
    let transcript = session.transcript;
    let continuation = session.continuation;
    let index = 0;
    while (index < queue.length()) {
        let message = queue[index];
        index = index + 1;
        baml.io.print("\n" + _magenta(`◆ queued (${index}/${queue.length()}) › ${message}`) + "\n");
        let supervised = run_supervised(transcript, continuation, message, backend);
        transcript = supervised.turn.transcript;
        continuation = supervised.turn.continuation;
        for (let typed in supervised.queued) {
            queue.push(typed);
        }
        if (supervised.turn.interrupted) {
            let remaining = queue.length() - index;
            baml.io.print(_yellow(`⨂ cancelled — skipped ${remaining} queued message(s).`) + "\n");
            return Session { transcript: transcript, continuation: continuation };
        };
    }
    Session { transcript: transcript, continuation: continuation }
}

function repl(backend: Backend) -> string {
    let provider_label = match (backend) {
        Backend.OpenAI => "OpenAI Responses",
        Backend.Google => "Google AI Gemini",
    };
    baml.io.print(_banner(provider_label));
    let session = Session { transcript: "", continuation: null };
    let queue: string[] = [];
    let running = true;
    while (running) {
        //# Read and classify the next application command
        baml.io.print(_prompt_label(queue));
        let line = _read_line();
        if (line.eof) {
            baml.io.print("\n");
            running = false;
        } else {
            let message = line.text.trim();
            match (message) {
                "exit" | "quit" | "/exit" => {
                    running = false;
                },
                "/queue" | "/q" => {
                    baml.io.print(_show_queue(queue));
                },
                "/memory" | "/mem" => {
                    let listing = default_store().listing() catch_all (error) {
                        _ => `  ERROR: could not list memory: ${error}` + "\n",
                    };
                    baml.io.print(listing);
                },
                let command if command.starts_with("/forget ") => {
                    let result = forget_command(command.substring(8, command.length()).trim()) catch_all (error) {
                        _ => `ERROR: could not forget memory: ${error}`,
                    };
                    baml.io.print("  " + result + "\n");
                },
                "/clear" => {
                    queue = [];
                    baml.io.print("  queue cleared.\n");
                },
                let command if command.starts_with("/queue ") || command.starts_with("/q ") => {
                    queue.push(_command_arg(command));
                    baml.io.print(
                        `  ✚ queued — ${queue.length()} waiting. Queue more, or press enter to run them.` + "\n",
                    );
                },
                "" => {
                    if (queue.length() == 0) {
                        baml.io.print("  (nothing queued)\n");
                    } else {
                        session = run_queue(session, queue, backend);
                        queue = [];
                    };
                },
                let immediate => {
                    //# Run now, then drain type-ahead and staged work
                    let supervised = run_supervised(
                        session.transcript,
                        session.continuation,
                        immediate,
                        backend,
                    );
                    session = Session {
                        transcript: supervised.turn.transcript,
                        continuation: supervised.turn.continuation,
                    };
                    for (let typed in supervised.queued) {
                        queue.push(typed);
                    }
                    if (!supervised.turn.interrupted && queue.length() > 0) {
                        session = run_queue(session, queue, backend);
                        queue = [];
                    };
                },
            };
        };
    }
    baml.io.print("\nbye 👋\n");
    "session ended"
}
