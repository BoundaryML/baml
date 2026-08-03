// Pooling: turn each side of a mapping row into a flat list of PooledMembers
// with deterministic match keys. Dedup/matching happens in matrix.baml.

/// The deterministic match key: lowercase with underscores stripped, so BAML
/// snake_case meets TS camelCase (`to_upper_case` ≡ `toUpperCase`). Explicit
/// per-row synonyms canonicalize the residue onto the TS key.
function match_key(name: string) -> string {
    name.to_lower_case().replace_all("_", "")
}

function apply_synonyms(key: string, synonyms: NamePair[]) -> string {
    let out = key;
    for (let pairing in synonyms) {
        if (match_key(pairing.baml_name) == key) {
            out = match_key(pairing.ts_name);
        }
    }
    out
}

/// `"<K>:<dotted.path>"` → `"<dotted.path>"`.
function strip_kind_prefix(id: string) -> string {
    let out = id;
    if let idx: int = id.index_of(":") {
        out = id.substring(idx + 1, id.length());
    }
    out
}

/// The container of an item id: everything up to the last dot of the
/// kind-stripped path — `"T:baml.time.Duration"` → `"baml.time"`,
/// `"V:baml._cleanup_begin"` → `"baml"`.
function container_of(id: string) -> string {
    let path = strip_kind_prefix(id);
    let cut = -1;
    let i = 0;
    while (i < path.length()) {
        if (path.char_at(i) == ".") {
            cut = i;
        }
        i += 1;
    }
    if (cut <= 0) {
        path
    } else {
        path.substring(0, cut)
    }
}

function last_segment(path: string) -> string {
    let cut = -1;
    let i = 0;
    while (i < path.length()) {
        if (path.char_at(i) == ".") {
            cut = i;
        }
        i += 1;
    }
    path.substring(cut + 1, path.length())
}

function first_doc_line(text: string?) -> string? {
    let out: string? = null;
    if let t: string = text {
        let all_lines = t.lines();
        if (all_lines.length() > 0) {
            out = all_lines.at(0);
        }
    }
    out
}

// ── export-side rendering helpers ───────────────────────────────────────────

function render_generics(generics: ExportGeneric[]?) -> string {
    let gs: ExportGeneric[] = generics ?? [];
    if (gs.length() == 0) {
        ""
    } else {
        let parts = gs.map(
            (g) -> {
                if (g.bounds.length() == 0) {
                    g.name
                } else {
                    `${g.name}: ${g.bounds.join(" + ")}`
                }
            },
        );
        `<${parts.join(", ")}>`
    }
}

function render_signature(sig: ExportSignature) -> string {
    let params = sig
        .params
        .map(
            (p) -> {
                `${p.name}: ${p.ty.display}`
            },
        )
        .join(", ");
    let ret = "";
    if let r: TyRef = sig.returns {
        ret = ` -> ${r.display}`;
    }
    let throws_suffix = "";
    if let spec: ExportThrows = sig.throws_spec {
        if let effective: TyRef = spec.effective {
            if (effective.display != "never") {
                throws_suffix = ` throws ${effective.display}`;
            }
        }
    }
    `${render_generics(sig.generics)}(${params})${ret}${throws_suffix}`
}

function throws_panics(sig: ExportSignature?) -> string[] {
    let out: string[] = [];
    if let s: ExportSignature = sig {
        if let spec: ExportThrows = s.throws_spec {
            for (let ty in spec.panics) {
                out.push(ty.display);
            }
        }
    }
    out
}

function throws_errors(sig: ExportSignature?) -> string[] {
    let out: string[] = [];
    if let s: ExportSignature = sig {
        if let spec: ExportThrows = s.throws_spec {
            for (let ty in spec.errors) {
                out.push(ty.display);
            }
        }
    }
    out
}

/// A BAML member is "static" when it takes no `self` receiver. Members whose
/// signature failed to resolve are treated as instance members.
function is_static_member(sig: ExportSignature?) -> bool {
    let out = false;
    if let s: ExportSignature = sig {
        if (s.params.length() == 0) {
            out = true;
        } else {
            if let first: ExportParam = s.params.at(0) {
                out = first.name != "self";
            }
        }
    }
    out
}

function rendered_signature(sig: ExportSignature?) -> string? {
    let out: string? = null;
    if let s: ExportSignature = sig {
        out = render_signature(s);
    }
    out
}

// ── pooling ─────────────────────────────────────────────────────────────────

class PoolAccumulator {
    members: PooledMember[],
    synthetic_hidden: int,
}

class ExportIndex {
    items: ExportItem[],
    by_id: map<string, ExportItem>,
    impl_by_id: map<string, ExportImpl>,
}

function index_export(doc: PackageExportDoc) -> ExportIndex {
    let by_id: map<string, ExportItem> = {};
    for (let item in doc.items) {
        let _ = by_id.set(item.id, item);
    }
    let impl_by_id: map<string, ExportImpl> = {};
    for (let impl_record in doc.impls) {
        let _ = impl_by_id.set(impl_record.id, impl_record);
    }
    ExportIndex { items: doc.items, by_id: by_id, impl_by_id: impl_by_id }
}

function push_method(
    acc: PoolAccumulator,
    row: ContainerPair,
    origin: string,
    prefix: string,
    method: ExportMethod,
    via: string?,
) -> void {
    if (method.name.includes("$")) {
        acc.synthetic_hidden += 1;
    } else {
        acc.members.push(PooledMember {
            key: apply_synonyms(match_key(method.name), row.synonyms),
            display: `${prefix}.${method.name}`,
            origin: origin,
            static: is_static_member(method.signature),
            signature: rendered_signature(method.signature),
            doc: first_doc_line(method.docstring),
            via: via,
            panics: throws_panics(method.signature),
            errors: throws_errors(method.signature),
        });
    }
}

/// Pools the member-bearing parts of one item: fields, own/interface methods,
/// and (for classes) methods contributed by attached impls. Class methods win
/// name collisions with impl methods, mirroring resolution order.
function pool_item_members(
    index: ExportIndex,
    item: ExportItem,
    row: ContainerPair,
    origin: string,
    acc: PoolAccumulator,
) -> void {
    let prefix = item.name;
    let own_names: map<string, bool> = {};

    let fields: ExportField[] = item.fields ?? [];
    for (let field in fields) {
        if (field.name.includes("$")) {
            acc.synthetic_hidden += 1;
        } else {
            let field_ty: string? = null;
            if let ty: TyRef = field.ty {
                field_ty = ty.display;
            }
            acc.members.push(PooledMember {
                key: apply_synonyms(match_key(field.name), row.synonyms),
                display: `${prefix}.${field.name}`,
                origin: origin,
                static: false,
                signature: field_ty,
                doc: first_doc_line(field.docstring),
                via: null,
                panics: [],
                errors: [],
            });
            let _ = own_names.set(field.name, true);
        }
    }

    let methods: ExportMethod[] = item.methods ?? [];
    for (let method in methods) {
        push_method(acc, row, origin, prefix, method, null);
        let _ = own_names.set(method.name, true);
    }
    let required_methods: ExportMethod[] = item.required_methods ?? [];
    for (let method in required_methods) {
        push_method(acc, row, origin, prefix, method, null);
        let _ = own_names.set(method.name, true);
    }
    let default_methods: ExportMethod[] = item.default_methods ?? [];
    for (let method in default_methods) {
        push_method(acc, row, origin, prefix, method, null);
        let _ = own_names.set(method.name, true);
    }

    let impl_ids: string[] = item.impls ?? [];
    for (let impl_id in impl_ids) {
        if let impl_record: ExportImpl = index.impl_by_id.get(impl_id) {
            for (let impl_method in impl_record.methods) {
                if (!own_names.has(impl_method.name)) {
                    if (impl_method.name.includes("$")) {
                        acc.synthetic_hidden += 1;
                    } else {
                        acc.members.push(PooledMember {
                            key: apply_synonyms(match_key(impl_method.name), row.synonyms),
                            display: `${prefix}.${impl_method.name}`,
                            origin: origin,
                            static: is_static_member(impl_method.signature),
                            signature: rendered_signature(impl_method.signature),
                            doc: first_doc_line(impl_method.docstring),
                            via: impl_record.interface,
                            panics: throws_panics(impl_method.signature),
                            errors: throws_errors(impl_method.signature),
                        });
                    }
                }
            }
        } else {
            throw baml.errors.InvalidArgument {
                message: `export inconsistency: ${item.id} references unknown impl ${impl_id}`,
            };
        }
    }
}

function pool_baml_type(
    index: ExportIndex,
    qualified: string,
    row: ContainerPair,
    acc: PoolAccumulator,
) -> void {
    if let item: ExportItem = index.by_id.get(`T:${qualified}`) {
        pool_item_members(index, item, row, qualified, acc);
    } else {
        throw baml.errors.InvalidArgument {
            message: `mapping row ${row.key}: no exported type ${qualified}`,
        };
    }
}

/// Pools every member-bearing item of a namespace: free functions by their own
/// name, class/interface members qualified by their type. Enums and type
/// aliases contribute no members (they are types, not callables).
function pool_baml_namespace(
    index: ExportIndex,
    ns: string,
    row: ContainerPair,
    acc: PoolAccumulator,
) -> void {
    let ns_short = last_segment(ns);
    for (let item in index.items) {
        if (container_of(item.id) != ns) {
            continue;
        }
        if (item.name.includes("$")) {
            acc.synthetic_hidden += 1;
            continue;
        }
        if (item.kind == "function") {
            acc.members.push(PooledMember {
                key: apply_synonyms(match_key(item.name), row.synonyms),
                display: `${ns_short}.${item.name}`,
                origin: ns,
                static: false,
                signature: rendered_signature(item.signature),
                doc: first_doc_line(item.docstring),
                via: null,
                panics: throws_panics(item.signature),
                errors: throws_errors(item.signature),
            });
        } else if (item.kind == "class" || item.kind == "interface") {
            pool_item_members(index, item, row, ns, acc);
        }
    }
}

function pool_baml_side(index: ExportIndex, row: ContainerPair) -> PoolAccumulator {
    let acc = PoolAccumulator { members: [], synthetic_hidden: 0 };
    for (let qualified in row.baml_types) {
        pool_baml_type(index, qualified, row, acc);
    }
    for (let ns in row.baml_namespaces) {
        pool_baml_namespace(index, ns, row, acc);
    }
    acc
}

function pool_ts_side(doc: TsSurfaceDoc, row: ContainerPair) -> PooledMember[] {
    let out: PooledMember[] = [];
    for (let cname in row.ts_containers) {
        let found = false;
        for (let container in doc.containers) {
            if (container.name == cname) {
                found = true;
                for (let member in container.members) {
                    out.push(PooledMember {
                        key: match_key(member.name),
                        display: `${cname}.${member.name}`,
                        origin: cname,
                        static: member.static,
                        signature: member.signatures.join("\n"),
                        doc: member.doc,
                        via: null,
                        panics: [],
                        errors: [],
                    });
                }
            }
        }
        if (!found) {
            throw baml.errors.InvalidArgument {
                message: `mapping row ${row.key}: TS container ${cname} not in surface document`,
            };
        }
    }
    out
}
