// Conformance suite for the algebra: the rendering round trip in both
// grammars the sites use, the kind bridge, the relation transfers, and the
// sites, each checked against the compiler rather than against expectations
// written by hand.

// ─── a fixed corpus of types ─────────────────────────────────────────────────

function box_decl() -> root.algebra.ClassDecl throws never {
    root.algebra.ClassDecl {
        name: "Box",
        params: ["T"],
        fields: [root.algebra.FieldDecl { name: "value", ty: root.algebra.Var.new("T") }],
    }
}

function pair_decl() -> root.algebra.ClassDecl throws never {
    root.algebra.ClassDecl {
        name: "Pair",
        params: [],
        fields: [
            root.algebra.FieldDecl { name: "left", ty: root.algebra.Prim.int() },
            root.algebra.FieldDecl { name: "right", ty: root.algebra.Prim.string() },
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
        assoc: [root.algebra.AssocDecl { name: "Item", default: root.algebra.Prim.int() }],
    }
}

function lorem_decl() -> root.algebra.IfaceDecl throws never {
    root.algebra.IfaceDecl {
        name: "Lorem",
        params: ["T"],
        assoc: [root.algebra.AssocDecl { name: "Out", default: null }],
    }
}

function rec_decl() -> root.algebra.AliasDecl throws never {
    root.algebra.AliasDecl.recursive("Rec", (self) -> {
        root.algebra.Union.new([root.algebra.Prim.int(), root.algebra.Array.new(self)])
    })
}

function id_decl() -> root.algebra.AliasDecl throws never {
    root.algebra.AliasDecl { name: "Id", body: root.algebra.Prim.int() }
}

class Corpus {
    name: string,
    ty: root.algebra.Ty,
    /// What the compiler prints for the type when that differs from the
    /// rendering: it flattens nested unions, spells defaulted interface pins,
    /// drops the parentheses this renderer puts around a union or function
    /// return type, and drops parameter names and `?` from function types.
    /// Both spellings denote the same type; recording the difference means a
    /// change in the compiler's printing is noticed.
    printed: string?,
}

function entry(name: string, ty: root.algebra.Ty) -> Corpus throws never {
    Corpus { name: name, ty: ty, printed: null }
}

function printed_as(name: string, ty: root.algebra.Ty, printed: string) -> Corpus throws never {
    Corpus { name: name, ty: ty, printed: printed }
}

function fn1() -> root.algebra.Ty throws never {
    root.algebra.Fn.new(
        [root.algebra.Prim.int()],
        [],
        root.algebra.Prim.string(),
        root.algebra.Never {  },
    )
}

function int_or_string() -> root.algebra.Ty throws never {
    root.algebra.Union.new([root.algebra.Prim.int(), root.algebra.Prim.string()])
}

/// Types whose rendering must survive a trip through the compiler in both
/// grammars. Every `Ty` variant except `Var` appears, and every constructor
/// appears in every position where precedence could matter: union member,
/// array element, map value, generic argument, interface argument and pin,
/// function parameter, return, and throws type.
function corpus() -> Corpus[] throws never {
    [
        entry("int", root.algebra.Prim.int()),
        entry("float", root.algebra.Prim.float()),
        entry("bool", root.algebra.Prim.bool()),
        entry("string", root.algebra.Prim.string()),
        entry("bigint", root.algebra.Prim.bigint()),
        entry("null", root.algebra.Prim.null_ty()),
        entry("uint8array", root.algebra.Prim.uint8array()),
        entry("never", root.algebra.Never {  }),
        entry("unknown", root.algebra.Top {  }),
        entry("lit int", root.algebra.Lit.new(1)),
        entry("lit negative", root.algebra.Lit.new(-2)),
        entry("lit string", root.algebra.Lit.new("sheep")),
        entry("lit escaped", root.algebra.Lit.new("say \"baa\"")),
        entry("lit newline", root.algebra.Lit.new("a\nb")),
        entry("lit bool", root.algebra.Lit.new(true)),
        entry("union", int_or_string()),
        entry("optional", root.algebra.Union.optional(root.algebra.Prim.string())),
        entry(
            "union of literals",
            root.algebra.Union.new(
                [root.algebra.Lit.new(1), root.algebra.Lit.new(2), root.algebra.Prim.int()],
            ),
        ),
        printed_as(
            "nested union",
            root.algebra.Union.new(
                [
                    root.algebra.Prim.int(),
                    root.algebra.Union.new([root.algebra.Prim.string(), root.algebra.Prim.bool()]),
                ],
            ),
            "int | string | bool",
        ),
        entry("array", root.algebra.Array.new(root.algebra.Prim.int())),
        entry("array of union", root.algebra.Array.new(int_or_string())),
        entry("array of array", root.algebra.Array.new(root.algebra.Array.new(root.algebra.Prim.int()))),
        entry("map", root.algebra.Map.new(root.algebra.Prim.int())),
        entry("map of union", root.algebra.Map.new(root.algebra.Union.optional(root.algebra.Prim.int()))),
        entry("map of fn", root.algebra.Map.new(fn1())),
        entry("fn", fn1()),
        entry("fn in union", root.algebra.Union.new([root.algebra.Prim.string(), fn1()])),
        entry("array of fn", root.algebra.Array.new(fn1())),
        printed_as(
            "fn returning union",
            root.algebra.Fn.new(
                [root.algebra.Prim.int()],
                [],
                int_or_string(),
                root.algebra.Never {  },
            ),
            "(int) -> int | string throws never",
        ),
        printed_as(
            "fn returning fn",
            root.algebra.Fn.new([], [], fn1(), root.algebra.Never {  }),
            "() -> ((int) -> string throws never) throws never",
        ),
        entry("fn throwing union", root.algebra.Fn.new([], [], root.algebra.Prim.int(), int_or_string())),
        entry("fn throwing fn", root.algebra.Fn.new([], [], root.algebra.Prim.int(), fn1())),
        entry(
            "fn taking union",
            root.algebra.Fn.new(
                [int_or_string()],
                [],
                root.algebra.Prim.int(),
                root.algebra.Never {  },
            ),
        ),
        entry(
            "fn taking fn",
            root.algebra.Fn.new([fn1()], [], root.algebra.Prim.int(), root.algebra.Never {  }),
        ),
        entry(
            "fn no params",
            root.algebra.Fn.new([], [], root.algebra.Prim.null_ty(), root.algebra.Top {  }),
        ),
        printed_as(
            "fn named params",
            root.algebra.Fn.new(
                [],
                [
                    root.algebra.Named.new("a", root.algebra.Prim.int(), false),
                    root.algebra.Named.new("b", root.algebra.Prim.string(), true),
                ],
                root.algebra.Prim.int(),
                root.algebra.Never {  },
            ),
            "(int, string) -> int throws never",
        ),
        entry("class", root.algebra.Class.new(pair_decl(), [])),
        entry("generic class", root.algebra.Class.new(box_decl(), [root.algebra.Prim.int()])),
        entry("generic class of union", root.algebra.Class.new(box_decl(), [int_or_string()])),
        entry("generic class of fn", root.algebra.Class.new(box_decl(), [fn1()])),
        entry("enum", root.algebra.Enum.new(side_decl())),
        entry("enum literal", root.algebra.EnumLit.new(side_decl(), "Left")),
        printed_as("interface", root.algebra.Iface.new(iter_decl(), [], []), "Iter<Item = int>"),
        entry(
            "interface pinned",
            root.algebra.Iface.new(
                iter_decl(),
                [],
                [root.algebra.Pin { name: "Item", ty: root.algebra.Prim.string() }],
            ),
        ),
        entry(
            "generic interface pinned",
            root.algebra.Iface.new(
                lorem_decl(),
                [root.algebra.Prim.int()],
                [root.algebra.Pin { name: "Out", ty: fn1() }],
            ),
        ),
        entry("recursive alias", root.algebra.Alias { decl: rec_decl() }),
        entry(
            "array of recursive alias",
            root.algebra.Array.new(root.algebra.Alias { decl: rec_decl() }),
        ),
        printed_as("transparent alias", root.algebra.Alias { decl: id_decl() }, "int"),
    ]
}

/// The compiler's reading of `ty`'s rendering as a class field.
function spelled(ty: root.algebra.Ty) -> root.engine.Spelled throws never {
    root.engine.type_of_spelling(
        root.algebra.render_decls(root.algebra.decls_of(ty)),
        root.algebra.render(ty),
    )
}

function failure_text(name: string, spelled: root.engine.Spelled) -> string? throws never {
    match (spelled) {
        reflect.Type => null,
        let failed: root.engine.CompileFailed => {
            `${name}: did not compile: ${failed.diagnostics.map((d) -> { d.message }).join("; ")}`
        },
        let threw: root.engine.InitThrew => `${name}: ${threw.message}`,
    }
}

// ─── render is a fixed point of parse-then-print, in the field grammar ───────

function fixed_point_failures() -> string[] throws never {
    corpus().filter_map((c) -> {
        let rendered = root.algebra.render(c.ty);
        match (spelled(c.ty)) {
            let t: reflect.Type => {
                let expected = c.printed ?? rendered;
                if (t.to_string() == expected) {
                    null
                } else {
                    `${c.name}: rendered ${rendered}, expected print ${expected}, compiler printed ${t.to_string()}`
                }
            },
            root.engine.CompileFailed | root.engine.InitThrew => failure_text(c.name, spelled(c.ty)),
        }
    })
}

test "render: every corpus spelling is read back and printed as expected" {
    assert.equal(fixed_point_failures().join("\n"), "")
}

// ─── every corpus spelling parses in the let-annotation grammar ──────────────

/// The `let` annotation grammar differs from the field grammar (it binds `|`
/// looser than `->`), and it is the grammar the `Let` site uses.
function let_grammar_failures() -> string[] throws never {
    corpus().filter_map((c) -> {
        let decls = root.algebra.render_decls(root.algebra.decls_of(c.ty));
        let spelling = root.algebra.render(c.ty);
        let source = `
            ${decls}function probe(value: ${spelling}) -> void {
                let target: ${spelling} = value;
            }
        `;
        let outcome = root.engine.verify(
            root.engine.Case {
                files: { "case.baml": source },
                key: root.engine.Compiles { tests: [] },
                trace: [],
            },
        );
        if (root.engine.agrees(outcome)) {
            null
        } else {
            `${c.name}: ${spelling} -> ${outcome}`
        }
    })
}

test "render: every corpus spelling parses as a let annotation" {
    assert.equal(let_grammar_failures().join("\n"), "")
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
                root.engine.CompileFailed | root.engine.InitThrew => {
                    failure_text(c.name, spelled(c.ty))
                },
            },
        }
    })
}

/// Every primitive, literal, union, array and map entry must be reachable
/// through the constructors, so a constructor arm cannot silently go dead.
function unreachable_structural_entries() -> string[] throws never {
    corpus().filter_map((c) -> {
        let structural = match (c.ty) {
            root.algebra.Prim | root.algebra.Never | root.algebra.Top | root.algebra.Lit => true,
            root.algebra.Union | root.algebra.Array | root.algebra.Map => {
                root.algebra.children(c.ty).every((child) -> {
                    root.algebra.to_reflect(child) != null
                })
            },
            root.algebra.EnumLit
                | root.algebra.Fn
                | root.algebra.Class
                | root.algebra.Enum
                | root.algebra.Iface
                | root.algebra.Alias
                | root.algebra.Var =>
            {
                false
            },
        };
        if (structural && root.algebra.to_reflect(c.ty) == null) {
            c.name
        } else {
            null
        }
    })
}

test "render: structural spellings denote the constructed types" {
    assert.equal(constructor_failures().join("\n"), "")
}

test "render: every structural corpus entry is constructor-reachable" {
    assert.equal(unreachable_structural_entries().join("\n"), "")
}

// ─── kinds ───────────────────────────────────────────────────────────────────

function kind_failures() -> string[] throws never {
    corpus().filter_map((c) -> {
        match (root.algebra.expected_kind(c.ty)) {
            null => `${c.name}: corpus types must have a realized kind`,
            let expected: root.algebra.Kind => match (spelled(c.ty)) {
                let t: reflect.Type => {
                    if (root.algebra.kind_of(t) == expected) {
                        null
                    } else {
                        `${c.name}: expected ${expected}, compiler says ${root.algebra.kind_of(t)}`
                    }
                },
                root.engine.CompileFailed | root.engine.InitThrew => {
                    failure_text(c.name, spelled(c.ty))
                },
            },
        }
    })
}

/// Every `Kind` must be produced by some corpus entry, so a mis-mapped arm in
/// `kind_of` cannot hide behind an unexercised kind.
function kinds_never_seen() -> string[] throws never {
    let seen = corpus().filter_map((c) -> {
        root.algebra.expected_kind(c.ty)
    });
    [
        root.algebra.Kind.Class,
        root.algebra.Kind.Enum,
        root.algebra.Kind.Union,
        root.algebra.Kind.Literal,
        root.algebra.Kind.Array,
        root.algebra.Kind.Map,
        root.algebra.Kind.Interface,
        root.algebra.Kind.Primitive,
        root.algebra.Kind.Function,
    ]
        .filter_map((k) -> {
            if (seen.includes(k)) {
                null
            } else {
                `${k}`
            }
        })
}

test "kinds: the compiler classifies every corpus type as expected" {
    assert.equal(kind_failures().join("\n"), "")
}

test "kinds: every kind appears in the corpus" {
    assert.equal(kinds_never_seen().join("\n"), "")
}

// ─── equivalence is canonical while spelling is not ──────────────────────────

class Equivalence {
    left: root.algebra.Ty,
    right: root.algebra.Ty,
}

function equivalences() -> Equivalence[] throws never {
    [
        Equivalence {
            left: int_or_string(),
            right: root.algebra.Union.new([root.algebra.Prim.string(), root.algebra.Prim.int()]),
        },
        Equivalence {
            left: root.algebra.Union.new([root.algebra.Lit.new(1), root.algebra.Prim.int()]),
            right: root.algebra.Prim.int(),
        },
        Equivalence {
            left: root.algebra.Union.new([root.algebra.Lit.new(true), root.algebra.Lit.new(false)]),
            right: root.algebra.Prim.bool(),
        },
        Equivalence {
            left: root.algebra.Union.optional(root.algebra.Prim.string()),
            right: root.algebra.Union.new([root.algebra.Prim.string(), root.algebra.Prim.null_ty()]),
        },
        Equivalence {
            left: root.algebra.Union.new(
                [
                    root.algebra.Prim.int(),
                    root.algebra.Union.new([root.algebra.Prim.string(), root.algebra.Prim.bool()]),
                ],
            ),
            right: root.algebra.Union.new(
                [root.algebra.Prim.int(), root.algebra.Prim.string(), root.algebra.Prim.bool()],
            ),
        },
        Equivalence {
            left: root.algebra.Union.new([root.algebra.Prim.int(), root.algebra.Never {  }]),
            right: root.algebra.Prim.int(),
        },
        Equivalence {
            left: root.algebra.Union.new([root.algebra.Prim.int(), root.algebra.Top {  }]),
            right: root.algebra.Top {  },
        },
    ]
}

function canonical_equalities() -> string[] throws never {
    equivalences().filter_map((e) -> {
        match (spelled(e.left)) {
            let l: reflect.Type => match (spelled(e.right)) {
                let r: reflect.Type => {
                    if (l == r) {
                        null
                    } else {
                        `${l} != ${r}`
                    }
                },
                root.engine.CompileFailed | root.engine.InitThrew => {
                    failure_text(root.algebra.render(e.right), spelled(e.right))
                },
            },
            root.engine.CompileFailed | root.engine.InitThrew => {
                failure_text(root.algebra.render(e.left), spelled(e.left))
            },
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
}

/// `in_union_with` declines when the extra member could absorb the difference.
function union_context_declines_absorbers() -> bool throws never {
    let rule = root.bank.union_member_covariant();
    let base = root.algebra.pair(
        root.algebra.Never {  },
        root.algebra.Prim.int(),
        root.algebra.Relation.Sub,
        root.bank.never_bottom_rule(),
    );
    let disjoint = root.algebra.knowledge(root.algebra.in_union_with(base, root.algebra.Prim.string(), rule));
    let absorbing_member = root.algebra.knowledge(root.algebra.in_union_with(base, root.algebra.Prim.int(), rule));
    let non_leaf = root.algebra.knowledge(
        root.algebra.in_union_with(base, root.algebra.Array.new(root.algebra.Prim.string()), rule),
    );
    let wide = root.algebra.pair(
        root.algebra.Lit.new(1),
        int_or_string(),
        root.algebra.Relation.Sub,
        root.bank.union_member_rule(),
    );
    let member_of_side = root.algebra.knowledge(root.algebra.in_union_with(wide, root.algebra.Prim.string(), rule));
    disjoint == root.algebra.Relation.Sub
        && absorbing_member == root.algebra.Undecided {  }
        && non_leaf == root.algebra.Undecided {  }
        && member_of_side == root.algebra.Undecided {  }
}

test "relations: transfer functions match the spec's variance rules" {
    assert.is_true(transfer_table_holds())
}

test "relations: a union context declines an extra member that could absorb the difference" {
    assert.is_true(union_context_declines_absorbers())
}

// ─── sites ───────────────────────────────────────────────────────────────────

function site_names() -> root.algebra.SiteNames throws never {
    root.algebra.SiteNames {
        function_name: "probe",
        value: "value",
        target: "target",
        sink: "sink",
        instance: "instance",
        holder: "Holder",
    }
}

/// A flow the algebra has an opinion about, and the verdict that opinion
/// implies.
class Flow {
    name: string,
    source: root.algebra.Ty,
    target: root.algebra.Ty,
    compiles: bool,
}

function flows() -> Flow[] throws never {
    let rec = root.algebra.Alias { decl: rec_decl() };
    let unfolded = root.algebra.Union.new([root.algebra.Prim.int(), root.algebra.Array.new(rec)]);
    let returns_int = root.algebra.Fn.new([], [], root.algebra.Prim.int(), root.algebra.Never {  });
    let returns_union = root.algebra.Fn.new([], [], int_or_string(), root.algebra.Never {  });
    [
        Flow {
            name: "identity",
            source: root.algebra.Prim.int(),
            target: root.algebra.Prim.int(),
            compiles: true,
        },
        Flow {
            name: "unrelated",
            source: root.algebra.Prim.string(),
            target: root.algebra.Prim.int(),
            compiles: false,
        },
        Flow {
            name: "member into union",
            source: root.algebra.Prim.int(),
            target: int_or_string(),
            compiles: true,
        },
        Flow {
            name: "union into member",
            source: int_or_string(),
            target: root.algebra.Prim.int(),
            compiles: false,
        },
        Flow {
            name: "literal into concrete",
            source: root.algebra.Lit.new(1),
            target: root.algebra.Prim.int(),
            compiles: true,
        },
        Flow {
            name: "concrete into literal",
            source: root.algebra.Prim.int(),
            target: root.algebra.Lit.new(1),
            compiles: false,
        },
        Flow {
            name: "never into leaf",
            source: root.algebra.Never {  },
            target: root.algebra.Prim.int(),
            compiles: true,
        },
        Flow {
            name: "leaf into never",
            source: root.algebra.Prim.int(),
            target: root.algebra.Never {  },
            compiles: false,
        },
        Flow {
            name: "leaf into unknown",
            source: root.algebra.Prim.int(),
            target: root.algebra.Top {  },
            compiles: true,
        },
        Flow {
            name: "unknown into leaf",
            source: root.algebra.Top {  },
            target: root.algebra.Prim.int(),
            compiles: false,
        },
        Flow { name: "alias into unfolding", source: rec, target: unfolded, compiles: true },
        Flow { name: "unfolding into alias", source: unfolded, target: rec, compiles: true },
        Flow {
            name: "covariant return",
            source: returns_int,
            target: returns_union,
            compiles: true,
        },
        Flow {
            name: "covariant return reversed",
            source: returns_union,
            target: returns_int,
            compiles: false,
        },
    ]
}

function site_case(site: root.algebra.SiteKind, flow: Flow) -> root.engine.Case throws never {
    let p = root.algebra.probe(site, flow.source, flow.target, site_names());
    let key: root.engine.Verdict = if (flow.compiles) {
        root.engine.Compiles { tests: [] }
    } else {
        root.engine.Rejected { codes: ["E0001"], anchor: p.anchor }
    };
    root.engine.Case { files: { "case.baml": p.source }, key: key, trace: [] }
}

/// Every site must agree with the algebra's verdict for every flow, with the
/// rejection code and the anchor pinned.
function site_disagreements() -> string[] throws never {
    flows().flat_map((flow) -> {
        root.algebra.all_sites().filter_map((site) -> {
            let outcome = root.engine.verify(site_case(site, flow));
            if (root.engine.agrees(outcome)) {
                null
            } else {
                `${flow.name} at ${site}: ${outcome}`
            }
        })
    })
}

test "sites: every site agrees with the algebra on every flow, code and anchor included" {
    assert.equal(site_disagreements().join("\n"), "")
}
