// BTEL query demo program. The LLM functions call a local mock
// OpenAI-compatible server (mock_llm.py), so recording captures real
// arguments, outputs and errors without a model or network.

client Mock = openai.GenericClient.new(
    model = "demo-model",
    base_url = env.MOCK_LLM_URL,
    api_key = "unused",
);

class Customer {
    name: string,
    tier: string,
    age: int,
}

class Review {
    customer: Customer,
    text: string,
    stars: int,
}

class Verdict {
    sentiment: string,
    score: float,
    tags: string[],
}

// Retained and captured (an LLM function): args, output and error.
function Classify(review: Review, strict: bool) -> Verdict {
    client: Mock
    prompt: `
        Classify the sentiment of this review (strict: ${strict}).
        ${review.text}
    `
}

function Summarize(texts: string[]) -> string {
    client: Mock
    prompt: `Summarize: ${texts}`
}

// Direct recursion: recursive calls reuse the caller's calling context.
function Fib(n: int) -> int {
    if (n < 2) {
        n
    } else {
        Fib(n - 1) + Fib(n - 2)
    }
}

function Weight(stars: int) -> int {
    stars * 10
}

function Score(review: Review) -> int {
    let verdict = Classify(review, review.stars < 3);
    let total = Weight(review.stars);
    if (verdict.sentiment == "positive") {
        total + 5
    } else {
        total
    }
}

function Batch(reviews: Review[]) -> int {
    let sum = 0;
    for (let r in reviews) {
        sum = sum + Score(r);
    }
    sum
}

function main(label: string) -> int {
    let ann = Customer { name: "Ann", tier: "gold", age: 34 };
    let bob = Customer { name: "Bob", tier: "basic", age: 19 };
    let reviews = [
        Review { customer: ann, text: "Fast and friendly, great support", stars: 5 },
        Review { customer: bob, text: "Slow delivery, product fine", stars: 3 },
        Review { customer: ann, text: "Would buy again", stars: 4 },
    ];
    // Two spawned threads, then a third call site of Weight in main.
    let left = spawn { Batch([reviews[0], reviews[1]]) };
    let right = spawn { Batch([reviews[2]]) };
    let fib = Fib(15);
    let summary = Summarize([label, "three reviews"]);
    (await left) + (await right) + fib + Weight(1)
}

// Errors: the mock answers 500 for an angry review, so Classify throws and
// the uncaught error ends the execution as errored.
function angry() -> int {
    let carl = Customer { name: "Carl", tier: "basic", age: 41 };
    Score(Review { customer: carl, text: "angry: never again", stars: 1 })
}

// Runs for a few seconds so queries can watch its recording grow: the
// recorder publishes a new immutable file about once a second.
function slow(rounds: int) -> int {
    let sum = 0;
    let i = 0;
    while (i < rounds) {
        sum = sum + Fib(12);
        baml.sys.sleep(baml.time.Duration.from_milliseconds(500));
        i = i + 1;
    }
    sum
}
