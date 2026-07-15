//! C++ SDK emitter. Slices 1-4 of the bridge-cpp codegen spec: the
//! single-header layout, namespace routing, free functions, classes + enums
//! with generated `Codec<T>` and `Ty<T>` specializations, static + instance
//! methods, optional arguments via per-function opts structs (spec D4),
//! recursion via `baml::Box` cycle-breaking, and generics as real templates
//! (spec D3: generic classes are class templates, generic callables are
//! function templates whose concrete bindings ride `CallFunctionArgs.type_args`).
//! Streaming, companions, media/handles, and stdlib surfaces land in later
//! slices; symbols they gate on are skipped and reported in a trailing
//! header comment (no silent caps).
//!
//! Every emitted identifier comes from the typed naming system in the
//! `naming` module: a request-collection pass walks the pool up front,
//! `naming::CppNames::allocate` resolves projections, collisions, and
//! generator-ident reservations once, and the emit passes carry
//! `naming::CppName` values whose canonical C++ spelling and BAML wire
//! identity stay paired.
//!
//! Output layout (spec D1):
//!   `include/baml_sdk.h`   - the typed surface
//!   `src/bindings.cc`      - non-template definitions over `::baml::detail`
//!   `src/_inlinedbaml.cc`  - embedded BAML sources + lazy runtime init
//!
//! Template callables (generic functions, and every method of a generic
//! class) define inline in the header; non-template callables keep the
//! declaration/definition split.
//!
//! Runtime init embeds the user's `.baml` sources and initializes through
//! `create_baml_runtime`; it switches to embedded bytecode once
//! `InitializeRuntimeFromBytecode` is exported over the C ABI.

mod naming;

use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    fmt::Write as _,
    path::PathBuf,
};

use baml_codegen_types::{
    CallableParam, Class, CodegenFunctionParamMode, Enum, Function, Name, Symbol, SymbolPool, Ty,
};
pub use baml_codegen_types::{NamingConvention, OutputType};

use crate::naming::{BamlFqn, CppName, CppNameKind, CppNames, GeneratorIdent, NameRequest};

/// A user BAML source file as it should appear in the emitter's
/// inlined-baml output. `rel_path` is relative to the `baml_src/` root.
pub type UserBamlFile = (PathBuf, String);

/// Build the C++ SDK output tree for `pool`. Returned paths are relative to
/// the `baml_sdk/` output root.
pub fn to_source_code_with_bytecode(
    pool: &SymbolPool,
    user_baml_files: &[UserBamlFile],
    baml_bytecode: &[u8],
    _naming_convention: NamingConvention,
) -> HashMap<PathBuf, String> {
    let mut skipped: Vec<String> = Vec::new();

    // Every identifier the output can contain is requested and allocated
    // before any emission: collisions and reservations resolve over the
    // full request set, so no pass can destabilize another pass's names.
    let names = CppNames::allocate(&collect_requests(pool));

    let mut pool_names: Vec<_> = pool.keys().collect();
    pool_names.sort();

    // Pass 1: enums (no dependencies).
    let mut enums: Vec<EmittedEnum> = Vec::new();
    let mut emitted_types: BTreeSet<Name> = BTreeSet::new();
    for name in &pool_names {
        if name.is_stream() {
            continue;
        }
        if let Symbol::Enum(enum_def) = &pool[*name] {
            enums.push(emit_enum(&names, name, enum_def));
            emitted_types.insert((*name).clone());
        }
    }

    // Pass 2: class fields (and recursive-alias wrapper structs), to a
    // fixed point so field dependencies resolve in emission (= declaration)
    // order. `$stream` companions emit like any class; the naming layer
    // routes them under stream_types::.
    let emit_type = |name: &Name,
                     emitted_types: &BTreeSet<Name>,
                     boxed: &BTreeSet<Name>|
     -> Result<Option<EmittedClass>, String> {
        match &pool[name] {
            Symbol::Class(class_def) => {
                emit_class(pool, &names, name, class_def, emitted_types, boxed)
            }
            Symbol::TypeAlias(alias) => {
                emit_alias_wrapper(pool, &names, name, alias, emitted_types, boxed)
            }
            Symbol::Enum(_) | Symbol::Function(_) => unreachable!(),
        }
    };
    let mut classes: Vec<EmittedClass> = Vec::new();
    let mut pending: Vec<&Name> = pool_names
        .iter()
        .copied()
        .filter(|name| match &pool[*name] {
            Symbol::Class(_) => true,
            Symbol::TypeAlias(alias) => alias.recursive,
            Symbol::Enum(_) | Symbol::Function(_) => false,
        })
        .collect();
    loop {
        let mut progressed = false;
        let mut still_pending = Vec::new();
        for name in pending {
            match emit_type(name, &emitted_types, &BTreeSet::new()) {
                Ok(Some(emitted)) => {
                    classes.push(emitted);
                    emitted_types.insert(name.clone());
                    progressed = true;
                }
                Ok(None) => still_pending.push(name),
                Err(reason) => {
                    skipped.push(format!("{name}: {reason}"));
                    progressed = true;
                }
            }
        }
        pending = still_pending;
        if pending.is_empty() || !progressed {
            break;
        }
    }
    // Leftovers are cycles (or depend on one): every class in the set counts
    // as available, and in-cycle class references are boxed (baml::Box needs
    // only a forward declaration). Classes that still fail depend on a
    // genuinely skipped type and are reported.
    if !pending.is_empty() {
        // A member that fails to emit invalidates the Box references other
        // cycle members already resolved against it, so survivors re-emit
        // from scratch until a round completes with no failures.
        let mut cycle_set: BTreeSet<Name> = pending.iter().map(|n| (*n).clone()).collect();
        loop {
            for name in &cycle_set {
                emitted_types.insert(name.clone());
            }
            let mut round = Vec::new();
            let mut failed = Vec::new();
            for name in &cycle_set {
                match emit_type(name, &emitted_types, &cycle_set) {
                    Ok(Some(emitted)) => round.push(emitted),
                    Ok(None) | Err(_) => failed.push(name.clone()),
                }
            }
            if failed.is_empty() {
                classes.extend(round);
                break;
            }
            for name in failed {
                emitted_types.remove(&name);
                cycle_set.remove(&name);
                skipped.push(format!("{name}: depends on a type this slice cannot emit"));
            }
            if cycle_set.is_empty() {
                break;
            }
        }
    }

    // Pass 3: methods, against the final emitted type set (declarations may
    // reference any emitted class thanks to the forward-declaration block).
    for class in &mut classes {
        if class.alias_wrapper {
            continue;
        }
        let Symbol::Class(class_def) = &pool[&class.pool_name] else {
            unreachable!()
        };
        for (methods, is_static) in [
            (&class_def.static_methods, true),
            (&class_def.instance_methods, false),
        ] {
            for method in methods {
                let fqn = BamlFqn::member(&class.pool_name, method.name.as_str());
                match emit_callable(
                    pool,
                    &names,
                    &fqn,
                    CppNameKind::Method,
                    method,
                    &emitted_types,
                    &class.generic_params,
                ) {
                    Ok(emitted) => {
                        if is_static {
                            class.static_methods.push(emitted);
                        } else {
                            class.instance_methods.push(emitted);
                        }
                    }
                    Err(reason) => {
                        skipped.push(format!("{}.{}: {reason}", class.pool_name, method.name));
                    }
                }
            }
        }
    }

    // Pass 4: free functions over the emitted type set.
    let mut fns_by_namespace: BTreeMap<Vec<String>, Vec<EmittedFn>> = BTreeMap::new();
    for name in &pool_names {
        let Symbol::Function(function) = &pool[*name] else {
            continue;
        };
        match emit_callable(
            pool,
            &names,
            &BamlFqn::symbol(name),
            CppNameKind::Function,
            function,
            &emitted_types,
            &[],
        ) {
            Ok(emitted) => {
                let ns = allocated_namespace(&names, name, false);
                fns_by_namespace.entry(ns).or_default().push(emitted);
            }
            Err(reason) => skipped.push(format!("{name}: {reason}")),
        }
    }

    let mut out = HashMap::new();
    out.insert(
        PathBuf::from("include/baml_sdk.h"),
        render_header(&enums, &classes, &fns_by_namespace, &skipped),
    );
    out.insert(
        PathBuf::from("src/bindings.cc"),
        render_bindings(&classes, &fns_by_namespace),
    );
    out.insert(
        PathBuf::from("src/_inlinedbaml.cc"),
        render_inlinedbaml(user_baml_files, baml_bytecode),
    );
    out
}

// ---------------------------------------------------------------------------
// Name requests
// ---------------------------------------------------------------------------

/// Member name of the synthesized per-callable opts struct within its
/// callable's identity.
const OPTS_MEMBER: &str = "opts";

/// One typed request per identifier any emit pass may need. Mirrors the
/// pool-level skip filters (`pkg`, `$stream`, `$` companions); symbols that
/// only emission can rule out (unsupported field types, broken cycles) still
/// get allocations, which are simply never rendered.
fn collect_requests(pool: &SymbolPool) -> BTreeSet<NameRequest> {
    let mut requests = BTreeSet::new();
    for (name, symbol) in pool {
        match symbol {
            Symbol::Enum(enum_def) => {
                request_namespace_segments(&mut requests, name, true);
                requests.insert(NameRequest::new(BamlFqn::symbol(name), CppNameKind::Enum));
                for variant in &enum_def.variants {
                    requests.insert(NameRequest::new(
                        BamlFqn::member(name, variant.name.as_str()),
                        CppNameKind::EnumVariant,
                    ));
                }
            }
            Symbol::Class(class_def) => {
                request_namespace_segments(&mut requests, name, true);
                requests.insert(NameRequest::new(BamlFqn::symbol(name), CppNameKind::Class));
                if is_tagged_heap_handle_class(name) {
                    requests.insert(tagged_handle_field_request(name));
                }
                for param in &class_def.generic_params {
                    requests.insert(NameRequest::new(
                        BamlFqn::member(name, param.as_str()),
                        CppNameKind::TypeVar,
                    ));
                }
                for prop in &class_def.properties {
                    requests.insert(NameRequest::new(
                        BamlFqn::member(name, prop.name.as_str()),
                        CppNameKind::Field,
                    ));
                }
                for method in class_def
                    .static_methods
                    .iter()
                    .chain(&class_def.instance_methods)
                {
                    let method_fqn = BamlFqn::member(name, method.name.as_str());
                    requests.insert(NameRequest::new(method_fqn.clone(), CppNameKind::Method));
                    request_callable_members(&mut requests, &method_fqn, method);
                }
            }
            Symbol::Function(function) => {
                request_namespace_segments(&mut requests, name, false);
                let fqn = BamlFqn::symbol(name);
                requests.insert(function_request(name));
                request_callable_members(&mut requests, &fqn, function);
            }
            // Non-recursive aliases resolve transparently (no declaration,
            // so no name); recursive aliases emit a named wrapper struct
            // that breaks the type recursion.
            Symbol::TypeAlias(alias) => {
                if alias.recursive {
                    request_namespace_segments(&mut requests, name, true);
                    requests.insert(NameRequest::new(BamlFqn::symbol(name), CppNameKind::Class));
                    requests.insert(alias_value_field_request(name));
                }
            }
        }
    }
    requests
}

/// One request per namespace segment, each scoped by its parent path, so
/// segment names allocate top-down. Segments come from the pkg-aware source
/// path (`baml`/`vendor/<pkg>`/`stream_types` prefixes included) and are
/// anchored in the `user` package so identical C++ scopes dedupe across
/// packages.
fn request_namespace_segments(
    requests: &mut BTreeSet<NameRequest>,
    name: &Name,
    honor_stream_suffix: bool,
) {
    let segments = naming::source_ns(name, honor_stream_suffix);
    for depth in 0..segments.len() {
        let segment = Name::new(
            baml_base::Name::from("user"),
            segments[..depth]
                .iter()
                .map(|seg| baml_base::Name::from(&**seg))
                .collect(),
            baml_base::Name::from(&*segments[depth]),
        );
        requests.insert(NameRequest::new(
            BamlFqn::symbol(&segment),
            CppNameKind::Namespace,
        ));
    }
}

/// `TypeVar`s, parameters, and (when optional parameters exist) the
/// synthesized opts struct + setters of one callable.
fn request_callable_members(
    requests: &mut BTreeSet<NameRequest>,
    fqn: &BamlFqn,
    function: &Function,
) {
    for param in &function.generic_params {
        requests.insert(NameRequest::new(
            fqn.child(param.as_str()),
            CppNameKind::TypeVar,
        ));
    }
    for arg in &function.arguments {
        requests.insert(NameRequest::new(
            fqn.child(arg.name.as_str()),
            CppNameKind::Parameter,
        ));
    }
    if function.arguments.iter().any(|arg| arg.default.is_some()) {
        requests.insert(opts_request(fqn, function));
        for arg in function.arguments.iter().filter(|a| a.default.is_some()) {
            requests.insert(setter_request(fqn, arg.name.as_str()));
        }
    }
}

/// Python-parity companion spelling for `$`-suffixed compiler-synthesized
/// functions: `foo$stream` -> `foo_stream`, `Foo$parse` -> `Foo__parse`,
/// `Foo$build_request` -> `Foo__build_request`. `None` for ordinary
/// functions.
fn companion_preferred(name: &Name) -> Option<String> {
    let raw = name.name.as_str();
    if raw.contains('$') {
        Some(function_spelling(raw))
    } else {
        None
    }
}

/// The C++ spelling of a function's source name: companions get their
/// Python-parity form, everything else is verbatim (bridges never re-case
/// user spellings).
fn function_spelling(raw: &str) -> String {
    match raw.split_once('$') {
        Some((base, "stream")) => format!("{base}_stream"),
        Some((base, companion)) => format!("{base}__{companion}"),
        None => raw.to_string(),
    }
}

/// The name request for a free function, companion-aware. Shared between
/// collection and emission so the lookup key cannot drift.
fn function_request(name: &Name) -> NameRequest {
    let fqn = BamlFqn::symbol(name);
    match companion_preferred(name) {
        Some(preferred) => NameRequest::synthesized(fqn, CppNameKind::Function, &preferred),
        None => NameRequest::new(fqn, CppNameKind::Function),
    }
}

/// `baml.llm.Stream` crosses the wire as a bare tagged-heap-handle value
/// (never as a `class_value`), so it emits with a single synthesized handle
/// field and a bare-handle codec.
fn is_tagged_heap_handle_class(name: &Name) -> bool {
    name.to_string() == "baml.llm.Stream"
}

/// `BamlHandleType.ADT_TAGGED_HEAP_HANDLE` (`baml_handle.proto`).
const ADT_TAGGED_HEAP_HANDLE: i32 = 14;

/// The synthesized `_handle` field request of a tagged-heap-handle class.
/// Shared between collection and emission so the lookup key cannot drift.
fn tagged_handle_field_request(class: &Name) -> NameRequest {
    NameRequest::synthesized(
        BamlFqn::member(class, "_handle"),
        CppNameKind::Field,
        "_handle",
    )
}

/// The synthesized `value` field request of a recursive-alias wrapper
/// struct. Shared between collection and emission so the lookup key cannot
/// drift.
fn alias_value_field_request(alias: &Name) -> NameRequest {
    NameRequest::synthesized(BamlFqn::member(alias, "value"), CppNameKind::Field, "value")
}

/// The request for a callable's synthesized opts struct. Shared between
/// collection and emission so the lookup key cannot drift.
fn opts_request(callable: &BamlFqn, function: &Function) -> NameRequest {
    NameRequest::synthesized(
        callable.child(OPTS_MEMBER),
        CppNameKind::OptsStruct,
        // Verbatim source spelling + "Opts" (probe -> probeOpts): the other
        // bridges never re-case user names (Python kwargs and TS's inline
        // $opts never even mint a type); C++ needs a name only because the
        // struct must be constructible.
        &format!("{}Opts", function_spelling(function.name.as_str())),
    )
}

/// The request for one opts-struct setter. Shared between collection and
/// emission so the lookup key cannot drift.
fn setter_request(callable: &BamlFqn, param: &str) -> NameRequest {
    NameRequest::synthesized(
        callable.child(OPTS_MEMBER).child(param),
        CppNameKind::Setter,
        &format!("set_{param}"),
    )
}

/// The allocated C++ namespace path for a symbol, as owned segments for the
/// namespace open/close renderers and the free-function grouping key.
fn allocated_namespace(names: &CppNames, name: &Name, honor_stream_suffix: bool) -> Vec<String> {
    let source: Vec<Box<str>> = naming::source_ns(name, honor_stream_suffix);
    names
        .ns_path(&source)
        .iter()
        .map(std::string::ToString::to_string)
        .collect()
}

/// Doc rollup shared by classes and enums: summary, then an
/// `Attributes:`/`Members:` section listing every member when at least one
/// member carries a doc (documented as `name: doc`, undocumented bare).
/// Member names are BAML source names — docs describe the BAML surface.
fn compose_doc(
    summary: Option<&String>,
    section: &str,
    members: &[(String, Option<String>)],
) -> Option<String> {
    let mut doc = summary.cloned().unwrap_or_default();
    if members.iter().any(|(_, d)| d.is_some()) {
        if !doc.is_empty() {
            doc.push_str("\n\n");
        }
        doc.push_str(section);
        for (name, member_doc) in members {
            match member_doc {
                Some(text) => {
                    let text = text.replace('\n', " ");
                    let _ = write!(doc, "\n    {name}: {text}");
                }
                None => {
                    let _ = write!(doc, "\n    {name}");
                }
            }
        }
    }
    if doc.is_empty() { None } else { Some(doc) }
}

// ---------------------------------------------------------------------------
// Enums
// ---------------------------------------------------------------------------

struct EmittedEnum {
    ns: Vec<String>,
    name: CppName,
    doc: Option<String>,
    /// (allocated enumerator, BAML variant *value* on the wire). The wire
    /// value comes from the pool's `EnumVariant.value`, not from the variant
    /// name the enumerator was allocated from.
    variants: Vec<(CppName, String)>,
}

fn emit_enum(names: &CppNames, name: &Name, enum_def: &Enum) -> EmittedEnum {
    EmittedEnum {
        ns: allocated_namespace(names, name, true),
        name: names
            .get(&NameRequest::new(BamlFqn::symbol(name), CppNameKind::Enum))
            .clone(),
        doc: compose_doc(
            enum_def.docstring.as_ref(),
            "Members:",
            &enum_def
                .variants
                .iter()
                .map(|v| (v.name.to_string(), v.docstring.clone()))
                .collect::<Vec<_>>(),
        ),
        variants: enum_def
            .variants
            .iter()
            .map(|v| {
                (
                    names
                        .get(&NameRequest::new(
                            BamlFqn::member(name, v.name.as_str()),
                            CppNameKind::EnumVariant,
                        ))
                        .clone(),
                    v.value.clone(),
                )
            })
            .collect(),
    }
}

// ---------------------------------------------------------------------------
// Classes
// ---------------------------------------------------------------------------

struct EmittedField {
    name: CppName,
    ty: String,
}

struct EmittedClass {
    pool_name: Name,
    ns: Vec<String>,
    name: CppName,
    doc: Option<String>,
    /// Template parameters; empty for non-generic classes.
    generic_params: Vec<CppName>,
    fields: Vec<EmittedField>,
    static_methods: Vec<EmittedFn>,
    instance_methods: Vec<EmittedFn>,
    /// A recursive-alias wrapper struct: one `value` field holding the
    /// alias's resolved type, structural codec (aliases have no wire
    /// identity), no methods.
    alias_wrapper: bool,
}

impl EmittedClass {
    fn is_template(&self) -> bool {
        !self.generic_params.is_empty()
    }

    /// `X` or `X<T, U>` — the class's own name as spelled inside its scope.
    fn self_type(&self) -> String {
        if self.is_template() {
            format!(
                "{}<{}>",
                self.name.declared(),
                type_param_list(&self.generic_params)
            )
        } else {
            self.name.declared().to_string()
        }
    }

    /// `::baml_sdk::ns::X<T, U>` — fully qualified parameterized spelling.
    fn qualified_self_type(&self) -> String {
        if self.is_template() {
            format!(
                "{}<{}>",
                self.name.identifier(),
                type_param_list(&self.generic_params)
            )
        } else {
            self.name.identifier().to_string()
        }
    }

    fn template_prefix(&self) -> String {
        template_prefix(&self.generic_params)
    }
}

fn template_prefix(params: &[CppName]) -> String {
    if params.is_empty() {
        String::new()
    } else {
        let typenames: Vec<String> = params
            .iter()
            .map(|p| format!("typename {}", p.declared()))
            .collect();
        format!("template <{}>\n", typenames.join(", "))
    }
}

/// `T, U` — template arguments at a use site.
fn type_param_list(params: &[CppName]) -> String {
    let spelled: Vec<String> = params.iter().map(|p| p.identifier().to_string()).collect();
    spelled.join(", ")
}

/// Ok(None) = not emittable *yet* (a field references a class not emitted so
/// far); the fixed-point loop retries. Err = never emittable in this slice.
fn emit_class(
    pool: &SymbolPool,
    names: &CppNames,
    name: &Name,
    class_def: &Class,
    emitted_types: &BTreeSet<Name>,
    boxed: &BTreeSet<Name>,
) -> Result<Option<EmittedClass>, String> {
    let generic_params: Vec<CppName> = class_def
        .generic_params
        .iter()
        .map(|p| {
            names
                .get(&NameRequest::new(
                    BamlFqn::member(name, p.as_str()),
                    CppNameKind::TypeVar,
                ))
                .clone()
        })
        .collect();
    let mut fields = Vec::new();
    if is_tagged_heap_handle_class(name) {
        // The wire form is a bare tagged heap handle; the class's declared
        // fields are engine-internal and never cross the boundary.
        fields.push(EmittedField {
            name: names.get(&tagged_handle_field_request(name)).clone(),
            ty: "::baml::Handle".to_string(),
        });
        return Ok(Some(EmittedClass {
            pool_name: name.clone(),
            ns: allocated_namespace(names, name, true),
            name: names
                .get(&NameRequest::new(BamlFqn::symbol(name), CppNameKind::Class))
                .clone(),
            doc: class_def.docstring.clone(),
            generic_params,
            fields,
            static_methods: Vec::new(),
            instance_methods: Vec::new(),
            alias_wrapper: false,
        }));
    }
    for prop in &class_def.properties {
        match translate_ty(pool, names, &prop.ty, emitted_types, boxed, &generic_params) {
            Translated::Cpp(ty) => {
                fields.push(EmittedField {
                    name: names
                        .get(&NameRequest::new(
                            BamlFqn::member(name, prop.name.as_str()),
                            CppNameKind::Field,
                        ))
                        .clone(),
                    ty,
                });
            }
            Translated::NotYet => return Ok(None),
            Translated::Unsupported(reason) => {
                return Err(format!("field `{}`: {reason}", prop.name));
            }
        }
    }
    // Method optional params become Arg<T> fields on opts structs nested in
    // this class's body, so their types must be complete (= defined earlier)
    // too. Delay the class while such a dep is merely not-yet-emitted; once
    // the cycle pass runs (boxed non-empty), stop blocking -- a dep that
    // still cannot resolve there means pass 3 skips that method, so no opts
    // struct references it.
    if boxed.is_empty() {
        for method in class_def
            .static_methods
            .iter()
            .chain(&class_def.instance_methods)
        {
            for arg in method.arguments.iter().filter(|a| a.default.is_some()) {
                if let Translated::NotYet =
                    translate_ty(pool, names, &arg.ty, emitted_types, boxed, &generic_params)
                {
                    return Ok(None);
                }
            }
        }
    }
    Ok(Some(EmittedClass {
        pool_name: name.clone(),
        ns: allocated_namespace(names, name, true),
        name: names
            .get(&NameRequest::new(BamlFqn::symbol(name), CppNameKind::Class))
            .clone(),
        doc: compose_doc(
            class_def.docstring.as_ref(),
            "Attributes:",
            &class_def
                .properties
                .iter()
                .map(|p| (p.name.to_string(), p.docstring.clone()))
                .collect::<Vec<_>>(),
        ),
        generic_params,
        fields,
        static_methods: Vec::new(),
        instance_methods: Vec::new(),
        alias_wrapper: false,
    }))
}

/// A recursive type alias as a named wrapper struct: `type RecList = int |
/// RecList[]` becomes `struct RecList { variant<int64_t,
/// vector<Box<RecList>>> value; }`. Self-references (and any in-cycle
/// references) are boxed by the same cycle machinery classes use; the
/// codec is structural (encode/decode the resolved type, wrapping and
/// unwrapping `value`) because aliases carry no wire identity.
fn emit_alias_wrapper(
    pool: &SymbolPool,
    names: &CppNames,
    name: &Name,
    alias: &baml_codegen_types::TypeAlias,
    emitted_types: &BTreeSet<Name>,
    boxed: &BTreeSet<Name>,
) -> Result<Option<EmittedClass>, String> {
    let inner = match translate_ty(pool, names, &alias.resolves_to, emitted_types, boxed, &[]) {
        Translated::Cpp(ty) => ty,
        Translated::NotYet => return Ok(None),
        Translated::Unsupported(reason) => {
            return Err(format!("aliased type: {reason}"));
        }
    };
    Ok(Some(EmittedClass {
        pool_name: name.clone(),
        ns: allocated_namespace(names, name, true),
        name: names
            .get(&NameRequest::new(BamlFqn::symbol(name), CppNameKind::Class))
            .clone(),
        doc: None,
        generic_params: Vec::new(),
        fields: vec![EmittedField {
            name: names.get(&alias_value_field_request(name)).clone(),
            ty: inner,
        }],
        static_methods: Vec::new(),
        instance_methods: Vec::new(),
        alias_wrapper: true,
    }))
}

// ---------------------------------------------------------------------------
// Callables (free functions and methods share this shape)
// ---------------------------------------------------------------------------

struct EmittedParam {
    name: CppName,
    ty: String,
    /// For callable-typed parameters: the callable's declared BAML param
    /// names in declared order ("" for unnamed required params). Switches
    /// the binding to `EncodeCallable` (host-callable registration) and
    /// keys supplied optional args on dispatch.
    callable_names: Option<Vec<String>>,
}

/// An optional parameter, rendered as an `Arg<type>` field on the opts
/// struct together with its synthesized setter.
struct EmittedOptParam {
    name: CppName,
    /// Normalized C++ type (the `Arg<...>` wrapper is added at render time).
    ty: String,
    setter: CppName,
}

struct EmittedFn {
    name: CppName,
    /// The BAML FQN the runtime call dispatches on: the wire symbol, plus
    /// `.member` (the method's source name) for methods. Never derived from
    /// C++ spellings.
    call_fqn: String,
    ret: String,
    params: Vec<EmittedParam>,
    opt_params: Vec<EmittedOptParam>,
    /// Opts struct name, when `opt_params` is non-empty.
    opts_name: Option<CppName>,
    /// The callable's own template parameters (function generics).
    type_params: Vec<CppName>,
    /// The enclosing class's template parameters (empty for free functions
    /// and methods of non-generic classes). Bound before `type_params` in
    /// the call's `type_args` (De Bruijn order).
    class_type_params: Vec<CppName>,
    doc: Option<String>,
    raises: Vec<String>,
}

impl EmittedFn {
    /// Template callables (a generic function, or any method that must see
    /// its class's template params) define inline in the header.
    fn is_template(&self) -> bool {
        !self.type_params.is_empty() || !self.class_type_params.is_empty()
    }
}

fn emit_callable(
    pool: &SymbolPool,
    names: &CppNames,
    fqn: &BamlFqn,
    kind: CppNameKind,
    function: &Function,
    emitted_types: &BTreeSet<Name>,
    class_type_params: &[CppName],
) -> Result<EmittedFn, String> {
    let request = if kind == CppNameKind::Function {
        function_request(&fqn.symbol)
    } else {
        NameRequest::new(fqn.clone(), kind)
    };
    let name = names.get(&request).clone();
    let type_params: Vec<CppName> = function
        .generic_params
        .iter()
        .map(|p| {
            names
                .get(&NameRequest::new(
                    fqn.child(p.as_str()),
                    CppNameKind::TypeVar,
                ))
                .clone()
        })
        .collect();
    let mut in_scope: Vec<CppName> = class_type_params.to_vec();
    in_scope.extend(type_params.iter().cloned());

    let mut params = Vec::new();
    let mut opt_params = Vec::new();
    for arg in &function.arguments {
        // Top-level callable parameters cross as host callables
        // (std::function); callables nested in other types stay
        // unsupported (translate_ty rejects them).
        let mut callable_names = None;
        let ty = if let Ty::Callable {
            params: callable_params,
            ret,
        } = &arg.ty
        {
            if arg.default.is_some() {
                return Err(format!(
                    "optional argument `{}` has a callable type (unsupported)",
                    arg.name
                ));
            }
            match translate_callable_ty(pool, names, callable_params, ret, emitted_types, &in_scope)
            {
                Ok((ty, wire_names)) => {
                    callable_names = Some(wire_names);
                    ty
                }
                Err(reason) => {
                    return Err(format!(
                        "argument `{}` has unsupported type {} ({reason})",
                        arg.name, arg.ty
                    ));
                }
            }
        } else {
            match translate_ty(
                pool,
                names,
                &arg.ty,
                emitted_types,
                &BTreeSet::new(),
                &in_scope,
            ) {
                Translated::Cpp(ty) => ty,
                Translated::NotYet | Translated::Unsupported(_) => {
                    return Err(format!(
                        "argument `{}` has unsupported type {}",
                        arg.name, arg.ty
                    ));
                }
            }
        };
        let param_name = names
            .get(&NameRequest::new(
                fqn.child(arg.name.as_str()),
                CppNameKind::Parameter,
            ))
            .clone();
        if arg.default.is_some() {
            opt_params.push(EmittedOptParam {
                name: param_name,
                ty,
                setter: names.get(&setter_request(fqn, arg.name.as_str())).clone(),
            });
        } else {
            params.push(EmittedParam {
                name: param_name,
                ty,
                callable_names,
            });
        }
    }
    let ret = match translate_return_ty(
        pool,
        names,
        &function.return_type,
        emitted_types,
        &BTreeSet::new(),
        &in_scope,
    ) {
        Translated::Cpp(ty) => ty,
        Translated::NotYet | Translated::Unsupported(_) => {
            return Err(format!("unsupported return type {}", function.return_type));
        }
    };

    let raises = match &function.throws {
        None => Vec::new(),
        Some(Ty::Union(items)) => items.iter().map(unqualified_leaf_name).collect(),
        Some(ty) => vec![unqualified_leaf_name(ty)],
    };

    let opts_name = if opt_params.is_empty() {
        None
    } else {
        Some(names.get(&opts_request(fqn, function)).clone())
    };

    // The runtime dispatches on the BAML FQN: for methods that is the class's
    // wire symbol plus the method's source member token, never a C++ name.
    let call_fqn = if name.kind() == CppNameKind::Method {
        let member = fqn.members.last().expect("method identity has a member");
        format!("{}.{member}", name.wire())
    } else {
        name.wire().to_string()
    };

    Ok(EmittedFn {
        name,
        call_fqn,
        ret,
        params,
        opt_params,
        opts_name,
        type_params,
        class_type_params: class_type_params.to_vec(),
        doc: function.docstring.clone(),
        raises,
    })
}

fn unqualified_leaf_name(ty: &Ty) -> String {
    match ty {
        Ty::Class(name, _) | Ty::Enum(name) | Ty::TypeAlias(name) => name.bare_name().to_string(),
        other => other.to_string(),
    }
}

// ---------------------------------------------------------------------------
// Type translation
// ---------------------------------------------------------------------------

enum Translated {
    Cpp(String),
    /// References a class that has not been emitted (yet) — retry later.
    NotYet,
    Unsupported(String),
}

/// Slice-1..4 type table: primitives, containers, null-normalized optionals,
/// variants, emitted classes/enums (with generic instantiations), transparent
/// aliases, boxed cycle references, and in-scope `TypeVars`. Everything else is
/// unsupported here and the surrounding symbol is skipped (reported, not
/// silently dropped).
fn translate_ty(
    pool: &SymbolPool,
    names: &CppNames,
    ty: &Ty,
    emitted_types: &BTreeSet<Name>,
    boxed: &BTreeSet<Name>,
    type_vars: &[CppName],
) -> Translated {
    let translated = match ty {
        Ty::Int => "int64_t".to_string(),
        Ty::Float => "double".to_string(),
        Ty::String => "std::string".to_string(),
        Ty::Bool => "bool".to_string(),
        Ty::Null => "std::monostate".to_string(),
        Ty::Uint8Array => "std::vector<uint8_t>".to_string(),
        // Opaque engine-managed state (`$rust_type` fields of handle-backed
        // stdlib classes): an owned engine handle.
        Ty::RustType => "::baml::Handle".to_string(),
        Ty::Bigint => "::baml::BigInt".to_string(),
        // The primitive media types are the stdlib media classes on the
        // wire (class_value with a `_data` handle), so they translate to
        // the generated baml.media.* class references.
        Ty::Media(kind) => {
            let class_name = match kind {
                baml_base::MediaKind::Image => "Image",
                baml_base::MediaKind::Audio => "Audio",
                baml_base::MediaKind::Video => "Video",
                baml_base::MediaKind::Pdf => "Pdf",
                baml_base::MediaKind::Generic => {
                    return Translated::Unsupported("generic media type".to_string());
                }
            };
            let media_class = Name::new(
                baml_base::Name::from("baml"),
                vec![baml_base::Name::from("media")],
                baml_base::Name::from(class_name),
            );
            return translate_ty(
                pool,
                names,
                &Ty::Class(media_class, Vec::new()),
                emitted_types,
                boxed,
                type_vars,
            );
        }
        Ty::Literal(lit) => {
            // Literal types widen to their base type (Python parity).
            match lit {
                baml_base::Literal::Int(_) => "int64_t".to_string(),
                baml_base::Literal::Bigint(_) => "::baml::BigInt".to_string(),
                baml_base::Literal::Float(_) => "double".to_string(),
                baml_base::Literal::String(_) => "std::string".to_string(),
                baml_base::Literal::Bool(_) => "bool".to_string(),
            }
        }
        Ty::TypeVar(name) => {
            // In-scope `TypeVar`s are matched by their BAML wire name; the
            // C++ template parameter may have been renamed.
            match type_vars.iter().find(|tv| tv.wire().is_key(name.as_str())) {
                Some(tv) => tv.identifier().to_string(),
                None => {
                    return Translated::Unsupported(format!("out-of-scope TypeVar {name}"));
                }
            }
        }
        Ty::TypeAlias(name) => {
            // Non-recursive aliases resolve transparently to their target
            // type; recursive aliases reference their wrapper struct like a
            // class (boxed inside their own cycle, since the box needs only
            // the forward declaration).
            let Some(Symbol::TypeAlias(alias)) = pool.get(name) else {
                return Translated::Unsupported(format!("unresolved alias {name}"));
            };
            if alias.recursive {
                let base = if boxed.contains(name) || emitted_types.contains(name) {
                    names
                        .get(&NameRequest::new(BamlFqn::symbol(name), CppNameKind::Class))
                        .identifier()
                        .to_string()
                } else {
                    return Translated::NotYet;
                };
                if boxed.contains(name) {
                    return Translated::Cpp(format!("::baml::Box<{base}>"));
                }
                return Translated::Cpp(base);
            }
            return translate_ty(
                pool,
                names,
                &alias.resolves_to,
                emitted_types,
                boxed,
                type_vars,
            );
        }
        Ty::Enum(name) => {
            return if emitted_types.contains(name) {
                Translated::Cpp(
                    names
                        .get(&NameRequest::new(BamlFqn::symbol(name), CppNameKind::Enum))
                        .identifier()
                        .to_string(),
                )
            } else {
                Translated::NotYet
            };
        }
        Ty::Class(name, args) => {
            let mut translated_args = Vec::new();
            for arg in args {
                match translate_ty(pool, names, arg, emitted_types, boxed, type_vars) {
                    Translated::Cpp(t) => translated_args.push(t),
                    other => return other,
                }
            }
            // Cycle members box their in-cycle class references: a Box only
            // needs the forward declaration, so no ordering constraint.
            let base = if boxed.contains(name) || emitted_types.contains(name) {
                names
                    .get(&NameRequest::new(BamlFqn::symbol(name), CppNameKind::Class))
                    .identifier()
                    .to_string()
            } else {
                return Translated::NotYet;
            };
            let spelled = if translated_args.is_empty() {
                base
            } else {
                format!("{base}<{}>", translated_args.join(", "))
            };
            if boxed.contains(name) {
                return Translated::Cpp(format!("::baml::Box<{spelled}>"));
            }
            return Translated::Cpp(spelled);
        }
        Ty::List(inner) => {
            match translate_ty(pool, names, inner, emitted_types, boxed, type_vars) {
                Translated::Cpp(inner) => format!("std::vector<{inner}>"),
                other => return other,
            }
        }
        Ty::Map { key, value } => {
            if !matches!(key.as_ref(), Ty::String) {
                return Translated::Unsupported("non-string map key".to_string());
            }
            match translate_ty(pool, names, value, emitted_types, boxed, type_vars) {
                Translated::Cpp(value) => format!("std::map<std::string, {value}>"),
                other => return other,
            }
        }
        Ty::Union(items) => {
            // Null-normalization (spec D-unions v2): strip the null member,
            // dedup alternatives that map to the same C++ type, emit a
            // variant (or the bare type when one alternative remains), and
            // wrap in optional when null was a member.
            let non_null: Vec<&Ty> = items.iter().filter(|t| !matches!(t, Ty::Null)).collect();
            let had_null = non_null.len() != items.len();
            let mut alternatives: Vec<String> = Vec::new();
            for item in non_null {
                match translate_ty(pool, names, item, emitted_types, boxed, type_vars) {
                    Translated::Cpp(alt) => {
                        if !alternatives.contains(&alt) {
                            alternatives.push(alt);
                        }
                    }
                    other => return other,
                }
            }
            // BAML unions are sets (string | int == int | string), so the
            // C++ spelling must be a function of the member set, not the
            // declaration sequence: canonical order = sorted rendered types.
            // Decode order independence is the codec's job (exact-kind pass
            // before widening pass in Codec<std::variant>).
            alternatives.sort();
            let inner = match alternatives.as_slice() {
                [] => return Translated::Unsupported("empty union".to_string()),
                [single] => single.clone(),
                _ => format!("std::variant<{}>", alternatives.join(", ")),
            };
            if had_null {
                // A nullable boxed recursive edge cannot be optional<Box<T>>
                // (std::optional needs a complete T at instantiation);
                // OptionalBox folds the null into the box itself.
                if let Some(boxed_inner) = inner
                    .strip_prefix("::baml::Box<")
                    .and_then(|rest| rest.strip_suffix('>'))
                {
                    format!("::baml::OptionalBox<{boxed_inner}>")
                } else {
                    format!("std::optional<{inner}>")
                }
            } else {
                inner
            }
        }
        other => return Translated::Unsupported(format!("type {other}")),
    };
    Translated::Cpp(translated)
}

fn translate_return_ty(
    pool: &SymbolPool,
    names: &CppNames,
    ty: &Ty,
    emitted_types: &BTreeSet<Name>,
    boxed: &BTreeSet<Name>,
    type_vars: &[CppName],
) -> Translated {
    if matches!(ty, Ty::Unit) {
        return Translated::Cpp("void".to_string());
    }
    translate_ty(pool, names, ty, emitted_types, boxed, type_vars)
}

/// A callable-typed parameter as `std::function<Ret(Slots...)>` plus its
/// declared BAML param names ("" for unnamed). Optional callable params
/// (`y?: int`) become `Arg` slots, so an arg BAML omits materializes as an
/// unset `Arg` and the host's own default applies.
fn translate_callable_ty(
    pool: &SymbolPool,
    names: &CppNames,
    callable_params: &[CallableParam],
    ret: &Ty,
    emitted_types: &BTreeSet<Name>,
    type_vars: &[CppName],
) -> Result<(String, Vec<String>), String> {
    let mut slots = Vec::new();
    let mut wire_names = Vec::new();
    for p in callable_params {
        let slot = match translate_ty(
            pool,
            names,
            &p.ty,
            emitted_types,
            &BTreeSet::new(),
            type_vars,
        ) {
            Translated::Cpp(t) => t,
            Translated::NotYet | Translated::Unsupported(_) => {
                return Err(format!("callable param type {}", p.ty));
            }
        };
        slots.push(match p.mode {
            CodegenFunctionParamMode::Required => slot,
            CodegenFunctionParamMode::Optional => format!("::baml::Arg<{slot}>"),
        });
        wire_names.push(
            p.name
                .as_ref()
                .map(|n| n.as_str().to_string())
                .unwrap_or_default(),
        );
    }
    let ret =
        match translate_return_ty(pool, names, ret, emitted_types, &BTreeSet::new(), type_vars) {
            Translated::Cpp(t) => t,
            Translated::NotYet | Translated::Unsupported(_) => {
                return Err(format!("callable return type {ret}"));
            }
        };
    Ok((
        format!("std::function<{ret}({})>", slots.join(", ")),
        wire_names,
    ))
}

// ---------------------------------------------------------------------------
// Rendering
// ---------------------------------------------------------------------------

/// How a callable renders: as a free function, a method declaration inside a
/// struct, an inline in-struct definition, or an out-of-line member
/// definition.
enum RenderPos<'a> {
    Free,
    StaticDecl,
    InstanceDecl,
    StaticDef { class: &'a EmittedClass },
    InstanceDef { class: &'a EmittedClass },
    StaticInline,
    InstanceInline,
}

fn push_doc(buf: &mut String, indent: &str, doc: Option<&String>, raises: &[String]) {
    if let Some(doc) = doc {
        for line in doc.lines() {
            if line.is_empty() {
                let _ = writeln!(buf, "{indent}///");
            } else {
                let _ = writeln!(buf, "{indent}/// {line}");
            }
        }
    }
    if !raises.is_empty() {
        let _ = writeln!(buf, "{indent}/// Raises: {}", raises.join(", "));
    }
}

fn open_namespaces(buf: &mut String, ns: &[String]) {
    for seg in ns {
        let _ = writeln!(buf, "namespace {seg} {{");
    }
}

fn close_namespaces(buf: &mut String, ns: &[String]) {
    for seg in ns.iter().rev() {
        let _ = writeln!(buf, "}}  // namespace {seg}");
    }
}

fn by_value_or_cref(ty: &str) -> String {
    match ty {
        "int64_t" | "double" | "bool" | "std::monostate" => ty.to_string(),
        _ => format!("const {ty}&"),
    }
}

fn signature(f: &EmittedFn, async_variant: bool, pos: &RenderPos) -> String {
    let mut params: Vec<String> = f
        .params
        .iter()
        .map(|p| format!("{} {}", by_value_or_cref(&p.ty), p.name.declared()))
        .collect();
    if let Some(opts_name) = &f.opts_name {
        let default = match pos {
            RenderPos::StaticDef { .. } | RenderPos::InstanceDef { .. } => "",
            _ => " = {}",
        };
        let qualified_opts = match pos {
            RenderPos::StaticDef { class } | RenderPos::InstanceDef { class } => {
                format!("{}::{}", class.self_type(), opts_name.declared())
            }
            _ => opts_name.declared().to_string(),
        };
        params.push(format!(
            "{qualified_opts} {opts}{default}",
            opts = GeneratorIdent::OptsParam.token()
        ));
    }
    let (ret, suffix) = if async_variant {
        (format!("::baml::Future<{}>", f.ret), "_async")
    } else {
        (f.ret.clone(), "")
    };
    let prefix = match pos {
        RenderPos::StaticDecl | RenderPos::StaticInline => "static ",
        _ => "",
    };
    let owner = match pos {
        RenderPos::StaticDef { class } | RenderPos::InstanceDef { class } => {
            format!("{}::", class.self_type())
        }
        _ => String::new(),
    };
    let constness = match pos {
        RenderPos::InstanceDecl | RenderPos::InstanceDef { .. } | RenderPos::InstanceInline => {
            " const"
        }
        _ => "",
    };
    format!(
        "{tpl}{prefix}{ret} {owner}{name}{suffix}({params}){constness}",
        tpl = template_prefix(&f.type_params),
        name = f.name.declared(),
        params = params.join(", ")
    )
}

fn render_opts_struct(buf: &mut String, indent: &str, f: &EmittedFn) {
    let Some(opts_name) = &f.opts_name else {
        return;
    };
    let opts_name = opts_name.declared();
    let _ = writeln!(buf, "{indent}struct {opts_name} {{");
    for p in &f.opt_params {
        let name = p.name.declared();
        let arg_ty = format!("::baml::Arg<{}>", p.ty);
        let _ = writeln!(buf, "{indent}  {arg_ty} {name};");
        let _ = writeln!(
            buf,
            "{indent}  {opts_name}& {setter}({arg_ty} {value}) {{\n\
             {indent}    {name} = std::move({value});\n\
             {indent}    return *this;\n\
             {indent}  }}",
            setter = p.setter.declared(),
            value = naming::GeneratorIdent::SetterValueParam.token()
        );
    }
    let _ = writeln!(buf, "{indent}}};");
}

/// Emits one binding body: runtime init, type args (class params first, then
/// the callable's own — De Bruijn order), self (for instance methods),
/// required args, set optional args, then the call. `self_type` is the
/// receiver's parameterized C++ spelling for instance methods.
fn render_body(
    buf: &mut String,
    indent: &str,
    f: &EmittedFn,
    async_variant: bool,
    self_type: Option<&str>,
) {
    let args = GeneratorIdent::ArgsLocal.token();
    let w = GeneratorIdent::WriterParam.token();
    let m = GeneratorIdent::TyWriterParam.token();
    let opts = GeneratorIdent::OptsParam.token();
    // Inside a template body, a Codec<ConcreteClass> reference is
    // non-dependent and would be checked at definition -- before the Codec
    // specializations, which render after the classes. dependent_t defers
    // the lookup to instantiation time.
    let codec_ty = |ty: &str| -> String {
        match f.class_type_params.iter().chain(&f.type_params).next() {
            Some(dep) => format!(
                "::baml::detail::dependent_t<{ty}, {dep}>",
                dep = dep.identifier()
            ),
            None => ty.to_string(),
        }
    };
    let _ = writeln!(
        buf,
        "{indent}::baml_sdk::{detail}::{ensure}();",
        detail = GeneratorIdent::DetailNamespace.token(),
        ensure = GeneratorIdent::EnsureRuntime.token()
    );
    let _ = writeln!(buf, "{indent}::baml::detail::ArgsEncoder {args};");
    for param in f.class_type_params.iter().chain(&f.type_params) {
        let _ = writeln!(
            buf,
            "{indent}{args}.AddTypeArg(\"{wire}\", [](::baml::detail::wire::Writer& {m}) {{ \
             ::baml::Ty<{cpp}>::Encode({m}); }});",
            wire = param.wire(),
            cpp = param.identifier()
        );
    }
    if let Some(self_type) = self_type {
        let _ = writeln!(
            buf,
            "{indent}{args}.AddArg(\"self\", [&](::baml::detail::wire::Writer& {w}) {{ \
             ::baml::Codec<{ty}>::Encode({w}, *this); }});",
            ty = codec_ty(self_type)
        );
    }
    for p in &f.params {
        if let Some(callable_names) = &p.callable_names {
            let names_array = callable_names
                .iter()
                .map(|n| format!("std::string(\"{n}\")"))
                .collect::<Vec<_>>()
                .join(", ");
            let _ = writeln!(
                buf,
                "{indent}{args}.AddArg(\"{wire}\", [&](::baml::detail::wire::Writer& {w}) {{ \
                 ::baml::detail::EncodeCallable({w}, {value}, \
                 std::array<std::string, {len}>{{{{{names_array}}}}}); }});",
                wire = p.name.wire(),
                value = p.name.identifier(),
                len = callable_names.len()
            );
        } else {
            let _ = writeln!(
                buf,
                "{indent}{args}.AddArg(\"{wire}\", [&](::baml::detail::wire::Writer& {w}) {{ \
                 ::baml::Codec<{ty}>::Encode({w}, {value}); }});",
                wire = p.name.wire(),
                ty = codec_ty(&p.ty),
                value = p.name.identifier()
            );
        }
    }
    for p in &f.opt_params {
        let field = p.name.identifier();
        let _ = writeln!(
            buf,
            "{indent}if ({opts}.{field}.is_set()) {{\n{indent}  \
             {args}.AddArg(\"{wire}\", [&](::baml::detail::wire::Writer& {w}) {{ \
             ::baml::Codec<{ty}>::Encode({w}, {opts}.{field}.value()); }});\n{indent}}}",
            wire = p.name.wire(),
            ty = codec_ty(&p.ty)
        );
    }
    let call = if async_variant {
        "StartCall"
    } else {
        "CallSync"
    };
    let _ = writeln!(
        buf,
        "{indent}return ::baml::detail::{call}<{ret}>(\"{fqn}\", std::move({args}));",
        ret = f.ret,
        fqn = f.call_fqn,
    );
}

fn render_header(
    enums: &[EmittedEnum],
    classes: &[EmittedClass],
    fns_by_namespace: &BTreeMap<Vec<String>, Vec<EmittedFn>>,
    skipped: &[String],
) -> String {
    let mut buf = String::new();
    buf.push_str(
        "// Generated by sdkgen_cpp - do not edit.\n\
         #ifndef BAML_SDK_H_\n\
         #define BAML_SDK_H_\n\n\
         #include <array>\n\
         #include <cstdint>\n\
         #include <functional>\n\
         #include <map>\n\
         #include <optional>\n\
         #include <string>\n\
         #include <utility>\n\
         #include <variant>\n\
         #include <vector>\n\n\
         #include <baml/baml.h>\n\n\
         namespace baml_sdk {\n\n",
    );
    let detail = GeneratorIdent::DetailNamespace.token();
    let _ = writeln!(buf, "namespace {detail} {{");
    buf.push_str(
        "// Lazily initializes the process-global runtime from the embedded\n\
         // BAML sources (see src/_inlinedbaml.cc). Every binding calls this.\n",
    );
    let _ = writeln!(buf, "void {}();", GeneratorIdent::EnsureRuntime.token());
    let _ = writeln!(buf, "}}  // namespace {detail}");

    // Forward declarations: method signatures may reference classes defined
    // later (C++ allows incomplete types in declarations).
    if !classes.is_empty() {
        buf.push('\n');
        for c in classes {
            open_namespaces(&mut buf, &c.ns);
            let _ = write!(buf, "{}", c.template_prefix());
            let _ = writeln!(buf, "struct {};", c.name.declared());
            close_namespaces(&mut buf, &c.ns);
        }
    }

    for e in enums {
        buf.push('\n');
        open_namespaces(&mut buf, &e.ns);
        push_doc(&mut buf, "", e.doc.as_ref(), &[]);
        // Hash-valued enumerators: the value derives from the wire value, so
        // it is stable across variant reordering and regeneration. The wire
        // still carries names; the ordinal is identity only.
        buf.push_str("// Enumerator values: FNV-1a-64 of the wire value (reorder-stable).\n");
        let _ = writeln!(buf, "enum class {} : uint64_t {{", e.name.declared());
        let mut seen: HashMap<u64, &str> = HashMap::new();
        for (variant, value) in &e.variants {
            let hash = naming::fnv1a64(value);
            if let Some(other) = seen.insert(hash, value) {
                // ~2^-64 within one enum; equal enumerator values would make
                // the two variants compare equal, so this must be loud, not
                // a suffix.
                panic!(
                    "enum {}: FNV-1a-64 collision between variant values \
                     '{other}' and '{value}'",
                    e.name.wire()
                );
            }
            let _ = writeln!(buf, "  {} = 0x{hash:016x}ULL,", variant.declared());
        }
        buf.push_str("};\n");
        close_namespaces(&mut buf, &e.ns);
    }

    // Classes are already in dependency order from the fixed-point loop.
    for c in classes {
        buf.push('\n');
        open_namespaces(&mut buf, &c.ns);
        push_doc(&mut buf, "", c.doc.as_ref(), &[]);
        let _ = write!(buf, "{}", c.template_prefix());
        let _ = writeln!(buf, "struct {} {{", c.name.declared());
        for field in &c.fields {
            let _ = writeln!(buf, "  {} {};", field.ty, field.name.declared());
        }
        for f in c.static_methods.iter().chain(&c.instance_methods) {
            render_opts_struct(&mut buf, "  ", f);
        }
        // Template classes define their methods inline (the bodies need the
        // class's template params); non-template classes split decl/def
        // unless the method itself is generic.
        for (methods, is_instance) in [(&c.static_methods, false), (&c.instance_methods, true)] {
            for f in methods {
                push_doc(&mut buf, "  ", f.doc.as_ref(), &f.raises);
                if c.is_template() || f.is_template() {
                    let inline_pos = if is_instance {
                        RenderPos::InstanceInline
                    } else {
                        RenderPos::StaticInline
                    };
                    for async_variant in [false, true] {
                        let _ = writeln!(buf, "  {} {{", signature(f, async_variant, &inline_pos));
                        let self_type = if is_instance {
                            Some(c.self_type())
                        } else {
                            None
                        };
                        render_body(&mut buf, "    ", f, async_variant, self_type.as_deref());
                        buf.push_str("  }\n");
                    }
                } else {
                    let decl_pos = if is_instance {
                        RenderPos::InstanceDecl
                    } else {
                        RenderPos::StaticDecl
                    };
                    let _ = writeln!(buf, "  {};", signature(f, false, &decl_pos));
                    let _ = writeln!(buf, "  {};", signature(f, true, &decl_pos));
                }
            }
        }
        let eq_terms: Vec<String> = c
            .fields
            .iter()
            .map(|field| {
                let n = field.name.identifier();
                format!("a.{n} == b.{n}")
            })
            .collect();
        let eq_expr = if eq_terms.is_empty() {
            "true".to_string()
        } else {
            eq_terms.join(" && ")
        };
        let _ = writeln!(
            buf,
            "  friend bool operator==(const {n}& a, const {n}& b) {{\n    \
             return {eq_expr};\n  }}\n  \
             friend bool operator!=(const {n}& a, const {n}& b) {{ return !(a == b); }}",
            n = c.name.declared()
        );
        buf.push_str("};\n");
        close_namespaces(&mut buf, &c.ns);
    }

    for (ns, fns) in fns_by_namespace {
        buf.push('\n');
        open_namespaces(&mut buf, ns);
        for f in fns {
            render_opts_struct(&mut buf, "", f);
            push_doc(&mut buf, "", f.doc.as_ref(), &f.raises);
            if f.is_template() {
                for async_variant in [false, true] {
                    let _ = writeln!(buf, "{} {{", signature(f, async_variant, &RenderPos::Free));
                    render_body(&mut buf, "  ", f, async_variant, None);
                    buf.push_str("}\n");
                }
            } else {
                let _ = writeln!(buf, "{};", signature(f, false, &RenderPos::Free));
                let _ = writeln!(buf, "{};", signature(f, true, &RenderPos::Free));
            }
        }
        close_namespaces(&mut buf, ns);
    }

    buf.push_str("\n}  // namespace baml_sdk\n");

    render_codecs(&mut buf, enums, classes);

    if !skipped.is_empty() {
        buf.push_str("\n// Symbols not yet emitted by this sdkgen_cpp slice:\n");
        for line in skipped {
            let _ = writeln!(buf, "//   {line}");
        }
    }
    buf.push_str("\n#endif  // BAML_SDK_H_\n");
    buf
}

/// Codec<T> and Ty<T> specializations for the generated enums and classes.
/// Emitted in the header (inline) so generic bindings can instantiate them
/// from any translation unit. Generic classes get partial specializations.
fn render_codecs(buf: &mut String, enums: &[EmittedEnum], classes: &[EmittedClass]) {
    buf.push_str("\nnamespace baml {\n");

    for e in enums {
        let q = e.name.identifier();
        let fqn = e.name.wire();
        let _ = writeln!(
            buf,
            "\ntemplate <>\nstruct Ty<{q}> {{\n  \
             static void Encode(detail::wire::Writer& m) {{\n    \
             detail::wire::Writer enum_ty;\n    \
             enum_ty.StringField(1, \"{fqn}\");\n    \
             m.MessageField(3, enum_ty);\n  }}\n}};",
        );
        let _ = writeln!(
            buf,
            "\ntemplate <>\nstruct Codec<{q}> {{\n  \
             static void Encode(detail::wire::Writer& value_msg, {q} v) {{\n    \
             detail::wire::Writer e;\n    \
             e.StringField(1, \"{fqn}\");\n    \
             e.StringField(2, ToWire(v));\n    \
             value_msg.MessageField(9, e);\n  }}\n  \
             static {q} Decode(const detail::OutboundValue& v) {{\n    \
             if (v.kind != detail::OutboundValue::Kind::Enum) {{\n      \
             detail::KindMismatch(\"enum {fqn}\", v);\n    }}\n    \
             return FromWire(v.string_v);\n  }}",
        );
        buf.push_str("  static const char* ToWire(");
        let _ = write!(buf, "{q} v) {{\n    switch (v) {{\n");
        for (variant, value) in &e.variants {
            let _ = writeln!(
                buf,
                "      case {q}::{variant}: return \"{value}\";",
                variant = variant.declared()
            );
        }
        buf.push_str("    }\n    throw BamlError(\"invalid enum value\");\n  }\n");
        let _ = writeln!(buf, "  static {q} FromWire(const std::string& value) {{");
        for (variant, value) in &e.variants {
            let _ = writeln!(
                buf,
                "    if (value == \"{value}\") return {q}::{variant};",
                variant = variant.declared()
            );
        }
        let _ = writeln!(
            buf,
            "    throw BamlError(\"unknown variant '\" + value + \"' for enum {fqn}\");\n  \
             }}\n}};",
        );
    }

    for c in classes {
        let spec_prefix = if c.is_template() {
            template_prefix(&c.generic_params)
        } else {
            "template <>\n".to_string()
        };
        let q = c.qualified_self_type();

        if c.alias_wrapper {
            // Structural codec: aliases have no wire identity, so the
            // wrapper encodes/decodes its resolved type directly, wrapping
            // and unwrapping `value`. No Ty<> specialization (an alias is
            // not a nominal type the engine can bind a TypeVar to).
            let inner = &c.fields[0].ty;
            let field = c.fields[0].name.identifier();
            let _ = write!(buf, "\n{spec_prefix}");
            let _ = writeln!(
                buf,
                "struct Codec<{q}> {{\n  \
                 static void Encode(detail::wire::Writer& value_msg, const {q}& v) {{\n    \
                 Codec<{inner}>::Encode(value_msg, v.{field});\n  }}\n  \
                 static {q} Decode(const detail::OutboundValue& v) {{\n    \
                 return {q}{{Codec<{inner}>::Decode(v)}};\n  }}\n}};"
            );
            continue;
        }

        let fqn = c.name.wire();

        // Ty<Class>: BamlTy.class_ty = 2 { name = 1, type_args = 2 }.
        let _ = write!(buf, "\n{spec_prefix}");
        let _ = writeln!(
            buf,
            "struct Ty<{q}> {{\n  \
             static void Encode(detail::wire::Writer& m) {{\n    \
             detail::wire::Writer class_ty;\n    \
             class_ty.StringField(1, \"{fqn}\");",
        );
        for param in &c.generic_params {
            let _ = writeln!(
                buf,
                "    {{\n      detail::wire::Writer arg;\n      \
                 Ty<{param}>::Encode(arg);\n      \
                 class_ty.MessageField(2, arg);\n    }}",
                param = param.identifier()
            );
        }
        buf.push_str("    m.MessageField(2, class_ty);\n  }\n};\n");

        if is_tagged_heap_handle_class(&c.pool_name) {
            // Bare tagged-heap-handle wire form (Python parity: BamlStream
            // encodes handle_value(ADT_TAGGED_HEAP_HANDLE), never a
            // class_value; the engine substitutes the type params from the
            // tagged handle).
            let handle_field = c.fields[0].name.identifier();
            let tag = ADT_TAGGED_HEAP_HANDLE;
            let _ = write!(buf, "\n{spec_prefix}");
            let _ = writeln!(
                buf,
                "struct Codec<{q}> {{\n  \
                 static void Encode(detail::wire::Writer& value_msg, const {q}& v) {{\n    \
                 detail::wire::Writer handle;\n    \
                 handle.Uint64Field(1, v.{handle_field}.CloneKeyForWire());\n    \
                 handle.Int64Field(2, {tag});\n    \
                 value_msg.MessageField(10, handle);\n  }}\n  \
                 static {q} Decode(const detail::OutboundValue& v) {{\n    \
                 if (v.kind != detail::OutboundValue::Kind::Handle ||\n      \
                 v.handle_type != {tag}) {{\n      \
                 detail::KindMismatch(\"stream handle {fqn}\", v);\n    }}\n    \
                 return {q}{{::baml::Handle(v.handle_key, v.handle_type)}};\n  }}\n}};"
            );
            continue;
        }

        let _ = write!(buf, "\n{spec_prefix}");
        let _ = writeln!(buf, "struct Codec<{q}> {{");
        // Encode: InboundValue.class_value = 8 { fields = 2, class_ty = 3 }
        let _ = writeln!(
            buf,
            "  static void Encode(detail::wire::Writer& value_msg, const {q}& v) {{\n    \
             detail::wire::Writer cls;"
        );
        for field in &c.fields {
            let _ = writeln!(
                buf,
                "    {{\n      detail::wire::Writer entry;\n      \
                 entry.StringField(1, \"{wire}\");\n      \
                 detail::wire::Writer val;\n      \
                 Codec<{ty}>::Encode(val, v.{name});\n      \
                 entry.MessageField(6, val);\n      \
                 cls.MessageField(2, entry);\n    }}",
                wire = field.name.wire(),
                ty = field.ty,
                name = field.name.identifier()
            );
        }
        let _ = writeln!(
            buf,
            "    detail::wire::Writer class_ty;\n    \
             class_ty.StringField(1, \"{fqn}\");",
        );
        for param in &c.generic_params {
            let _ = writeln!(
                buf,
                "    {{\n      detail::wire::Writer arg;\n      \
                 Ty<{param}>::Encode(arg);\n      \
                 class_ty.MessageField(2, arg);\n    }}",
                param = param.identifier()
            );
        }
        buf.push_str(
            "    cls.MessageField(3, class_ty);\n    \
             value_msg.MessageField(8, cls);\n  }\n",
        );
        // Decode: strict field mapping (extra field or missing field = error,
        // pydantic extra="forbid" parity), FQN-checked for precise
        // variant-of-class dispatch. Fields land in optional locals so
        // non-default-constructible field types (baml::Box) work.
        let _ = writeln!(
            buf,
            "  static {q} Decode(const detail::OutboundValue& v) {{\n    \
             if (v.kind != detail::OutboundValue::Kind::Class ||\n      \
             (!v.name.empty() && v.name != \"{fqn}\")) {{\n      \
             detail::KindMismatch(\"class {fqn}\", v);\n    }}",
        );
        for field in &c.fields {
            let _ = writeln!(
                buf,
                "    std::optional<{ty}> field_{name};",
                ty = field.ty,
                name = field.name.declared()
            );
        }
        buf.push_str("    for (const auto& field : v.fields) {\n");
        let mut first = true;
        for field in &c.fields {
            let kw = if first { "if" } else { "} else if" };
            first = false;
            let _ = writeln!(
                buf,
                "      {kw} (field.first == \"{wire}\") {{\n        \
                 field_{name} = Codec<{ty}>::Decode(field.second);",
                wire = field.name.wire(),
                ty = field.ty,
                name = field.name.declared()
            );
        }
        if !c.fields.is_empty() {
            buf.push_str("      } else {\n");
        } else {
            buf.push_str("      {\n");
        }
        let _ = writeln!(
            buf,
            "        throw BamlError(\"unexpected field '\" + field.first + \"' on {fqn}\");\n      \
             }}\n    }}",
        );
        for field in &c.fields {
            let _ = writeln!(
                buf,
                "    if (!field_{name}.has_value()) {{\n      \
                 throw BamlError(\"missing field '{wire}' on {fqn}\");\n    }}",
                name = field.name.declared(),
                wire = field.name.wire()
            );
        }
        let ctor_args: Vec<String> = c
            .fields
            .iter()
            .map(|field| format!("std::move(*field_{})", field.name.declared()))
            .collect();
        let _ = writeln!(
            buf,
            "    return {q}{{{args}}};\n  }}\n}};",
            args = ctor_args.join(", ")
        );
    }

    buf.push_str("\n}  // namespace baml\n");
}

fn render_bindings(
    classes: &[EmittedClass],
    fns_by_namespace: &BTreeMap<Vec<String>, Vec<EmittedFn>>,
) -> String {
    let mut buf = String::new();
    buf.push_str(
        "// Generated by sdkgen_cpp - do not edit.\n\
         #include <baml_sdk.h>\n\n\
         #include <utility>\n\n\
         namespace baml_sdk {\n",
    );

    for c in classes {
        if c.is_template() {
            continue; // template methods define inline in the header
        }
        let has_non_template = c
            .static_methods
            .iter()
            .chain(&c.instance_methods)
            .any(|f| !f.is_template());
        if !has_non_template {
            continue;
        }
        buf.push('\n');
        open_namespaces(&mut buf, &c.ns);
        for f in c.static_methods.iter().filter(|f| !f.is_template()) {
            for async_variant in [false, true] {
                let pos = RenderPos::StaticDef { class: c };
                let _ = writeln!(buf, "\n{} {{", signature(f, async_variant, &pos));
                render_body(&mut buf, "  ", f, async_variant, None);
                buf.push_str("}\n");
            }
        }
        for f in c.instance_methods.iter().filter(|f| !f.is_template()) {
            for async_variant in [false, true] {
                let pos = RenderPos::InstanceDef { class: c };
                let _ = writeln!(buf, "\n{} {{", signature(f, async_variant, &pos));
                render_body(&mut buf, "  ", f, async_variant, Some(&c.self_type()));
                buf.push_str("}\n");
            }
        }
        close_namespaces(&mut buf, &c.ns);
    }

    let opts = GeneratorIdent::OptsParam.token();
    for (ns, fns) in fns_by_namespace {
        let non_template: Vec<&EmittedFn> = fns.iter().filter(|f| !f.is_template()).collect();
        if non_template.is_empty() {
            continue;
        }
        buf.push('\n');
        open_namespaces(&mut buf, ns);
        for f in non_template {
            for async_variant in [false, true] {
                let sig = signature(f, async_variant, &RenderPos::Free);
                // Free-function definitions must not repeat the default arg.
                let sig = sig.replace(&format!(" {opts} = {{}}"), &format!(" {opts}"));
                let _ = writeln!(buf, "\n{sig} {{");
                render_body(&mut buf, "  ", f, async_variant, None);
                buf.push_str("}\n");
            }
        }
        close_namespaces(&mut buf, ns);
    }

    buf.push_str("\n}  // namespace baml_sdk\n");
    buf
}

fn render_inlinedbaml(user_baml_files: &[UserBamlFile], baml_bytecode: &[u8]) -> String {
    let mut buf = String::new();
    buf.push_str(
        "// Generated by sdkgen_cpp - do not edit. Embedded BAML bytecode (the\n\
         // runtime payload), the original sources (reference only), and lazy\n\
         // runtime initialization.\n\
         #include <cstdint>\n\
         #include <mutex>\n\n\
         #include <baml/baml.h>\n\n\
         namespace baml_sdk {\n",
    );
    let detail = GeneratorIdent::DetailNamespace.token();
    let _ = writeln!(buf, "namespace {detail} {{");
    for (rel_path, _) in user_baml_files {
        let path = rel_path.to_string_lossy().replace('\\', "/");
        let _ = writeln!(buf, "// source: {path}");
    }
    buf.push_str("\nnamespace {\nconst uint8_t kBamlBytecode[] = {");
    for (i, byte) in baml_bytecode.iter().enumerate() {
        if i % 20 == 0 {
            buf.push_str("\n  ");
        }
        let _ = write!(buf, "{byte},");
    }
    buf.push_str("\n};\n}  // namespace\n");
    let _ = writeln!(buf, "\nvoid {}() {{", GeneratorIdent::EnsureRuntime.token());
    // The canonical version stamped at generation time: register_bridge
    // requires exact equality with the loaded runtime.
    let _ = writeln!(
        buf,
        "  static std::once_flag once;\n  \
         std::call_once(once, [] {{\n    \
         ::baml::InitializeRuntimeFromBytecode(kBamlBytecode, sizeof(kBamlBytecode),\n                                          \
         \"{version}\");\n  \
         }});\n\
         }}\n",
        version = baml_version::CANONICAL_VERSION
    );
    let _ = writeln!(buf, "}}  // namespace {detail}");
    buf.push_str("}  // namespace baml_sdk\n");
    buf
}
