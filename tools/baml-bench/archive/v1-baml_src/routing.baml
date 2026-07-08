// Static routing prefilter — deterministic tier of card routing.
//
// Every card gets one of three modes:
//   "static" — the rule IS the router. Match -> routed (no LLM opinion needed;
//              the trigger is purely structural). No match -> not routed.
//   "gate"   — the rule is a cheap high-recall filter. No match -> hard no
//              (the card never reaches a prompt). Match -> LOW CONFIDENCE:
//              fall back to inference — the card goes into the LLM router's
//              candidate index for confirmation.
//   (no rule)— always a candidate; the LLM router decides. The safe default.
//
// This prunes the O(chunks × cards) space before any model call: a card about
// CSV parsing never reaches a prompt for a chunk that never mentions CSV, and
// the surviving candidate index per chunk is ~10-30 cards instead of 127 —
// smaller prompts AND less trigger-soup for the router to skim past.
//
// Rules err on recall (broad any_of keywords): a false gate-match only costs
// one extra line in the router prompt; a false gate-miss silences a card, so
// signals are generous. Matching is case-insensitive substring over the chunk
// source, plus an optional chunk-kind restriction.

class RouteRule {
    card_id: string,
    mode: string, // "static" | "gate"
    kinds: string[], // non-empty: chunk.kind must be one of these
    any_of: string[], // non-empty: at least one (lowercased) substring must occur
}

// The result of prefiltering one chunk against the whole catalog.
class Prefilter {
    static_ids: string[], // routed deterministically, sorted
    candidates: PrincipleCard[], // cards the LLM router arbitrates, catalog order
}

function routing_rules() -> RouteRule[] {
    return [
        // ---- static: structural triggers the rule can decide alone ----------
        // every LLM function with a structured return should carry the
        // type-derived output format
        RouteRule { card_id: "P-036-2", mode: "static", kinds: ["llm_function"], any_of: [] },
        // every test chunk grades against test-shape cards
        RouteRule { card_id: "P-023-2", mode: "static", kinds: ["test"], any_of: [] },
        RouteRule { card_id: "P-023-4", mode: "static", kinds: ["test"], any_of: [] },
        // ---- gates: domain keyword present -> LLM confirms -------------------
        // streaming
        RouteRule { card_id: "P-009-1", mode: "gate", kinds: [], any_of: ["stream"] },
        RouteRule {
            card_id: "P-009-2",
            mode: "gate",
            kinds: [],
            any_of: ["on_tick", "sse", "stream"],
        },
        RouteRule {
            card_id: "P-009-3",
            mode: "gate",
            kinds: [],
            any_of: ["retry", "fallback", "while (", "catch"],
        },
        RouteRule { card_id: "P-009-4", mode: "gate", kinds: [], any_of: ["stream"] },
        RouteRule { card_id: "P-009-5", mode: "gate", kinds: [], any_of: ["stream", "abort"] },
        // mock
        RouteRule {
            card_id: "P-058-1",
            mode: "gate",
            kinds: [],
            any_of: ["mock", "test_mode", "fake"],
        },
        RouteRule { card_id: "P-058-2", mode: "gate", kinds: [], any_of: ["mock"] },
        RouteRule {
            card_id: "P-058-3",
            mode: "gate",
            kinds: [],
            any_of: ["mock", "spy", "call_count"],
        },
        // functions-as-tools
        RouteRule {
            card_id: "P-059-1",
            mode: "gate",
            kinds: [],
            any_of: ["tool", "dispatch", "$tool"],
        },
        RouteRule {
            card_id: "P-059-2",
            mode: "gate",
            kinds: [],
            any_of: ["tool", "token", "api_key", "secret"],
        },
        // tests / evals / optimize
        RouteRule {
            card_id: "P-023-1",
            mode: "gate",
            kinds: [],
            any_of: ["judge", "score", "eval", "assert"],
        },
        RouteRule { card_id: "P-023-3", mode: "gate", kinds: ["test"], any_of: [] },
        RouteRule { card_id: "P-036-1", mode: "gate", kinds: ["llm_function", "test"], any_of: [] },
        RouteRule {
            card_id: "P-036-3",
            mode: "gate",
            kinds: [],
            any_of: ["template_string", "@description"],
        },
        // middleware / budgets
        RouteRule {
            card_id: "P-040-1",
            mode: "gate",
            kinds: [],
            any_of: ["retry", "timeout", "backoff", "sleep"],
        },
        RouteRule {
            card_id: "P-040-2",
            mode: "gate",
            kinds: [],
            any_of: ["cost", "budget", "token_limit", "spend"],
        },
        // bridges / host errors
        RouteRule {
            card_id: "P-056-1",
            mode: "gate",
            kinds: [],
            any_of: ["bamlerror", "except", "isinstance"],
        },
        RouteRule {
            card_id: "P-056-2",
            mode: "gate",
            kinds: [],
            any_of: ["bamlpanic", "except", "cancel"],
        },
        // codegen / host bindings
        RouteRule {
            card_id: "P-030-1",
            mode: "gate",
            kinds: [],
            any_of: ["baml_sdk", "_async", "asyncio"],
        },
        RouteRule {
            card_id: "P-030-2",
            mode: "gate",
            kinds: [],
            any_of: ["$parse", "$render_prompt", "$build_request", "build_request"],
        },
        // time
        RouteRule {
            card_id: "P-021-1",
            mode: "gate",
            kinds: [],
            any_of: ["time", "date", "epoch", "timestamp", "duration", "_ms", "_at", "now(", "sleep"],
        },
        RouteRule {
            card_id: "P-021-2",
            mode: "gate",
            kinds: [],
            any_of: ["timezone", "offset", "utc", "zoned"],
        },
        RouteRule { card_id: "P-021-3", mode: "gate", kinds: [], any_of: ["parse", "iso", "rfc"] },
        RouteRule {
            card_id: "P-021-4",
            mode: "gate",
            kinds: [],
            any_of: ["now(", "clock", "overflow"],
        },
        // bigint
        RouteRule {
            card_id: "P-022-1",
            mode: "gate",
            kinds: [],
            any_of: ["bigint", "i64", "epoch", "nanosecond"],
        },
        RouteRule { card_id: "P-022-2", mode: "gate", kinds: [], any_of: ["bigint"] },
        // json
        RouteRule {
            card_id: "P-038-1",
            mode: "gate",
            kinds: [],
            any_of: ["json", "escape", "http"],
        },
        RouteRule { card_id: "P-038-2", mode: "gate", kinds: [], any_of: ["json"] },
        RouteRule { card_id: "P-038-3", mode: "gate", kinds: [], any_of: ["json"] },
        RouteRule { card_id: "P-038-4", mode: "gate", kinds: [], any_of: ["json", "unknown"] },
        RouteRule { card_id: "P-038-5", mode: "gate", kinds: [], any_of: ["@alias", "@skip"] },
        // reflection / generics
        RouteRule {
            card_id: "P-039-1",
            mode: "gate",
            kinds: [],
            any_of: ["reflect", "type_of", "get_return_type"],
        },
        RouteRule {
            card_id: "P-039-2",
            mode: "gate",
            kinds: [],
            any_of: ["reflect", "type_of", "to_string() =="],
        },
        RouteRule {
            card_id: "P-039-3",
            mode: "gate",
            kinds: [],
            any_of: ["<t>", "decode<", "fetch_"],
        },
        // core methods / strings
        RouteRule { card_id: "P-043-1", mode: "gate", kinds: [], any_of: ["while (", "for ("] },
        RouteRule {
            card_id: "P-043-3",
            mode: "gate",
            kinds: [],
            any_of: ["length()", "substring(", "char"],
        },
        // uuid / base64 / sql / random
        RouteRule { card_id: "P-045-1", mode: "gate", kinds: [], any_of: ["uuid", "id.new", "_id"] },
        RouteRule {
            card_id: "P-045-2",
            mode: "gate",
            kinds: [],
            any_of: ["uuid", "token", "secret"],
        },
        RouteRule { card_id: "P-045-3", mode: "gate", kinds: [], any_of: ["uuid"] },
        RouteRule {
            card_id: "P-046-1",
            mode: "gate",
            kinds: [],
            any_of: ["base64", "btoa", "to_utf8"],
        },
        RouteRule { card_id: "P-046-2", mode: "gate", kinds: [], any_of: ["base64", "jwt"] },
        RouteRule {
            card_id: "P-047-1",
            mode: "gate",
            kinds: [],
            any_of: ["select ", "insert ", "update ", "sql", "query"],
        },
        RouteRule {
            card_id: "P-047-2",
            mode: "gate",
            kinds: [],
            any_of: ["select ", "sql", "order by"],
        },
        RouteRule {
            card_id: "P-047-3",
            mode: "gate",
            kinds: [],
            any_of: ["sql", "transaction", "commit"],
        },
        RouteRule {
            card_id: "P-047-4",
            mode: "gate",
            kinds: [],
            any_of: ["sql", "connect", "database"],
        },
        RouteRule { card_id: "P-048-1", mode: "gate", kinds: [], any_of: ["random", "seed", "rng"] },
        RouteRule { card_id: "P-048-2", mode: "gate", kinds: [], any_of: ["random", "rng"] },
        // interpolation / prompts / templates
        RouteRule {
            card_id: "P-049-1",
            mode: "gate",
            kinds: [],
            any_of: ["{{", "{%", "template_string"],
        },
        RouteRule { card_id: "P-049-2", mode: "gate", kinds: [], any_of: [".join(", "+= ", "{{"] },
        RouteRule { card_id: "P-049-3", mode: "gate", kinds: [], any_of: ["\\n\" +", "+ \"\\n"] },
        RouteRule {
            card_id: "P-049-4",
            mode: "gate",
            kinds: [],
            any_of: ["enumerate", "padstart", "tofixed", "index"],
        },
        RouteRule {
            card_id: "P-049-5",
            mode: "gate",
            kinds: [],
            any_of: ["template_string", "_.role", "_.chat"],
        },
        // regex
        RouteRule {
            card_id: "P-052-1",
            mode: "gate",
            kinds: [],
            any_of: ["regex", "pattern", "match_all", "baml.re"],
        },
        // fs / glob / csv
        RouteRule {
            card_id: "P-032-1",
            mode: "gate",
            kinds: [],
            any_of: ["baml.fs", "fs.read", "fs.write", "mkdir", "open("],
        },
        RouteRule {
            card_id: "P-037-1",
            mode: "gate",
            kinds: [],
            any_of: ["read_dir", "glob", "ends_with(\".", "mkdir"],
        },
        RouteRule { card_id: "P-060-1", mode: "gate", kinds: [], any_of: ["csv", "split(\",\")"] },
        RouteRule { card_id: "P-060-2", mode: "gate", kinds: [], any_of: ["csv"] },
        RouteRule { card_id: "P-060-3", mode: "gate", kinds: [], any_of: ["csv", "```"] },
        RouteRule { card_id: "P-060-4", mode: "gate", kinds: [], any_of: ["csv", ".join(\",\")"] },
        RouteRule {
            card_id: "P-061-1",
            mode: "gate",
            kinds: [],
            any_of: ["mkdir", "remove", "rm", "create_dir"],
        },
        // concurrency
        RouteRule {
            card_id: "P-034-1",
            mode: "gate",
            kinds: [],
            any_of: ["spawn", "await", "future", "_async", "_sync"],
        },
        RouteRule {
            card_id: "P-034-2",
            mode: "gate",
            kinds: [],
            any_of: ["spawn", "await", "http", "fetch"],
        },
        RouteRule {
            card_id: "P-034-3",
            mode: "gate",
            kinds: [],
            any_of: ["race", "any(", "all(", "is_settled", "future"],
        },
        RouteRule { card_id: "P-034-4", mode: "gate", kinds: [], any_of: ["cancel", "catch"] },
        RouteRule {
            card_id: "P-034-5",
            mode: "gate",
            kinds: [],
            any_of: ["taskgroup", "limit", "retry", "batch", "throttle"],
        },
        RouteRule { card_id: "P-034-6", mode: "gate", kinds: [], any_of: ["spawn"] },
        RouteRule { card_id: "P-031-1", mode: "gate", kinds: [], any_of: ["spawn"] },
        // cleanup
        RouteRule {
            card_id: "P-042-1",
            mode: "gate",
            kinds: [],
            any_of: ["defer", "close(", "release(", "cleanup"],
        },
        RouteRule {
            card_id: "P-042-2",
            mode: "gate",
            kinds: [],
            any_of: ["cleanup", "close(", "dispose"],
        },
        RouteRule {
            card_id: "P-042-3",
            mode: "gate",
            kinds: [],
            any_of: ["cleanup", "is_closed", "closed"],
        },
        RouteRule {
            card_id: "P-042-4",
            mode: "gate",
            kinds: [],
            any_of: ["defer", "cause", "root_cause"],
        },
        // iterators
        RouteRule {
            card_id: "P-051-1",
            mode: "gate",
            kinds: [],
            any_of: ["while (", "iterator", ".map(", ".filter(", "next("],
        },
        RouteRule { card_id: "P-051-2", mode: "gate", kinds: [], any_of: ["iterator", "next("] },
        RouteRule {
            card_id: "P-051-3",
            mode: "gate",
            kinds: [],
            any_of: ["iterator", "next(", "collect", "repeat"],
        },
        // patterns / match / catch
        RouteRule { card_id: "P-015-1", mode: "gate", kinds: [], any_of: ["match ("] },
        RouteRule { card_id: "P-015-2", mode: "gate", kinds: [], any_of: ["match ("] },
        RouteRule { card_id: "P-015-3", mode: "gate", kinds: [], any_of: ["match (", "let "] },
        RouteRule {
            card_id: "P-015-4",
            mode: "gate",
            kinds: [],
            any_of: ["if let", "match (", "else {"],
        },
        RouteRule { card_id: "P-011-1", mode: "gate", kinds: [], any_of: ["match (", "catch"] },
        RouteRule { card_id: "P-011-2", mode: "gate", kinds: [], any_of: ["match (", "catch"] },
        RouteRule { card_id: "P-016-1", mode: "gate", kinds: [], any_of: ["catch"] },
        RouteRule { card_id: "P-028-1", mode: "gate", kinds: [], any_of: ["catch", "trace"] },
        RouteRule { card_id: "P-028-2", mode: "gate", kinds: [], any_of: ["catch"] },
        // interfaces / associated types
        RouteRule {
            card_id: "P-044-1",
            mode: "gate",
            kinds: [],
            any_of: ["interface", "implements", "match ("],
        },
        RouteRule {
            card_id: "P-044-2",
            mode: "gate",
            kinds: [],
            any_of: ["interface", "implements", "unknown"],
        },
        RouteRule {
            card_id: "P-044-3",
            mode: "gate",
            kinds: [],
            any_of: ["implements", "default."],
        },
        RouteRule { card_id: "P-044-4", mode: "gate", kinds: [], any_of: ["implements"] },
        RouteRule {
            card_id: "P-044-5",
            mode: "gate",
            kinds: [],
            any_of: ["upcast", "implements", "is "],
        },
        RouteRule { card_id: "P-044-6", mode: "gate", kinds: [], any_of: ["requires", "interface"] },
        RouteRule { card_id: "P-057-1", mode: "gate", kinds: [], any_of: ["interface", "type "] },
        RouteRule { card_id: "P-057-2", mode: "gate", kinds: [], any_of: ["extends", "interface"] },
        RouteRule {
            card_id: "P-057-3",
            mode: "gate",
            kinds: [],
            any_of: ["iterator<", "interface"],
        },
        // function interface / records / unset
        RouteRule {
            card_id: "P-062-1",
            mode: "gate",
            kinds: [],
            any_of: ["extends function", "wrapper", "traced"],
        },
        RouteRule {
            card_id: "P-062-2",
            mode: "gate",
            kinds: [],
            any_of: ["throws", "timeout", "wrapper"],
        },
        RouteRule { card_id: "P-062-3", mode: "gate", kinds: [], any_of: ["map<string"] },
        RouteRule {
            card_id: "P-062-4",
            mode: "gate",
            kinds: [],
            any_of: ["unset", ".get(", ".has("],
        },
        // companions
        RouteRule {
            card_id: "P-012-1",
            mode: "gate",
            kinds: [],
            any_of: ["$parse", "$render_prompt", "$build_request", "prompt", "parse"],
        },
        RouteRule { card_id: "P-012-2", mode: "gate", kinds: [], any_of: ["$"] },
        // namespaces
        RouteRule { card_id: "P-008-1", mode: "gate", kinds: [], any_of: ["ns_"] },
        RouteRule { card_id: "P-008-2", mode: "gate", kinds: [], any_of: ["root.", "ns_"] },
        RouteRule { card_id: "P-008-3", mode: "gate", kinds: [], any_of: ["response", "ns_"] },
        // visibility (pending)
        RouteRule { card_id: "P-004-1", mode: "gate", kinds: [], any_of: ["public", "private"] },
        RouteRule { card_id: "P-004-2", mode: "gate", kinds: [], any_of: ["public", "private"] },
        // print / debug / stdout
        RouteRule {
            card_id: "P-026-1",
            mode: "gate",
            kinds: [],
            any_of: ["print", "to_string", "to_debug", "log"],
        },
        RouteRule { card_id: "P-026-2", mode: "gate", kinds: [], any_of: ["json", "print", "log"] },
        RouteRule {
            card_id: "P-026-3",
            mode: "gate",
            kinds: [],
            any_of: ["format", "tofixed", "printf"],
        },
        RouteRule { card_id: "P-026-4", mode: "gate", kinds: [], any_of: ["prompt", "to_string"] },
        // standalone execution
        RouteRule {
            card_id: "P-027-1",
            mode: "gate",
            kinds: [],
            any_of: ["argv", "main", "cli", "flag"],
        },
        RouteRule { card_id: "P-027-2", mode: "gate", kinds: [], any_of: ["exit", "main", "print"] },
        RouteRule {
            card_id: "P-027-3",
            mode: "gate",
            kinds: [],
            any_of: ["baml.toml", "scripts", "baml run"],
        },
        // metrics / tracing / identity (proposed)
        RouteRule {
            card_id: "P-050-1",
            mode: "gate",
            kinds: [],
            any_of: ["metric", "score", "eval"],
        },
        RouteRule { card_id: "P-050-2", mode: "gate", kinds: [], any_of: ["trace", "_id", "$id"] },
        RouteRule {
            card_id: "P-050-3",
            mode: "gate",
            kinds: [],
            any_of: ["metric", "judge", "sample"],
        },
        RouteRule {
            card_id: "P-050-4",
            mode: "gate",
            kinds: [],
            any_of: ["passrate", "after", "testset"],
        },
        RouteRule {
            card_id: "P-053-1",
            mode: "gate",
            kinds: [],
            any_of: ["hash", "version", "fingerprint"],
        },
        RouteRule {
            card_id: "P-053-2",
            mode: "gate",
            kinds: [],
            any_of: ["hash", "version", "interface"],
        },
        RouteRule {
            card_id: "P-053-3",
            mode: "gate",
            kinds: [],
            any_of: ["trace", "span", "telemetry"],
        },
        RouteRule {
            card_id: "P-054-1",
            mode: "gate",
            kinds: [],
            any_of: ["#[", "//baml:", "attribute"],
        },
    ];
    // Cards with no rule here (e.g. P-017-* syntax forms, P-033-* parameter
    // design, P-005-1, the stdlib META cards) apply too broadly for a cheap
    // signal — they stay in "llm" mode: always candidates, router decides.
}

// True when `rule` matches the chunk (kind restriction + any-of substrings,
// case-insensitive).
function rule_matches(rule: RouteRule, chunk: Chunk, source_lower: string) -> bool {
    if (rule.kinds.length() > 0 && !rule.kinds.includes(chunk.kind)) {
        return false;
    }
    if (rule.any_of.length() == 0) {
        return true;
    }
    for (let sig in rule.any_of) {
        if (source_lower.includes(sig)) {
            return true;
        }
    }
    false
}

// Prefilter one chunk: deterministic static routes + the LLM candidate set.
// Cards with gate rules that don't fire are excluded entirely (hard no).
function prefilter_for(chunk: Chunk, cards: PrincipleCard[]) -> Prefilter {
    let rules = routing_rules();
    let by_card: map<string, RouteRule[]> = {};
    for (let r in rules) {
        let existing: RouteRule[] = match (by_card.get(r.card_id)) {
            let v: RouteRule[] => v,
            null => {
                let empty: RouteRule[] = [];
                empty
            },
        };
        existing.push(r);
        by_card.set(r.card_id, existing);
    }
    let source_lower = chunk.source.to_lower_case();
    let static_ids: string[] = [];
    let candidates: PrincipleCard[] = [];
    for (let c in cards) {
        match (by_card.get(c.id)) {
            let card_rules: RouteRule[] => {
                let matched_static = false;
                let matched_gate = false;
                for (let r in card_rules) {
                    if (rule_matches(r, chunk, source_lower)) {
                        if (r.mode == "static") {
                            matched_static = true;
                        } else {
                            matched_gate = true;
                        }
                    }
                }
                if (matched_static) {
                    static_ids.push(c.id);
                } else {
                    if (matched_gate) {
                        candidates.push(c);
                    }
                    // no rule fired -> hard no: card never reaches a prompt
                }
            },
            null => {
                // no rules authored -> LLM decides
                candidates.push(c);
            },
        }
    }
    return Prefilter {
        static_ids: static_ids
            .sort_by_key(
                (s: string) -> string {
                    s
                },
            ),
        candidates: candidates,
    };
}
