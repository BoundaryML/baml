// Conformance suite for the bank: spec anchors resolve, every fact is
// registered, every rule is isolatable, and every name pool entry is a legal
// identifier. Item conformance itself runs in engine.baml over the registry.
//
// Test-block locals are boxed by the VM, so every case body is a function and
// the `test` blocks only assert.

// ─── locating the repository files ───────────────────────────────────────────

/// `relative`, resolved by walking up from the working directory: the suite
/// runs from `baml_language/`, from `tools/type_quiz/`, and from
/// `crates/baml_tests/` under CI, and the files it needs live at fixed places
/// under `baml_language/`.
function find_upward(relative: string) -> string? throws never {
    let prefixes = ["", "../", "../../", "../../../", "../../../../"];
    prefixes
        .map((p) -> {
            `${p}${relative}`
        })
        .find((path) -> {
            baml.fs.exists(path) catch (e) {
                baml.errors.Io => false,
            }
        })
}

function read_upward(relative: string) -> string? throws never {
    let path = find_upward(relative)
        ?? return null;
    baml.fs.read(path) catch (e) {
        baml.errors.Io => null,
        baml.errors.ParseError => null,
    }
}

/// The heading texts of a markdown document, without their `#` marks.
function headings(markdown: string) -> string[] throws never {
    markdown.lines().filter_map((line) -> {
        if (line.starts_with("#")) {
            let stripped = line.trim_start();
            let i = 0;
            while (i < stripped.length() && stripped.at(i) == "#") {
                i = i + 1;
            }
            stripped.slice(i, stripped.length()).trim()
        } else {
            null
        }
    })
}

// ─── spec anchors ────────────────────────────────────────────────────────────

/// A ledger entry is a heading that starts with its id.
function ledger_has(markdown: string, id: string) -> bool throws never {
    headings(markdown).some((h) -> {
        h.starts_with(`${id}:`) || h == id
    })
}

function unresolved_spec_refs() -> string[] throws never {
    let spec = read_upward("TYPE_SYSTEM.md")
        ?? return ["TYPE_SYSTEM.md not found above the working directory"];
    let gaps = read_upward("tools/type_quiz/SPEC_GAPS.md")
        ?? return ["tools/type_quiz/SPEC_GAPS.md not found above the working directory"];
    let spec_headings = headings(spec);
    root.bank.rules().filter_map((rule) -> {
        match (rule.spec) {
            let s: root.engine.Section => {
                if (spec_headings.includes(s.heading)) {
                    null
                } else {
                    `${rule.name}: no heading "${s.heading}" in TYPE_SYSTEM.md`
                }
            },
            let g: root.engine.Gap => {
                if (ledger_has(gaps, g.id)) {
                    null
                } else {
                    `${rule.name}: no entry ${g.id} in SPEC_GAPS.md`
                }
            },
        }
    })
}

function unresolved_statuses() -> string[] throws never {
    let gaps = read_upward("tools/type_quiz/SPEC_GAPS.md")
        ?? return ["tools/type_quiz/SPEC_GAPS.md not found above the working directory"];
    let divergences = read_upward("tools/type_quiz/COMPILER_DIVERGENCE.md")
        ?? return ["tools/type_quiz/COMPILER_DIVERGENCE.md not found above the working directory"];
    root.bank.items().filter_map((item) -> {
        match (item.status()) {
            root.engine.Verified => null,
            let d: root.engine.CompilerDiverges => {
                if (ledger_has(divergences, d.divergence)) {
                    null
                } else {
                    `${item.id()}: no entry ${d.divergence} in COMPILER_DIVERGENCE.md`
                }
            },
            let s: root.engine.SpecOnly => {
                if (ledger_has(gaps, s.gap)) {
                    null
                } else {
                    `${item.id()}: no entry ${s.gap} in SPEC_GAPS.md`
                }
            },
        }
    })
}

test "spec: every rule cites a heading in TYPE_SYSTEM.md or an entry in SPEC_GAPS.md" {
    assert.equal(unresolved_spec_refs().join("\n"), "")
}

test "spec: every diverging or spec-only item cites a ledger entry" {
    assert.equal(unresolved_statuses().join("\n"), "")
}

// ─── fact registry completeness ──────────────────────────────────────────────

/// Every function in this package with a fact generator's signature, by its
/// package-local name.
function fact_shaped_functions() -> string[] throws never {
    let derived = reflect.Type.of<root.bank.Derived>();
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
    let registered = root.bank.facts().map((f) -> {
        f.name
    });
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

// ─── every rule is isolatable ────────────────────────────────────────────────

/// The last rule of a trace before the flow rule, or the last rule when the
/// trace has no flow rule.
function isolated_rule(trace: root.engine.Rule[]) -> string? throws never {
    let flow = root.bank.flow_requires_subtype().name;
    let own = trace.filter((r) -> {
        r.name != flow
    });
    own.at(own.length() - 1)?.name
}

function rules_without_an_isolating_item() -> string[] throws never {
    let isolated: map<string, bool> = {};
    for (let item in root.bank.items()) {
        let i = 0;
        while (i < 4) {
            if let candidate: root.engine.Candidate = item.generate(root.engine.seed_for(item.id(), i)) {
                if let name: string = isolated_rule(candidate.case.trace) {
                    let _ = isolated.set(name, true);
                }
            }
            i = i + 1;
        }
    }
    root.bank.rules().filter_map((rule) -> {
        if (isolated.has(rule.name) || rule.name == root.bank.flow_requires_subtype().name) {
            null
        } else {
            rule.name
        }
    })
}

test "rules: every rule is the isolated rule of some item" {
    assert.equal(rules_without_an_isolating_item().join("\n"), "")
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
