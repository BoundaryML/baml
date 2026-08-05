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
type PathStep = string | ImplStep

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
        function from_json(j: baml.json.json) -> Self throws baml.json.JsonDecodeError {
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
class BamlSymbol {
    symbol: PathStep[],
    display: string,
    origin: string,
    signature: SymbolSignature?,
    doc: string?,
    /// Implementation detail with no cross-language counterpart — the `private`
    /// BAML does not have yet. Matching skips these.
    internal: bool,
}

/// Underscore-prefixed names are the stdlib's own plumbing (`_fetch`,
/// `__await_any`, `Duration._nanoseconds`) — reachable, but not surface a
/// user is meant to call, and never something another language mirrors.
///
/// This is the mechanical half of the classification only. Whole types that
/// exist to own a primitive's methods (`baml.Null`) read as ordinary public
/// names and need judgment, which a later pass supplies.
function is_internal_name(name: string) -> bool {
    name.starts_with("_")
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
                    SignatureArg {
                        name: param.name,
                        ty: normalize_ty_display(param.ty.display),
                    },
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
        out = SymbolSignature {
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
/// Compiler-synthesized `$` names are dropped outright — they are not surface
/// in any sense, unlike internal names, which are surface a user should not
/// reach for.
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
            symbols.push(BamlSymbol {
                symbol: owner_path,
                display: qualified,
                origin: "free",
                signature: symbol_signature(item.signature),
                doc: item.docstring,
                internal: is_internal_name(item.name),
            });
            continue;
        }

        symbols.push(BamlSymbol {
            symbol: owner_path,
            display: owner_display,
            origin: "type",
            signature: null,
            doc: item.docstring,
            internal: is_internal_name(item.name),
        });

        let fields: ExportField[] = item.fields ?? [];
        for (let field in fields) {
            if (field.name.includes("$")) {
                continue;
            }
            symbols.push(BamlSymbol {
                symbol: member_steps(qualified, field.name),
                display: `${owner_display}.${field.name}`,
                origin: "field",
                signature: null,
                doc: field.docstring,
                internal: is_internal_name(item.name) || is_internal_name(field.name),
            });
        }

        let methods: ExportMethod[] = item.methods ?? [];
        let required: ExportMethod[] = item.required_methods ?? [];
        let defaults: ExportMethod[] = item.default_methods ?? [];
        for (let method in methods.concat(required).concat(defaults)) {
            if (method.name.includes("$")) {
                continue;
            }
            symbols.push(BamlSymbol {
                symbol: member_steps(qualified, method.name),
                display: `${owner_display}.${method.name}`,
                origin: origin_of_method(method.signature),
                signature: symbol_signature(method.signature),
                doc: method.docstring,
                internal: is_internal_name(item.name) || is_internal_name(method.name),
            });
        }
    }

    for (let impl_record in doc.impls) {
        let step = ImplStep {
            base: impl_record.for_ty.display,
            iface: impl_record.interface.split("."),
        };
        for (let method in impl_record.methods) {
            if (method.name.includes("$")) {
                continue;
            }
            let impl_path: PathStep[] = [step];
            impl_path.push(method.name);
            symbols.push(BamlSymbol {
                symbol: impl_path,
                display: `(${impl_record.for_ty.display} as ${impl_record.interface}).${method.name}`,
                origin: "impl_method",
                signature: symbol_signature(method.signature),
                doc: method.docstring,
                internal: is_internal_name(method.name),
            });
        }
    }

    symbols
}

/// Dump the symbol layer for inspection.
function dump_symbols(
    baml_surface_file: string = "tools/stdlib-matrix/data/baml_surface.json",
) -> BamlSymbol[] {
    extract_symbols(load_baml_surface(baml_surface_file))
}
