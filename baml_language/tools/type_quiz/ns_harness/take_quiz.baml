// Stand a language model in for a learner and have it sit the quiz.
//
// One run is one model taking one sitting in mastery mode, exactly as the page
// puts it: the same adaptive selection, the same three answers, the same reveal
// after each one. The quiz is driven through `root.adaptive_step`, whose
// `asked` half carries only what a learner may see, so this harness cannot
// pass on a key it never receives. It calls the quiz in-process: nothing here
// shells out to the CLI, and a step costs what the model takes to answer.
//
// The model is a `claude -p` session with EVERY tool denied and no MCP server
// (proved, not assumed: ask it what tools it has and it says none). It cannot
// read the bank, run the compiler, or search. It has the specification and its
// own reasoning, which is what we are trying to measure.
//
//   baml run take_quiz --from tools/type_quiz -- --sessions 3 --models opus,sonnet,haiku
//
// run from `baml_language`, where TYPE_SYSTEM.md and target/ are. Every run
// writes as it goes and can be resumed, because the whole state of a sitting
// is its session number and the answers given so far.

/// Every tool the model must not have. Denying them all leaves it with none.
function denied() -> string[] throws never {
    [
        "Bash",
        "Read",
        "Glob",
        "Grep",
        "Edit",
        "Write",
        "MultiEdit",
        "NotebookEdit",
        "NotebookRead",
        "WebFetch",
        "WebSearch",
        "Task",
        "Agent",
        "ListAgents",
        "TodoWrite",
        "BashOutput",
        "KillShell",
        "SlashCommand",
        "ExitPlanMode",
        "Skill",
        "ToolSearch",
        "Workflow",
        "LSP",
        "Monitor",
        "SendMessage",
        "TaskCreate",
        "TaskGet",
        "TaskList",
        "TaskUpdate",
        "TaskOutput",
        "TaskStop",
        "CronCreate",
        "CronDelete",
        "CronList",
        "DesignSync",
        "EnterWorktree",
        "ExitWorktree",
        "PushNotification",
        "RemoteTrigger",
        "ReportFindings",
        "ScheduleWakeup",
        "AskUserQuestion",
    ]
}

/// The settings of a sweep, as the flags arrive.
class Settings {
    out: string,
    spec: string,
    max: int,
    tries: int,
    billing: string,
}

/// What `claude -p --output-format json` prints; every other field it prints
/// is ignored on decoding.
class Reply {
    session_id: string,
    result: string?,
    total_cost_usd: float?,
    is_error: bool?,
}

/// One model taking one sitting, as written to disk after every answer.
class Run {
    model: string,
    session: int,
    answers: string[],
    replies: string[],
    marks: root.Marked[],
    cost: float,
    session_id: string?,
    /// Why the harness stopped early, when it did; null for a sitting the
    /// quiz itself ended.
    stopped: string?,
    ended: root.Ended?,
    debrief: string?,
    /// The scales at the end of the debrief, as the JSON block the model
    /// ended with; null when it ended with no such block.
    ratings: string?,
    /// The transcript a learner would have downloaded, as its JSON.
    transcript: string?,
}

/// Three backticks. A template ends at a backtick, so a fence is spelled from
/// a plain string wherever a prompt needs one.
function fence() -> string throws never {
    "```"
}

// ─── the system prompt ───────────────────────────────────────────────────────

/// The system prompt every run shares, written beside the runs so what the
/// models were told is part of the record. Returns its path.
function system_prompt(
    settings: Settings,
) -> string throws baml.errors.Io | baml.errors.ParseError {
    let spec = baml.fs.read(settings.spec);
    let prompt = [
        "You are sitting a quiz on the BAML type system. Each question shows you a",
        "BAML program, or two programs, and asks what the compiler makes of them.",
        "",
        "You have no tools: no compiler, no files, no search, no internet. Answer",
        "from the specification below and your own reasoning. Nobody will tell you",
        "a case is unfair, so if the specification does not settle a question, say",
        "so by answering unsure.",
        "",
        "Scoring: a right answer earns 4 points, a wrong one costs 12, and unsure",
        "costs nothing. Committing therefore pays only when you are at least 75%",
        "sure. Answer unsure whenever you are not — guessing loses points, and an",
        "honest unsure is worth more to us than a lucky guess.",
        "",
        "How to answer:",
        "  One program  — ANSWER: compiles | ANSWER: rejected | ANSWER: unsure",
        "  Two programs — exactly one of them compiles; say which:",
        "                 ANSWER: first | ANSWER: second | ANSWER: unsure",
        "",
        "Reason briefly first if it helps, then finish with the ANSWER line and",
        "write nothing after it.",
        "",
        "════════ THE BAML TYPE SYSTEM ════════",
        "",
        spec,
    ]
        .join("\n");
    let path = `${settings.out}/system-prompt.txt`;
    let _ = baml.fs.write(path, prompt);
    path
}

// ─── one question ────────────────────────────────────────────────────────────

/// What to put to the model for this step: how the last answer went, the rule
/// to read first if there is one, and the question.
function ask(asked: root.Asked, marked: root.Marked?, index: int) -> string throws never {
    let lines: string[] = [];
    match (marked) {
        let m: root.Marked => {
            let verdict = match (m.correct) {
                let right: bool => {
                    if (right) {
                        "That was right."
                    } else {
                        "That was wrong."
                    }
                },
                null => "You withheld.",
            };
            let compiled = if (m.compiles.length() == 1) {
                if (m.compiles[0]) {
                    "The compiler accepts it."
                } else {
                    "The compiler rejects it."
                }
            } else if (m.compiles[0]) {
                "The compiler accepts the first."
            } else {
                "The compiler accepts the second."
            };
            lines.push(`Last question: you said ${m.said}. ${verdict} ${compiled}`);
            for (let because in m.because) {
                lines.push(`  · ${because}`);
            }
            if (m.difference != "") {
                lines.push(`  · ${m.difference}`);
            }
            lines.push("");
        },
        null => {},
    }
    if (asked.teach != "") {
        lines.push(`From the specification, on a rule you have been shaky on: ${asked.teach}`);
        lines.push("");
    }
    lines.push(`Question ${index + 1}.`);
    lines.push("");
    if (asked.programs.length() == 1) {
        lines.push(`${fence()}baml`);
        lines.push(asked.programs[0].trim_end());
        lines.push(fence());
        lines.push("");
        lines.push("Does it compile?");
    } else {
        for (let i = 0; i < asked.programs.length(); i = i + 1) {
            let which = if (i == 0) {
                "one"
            } else {
                "two"
            };
            lines.push(`Program ${which}:`);
            lines.push(`${fence()}baml`);
            lines.push(asked.programs[i].trim_end());
            lines.push(fence());
            lines.push("");
        }
        lines.push("Exactly one of these compiles. Which?");
    }
    lines.join("\n")
}

/// The answer word in a reply, or null when its ANSWER line does not hold
/// exactly one of `allowed`.
function answer_in(reply: string, allowed: string[]) -> string? throws never {
    let lines = reply.lines();
    // The last line that names an answer.
    let found: string? = null;
    for (let line in lines) {
        if (line.to_lower_case().includes("answer")) {
            found = line;
        }
    }
    let line = found
        ?? return null;
    let at = line.to_lower_case().index_of("answer")
        ?? return null;
    let after = line.slice(at + "answer".length(), line.length());
    let bare = after.replace(":", " ").replace("*", " ").trim().to_lower_case();
    let word = bare.split(" ").at(0)
        ?? return null;
    let clean = word.split(".").at(0)
        ?? return null;
    if (allowed.includes(clean)) {
        clean
    } else {
        null
    }
}

// ─── asking the model ────────────────────────────────────────────────────────

/// One turn of a `claude -p` session, resumed when there is one.
///
/// On the plan, `ANTHROPIC_API_KEY` takes precedence over a claude.ai login,
/// so leaving it in the environment would spend API credits without anyone
/// choosing to — measured, not assumed. The default unsets it through `env -u`
/// and runs on the plan; spending credits is something you have to ask for.
function put(
    model: string,
    session_id: string?,
    message: string,
    system: string,
    settings: Settings,
) -> Reply throws baml.errors.Io
    | baml.errors.Timeout
    | baml.json.ParseError
    | baml.json.DecodeError
    | baml.errors.InvalidArgument {
    let args = [
        "-p",
        "--model",
        model,
        "--output-format",
        "json",
        "--system-prompt-file",
        system,
        "--strict-mcp-config",
        "--mcp-config",
        "{\"mcpServers\":{}}",
        "--disallowedTools",
    ]
        .concat(denied());
    let resumed = match (session_id) {
        let id: string => args.concat(["--resume", id]),
        null => args,
    };
    let full = resumed.concat(["--", message]);
    let options = baml.sys.ProcessOptions {
        cwd: null,
        env: null,
        timeout_ms: 600000,
        stdin: null,
        stderr: baml.sys.StderrMode.Inherit,
    };
    let out = if (settings.billing == "api") {
        baml.sys.exec("claude", full, options)
    } else {
        baml.sys.exec("env", ["-u", "ANTHROPIC_API_KEY", "claude"].concat(full), options)
    };
    let printed = string.from_utf8(out.stdout);
    baml.json.from_string<Reply>(printed)
}

// ─── the debrief ─────────────────────────────────────────────────────────────

/// The end-of-sitting readout, as a learner is shown one.
function readout(run: Run, rules: int?) -> string throws never {
    let held = run.answers
        .filter((a) -> {
            a == "unsure"
        })
        .length();
    let right = run.marks
        .filter((m) -> {
            m.correct == true
        })
        .length();
    let why = match (run.ended) {
        let e: root.Ended => {
            if (e.why == "mastered") {
                "the model is confident it has your measure"
            } else if (e.why == "budget") {
                "it ran out of questions"
            } else if (e.why == "stalled") {
                "your estimates stopped moving"
            } else {
                e.why
            }
        },
        null => `the harness stopped it (${run.stopped ?? "?"})`,
    };
    let lines = [
        `  Ended because: ${why}`,
        `  Questions: ${run.answers.length()} — you committed on ${run.answers.length() - held} and withheld on ${held}`,
        `  Right: ${right} of the ${run.answers.length() - held} you committed on`,
    ];
    match (run.ended) {
        let e: root.Ended => {
            let of = match (rules) {
                let n: int => ` of ${n}`,
                null => "",
            };
            lines.push(`  Rules it will now bet on you for: ${e.mastered.length()}${of}`);
            if (e.shaky.length() > 0) {
                lines.push(`  Least certain of: ${e.shaky.slice(0, 4).join(", ")}`);
            }
            match (e.commits_unsure) {
                let c: float => {
                    lines.push(`  How often you committed to an answer you did not have: ${c}`)
                },
                null => {},
            }
            lines.push(
                `  A naive misconception your errors resembled: ${e.suspected ?? "none stood out"}`,
            );
        },
        null => {},
    }
    lines.join("\n")
}

/// The debrief. It is asked in the same session, so the model still has every
/// question it answered and can point at them.
///
/// It is about the INSTRUMENT, not the type system, and every question asks
/// for something the model is actually in a position to know: what it noticed
/// while sitting, not whether our estimates are any good. A model told to
/// critique will praise unless praise is made worthless, so the prompt says so
/// and every question demands a specific case.
function debrief_prompt(run: Run, rules: int?) -> string throws never {
    [
        "That is the end of the sitting. Here is the readout a learner is shown:",
        "",
        readout(run, rules),
        "",
        "Now I want your critique of the QUIZ ITSELF — the teaching instrument, not",
        "the type system. You are the only one who has just sat it, so what you",
        "noticed is evidence we cannot get any other way.",
        "",
        "Be blunt. \"It was well designed\" is worthless to us: praise costs you",
        "nothing and teaches us nothing. Every claim should point at a specific",
        "question or a specific stretch of the sitting, quoted enough that we can",
        "find it. If you did not notice something, say so rather than inventing it —",
        "a confident invention here is worse than a gap.",
        "",
        "1. PACING. Did the difficulty follow you? Name the first point where it",
        "   felt too easy or too hard and say what you were being asked there. If it",
        "   never seemed to adjust to your answers, say that instead.",
        "",
        "2. REPETITION. Which questions were, for your purposes, the same question",
        "   you had already answered? Quote enough of each to identify it. And was",
        "   there anything you wanted asked again — to check you had really learnt",
        "   it — that never came back?",
        "",
        "3. TELLS. Did you ever get one right from the SHAPE of the program rather",
        "   than by reasoning — a habit in how cases are built, their length, the",
        "   names, which construct shows up, how the two-program questions look? If",
        "   you found such a pattern, state it exactly. This is the most damaging",
        "   flaw the quiz can have, so it is the answer we most want.",
        "",
        "4. COVERAGE. What part of the specification were you never asked about and",
        "   expected to be? What was asked about more than it deserved?",
        "",
        "5. UNFAIR OR UNDECIDABLE. Name every question where you think the answer",
        "   you were given is wrong, or where the specification does not actually",
        "   settle it. For each, say what the specification would have to state for",
        "   the question to be answerable.",
        "",
        "6. ABSTENTION. \"Unsure\" cost you nothing and a wrong answer cost three",
        "   times what a right one earned. Did you use it honestly? Name a question",
        "   where you committed while knowing you were guessing, and say what pushed",
        "   you to.",
        "",
        "7. FEEDBACK. After each answer you were told what the compiler does and",
        "   which rules the case turned on. Name a case where that taught you",
        "   something you then used later, or say plainly that it never did.",
        "",
        "8. THE READOUT ABOVE. Where is it wrong about you, and how do you know?",
        "",
        "9. ONE CHANGE. One change to how this quiz chooses or presents questions.",
        "   What, and what would it cost?",
        "",
        "Finish with a fenced json block and nothing after it:",
        "",
        `${fence()}json`,
        "{",
        "  \"self_estimated_accuracy\": 0.0,",
        "  \"difficulty_tracked_me\": 0,",
        "  \"felt_repetitive\": 0,",
        "  \"noticed_a_shape_tell\": false,",
        "  \"explanations_taught_me\": 0,",
        "  \"worst_flaw\": \"\"",
        "}",
        fence(),
        "",
        "where `self_estimated_accuracy` is the share of the questions you committed",
        "on that you think you got right, and the three 0-5 scales run from 0 = not",
        "at all to 5 = completely.",
    ]
        .join("\n")
}

/// The JSON block a debrief ends with, when it ends with one.
function ratings_in(reply: string) -> string? throws never {
    let open = `${fence()}json`;
    let start = reply.last_index_of(open)
        ?? return null;
    let rest = reply.slice(start + open.length(), reply.length());
    let close = rest.index_of(fence())
        ?? return null;
    rest.slice(0, close).trim()
}

// ─── one sitting ─────────────────────────────────────────────────────────────

function keep(path: string, run: Run) -> void throws baml.errors.Io | baml.json.SerializationError {
    let _ = baml.fs.write(path, baml.json.to_string(run));
}

/// The sitting driven to its end, or to the harness's own stop, on a run that
/// may already hold answers from an earlier attempt. Every failure it can
/// meet — the model not answering, a file not reading, a reply not parsing —
/// is thrown, and `sit` records it on the run.
function drive(settings: Settings, model: string, session: int, system: string, run: Run) -> void {
    let path = `${settings.out}/${model}-${session}.json`;
    if (baml.fs.exists(path)) {
        let kept = baml.json.from_string<Run>(baml.fs.read(path));
        run.answers = kept.answers;
        run.replies = kept.replies;
        run.marks = kept.marks;
        run.cost = kept.cost;
        run.session_id = kept.session_id;
        run.stopped = kept.stopped;
        run.ended = kept.ended;
        run.debrief = kept.debrief;
        run.ratings = kept.ratings;
    }
    let going = true;
    while (going) {
        let step = baml.json.from_string<root.Step>(root.adaptive_step(session, run.answers.join(",")));
        // The marking is always of the answer just given, so it is stored by
        // the answer it belongs to and a resumed run rewrites it rather than
        // appending a second copy.
        match (step.marked) {
            let m: root.Marked => {
                if (run.marks.length() < run.answers.length()) {
                    run.marks.push(m);
                } else if (run.answers.length() > 0) {
                    run.marks[run.answers.length() - 1] = m;
                }
            },
            null => {},
        }
        match (step.ended) {
            let e: root.Ended => {
                run.ended = e;
                going = false;
            },
            null => {
                if (settings.max > 0 && run.answers.length() >= settings.max) {
                    run.stopped = "max";
                    going = false;
                } else {
                    let asked = step.asked
                        ?? {
                            run.stopped = "the quiz had nothing to ask";
                            going = false;
                            root.Asked { programs: [], teach: "" }
                        };
                    if (going) {
                        let allowed = if (asked.programs.length() == 1) {
                            ["compiles", "rejected", "unsure"]
                        } else {
                            ["first", "second", "unsure"]
                        };
                        let message = ask(asked, step.marked, run.answers.length());
                        let said: string? = null;
                        let reply = "";
                        let tries = 0;
                        while (said == null && tries < settings.tries) {
                            let answered = put(model, run.session_id, message, system, settings);
                            run.session_id = answered.session_id;
                            run.cost = run.cost + (answered.total_cost_usd ?? 0.0);
                            reply = answered.result ?? "";
                            said = answer_in(reply, allowed);
                            message
                                = `That reply did not end with an ANSWER line I could read. Answer with exactly one of: ${allowed.join(", ")}. End your reply with "ANSWER: <word>".`;
                            tries = tries + 1;
                        }
                        match (said) {
                            let word: string => {
                                run.answers.push(word);
                                run.replies.push(reply);
                                keep(path, run);
                                baml.io.println(
                                    `${model}/${session}: ${run.answers.length()} answered (${word})`,
                                );
                            },
                            null => {
                                run.stopped
                                    = `no answer after ${settings.tries} tries: ${reply.slice(0, 200.min(reply.length()))}`;
                                going = false;
                            },
                        }
                    }
                }
            },
        }
    }
    run.transcript = root.adaptive_record(session, run.answers.join(","));
    keep(path, run);
    // The debrief runs in the same session, so the model still has every
    // question it answered and can point at them.
    if (run.session_id != null && run.debrief == null) {
        let rules = match (run.ended) {
            root.Ended => 30,
            null => null,
        };
        let answered = put(model, run.session_id, debrief_prompt(run, rules), system, settings);
        run.cost = run.cost + (answered.total_cost_usd ?? 0.0);
        run.debrief = answered.result;
        run.ratings
            = match (answered.result) {
                let text: string => ratings_in(text),
                null => null,
            };
        keep(path, run);
    }
}

/// One model taking one sitting, written as it goes so nothing is lost. A
/// failure ends the run and is written on it, so the sweep goes on and the
/// summary says which sittings did not finish and why.
function sit(settings: Settings, model: string, session: int, system: string) -> Run throws never {
    let run = Run {
        model,
        session,
        answers: [],
        replies: [],
        marks: [],
        cost: 0.0,
        session_id: null,
        stopped: null,
        ended: null,
        debrief: null,
        ratings: null,
        transcript: null,
    };
    drive(settings, model, session, system, run) catch (e) {
        baml.errors.Io => {
            run.stopped = `io: ${e}`;
        },
        baml.errors.Timeout => {
            run.stopped = `the model did not answer in time: ${e}`;
        },
        baml.errors.ParseError => {
            run.stopped = `not text: ${e}`;
        },
        baml.errors.InvalidArgument => {
            run.stopped = `not text: ${e}`;
        },
        baml.json.ParseError => {
            run.stopped = `not json: ${e}`;
        },
        baml.json.DecodeError => {
            run.stopped = `unexpected json: ${e}`;
        },
        baml.json.SerializationError => {
            run.stopped = `could not write the run: ${e}`;
        },
    };
    run
}

/// How a run turned out, in one line.
function summarise(run: Run) -> string throws never {
    let held = run.answers
        .filter((a) -> {
            a == "unsure"
        })
        .length();
    let right = run.marks
        .filter((m) -> {
            m.correct == true
        })
        .length();
    let why = match (run.ended) {
        let e: root.Ended => e.why,
        null => run.stopped ?? "unfinished",
    };
    let mastered = match (run.ended) {
        let e: root.Ended => `${e.mastered.length()}`,
        null => "?",
    };
    `${run.model}/${run.session} questions=${run.answers.length()} why=${why} right=${right} committed=${run.answers.length() - held} withheld=${held} mastered=${mastered} ratings=${run.ratings != null} cost=$${run.cost}`
}

// ─── the sweep ───────────────────────────────────────────────────────────────

/// Every model sits every session — the same questions for all of them until
/// their answers part company — `concurrency` sittings at a time.
///
/// `models` is comma-separated; `billing` is `plan` (the claude.ai login, with
/// `ANTHROPIC_API_KEY` unset for the models) or `api`; `max` stops a sitting
/// after that many answers, zero for never; `spec` is the specification the
/// models are given. The cost a run reports is what its tokens would cost at
/// API rates — on the plan nothing is charged.
function take_quiz(
    sessions: int = 1,
    models: string = "haiku",
    first: int = 1,
    out: string = "target/type-quiz-llm",
    concurrency: int = 3,
    max: int = 0,
    tries: int = 3,
    billing: string = "plan",
    spec: string = "TYPE_SYSTEM.md",
) -> string {
    if (billing != "plan" && billing != "api") {
        baml.sys.exit(2);
    }
    let settings = Settings { out, spec, max, tries, billing };
    let _ = baml.fs.mkdir(out, baml.fs.MkdirOptions { recursive: true });
    let system = system_prompt(settings);
    baml.io.println(
        if (billing == "plan") {
            "billing: the claude.ai plan (ANTHROPIC_API_KEY unset for the models)"
        } else {
            "billing: PAID API CREDITS (ANTHROPIC_API_KEY passed through)"
        },
    );
    let named = models.split(",")
        .map((m) -> {
            m.trim()
        })
        .filter((m) -> {
            m != ""
        });
    let group = baml.spawn.TaskGroup.new(concurrency.max(1));
    let futures: baml.future.Future<Run, never>[] = [];
    for (let i = 0; i < sessions; i = i + 1) {
        for (let model in named) {
            let session = first + i;
            futures.push(spawn with baml.spawn.options(group = group) {
                sit(settings, model, session, system)
            });
        }
    }
    let runs: Run[] = [];
    for (let f in futures) {
        runs.push(await f);
    }
    let lines = runs.map((run) -> {
        summarise(run)
    });
    let spent = runs.reduce(
        (acc, run) -> {
            acc + run.cost
        },
        0.0,
    );
    lines.push(`total api-rate cost=$${spent} billed=${billing}`);
    let summary = lines.join("\n");
    let _ = baml.fs.write(`${out}/summary.txt`, `${summary}\n`);
    baml.io.println(`\n${summary}`);
    summary
}
