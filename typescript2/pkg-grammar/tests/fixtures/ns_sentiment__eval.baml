// Eval machinery for the sentiment classifier: the LLM judge, synthetic data
// generation, and case loading from a file or over HTTP. Same namespace as
// sentiment.baml, so tests.baml refers to everything bare.

// Stronger model, reserved for judging — you want the grader to be at least
// as capable as the model being graded.
client<llm> Grader {
    provider: anthropic,
    options: {
        model: "claude-sonnet-4-6",
        api_key: env.ANTHROPIC_API_KEY,
        max_tokens: 1024,
    },
}

// ---------------------------------------------------------------------------
// LLM-as-judge: a second model grades the first model's answer.
// ---------------------------------------------------------------------------
class Judgement {
    reasonable: bool,
    why: string,
}

function judge(text: string, label: Label) -> Judgement {
    client: Grader
    prompt: #"
        A sentiment classifier labeled this text as "{{ label }}":

        Text: {{ text }}

        Would a careful human reader accept that label as reasonable?
        Reasonable means defensible — not necessarily the only valid answer.

        {{ ctx.output_format }}
    "#
}

// ---------------------------------------------------------------------------
// Synthetic test data: ask a model to invent scenarios with a known answer.
// ---------------------------------------------------------------------------
function generate_examples(label: Label, n: int) -> string[] {
    client: Mini
    prompt: #"
        Write exactly {{ n }} short, realistic product-review sentences whose
        sentiment is clearly {{ label }}. Vary the products and phrasing, and
        do not use the word "{{ label }}" itself.

        {{ ctx.output_format }}
    "#
}

// ---------------------------------------------------------------------------
// External test data: the same cases, loadable from a file or over HTTP.
// ---------------------------------------------------------------------------
class Case {
    text: string,
    expected: Label,
}

// Edit this file freely — the from_file/from_api testsets pick it up.
function cases_path() -> string {
    "testdata/sentiment_cases.json"
}

function load_cases(path: string) -> Case[] {
    baml.json.from_string<Case[]>(baml.fs.read(path))
}

// Test-discovery helpers: the from_file testset generates one testset per
// case, named after the case. Names are slugged to ASCII — the test registry
// mishandles multibyte characters in dynamic testset names (a name followed
// by a longer one breaks expansion) — and prefixed with the case's position
// so edited-in duplicates can't collide.
function slug(text: string) -> string {
    // Walk UTF-8 bytes rather than chars: char_at panics near multibyte
    // characters on this toolchain, and we only keep ASCII anyway.
    let bytes = text.to_lower_case().to_utf8();
    let out = "";
    let i = 0;
    while (i < bytes.length() && out.length() < 48) {
        let b = bytes.at(i) ?? 0;
        let keep = (b >= 97 && b <= 122) || (b >= 48 && b <= 57) || b == 32;
        if (keep) {
            out = out + string.from_code_points([b]);
        };
        i = i + 1;
    }
    out.trim()
}

function case_name(i: int, c: Case) -> string {
    // substring(0, 40) caps the name AND forces a real copy first: feeding
    // file-decoded strings (or full-length substrings, which return the
    // original) into dynamic testset names breaks discovery on this
    // toolchain — the registry re-reads the name as "" during expansion.
    baml.unstable.string(i) + " - " + slug(c.text.substring(0, 40))
}

function case_names() -> string[] {
    let names: string[] = [];
    let i = 0;
    for (let c in load_cases(cases_path())) {
        i = i + 1;
        names.push(case_name(i, c));
    }
    names
}

function case_for(name: string) -> Case {
    let i = 0;
    for (let c in load_cases(cases_path())) {
        i = i + 1;
        if (name == case_name(i, c)) {
            return c;
        };
    }
    throw "no case in " + cases_path() + " matching test name: " + name;
}

function fetch_cases(url: string) -> Case[] {
    let res = baml.http.fetch(url);
    if (!res.ok()) {
        throw "HTTP " + baml.unstable.string(res.status_code) + " from " + url;
    };
    baml.json.from_string<Case[]>(res.text())
}

// One-shot HTTP server: accept a single connection, reply with the contents
// of the cases file, exit. Reuses root.server.build_response for the HTTP
// framing. Spawned by the from_api testset so the test is self-contained.
function serve_cases_once(addr: string) -> null {
    let listener = baml.net.TcpListener.bind(addr);
    let sock = listener.accept();
    let _req = sock.read() catch_all (e) {
        _ => "",
    };
    let body = baml.fs.read(cases_path());
    let _w = sock.write(root.server.build_response(body).to_utf8()) catch_all (e) {
        _ => 0,
    };
    let _c = sock.close() catch_all (e) {
        _ => null,
    };
    null
}
