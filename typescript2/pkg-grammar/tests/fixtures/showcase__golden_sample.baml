// BAML golden sample: the file every highlighter port is checked against, and
// the sample shipped to registries (Shiki, Linguist). It compiles and its test
// passes on the current toolchain; keep it that way, keep it under 100
// columns, and extend it when the language grows a new surface.

// The return type of an LLM function is the schema the model must produce.
enum Priority {
    High,
    Low,
}

class LineItem {
    name: string,
    amount: float,
    priority: Priority,
}

class Invoice {
    vendor: string @alias("seller"),
    status: "draft" | "final" @description("invoice state"),
    items: LineItem[],
    note: string?,
}

// Clients are ordinary values, wrapped to add retries and fallbacks.
client Fast = openai.ResponsesClient.new(model = "gpt-6-luna", api_key = env.OPENAI_API_KEY);

client Reliable = ai.clients.Retry.new(inner = Fast, max_attempts = 3);

function Extract(raw: string) -> Invoice {
    client: Reliable
    prompt: `Extract the invoice. ${ctx.output_format()}\n${raw}`
}

const MAX_ATTEMPTS = 3;

class Queued {
}

class Retrying {
    attempt: int,
}

class Done {
    invoice: Invoice,
}

type Job = Queued | Retrying | Done;

interface Labeled {
    function label(self) -> string throws never;
}

class Report {
    jobs: Job[],
    implements Labeled {
        function label(self) -> string {
            `${self.jobs.length()} jobs, ${total(self.jobs)} billed`
        }
    }
}

function status(job: Job) -> string {
    match (job) {
        Queued => "queued",
        Retrying { attempt: let n } if n >= MAX_ATTEMPTS => "giving up",
        Retrying { attempt: let n } => `retry ${n}`,
        let d: Done => `done: ${d.invoice.vendor}`,
    }
}

function total(jobs: Job[]) -> float {
    jobs
        .filter_map((j) -> {
            if let d: Done = j {
                d.invoice
            } else {
                null
            }
        })
        .flat_map((inv) -> {
            inv.items
        })
        .reduce(
            (sum, item) -> {
                sum + item.amount
            },
            0.0,
        )
}

function extract_all(texts: string[]) -> Invoice[] {
    let limit = baml.spawn.Limit.new(4);
    let futures = texts.map((t) -> {
        spawn with limit { Extract(t) }
    });
    await baml.future.all(futures)
}

function parse_count(text: string) -> int {
    baml.Int.parse(text) catch (e) {
        baml.errors.ParseError => 0
    }
}

test "status and totals without calling the model" {
    let reply = `{"seller": "Acme", "status": "final", "items": [], "note": null}`;
    let inv = Extract@spec("raw").parse(reply);
    inv.items.push(LineItem { name: "srv", amount: 900.0, priority: Priority.High });
    let jobs: Job[] = [Queued {}, Retrying { attempt: 3 }, Done { invoice: inv }];
    assert.equal(
        jobs.map((j) -> {
            status(j)
        }),
        ["queued", "giving up", "done: Acme"],
    );
    assert.equal(Report { jobs: jobs }.label(), "3 jobs, 900.0 billed");
    assert.equal(parse_count("x"), 0)
}
