//! C++ SDK emitter, scoped to the packaging/publishing slice (bridge-week
//! steps 1-8): the single-header layout, namespace routing, free functions
//! with required + optional arguments (per-function opts structs, spec D4),
//! classes + enums with generated `codec<T>` specializations, transparent
//! and recursive type aliases, and recursion via `baml::Box` cycle-breaking.
//! multi-member unions as order-canonical `::baml::variant` aliases, and
//! typed error unions via `BamlThrown`.
//! Post-step-8 features (async, methods, callbacks, generics, streaming
//! companions, media/handles) are skipped and reported in a trailing
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

use baml_codegen_types::{
    CallableParam, Class, CodegenFunctionParamMode, Enum, Function, Name, Symbol, SymbolPool, Ty,
};

use crate::naming::{BamlFqn, CppName, CppNameKind, CppNames, GeneratorIdent, NameRequest};

/// A user BAML source path as it should appear in the emitter's
/// inlined-baml output, relative to the `baml_src/` root. Only the path is
/// embedded (as a reference comment); the runtime payload is bytecode.
pub type UserBamlFile = PathBuf;

/// Build the C++ SDK output tree for `pool`. Returned paths are relative to
/// the `baml_sdk/` output root.
pub fn to_source_code_with_bytecode(
    pool: &SymbolPool,
    user_baml_files: &[UserBamlFile],
    baml_bytecode: &[u8],
) -> HashMap<PathBuf, String> {
    to_source_code_with_optional_metadata(pool, user_baml_files, baml_bytecode, None)
}

pub fn to_source_code_with_bytecode_and_metadata(
    pool: &SymbolPool,
    user_baml_files: &[UserBamlFile],
    baml_bytecode: &[u8],
    embedded_baml_toml: &str,
) -> HashMap<PathBuf, String> {
    to_source_code_with_optional_metadata(
        pool,
        user_baml_files,
        baml_bytecode,
        Some(embedded_baml_toml),
    )
}

fn to_source_code_with_optional_metadata(
    pool: &SymbolPool,
    user_baml_files: &[UserBamlFile],
    baml_bytecode: &[u8],
    embedded_baml_toml: Option<&str>,
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
        if skip_symbol(name) {
            continue;
        }
        if let Symbol::Enum(enum_def) = &pool[*name] {
            enums.push(emit_enum(&names, name, enum_def));
            emitted_types.insert((*name).clone());
        }
    }

    // Pass 2: classes, recursive-alias wrapper structs, and `using`
    // aliases, to a fixed point so declaration dependencies resolve in
    // emission (= declaration) order.
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
            if skip_symbol(name) {
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
            // Only forward-declarable members may be assumed available: a
            // struct (and a recursive alias's wrapper struct) can be named
            // through `baml::box` ahead of its definition. A NON-recursive
            // alias emits a `using`, which cannot be forward-declared, so it
            // has to be declared before anything that names it. Seeding it
            // here let a struct that names it win `cycle_set`'s alphabetical
            // order (`ChatMessage` sorts before `ChatMessageContent`) and
            // emit a use of an undeclared alias. Instead the plain aliases
            // stay unavailable and the inner loop below settles the real
            // order.
            for name in &cycle_set {
                if is_plain_alias(pool, name) {
                    emitted_types.remove(name);
                } else {
                    emitted_types.insert(name.clone());
                }
            }
            let mut round = Vec::new();
            let mut failed = Vec::new();
            // Inner fixed point: `using` declarations (and whatever names
            // them) land in dependency order rather than name order.
            let mut cycle_pending: Vec<&Name> = cycle_set.iter().collect();
            loop {
                let mut progressed = false;
                let mut still_pending = Vec::new();
                for name in cycle_pending {
                    match emit_type(name, &emitted_types, &cycle_set) {
                        Ok(Some(emitted)) => {
                            round.push(emitted);
                            emitted_types.insert(name.clone());
                            progressed = true;
                        }
                        Ok(None) => still_pending.push(name),
                        Err(_) => {
                            failed.push(name.clone());
                            progressed = true;
                        }
                    }
                }
                cycle_pending = still_pending;
                if cycle_pending.is_empty() || !progressed {
                    break;
                }
            }
            failed.extend(cycle_pending.into_iter().cloned());
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

    // Pass 3: methods, against the final emitted type set. Structs may be
    // referenced before their definitions thanks to the forward-declaration
    // block, but aliases cannot be forward-declared and standard-library
    // containers generally require their element types to be complete.
    //
    // Track definitions in the exact order render_header uses. A method that
    // would store an incomplete type in its nested opts struct (or name a
    // later alias) is omitted rather than making the entire generated SDK
    // ill-formed.
    let mut complete_types = BTreeSet::new();
    for emitted in &mut classes {
        let EmittedType::Class(class) = emitted else {
            let EmittedType::Using(alias) = emitted else {
                unreachable!()
            };
            complete_types.insert(alias.pool_name.clone());
            continue;
        };
        if class.alias_wrapper {
            complete_types.insert(class.pool_name.clone());
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
                if !method.generic_params.is_empty() {
                    skipped.push(format!(
                        "{}.{}: generic method (post-step-8)",
                        class.pool_name, method.name
                    ));
                    continue;
                }
                if let Some(reason) = in_class_declaration_issue(pool, method, &complete_types) {
                    skipped.push(format!("{}.{}: {reason}", class.pool_name, method.name));
                    continue;
                }
                let fqn = BamlFqn::member(&class.pool_name, method.name.as_str());
                match emit_callable(
                    pool,
                    &names,
                    &fqn,
                    CppNameKind::Method,
                    method,
                    &emitted_types,
                ) {
                    Ok(emitted_fn) => {
                        if is_static {
                            class.static_methods.push(emitted_fn);
                        } else {
                            class.instance_methods.push(emitted_fn);
                        }
                    }
                    Err(reason) => {
                        skipped.push(format!("{}.{}: {reason}", class.pool_name, method.name));
                    }
                }
            }
        }
        complete_types.insert(class.pool_name.clone());
    }

    // Pass 4: free functions over the emitted type set.
    let mut fns_by_namespace: BTreeMap<Vec<String>, Vec<EmittedFn>> = BTreeMap::new();
    for name in &pool_names {
        let Symbol::Function(function) = &pool[*name] else {
            continue;
        };
        if skip_symbol(name) {
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
            CppNameKind::Function,
            function,
            &emitted_types,
        ) {
            Ok(emitted) => {
                let ns = allocated_namespace(&names, name);
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
        render_inlinedbaml(user_baml_files, baml_bytecode, embedded_baml_toml),
    );
    out.insert(PathBuf::from("CMakeLists.txt"), CMAKE_LISTS.to_string());
    for (rel, content) in BRIDGE_HEADERS {
        out.insert(PathBuf::from(rel), (*content).to_string());
    }
    for (rel, gz) in PB_SOURCES {
        out.insert(PathBuf::from(rel), gunzip(rel, gz));
    }
    out
}

/// `CMake` integration: consumers `add_subdirectory(baml_sdk)` and link
/// `baml::sdk`. Static content -- the generated source layout never varies.
/// `CMake` is optional; any build system that passes one include path and
/// compiles the two sources works (see `sdks/cpp/README.md`).
const CMAKE_LISTS: &str = "\
# Generated by sdkgen_cpp - do not edit.
# Usage: add_subdirectory(baml_sdk) then target_link_libraries(app baml::sdk).
cmake_minimum_required(VERSION 3.16)
project(baml_sdk LANGUAGES CXX)

# Pinned protobuf-lite runtime; must match the vendored pb sources under
# src/pb/. See cmake/fetch_protobuf.cmake for offline/dev overrides.
include(${CMAKE_CURRENT_SOURCE_DIR}/cmake/fetch_protobuf.cmake)

add_library(baml_sdk STATIC
  src/bindings.cc
  src/_inlinedbaml.cc
  src/pb/baml_handle.pb.cc
  src/pb/baml_inbound.pb.cc
  src/pb/baml_outbound.pb.cc
  src/pb/baml_type.pb.cc
)
add_library(baml::sdk ALIAS baml_sdk)

target_include_directories(baml_sdk PUBLIC ${CMAKE_CURRENT_SOURCE_DIR}/include)
target_compile_features(baml_sdk PUBLIC cxx_std_17)

# The bridge dlopens the shared BAML runtime at first use and waits on call
# envelopes; the only link-time dependencies are the pinned protobuf-lite
# runtime, the platform loader, and threads.
find_package(Threads REQUIRED)
target_link_libraries(baml_sdk
  PUBLIC protobuf::libprotobuf-lite Threads::Threads ${CMAKE_DL_LIBS})
";

/// The generated protobuf sources for the CFFI wire schema, vendored into
/// every generated SDK. Embedded gzipped (see build.rs): the plain text is
/// ~1.9 MB and `baml-cli` carries every generator under a size gate.
const PB_SOURCES: &[(&str, &[u8])] = &[
    (
        "include/baml_bridge/cffi/v1/baml_handle.pb.h",
        include_bytes!(concat!(env!("OUT_DIR"), "/baml_handle.pb.h.gz")),
    ),
    (
        "include/baml_bridge/cffi/v1/baml_inbound.pb.h",
        include_bytes!(concat!(env!("OUT_DIR"), "/baml_inbound.pb.h.gz")),
    ),
    (
        "include/baml_bridge/cffi/v1/baml_outbound.pb.h",
        include_bytes!(concat!(env!("OUT_DIR"), "/baml_outbound.pb.h.gz")),
    ),
    (
        "include/baml_bridge/cffi/v1/baml_type.pb.h",
        include_bytes!(concat!(env!("OUT_DIR"), "/baml_type.pb.h.gz")),
    ),
    (
        "src/pb/baml_handle.pb.cc",
        include_bytes!(concat!(env!("OUT_DIR"), "/baml_handle.pb.cc.gz")),
    ),
    (
        "src/pb/baml_inbound.pb.cc",
        include_bytes!(concat!(env!("OUT_DIR"), "/baml_inbound.pb.cc.gz")),
    ),
    (
        "src/pb/baml_outbound.pb.cc",
        include_bytes!(concat!(env!("OUT_DIR"), "/baml_outbound.pb.cc.gz")),
    ),
    (
        "src/pb/baml_type.pb.cc",
        include_bytes!(concat!(env!("OUT_DIR"), "/baml_type.pb.cc.gz")),
    ),
];

fn gunzip(name: &str, gz: &[u8]) -> String {
    use std::io::Read as _;
    let mut out = String::new();
    flate2::read::GzDecoder::new(gz)
        .read_to_string(&mut out)
        .unwrap_or_else(|e| panic!("embedded {name} failed to decompress: {e}"));
    out
}

/// The bridge runtime headers, vendored verbatim into every generated SDK
/// (embedded at emitter build time, so headers and generator are the same
/// version by construction). The generated tree is self-contained source;
/// the only external artifact is the shared runtime library the bridge
/// dlopens at first use.
const BRIDGE_HEADERS: &[(&str, &str)] = &[
    (
        "cmake/fetch_protobuf.cmake",
        include_str!("../../bridge_cpp/cmake/fetch_protobuf.cmake"),
    ),
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
        "include/baml/future.h",
        include_str!("../../bridge_cpp/include/baml/future.h"),
    ),
    (
        "include/baml/lit.h",
        include_str!("../../bridge_cpp/include/baml/lit.h"),
    ),
    (
        "include/baml/runtime.h",
        include_str!("../../bridge_cpp/include/baml/runtime.h"),
    ),
    (
        "include/baml/version.h",
        include_str!("../../bridge_cpp/include/baml/version.h"),
    ),
    (
        "include/baml/variant.h",
        include_str!("../../bridge_cpp/include/baml/variant.h"),
    ),
    (
        "include/baml/detail/call.h",
        include_str!("../../bridge_cpp/include/baml/detail/call.h"),
    ),
    (
        "include/baml/detail/host_value.h",
        include_str!("../../bridge_cpp/include/baml/detail/host_value.h"),
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
];

// ---------------------------------------------------------------------------
// Name requests
// ---------------------------------------------------------------------------

/// Member name of the synthesized per-callable opts struct within its
/// callable's identity.
const OPTS_MEMBER: &str = "opts";

/// Member name of the synthesized async sibling within its callable's
/// identity.
const ASYNC_MEMBER: &str = "async";

/// One typed request per identifier any emit pass may need. Mirrors the
/// pool-level skip filters (`pkg`, `$stream`, `$` companions); symbols that
/// only emission can rule out (unsupported field types, broken cycles) still
/// get allocations, which are simply never rendered.
fn collect_requests(pool: &SymbolPool) -> BTreeSet<NameRequest> {
    let mut requests = BTreeSet::new();
    for (name, symbol) in pool {
        // Post-step-8 symbols never allocate names.
        if skip_symbol(name) {
            continue;
        }
        match symbol {
            Symbol::Enum(enum_def) => {
                request_namespace_segments(&mut requests, name);
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
                request_namespace_segments(&mut requests, name);
                requests.insert(NameRequest::new(BamlFqn::symbol(name), CppNameKind::Class));
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
                    if !method.generic_params.is_empty() {
                        continue; // generic methods (post-step-8)
                    }
                    let method_fqn = BamlFqn::member(name, method.name.as_str());
                    requests.insert(NameRequest::new(method_fqn.clone(), CppNameKind::Method));
                    requests.insert(async_request(&method_fqn, method));
                    request_callable_members(&mut requests, &method_fqn, method);
                }
            }
            Symbol::Function(function) => {
                if !function.generic_params.is_empty() {
                    continue; // generics disabled (post-step-8)
                }
                request_namespace_segments(&mut requests, name);
                let fqn = BamlFqn::symbol(name);
                requests.insert(function_request(name));
                requests.insert(async_request(&fqn, function));
                request_callable_members(&mut requests, &fqn, function);
            }
            // Non-recursive aliases emit a `using` declaration; recursive
            // aliases emit a named wrapper struct that breaks the type
            // recursion (an alias-declaration cannot reference itself).
            Symbol::TypeAlias(alias) => {
                request_namespace_segments(&mut requests, name);
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
/// path (`baml`/`vendor/<pkg>` prefixes included) and are
/// anchored in the `user` package so identical C++ scopes dedupe across
/// packages.
fn request_namespace_segments(requests: &mut BTreeSet<NameRequest>, name: &Name) {
    let segments = naming::source_ns(name);
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

/// Parameters and (when optional parameters exist) the synthesized opts
/// struct + setters of one callable.
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

/// Post-step-8 symbols this slice never emits: `$stream` companions and
/// `$`-suffixed companion functions.
fn skip_symbol(name: &Name) -> bool {
    name.is_stream() || name.bare_name().contains('$')
}

/// Whether `name` is a NON-recursive type alias, i.e. one that emits a
/// `using` declaration. Unlike a struct (or a recursive alias's wrapper
/// struct) a `using` cannot be forward-declared, so every use of it must
/// follow its declaration.
fn is_plain_alias(pool: &SymbolPool, name: &Name) -> bool {
    matches!(pool.get(name), Some(Symbol::TypeAlias(alias)) if !alias.recursive)
}

/// The name request for a free function. Shared between collection and
/// emission so the lookup key cannot drift.
fn function_request(name: &Name) -> NameRequest {
    NameRequest::new(BamlFqn::symbol(name), CppNameKind::Function)
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
        &format!("{}_opts", function.name.as_str()),
    )
}

/// The request for a callable's async sibling. Shared between collection
/// and emission so the lookup key cannot drift. Verbatim source spelling +
/// "Async" (probe -> probeAsync), following the opts-struct convention:
/// user names are never re-cased, only suffixed.
fn async_request(callable: &BamlFqn, function: &Function) -> NameRequest {
    NameRequest::synthesized(
        callable.child(ASYNC_MEMBER),
        CppNameKind::Function,
        &format!("{}_async", function.name.as_str()),
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
fn allocated_namespace(names: &CppNames, name: &Name) -> Vec<String> {
    let source: Vec<Box<str>> = naming::source_ns(name);
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
        ns: allocated_namespace(names, name),
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
    /// The pool symbol this class was emitted from (pass 3 looks methods
    /// back up by it).
    pool_name: Name,
    ns: Vec<String>,
    name: CppName,
    doc: Option<String>,
    fields: Vec<EmittedField>,
    static_methods: Vec<EmittedFn>,
    instance_methods: Vec<EmittedFn>,
    /// A recursive-alias wrapper struct: one `value` field holding the
    /// alias's resolved type, structural codec (aliases have no wire
    /// identity), no methods.
    alias_wrapper: bool,
}

impl EmittedClass {
    /// The receiver's C++ spelling inside its own scope (no templates this
    /// slice, so just the bare class name).
    fn self_type(&self) -> &str {
        self.name.declared()
    }
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
    // Method optional params become arg<T> fields on opts structs nested in
    // this class's body, so their types must be complete (= defined
    // earlier) too. Delay the class while such a dep is merely
    // not-yet-emitted; once the cycle pass runs (boxed non-empty), stop
    // blocking -- a dep that still cannot resolve there means pass 3 skips
    // that method, so no opts struct references it.
    if boxed.is_empty() {
        for method in class_def
            .static_methods
            .iter()
            .chain(&class_def.instance_methods)
        {
            for arg in method.arguments.iter().filter(|a| a.default.is_some()) {
                if let Translated::NotYet = translate_ty(pool, names, &arg.ty, emitted_types, boxed)
                {
                    return Ok(None);
                }
            }
        }
    }
    Ok(Some(EmittedClass {
        pool_name: name.clone(),
        ns: allocated_namespace(names, name),
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
    let inner = match translate_ty(pool, names, &alias.resolves_to, emitted_types, boxed) {
        Translated::Cpp(ty) => ty,
        Translated::NotYet => return Ok(None),
        Translated::Unsupported(reason) => {
            return Err(format!("aliased type: {reason}"));
        }
    };
    Ok(Some(EmittedClass {
        pool_name: name.clone(),
        ns: allocated_namespace(names, name),
        name: names
            .get(&NameRequest::new(BamlFqn::symbol(name), CppNameKind::Class))
            .clone(),
        doc: None,
        fields: vec![EmittedField {
            name: names.get(&alias_value_field_request(name)).clone(),
            ty: inner,
        }],
        static_methods: Vec::new(),
        instance_methods: Vec::new(),
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
/// synonym, so no codec is emitted: `codec<Alias>` *is* `codec<Target>`.
struct EmittedUsing {
    pool_name: Name,
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
        pool_name: name.clone(),
        ns: allocated_namespace(names, name),
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
// Callables (free functions)
// ---------------------------------------------------------------------------

struct EmittedParam {
    name: CppName,
    ty: String,
    /// For callable-typed parameters: the callable's declared BAML param
    /// names in declared order ("" for unnamed required params). Switches
    /// the binding to `encode_callable` (host-callable registration) and
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
    /// The async sibling's allocated name (`{name}Async`).
    async_name: CppName,
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
    /// The declared throws set as a `::baml::variant<...>` spelling, when
    /// every member translates; `None` uses the untyped `error` path.
    thrown: Option<String>,
}

fn emit_callable(
    pool: &SymbolPool,
    names: &CppNames,
    fqn: &BamlFqn,
    kind: CppNameKind,
    function: &Function,
    emitted_types: &BTreeSet<Name>,
) -> Result<EmittedFn, String> {
    let name = names.get(&NameRequest::new(fqn.clone(), kind)).clone();

    let mut params = Vec::new();
    let mut opt_params = Vec::new();
    for arg in &function.arguments {
        // Top-level callable parameters cross as host callables
        // (std::function); callables nested in other types stay
        // unsupported (translate_ty rejects them).
        let mut callable_names = None;
        let ty = if let Ty::Function {
            params: callable_params,
            ret,
            ..
        } = &arg.ty
        {
            if arg.default.is_some() {
                return Err(format!(
                    "optional argument `{}` has a callable type (unsupported)",
                    arg.name
                ));
            }
            match translate_callable_ty(pool, names, callable_params, ret, emitted_types) {
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
            match translate_ty(pool, names, &arg.ty, emitted_types, &BTreeSet::new()) {
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
    let ret = match &function.return_type {
        Ty::Void { .. } => "void".to_string(),
        Ty::Function { params, ret, .. } => {
            translate_callable_ty(pool, names, params, ret, emitted_types)?.0
        }
        return_type => {
            match translate_ty(pool, names, return_type, emitted_types, &BTreeSet::new()) {
                Translated::Cpp(ty) => ty,
                Translated::NotYet | Translated::Unsupported(_) => {
                    return Err(format!("unsupported return type {}", function.return_type));
                }
            }
        }
    };

    let raises = match &function.throws {
        None => Vec::new(),
        Some(Ty::Union(items, _)) => items.iter().map(unqualified_leaf_name).collect(),
        Some(ty) => vec![unqualified_leaf_name(ty)],
    };

    // The declared throws set as a C++ type for the typed error path:
    // always spelled as a ::baml::variant (a single thrown type wraps into a
    // one-alternative Union) so every catch site reads uniformly via
    // baml::match. A throws set this slice cannot translate falls back to
    // the untyped untyped error path (None -> call_sync's ThrownU = void).
    let thrown = function.throws.as_ref().and_then(|ty| {
        match translate_ty(pool, names, ty, emitted_types, &BTreeSet::new()) {
            Translated::Cpp(t) => {
                if t.starts_with("::baml::variant<") {
                    Some(t)
                } else {
                    Some(format!("::baml::variant<{t}>"))
                }
            }
            Translated::NotYet | Translated::Unsupported(_) => None,
        }
    });

    let opts_name = if opt_params.is_empty() {
        None
    } else {
        Some(names.get(&opts_request(fqn, function)).clone())
    };

    // The runtime dispatches on the BAML FQN: for methods that is the
    // class's wire symbol plus the method's source member token, never a
    // C++ name.
    let call_fqn = if kind == CppNameKind::Method {
        let member = fqn.members.last().expect("method identity has a member");
        format!("{}.{member}", name.wire())
    } else {
        name.wire().to_string()
    };

    Ok(EmittedFn {
        call_fqn,
        name,
        async_name: names.get(&async_request(fqn, function)).clone(),
        ret,
        params,
        opt_params,
        opts_name,
        doc: function.docstring.clone(),
        raises,
        thrown,
    })
}

/// A callable-typed parameter as `std::function<Ret(Slots...)>` plus its
/// declared BAML param names ("" for unnamed). Optional callable params
/// (`y?: int`) become `arg` slots, so an argument BAML omits materializes
/// as an unset `arg` and the host's own default applies.
fn translate_callable_ty(
    pool: &SymbolPool,
    names: &CppNames,
    callable_params: &[CallableParam],
    ret: &Ty,
    emitted_types: &BTreeSet<Name>,
) -> Result<(String, Vec<String>), String> {
    let mut slots = Vec::new();
    let mut wire_names = Vec::new();
    for p in callable_params {
        let slot = match translate_ty(pool, names, &p.ty, emitted_types, &BTreeSet::new()) {
            Translated::Cpp(t) => t,
            Translated::NotYet | Translated::Unsupported(_) => {
                return Err(format!("callable param type {}", p.ty));
            }
        };
        slots.push(match p.mode {
            CodegenFunctionParamMode::Required => slot,
            CodegenFunctionParamMode::Optional => format!("::baml::arg<{slot}>"),
        });
        wire_names.push(
            p.name
                .as_ref()
                .map(|n| n.as_str().to_string())
                .unwrap_or_default(),
        );
    }
    let ret_ty = if matches!(ret, Ty::Void { .. }) {
        "void".to_string()
    } else {
        match translate_ty(pool, names, ret, emitted_types, &BTreeSet::new()) {
            Translated::Cpp(t) => t,
            Translated::NotYet | Translated::Unsupported(_) => {
                return Err(format!("callable return type {ret}"));
            }
        }
    };
    Ok((
        format!("std::function<{ret_ty}({})>", slots.join(", ")),
        wire_names,
    ))
}

fn unqualified_leaf_name(ty: &Ty) -> String {
    match ty {
        Ty::Class(name, ..) | Ty::Enum(name, _) | Ty::TypeAlias(name, _) => {
            name.bare_name().to_string()
        }
        other => other.to_string(),
    }
}

/// Returns why `function` cannot be declared inside a class at its current
/// point in the generated header.
///
/// Ordinary parameters and returns only declare functions; their bodies and
/// codec instantiations are emitted after every class definition. Optional
/// arguments are different: each is stored as `arg<T>` in an in-class opts
/// struct, so completeness-sensitive outer types must be rejected there.
/// C++ `using` aliases also have no forward-declaration syntax in any context.
fn in_class_declaration_issue(
    pool: &SymbolPool,
    function: &Function,
    complete_types: &BTreeSet<Name>,
) -> Option<String> {
    for arg in &function.arguments {
        if let Some(reason) = undeclared_alias_issue(pool, &arg.ty, complete_types) {
            return Some(format!("argument `{}` {reason}", arg.name));
        }
        if arg.default.is_some()
            && let Some(reason) = incomplete_stored_type_issue(pool, &arg.ty, complete_types)
        {
            return Some(format!("optional argument `{}` {reason}", arg.name));
        }
    }
    if let Some(reason) = undeclared_alias_issue(pool, &function.return_type, complete_types) {
        return Some(format!("return type {reason}"));
    }
    if let Some(throws) = &function.throws {
        if let Some(reason) = undeclared_alias_issue(pool, throws, complete_types) {
            return Some(format!("throws type {reason}"));
        }
    }
    None
}

fn undeclared_alias_issue(
    pool: &SymbolPool,
    ty: &Ty,
    complete_types: &BTreeSet<Name>,
) -> Option<String> {
    match ty {
        Ty::Class(_, args, _) => args
            .iter()
            .find_map(|arg| undeclared_alias_issue(pool, arg, complete_types)),
        Ty::TypeAlias(name, _) => {
            let forward_declarable = matches!(
                pool.get(name),
                Some(Symbol::TypeAlias(alias)) if alias.recursive
            );
            if !complete_types.contains(name) && !forward_declarable {
                Some(format!("references alias `{name}` before its declaration"))
            } else {
                None
            }
        }
        Ty::List(item, _) => undeclared_alias_issue(pool, item, complete_types),
        Ty::Map { key, value, .. } => undeclared_alias_issue(pool, key, complete_types)
            .or_else(|| undeclared_alias_issue(pool, value, complete_types)),
        Ty::Union(items, _) => items
            .iter()
            .find_map(|item| undeclared_alias_issue(pool, item, complete_types)),
        Ty::Function {
            params,
            ret,
            throws,
            ..
        } => params
            .iter()
            .find_map(|param| undeclared_alias_issue(pool, &param.ty, complete_types))
            .or_else(|| undeclared_alias_issue(pool, ret, complete_types))
            .or_else(|| undeclared_alias_issue(pool, throws, complete_types)),
        Ty::Future(value, throws, _) => undeclared_alias_issue(pool, value, complete_types)
            .or_else(|| undeclared_alias_issue(pool, throws, complete_types)),
        _ => None,
    }
}

/// Returns the incomplete nominal type that prevents `ty` from being stored
/// as `arg<ty>` in an in-class opts struct.
///
/// Propagating through every BAML container is deliberately conservative
/// across libstdc++, libc++, and MSVC: opts structs are concrete data members,
/// not mere function declarations, and must be valid at the point rendered.
fn incomplete_stored_type_issue(
    pool: &SymbolPool,
    ty: &Ty,
    complete_types: &BTreeSet<Name>,
) -> Option<String> {
    match ty {
        Ty::Class(name, _, _) if !complete_types.contains(name) => {
            Some(format!("stores incomplete class `{name}`"))
        }
        Ty::TypeAlias(name, _)
            if matches!(
                pool.get(name),
                Some(Symbol::TypeAlias(alias)) if alias.recursive
            ) && !complete_types.contains(name) =>
        {
            Some(format!("stores incomplete recursive alias `{name}`"))
        }
        Ty::List(item, _) => incomplete_stored_type_issue(pool, item, complete_types),
        Ty::Map { key, value, .. } => incomplete_stored_type_issue(pool, key, complete_types)
            .or_else(|| incomplete_stored_type_issue(pool, value, complete_types)),
        Ty::Union(items, _) => items
            .iter()
            .filter(|item| !matches!(item, Ty::Null { .. }))
            .find_map(|item| incomplete_stored_type_issue(pool, item, complete_types)),
        _ => None,
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

/// The step-8 type table: primitives, containers, null-normalized
/// optionals, emitted classes/enums, named aliases, and boxed cycle
/// references. Everything else is unsupported here and the surrounding
/// symbol is skipped (reported, not silently dropped).
fn translate_ty(
    pool: &SymbolPool,
    names: &CppNames,
    ty: &Ty,
    emitted_types: &BTreeSet<Name>,
    boxed: &BTreeSet<Name>,
) -> Translated {
    let translated = match ty {
        Ty::Int { .. } => "int64_t".to_string(),
        Ty::Float { .. } => "double".to_string(),
        Ty::String { .. } => "std::string".to_string(),
        Ty::Bool { .. } => "bool".to_string(),
        Ty::Null { .. } => "std::monostate".to_string(),
        Ty::Uint8Array { .. } => "std::vector<uint8_t>".to_string(),
        Ty::RustType { .. } => {
            return Translated::Unsupported("handle type (post-step-8)".to_string());
        }
        Ty::Bigint { .. } => {
            return Translated::Unsupported("bigint (post-step-8)".to_string());
        }
        Ty::Media(..) => {
            return Translated::Unsupported("media type (post-step-8)".to_string());
        }
        Ty::Literal(lit, ..) => {
            // Literal types are singleton ::baml::lit types (each distinct
            // value a distinct C++ type), spelled as char packs / typed
            // scalars directly -- the BAML_LIT macro family is user-side
            // sugar only. Float literals stay widened: float NTTPs are
            // C++20 and BAML has no float literal types in practice.
            match lit {
                baml_base::Literal::Int(v) => format!("::baml::lit<{}>", lit_int_spelling(*v)),
                baml_base::Literal::Bigint(_) => {
                    return Translated::Unsupported("bigint literal (post-step-8)".to_string());
                }
                baml_base::Literal::Float(_) => "double".to_string(),
                baml_base::Literal::String(s) => {
                    let chars: Vec<String> = s.bytes().map(lit_char_spelling).collect();
                    format!("::baml::lit<{}>", chars.join(", "))
                }
                baml_base::Literal::Bool(b) => format!("::baml::lit<{b}>"),
            }
        }
        Ty::TypeVar(name, _) => {
            return Translated::Unsupported(format!("TypeVar {name} (generics post-step-8)"));
        }
        Ty::TypeAlias(name, _) => {
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
                    return Translated::Cpp(format!("::baml::box<{base}>"));
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
        Ty::Enum(name, _) => {
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
        // An enum-variant type (`Sentiment.Positive`) is a singleton Lit
        // over the enum value, so unions of variants dispatch and match
        // per-variant at compile time.
        Ty::EnumVariant(name, variant, _) => {
            return if emitted_types.contains(name) {
                let enum_path = names
                    .get(&NameRequest::new(BamlFqn::symbol(name), CppNameKind::Enum))
                    .identifier()
                    .to_string();
                let variant_name = names
                    .get(&NameRequest::new(
                        BamlFqn::member(name, variant.as_str()),
                        CppNameKind::EnumVariant,
                    ))
                    .declared();
                Translated::Cpp(format!("::baml::lit<{enum_path}::{variant_name}>"))
            } else {
                Translated::NotYet
            };
        }
        Ty::Class(name, args, _) => {
            let wire_name = name.to_string();
            if wire_name == "ai.Prompt" && args.is_empty() {
                return Translated::Cpp("::baml::prompt".to_string());
            }
            if wire_name == "ai.FunctionSpec" && args.len() == 1 {
                let output = match translate_ty(pool, names, &args[0], emitted_types, boxed) {
                    Translated::Cpp(ty) => ty,
                    other => return other,
                };
                return Translated::Cpp(format!("::baml::function_spec<{output}>"));
            }
            if wire_name == "ai.stream.Stream" && args.len() == 2 {
                let partial = match translate_ty(pool, names, &args[0], emitted_types, boxed) {
                    Translated::Cpp(ty) => ty,
                    other => return other,
                };
                let output = match translate_ty(pool, names, &args[1], emitted_types, boxed) {
                    Translated::Cpp(ty) => ty,
                    other => return other,
                };
                return Translated::Cpp(format!("::baml::stream<{partial}, {output}>"));
            }
            // Type args occur only on generic-class instantiations, and
            // generic classes never emit this slice.
            if !args.is_empty() {
                return Translated::Unsupported(
                    "generic class instantiation (post-step-8)".to_string(),
                );
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
            if boxed.contains(name) {
                return Translated::Cpp(format!("::baml::box<{base}>"));
            }
            return Translated::Cpp(base);
        }
        Ty::List(inner, _) => match translate_ty(pool, names, inner, emitted_types, boxed) {
            Translated::Cpp(inner) => format!("std::vector<{inner}>"),
            other => return other,
        },
        Ty::Map { key, value, .. } => {
            if !matches!(key.as_ref(), Ty::String { .. }) {
                return Translated::Unsupported("non-string map key".to_string());
            }
            match translate_ty(pool, names, value, emitted_types, boxed) {
                Translated::Cpp(value) => format!("std::unordered_map<std::string, {value}>"),
                other => return other,
            }
        }
        Ty::Function { .. } => {
            return Translated::Unsupported("nested callable type".to_string());
        }
        Ty::Union(items, _) => {
            // Null-normalization (spec D-unions v2): strip the null member,
            // dedup alternatives that map to the same C++ type, emit a
            // variant (or the bare type when one alternative remains), and
            // wrap in optional when null was a member.
            let non_null: Vec<&Ty> = items
                .iter()
                .filter(|t| !matches!(t, Ty::Null { .. }))
                .collect();
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
            // Multi-member unions spell ::baml::variant<...>, an
            // order-canonical std::variant alias: the C++ type system
            // dedups spellings (Union<A, B> == Union<B, A>), so
            // declaration order is fine here; sorting the rendered text
            // just keeps regenerated headers byte-stable.
            alternatives.sort();
            let inner = match alternatives.as_slice() {
                [] => return Translated::Unsupported("empty union".to_string()),
                [single] => single.clone(),
                many => format!("::baml::variant<{}>", many.join(", ")),
            };
            if had_null {
                // A nullable boxed recursive edge cannot be optional<Box<T>>
                // (std::optional needs a complete T at instantiation);
                // OptionalBox folds the null into the box itself.
                if let Some(boxed_inner) = inner
                    .strip_prefix("::baml::box<")
                    .and_then(|rest| rest.strip_suffix('>'))
                {
                    format!("::baml::optional_box<{boxed_inner}>")
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

// ---------------------------------------------------------------------------
// Rendering
// ---------------------------------------------------------------------------

/// How a callable renders: a free-function declaration (defaulted opts
/// parameter) or definition, an in-class method declaration, or an
/// out-of-line method definition in bindings.cc (owner-qualified, no
/// default repeated).
#[derive(Clone, Copy)]
enum RenderPos<'a> {
    Decl,
    Def,
    StaticDecl,
    InstanceDecl,
    StaticDef { class: &'a EmittedClass },
    InstanceDef { class: &'a EmittedClass },
}

/// Which spelling of a callable renders: the synchronous form or the
/// `{name}Async` sibling returning a `baml::Future`.
#[derive(Clone, Copy)]
enum FnVariant {
    Sync,
    Async,
}

const FN_VARIANTS: [FnVariant; 2] = [FnVariant::Sync, FnVariant::Async];

/// The async sibling's return type: the sync return wrapped in
/// `baml::Future`, with the declared throws union as the second parameter
/// when the function has a typed throws set.
fn future_ret(f: &EmittedFn) -> String {
    match &f.thrown {
        Some(u) => format!("::baml::future<{}, {}>", f.ret, u),
        None => format!("::baml::future<{}>", f.ret),
    }
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
    // Lit types are empty unit structs: by value.
    if ty.starts_with("::baml::lit<") {
        return ty.to_string();
    }
    match ty {
        "int64_t" | "double" | "bool" | "std::monostate" => ty.to_string(),
        _ => format!("const {ty}&"),
    }
}

/// Spells one byte of a BAML string literal as a C++ char literal for a
/// `::baml::lit` char pack. Bytes, not code points: the pack mirrors the
/// literal's UTF-8 encoding, matching what `BAML_LIT`'s sizeof-based
/// expansion produces.
fn lit_char_spelling(b: u8) -> String {
    match b {
        b'\'' => "'\\''".to_string(),
        b'\\' => "'\\\\'".to_string(),
        b'\n' => "'\\n'".to_string(),
        b'\r' => "'\\r'".to_string(),
        b'\t' => "'\\t'".to_string(),
        0x20..=0x7e => format!("'{}'", b as char),
        _ => format!("'\\x{b:02x}'"),
    }
}

/// Spells an int literal's value as the canonical `int64_t{...}` template
/// argument. `i64::MIN` has no valid literal spelling (the unary minus
/// applies after the out-of-range positive literal), hence the subtraction.
fn lit_int_spelling(v: i64) -> String {
    if v == i64::MIN {
        return "int64_t{-9223372036854775807 - 1}".to_string();
    }
    format!("int64_t{{{v}}}")
}

fn signature(f: &EmittedFn, pos: RenderPos, variant: FnVariant) -> String {
    let mut params: Vec<String> = f
        .params
        .iter()
        .map(|p| format!("{} {}", by_value_or_cref(&p.ty), p.name.declared()))
        .collect();
    if let Some(opts_name) = &f.opts_name {
        let default = match pos {
            RenderPos::Def | RenderPos::StaticDef { .. } | RenderPos::InstanceDef { .. } => "",
            RenderPos::Decl | RenderPos::StaticDecl | RenderPos::InstanceDecl => " = {}",
        };
        // Out-of-line method definitions must qualify the nested opts type.
        let opts_ty = match pos {
            RenderPos::StaticDef { class } | RenderPos::InstanceDef { class } => {
                format!("{}::{}", class.self_type(), opts_name.declared())
            }
            _ => opts_name.declared().to_string(),
        };
        params.push(format!(
            "{opts_ty} {opts}{default}",
            opts = GeneratorIdent::OptsParam.token()
        ));
    }
    let (ret, name) = match variant {
        FnVariant::Sync => (f.ret.clone(), f.name.declared()),
        FnVariant::Async => (future_ret(f), f.async_name.declared()),
    };
    let prefix = match pos {
        RenderPos::StaticDecl => "static ",
        _ => "",
    };
    let owner = match pos {
        RenderPos::StaticDef { class } | RenderPos::InstanceDef { class } => {
            format!("{}::", class.self_type())
        }
        _ => String::new(),
    };
    let constness = match pos {
        RenderPos::InstanceDecl | RenderPos::InstanceDef { .. } => " const",
        _ => "",
    };
    format!(
        "{prefix}{ret} {owner}{name}({params}){constness}",
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
        let arg_ty = format!("::baml::arg<{}>", p.ty);
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

/// Emits one binding body: runtime init, self (for instance methods),
/// required args, set optional args, then the call (blocking `call_sync`,
/// or `start_call` returning the in-flight `baml::future` for the async
/// sibling). `self_type` is the receiver's C++ spelling for instance
/// methods.
fn render_body(
    buf: &mut String,
    indent: &str,
    f: &EmittedFn,
    variant: FnVariant,
    self_type: Option<&str>,
) {
    let args = GeneratorIdent::ArgsLocal.token();
    let w = GeneratorIdent::WriterParam.token();
    let opts = GeneratorIdent::OptsParam.token();
    let _ = writeln!(
        buf,
        "{indent}::baml_sdk::{detail}::{ensure}();",
        detail = GeneratorIdent::DetailNamespace.token(),
        ensure = GeneratorIdent::EnsureRuntime.token()
    );
    let _ = writeln!(buf, "{indent}::baml::detail::args_encoder {args};");
    if let Some(self_type) = self_type {
        let _ = writeln!(
            buf,
            "{indent}{args}.add_arg(\"self\", [&](::baml::detail::pb::InboundValue& {w}) {{ \
             ::baml::codec<{self_type}>::encode({w}, *this); }});",
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
                "{indent}{args}.add_arg(\"{wire}\", [&](::baml::detail::pb::InboundValue& {w}) {{ \
                 ::baml::detail::encode_callable({w}, {value}, \
                 std::array<std::string, {n}>{{{{{names_array}}}}}); }});",
                wire = p.name.wire(),
                value = p.name.identifier(),
                n = callable_names.len(),
            );
            continue;
        }
        let _ = writeln!(
            buf,
            "{indent}{args}.add_arg(\"{wire}\", [&](::baml::detail::pb::InboundValue& {w}) {{ \
             ::baml::codec<{ty}>::encode({w}, {value}); }});",
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
             {args}.add_arg(\"{wire}\", [&](::baml::detail::pb::InboundValue& {w}) {{ \
             ::baml::codec<{ty}>::encode({w}, {opts}.{field}.value()); }});\n{indent}}}",
            wire = p.name.wire(),
            ty = p.ty
        );
    }
    let thrown = match &f.thrown {
        Some(u) => format!(", {u}"),
        None => String::new(),
    };
    let driver = match variant {
        FnVariant::Sync => "call_sync",
        FnVariant::Async => "start_call",
    };
    let _ = writeln!(
        buf,
        "{indent}return ::baml::detail::{driver}<{ret}{thrown}>(\"{fqn}\", \
         std::move({args}));",
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
        for f in c.static_methods.iter().chain(&c.instance_methods) {
            render_opts_struct(&mut buf, "  ", f);
        }
        // Declarations only; the bodies live out-of-line in bindings.cc.
        for (methods, decl_pos) in [
            (&c.static_methods, RenderPos::StaticDecl),
            (&c.instance_methods, RenderPos::InstanceDecl),
        ] {
            for f in methods {
                for variant in FN_VARIANTS {
                    push_doc(&mut buf, "  ", f.doc.as_ref(), &f.raises);
                    let _ = writeln!(buf, "  {};", signature(f, decl_pos, variant));
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
            for variant in FN_VARIANTS {
                push_doc(&mut buf, "", f.doc.as_ref(), &f.raises);
                let _ = writeln!(buf, "{};", signature(f, RenderPos::Decl, variant));
            }
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

/// codec<T> specializations for the generated enums and classes. Emitted in
/// the header (inline) so they are visible from any translation unit.
fn render_codecs(buf: &mut String, enums: &[EmittedEnum], classes: &[&EmittedClass]) {
    buf.push_str("\nnamespace baml {\n");

    for e in enums {
        let q = e.name.identifier();
        let fqn = e.name.wire();
        let _ = writeln!(
            buf,
            "\ntemplate <>\nstruct codec<{q}> {{\n  \
             static detail::pb::BamlTy baml_ty() {{\n    \
             detail::pb::BamlTy ty;\n    \
             ty.mutable_enum_()->set_name(\"{fqn}\");\n    \
             return ty;\n  }}\n  \
             static void encode(detail::pb::InboundValue& value_msg, {q} v) {{\n    \
             auto* e = value_msg.mutable_enum_value();\n    \
             e->set_name(\"{fqn}\");\n    \
             e->set_value(ToWire(v));\n  }}\n  \
             static {q} decode(const detail::pb::BamlOutboundValue& raw) {{\n    \
             const auto& v = detail::unwrap(raw);\n    \
             if (v.value_case() != detail::pb::BamlOutboundValue::kEnumValue ||\n      \
             (!v.enum_value().name().empty() &&\n       \
             v.enum_value().name() != \"{fqn}\")) {{\n      \
             detail::kind_mismatch(\"enum {fqn}\", v);\n    }}\n    \
             return FromWire(v.enum_value().value());\n  }}",
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
        buf.push_str("    }\n    throw error(\"invalid enum value\");\n  }\n");
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
            "    throw error(\"unknown variant '\" + value + \"' for enum {fqn}\");\n  \
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
            let fqn = c.name.wire();
            buf.push_str("\ntemplate <>\n");
            let _ = writeln!(
                buf,
                "struct codec<{q}> {{\n  \
                 static detail::pb::BamlTy baml_ty() {{\n    \
                   detail::pb::BamlTy ty;\n    \
                   ty.mutable_type_alias()->set_name(\"{fqn}\");\n    \
                   return ty;\n  }}\n  \
                 static void encode(detail::pb::InboundValue& value_msg, const {q}& v) {{\n    \
                 codec<{inner}>::encode(value_msg, v.{field});\n  }}\n  \
                 static {q} decode(const detail::pb::BamlOutboundValue& v) {{\n    \
                 return {q}{{codec<{inner}>::decode(v)}};\n  }}\n}};"
            );
            continue;
        }

        let fqn = c.name.wire();

        buf.push_str("\ntemplate <>\n");
        let _ = writeln!(buf, "struct codec<{q}> {{");
        let _ = writeln!(
            buf,
            "  static detail::pb::BamlTy baml_ty() {{\n    \
             detail::pb::BamlTy ty;\n    \
             ty.mutable_class_ty()->set_name(\"{fqn}\");\n    \
             return ty;\n  }};"
        );
        let _ = writeln!(
            buf,
            "  static void encode(detail::pb::InboundValue& value_msg, const {q}& v) {{\n    \
             auto* cls = value_msg.mutable_class_value();"
        );
        for field in &c.fields {
            let _ = writeln!(
                buf,
                "    {{\n      auto* entry = cls->add_fields();\n      \
                 entry->set_string_key(\"{wire}\");\n      \
                 codec<{ty}>::encode(*entry->mutable_value(), v.{name});\n    }}",
                wire = field.name.wire(),
                ty = field.ty,
                name = field.name.identifier()
            );
        }
        let _ = writeln!(
            buf,
            "    value_msg.mutable_value_type()->mutable_class_ty()->set_name(\"{fqn}\");\n  }}",
        );
        // Decode: strict field mapping (extra field or missing field = error,
        // pydantic extra="forbid" parity), FQN-checked for precise
        // variant-of-class dispatch. Fields land in optional locals so
        // non-default-constructible field types (baml::Box) work.
        let _ = writeln!(
            buf,
            "  static {q} decode(const detail::pb::BamlOutboundValue& raw) {{\n    \
             const auto& v = detail::unwrap(raw);\n    \
             if (v.value_case() != detail::pb::BamlOutboundValue::kClassValue ||\n      \
             (!v.class_value().name().empty() &&\n       \
             v.class_value().name() != \"{fqn}\")) {{\n      \
             detail::kind_mismatch(\"class {fqn}\", v);\n    }}",
        );
        for field in &c.fields {
            let _ = writeln!(
                buf,
                "    std::optional<{ty}> field_{name};",
                ty = field.ty,
                name = field.name.declared()
            );
        }
        buf.push_str("    for (const auto& field : v.class_value().fields()) {\n");
        let mut first = true;
        for field in &c.fields {
            let kw = if first { "if" } else { "} else if" };
            first = false;
            let _ = writeln!(
                buf,
                "      {kw} (field.key() == \"{wire}\") {{\n        \
                 field_{name} = codec<{ty}>::decode(field.value());",
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
            "        throw error(\"unexpected field '\" + field.key() + \"' on {fqn}\");\n      \
             }}\n    }}",
        );
        for field in &c.fields {
            let _ = writeln!(
                buf,
                "    if (!field_{name}.has_value()) {{\n      \
                 throw error(\"missing field '{wire}' on {fqn}\");\n    }}",
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
    types: &[EmittedType],
    fns_by_namespace: &BTreeMap<Vec<String>, Vec<EmittedFn>>,
) -> String {
    let mut buf = String::new();
    buf.push_str(
        "// Generated by sdkgen_cpp - do not edit.\n\
         #include <baml_sdk.h>\n\n\
         #include <utility>\n\n\
         namespace baml_sdk {\n",
    );

    // Out-of-line method definitions, owner-qualified inside the class's
    // namespace.
    for t in types {
        let EmittedType::Class(c) = t else { continue };
        if c.static_methods.is_empty() && c.instance_methods.is_empty() {
            continue;
        }
        buf.push('\n');
        open_namespaces(&mut buf, &c.ns);
        for (methods, is_instance) in [(&c.static_methods, false), (&c.instance_methods, true)] {
            for f in methods {
                let (def_pos, self_type) = if is_instance {
                    (RenderPos::InstanceDef { class: c }, Some(c.self_type()))
                } else {
                    (RenderPos::StaticDef { class: c }, None)
                };
                for variant in FN_VARIANTS {
                    let _ = writeln!(buf, "\n{} {{", signature(f, def_pos, variant));
                    render_body(&mut buf, "  ", f, variant, self_type);
                    buf.push_str("}\n");
                }
            }
        }
        close_namespaces(&mut buf, &c.ns);
    }

    for (ns, fns) in fns_by_namespace {
        buf.push('\n');
        open_namespaces(&mut buf, ns);
        for f in fns {
            for variant in FN_VARIANTS {
                let _ = writeln!(buf, "\n{} {{", signature(f, RenderPos::Def, variant));
                render_body(&mut buf, "  ", f, variant, None);
                buf.push_str("}\n");
            }
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

fn render_inlinedbaml(
    user_baml_files: &[UserBamlFile],
    baml_bytecode: &[u8],
    embedded_baml_toml: Option<&str>,
) -> String {
    let mut buf = String::new();
    buf.push_str(
        "// Generated by sdkgen_cpp - do not edit. Embedded BAML bytecode (the\n\
         // runtime payload), the source file list (reference only), and lazy\n\
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
    for rel_path in user_baml_files {
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
    if let Some(embedded_baml_toml) = embedded_baml_toml {
        buf.push_str("const char kEmbeddedBamlToml[] =\n");
        for line in embedded_baml_toml.as_bytes().chunks(32) {
            buf.push_str("    \"");
            escape_bytecode(&mut buf, line);
            buf.push_str("\"\n");
        }
        buf.push_str("    ;\n");
    }
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
         ::baml::initialize_runtime_from_bytecode(\n        \
         reinterpret_cast<const uint8_t*>(bytecode.data()), bytecode.size(),\n        \
         \"{version}\");\n  \
         }});\n\
         }}\n",
        version = baml_version::CANONICAL_VERSION
    );
    if embedded_baml_toml.is_some() {
        let legacy_call = format!(
            "::baml::initialize_runtime_from_bytecode(\n        reinterpret_cast<const uint8_t*>(bytecode.data()), bytecode.size(),\n        \"{}\");",
            baml_version::CANONICAL_VERSION
        );
        buf = buf.replace(
            &legacy_call,
            "::baml::initialize_runtime_from_bytecode_with_metadata(\n        reinterpret_cast<const uint8_t*>(bytecode.data()), bytecode.size(),\n        kEmbeddedBamlToml);",
        );
    }
    let _ = writeln!(buf, "}}  // namespace {detail}");
    buf.push_str("}  // namespace baml_sdk\n");
    buf
}

#[cfg(test)]
mod bytecode_escape_tests {
    use super::{BRIDGE_HEADERS, escape_bytecode, render_inlinedbaml};

    fn escaped(bytes: &[u8]) -> String {
        let mut out = String::new();
        escape_bytecode(&mut out, bytes);
        out
    }

    #[test]
    fn generated_runtime_embeds_and_validates_the_manifest() {
        let output = render_inlinedbaml(
            &[],
            b"bytecode",
            Some("[__baml_codegen]\nmetadata_version = 1\n"),
        );

        assert!(output.contains("const char kEmbeddedBamlToml[]"));
        assert!(output.contains("initialize_runtime_from_bytecode_with_metadata("));
        assert!(output.contains("kEmbeddedBamlToml);"));
    }

    #[test]
    fn generated_sdk_vendors_the_public_version_header() {
        let version_header = BRIDGE_HEADERS
            .iter()
            .find(|(path, _)| *path == "include/baml/version.h");

        assert!(version_header.is_some());
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

#[cfg(test)]
mod declaration_safety_tests {
    use std::collections::BTreeSet;

    use baml_base::TyAttr;

    use super::{Name, SymbolPool, Ty, incomplete_stored_type_issue, undeclared_alias_issue};

    fn qualified(name: &str) -> Name {
        Name::new(
            baml_base::Name::from("user"),
            vec![],
            baml_base::Name::from(name),
        )
    }

    fn attr() -> TyAttr {
        TyAttr::default()
    }

    #[test]
    fn required_and_return_types_can_name_later_classes() {
        let later = qualified("Later");
        let later_ty = Ty::Class(later, vec![], TyAttr::default());
        let pool = SymbolPool::new();
        let complete = BTreeSet::new();

        assert!(
            undeclared_alias_issue(&pool, &later_ty, &complete).is_none(),
            "ordinary method declarations may use forward-declared classes"
        );
    }

    #[test]
    fn optional_arg_storage_rejects_later_classes_in_every_container() {
        let later = qualified("Later");
        let class = Ty::Class(later.clone(), vec![], TyAttr::default());
        let types = [
            class.clone(),
            Ty::List(Box::new(class.clone()), attr()),
            Ty::Map {
                key: Box::new(Ty::String { attr: attr() }),
                value: Box::new(class.clone()),
                attr: attr(),
            },
            Ty::Union(vec![class.clone(), Ty::String { attr: attr() }], attr()),
            Ty::Union(vec![class, Ty::Null { attr: attr() }], attr()),
        ];
        let pool = SymbolPool::new();
        let mut complete = BTreeSet::new();

        for ty in &types {
            assert_eq!(
                incomplete_stored_type_issue(&pool, ty, &complete),
                Some("stores incomplete class `user.Later`".to_string())
            );
        }

        complete.insert(later);
        for ty in &types {
            assert!(incomplete_stored_type_issue(&pool, ty, &complete).is_none());
        }
    }

    #[test]
    fn aliases_must_precede_in_class_method_declarations() {
        let alias = qualified("PayloadAlias");
        let alias_ty = Ty::TypeAlias(alias.clone(), TyAttr::default());
        let pool = SymbolPool::new();
        let mut complete = BTreeSet::new();

        assert_eq!(
            undeclared_alias_issue(&pool, &alias_ty, &complete),
            Some("references alias `user.PayloadAlias` before its declaration".to_string())
        );

        complete.insert(alias);
        assert!(undeclared_alias_issue(&pool, &alias_ty, &complete).is_none());
    }
}
