use baml_sdkgen_types::{
    Class, ClassProperty, Enum, EnumVariant, Name, NamingConvention, Origin, Symbol, SymbolPool, Ty,
};

// Exercise self-reference, mutual cross-namespace references, and a later enum.
// This is a generator fixture, not an alternate declaration of the app-local SDK.
pub(super) fn generate() -> tempfile::TempDir {
    let node = Name::new("user".into(), vec!["alpha".into()], "Node".into());
    let link = Name::new("user".into(), vec!["zeta".into()], "Link".into());
    let mood = Name::new("user".into(), vec!["zeta".into()], "Mood".into());
    let origin = Origin {
        source_file_path: "class_loading.baml".into(),
        span_start: 0,
    };
    let property = |name: &str, ty| ClassProperty {
        name: name.into(),
        docstring: None,
        ty,
    };
    let class = |name: Name, properties| {
        Symbol::Class(Class {
            name,
            generic_params: vec![],
            docstring: None,
            properties,
            static_methods: vec![],
            instance_methods: vec![],
            origin: origin.clone(),
        })
    };
    let optional = |ty| Ty::Union(Box::new([ty, Ty::Null]));
    let node_ty = Ty::Class(node.clone(), Box::new([]));
    let link_ty = Ty::Class(link.clone(), Box::new([]));
    let pool = SymbolPool::from([
        (
            node.clone(),
            class(
                node,
                vec![
                    property("nodeLabel", Ty::String),
                    property("nextNode", optional(node_ty.clone())),
                    property("link", optional(link_ty.clone())),
                    property("mood", Ty::Enum(mood.clone())),
                    property("children", Ty::List(Box::new(node_ty.clone()))),
                    property(
                        "links",
                        Ty::Map {
                            key: Box::new(Ty::String),
                            value: Box::new(link_ty),
                        },
                    ),
                ],
            ),
        ),
        (
            link.clone(),
            class(
                link,
                vec![
                    property("node", optional(node_ty)),
                    property("weight", Ty::Int),
                ],
            ),
        ),
        (
            mood.clone(),
            Symbol::Enum(Enum {
                name: mood,
                docstring: None,
                variants: vec![EnumVariant {
                    name: "Happy".into(),
                    docstring: None,
                    value: "happy".into(),
                }],
                origin,
            }),
        ),
    ]);
    let (files, skipped) = sdkgen_ruby_sorbet::to_source_code_with_bytecode_and_skipped(
        &pool,
        &[],
        NamingConvention::Language,
    )
    .expect("generate cross-reference fixture");
    assert!(skipped.is_empty(), "unexpected omissions: {skipped:?}");
    let directory = tempfile::tempdir().expect("class-loading fixture directory");
    for (path, source) in files {
        let path = directory.path().join(path);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, source).unwrap();
    }
    directory
}
