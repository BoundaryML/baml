// The model matrix and the budget arithmetic.
//
// "Current" means the models each provider ships as its active lineup today
// (pro-tier variants excluded: at $30/$180 per MTok one build alone could eat
// the whole experiment budget).

class ModelSpec {
    provider: string, // "openai" | "anthropic"
    name: string, // API model id
    base_url: string,
    key_env: string, // env var holding the builder API key
    in_price: float, // $ per 1M input tokens
    out_price: float, // $ per 1M output tokens
}

function models() -> ModelSpec[] {
    [
        ModelSpec {
            provider: "openai",
            name: "gpt-5.5",
            base_url: "https://api.openai.com/v1",
            key_env: "OPENAI_API_KEY",
            in_price: 5.0,
            out_price: 30.0,
        },
        ModelSpec {
            provider: "openai",
            name: "gpt-5.4",
            base_url: "https://api.openai.com/v1",
            key_env: "OPENAI_API_KEY",
            in_price: 2.5,
            out_price: 15.0,
        },
        ModelSpec {
            provider: "openai",
            name: "gpt-5.4-mini",
            base_url: "https://api.openai.com/v1",
            key_env: "OPENAI_API_KEY",
            in_price: 0.75,
            out_price: 4.5,
        },
        ModelSpec {
            provider: "openai",
            name: "gpt-5.4-nano",
            base_url: "https://api.openai.com/v1",
            key_env: "OPENAI_API_KEY",
            in_price: 0.2,
            out_price: 1.25,
        },
        ModelSpec {
            provider: "anthropic",
            name: "claude-fable-5",
            base_url: "https://api.anthropic.com",
            key_env: "ANTHROPIC_API_KEY",
            in_price: 10.0,
            out_price: 50.0,
        },
        ModelSpec {
            provider: "anthropic",
            name: "claude-opus-4-8",
            base_url: "https://api.anthropic.com",
            key_env: "ANTHROPIC_API_KEY",
            in_price: 5.0,
            out_price: 25.0,
        },
        ModelSpec {
            provider: "anthropic",
            name: "claude-sonnet-5",
            base_url: "https://api.anthropic.com",
            key_env: "ANTHROPIC_API_KEY",
            // introductory pricing through 2026-08-31
            in_price: 2.0,
            out_price: 10.0,
        },
        ModelSpec {
            provider: "anthropic",
            name: "claude-haiku-4-5",
            base_url: "https://api.anthropic.com",
            key_env: "ANTHROPIC_API_KEY",
            in_price: 1.0,
            out_price: 5.0,
        },
    ]
}

function find_model(name: string) -> ModelSpec? {
    for (let m in models()) {
        if (m.name == name) {
            return m;
        }
    }
    null
}

// Resolve CLI arguments into specs. "all" (or no args) selects everything;
// "openai" / "anthropic" select a provider's lineup. Panics on unknown names
// so a typo cannot silently benchmark the wrong matrix.
function resolve_models(args: string[]) -> ModelSpec[] {
    if (args.length() == 0) {
        return models();
    }
    let out: ModelSpec[] = [];
    for (let a in args) {
        if (a == "all") {
            return models();
        }
        if (a == "openai" || a == "anthropic") {
            for (let m in models()) {
                if (m.provider == a) {
                    out.push(m);
                }
            }
            continue;
        }
        match (find_model(a)) {
            let m: ModelSpec => {
                out.push(m);
            },
            null => {
                baml.sys
                    .panic(
                        "unknown model: "
                            + a
                            + " (known: "
                            + model_names().join(", ")
                            + ", all, openai, anthropic)",
                    );
            },
        }
    }
    out
}

function model_names() -> string[] {
    let names: string[] = [];
    for (let m in models()) {
        names.push(m.name);
    }
    names
}

// ---- budget ------------------------------------------------------------------

function budget_limit_usd() -> float {
    20.0
}

function max_turns() -> int {
    20
}

// Per-call caps the worker enforces (max output tokens is set on the builder
// clients; the input bound follows from the transcript window in builder.baml:
// 12k chars of transcript + task + action schema is safely under 8k tokens).
function max_input_tokens() -> int {
    8000
}

function max_output_tokens() -> int {
    4096
}

// Deliberately conservative worst case: every turn maxes out both the input
// bound and the output cap.
function worst_case_cost(m: ModelSpec) -> float {
    let per_turn = (1.0 * max_input_tokens() * m.in_price + 1.0 * max_output_tokens() * m.out_price) / 1000000.0;
    per_turn * max_turns()
}

class BudgetEntry {
    model: string,
    reserved_usd: float,
}

class BudgetLedger {
    limit_usd: float,
    reserved_usd: float,
    entries: BudgetEntry[],
}

function load_ledger(path: string) -> BudgetLedger {
    let text = baml.fs.read(path) catch (_) {
        _ => ""
    };
    if (text == "") {
        return BudgetLedger { limit_usd: budget_limit_usd(), reserved_usd: 0.0, entries: [] };
    }
    baml.json.from_string<BudgetLedger>(text) catch (_) {
        _ => BudgetLedger { limit_usd: budget_limit_usd(), reserved_usd: 0.0, entries: [] }
    }
}

class BudgetExceeded {
    message: string,
}

// Reserve worst-case spend for one build. Returns the updated ledger, or
// throws when the reservation would cross the hard limit. The ledger persists
// across invocations so repeated runs cannot silently exceed the cap.
function reserve(
    ledger: BudgetLedger,
    path: string,
    m: ModelSpec,
) -> BudgetLedger throws BudgetExceeded {
    let amount = worst_case_cost(m);
    let next_total = ledger.reserved_usd + amount;
    if (next_total > budget_limit_usd()) {
        throw BudgetExceeded {
            message: "hard budget stop: reserving $"
                + f2(amount)
                + " for "
                + m.name
                + " would bring the ledger to $"
                + f2(next_total)
                + " (limit $"
                + f2(budget_limit_usd())
                + "). Delete budget.json only if you accept re-spending.",
        };
    }
    let entries = ledger.entries.concat([BudgetEntry { model: m.name, reserved_usd: amount }]);
    let next = BudgetLedger { limit_usd: budget_limit_usd(), reserved_usd: next_total, entries: entries };
    baml.fs
        .write(
            path,
            baml.json.to_string(next) catch (_) {
                _ => "{}"
            },
        ) catch (_) {
        _ => null
    };
    next
}

test "registry sanity" {
    assert.equal(models().length(), 8);
    assert.equal(resolve_models([]).length(), 8);
    assert.equal(resolve_models(["all"]).length(), 8);
    assert.equal(resolve_models(["openai"]).length(), 4);
    assert.equal(resolve_models(["anthropic"]).length(), 4);
    assert.equal(resolve_models(["gpt-5.4-nano", "claude-haiku-4-5"]).length(), 2);
    match (find_model("claude-fable-5")) {
        let m: ModelSpec => {
            assert.equal(m.provider, "anthropic")
        },
        null => {
            assert.is_true(false)
        },
    }
    assert.equal(find_model("gpt-4o") == null, true);
}

test "budget math stays under the cap for the full matrix" {
    let total = 0.0;
    for (let m in models()) {
        total = total + worst_case_cost(m);
    }
    // Worst case for all 8 models must fit the $20 experiment budget.
    assert.is_true(total < budget_limit_usd());
    // Ledger reservations enforce the same bound incrementally.
    let ledger = BudgetLedger { limit_usd: budget_limit_usd(), reserved_usd: 19.99, entries: [] };
    let blocked = false;
    match (models().at(0)) {
        let m: ModelSpec => {
            reserve(ledger, "/tmp/benchexp-test-budget.json", m) catch (e) {
                BudgetExceeded => {
                    blocked = true;
                    ledger
                }
            };
        },
        null => null,
    }
    assert.is_true(blocked)
}
