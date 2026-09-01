// The symbol layer: every addressable BAML stdlib symbol, flattened.
//
// The matrix used to be organized by container pair, which forced every
// question into "what does this container's residue look like". Alignment is
// really a question about *symbols* — does this BAML symbol have a TypeScript
// counterpart — so symbols are the unit here, and grouping is something the
// passes choose rather than something the data imposes.
//
// A symbol's `path` is structural (package, owner, member) while `display` is
// how it is written in source. They differ wherever the stdlib backs a
// primitive with a companion class: `["baml", "Uint8Array", "slice"]` is where
// the method lives, `uint8array.slice` is what a user types.

/// One step of a symbol path. Most steps are a plain name; a step that needs
/// more structure than a name can carry — an impl's receiver-plus-interface,
/// and later generic instantiations — is a node instead.
type PathStep = string | ImplStep;

/// The receiver side of an impl method: `implements baml.ops.Add for string`
/// reaches `add` on `string` *through* `baml.ops.Add`, and neither half alone
/// identifies it.
/// The field is `iface` in BAML and `interface` on the wire. A struct literal
/// naming a field `interface` does not parse, so a producer cannot build one —
/// and `@alias` is excluded from JSON interchange by BEP-038. The conversion
/// overrides below spell the key the format wants without the parser ever
/// seeing it in literal position.
class ImplStep {
    base: string,
    iface: string[],

    implements baml.ToJson {
        function to_json(self) -> baml.json.json {
            let segments: baml.json.json[] = [];
            for (let segment in self.iface) {
                segments.push(segment);
            }
            let out: map<string, baml.json.json> = {};
            let _ = out.set("base", self.base);
            let _ = out.set("interface", segments);
            out
        }
    }

    implements baml.FromJson {
        function from_json(j: baml.json.json) -> Self throws baml.json.DecodeError {
            ImplStep {
                base: baml.json.to<string>(baml.json.field(j, "base")),
                iface: baml.json.to<string[]>(baml.json.field(j, "interface")),
            }
        }
    }
}

class SignatureArg {
    name: string,
    ty: string,
}

/// A resolved signature. `args` are positional; `kwargs` are the
/// default-valued parameters, which BAML makes named-only. `errors` is the
/// effective throws contract — declared when written, inferred otherwise.
/// Panics are deliberately absent: they are not part of the signature and are
/// only sporadically documented.
class SymbolSignature {
    display: string,
    generic_params: string[],
    args: SignatureArg[],
    kwargs: map<string, string>,
    returns: string,
    errors: string,
}

/// How a symbol is reached. `free` is a namespace-level function, `method` and
/// `static_method` are declared on a type, `impl_method` arrives through an
/// interface impl, and `type`/`field`/`variant`/`assoc_type` are the
/// non-callable surface.
///
/// `kind` is what the symbol *is*, as declared — `class`, `enum`, `interface`,
/// `type`, `associated type`, `function`, `field`, `variant`. The two differ
/// because reach and declaration are different questions: a `static_method` and
/// a `method` are both `function`s.
///
/// A type alias and an associated type share the `type` keyword and nothing
/// else. An alias names a type: `type json = …` says the name *is* that type,
/// always. An associated type declares a slot each implementor fills, and the
/// interface may offer a default for one — which is a fallback, not a
/// definition. They are separate kinds carrying separate fields so that no
/// reader has to know which meaning a shared field held.
class BamlSymbol {
    /// The export's own id for this declaration: stable across runs, unique
    /// across the document, and what a judgement is recorded against.
    id: string,
    /// The declaration an impl entry re-lists, when it has an id of its own.
    /// Entries sharing one are the same code reached through different
    /// receivers.
    declared_by: string?,
    symbol: PathStep[],
    display: string,
    kind: string,
    origin: string,
    /// Declared type parameters, rendered as written (`T`, `T: baml.Comparable`).
    generics: string[],
    /// The type a field is declared to hold, written `name: T`. Null for every
    /// other kind.
    ty: string?,
    /// What a type alias names, fully resolved. Null for every other kind.
    resolved: string?,
    /// What an associated type falls back to when an implementor does not bind
    /// it. Null for every other kind, and for the associated types — most of
    /// them — that require a binding.
    default: string?,
    signature: SymbolSignature?,
    doc: string?,
    /// The ids of the symbols this declaration names — see references.baml.
    /// Filled after extraction, once the whole set is known.
    references: string[],
}

/// Containers are invariant, so a `string[]` is not a `PathStep[]` even though
/// every element would be valid — the steps are built element-wise.
function path_steps(qualified: string) -> PathStep[] {
    let steps: PathStep[] = [];
    for (let segment in qualified.split(".")) {
        steps.push(segment);
    }
    steps
}

function member_steps(qualified: string, member: string) -> PathStep[] {
    let steps = path_steps(qualified);
    steps.push(member);
    steps
}

/// Type parameters as written, with their bounds when they have any.
function declared_generics(generics: ExportGeneric[]?) -> string[] {
    let out: string[] = [];
    let declared: ExportGeneric[] = generics ?? [];
    for (let generic in declared) {
        if (generic.bounds.length() == 0) {
            out.push(generic.name);
        } else {
            out.push(`${generic.name}: ${generic.bounds.join(" + ")}`);
        }
    }
    out
}

/// A type reference that may be absent, normalized when it is not. The three
/// places a declaration mentions a type — a field's annotation, an alias's
/// target, an associated type's default — are each optional in the export.
function normalize_optional_ty(ty: TyRef?) -> string? {
    let out: string? = null;
    if let present: TyRef = ty {
        out = normalize_ty_display(present.display);
    }
    out
}

/// `(self: T, …)` marks an instance method; anything else on a type is static.
function origin_of_method(signature: ExportSignature?) -> string {
    if (is_static_member(signature)) {
        "static_method"
    } else {
        "method"
    }
}

function render_symbol_signature(
    generic_params: string[],
    args: SignatureArg[],
    kwargs: map<string, string>,
    returns: string,
    errors: string,
) -> string {
    let parts: string[] = [];
    for (let arg in args) {
        parts.push(`${arg.name}: ${arg.ty}`);
    }
    for (let name in kwargs.keys()) {
        parts.push(`${name}: ${kwargs.get(name) ?? "unknown"} = …`);
    }
    let generics = "";
    if (generic_params.length() > 0) {
        generics = `<${generic_params.join(", ")}>`;
    }
    let throws_suffix = "";
    if (errors != "never") {
        throws_suffix = ` throws ${errors}`;
    }
    `${generics}(${parts.join(", ")}) -> ${returns}${throws_suffix}`
}

function symbol_signature(signature: ExportSignature?) -> SymbolSignature? {
    let out: SymbolSignature? = null;
    if let sig: ExportSignature = signature {
        let args: SignatureArg[] = [];
        let kwargs: map<string, string> = {};
        for (let param in sig.params) {
            if (param.optional ?? false) {
                let _ = kwargs.set(param.name, normalize_ty_display(param.ty.display));
            } else {
                args.push(
                    SignatureArg { name: param.name, ty: normalize_ty_display(param.ty.display) },
                );
            }
        }
        let generics: string[] = [];
        let declared: ExportGeneric[] = sig.generics ?? [];
        for (let generic in declared) {
            if (generic.bounds.length() == 0) {
                generics.push(generic.name);
            } else {
                generics.push(`${generic.name}: ${generic.bounds.join(" + ")}`);
            }
        }
        let errors = normalize_ty_display(sig.throws_spec?.display ?? "never");
        let returns = normalize_ty_display(sig.returns?.display ?? "void");
        // Rebuilt from the normalized parts rather than reusing the export's
        // rendering, so the display and the structured fields cannot disagree.
        out
            = SymbolSignature {
                display: render_symbol_signature(generics, args, kwargs, returns, errors),
                generic_params: generics,
                args: args,
                kwargs: kwargs,
                returns: returns,
                errors: errors,
            };
    }
    out
}

/// Flattens a package export into its symbols: the types themselves, their
/// members, namespace-level functions, and the methods impls contribute.
/// Compiler-synthesized `$` names are dropped outright; everything else a user
/// can reach is listed, since BAML has no visibility modifiers.
function extract_symbols(doc: PackageExportDoc) -> BamlSymbol[] {
    let symbols: BamlSymbol[] = [];
    for (let item in doc.items) {
        if (item.name.includes("$")) {
            continue;
        }
        let qualified = strip_kind_prefix(item.id);
        let owner_path = path_steps(qualified);
        let owner_display = type_display_name(qualified);

        if (item.kind == "function") {
            symbols.push(
                BamlSymbol {
                    id: item.id,
                    declared_by: null,
                    symbol: owner_path,
                    display: qualified,
                    kind: "function",
                    origin: "free",
                    generics: [],
                    ty: null,
                    resolved: null,
                    default: null,
                    signature: symbol_signature(item.signature),
                    doc: item.docstring,
                    references: [],
                },
            );
            continue;
        }

        symbols.push(
            BamlSymbol {
                id: item.id,
                declared_by: null,
                symbol: owner_path,
                display: owner_display,
                // `type_alias` is spelled `type` in source.
                kind: if (item.kind == "type_alias") {
                    "type"
                } else {
                    item.kind
                },
                origin: "type",
                generics: declared_generics(item.generics),
                ty: null,
                // Only an alias names a type; the other kinds declare one.
                resolved: normalize_optional_ty(item.resolved),
                default: null,
                signature: null,
                doc: item.docstring,
                references: [],
            },
        );

        let fields: ExportField[] = item.fields ?? [];
        for (let field in fields) {
            if (field.name.includes("$")) {
                continue;
            }
            symbols.push(
                BamlSymbol {
                    id: field.id,
                    declared_by: null,
                    symbol: member_steps(qualified, field.name),
                    display: `${owner_display}.${field.name}`,
                    kind: "field",
                    origin: "field",
                    generics: [],
                    ty: normalize_optional_ty(field.ty),
                    resolved: null,
                    default: null,
                    signature: null,
                    doc: field.docstring,
                    references: [],
                },
            );
        }

        let variants: ExportVariant[] = item.variants ?? [];
        for (let variant in variants) {
            symbols.push(
                BamlSymbol {
                    id: variant.id,
                    declared_by: null,
                    symbol: member_steps(qualified, variant.name),
                    display: `${owner_display}.${variant.name}`,
                    kind: "variant",
                    origin: "variant",
                    generics: [],
                    ty: null,
                    resolved: null,
                    default: null,
                    signature: null,
                    doc: variant.docstring,
                    references: [],
                },
            );
        }

        // An interface's associated types are members of it: `Sortable` is a
        // name plus a `SortError`, and hiding the second leaves the interface
        // looking like it has only the methods.
        let assoc_types: ExportAssocType[] = item.assoc_types ?? [];
        for (let assoc in assoc_types) {
            symbols.push(
                BamlSymbol {
                    id: assoc.id,
                    declared_by: null,
                    symbol: member_steps(qualified, assoc.name),
                    display: `${owner_display}.${assoc.name}`,
                    kind: "associated type",
                    origin: "assoc_type",
                    generics: [],
                    ty: null,
                    resolved: null,
                    default: normalize_optional_ty(assoc.default),
                    signature: null,
                    doc: null,
                    references: [],
                },
            );
        }

        let methods: ExportMethod[] = item.methods ?? [];
        let required: ExportMethod[] = item.required_methods ?? [];
        let defaults: ExportMethod[] = item.default_methods ?? [];
        for (let method in methods.concat(required).concat(defaults)) {
            if (method.name.includes("$")) {
                continue;
            }
            symbols.push(
                BamlSymbol {
                    id: method.id,
                    declared_by: null,
                    symbol: member_steps(qualified, method.name),
                    display: `${owner_display}.${method.name}`,
                    kind: "function",
                    origin: origin_of_method(method.signature),
                    generics: [],
                    ty: null,
                    resolved: null,
                    default: null,
                    signature: symbol_signature(method.signature),
                    doc: method.docstring,
                    references: [],
                },
            );
        }
    }

    for (let impl_record in doc.impls) {
        let step = ImplStep { base: impl_record.for_ty.display, iface: impl_record.interface.split(".") };
        for (let method in impl_record.methods) {
            if (method.name.includes("$")) {
                continue;
            }
            let impl_path: PathStep[] = [step];
            impl_path.push(method.name);
            symbols.push(
                BamlSymbol {
                    id: method.id,
                    declared_by: method.declared_by,
                    symbol: impl_path,
                    display: `(${impl_record.for_ty.display} as ${impl_record.interface}).${method.name}`,
                    kind: "function",
                    origin: "impl_method",
                    generics: [],
                    ty: null,
                    resolved: null,
                    default: null,
                    signature: symbol_signature(method.signature),
                    doc: method.docstring,
                    references: [],
                },
            );
        }
    }

    attach_baml_references(symbols);
    symbols
}

/// Dump the symbol layer for inspection.
function dump_symbols(
    baml_surface_file: string = "tools/stdlib-matrix/data/baml_surface.json",
) -> BamlSymbol[] {
    extract_symbols(load_baml_surface(baml_surface_file))
}
