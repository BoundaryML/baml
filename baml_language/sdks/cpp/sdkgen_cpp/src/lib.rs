//! C++ SDK emitter, scoped to the packaging/publishing slice (bridge-week
//! steps 1-8): the single-header layout, namespace routing, free functions
//! with required + optional arguments (per-function opts structs, spec D4),
//! classes + enums with generated `Codec<T>` specializations, transparent
//! and recursive type aliases, and recursion via `baml::Box` cycle-breaking.
//! Post-step-8 features (async, methods, callbacks, generics, streaming
//! companions, media/handles, unions) are skipped and reported in a trailing
//! header comment (no silent caps); the full implementation is preserved on
//! the avery/bridge-cpp-full branch.
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
//!   `src/bindings.cc`      - definitions over `::baml::detail`
//!   `src/_inlinedbaml.cc`  - embedded BAML bytecode + lazy runtime init

mod naming;

use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    fmt::Write as _,
    path::PathBuf,
};

use baml_codegen_types::{Class, Enum, Function, Name, Symbol, SymbolPool, Ty};
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
     -> Result<Option<EmittedType>, String> {
        match &pool[name] {
            Symbol::Class(class_def) => {
                Ok(
                    emit_class(pool, &names, name, class_def, emitted_types, boxed)?
                        .map(EmittedType::Class),
                )
            }
            Symbol::TypeAlias(alias) if alias.recursive => {
                Ok(
                    emit_alias_wrapper(pool, &names, name, alias, emitted_types, boxed)?
                        .map(EmittedType::Class),
                )
            }
            Symbol::TypeAlias(alias) => {
                Ok(
                    emit_alias_using(pool, &names, name, alias, emitted_types, boxed)?
                        .map(EmittedType::Using),
                )
            }
            Symbol::Enum(_) | Symbol::Function(_) => unreachable!(),
        }
    };
    let mut classes: Vec<EmittedType> = Vec::new();
    let mut pending: Vec<&Name> = pool_names
        .iter()
        .copied()
        .filter(|name| {
            if name.is_stream() {
                return false;
            }
            match &pool[*name] {
                Symbol::Class(class_def) => class_def.generic_params.is_empty(),
                Symbol::TypeAlias(_) => true,
                Symbol::Enum(_) | Symbol::Function(_) => false,
            }
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

    // Pass 4: free functions over the emitted type set.
    let mut fns_by_namespace: BTreeMap<Vec<String>, Vec<EmittedFn>> = BTreeMap::new();
    for name in &pool_names {
        let Symbol::Function(function) = &pool[*name] else {
            continue;
        };
        if name.is_stream() || name.bare_name().contains('$') {
            skipped.push(format!("{name}: companion functions (post-step-8)"));
            continue;
        }
        if !function.generic_params.is_empty() {
            skipped.push(format!("{name}: generic function (post-step-8)"));
            continue;
        }
        match emit_callable(
            pool,
            &names,
            &BamlFqn::symbol(name),
            function,
            &emitted_types,
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
        render_bindings(&fns_by_namespace),
    );
    out.insert(
        PathBuf::from("src/_inlinedbaml.cc"),
        render_inlinedbaml(user_baml_files, baml_bytecode),
    );
    for (rel, content) in BRIDGE_HEADERS {
        out.insert(PathBuf::from(rel), (*content).to_string());
    }
    out
}

/// The bridge runtime headers, vendored verbatim into every generated SDK
/// (embedded at emitter build time, so headers and generator are the same
/// version by construction). The generated tree is self-contained source;
/// the only external artifact is the shared runtime library the bridge
/// dlopens at first use.
const BRIDGE_HEADERS: &[(&str, &str)] = &[
    (
        "include/baml_cffi.h",
        include_str!("../../../../crates/bridge_cffi/include/baml_cffi.h"),
    ),
    (
        "include/baml/arg.h",
        include_str!("../../bridge_cpp/include/baml/arg.h"),
    ),
    (
        "include/baml/baml.h",
        include_str!("../../bridge_cpp/include/baml/baml.h"),
    ),
    (
        "include/baml/box.h",
        include_str!("../../bridge_cpp/include/baml/box.h"),
    ),
    (
        "include/baml/buffer.h",
        include_str!("../../bridge_cpp/include/baml/buffer.h"),
    ),
    (
        "include/baml/codec.h",
        include_str!("../../bridge_cpp/include/baml/codec.h"),
    ),
    (
        "include/baml/errors.h",
        include_str!("../../bridge_cpp/include/baml/errors.h"),
    ),
    (
        "include/baml/runtime.h",
        include_str!("../../bridge_cpp/include/baml/runtime.h"),
    ),
    (
        "include/baml/detail/call.h",
        include_str!("../../bridge_cpp/include/baml/detail/call.h"),
    ),
    (
        "include/baml/detail/json.h",
        include_str!("../../bridge_cpp/include/baml/detail/json.h"),
    ),
    (
        "include/baml/detail/loader.h",
        include_str!("../../bridge_cpp/include/baml/detail/loader.h"),
    ),
    (
        "include/baml/detail/proto.h",
        include_str!("../../bridge_cpp/include/baml/detail/proto.h"),
    ),
    (
        "include/baml/detail/registry.h",
        include_str!("../../bridge_cpp/include/baml/detail/registry.h"),
    ),
    (
        "include/baml/detail/wire.h",
        include_str!("../../bridge_cpp/include/baml/detail/wire.h"),
    ),
];

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
        // Post-step-8 features are disabled: stream companions and
        // $-companion functions never allocate names.
        if name.is_stream() || name.bare_name().contains('$') {
            continue;
        }
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
                if !class_def.generic_params.is_empty() {
                    continue; // generics disabled (post-step-8)
                }
                request_namespace_segments(&mut requests, name, true);
                requests.insert(NameRequest::new(BamlFqn::symbol(name), CppNameKind::Class));
                for prop in &class_def.properties {
                    requests.insert(NameRequest::new(
                        BamlFqn::member(name, prop.name.as_str()),
                        CppNameKind::Field,
                    ));
                }
            }
            Symbol::Function(function) => {
                if !function.generic_params.is_empty() {
                    continue; // generics disabled (post-step-8)
                }
                request_namespace_segments(&mut requests, name, false);
                let fqn = BamlFqn::symbol(name);
                requests.insert(function_request(name));
                request_callable_members(&mut requests, &fqn, function);
            }
            // Non-recursive aliases emit a `using` declaration; recursive
            // aliases emit a named wrapper struct that breaks the type
            // recursion (an alias-declaration cannot reference itself).
            Symbol::TypeAlias(alias) => {
                request_namespace_segments(&mut requests, name, true);
                if alias.recursive {
                    requests.insert(NameRequest::new(BamlFqn::symbol(name), CppNameKind::Class));
                    requests.insert(alias_value_field_request(name));
                } else {
                    requests.insert(NameRequest::new(
                        BamlFqn::symbol(name),
                        CppNameKind::TypeAlias,
                    ));
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
    ns: Vec<String>,
    name: CppName,
    doc: Option<String>,
    fields: Vec<EmittedField>,
    /// A recursive-alias wrapper struct: one `value` field holding the
    /// alias's resolved type, structural codec (aliases have no wire
    /// identity), no methods.
    alias_wrapper: bool,
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
    let mut fields = Vec::new();
    for prop in &class_def.properties {
        match translate_ty(pool, names, &prop.ty, emitted_types, boxed) {
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
    Ok(Some(EmittedClass {
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
        fields,
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
    let inner = match translate_ty(pool, names, &alias.resolves_to, emitted_types, boxed) {
        Translated::Cpp(ty) => ty,
        Translated::NotYet => return Ok(None),
        Translated::Unsupported(reason) => {
            return Err(format!("aliased type: {reason}"));
        }
    };
    Ok(Some(EmittedClass {
        ns: allocated_namespace(names, name, true),
        name: names
            .get(&NameRequest::new(BamlFqn::symbol(name), CppNameKind::Class))
            .clone(),
        doc: None,
        fields: vec![EmittedField {
            name: names.get(&alias_value_field_request(name)).clone(),
            ty: inner,
        }],
        alias_wrapper: true,
    }))
}

/// A named type as it interleaves in declaration order: a struct (class or
/// recursive-alias wrapper) or a `using` declaration.
enum EmittedType {
    Class(EmittedClass),
    Using(EmittedUsing),
}

/// A non-recursive type alias as a `using` declaration. A `using` is a pure
/// synonym, so no codec is emitted: `Codec<Alias>` *is* `Codec<Target>`.
struct EmittedUsing {
    ns: Vec<String>,
    name: CppName,
    target: String,
}

/// A non-recursive type alias: `type StringList = string[]` becomes
/// `using StringList = std::vector<std::string>;`. It joins the class
/// fixed point so the declaration lands after the types its target names.
fn emit_alias_using(
    pool: &SymbolPool,
    names: &CppNames,
    name: &Name,
    alias: &baml_codegen_types::TypeAlias,
    emitted_types: &BTreeSet<Name>,
    boxed: &BTreeSet<Name>,
) -> Result<Option<EmittedUsing>, String> {
    let target = match translate_ty(pool, names, &alias.resolves_to, emitted_types, boxed) {
        Translated::Cpp(ty) => ty,
        Translated::NotYet => return Ok(None),
        Translated::Unsupported(reason) => {
            return Err(format!("aliased type: {reason}"));
        }
    };
    Ok(Some(EmittedUsing {
        ns: allocated_namespace(names, name, true),
        name: names
            .get(&NameRequest::new(
                BamlFqn::symbol(name),
                CppNameKind::TypeAlias,
            ))
            .clone(),
        target,
    }))
}

// ---------------------------------------------------------------------------
// Callables (free functions and methods share this shape)
// ---------------------------------------------------------------------------

struct EmittedParam {
    name: CppName,
    ty: String,
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
    /// The BAML FQN the runtime call dispatches on (the wire symbol);
    /// never derived from C++ spellings.
    call_fqn: String,
    ret: String,
    params: Vec<EmittedParam>,
    opt_params: Vec<EmittedOptParam>,
    /// Opts struct name, when `opt_params` is non-empty.
    opts_name: Option<CppName>,
    doc: Option<String>,
    raises: Vec<String>,
}

fn emit_callable(
    pool: &SymbolPool,
    names: &CppNames,
    fqn: &BamlFqn,
    function: &Function,
    emitted_types: &BTreeSet<Name>,
) -> Result<EmittedFn, String> {
    let name = names.get(&function_request(&fqn.symbol)).clone();

    let mut params = Vec::new();
    let mut opt_params = Vec::new();
    for arg in &function.arguments {
        let ty = match translate_ty(pool, names, &arg.ty, emitted_types, &BTreeSet::new()) {
            Translated::Cpp(ty) => ty,
            Translated::NotYet | Translated::Unsupported(_) => {
                return Err(format!(
                    "argument `{}` has unsupported type {}",
                    arg.name, arg.ty
                ));
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
            });
        }
    }
    let ret = match translate_return_ty(
        pool,
        names,
        &function.return_type,
        emitted_types,
        &BTreeSet::new(),
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

    Ok(EmittedFn {
        call_fqn: name.wire().to_string(),
        name,
        ret,
        params,
        opt_params,
        opts_name,
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
) -> Translated {
    let translated = match ty {
        Ty::Int => "int64_t".to_string(),
        Ty::Float => "double".to_string(),
        Ty::String => "std::string".to_string(),
        Ty::Bool => "bool".to_string(),
        Ty::Null => "std::monostate".to_string(),
        Ty::Uint8Array => "std::vector<uint8_t>".to_string(),
        Ty::RustType => {
            return Translated::Unsupported("handle type (post-step-8)".to_string());
        }
        Ty::Bigint => {
            return Translated::Unsupported("bigint (post-step-8)".to_string());
        }
        Ty::Media(_) => {
            return Translated::Unsupported("media type (post-step-8)".to_string());
        }
        Ty::Literal(lit) => {
            // Literal types widen to their base type (Python parity).
            match lit {
                baml_base::Literal::Int(_) => "int64_t".to_string(),
                baml_base::Literal::Bigint(_) => {
                    return Translated::Unsupported("bigint literal (post-step-8)".to_string());
                }
                baml_base::Literal::Float(_) => "double".to_string(),
                baml_base::Literal::String(_) => "std::string".to_string(),
                baml_base::Literal::Bool(_) => "bool".to_string(),
            }
        }
        Ty::TypeVar(name) => {
            return Translated::Unsupported(format!("TypeVar {name} (generics post-step-8)"));
        }
        Ty::TypeAlias(name) => {
            // Aliases render by name once declared: recursive aliases
            // reference their wrapper struct like a class (boxed inside
            // their own cycle, since the box needs only the forward
            // declaration); non-recursive aliases reference their `using`
            // declaration, which cannot be forward-declared, so uses wait
            // for the declaration itself.
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
            if !emitted_types.contains(name) {
                return Translated::NotYet;
            }
            return Translated::Cpp(
                names
                    .get(&NameRequest::new(
                        BamlFqn::symbol(name),
                        CppNameKind::TypeAlias,
                    ))
                    .identifier()
                    .to_string(),
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
                match translate_ty(pool, names, arg, emitted_types, boxed) {
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
        Ty::List(inner) => match translate_ty(pool, names, inner, emitted_types, boxed) {
            Translated::Cpp(inner) => format!("std::vector<{inner}>"),
            other => return other,
        },
        Ty::Map { key, value } => {
            if !matches!(key.as_ref(), Ty::String) {
                return Translated::Unsupported("non-string map key".to_string());
            }
            match translate_ty(pool, names, value, emitted_types, boxed) {
                Translated::Cpp(value) => format!("std::unordered_map<std::string, {value}>"),
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
                match translate_ty(pool, names, item, emitted_types, boxed) {
                    Translated::Cpp(alt) => {
                        if !alternatives.contains(&alt) {
                            alternatives.push(alt);
                        }
                    }
                    other => return other,
                }
            }
            // Multi-member unions (std::variant) are disabled pending a
            // representation redesign; only the null-normalized single-
            // member forms (T? and bare T after dedup) emit.
            alternatives.sort();
            let inner = match alternatives.as_slice() {
                [] => return Translated::Unsupported("empty union".to_string()),
                [single] => single.clone(),
                _ => {
                    return Translated::Unsupported(
                        "union type (disabled pending redesign)".to_string(),
                    );
                }
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
) -> Translated {
    if matches!(ty, Ty::Unit) {
        return Translated::Cpp("void".to_string());
    }
    translate_ty(pool, names, ty, emitted_types, boxed)
}

// ---------------------------------------------------------------------------
// Rendering
// ---------------------------------------------------------------------------

/// Whether a function renders as its header declaration (defaulted opts
/// parameter) or its bindings.cc definition (no default repeated).
#[derive(Clone, Copy)]
enum RenderPos {
    Decl,
    Def,
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

fn signature(f: &EmittedFn, pos: RenderPos) -> String {
    let mut params: Vec<String> = f
        .params
        .iter()
        .map(|p| format!("{} {}", by_value_or_cref(&p.ty), p.name.declared()))
        .collect();
    if let Some(opts_name) = &f.opts_name {
        let default = match pos {
            RenderPos::Def => "",
            RenderPos::Decl => " = {}",
        };
        params.push(format!(
            "{opts_name} {opts}{default}",
            opts_name = opts_name.declared(),
            opts = GeneratorIdent::OptsParam.token()
        ));
    }
    format!(
        "{ret} {name}({params})",
        ret = f.ret,
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
fn render_body(buf: &mut String, indent: &str, f: &EmittedFn) {
    let args = GeneratorIdent::ArgsLocal.token();
    let w = GeneratorIdent::WriterParam.token();
    let opts = GeneratorIdent::OptsParam.token();
    let _ = writeln!(
        buf,
        "{indent}::baml_sdk::{detail}::{ensure}();",
        detail = GeneratorIdent::DetailNamespace.token(),
        ensure = GeneratorIdent::EnsureRuntime.token()
    );
    let _ = writeln!(buf, "{indent}::baml::detail::ArgsEncoder {args};");
    for p in &f.params {
        let _ = writeln!(
            buf,
            "{indent}{args}.AddArg(\"{wire}\", [&](::baml::detail::wire::Writer& {w}) {{ \
             ::baml::Codec<{ty}>::Encode({w}, {value}); }});",
            wire = p.name.wire(),
            ty = p.ty,
            value = p.name.identifier()
        );
    }
    for p in &f.opt_params {
        let field = p.name.identifier();
        let _ = writeln!(
            buf,
            "{indent}if ({opts}.{field}.is_set()) {{\n{indent}  \
             {args}.AddArg(\"{wire}\", [&](::baml::detail::wire::Writer& {w}) {{ \
             ::baml::Codec<{ty}>::Encode({w}, {opts}.{field}.value()); }});\n{indent}}}",
            wire = p.name.wire(),
            ty = p.ty
        );
    }
    let _ = writeln!(
        buf,
        "{indent}return ::baml::detail::CallSync<{ret}>(\"{fqn}\", std::move({args}));",
        ret = f.ret,
        fqn = f.call_fqn,
    );
}
fn render_header(
    enums: &[EmittedEnum],
    types: &[EmittedType],
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
         #include <unordered_map>\n\
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

    // Forward declarations for the structs (`using` declarations cannot be
    // forward-declared; alias uses wait for the declaration itself).
    let classes: Vec<&EmittedClass> = types
        .iter()
        .filter_map(|t| match t {
            EmittedType::Class(c) => Some(c),
            EmittedType::Using(_) => None,
        })
        .collect();
    if !classes.is_empty() {
        buf.push('\n');
        for c in &classes {
            open_namespaces(&mut buf, &c.ns);
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

    // Types are already in dependency order from the fixed-point loop.
    for t in types {
        let c = match t {
            EmittedType::Class(c) => c,
            EmittedType::Using(u) => {
                buf.push('\n');
                open_namespaces(&mut buf, &u.ns);
                let _ = writeln!(buf, "using {} = {};", u.name.declared(), u.target);
                close_namespaces(&mut buf, &u.ns);
                continue;
            }
        };
        buf.push('\n');
        open_namespaces(&mut buf, &c.ns);
        push_doc(&mut buf, "", c.doc.as_ref(), &[]);
        let _ = writeln!(buf, "struct {} {{", c.name.declared());
        for field in &c.fields {
            let _ = writeln!(buf, "  {} {};", field.ty, field.name.declared());
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
            let _ = writeln!(buf, "{};", signature(f, RenderPos::Decl));
        }
        close_namespaces(&mut buf, ns);
    }

    buf.push_str("\n}  // namespace baml_sdk\n");

    render_codecs(&mut buf, enums, &classes);

    if !skipped.is_empty() {
        buf.push_str("\n// Symbols not yet emitted by this sdkgen_cpp slice:\n");
        for line in skipped {
            let _ = writeln!(buf, "//   {line}");
        }
    }
    buf.push_str("\n#endif  // BAML_SDK_H_\n");
    buf
}

/// Codec<T> specializations for the generated enums and classes. Emitted in
/// the header (inline) so they are visible from any translation unit.
fn render_codecs(buf: &mut String, enums: &[EmittedEnum], classes: &[&EmittedClass]) {
    buf.push_str("\nnamespace baml {\n");

    for e in enums {
        let q = e.name.identifier();
        let fqn = e.name.wire();
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
        let q = c.name.identifier();

        if c.alias_wrapper {
            // Structural codec: aliases have no wire identity, so the
            // wrapper encodes/decodes its resolved type directly, wrapping
            // and unwrapping `value`. No Ty<> specialization (an alias is
            // not a nominal type the engine can bind a TypeVar to).
            let inner = &c.fields[0].ty;
            let field = c.fields[0].name.identifier();
            buf.push_str("\ntemplate <>\n");
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

        buf.push_str("\ntemplate <>\n");
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

fn render_bindings(fns_by_namespace: &BTreeMap<Vec<String>, Vec<EmittedFn>>) -> String {
    let mut buf = String::new();
    buf.push_str(
        "// Generated by sdkgen_cpp - do not edit.\n\
         #include <baml_sdk.h>\n\n\
         #include <utility>\n\n\
         namespace baml_sdk {\n",
    );

    for (ns, fns) in fns_by_namespace {
        buf.push('\n');
        open_namespaces(&mut buf, ns);
        for f in fns {
            let _ = writeln!(buf, "\n{} {{", signature(f, RenderPos::Def));
            render_body(&mut buf, "  ", f);
            buf.push_str("}\n");
        }
        close_namespaces(&mut buf, ns);
    }

    buf.push_str("\n}  // namespace baml_sdk\n");
    buf
}

/// Payload bytes per generated string-literal chunk. Worst-case escaping
/// (all octal, 4 chars/byte) keeps a chunk's literal under 33 KB -- half of
/// MSVC's 65,535-byte string-literal cap.
const BYTECODE_CHUNK_BYTES: usize = 8192;

/// Escapes bytes into C string-literal form, protobuf-style: printable
/// ASCII stays raw; quote, backslash, and `?` (trigraph paranoia) escape;
/// everything else is a fixed three-digit octal escape, so a following
/// digit can never extend it.
fn escape_bytecode(out: &mut String, bytes: &[u8]) {
    use std::fmt::Write as _;
    for &b in bytes {
        match b {
            b'"' => out.push_str("\\\""),
            b'\\' => out.push_str("\\\\"),
            b'?' => out.push_str("\\?"),
            b'\n' => out.push_str("\\n"),
            b'\r' => out.push_str("\\r"),
            b'\t' => out.push_str("\\t"),
            0x20..=0x7e => out.push(b as char),
            _ => {
                let _ = write!(out, "\\{b:03o}");
            }
        }
    }
}

fn render_inlinedbaml(user_baml_files: &[UserBamlFile], baml_bytecode: &[u8]) -> String {
    let mut buf = String::new();
    buf.push_str(
        "// Generated by sdkgen_cpp - do not edit. Embedded BAML bytecode (the\n\
         // runtime payload), the original sources (reference only), and lazy\n\
         // runtime initialization.\n\
         #include <cstddef>\n\
         #include <cstdint>\n\
         #include <mutex>\n\
         #include <string>\n\n\
         #include <baml/baml.h>\n\n\
         namespace baml_sdk {\n",
    );
    let detail = GeneratorIdent::DetailNamespace.token();
    let _ = writeln!(buf, "namespace {detail} {{");
    for (rel_path, _) in user_baml_files {
        let path = rel_path.to_string_lossy().replace('\\', "/");
        let _ = writeln!(buf, "// source: {path}");
    }
    // Bytecode as string-literal chunks (protobuf's descriptor-embedding
    // technique): string literals lex as single tokens, so multi-megabyte
    // payloads compile in near-linear time, where per-byte initializer
    // lists are pathological (and blow memory on MSVC). Each chunk holds
    // at most BYTECODE_CHUNK_BYTES payload bytes; worst-case octal
    // escaping quadruples that, staying far under MSVC's 65,535-byte
    // string-literal cap.
    buf.push_str("\nnamespace {\n");
    let chunks: Vec<&[u8]> = baml_bytecode.chunks(BYTECODE_CHUNK_BYTES).collect();
    for (i, chunk) in chunks.iter().enumerate() {
        let _ = writeln!(buf, "const char kBamlBytecodeChunk{i}[] =");
        for line in chunk.chunks(32) {
            buf.push_str("    \"");
            escape_bytecode(&mut buf, line);
            buf.push_str("\"\n");
        }
        buf.push_str("    ;\n");
    }
    buf.push_str(
        "struct BamlBytecodeChunk {\n  const char* data;\n  size_t len;\n};\n\
         const BamlBytecodeChunk kBamlBytecodeChunks[] = {\n",
    );
    for i in 0..chunks.len() {
        let _ = writeln!(
            buf,
            "    {{kBamlBytecodeChunk{i}, sizeof(kBamlBytecodeChunk{i}) - 1}},"
        );
    }
    let _ = writeln!(
        buf,
        "}};\nconst size_t kBamlBytecodeSize = {};\n}}  // namespace",
        baml_bytecode.len()
    );
    let _ = writeln!(buf, "\nvoid {}() {{", GeneratorIdent::EnsureRuntime.token());
    // The canonical version stamped at generation time: register_bridge
    // requires exact equality with the loaded runtime.
    let _ = writeln!(
        buf,
        "  static std::once_flag once;\n  \
         std::call_once(once, [] {{\n    \
         std::string bytecode;\n    \
         bytecode.reserve(kBamlBytecodeSize);\n    \
         for (const BamlBytecodeChunk& chunk : kBamlBytecodeChunks) {{\n      \
         bytecode.append(chunk.data, chunk.len);\n    \
         }}\n    \
         ::baml::InitializeRuntimeFromBytecode(\n        \
         reinterpret_cast<const uint8_t*>(bytecode.data()), bytecode.size(),\n        \
         \"{version}\");\n  \
         }});\n\
         }}\n",
        version = baml_version::CANONICAL_VERSION
    );
    let _ = writeln!(buf, "}}  // namespace {detail}");
    buf.push_str("}  // namespace baml_sdk\n");
    buf
}

#[cfg(test)]
mod bytecode_escape_tests {
    use super::escape_bytecode;

    fn escaped(bytes: &[u8]) -> String {
        let mut out = String::new();
        escape_bytecode(&mut out, bytes);
        out
    }

    #[test]
    fn printable_ascii_stays_raw() {
        assert_eq!(escaped(b"abc XYZ 09"), "abc XYZ 09");
    }

    #[test]
    fn specials_escape() {
        assert_eq!(escaped(b"\"\\?"), "\\\"\\\\\\?");
        assert_eq!(escaped(b"\n\r\t"), "\\n\\r\\t");
    }

    #[test]
    fn octal_is_fixed_width_so_digits_cannot_extend_it() {
        // 0x01 followed by ASCII '7': a variable-width octal escape would
        // lex as \017; three fixed digits keep them distinct.
        assert_eq!(escaped(&[0x01, b'7']), "\\0017");
        assert_eq!(escaped(&[0xff, 0x00]), "\\377\\000");
    }
}
