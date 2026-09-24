// Spelling a `Ty` and its declarations as BAML source.
//
// This is the one piece of language surface the tool owns. It is kept honest
// by the conformance suite: every corpus spelling must be read back by the
// compiler as the intended type in both the class-field and the
// `let`-annotation grammars, and printed back as the same text up to the
// compiler's documented canonicalisations.

/// The source spelling of `ty`.
function render(ty: Ty) -> string throws never {
    match (ty) {
        let p: Prim => render_prim(p.kind),
        Never => "never",
        Top => "unknown",
        let l: Lit => render_lit(l.value),
        let e: EnumLit => `${e.decl.name}.${e.variant}`,
        let u: Union => {
            u.members()
                .map((m) -> {
                    render_atom(m)
                })
                .join(" | ")
        },
        let a: Array => `${render_atom(a.element)}[]`,
        let m: Map => `map<string, ${render(m.value)}>`,
        let f: Fn => render_fn(f),
        let c: Class => `${c.decl.name}${render_args(c.args)}`,
        let e: Enum => e.decl.name,
        let i: Iface => render_iface(i),
        let a: Alias => a.decl.name,
        let v: Var => v.name,
    }
}

function render_prim(kind: PrimKind) -> string throws never {
    match (kind) {
        PrimKind.Int => "int",
        PrimKind.Float => "float",
        PrimKind.Bool => "bool",
        PrimKind.String => "string",
        PrimKind.Bigint => "bigint",
        PrimKind.Null => "null",
        PrimKind.Uint8Array => "uint8array",
    }
}

function render_lit(value: int | string | bool) -> string throws never {
    match (value) {
        let i: int => `${i}`,
        let s: string => quote(s),
        let b: bool => `${b}`,
    }
}

/// A double-quoted string literal. Backslash, quote, newline, carriage
/// return and tab are escaped; other control characters have no escape here
/// and must not appear in a literal.
function quote(s: string) -> string throws never {
    let escaped = s.replace_all("\\", "\\\\")
        .replace_all("\"", "\\\"")
        .replace_all("\n", "\\n")
        .replace_all("\r", "\\r")
        .replace_all("\t", "\\t");
    `"${escaped}"`
}

/// A type in a position that binds tighter than `|` and `->`: a union member,
/// an array element, a function's return type. Unions and function types
/// take parentheses there; everything else is atomic already.
function render_atom(ty: Ty) -> string throws never {
    match (ty) {
        Union | Fn => `(${render(ty)})`,
        Prim | Never | Top | Lit | EnumLit | Array | Map | Class | Enum | Iface | Alias | Var => {
            render(ty)
        },
    }
}

function render_args(args: Ty[]) -> string throws never {
    if (args.length() == 0) {
        ""
    } else {
        `<${args.map((a) -> { render(a) }).join(", ")}>`
    }
}

/// The return type is spelled as an atom: in a `let` annotation the grammar
/// binds `|` looser than `->`, so a bare union return does not parse there.
/// The `throws` type is delimited by the end of the type and parses bare in
/// every position.
function render_fn(f: Fn) -> string throws never {
    let params = f.positional
        .map((t) -> {
            render(t)
        })
        .concat(
            f.named.map((n) -> {
                render_named(n)
            }),
        );
    `(${params.join(", ")}) -> ${render_atom(f.ret)} throws ${render(f.error)}`
}

function render_named(n: Named) -> string throws never {
    if (n.optional) {
        `${n.name}?: ${render(n.ty)}`
    } else {
        `${n.name}: ${render(n.ty)}`
    }
}

/// Generic arguments and pins share one angle-bracket list, arguments first:
/// `Lorem<int, Item = string>`.
function render_iface(i: Iface) -> string throws never {
    let all = i.args
        .map((a) -> {
            render(a)
        })
        .concat(
            i.pins.map((p) -> {
                `${p.name} = ${render(p.ty)}`
            }),
        );
    if (all.length() == 0) {
        i.decl.name
    } else {
        `${i.decl.name}<${all.join(", ")}>`
    }
}

// ─── declarations ────────────────────────────────────────────────────────────

function render_decl(decl: Decl) -> string throws never {
    match (decl) {
        let d: ClassDecl => render_class_decl(d),
        let d: EnumDecl => render_enum_decl(d),
        let d: IfaceDecl => render_iface_decl(d),
        let d: AliasDecl => `type ${d.name} = ${render(d.body)};`,
    }
}

function render_params(params: string[]) -> string throws never {
    if (params.length() == 0) {
        ""
    } else {
        `<${params.join(", ")}>`
    }
}

function render_class_decl(d: ClassDecl) -> string throws never {
    let fields = d.fields
        .map((f) -> {
            "    " + `${f.name}: ${render(f.ty)},\n`
        })
        .join("");
    let impls = d.impls
        .map((i) -> {
            render_impl(i)
        })
        .join("");
    `class ${d.name}${render_params(d.params)} {\n${fields}${impls}}`
}

/// An in-body `implements` block. Its associated-type pins are statements
/// inside the block (`type Held = int`), not arguments in the angle-bracket
/// list an existential spells them in.
function render_impl(i: Impl) -> string throws never {
    let head = "    " + `implements ${i.iface.name}${render_args(i.args)}`;
    if (i.pins.length() == 0) {
        `${head} {}\n`
    } else {
        let pins = i.pins
            .map((p) -> {
                "        " + `type ${p.name} = ${render(p.ty)}\n`
            })
            .join("");
        `${head} {\n${pins}    }\n`
    }
}

function render_enum_decl(d: EnumDecl) -> string throws never {
    let variants = d.variants
        .map((v) -> {
            "    " + `${v},\n`
        })
        .join("");
    `enum ${d.name} {\n${variants}}`
}

function render_iface_decl(d: IfaceDecl) -> string throws never {
    let assoc = d.assoc
        .map((a) -> {
            match (a.default) {
                let t: Ty => "    " + `type ${a.name} = ${render(t)}\n`,
                null => "    " + `type ${a.name}\n`,
            }
        })
        .join("");
    `interface ${d.name}${render_params(d.params)}${render_requires(d.bounds)} {\n${assoc}}`
}

/// The `requires` clause, or nothing. Bounds are comma-separated: the
/// language accepts neither `+` between them nor a repeated `requires`.
function render_requires(bounds: Iface[]) -> string throws never {
    if (bounds.length() == 0) {
        ""
    } else {
        let names = bounds
            .map((i) -> {
                render_iface(i)
            })
            .join(", ");
        " " + `requires ${names}`
    }
}

/// The type parameters a declaration must bind to spell `vars`, as
/// `<T extends Base, U>`, or nothing when there are none.
function render_binders(vars: Var[]) -> string throws never {
    if (vars.length() == 0) {
        ""
    } else {
        let bound = vars
            .map((v) -> {
                render_binder(v)
            })
            .join(", ");
        `<${bound}>`
    }
}

function render_binder(v: Var) -> string throws never {
    match (v.bound) {
        let i: Iface => `${v.name} extends ${render_iface(i)}`,
        null => v.name,
    }
}

/// Every declaration `ty` refers to, transitively, in first-seen order.
function decls_of(ty: Ty) -> Decl[] throws never {
    decls_of_all([ty])
}

/// The declarations of every type in `tys`, deduplicated together, in
/// first-seen order. Recursive aliases are visited once, so cycles
/// terminate. Two declarations that share a name must be the same
/// declaration, since one file cannot hold both; anything else is a
/// generator bug. (Structural `==` is coinductive and terminates on the
/// cyclic alias values.)
///
/// # Panics
///
/// - `baml.panics.Unreachable` if two declarations share a name but differ.
function decls_of_all(tys: Ty[]) -> Decl[] throws never {
    let out: Decl[] = [];
    let seen: map<string, Decl> = {};
    for (let ty in tys) {
        collect(ty, out, seen);
    }
    out
}

function collect(ty: Ty, out: Decl[], seen: map<string, Decl>) -> void throws never {
    match (ty) {
        let e: EnumLit => visit(e.decl, out, seen),
        let c: Class => visit(c.decl, out, seen),
        let e: Enum => visit(e.decl, out, seen),
        let i: Iface => visit(i.decl, out, seen),
        let a: Alias => visit(a.decl, out, seen),
        let v: Var => {
            match (v.bound) {
                let i: Iface => collect(i, out, seen),
                null => {},
            }
        },
        Prim | Never | Top | Lit | Union | Array | Map | Fn => {},
    }
    for (let child in children(ty)) {
        collect(child, out, seen);
    }
}

function visit(decl: Decl, out: Decl[], seen: map<string, Decl>) -> void throws never {
    let name = decl_name(decl);
    match (seen.get(name)) {
        let prior: Decl => {
            if (prior != decl) {
                throw baml.panics.Unreachable { message: `two different declarations named ${name} in one case` }
            }
        },
        null => {
            seen.set(name, decl);
            out.push(decl);
            match (decl) {
                let d: ClassDecl => {
                    for (let f in d.fields) {
                        collect(f.ty, out, seen);
                    }
                    for (let i in d.impls) {
                        visit(i.iface, out, seen);
                        for (let arg in i.args) {
                            collect(arg, out, seen);
                        }
                        for (let pin in i.pins) {
                            collect(pin.ty, out, seen);
                        }
                    }
                },
                EnumDecl => {},
                let d: IfaceDecl => {
                    for (let bound in d.bounds) {
                        collect(bound, out, seen);
                    }
                    for (let a in d.assoc) {
                        match (a.default) {
                            let t: Ty => collect(t, out, seen),
                            null => {},
                        }
                    }
                },
                let d: AliasDecl => collect(d.body, out, seen),
            }
        },
    }
}

/// The names of every declaration `tys` refer to: what a further declaration
/// drawn beside them must not be called.
function decl_names_of(tys: Ty[]) -> string[] throws never {
    decls_of_all(tys).map((decl) -> {
        decl_name(decl)
    })
}

/// Whether the alias refers back to itself through its body.
function is_recursive(decl: AliasDecl) -> bool throws never {
    decls_of(decl.body).some((d) -> {
        decl_name(d) == decl.name
    })
}

/// All declarations rendered in order, each followed by a blank line.
function render_decls(decls: Decl[]) -> string throws never {
    decls
        .map((d) -> {
            `${render_decl(d)}\n\n`
        })
        .join("")
}
