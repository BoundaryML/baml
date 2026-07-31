// The coding Task, its providers, and one supervised Agent turn.

enum Backend {
    OpenAI,
    Google,
}

function openai_provider(max_output_tokens: int = 4096) -> root.openai.OpenAIProvider {
    root.openai
        .responses(
            model = "gpt-5.6-luna",
            parallel_tool_calls = false,
            extra_body = { "max_output_tokens": max_output_tokens },
        )
}

// Constructed without credentials so deterministic tests can inspect it; live
// runs go through live_google_provider, which adds the API key.
function google_provider(max_output_tokens: int = 4096) -> root.google.ai.Gemini {
    root.google
        .ai
        .gemini(
            model = "gemini-2.5-flash",
            parallel_tool_calls = false,
            extra_body = { "generationConfig": { "maxOutputTokens": max_output_tokens } },
        )
}

function live_google_provider(max_output_tokens: int = 4096) -> root.google.ai.Gemini {
    root.google.ai.Gemini {
        ...google_provider(max_output_tokens = max_output_tokens),
        api_key: baml.env.get_or_panic("GOOGLE_API_KEY"),
    }
}

// The single backend switch: adding a provider means one new arm here.
function _provider_for(backend: Backend, max_output_tokens: int = 4096) -> ai.Provider {
    match (backend) {
        Backend.OpenAI => openai_provider(max_output_tokens = max_output_tokens),
        Backend.Google => live_google_provider(max_output_tokens = max_output_tokens),
    }
}

function Turn(transcript: string) -> string {
    provider: openai_provider()
    tools: [read_file, write_file, edit_file, list_dir, run_bash]
    prompt: `
        You are bamlcode, a CLI coding agent operating inside the user's current working directory.

        Complete software tasks by taking one tool action at a time. Put a short
        action summary in each tool call's \`summary\` argument.

        Memory:
          - Durable facts may appear under [RECALLED MEMORY].
          - Trust recalled facts unless the user contradicts them.
          - You do not manage memory. A separate curator runs after the turn.

        Operating rules:
          - Inspect before changing files.
          - Make old_str unique and copy it byte-for-byte before edit_file.
          - Use run_bash for search, tests, git, mkdir, and unsupported operations.
          - Do not repeat an action that already succeeded.
          - When complete, or when user input is required, return the final answer.
          - Never call more than one tool in a model step.

        Application transcript:
        ----------------------------------------------------------------------
        ${transcript}
        ----------------------------------------------------------------------

        ${ctx.output_format}
    `
}

// What one model turn produced. `queued` lives on SupervisedTurn — only
// the terminal supervisor owns type-ahead.
class AgentTurn {
    answer: string,
    transcript: string,
    continuation: ai.run.AgentSession<string>?,
    steps: int,
    interrupted: bool,
}

class SupervisedTurn {
    turn: AgentTurn,
    queued: string[],
}

// The REPL's own state: the transcript is application data (it renders the
// first turn's prompt and feeds the curator); the continuation is the
// stdlib's task-plus-conversation pairing.
class Session {
    transcript: string,
    continuation: ai.run.AgentSession<string>?,
}

class Trace {
    transcript: string,
}

class Observer {
    trace: Trace,
    live: bool,

    implements ai.observe.AgentObserver {
        function on_event(self, event: ai.observe.AgentEvent) -> null {
            match (event) {
                let tool_event: ai.observe.ToolCallEvent => {
                    if (tool_event.phase == "proposed") {
                        let summary = _call_summary(tool_event.call);
                        let label = _describe_call(tool_event.call);
                        if (summary.length() > 0) {
                            self.trace.transcript
                                = self.trace.transcript + "[THOUGHT] " + summary + "\n";
                            if (self.live) {
                                baml.io.print("  " + _dim(summary) + "\n");
                            };
                        };
                        self.trace.transcript = self.trace.transcript + "[ACTION] " + label + "\n";
                        if (self.live) {
                            baml.io.print("  " + _arrow(label) + "\n");
                        };
                    } else if (tool_event.phase == "finished" || tool_event.phase == "blocked") {
                        match (tool_event.result) {
                            let result: ai.tools.ToolResult => {
                                let rendered = _render_tool_result(result);
                                self.trace.transcript
                                    = self.trace.transcript
                                        + "[RESULT]\n"
                                        + _clip(rendered, 6000)
                                        + "\n";
                                if (self.live) {
                                    baml.io.print(
                                        _style_result(_clip(rendered, 1200), ai.tools.result_is_error(result))
                                            + "\n",
                                    );
                                };
                            },
                            null => {},
                        }
                    };
                },
                _ => {},
            }
            null
        }
    }
}

function runner(
    cancel: baml.spawn.CancelToken? = null,
    observers: ai.observe.AgentObserver[] = [],
) -> ai.run.Agent<string> {
    ai
        .run
        .Agent<string>
        .new(
            max_steps = 30,
            max_tool_calls_per_step = 1,
            observers = observers,
            cancel = cancel,
        )
}

function run_turn(
    history: string,
    continuation: ai.run.AgentSession<string>?,
    user_msg: string,
    backend: Backend,
    cancel: baml.spawn.CancelToken? = null,
) -> AgentTurn {
    //# Recall durable facts before the model turn
    let recall = if (enabled()) {
        default_store().inject(user_msg, history)
    } else {
        Recall { block: "", names: [] }
    };
    if (recall.names.length() > 0) {
        baml.io.print("  " + _gray(`⟳ recalled: ${recall.names.join(", ")}`) + "\n");
    };
    let opening = _opening_transcript(history, recall.block, user_msg);
    let trace = Trace { transcript: opening };
    let observer = Observer { trace: trace, live: true };
    let turn_runner = runner(cancel = cancel, observers = [observer]);
    match (continuation) {
        let session: ai.run.AgentSession<string> => {
            //# A later turn sends one message on the exact continuation. The
            //# session owns the provider and advances in place; the backend
            //# only matters on turn one.
            let outcome = session.send(
                ai.ChatMessage.user(_continuation_text(recall.block, user_msg)),
                runner = turn_runner,
            );
            _finish_turn(outcome, session, trace)
        },
        null => {
            //# The first turn renders the Task once and selects the backend
            let task = match (backend) {
                Backend.OpenAI => Turn@task(opening),
                Backend.Google => Turn@task(opening).with_provider(_provider_for(backend)),
            };
            let outcome = task.run(runner = turn_runner);
            _finish_turn(outcome, ai.run.AgentSession<string>.of(task, outcome), trace)
        },
    }
}

function _finish_turn(
    outcome: ai.Done<string> | ai.Stopped | ai.Handoff | ai.Interrupted | ai.Failed,
    next: ai.run.AgentSession<string>,
    trace: Trace,
) -> AgentTurn {
    let answer = "";
    let interrupted = false;
    //# Outcomes carry whole-run accounting; no observer counting needed
    let steps = 0;
    match (outcome) {
        let done: ai.Done<string> => {
            answer = done.value;
            steps = done.steps_taken;
        },
        let budget: ai.Stopped => {
            answer = `(stopped after ${budget.steps_taken} steps: ${budget.reason})`;
            steps = budget.steps_taken;
        },
        let handoff: ai.Handoff => {
            answer = `(unexpected handoff to ${handoff.call.name})`;
            steps = handoff.steps_taken;
        },
        let stopped: ai.Interrupted => {
            answer = "⨂ interrupted.";
            steps = stopped.steps_taken;
            interrupted = true;
        },
        let failed: ai.Failed => {
            //# An involuntary stop at a committed checkpoint: distinct from a
            //# user ESC. The session can resume once the fault clears.
            answer = `✖ the turn hit a provider fault and stopped (resumable): ${failed.cause}`;
            steps = failed.steps_taken;
            interrupted = true;
        },
    }
    if (interrupted) {
        baml.io.print("\n" + _yellow(answer) + "\n");
        trace.transcript = trace.transcript + "[INTERRUPTED by user]\n";
    } else {
        baml.io.print("\n" + _style_assistant(answer) + "\n");
        trace.transcript = trace.transcript + "ASSISTANT: " + answer + "\n";
    };
    AgentTurn {
        answer: answer,
        transcript: trace.transcript,
        continuation: next,
        steps: steps,
        interrupted: interrupted,
    }
}
