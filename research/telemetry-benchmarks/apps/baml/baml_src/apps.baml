// The same four small apps are written in Python in ../py_*.py.
// Every model call goes to MODEL_BASE_URL: a loopback mock in most runs,
// the real OpenAI API in the live run.

client Model = openai.ChatClient.new(
    model = "gpt-4.1-mini",
    api_key = env.MODEL_API_KEY,
    base_url = env.MODEL_BASE_URL,
);

// One automatic retry, like the Python OpenAI SDK's max_retries=1.
client Reliable = ai.clients.Retry.new(inner = Model, max_attempts = 2);

class Invoice {
    vendor: string,
    total: float,
    currency: string,
    paid: bool,
}

class Lookup {
    sku: string,
}

// App 1: no model, no captured values.
function Hello() -> string {
    "hello world"
}

// App 2: a yes/no question about a document.
function Classify(text: string) -> bool {
    client: Model
    prompt: `
        Is this document an invoice? Reply with only true or false.
        ${text}
    `
}

// App 3: structured extraction into a class.
function Extract(text: string) -> Invoice {
    client: Model
    prompt: `
        Extract the invoice. ${ctx.output_format()}
        ${text}
    `
}

// App 4: a two-step agent with a tool and a retried model failure.
function ChooseTool(ticket: string) -> Lookup {
    client: Model
    prompt: `
        Choose the SKU for inventory lookup. ${ctx.output_format()}
        Ticket: ${ticket}
    `
}

function Stock(sku: string) -> int {
    match (sku) {
        "ACME-1" => 7,
        _ => 0,
    }
}

function Reply(stock: int) -> string {
    client: Reliable
    prompt: `Respond to the customer. Units available: ${stock}`
}

function Agent(ticket: string) -> string {
    let lookup = ChooseTool(ticket);
    let stock = Stock(lookup.sku, $trace = trace.span(inputs = true, output = true));
    Reply(stock)
}

// Runs one app `n` times in this process. With `vary`, each call's text gets a
// distinct suffix so no two inputs are identical.
function Many(app: string, text: string, n: int, vary: bool) -> int {
    let done = 0;
    while (done < n) {
        // With vary, `text` holds one document per call, separated by "|".
        let input = if (vary) {
            text.split("|").at(done) ?? `${text} #${done}`
        } else {
            text
        };
        match (app) {
            "hello" => {
                Hello();
            },
            "classify" => {
                Classify(input);
            },
            "extract" => {
                Extract(input);
            },
            _ => {
                Agent(input);
            },
        };
        done += 1;
    }
    done
}
