// Conformance suite for the algebra: the rendering round trip, the kind
// bridge, the relation transfers, and the sites, each checked against the
// compiler rather than against expectations written by hand.
//
// Test-block locals are boxed by the VM, so every case body is a function and
// the `test` blocks only assert.

// ─── a fixed corpus of types ─────────────────────────────────────────────────

function box_decl() -> root.algebra.ClassDecl throws never {
    root.algebra.ClassDecl {
        name: "Box",
        params: ["T"],
        fields: [root.algebra.FieldDecl { name: "value", ty: root.algebra.var("T") }],
    }
}

function pair_decl() -> root.algebra.ClassDecl throws never {
    root.algebra.ClassDecl {
        name: "Pair",
        params: [],
        fields: [
            root.algebra.FieldDecl { name: "left", ty: root.algebra.int() },
            root.algebra.FieldDecl { name: "right", ty: root.algebra.string() },
        ],
    }
}

function side_decl() -> root.algebra.EnumDecl throws never {
    root.algebra.EnumDecl { name: "Side", variants: ["Left", "Right"] }
}

function iter_decl() -> root.algebra.IfaceDecl throws never {
    root.algebra.IfaceDecl {
        name: "Iter",
        params: [],
        assoc: [root.algebra.AssocDecl { name: "Item", default: root.algebra.int() }],
    }
}

function rec_decl() -> root.algebra.AliasDecl throws never {
    root.algebra.recursive_alias("Rec", (self) -> {
        root.algebra.union([root.algebra.int(), root.algebra.array(self)])
    })
}

class Corpus {
    name: string,
    ty: root.algebra.Ty,
    /// What the compiler prints for the type when that differs from the
    /// rendering: the compiler flattens nested unions and spells defaulted
    /// interface pins. Both spellings denote the same type; recording the
    /// difference here means a change in the compiler's printing is noticed.
    printed: string?,
}

function entry(name: string, ty: root.algebra.Ty) -> Corpus throws never {
    Corpus { name: name, ty: ty, printed: null }
}

/// Types whose rendering must survive a trip through the compiler. Every
/// variant of `Ty` except `Var` appears, and the nesting covers each place
/// where precedence matters.
function corpus() -> Corpus[] throws never {
    let fn1 = root.algebra.fn(
        [root.algebra.positional(root.algebra.int())],
        root.algebra.string(),
        root.algebra.never(),
    );
    [
        entry("int", root.algebra.int()),
        entry("float", root.algebra.float()),
        entry("bool", root.algebra.bool()),
        entry("string", root.algebra.string()),
        entry("bigint", root.algebra.bigint()),
        entry("null", root.algebra.null_ty()),
        entry("uint8array", root.algebra.uint8array()),
        entry("never", root.algebra.never()),
        entry("unknown", root.algebra.unknown()),
        entry("lit int", root.algebra.lit(1)),
        entry("lit negative", root.algebra.lit(-2)),
        entry("lit string", root.algebra.lit("sheep")),
        entry("lit escaped", root.algebra.lit("say \"baa\"")),
        entry("lit bool", root.algebra.lit(true)),
        entry("union", root.algebra.union([root.algebra.int(), root.algebra.string()])),
        entry("optional", root.algebra.optional(root.algebra.string())),
        entry(
            "union of literals",
            root.algebra.union([root.algebra.lit(1), root.algebra.lit(2), root.algebra.int()]),
        ),
        Corpus {
            name: "nested union",
            ty: root.algebra.union(
                [
                    root.algebra.int(),
                    root.algebra.union([root.algebra.string(), root.algebra.bool()]),
                ],
            ),
            printed: "int | string | bool",
        },
        entry("array", root.algebra.array(root.algebra.int())),
        entry(
            "array of union",
            root.algebra.array(root.algebra.union([root.algebra.int(), root.algebra.string()])),
        ),
        entry("array of array", root.algebra.array(root.algebra.array(root.algebra.int()))),
        entry("map", root.algebra.map(root.algebra.string(), root.algebra.int())),
        entry(
            "map of union",
            root.algebra.map(
                root.algebra.string(),
                root.algebra.union([root.algebra.int(), root.algebra.null_ty()]),
            ),
        ),
        entry("fn", fn1),
        entry("fn in union", root.algebra.union([root.algebra.string(), fn1])),
        entry("array of fn", root.algebra.array(fn1)),
        entry(
            "fn returning union",
            root.algebra.fn(
                [root.algebra.positional(root.algebra.int())],
                root.algebra.union([root.algebra.int(), root.algebra.string()]),
                root.algebra.never(),
            ),
        ),
        entry("fn no params", root.algebra.fn([], root.algebra.null_ty(), root.algebra.unknown())),
        entry("class", root.algebra.class_ty(pair_decl(), [])),
        entry("generic class", root.algebra.class_ty(box_decl(), [root.algebra.int()])),
        entry(
            "generic class of union",
            root.algebra.class_ty(
                box_decl(),
                [root.algebra.union([root.algebra.int(), root.algebra.string()])],
            ),
        ),
        entry("enum", root.algebra.enum_ty(side_decl())),
        entry("enum literal", root.algebra.enum_lit(side_decl(), "Left")),
        Corpus {
            name: "interface",
            ty: root.algebra.iface(iter_decl(), [], []),
            printed: "Iter<Item = int>",
        },
        entry(
            "interface pinned",
            root.algebra.iface(
                iter_decl(),
                [],
                [root.algebra.Pin { name: "Item", ty: root.algebra.string() }],
            ),
        ),
        entry("recursive alias", root.algebra.Alias { decl: rec_decl() }),
        entry("array of recursive alias", root.algebra.array(root.algebra.Alias { decl: rec_decl() })),
    ]
}

/// The compiler's reading of `ty`'s rendering.
function spelled(ty: root.algebra.Ty) -> root.engine.Spelled throws never {
    let decls = root.algebra.render_decls(root.algebra.decls_of(ty));
    root.engine.type_of_spelling(decls, root.algebra.render(ty))
}

// ─── render is a fixed point of parse-then-print ─────────────────────────────

function fixed_point_failures() -> string[] throws never {
    corpus().filter_map((c) -> {
        let rendered = root.algebra.render(c.ty);
        match (spelled(c.ty)) {
            let t: reflect.Type => {
                let printed = t.to_string();
                let expected = c.printed ?? rendered;
                if (printed == expected) {
                    null
                } else {
                    `${c.name}: rendered ${rendered}, expected print ${expected}, compiler printed ${printed}`
                }
            },
            let failed: root.engine.CompileFailed => {
                `${c.name}: ${rendered} did not compile: ${failed.diagnostics.map((d) -> { d.message }).join("; ")}`
            },
            let threw: root.engine.InitThrew => `${c.name}: ${threw.message}`,
        }
    })
}

test "render: every corpus spelling is read back and printed identically" {
    assert.equal(fixed_point_failures().join("\n"), "")
}

// ─── the parsed rendering is the type built through reflection's constructors ─

function constructor_failures() -> string[] throws never {
    corpus().filter_map((c) -> {
        match (root.algebra.to_reflect(c.ty)) {
            null => null,
            let built: reflect.Type => match (spelled(c.ty)) {
                let parsed: reflect.Type => {
                    if (parsed == built) {
                        null
                    } else {
                        `${c.name}: parsed ${parsed} is not the constructed ${built}`
                    }
                },
                let failed: root.engine.CompileFailed => `${c.name}: did not compile`,
                let threw: root.engine.InitThrew => `${c.name}: ${threw.message}`,
            },
        }
    })
}

function constructor_coverage() -> int throws never {
    corpus()
        .filter((c) -> {
            root.algebra.to_reflect(c.ty) != null
        })
        .length()
}

test "render: structural spellings denote the constructed types" {
    assert.equal(constructor_failures().join("\n"), "")
}

test "render: the constructor check covers the structural corpus" {
    assert.is_true(constructor_coverage() >= 20)
}

// ─── kinds ───────────────────────────────────────────────────────────────────

function kind_failures() -> string[] throws never {
    corpus().filter_map((c) -> {
        match (root.algebra.expected_kind(c.ty)) {
            null => `${c.name}: corpus types must have a realized kind`,
            let expected: root.algebra.Kind => match (spelled(c.ty)) {
                let t: reflect.Type => {
                    let got = root.algebra.kind_of(t);
                    if (got == expected) {
                        null
                    } else {
                        `${c.name}: expected ${expected}, compiler says ${got}`
                    }
                },
                let failed: root.engine.CompileFailed => `${c.name}: did not compile`,
                let threw: root.engine.InitThrew => `${c.name}: ${threw.message}`,
            },
        }
    })
}

test "kinds: the compiler classifies every corpus type as expected" {
    assert.equal(kind_failures().join("\n"), "")
}

// ─── equivalence is canonical while spelling is not ──────────────────────────

function canonical_equalities() -> string[] throws never {
    let pairs = [
        [
            root.algebra.union([root.algebra.int(), root.algebra.string()]),
            root.algebra.union([root.algebra.string(), root.algebra.int()]),
        ],
        [root.algebra.union([root.algebra.lit(1), root.algebra.int()]), root.algebra.int()],
        [root.algebra.union([root.algebra.lit(true), root.algebra.lit(false)]), root.algebra.bool()],
        [
            root.algebra.optional(root.algebra.string()),
            root.algebra.union([root.algebra.string(), root.algebra.null_ty()]),
        ],
        [
            root.algebra.union(
                [
                    root.algebra.int(),
                    root.algebra.union([root.algebra.string(), root.algebra.bool()]),
                ],
            ),
            root.algebra.union([root.algebra.int(), root.algebra.string(), root.algebra.bool()]),
        ],
        [root.algebra.union([root.algebra.int(), root.algebra.never()]), root.algebra.int()],
        [root.algebra.union([root.algebra.int(), root.algebra.unknown()]), root.algebra.unknown()],
    ];
    pairs.filter_map((pair) -> {
        match (spelled(pair[0])) {
            let l: reflect.Type => match (spelled(pair[1])) {
                let r: reflect.Type => {
                    if (l == r) {
                        null
                    } else {
                        `${l} != ${r}`
                    }
                },
                let failed: root.engine.CompileFailed => {
                    `${root.algebra.render(pair[1])} did not compile`
                },
                let threw: root.engine.InitThrew => threw.message,
            },
            let failed: root.engine.CompileFailed => {
                `${root.algebra.render(pair[0])} did not compile`
            },
            let threw: root.engine.InitThrew => threw.message,
        }
    })
}

test "equivalence: the compiler identifies spellings the spec calls equivalent" {
    assert.equal(canonical_equalities().join("\n"), "")
}

// ─── relation transfers ──────────────────────────────────────────────────────

function transfer_table_holds() -> bool throws never {
    let sub: root.algebra.Knowledge = root.algebra.Relation.Sub;
    let sup: root.algebra.Knowledge = root.algebra.Relation.Super;
    let eq: root.algebra.Knowledge = root.algebra.Relation.Equiv;
    let un: root.algebra.Knowledge = root.algebra.Relation.Unrelated;
    let none: root.algebra.Knowledge = root.algebra.Undecided {  };
    root.algebra.covariant(sub) == sub
        && root.algebra.contravariant(sub) == sup
        && root.algebra.contravariant(sup) == sub
        && root.algebra.contravariant(eq) == eq
        && root.algebra.contravariant(un) == un
        && root.algebra.contravariant(none) == none
        && root.algebra.invariant(sub) == un
        && root.algebra.invariant(sup) == un
        && root.algebra.invariant(eq) == eq
        && root.algebra.invariant(un) == un
        && root.algebra.invariant(none) == none
        && root.algebra.absorbing(sub) == sub
        && root.algebra.absorbing(sup) == sup
        && root.algebra.absorbing(eq) == eq
        && root.algebra.absorbing(un) == none
        && root.algebra.absorbing(none) == none
        && root.algebra.knowledge(root.algebra.start(root.algebra.int())) == eq
}

test "relations: transfer functions match the spec's variance rules" {
    assert.is_true(transfer_table_holds())
}

// ─── sites ───────────────────────────────────────────────────────────────────

function site_names() -> root.algebra.SiteNames throws never {
    root.algebra.SiteNames {
        function_name: "probe",
        value: "value",
        target: "target",
        sink: "sink",
        holder: "Holder",
    }
}

function site_case(
    site: root.algebra.SiteKind,
    source: root.algebra.Ty,
    target: root.algebra.Ty,
) -> root.engine.Case throws never {
    let p = root.algebra.probe(site, source, target, site_names());
    root.engine.Case {
        files: { "case.baml": p.source },
        key: root.engine.Compiles { tests: [] },
        trace: [],
    }
}

/// Every site must accept a value flowing into a slot of the same type.
function sites_accepting_identity() -> string[] throws never {
    root.algebra.all_sites().filter_map((site) -> {
        let outcome = root.engine.verify(site_case(site, root.algebra.int(), root.algebra.int()));
        if (root.engine.agrees(outcome)) {
            null
        } else {
            `${site}: ${outcome}`
        }
    })
}

/// Every site must reject a value of an unrelated type.
function sites_rejecting_unrelated() -> string[] throws never {
    root.algebra.all_sites().filter_map((site) -> {
        let outcome = root.engine.verify(site_case(site, root.algebra.string(), root.algebra.int()));
        if (root.engine.agrees(outcome)) {
            `${site}: compiled`
        } else {
            null
        }
    })
}

test "sites: every site accepts the identity flow" {
    assert.equal(sites_accepting_identity().join("\n"), "")
}

test "sites: every site rejects an unrelated flow" {
    assert.equal(sites_rejecting_unrelated().join("\n"), "")
}
