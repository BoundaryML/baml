//! Focused regressions for PPIR package aggregation.

#[cfg(test)]
mod tests {
    use baml_artifact::ArtifactKind;
    use baml_base::Name;
    use baml_compiler2_hir::package::PackageId;
    use baml_compiler2_hir_ty::package_interface::PackageInterface;
    use baml_db::ProjectDatabase;

    use crate::engine::TestDbExt;

    fn namespace_iteration_order(files: &[&str]) -> Vec<Vec<String>> {
        let mut db = ProjectDatabase::new();
        db.workspace(std::path::Path::new("."));

        for (index, namespace) in files.iter().enumerate() {
            db.file(
                format!("ns_{namespace}/marker_{index}.baml"),
                &format!("class Marker{index} {{}}"),
            );
        }

        let package =
            baml_compiler2_ppir::package_items(&db, PackageId::new(&db, Name::new("user")));
        package
            .namespaces
            .keys()
            .map(|path| path.iter().map(ToString::to_string).collect())
            .collect()
    }

    fn first_seen_namespace_order(files: &[&str]) -> Vec<Vec<String>> {
        let mut expected = Vec::new();
        for namespace in files {
            let path = vec![namespace.to_string()];
            if !expected.contains(&path) {
                expected.push(path);
            }
        }
        expected
    }

    #[test]
    fn package_namespace_iteration_preserves_file_discovery_order() {
        let namespaces = [
            "zulu", "alpha", "quebec", "bravo", "papa", "charlie", "oscar", "delta", "november",
            "echo", "mike", "foxtrot", "lima", "golf", "kilo", "hotel", "juliet", "india", "alpha",
            "zulu", "golf",
        ];
        let expected = first_seen_namespace_order(&namespaces);
        assert_eq!(namespace_iteration_order(&namespaces), expected);
        assert_eq!(
            expected.len(),
            18,
            "duplicate namespace paths must be removed"
        );

        let mut reversed = namespaces;
        reversed.reverse();
        assert_eq!(
            namespace_iteration_order(&reversed),
            first_seen_namespace_order(&reversed)
        );

        let mut rotated = namespaces;
        rotated.rotate_left(7);
        assert_eq!(
            namespace_iteration_order(&rotated),
            first_seen_namespace_order(&rotated)
        );
    }

    #[test]
    fn package_interface_artifacts_preserve_declaration_order_and_reject_v1() {
        let mut db = ProjectDatabase::new();
        db.workspace(std::path::Path::new("."));
        db.file(
            "types.baml",
            "class Zulu {}\nclass Alpha {}\nclass Mike {}\n",
        );

        let package_id = PackageId::new(&db, Name::new("user"));
        let interface =
            baml_compiler2_hir_ty::package_interface::package_interface(&db, package_id);
        let artifact = baml_artifact::encode(ArtifactKind::PackageInterface, interface)
            .expect("package interface encodes");
        let decoded: PackageInterface =
            baml_artifact::decode(ArtifactKind::PackageInterface, &artifact)
                .expect("current package interface decodes");
        let declaration_order: Vec<_> = decoded.types[&Vec::<Name>::new()]
            .keys()
            .map(Name::as_str)
            .filter(|name| !name.contains('$'))
            .collect();
        assert_eq!(declaration_order, ["Zulu", "Alpha", "Mike"]);

        let legacy = baml_artifact::encode_with_format_for_test(
            1,
            ArtifactKind::PackageInterface,
            interface,
        )
        .expect("legacy package interface envelope encodes");
        assert!(matches!(
            baml_artifact::decode::<PackageInterface>(ArtifactKind::PackageInterface, &legacy),
            Err(baml_artifact::Error::Incompatible {
                artifact_format: 1,
                runtime_format: baml_artifact::FORMAT_VERSION,
                ..
            })
        ));
    }

    /// The unified body-owner queries (`body`/`body_source_map`/`body_scope`/
    /// `file_body_owners`) must agree with the per-kind queries they dispatch
    /// to.
    #[test]
    fn body_owner_queries_match_per_kind_queries() {
        use baml_compiler2_hir::body::{BodyOwnerId, FunctionBody, OwnerBody};

        let mut db = ProjectDatabase::new();
        db.workspace(std::path::Path::new("."));
        let file = db.file(
            "test.baml",
            "function f() -> int { 1 }\n\nfunction g() -> int { f() }\n",
        );

        let owners = baml_compiler2_ppir::file_body_owners(&db, file);
        let functions = baml_compiler2_ppir::item_data::file_functions(&db, file);
        assert_eq!(owners.len(), functions.len());

        for (&func_loc, &owner) in functions.iter().zip(owners.iter()) {
            assert_eq!(owner, BodyOwnerId::Function(func_loc));
            assert!(owner.file(&db) == file);

            let unified = baml_compiler2_ppir::body(&db, owner);
            let direct = baml_compiler2_ppir::function_body(&db, func_loc);
            assert!(matches!(direct.as_ref(), FunctionBody::Expr(_)));
            let OwnerBody::Function(unified_body) = &unified else {
                panic!("function owner must dispatch to the function body query");
            };
            assert_eq!(unified_body, &direct);
            assert_eq!(
                unified.expr_body(),
                match direct.as_ref() {
                    FunctionBody::Expr(body) => Some(body),
                    _ => None,
                }
            );

            assert_eq!(
                baml_compiler2_ppir::body_source_map(&db, owner),
                baml_compiler2_ppir::function_body_source_map(&db, func_loc)
            );
            assert!(
                baml_compiler2_ppir::body_scope(&db, owner)
                    == baml_compiler2_ppir::item_data::function_scope(&db, func_loc)
            );
        }
    }

    #[test]
    fn llm_stream_projection_is_internal_and_source_callable() {
        let mut db = ProjectDatabase::new();
        db.workspace(std::path::Path::new("."));
        let file = db.file(
            "stream.baml",
            r#"
class Box<T> {
    value T
}

class Runner {
    prefix string

    function Ask(self, text: string) -> string {
        client: "openai/gpt-4o-mini"
        prompt: `${self.prefix}: ${text}`
    }

    function StaticAsk(text: string) -> string {
        client: "openai/gpt-4o-mini"
        prompt: `${text}`
    }
}

function Ask<T>(text: string, tone: string = "plain") -> Box<T> {
    client: "openai/gpt-4o-mini"
    prompt: `answer ${text} in a ${tone} tone`
}

function Dollar$Ask(text: string) -> string {
    client: "openai/gpt-4o-mini"
    prompt: `${text}`
}

function Start(text: string) -> ai.stream.Stream<Box$stream<int>?, Box<int>> {
    Ask@stream<int>(text)
}

function StartDollar(text: string) -> ai.stream.Stream<string?, string> {
    Dollar$Ask@stream(text)
}

function StartBound(runner: Runner, text: string) -> ai.stream.Stream<string?, string> {
    runner.Ask@stream(text)
}

function StartQualified(text: string) -> ai.stream.Stream<string?, string> {
    Runner.StaticAsk@stream(text)
}

function BoundSpec(runner: Runner, text: string) -> ai.FunctionSpec<string> {
    runner.Ask@spec(text)
}

function StaticSpec(text: string) -> ai.FunctionSpec<string> {
    Runner.StaticAsk@spec(text)
}
"#,
        );

        let diagnostics = baml_db::collect_compiler2_diagnostics(&db);
        assert!(
            diagnostics.is_empty(),
            "source `Fn@stream` must resolve to the private PPIR entry: {diagnostics:#?}"
        );

        let package =
            baml_compiler2_ppir::package_items(&db, PackageId::new(&db, Name::new("user")));
        let root = package
            .namespaces
            .get(&Vec::new())
            .expect("user root namespace");
        assert!(root.values.contains_key(&Name::new("Ask")));
        assert!(root.values.contains_key(&Name::new("Ask@stream")));
        assert!(root.values.contains_key(&Name::new("Dollar$Ask")));
        assert!(root.values.contains_key(&Name::new("Dollar$Ask@stream")));
        assert!(root.types.contains_key(&Name::new("Box$stream")));

        let expansion = baml_compiler2_ppir::ppir_expansion_items(&db, file);
        let stream = expansion
            .items(&db)
            .iter()
            .find_map(|item| match item {
                baml_compiler2_ast::ast::Item::Function(function)
                    if function.name.as_str() == "Ask@stream" =>
                {
                    Some(function)
                }
                _ => None,
            })
            .expect("PPIR must synthesize the private stream function");
        let authored_ast = baml_compiler2_hir::file_ast(&db, file);
        let authored = authored_ast
            .items
            .iter()
            .find_map(|item| match item {
                baml_compiler2_ast::ast::Item::Function(function)
                    if function.name.as_str() == "Ask" =>
                {
                    Some(function)
                }
                _ => None,
            })
            .expect("authored LLM function");

        assert_eq!(stream.generic_params, authored.generic_params);
        assert_eq!(stream.defaults, authored.defaults);
        assert_eq!(
            stream.is_tagged_template_tag,
            authored.is_tagged_template_tag
        );
        assert_eq!(
            stream.metadata.origin,
            baml_compiler2_ast::ast::FunctionOrigin::Companion
        );
        assert!(stream.metadata.is_language_internal);
        assert_eq!(
            stream
                .params
                .iter()
                .map(|param| param.name.as_str())
                .collect::<Vec<_>>(),
            ["text", "tone", "client", "on_event"]
        );
    }
}
