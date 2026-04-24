//! Rig-test fixtures.
//!
//! Build-time helpers that construct `SymbolPool`s matching the
//! historical `baml_codegen_tests::fixtures::*` shapes. The deleted
//! crate was test-only; this build-dep replaces just the fixture
//! surface needed by `rig_tests/crates/python_*` so G6 can re-enable
//! those suites without resurrecting the 1200-line DSL builders.

use baml_base::{Literal, Name as BaseName};
use baml_codegen_types::{
    Class, ClassProperty, Enum, EnumVariant, Function, FunctionArgument, Name, Origin, Symbol,
    SymbolPool, Ty, TypeAlias,
};

/// Fetch a fixture by its bare name (the string after `python_` in each
/// rig crate's directory name). Panics on unknown names so the build
/// script fails loudly rather than emitting an empty tree.
pub fn fixture(name: &str) -> SymbolPool {
    match name {
        "empty" => empty(),
        "literal_types" => literal_types(),
        "map_types" => map_types(),
        "mixed_complex_types" => mixed_complex_types(),
        "union_types_extended" => union_types_extended(),
        "full_type_coverage" => full_type_coverage(),
        "semantic_streaming" => semantic_streaming(),
        "packages_and_namespaces" => packages_and_namespaces(),
        "companion_functions" => companion_functions(),
        other => panic!("unknown fixture: {other}"),
    }
}

fn cg_name(pkg: &str, ns: &[&str], n: &str) -> Name {
    Name::new(
        BaseName::new(pkg),
        ns.iter().map(|s| BaseName::new(*s)).collect(),
        BaseName::new(n),
    )
}

fn origin(file: &str, span: u32) -> Origin {
    Origin {
        source_file_path: file.to_string(),
        span_start: span,
    }
}

fn class(name: Name, props: Vec<(&str, Ty)>, file: &str, span: u32) -> (Name, Symbol) {
    let sym = Symbol::Class(Class {
        name: name.clone(),
        docstring: None,
        properties: props
            .into_iter()
            .map(|(n, ty)| ClassProperty {
                name: BaseName::new(n),
                docstring: None,
                ty,
            })
            .collect(),
        origin: origin(file, span),
    });
    (name, sym)
}

#[allow(dead_code)]
fn enum_sym(name: Name, variants: &[&str], file: &str, span: u32) -> (Name, Symbol) {
    let sym = Symbol::Enum(Enum {
        name: name.clone(),
        docstring: None,
        variants: variants
            .iter()
            .map(|v| EnumVariant {
                name: BaseName::new(*v),
                docstring: None,
                value: (*v).to_string(),
            })
            .collect(),
        origin: origin(file, span),
    });
    (name, sym)
}

fn type_alias(
    name: Name,
    resolves_to: Ty,
    recursive: bool,
    file: &str,
    span: u32,
) -> (Name, Symbol) {
    let sym = Symbol::TypeAlias(TypeAlias {
        name: name.clone(),
        resolves_to,
        recursive,
        origin: origin(file, span),
    });
    (name, sym)
}

fn fn_arg(name: &str, ty: Ty) -> FunctionArgument {
    FunctionArgument {
        name: BaseName::new(name),
        docstring: None,
        ty,
    }
}

fn function(
    key: Name,
    bare: &str,
    args: Vec<FunctionArgument>,
    ret: Ty,
    file: &str,
    span: u32,
    companions: Vec<(&str, Vec<FunctionArgument>, Ty)>,
) -> (Name, Symbol) {
    let companions = companions
        .into_iter()
        .map(|(cn, cargs, cret)| {
            (
                cn.to_string(),
                Function {
                    name: BaseName::new(cn),
                    docstring: None,
                    arguments: cargs,
                    return_type: cret,
                    stream_return_type: None,
                    watchers: vec![],
                    companions: vec![],
                    origin: origin(file, span),
                },
            )
        })
        .collect();
    let sym = Symbol::Function(Function {
        name: BaseName::new(bare),
        docstring: None,
        arguments: args,
        return_type: ret,
        stream_return_type: None,
        watchers: vec![],
        companions,
        origin: origin(file, span),
    });
    (key, sym)
}

fn insert_all(pool: &mut SymbolPool, syms: Vec<(Name, Symbol)>) {
    for (k, v) in syms {
        pool.insert(k, v);
    }
}

// ---- fixtures --------------------------------------------------------

fn empty() -> SymbolPool {
    SymbolPool::new()
}

fn literal_types() -> SymbolPool {
    let mut pool = SymbolPool::new();
    let literals = cg_name("user", &[], "Literals");
    insert_all(
        &mut pool,
        vec![class(
            literals,
            vec![
                ("priority_1", Ty::Literal(Literal::String("1".into()))),
                ("priority_2", Ty::Literal(Literal::String("2".into()))),
                ("priority_3", Ty::Literal(Literal::String("3".into()))),
                ("status_draft", Ty::Literal(Literal::String("draft".into()))),
                (
                    "status_published",
                    Ty::Literal(Literal::String("published".into())),
                ),
                ("count", Ty::Literal(Literal::Int(42))),
                ("enabled", Ty::Literal(Literal::Bool(true))),
                ("disabled", Ty::Literal(Literal::Bool(false))),
            ],
            "literals.baml",
            0,
        )],
    );
    pool
}

fn map_types() -> SymbolPool {
    let mut pool = SymbolPool::new();
    let map_container = cg_name("user", &[], "MapContainer");
    let str_ty = || Ty::String;
    let int_ty = || Ty::Int;
    let map_s_i = || Ty::Map {
        key: Box::new(str_ty()),
        value: Box::new(int_ty()),
    };
    let map_s_s = || Ty::Map {
        key: Box::new(str_ty()),
        value: Box::new(str_ty()),
    };
    insert_all(
        &mut pool,
        vec![
            class(
                map_container,
                vec![
                    ("simple", map_s_i()),
                    (
                        "nested",
                        Ty::Map {
                            key: Box::new(str_ty()),
                            value: Box::new(map_s_s()),
                        },
                    ),
                    (
                        "array_val",
                        Ty::Map {
                            key: Box::new(str_ty()),
                            value: Box::new(Ty::List(Box::new(str_ty()))),
                        },
                    ),
                    (
                        "union_val",
                        Ty::Map {
                            key: Box::new(str_ty()),
                            value: Box::new(Ty::Union(vec![str_ty(), int_ty()])),
                        },
                    ),
                ],
                "maps.baml",
                0,
            ),
            function(
                cg_name("user", &[], "map_string_int"),
                "map_string_int",
                vec![fn_arg("m", map_s_i())],
                map_s_s(),
                "maps.baml",
                100,
                vec![],
            ),
            function(
                cg_name("user", &[], "nested_map"),
                "nested_map",
                vec![fn_arg(
                    "m",
                    Ty::Map {
                        key: Box::new(str_ty()),
                        value: Box::new(map_s_i()),
                    },
                )],
                Ty::Unit,
                "maps.baml",
                200,
                vec![],
            ),
            function(
                cg_name("user", &[], "map_of_arrays"),
                "map_of_arrays",
                vec![fn_arg(
                    "m",
                    Ty::Map {
                        key: Box::new(str_ty()),
                        value: Box::new(Ty::List(Box::new(int_ty()))),
                    },
                )],
                Ty::Unit,
                "maps.baml",
                300,
                vec![],
            ),
        ],
    );
    pool
}

fn mixed_complex_types() -> SymbolPool {
    let mut pool = SymbolPool::new();
    let cls = |n: &str| cg_name("user", &[], n);

    let s = || Ty::String;
    let i = || Ty::Int;
    let f = || Ty::Float;
    let b = || Ty::Bool;
    let list = |t| Ty::List(Box::new(t));
    let map = |k, v| Ty::Map {
        key: Box::new(k),
        value: Box::new(v),
    };
    let cls_ty = |n: &str| Ty::Class(cls(n));
    let opt = |t| Ty::Optional(Box::new(t));

    let mut file_span = 0u32;
    let mut next_span = || {
        file_span += 10;
        file_span
    };

    insert_all(
        &mut pool,
        vec![
            class(
                cls("KitchenSink"),
                vec![
                    ("id", i()),
                    ("name", s()),
                    ("score", f()),
                    ("active", b()),
                    ("status", s()),
                    ("priority", i()),
                    ("tags", list(s())),
                    ("numbers", list(i())),
                    ("matrix", list(list(i()))),
                    ("metadata", map(s(), s())),
                    ("scores", map(s(), f())),
                    ("description", opt(s())),
                    ("data", Ty::Union(vec![s(), i(), cls_ty("DataObject")])),
                    (
                        "result",
                        Ty::Union(vec![cls_ty("Success"), cls_ty("Error")]),
                    ),
                    ("user", cls_ty("User")),
                    ("items", list(cls_ty("Item"))),
                    ("config", cls_ty("Configuration")),
                ],
                "mixed.baml",
                next_span(),
            ),
            class(
                cls("DataObject"),
                vec![("type_", s()), ("value", map(s(), s()))],
                "mixed.baml",
                next_span(),
            ),
            class(
                cls("Success"),
                vec![("type_", s()), ("data", map(s(), s()))],
                "mixed.baml",
                next_span(),
            ),
            class(
                cls("Error"),
                vec![("type_", s()), ("message", s()), ("code", i())],
                "mixed.baml",
                next_span(),
            ),
            class(
                cls("User"),
                vec![
                    ("id", i()),
                    ("profile", cls_ty("UserProfile")),
                    ("settings", map(s(), cls_ty("Setting"))),
                ],
                "mixed.baml",
                next_span(),
            ),
            class(
                cls("UserProfile"),
                vec![
                    ("name", s()),
                    ("email", s()),
                    ("bio", opt(s())),
                    ("links", list(s())),
                ],
                "mixed.baml",
                next_span(),
            ),
            class(
                cls("Setting"),
                vec![
                    ("key", s()),
                    ("value", Ty::Union(vec![s(), i(), b()])),
                    ("metadata", opt(map(s(), s()))),
                ],
                "mixed.baml",
                next_span(),
            ),
            class(
                cls("Item"),
                vec![
                    ("id", i()),
                    ("name", s()),
                    ("variants", list(cls_ty("Variant"))),
                    ("attributes", map(s(), Ty::Union(vec![s(), i(), f(), b()]))),
                ],
                "mixed.baml",
                next_span(),
            ),
            class(
                cls("Variant"),
                vec![
                    ("sku", s()),
                    ("price", f()),
                    ("stock", i()),
                    ("options", map(s(), s())),
                ],
                "mixed.baml",
                next_span(),
            ),
            class(
                cls("Configuration"),
                vec![
                    ("version", s()),
                    ("features", list(cls_ty("Feature"))),
                    ("environments", map(s(), cls_ty("Environment"))),
                    ("rules", list(cls_ty("Rule"))),
                ],
                "mixed.baml",
                next_span(),
            ),
            class(
                cls("Feature"),
                vec![
                    ("name", s()),
                    ("enabled", b()),
                    ("config", opt(map(s(), Ty::Union(vec![s(), i(), b()])))),
                    ("dependencies", list(s())),
                ],
                "mixed.baml",
                next_span(),
            ),
            class(
                cls("Environment"),
                vec![
                    ("name", s()),
                    ("url", s()),
                    ("variables", map(s(), s())),
                    ("secrets", opt(map(s(), s()))),
                ],
                "mixed.baml",
                next_span(),
            ),
            class(
                cls("Rule"),
                vec![
                    ("id", i()),
                    ("name", s()),
                    ("condition", cls_ty("Condition")),
                    ("actions", list(cls_ty("Action"))),
                    ("priority", i()),
                ],
                "mixed.baml",
                next_span(),
            ),
            class(
                cls("Condition"),
                vec![
                    ("type_", s()),
                    (
                        "conditions",
                        list(Ty::Union(vec![
                            cls_ty("Condition"),
                            cls_ty("SimpleCondition"),
                        ])),
                    ),
                ],
                "mixed.baml",
                next_span(),
            ),
            class(
                cls("SimpleCondition"),
                vec![
                    ("field", s()),
                    ("operator", s()),
                    ("value", Ty::Union(vec![s(), i(), f(), b()])),
                ],
                "mixed.baml",
                next_span(),
            ),
            class(
                cls("Action"),
                vec![
                    ("type_", s()),
                    ("parameters", map(s(), Ty::Union(vec![s(), i(), b()]))),
                    ("async_", b()),
                ],
                "mixed.baml",
                next_span(),
            ),
            class(
                cls("UltraComplex"),
                vec![
                    ("tree", cls_ty("Node")),
                    ("widgets", list(cls_ty("Widget"))),
                    ("data", opt(cls_ty("ComplexData"))),
                    ("response", cls_ty("UserResponse")),
                    ("assets", list(cls_ty("Asset"))),
                ],
                "mixed.baml",
                next_span(),
            ),
            class(
                cls("Node"),
                vec![
                    ("id", i()),
                    ("type_", s()),
                    (
                        "value",
                        Ty::Union(vec![
                            s(),
                            i(),
                            list(cls_ty("Node")),
                            map(s(), cls_ty("Node")),
                        ]),
                    ),
                    ("metadata", opt(cls_ty("NodeMetadata"))),
                ],
                "mixed.baml",
                next_span(),
            ),
            class(
                cls("NodeMetadata"),
                vec![
                    ("created", s()),
                    ("modified", s()),
                    ("tags", list(s())),
                    ("attributes", map(s(), Ty::Union(vec![s(), i(), b()]))),
                ],
                "mixed.baml",
                next_span(),
            ),
            class(
                cls("Widget"),
                vec![
                    ("type_", s()),
                    ("button", opt(cls_ty("ButtonWidget"))),
                    ("text", opt(cls_ty("TextWidget"))),
                    ("img", opt(cls_ty("ImageWidget"))),
                    ("container", opt(cls_ty("ContainerWidget"))),
                ],
                "mixed.baml",
                next_span(),
            ),
            class(
                cls("ButtonWidget"),
                vec![("label", s()), ("action", s()), ("style", map(s(), s()))],
                "mixed.baml",
                next_span(),
            ),
            class(
                cls("TextWidget"),
                vec![("content", s()), ("format", s()), ("style", map(s(), s()))],
                "mixed.baml",
                next_span(),
            ),
            class(
                cls("ImageWidget"),
                vec![("alt", s()), ("dimensions", cls_ty("Dimensions"))],
                "mixed.baml",
                next_span(),
            ),
            class(
                cls("Dimensions"),
                vec![("width", i()), ("height", i())],
                "mixed.baml",
                next_span(),
            ),
            class(
                cls("ContainerWidget"),
                vec![
                    ("layout", s()),
                    ("children", list(cls_ty("Widget"))),
                    ("style", map(s(), s())),
                ],
                "mixed.baml",
                next_span(),
            ),
            class(
                cls("ComplexData"),
                vec![
                    ("primary", cls_ty("PrimaryData")),
                    ("secondary", opt(cls_ty("SecondaryData"))),
                    ("tertiary", opt(cls_ty("TertiaryData"))),
                ],
                "mixed.baml",
                next_span(),
            ),
            class(
                cls("PrimaryData"),
                vec![
                    ("values", list(Ty::Union(vec![s(), i(), f()]))),
                    ("mappings", map(s(), map(s(), s()))),
                    ("flags", list(b())),
                ],
                "mixed.baml",
                next_span(),
            ),
            class(
                cls("SecondaryData"),
                vec![
                    ("records", list(cls_ty("Record"))),
                    ("index", map(s(), cls_ty("Record"))),
                ],
                "mixed.baml",
                next_span(),
            ),
            class(
                cls("Record"),
                vec![
                    ("id", i()),
                    ("data", map(s(), Ty::Union(vec![s(), i(), b()]))),
                    ("related", opt(list(cls_ty("Record")))),
                ],
                "mixed.baml",
                next_span(),
            ),
            class(
                cls("TertiaryData"),
                vec![("raw", s()), ("parsed", opt(map(s(), s()))), ("valid", b())],
                "mixed.baml",
                next_span(),
            ),
            class(
                cls("UserResponse"),
                vec![
                    ("status", s()),
                    ("data", opt(cls_ty("User"))),
                    ("error", opt(cls_ty("ErrorDetail"))),
                    ("metadata", cls_ty("ResponseMetadata")),
                ],
                "mixed.baml",
                next_span(),
            ),
            class(
                cls("ErrorDetail"),
                vec![
                    ("code", s()),
                    ("message", s()),
                    ("details", opt(map(s(), s()))),
                ],
                "mixed.baml",
                next_span(),
            ),
            class(
                cls("ResponseMetadata"),
                vec![
                    ("timestamp", s()),
                    ("requestId", s()),
                    ("duration", i()),
                    ("retries", i()),
                ],
                "mixed.baml",
                next_span(),
            ),
            class(
                cls("Asset"),
                vec![
                    ("id", i()),
                    ("type_", s()),
                    ("metadata", cls_ty("AssetMetadata")),
                    ("tags", list(s())),
                ],
                "mixed.baml",
                next_span(),
            ),
            class(
                cls("AssetMetadata"),
                vec![
                    ("filename", s()),
                    ("size", i()),
                    ("mimeType", s()),
                    ("uploaded", s()),
                    ("checksum", s()),
                ],
                "mixed.baml",
                next_span(),
            ),
            function(
                cls("TestKitchenSink"),
                "TestKitchenSink",
                vec![fn_arg("input", s())],
                cls_ty("KitchenSink"),
                "mixed.baml",
                next_span(),
                vec![],
            ),
            function(
                cls("TestUltraComplex"),
                "TestUltraComplex",
                vec![fn_arg("input", s())],
                cls_ty("UltraComplex"),
                "mixed.baml",
                next_span(),
                vec![],
            ),
            function(
                cls("TestRecursiveComplexity"),
                "TestRecursiveComplexity",
                vec![fn_arg("input", s())],
                cls_ty("Node"),
                "mixed.baml",
                next_span(),
                vec![],
            ),
        ],
    );
    pool
}

fn union_types_extended() -> SymbolPool {
    let mut pool = SymbolPool::new();
    let root_cls = |n: &str| cg_name("user", &[], n);

    insert_all(
        &mut pool,
        vec![
            function(
                root_cls("union_simple"),
                "union_simple",
                vec![fn_arg("u", Ty::Union(vec![Ty::String, Ty::Int]))],
                Ty::Bool,
                "unions.baml",
                10,
                vec![],
            ),
            function(
                root_cls("union_complex"),
                "union_complex",
                vec![fn_arg(
                    "u",
                    Ty::Union(vec![
                        Ty::Class(root_cls("User")),
                        Ty::Class(root_cls("Company")),
                        Ty::String,
                    ]),
                )],
                Ty::Unit,
                "unions.baml",
                20,
                vec![],
            ),
            function(
                root_cls("union_in_list"),
                "union_in_list",
                vec![fn_arg(
                    "items",
                    Ty::List(Box::new(Ty::Union(vec![Ty::String, Ty::Int]))),
                )],
                Ty::Unit,
                "unions.baml",
                30,
                vec![],
            ),
            function(
                root_cls("union_return"),
                "union_return",
                vec![],
                Ty::Union(vec![Ty::String, Ty::Int]),
                "unions.baml",
                40,
                vec![],
            ),
            class(
                root_cls("User"),
                vec![("name", Ty::String)],
                "unions.baml",
                50,
            ),
            class(
                root_cls("Company"),
                vec![("name", Ty::String), ("industry", Ty::String)],
                "unions.baml",
                60,
            ),
            class(
                root_cls("Container"),
                vec![
                    (
                        "items",
                        Ty::List(Box::new(Ty::Union(vec![Ty::String, Ty::Int, Ty::Bool]))),
                    ),
                    (
                        "matrix",
                        Ty::List(Box::new(Ty::List(Box::new(Ty::Union(vec![
                            Ty::String,
                            Ty::Int,
                        ]))))),
                    ),
                    (
                        "optional_union",
                        Ty::Optional(Box::new(Ty::Union(vec![Ty::String, Ty::Int]))),
                    ),
                ],
                "unions.baml",
                70,
            ),
        ],
    );
    pool
}

fn full_type_coverage() -> SymbolPool {
    let mut pool = SymbolPool::new();
    let my_class = cg_name("user", &[], "MyClass");
    let cover_all = cg_name("user", &[], "CoverAll");
    let recursive_alias = cg_name("user", &[], "RecursiveAlias");

    insert_all(
        &mut pool,
        vec![
            class(my_class.clone(), vec![("id", Ty::Int)], "cover.baml", 10),
            class(
                cover_all.clone(),
                vec![
                    ("unknown_field", Ty::BuiltinUnknown),
                    (
                        "callable_field",
                        Ty::Callable {
                            params: vec![Ty::Int],
                            ret: Box::new(Ty::String),
                        },
                    ),
                    ("alias_field", Ty::TypeAlias(recursive_alias.clone())),
                    // `Ty::Media` would render as `baml.media.Image`; the
                    // handle-backed media classes are deferred per
                    // 10g2 §9.1 / 11h §12.5, so this fixture skips them
                    // to avoid surfacing the absence.
                    (
                        "literal_field",
                        Ty::Literal(Literal::String("Hello".into())),
                    ),
                    (
                        "optional_nested",
                        Ty::List(Box::new(Ty::Optional(Box::new(Ty::Class(my_class))))),
                    ),
                    ("union_field", Ty::Union(vec![Ty::Int, Ty::String])),
                    ("self_ref", Ty::Optional(Box::new(Ty::Class(cover_all)))),
                ],
                "cover.baml",
                20,
            ),
            type_alias(
                recursive_alias.clone(),
                Ty::Union(vec![
                    Ty::Int,
                    Ty::List(Box::new(Ty::TypeAlias(recursive_alias))),
                ]),
                true,
                "cover.baml",
                30,
            ),
        ],
    );
    pool
}

fn semantic_streaming() -> SymbolPool {
    let mut pool = SymbolPool::new();
    let n = |bare: &str| cg_name("user", &[], bare);

    let s = || Ty::String;
    let i = || Ty::Int;

    // stream_state<T> renders the same as a class in BEP-030; we pick
    // `Optional<T>` as a proxy so the tree type-checks even though the
    // BAML DSL used a first-class `stream_state<T>` ctor. The rig test
    // here is import-only; the semantic check is that `stream_types/`
    // counterparts emit alongside the non-stream ones.
    let stream_state = |inner: Ty| Ty::Optional(Box::new(inner));

    let small_thing = n("SmallThing");
    let class_without_done = n("ClassWithoutDone");
    let class_with_block_done = n("ClassWithBlockDone");
    let semantic_container = n("SemanticContainer");

    insert_all(
        &mut pool,
        vec![
            class(
                semantic_container.clone(),
                vec![
                    ("sixteen_digit_number", i()),
                    ("string_with_twenty_words", stream_state(s())),
                    ("class_1", Ty::Class(class_without_done.clone())),
                    ("class_2", Ty::Class(class_with_block_done.clone())),
                    (
                        "class_done_needed",
                        stream_state(Ty::Class(class_with_block_done.clone())),
                    ),
                    (
                        "class_needed",
                        stream_state(Ty::Class(class_without_done.clone())),
                    ),
                    (
                        "three_small_things",
                        Ty::List(Box::new(Ty::Class(small_thing.clone()))),
                    ),
                    ("final_string", s()),
                ],
                "stream.baml",
                10,
            ),
            class(
                class_without_done.clone(),
                vec![("i_16_digits", i()), ("s_20_words", stream_state(s()))],
                "stream.baml",
                20,
            ),
            class(
                class_with_block_done.clone(),
                vec![("i_16_digits", i()), ("s_20_words", s())],
                "stream.baml",
                30,
            ),
            class(
                small_thing,
                vec![("i_16_digits", stream_state(i())), ("i_8_digits", i())],
                "stream.baml",
                40,
            ),
            // $stream companions — route to stream_types/
            class(
                cg_name("user", &[], "SemanticContainer$stream"),
                vec![
                    ("sixteen_digit_number", i()),
                    ("string_with_twenty_words", stream_state(s())),
                    (
                        "class_1",
                        Ty::Class(cg_name("user", &[], "ClassWithoutDone$stream")),
                    ),
                    ("class_2", Ty::Class(class_with_block_done.clone())),
                    (
                        "class_done_needed",
                        stream_state(Ty::Class(class_with_block_done.clone())),
                    ),
                    (
                        "class_needed",
                        stream_state(Ty::Class(cg_name("user", &[], "ClassWithoutDone$stream"))),
                    ),
                    (
                        "three_small_things",
                        Ty::List(Box::new(Ty::Class(cg_name(
                            "user",
                            &[],
                            "SmallThing$stream",
                        )))),
                    ),
                    ("final_string", s()),
                ],
                "stream.baml",
                50,
            ),
            class(
                cg_name("user", &[], "ClassWithoutDone$stream"),
                vec![("i_16_digits", i()), ("s_20_words", stream_state(s()))],
                "stream.baml",
                60,
            ),
            class(
                cg_name("user", &[], "SmallThing$stream"),
                vec![("i_16_digits", stream_state(i())), ("i_8_digits", i())],
                "stream.baml",
                70,
            ),
            function(
                n("MakeSemanticContainer"),
                "MakeSemanticContainer",
                vec![],
                Ty::Class(semantic_container),
                "stream.baml",
                80,
                vec![],
            ),
            function(
                n("MakeClassWithBlockDone"),
                "MakeClassWithBlockDone",
                vec![],
                Ty::Class(class_with_block_done),
                "stream.baml",
                90,
                vec![],
            ),
            function(
                n("MakeClassWithExternalDone"),
                "MakeClassWithExternalDone",
                vec![],
                stream_state(Ty::Class(class_without_done)),
                "stream.baml",
                100,
                vec![],
            ),
        ],
    );
    pool
}

fn packages_and_namespaces() -> SymbolPool {
    let mut pool = SymbolPool::new();

    insert_all(
        &mut pool,
        vec![
            // root.Resume — user package, no namespace
            class(
                cg_name("user", &[], "Resume"),
                vec![("name", Ty::String)],
                "pkgs.baml",
                10,
            ),
            // user, ns=foo → baml_sdk/foo/
            class(
                cg_name("user", &["foo"], "Sentiment"),
                vec![("label", Ty::String)],
                "pkgs.baml",
                20,
            ),
            // vendor "other", ns=foo → baml_sdk/vendor/other/foo/
            class(
                cg_name("other", &["foo"], "Address"),
                vec![("street", Ty::String)],
                "pkgs.baml",
                30,
            ),
            // baml package, ns=http → baml_sdk/baml/http/
            class(
                cg_name("baml", &["http"], "Request"),
                vec![("url", Ty::String)],
                "pkgs.baml",
                40,
            ),
        ],
    );
    pool
}

fn companion_functions() -> SymbolPool {
    let mut pool = SymbolPool::new();

    insert_all(
        &mut pool,
        vec![
            class(
                cg_name("user", &[], "Resume"),
                vec![("name", Ty::String)],
                "comp.baml",
                10,
            ),
            class(
                cg_name("user", &["foo"], "Sentiment"),
                vec![("label", Ty::String)],
                "comp.baml",
                20,
            ),
            function(
                cg_name("user", &[], "ExtractResume"),
                "ExtractResume",
                vec![fn_arg("resume", Ty::String)],
                Ty::Class(cg_name("user", &[], "Resume")),
                "comp.baml",
                30,
                vec![
                    (
                        "build_request",
                        vec![fn_arg("resume", Ty::String)],
                        Ty::String,
                    ),
                    (
                        "render_prompt",
                        vec![fn_arg("resume", Ty::String)],
                        Ty::String,
                    ),
                    (
                        "parse",
                        vec![fn_arg("json", Ty::String)],
                        Ty::Class(cg_name("user", &[], "Resume")),
                    ),
                ],
            ),
            function(
                cg_name("user", &["foo"], "ClassifySentiment"),
                "ClassifySentiment",
                vec![fn_arg("input", Ty::String)],
                Ty::Class(cg_name("user", &["foo"], "Sentiment")),
                "comp.baml",
                40,
                vec![(
                    "build_request",
                    vec![fn_arg("input", Ty::String)],
                    Ty::String,
                )],
            ),
        ],
    );
    pool
}
