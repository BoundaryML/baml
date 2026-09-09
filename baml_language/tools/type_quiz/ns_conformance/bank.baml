// Conformance suite for the bank: every rule quotes the text it cites, every
// fact and rule generator is registered, every rule can be drilled on its own
// or directly on top of one that can, and every name pool entry is a legal
// identifier. Item conformance itself runs in engine.baml over the registry.
//
// Test-block locals are boxed by the VM, so every case body is a function and
// the `test` blocks only assert.

// ─── locating the repository files ───────────────────────────────────────────

function exists(path: string) -> bool throws never {
    baml.fs.exists(path) catch (e) {
        baml.errors.Io => false,
    }
}

/// The `baml_language` directory, as a prefix to join paths to.
///
/// BAML has no package-relative file access, so the suite has to find its own
/// tree by walking up from the working directory: it may be run from the
/// package, from `baml_language`, from a crate under it, or from the
/// repository root, which holds `baml_language` rather than being it. Both
/// the spec and this package's ledger must be present at the same prefix: a
/// directory holding only one of them is some other checkout, and accepting
/// it would check the citations against the wrong text.
function repository() -> string? throws never {
    let ascents = ["", "../", "../../", "../../../", "../../../../"];
    ascents
        .flat_map((up) -> {
            [up, `${up}baml_language/`]
        })
        .find((prefix) -> {
            exists(`${prefix}TYPE_SYSTEM.md`) && exists(`${prefix}tools/type_quiz/SPEC_GAPS.md`)
        })
}

function read_in(tree: string, relative: string) -> string? throws never {
    baml.fs.read(`${tree}${relative}`) catch (e) {
        baml.errors.Io => null,
        baml.errors.ParseError => null,
    }
}

/// Every document the citations are checked against, or `null` when the tree
/// could not be found.
class Sources {
    spec: string,
    gaps: string,
    divergences: string,
}

function sources() -> Sources? throws never {
    let tree = repository()
        ?? return null;
    let spec = read_in(tree, "TYPE_SYSTEM.md")
        ?? return null;
    let gaps = read_in(tree, "tools/type_quiz/SPEC_GAPS.md")
        ?? return null;
    let divergences = read_in(tree, "tools/type_quiz/COMPILER_DIVERGENCE.md")
        ?? return null;
    Sources { spec, gaps, divergences }
}

/// One line of explanation for every test that needs the tree, so a suite run
/// from the wrong directory says so once instead of reporting citations as
/// divergences.
function missing_tree() -> string[] throws never {
    [
        "TYPE_SYSTEM.md and tools/type_quiz/SPEC_GAPS.md were not found together at or above the working directory; run the suite from inside the repository",
    ]
}

test "spec: the repository tree is found from the working directory" {
    assert.is_true(sources() != null)
}

// ─── reading markdown ────────────────────────────────────────────────────────

/// The text of a markdown heading line, without its `#` marks.
function heading_text(line: string) -> string? throws never {
    if (!line.starts_with("#")) {
        return null;
    }
    let i = 0;
    while (i < line.length() && line.at(i) == "#") {
        i = i + 1;
    }
    line.slice(i, line.length()).trim()
}

/// The lines under the first heading `matches` accepts, up to the next
/// heading of any level.
function section_body(
    markdown: string,
    matches: (string) -> bool throws never,
) -> string? throws never {
    let lines = markdown.lines();
    let start = baml.iter.Range.new(0, lines.length()).find((i) -> {
        match (heading_text(lines[i])) {
            let h: string => matches(h),
            null => false,
        }
    })
        ?? return null;
    let end = baml.iter.Range.new(start + 1, lines.length()).find((i) -> {
        lines[i].starts_with("#")
    })
        ?? lines.length();
    lines.slice(start + 1, end).join("\n")
}

/// `text` with every run of whitespace collapsed to one space.
///
/// Markdown is hard-wrapped, so a sentence of the spec is not a line of it: a
/// quote is compared against the words of a section, not its line breaks, and
/// rewrapping a paragraph does not falsify a citation.
function words_of(text: string) -> string throws never {
    text.replace_all("\n", " ")
        .replace_all("\r", " ")
        .replace_all("\t", " ")
        .split(" ")
        .filter((word) -> {
            word.length() > 0
        })
        .join(" ")
}

/// A ledger entry's body: its heading is the id, alone or before a colon.
function entry_body(markdown: string, id: string) -> string? throws never {
    section_body(markdown, (h) -> {
        h == id || h.starts_with(`${id}:`)
    })
}

// ─── every rule quotes what it cites ─────────────────────────────────────────

/// Where a rule says its words come from.
function cited(rule: root.engine.Rule) -> string throws never {
    match (rule.spec) {
        let s: root.engine.Section => `TYPE_SYSTEM.md, "${s.heading}"`,
        let g: root.engine.Gap => `SPEC_GAPS.md, ${g.id}`,
    }
}

/// The text a rule's quotes must be found in.
function citation(rule: root.engine.Rule, sources: Sources) -> string? throws never {
    match (rule.spec) {
        let s: root.engine.Section => {
            section_body(sources.spec, (h) -> {
                h == s.heading
            })
        },
        let g: root.engine.Gap => entry_body(sources.gaps, g.id),
    }
}

/// Every quote of every rule, checked against the section or ledger entry the
/// rule cites. This is what makes "quoted from the spec" a fact rather than a
/// promise: a rule that paraphrases, or whose source is reworded, fails here.
function misquoted_rules() -> string[] throws never {
    let found = sources()
        ?? return missing_tree();
    root.bank.rules().flat_map((rule) -> {
        match (citation(rule, found)) {
            let body: string => {
                let words = words_of(body);
                rule.quotes().filter_map((quote) -> {
                    if (words.includes(words_of(quote))) {
                        null
                    } else {
                        `${rule.name}: ${cited(rule)} does not contain "${quote}"`
                    }
                })
            },
            null => [`${rule.name}: ${cited(rule)} names no section or entry`],
        }
    })
}

function unresolved_statuses() -> string[] throws never {
    let found = sources()
        ?? return missing_tree();
    root.bank.items().filter_map((item) -> {
        match (item.status()) {
            root.engine.Verified => null,
            let d: root.engine.CompilerDiverges => {
                if (entry_body(found.divergences, d.divergence) != null) {
                    null
                } else {
                    `${item.id()}: no entry ${d.divergence} in COMPILER_DIVERGENCE.md`
                }
            },
            let s: root.engine.SpecOnly => {
                if (entry_body(found.gaps, s.gap) != null) {
                    null
                } else {
                    `${item.id()}: no entry ${s.gap} in SPEC_GAPS.md`
                }
            },
        }
    })
}

/// The citation check must be able to fail. Were it vacuous, every rule
/// would pass it however it was worded, so the same section is asked about a
/// real quote and a paraphrase of the same principle and must tell them
/// apart.
function citation_check_tells_a_paraphrase_apart() -> bool throws never {
    let found = sources()
        ?? return false;
    let real = root.bank.never_bottom_rule();
    let paraphrase = root.engine.Rule { ...real, quote: "never is the bottom type of BAML" };
    let body = citation(real, found)
        ?? return false;
    let words = words_of(body);
    words.includes(words_of(real.quote)) && !words.includes(words_of(paraphrase.quote))
}

/// A composite rule reads as one statement with its elisions marked.
function composite_statements_mark_their_elisions() -> bool throws never {
    let single = root.bank.never_bottom_rule();
    let composite = root.bank.unknown_absorbs();
    single.statement() == single.quote
        && composite.quotes().length() == 2
        && composite.statement() == `${composite.quote} … ${composite.more[0]}`
}

test "spec: the citation check tells a quote from a paraphrase" {
    assert.is_true(citation_check_tells_a_paraphrase_apart())
}

test "spec: a composite rule reads as one statement" {
    assert.is_true(composite_statements_mark_their_elisions())
}

test "spec: every rule's words are found in the section or entry it cites" {
    assert.equal(misquoted_rules().join("\n"), "")
}

test "spec: every diverging or spec-only item cites a ledger entry" {
    assert.equal(unresolved_statuses().join("\n"), "")
}

// ─── fact registry completeness ──────────────────────────────────────────────

/// Every function in this package with a fact generator's signature, by its
/// package-local name.
function fact_shaped_functions() -> string[] throws never {
    let derived = reflect.Type.of<root.engine.Rolled<root.algebra.Derivation>>();
    let seed = reflect.Type.of<root.engine.Seed>();
    let functions = reflect.Package.current().functions();
    functions.keys().filter((name) -> {
        match (functions.get(name)) {
            let f: reflect.function.Type => {
                let params = f.params();
                f.return_type() == derived && params.length() == 1 && params[0].type == seed
            },
            null => false,
        }
    })
}

function local_name(qualified: string) -> string throws never {
    match (qualified.last_index_of(".")) {
        let i: int => qualified.slice(i + 1, qualified.length()),
        null => qualified,
    }
}

function unregistered_facts() -> string[] throws never {
    let registered = root.bank.facts()
        .map((f) -> {
            f.name
        })
        .concat(root.bank.fact_helpers());
    fact_shaped_functions().filter((name) -> {
        !registered.includes(local_name(name))
    })
}

function misnamed_facts() -> string[] throws never {
    let shaped = fact_shaped_functions().map((name) -> {
        local_name(name)
    });
    root.bank.facts().filter_map((f) -> {
        if (shaped.includes(f.name)) {
            null
        } else {
            `${f.name}: no fact generator function of that name`
        }
    })
}

test "facts: every fact-shaped function is registered" {
    assert.equal(unregistered_facts().join("\n"), "")
}

test "facts: every registered fact names its generator" {
    assert.equal(misnamed_facts().join("\n"), "")
}

test "facts: reflection sees the generators" {
    assert.is_true(fact_shaped_functions().length() >= 20)
}

/// The sweeps in this package pass vacuously over an empty registry, so the
/// registry must not be empty.
function registry_is_populated() -> bool throws never {
    root.bank.items().length() > 0
        && root.bank.facts().length() > 0
        && root.bank.rules().length() > 0
        && root.bank.models().length() > 0
}

test "bank: the registry is not empty" {
    assert.is_true(registry_is_populated())
}

// ─── every rule generator is registered ──────────────────────────────────────

type RuleFn = () -> root.engine.Rule throws never;

/// Every function in this package that produces a rule, by its qualified
/// name. The rule table's entries are the only ones that reach a learner, so
/// a rule written and left out of `rules()` would be invisible.
function rule_shaped_functions() -> string[] throws never {
    let produces = reflect.Type.of<root.engine.Rule>();
    let functions = reflect.Package.current().functions();
    functions.keys().filter((name) -> {
        match (functions.get(name)) {
            let f: reflect.function.Type => {
                f.return_type() == produces && f.params().length() == 0
            },
            null => false,
        }
    })
}

/// The rule generator called `name`, or `null` when this package does not
/// declare one.
function rule_generator(package: reflect.Package, name: string) -> RuleFn? throws never {
    package.get_function<RuleFn>(name) catch (e) {
        reflect.errors.CompilationError => null,
    }
}

/// Rule generators are matched to the table by the rule they produce, not by
/// their own name: a rule's name is its place in the type system, and its
/// function's name is only how BAML spells it.
function unregistered_rules() -> string[] throws never {
    let package = reflect.Package.current();
    let registered = root.bank.rules().map((r) -> {
        r.name
    });
    rule_shaped_functions().filter_map((name) -> {
        let produce: RuleFn = rule_generator(package, name)
            ?? return `${name}: produces a rule but cannot be called`;
        let produced = produce().name;
        if (registered.includes(produced)) {
            null
        } else {
            `${name}: produces rule ${produced}, which rules() omits`
        }
    })
}

test "rules: every rule generator is registered" {
    assert.equal(unregistered_rules().join("\n"), "")
}

test "rules: reflection sees the generators" {
    assert.is_true(rule_shaped_functions().length() >= 20)
}

// ─── every rule can be drilled ───────────────────────────────────────────────

/// The rules of each case the bank produces over the first four seeds of
/// every item, with the flow rule every probe ends in dropped: what is left
/// is the type-system content of the case.
function own_traces() -> string[][] throws never {
    let flow = root.bank.flow_requires_subtype().name;
    root.bank.items().flat_map((item) -> {
        let id = item.id();
        baml.iter.Range.new(0, 4)
            .filter_map((i) -> {
                let candidate = root.engine.generate(item, root.engine.seed_for(id, i))
                    ?? return null;
                let own: string[] = candidate.case.trace
                    .map((claim) -> {
                        claim.rule.name
                    })
                    .filter((name) -> {
                        name != flow
                    });
                own
            })
            .collect()
    })
}

/// The fewest rules in any trace `rule` concludes, or `null` when it
/// concludes none. A rule that concludes a one-rule trace can be drilled
/// alone; one that concludes a two-rule trace can be drilled directly on top
/// of a single other rule, which is as isolated as a rule about constructors
/// can be.
function shortest_conclusion(rule: string, traces: string[][]) -> int? throws never {
    traces
        .filter_map((trace) -> {
            if (trace.at(-1) == rule) {
                trace.length()
            } else {
                null
            }
        })
        .sort_by_key((n) -> {
            n
        })
        .at(0)
}

/// Bisection serves the shortest case concluding a rule the learner missed,
/// so every rule needs one of at most two rules. A rule reachable only
/// through longer derivations could never be put to a learner on its own.
function rules_without_a_short_case() -> string[] throws never {
    let traces = own_traces();
    let flow = root.bank.flow_requires_subtype().name;
    root.bank.rules().filter_map((rule) -> {
        if (rule.name == flow) {
            return null;
        }
        match (shortest_conclusion(rule.name, traces)) {
            let n: int => {
                if (n <= 2) {
                    null
                } else {
                    `${rule.name}: the shortest case concluding it uses ${n} rules`
                }
            },
            null => `${rule.name}: no item concludes with it`,
        }
    })
}

/// The rules that never stand alone: every case concluding them also uses
/// another rule first.
function wrapping_rules() -> string[] throws never {
    let traces = own_traces();
    root.bank.rules().filter_map((rule) -> {
        match (shortest_conclusion(rule.name, traces)) {
            let n: int => {
                if (n == 1) {
                    null
                } else {
                    rule.name
                }
            },
            null => null,
        }
    })
}

/// A rule needs something to wrap exactly when it is about variance, because
/// variance says what a constructor does to a relation and so has nothing to
/// say until another rule has supplied one. Every other rule must be able to
/// stand as a case by itself. The categories are read back from what the bank
/// generates, so a rule that quietly changes category is caught here rather
/// than making bisection unable to reach it.
function miscategorised_rules() -> string[] throws never {
    let wrapping = wrapping_rules();
    root.bank.rules().filter_map((rule) -> {
        let wraps = wrapping.includes(rule.name);
        let variance = rule.name.starts_with("variance/");
        if (wraps == variance) {
            null
        } else if (wraps) {
            `${rule.name}: never stands alone, but is not a variance rule`
        } else {
            `${rule.name}: a variance rule, but stands alone`
        }
    })
}

test "rules: every rule concludes a case of at most two rules" {
    assert.equal(rules_without_a_short_case().join("\n"), "")
}

test "rules: a rule needs something to wrap exactly when it is about variance" {
    assert.equal(miscategorised_rules().join("\n"), "")
}

// ─── name pools compile as identifiers ───────────────────────────────────────

function compiles(source: string) -> bool throws never {
    root.engine.agrees(
        root.engine.verify(
            root.engine.Case {
                files: { "case.baml": source },
                key: root.engine.Compiles { tests: [] },
                trace: [],
            },
        ),
    )
}

function illegal_names() -> string[] throws never {
    let types = root.bank.type_names().filter_map((n) -> {
        if (compiles(`class ${n} {\n}\n`)) {
            null
        } else {
            `type name ${n}`
        }
    });
    let functions = root.bank.function_names().filter_map((n) -> {
        if (compiles(`function ${n}() -> void {\n}\n`)) {
            null
        } else {
            `function name ${n}`
        }
    });
    let bindings = root.bank.binding_names().filter_map((n) -> {
        if (compiles(`function f(${n}: int) -> int {\n    ${n}\n}\n`)) {
            null
        } else {
            `binding name ${n}`
        }
    });
    types.concat(functions).concat(bindings)
}

test "names: every pool entry is a legal identifier of its kind" {
    assert.equal(illegal_names().join("\n"), "")
}
