// Spelling a `Ty` and its declarations as BAML source.
//
// This is the one piece of language surface the tool owns. It is kept honest
// by the conformance suite: the compiler must read every rendering back as
// the same type, and must print it back as the same text.

/// The source spelling of `ty`.
function render(ty: Ty) -> string throws never {
    match (ty) {
        let p: Prim => render_prim(p.kind),
        Never => "never",
        Top => "unknown",
        let l: Lit => render_lit(l.value),
        let e: EnumLit => `${e.decl.name}.${e.variant}`,
        let u: Union => {
            u.members
                .map((m) -> {
                    render_union_member(m)
                })
                .join(" | ")
        },
        let a: Array => `${render_atom(a.element)}[]`,
        let m: Map => `map<${render(m.key)}, ${render(m.value)}>`,
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

/// A double-quoted string literal with `\\` and `"` escaped. Generated
/// literals are drawn from name pools, so no other escapes are needed; a
/// control character here would be a generator bug, not a rendering case.
function quote(s: string) -> string throws never {
    `"${s.replace_all("\\", "\\\\").replace_all("\"", "\\\"")}"`
}

/// A union member: nested unions and function types need parentheses.
function render_union_member(ty: Ty) -> string throws never {
    match (ty) {
        Union => `(${render(ty)})`,
        Fn => `(${render(ty)})`,
        Prim => render(ty),
        Never => render(ty),
        Top => render(ty),
        Lit => render(ty),
        EnumLit => render(ty),
        Array => render(ty),
        Map => render(ty),
        Class => render(ty),
        Enum => render(ty),
        Iface => render(ty),
        Alias => render(ty),
        Var => render(ty),
    }
}

/// A type in a position that binds tighter than `|` and `->`, such as an
/// array element.
function render_atom(ty: Ty) -> string throws never {
    render_union_member(ty)
}

function render_args(args: Ty[]) -> string throws never {
    if (args.length() == 0) {
        ""
    } else {
        `<${args.map((a) -> { render(a) }).join(", ")}>`
    }
}

function render_fn(f: Fn) -> string throws never {
    let params = f.params
        .map((p) -> {
            render_param(p)
        })
        .join(", ");
    `(${params}) -> ${render(f.ret)} throws ${render(f.error)}`
}

function render_param(p: Param) -> string throws never {
    match (p) {
        let pos: Positional => render(pos.ty),
        let n: Named => {
            if (n.optional) {
                `${n.name}?: ${render(n.ty)}`
            } else {
                `${n.name}: ${render(n.ty)}`
            }
        },
    }
}

/// Generic arguments and pins share one angle-bracket list, arguments first:
/// `Lorem<int, Item = string>`.
function render_iface(i: Iface) -> string throws never {
    let args = i.args.map((a) -> {
        render(a)
    });
    let pins = i.pins.map((p) -> {
        `${p.name} = ${render(p.ty)}`
    });
    let all = args.concat(pins);
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
            `    ${f.name}: ${render(f.ty)},\n`
        })
        .join("");
    `class ${d.name}${render_params(d.params)} {\n${fields}}`
}

function render_enum_decl(d: EnumDecl) -> string throws never {
    let variants = d.variants
        .map((v) -> {
            `    ${v},\n`
        })
        .join("");
    `enum ${d.name} {\n${variants}}`
}

function render_iface_decl(d: IfaceDecl) -> string throws never {
    let assoc = d.assoc
        .map((a) -> {
            match (a.default) {
                let t: Ty => `    type ${a.name} = ${render(t)}\n`,
                null => `    type ${a.name}\n`,
            }
        })
        .join("");
    `interface ${d.name}${render_params(d.params)} {\n${assoc}}`
}

/// Every declaration `ty` refers to, transitively, in first-seen order.
///
/// Recursive aliases are visited once, so cycles terminate. Two declarations
/// that share a name must render identically, since one file cannot hold
/// both; anything else is a generator bug. Renderings are compared rather
/// than the declarations themselves because a recursive alias is a cyclic
/// value and structural equality would not terminate on it.
///
/// # Panics
///
/// - `baml.panics.Unreachable` if two declarations share a name but differ.
function decls_of(ty: Ty) -> Decl[] throws never {
    let out: Decl[] = [];
    let seen: map<string, Decl> = {};
    collect(ty, out, seen);
    out
}

/// The declarations of every type in `tys`, deduplicated together.
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
        Prim => {},
        Never => {},
        Top => {},
        Lit => {},
        let e: EnumLit => visit(e.decl, out, seen),
        let u: Union => {
            for (let m in u.members) {
                collect(m, out, seen);
            }
        },
        let a: Array => collect(a.element, out, seen),
        let m: Map => {
            collect(m.key, out, seen);
            collect(m.value, out, seen);
        },
        let f: Fn => {
            for (let p in f.params) {
                collect(param_ty(p), out, seen);
            }
            collect(f.ret, out, seen);
            collect(f.error, out, seen);
        },
        let c: Class => {
            visit(c.decl, out, seen);
            for (let a in c.args) {
                collect(a, out, seen);
            }
        },
        let e: Enum => visit(e.decl, out, seen),
        let i: Iface => {
            visit(i.decl, out, seen);
            for (let a in i.args) {
                collect(a, out, seen);
            }
            for (let p in i.pins) {
                collect(p.ty, out, seen);
            }
        },
        let a: Alias => visit(a.decl, out, seen),
        Var => {},
    }
}

function param_ty(p: Param) -> Ty throws never {
    match (p) {
        let pos: Positional => pos.ty,
        let n: Named => n.ty,
    }
}

function visit(decl: Decl, out: Decl[], seen: map<string, Decl>) -> void throws never {
    let name = decl_name(decl);
    match (seen.get(name)) {
        let prior: Decl => {
            if (render_decl(prior) != render_decl(decl)) {
                throw baml.panics.Unreachable {
                    message: `two different declarations named ${name} in one case`,
                }
            }
        },
        null => {
            let _ = seen.set(name, decl);
            let _ = out.push(decl);
            match (decl) {
                let d: ClassDecl => {
                    for (let f in d.fields) {
                        collect(f.ty, out, seen);
                    }
                },
                EnumDecl => {},
                let d: IfaceDecl => {
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

/// All declarations rendered in order, each followed by a blank line.
function render_decls(decls: Decl[]) -> string throws never {
    decls
        .map((d) -> {
            `${render_decl(d)}\n\n`
        })
        .join("")
}
