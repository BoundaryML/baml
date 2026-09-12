function ImplementBenchmark(
    workspace: string,
    claude_executable: string,
    model: string,
    timeout_ms: int,
) -> AgentCompletion {
    client: claude_code.ClaudeCodeClient.new(
        executable = claude_executable,
        model = model,
        cwd = workspace,
        timeout_ms = timeout_ms,
        permission_mode = "bypassPermissions",
        harness_tools = ["Bash", "Edit", "Glob", "Grep", "Read", "Skill", "Write"],
    )
    prompt: `
        Implement the project in the current working directory.

        You must invoke and follow the installed baml-core skill before editing.
        Read AGENTS.md, README.md, SPEC.md, the supplied interface, and all tests.
        Use the baml CLI available on PATH. Keep working until the implementation
        compiles and the supplied tests pass. Do not weaken, remove, or rewrite tests.

        Return completed=true only when you believe the implementation is finished.

        ${ctx.output_format()}
    `
}

function run_agent(
    workspace: string,
    claude_executable: string,
    model: string = "opus",
    timeout_ms: int = 1800000,
) -> AgentCompletion {
    ImplementBenchmark(workspace, claude_executable, model, timeout_ms)
}
