/// Controller-only session registration. No application key enters the agent.
function session_environment() -> map<string, string> {
    let env: map<string, string> = { "PATH": "/usr/local/bin:/usr/bin:/bin" };
    for (
        let name in [
            "FEEDBACK_SUPABASE_URL",
            "FEEDBACK_SUPABASE_KEY",
            "ATB_SLACK_BOT_TOKEN",
            "ATB2_SLACK_BOT_TOKEN",
            "ATB2_DATASET",
            "ATB2_MODEL",
            "ATB2_UI_URL",
        ]
    ) {
        let value = baml.env.get(name);
        if (value != null) {
            env.set(name, value ?? "");
        };
    }
    env
}

function bind_agent_session(
    sb: Sandbox,
    thread: SlackRef?,
    issue_id: string?,
    pr: string?,
    kind: string,
) -> null {
    if (thread == null || !db_configured()) {
        return null;
    };
    let ref = thread ?? baml.sys.panic("checked thread");
    let out = baml.sys.exec(
        "/usr/bin/python3",
        ["-I", "/usr/local/lib/atb2/sessions.py", "bind"],
        baml.sys.ProcessOptions {
            cwd: atb2_home(),
            env: session_environment(),
            timeout_ms: 180000,
            stdin: baml.json.stringify(
                {
                    "channel": ref.channel,
                    "thread_ts": ref.ts,
                    "workspace": sb.worktree,
                    "issue_id": issue_id,
                    "pr": pr,
                    "kind": kind,
                }
                    .to_json(),
            ),
        },
    );
    if (out.exit_code != 0) {
        throw StoreError { message: "could not preserve the thread's agent session" };
    };
    null
}

function agent_turn_loop() -> never {
    while (true) {
        if (db_configured()) {
            let result = run_with(
                Cmd {
                    program: "/usr/bin/python3",
                    args: ["-I", "/usr/local/lib/atb2/sessions.py", "once"],
                    cwd: atb2_home(),
                    timeout_ms: 600000,
                },
                session_environment(),
            );
            if (!result.ok) {
                log.warn({ "agent_turn": "unavailable; retrying next pass" });
            };
        };
        baml.sys.sleep(baml.time.Duration.from_seconds(5));
    }
}

function release_agent_session(sb: Sandbox) -> null {
    let result = run_with(
        Cmd {
            program: "/usr/bin/python3",
            args: ["-I", "/usr/local/lib/atb2/sessions.py", "release", sb.worktree],
            cwd: atb2_home(),
            timeout_ms: 180000,
        },
        session_environment(),
    );
    if (!result.ok) {
        log.warn({ "agent_session": "cleanup deferred; checkout retained" });
    };
    null
}
