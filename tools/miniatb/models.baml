class Feedback {
    source: "Slack" | "BamlFeedback" | "Github",
    reported_version: string?,
    files: map<string, string>,
    id: string,
    description: string,
    source_url: string?,
    channel: string?,
    thread: string?,
}

enum Difficulty {
    Easy @description("A localized change with a clear scope"),
    Hard @description("Substantial investigation, design, or changes across components"),
}

enum Kind {
    Bug @description("Existing behavior is incorrect"),
    Feature @description("New behavior or capability is requested"),
}

enum Subsystem {
    Syntax @description("Language syntax, lexing, parsing, or source formatting"),
    Compiler @description("Type checking, name resolution, lowering, or code compilation"),
    Runtime @description("VM execution, scheduling, memory management, or runtime failures"),
    StdLibrary @description("Built-in APIs, standard library functions, or model provider clients"),
    Tooling @description("CLI, editor integration, playground, SDK generation, or SDK bridges"),
    Unknown
        @description("Insufficient evidence, no category fits, or several subsystems are equally implicated"),
}

class QuickChecks {
    difficulty: Difficulty @description("How much work is needed to address this report?"),
    subsystem: Subsystem @description(
        "Which BAML subsystem is most likely responsible? Use Unknown when the report does not identify one clearly.",
    ),
}

class Classification {
    difficulty: Difficulty?,
    kind: Kind,
    subsystem: Subsystem,
}

class Workspace {
    id: string,
    version: string,
    revision: string,
    branch: string,
}

class Execution {
    exit_code: int,
    stdout: string,
    stderr: string,
}

class Repro {
    kind: Kind,
    files: map<string, string>,
    command: "check" | "test",
    explanation: string,
    blocker: string?,
}

class Investigation {
    sentences: string[],
    confirms_report: bool,
}

class Ticket {
    title: string,
    description: string[],
    investigation: string[],
}

class Issue {
    id: string,
    feedback: Feedback,
    classification: Classification,
    workspace: Workspace,
    ticket: Ticket,
    repro: Repro,
    execution: Execution,
}

class Fix {
    summary: string,
    test_command: string,
    blocker: string?,
}
