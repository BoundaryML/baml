//! Focused tests for `baml_compiler2_hir_ty` declaration lowering (S4).

#[cfg(test)]
mod tests {
    use baml_compiler2_hir_ty::lower::{
        FunctionSignature, class_field_types, function_signature, type_alias_value,
    };
    use baml_db::ProjectDatabase;

    use crate::engine::TestDbExt;

    #[expect(
        deprecated,
        reason = "pre-D3: materializes interned inference state; moves to the finalized facts layer with the cutover"
    )]
    fn render(ty: &baml_type::interned::Ty) -> String {
        ty.to_plain().render_canonical()
    }

    fn signature_of(
        db: &ProjectDatabase,
        file: baml_base::SourceFile,
        name: &str,
    ) -> FunctionSignature {
        let function = *baml_compiler2_ppir::item_data::file_functions(db, file)
            .iter()
            .find(|&&loc| {
                baml_compiler2_ppir::item_data::function_data(db, loc)
                    .name
                    .as_str()
                    == name
            })
            .unwrap_or_else(|| panic!("function `{name}` not found"));
        function_signature(db, function).clone()
    }

    fn param_renders(signature: &FunctionSignature) -> Vec<String> {
        signature.params.iter().map(|p| render(&p.ty)).collect()
    }

    #[test]
    fn lowers_signature_types_with_name_resolution() {
        let mut db = crate::compiler2_tir::support::make_db();
        let file = db.file(
            "test.baml",
            r#"
class Box<T> { v T }
enum Status { Active, Done }
type Alias = int | string

function f(
    a: int,
    b: Box<int>,
    c: Status,
    d: Alias,
    e: Status.Active,
    g: int?,
    h: map<string, bool>,
    i: Box,
) -> string throws never {
    "x"
}
"#,
        );
        let signature = signature_of(&db, file, "f");
        assert_eq!(
            param_renders(&signature),
            [
                "int",
                "user.Box<int>",
                "user.Status",
                "user.Alias",
                "user.Status.Active",
                "int | null",
                "map<string, bool>",
                // Arity recovery: bare generic head pads with the sentinel.
                "user.Box<!error>",
            ]
        );
        assert_eq!(render(&signature.ret), "string");
        assert_eq!(render(&signature.throws), "never");
    }

    #[test]
    fn interface_existentials_and_signature_holes() {
        let mut db = crate::compiler2_tir::support::make_db();
        let file = db.file(
            "test.baml",
            r#"
interface Show<T> {
    type Out
    function show(self, x: T) -> string throws never
}

function f(
    a: Show<int>,
    b: Show<int, Out = string>,
    c: _,
) -> int throws never {
    1
}
"#,
        );
        let signature = signature_of(&db, file, "f");
        assert_eq!(
            param_renders(&signature),
            [
                // An existential denotes one complete instantiation: the
                // unpinned, defaultless `Out` is diagnosed (E0191-analog)
                // and its slot recovers as the error sentinel.
                "user.Show<int, Out = !error>",
                "user.Show<int, Out = string>",
                // Ruling 4: `_` never infers in declaration signatures - the
                // hole node is rejected by the signature-side policy fold.
                "!error",
            ]
        );
    }

    #[test]
    fn generic_frames_cover_functions_and_methods() {
        let mut db = crate::compiler2_tir::support::make_db();
        let file = db.file(
            "test.baml",
            r#"
class Holder<T> {
    v T
    function m<U>(self, x: T, y: U) -> T throws never {
        x
    }
}

function pair<T>(x: T, y: T[]) -> T throws never {
    x
}
"#,
        );
        let pair = signature_of(&db, file, "pair");
        assert_eq!(param_renders(&pair), ["T", "T[]"]);
        assert_eq!(render(&pair.ret), "T");

        // Method frames prepend the class generics: T = 0, U = 1. The
        // `self` receiver is the owner class applied to its own params
        // (S11 `class_self_ty`).
        let method = signature_of(&db, file, "m");
        assert_eq!(param_renders(&method), ["user.Holder<T>", "T", "U"]);
        assert_eq!(render(&method.ret), "T");
        assert_eq!(
            method
                .generic_params
                .iter()
                .map(|p| (p.index(), p.as_str().to_owned()))
                .collect::<Vec<_>>(),
            [(0, "T".to_owned()), (1, "U".to_owned())]
        );
    }

    #[test]
    fn resolves_across_namespaces_and_packages() {
        let mut db = crate::compiler2_tir::support::make_db();
        db.file("ns_util/util.baml", "class Helper {}");
        let file = db.file(
            "main.baml",
            r#"
function f(
    a: root.util.Helper,
    b: baml.future.Future<int, never>,
) -> int throws never {
    1
}
"#,
        );
        let signature = signature_of(&db, file, "f");
        assert_eq!(
            param_renders(&signature),
            ["user.util.Helper", "baml.future.Future<int, never>"]
        );

        // Inside the namespace, the bare name resolves namespace-relative.
        let util_file = db.file(
            "ns_util/use.baml",
            "function g(h: Helper) -> int throws never { 1 }",
        );
        let inside = signature_of(&db, util_file, "g");
        assert_eq!(param_renders(&inside), ["user.util.Helper"]);
    }

    #[test]
    fn class_fields_and_alias_values_lower() {
        let mut db = crate::compiler2_tir::support::make_db();
        let file = db.file(
            "test.baml",
            r#"
class Pair<L, R> {
    left L
    right R[]
}
type Loop = Loop[]
"#,
        );
        let class = baml_compiler2_ppir::item_data::file_classes(&db, file)
            .iter()
            .copied()
            .find(|&loc| {
                baml_compiler2_ppir::item_data::class_data(&db, loc)
                    .name
                    .as_str()
                    == "Pair"
            })
            .expect("Pair exists");
        let fields: Vec<(String, String)> = class_field_types(&db, class)
            .iter()
            .map(|(name, ty)| (name.to_string(), render(ty)))
            .collect();
        assert_eq!(
            fields,
            [
                ("left".to_owned(), "L".to_owned()),
                ("right".to_owned(), "R[]".to_owned())
            ]
        );

        // A recursive alias stays nominal in its own value - no expansion at
        // lowering time. (Synthetic `$stream` companions sort first in the
        // enumeration, so select by name.)
        let alias = baml_compiler2_ppir::item_data::file_type_aliases(&db, file)
            .iter()
            .copied()
            .find(|&loc| {
                baml_compiler2_ppir::item_data::type_alias_data(&db, loc)
                    .name
                    .as_str()
                    == "Loop"
            })
            .expect("alias exists");
        assert_eq!(render(&type_alias_value(&db, alias)), "user.Loop[]");
    }
}
