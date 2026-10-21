// Global variables: the binding is immutable, what it holds is not.
//
// `$init` writes every global once, and every request then shares it, so a
// global cannot be reassigned. Its contents — a field, an element, an entry —
// are an ordinary heap value, and changing them is visible to every reader.

class Settings {
    retries int
}

const LIMIT = 3;
const SETTINGS = Settings { retries: 1 };
const ITEMS: int[] = [1, 2];
const LOOKUP: map<string, int> = { "a": 1 };

client Model = openai.ChatClient.new(model = "gpt-4o-mini");

// ── Refused: reassigning the binding ─────────────────────────────────────────
function reassigns() -> int {
    LIMIT = 4;
    LIMIT += 1;
    LIMIT
}

function reassigns_qualified() -> int {
    root.LIMIT = 5;
    LIMIT
}

function reassigns_a_client() -> null {
    Model = openai.ChatClient.new(model = "gpt-4o");
    null
}

// ── Clean: changing what the binding holds ──────────────────────────────────
function mutates_contents() -> int {
    SETTINGS.retries = 2;
    SETTINGS.retries += 1;
    ITEMS[0] = 10;
    ITEMS.push(3);
    LOOKUP["b"] = 2;
    SETTINGS.retries
}

// ── Clean: a local copy is a new binding ─────────────────────────────────────
function copies() -> int {
    let limit = LIMIT;
    limit += 1;
    limit
}
