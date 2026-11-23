// The BAML on /baml. Each sample is a program that compiles on its own, because each is loaded
// into a live editor with its own project. Every one of them was compiled by the page's own
// compiler, and the runnable ones were run, before this was committed: do the same after changing
// one (open the page, press Run on each, and read the output).

export interface Sample {
  code: string;
  /** The file name shown above the editor. */
  file: string;
  /** False for a sample that calls a model: it needs an API key, which the page doesn't have. */
  runnable: boolean;
}

export const SAMPLES = {
  agent: {
    code: `// An agent is a function with a model, a prompt, and tools.
function support_agent(ticket: string) -> Answer | FollowUp {
    client: anthropic.Client.new(model = "claude-sonnet-5-5")
    tools: [lookup_customer, search_tickets]
    prompt: \`
        Answer this ticket, or ask one question.
        Look up the customer first.
        \${ticket}
    \`
}

class Answer {
    reply: string,
}

class FollowUp {
    question: string,
}

// Tools are plain functions. Their docs tell the model what they do.
/// Look up a customer's plan and billing status.
function lookup_customer(email: string) -> string {
    \`\${email}: pro plan, paid through March\`
}

/// Search past tickets for ones like this one.
function search_tickets(query: string) -> string[] {
    ["Charged twice", "Wrong plan on invoice"]
}
`,
    file: 'agent.baml',
    runnable: false,
  },
  errors: {
    code: `// handle is declared to throw nothing, and the compiler holds it
// to that: delete either arm and this no longer compiles.
function handle(ticket: string) -> string throws never {
    route(ticket) catch (e) {
        Unroutable => \`triage queue (\${e.label})\`,
        Spam => "discarded",
    }
}

function route(ticket: string) -> string throws Unroutable | Spam {
    match (classify(ticket)) {
        "billing" => "support",
        "bug" => "engineering",
        "spam" => throw Spam { reason: "prize" },
        "other" => throw Unroutable { label: "other" },
    }
}

class Unroutable {
    label: Label,
}

class Spam {
    reason: string,
}

type Label = "billing" | "bug" | "spam" | "other";

// Stands in for a model call, so this runs without an API key.
function classify(ticket: string) -> Label {
    let t = ticket.to_lower_case();
    if (t.includes("charged") || t.includes("invoice")) {
        "billing"
    } else if (t.includes("crash") || t.includes("error")) {
        "bug"
    } else if (t.includes("prize")) {
        "spam"
    } else {
        "other"
    }
}

function main() -> string[] {
    [
        handle("Charged twice for one order"),
        handle("App crashes on login"),
        handle("You have won a prize"),
        handle("Do you ship to Canada?"),
    ]
}
`,
    file: 'errors.baml',
    runnable: true,
  },
  match: {
    code: `// A union is matched by type. Remove an arm and the compiler
// tells you which case is no longer handled.
function route(msg: Refund | Question | string) -> string {
    match (msg) {
        Refund => \`refund \${msg.id}\`,
        Question { text } => \`answer: \${text}\`,
        string => \`text: \${msg}\`,
    }
}

// A match can also be on values, with guards.
function grade(score: int) -> string {
    match (score) {
        100 => "perfect",
        let s if s >= 60 => "pass",
        _ => "fail",
    }
}

class Refund {
    id: string,
}

class Question {
    text: string,
}

function main() -> string[] {
    [
        route(Refund { id: "r_812" }),
        route(Question { text: "Where is my order?" }),
        route("hello"),
        grade(100),
        grade(72),
        grade(12),
    ]
}
`,
    file: 'match.baml',
    runnable: true,
  },
  narrowing: {
    code: `class User {
    name: string,
    email: string,
}

function email_of(raw: unknown) -> string {
    if (raw is User) {
        // Here raw is a User, because the check above ran.
        raw.email.to_lower_case()
    } else {
        throw "not a user"
    }
}

function main() -> string {
    email_of(User { name: "Ada", email: "ADA@EXAMPLE.COM" })
}
`,
    file: 'types.baml',
    runnable: true,
  },
  tests: {
    code: `test "a double charge is billing" {
    assert.equal(classify("Charged twice for one order"), "billing")
}

// A testset builds its tests when it runs: one for each case here,
// named for the ticket.
testset "tickets" {
    let cases = [
        Case { ticket: "Wrong plan on invoice", label: "billing" },
        Case { ticket: "App crashes on login", label: "bug" },
        Case { ticket: "Do you ship to Canada?", label: "other" },
    ];
    for (let c in cases) {
        test c.ticket {
            assert.equal(classify(c.ticket), c.label)
        }
    }
}

class Case {
    ticket: string,
    label: Label,
}

type Label = "billing" | "bug" | "other";

// Stands in for a model call, so this runs without an API key.
function classify(ticket: string) -> Label {
    let t = ticket.to_lower_case();
    if (t.includes("charged") || t.includes("invoice")) {
        "billing"
    } else if (t.includes("crash") || t.includes("error")) {
        "bug"
    } else {
        "other"
    }
}
`,
    file: 'tests.baml',
    runnable: true,
  },
  threads: {
    code: `// Nothing here is marked async. spawn starts a green thread,
// and await joins it.
function main() -> int {
    let a = spawn { score(3) };
    let b = spawn { score(4) };
    let c = spawn { score(5) };
    (await a) + (await b) + (await c)
}

// One shard of work.
function score(seed: int) -> int {
    let total = 0;
    for (let i = 0; i < seed * 1000; i += 1) {
        total += i % 7;
    }
    total
}

test "three shards add up" {
    assert.is_true(main() > 0)
}
`,
    file: 'threads.baml',
    runnable: true,
  },
} satisfies Record<string, Sample>;
