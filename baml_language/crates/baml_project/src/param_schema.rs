//! Function-parameter schemas for the playground's dynamic args form.
//!
//! Converts a function's resolved parameter types ([`Ty`]) into a
//! self-contained, serializable [`FieldSchema`] tree: class fields and enum
//! variants are expanded inline (with a cycle guard — recursive types are
//! legal through optional/list/map positions), nullable unions fold into
//! `Optional`, type aliases resolve before dispatch, and every non-data
//! variant (functions, interfaces, type variables, TIR sentinels, …) degrades
//! to `Unsupported` rather than erroring: users hold invalid intermediate
//! states constantly while editing.
//!
//! Class and enum `name`s are the canonical dotted FQN the engine registers
//! and emits (`user.shapes.Foo` — [`QualifiedTypeName::render_dotted`] with
//! `user_facing = false`), so a `$baml: { type: name }` marker built from a
//! schema round-trips through the args wire protocol unchanged.

use baml_compiler2_hir::package::PackageId;
use baml_compiler2_tir::{
    package_interface::{ExportedType, PackageInterface, package_interface},
    ty::{FunctionParamMode, LiteralValue, QualifiedTypeName, Ty},
};
use baml_db::Name;
use serde::Serialize;

use crate::db::ProjectDatabase;

/// Guards runaway recursion through pathological (but legal) deeply-nested
/// types. Same bound as TIR's `CycleDetector`.
const MAX_DEPTH: usize = 64;

/// Schema for one function parameter.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ParamSchema {
    pub name: String,
    /// Whether the parameter has a default value and can be omitted entirely
    /// ([`FunctionParamMode::Optional`]). Distinct from a nullable type, which
    /// shows up as [`FieldSchema::Optional`] in `schema`.
    pub has_default: bool,
    pub schema: FieldSchema,
}

/// One class field; optionality is folded into `schema` as
/// [`FieldSchema::Optional`], not a flag here.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FieldSchemaField {
    pub name: String,
    pub schema: FieldSchema,
}

/// A self-contained, recursively-expanded type schema for form rendering.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum FieldSchema {
    String,
    Int,
    Float,
    Bool,
    Null,
    Bigint,
    /// `kind` is the BEP-038 tag (`image` / `audio` / `video` / `pdf` /
    /// `media`); rendered as a raw-JSON field for now.
    Media {
        kind: String,
    },
    /// A literal type — the parameter admits exactly this value.
    Literal {
        value: serde_json::Value,
    },
    Enum {
        name: String,
        values: Vec<String>,
    },
    Class {
        name: String,
        /// Empty when `recursive` — the cycle guard stops inline expansion on
        /// a repeated class; the form falls back to raw JSON for it.
        fields: Vec<FieldSchemaField>,
        recursive: bool,
    },
    List {
        item: Box<FieldSchema>,
    },
    Map {
        key: Box<FieldSchema>,
        value: Box<FieldSchema>,
    },
    /// A nullable union (`T?` / `T | null`), folded to its non-null payload.
    Optional {
        inner: Box<FieldSchema>,
    },
    Union {
        variants: Vec<FieldSchema>,
    },
    /// Anything the form cannot render as a typed widget; `display` is the
    /// user-facing type string for labeling the raw-JSON fallback field.
    Unsupported {
        display: String,
    },
}

/// Extract parameter schemas for the function `name` in `namespace_path` of
/// the user package. `None` means the function is missing from the package
/// interface (mid-edit inconsistency) — the UI treats it as "no schema
/// available", which is distinct from `Some(vec![])` ("takes no arguments").
pub(crate) fn function_param_schemas(
    db: &ProjectDatabase,
    iface: &PackageInterface,
    namespace_path: &[Name],
    name: &Name,
) -> Option<Vec<ParamSchema>> {
    let func = iface.lookup_function(namespace_path, name)?;
    let cx = SchemaCx {
        db,
        user_iface: iface,
    };
    let params = func
        .params
        .iter()
        .enumerate()
        .map(|(i, param)| ParamSchema {
            name: param
                .name
                .as_ref()
                .map_or_else(|| format!("arg{i}"), ToString::to_string),
            has_default: matches!(param.mode, FunctionParamMode::Optional),
            schema: cx.field_schema(&param.ty, &mut Vec::new(), 0),
        })
        .collect();
    Some(params)
}

struct SchemaCx<'db> {
    db: &'db ProjectDatabase,
    user_iface: &'db PackageInterface,
}

impl<'db> SchemaCx<'db> {
    /// Resolve a type name to its exported definition: own-package types via
    /// the user interface, dependency types (stdlib/builtins) via that
    /// package's own Salsa-cached interface. A miss is expected mid-edit and
    /// for undeclared packages — callers degrade to `Unsupported`.
    fn lookup_type(&self, qtn: &QualifiedTypeName) -> Option<&'db ExportedType> {
        if qtn.is_local() {
            self.user_iface.lookup_type(qtn.namespace(), qtn.name())
        } else {
            let pkg_id = PackageId::new(self.db, qtn.package().clone());
            package_interface(self.db, pkg_id).lookup_type(qtn.namespace(), qtn.name())
        }
    }

    /// `path` holds the class/alias names currently being expanded (cycle
    /// guard); `depth` counts every recursion step (structural nesting too).
    fn field_schema(
        &self,
        ty: &Ty,
        path: &mut Vec<QualifiedTypeName>,
        depth: usize,
    ) -> FieldSchema {
        if depth >= MAX_DEPTH {
            return unsupported(ty);
        }
        match ty {
            Ty::String { .. } => FieldSchema::String,
            Ty::Int { .. } => FieldSchema::Int,
            Ty::Float { .. } => FieldSchema::Float,
            Ty::Bool { .. } => FieldSchema::Bool,
            Ty::Null { .. } => FieldSchema::Null,
            Ty::Bigint { .. } => FieldSchema::Bigint,
            Ty::Media(kind, _) => FieldSchema::Media {
                kind: kind.tag_str().to_string(),
            },
            Ty::Literal(lit, _, _) => match literal_value(lit) {
                Some(value) => FieldSchema::Literal { value },
                None => unsupported(ty),
            },
            Ty::Enum(qtn, _) => match self.lookup_type(qtn) {
                Some(ExportedType::Enum { variants, .. }) => FieldSchema::Enum {
                    name: qtn.render_dotted(false),
                    values: variants.iter().map(ToString::to_string).collect(),
                },
                _ => unsupported(ty),
            },
            // A specific-variant type (`s: Status.Active`): a single-value
            // enum, so the form still emits a real enum wire value.
            Ty::EnumVariant(qtn, variant, _) => match self.lookup_type(qtn) {
                Some(ExportedType::Enum { .. }) => FieldSchema::Enum {
                    name: qtn.render_dotted(false),
                    values: vec![variant.to_string()],
                },
                _ => unsupported(ty),
            },
            Ty::Class(qtn, args, _) => {
                // Generic instantiations are out of scope: the `$baml` marker
                // encodes `typeArgs: []`, which the engine treats as unbound.
                if !args.is_empty() {
                    return unsupported(ty);
                }
                let name = qtn.render_dotted(false);
                if path.contains(qtn) {
                    return FieldSchema::Class {
                        name,
                        fields: Vec::new(),
                        recursive: true,
                    };
                }
                match self.lookup_type(qtn) {
                    Some(ExportedType::Class {
                        fields,
                        generic_params,
                        ..
                    }) if generic_params.is_empty() => {
                        path.push(qtn.clone());
                        let fields = fields
                            .iter()
                            .map(|(field_name, field_ty)| FieldSchemaField {
                                name: field_name.to_string(),
                                schema: self.field_schema(field_ty, path, depth + 1),
                            })
                            .collect();
                        path.pop();
                        FieldSchema::Class {
                            name,
                            fields,
                            recursive: false,
                        }
                    }
                    _ => unsupported(ty),
                }
            }
            // Only recursive aliases survive TIR lowering (non-recursive ones
            // are pre-expanded), so a revisit here is the common case, not the
            // exception.
            Ty::TypeAlias(qtn, _) => {
                if path.contains(qtn) {
                    return unsupported(ty);
                }
                match self.lookup_type(qtn) {
                    Some(ExportedType::TypeAlias { resolved, .. }) => {
                        path.push(qtn.clone());
                        let schema = self.field_schema(resolved, path, depth + 1);
                        path.pop();
                        schema
                    }
                    _ => unsupported(ty),
                }
            }
            Ty::List(item, _) => FieldSchema::List {
                item: Box::new(self.field_schema(item, path, depth + 1)),
            },
            Ty::Map { key, value, .. } => FieldSchema::Map {
                key: Box::new(self.field_schema(key, path, depth + 1)),
                value: Box::new(self.field_schema(value, path, depth + 1)),
            },
            Ty::Union(members, _) => {
                if ty.is_nullable_union() {
                    let stripped = ty.strip_null();
                    // `strip_null` returns the union unchanged when every
                    // member is null; the type is then just `null`.
                    if stripped.is_nullable_union() {
                        FieldSchema::Null
                    } else {
                        FieldSchema::Optional {
                            inner: Box::new(self.field_schema(&stripped, path, depth + 1)),
                        }
                    }
                } else {
                    FieldSchema::Union {
                        variants: members
                            .iter()
                            .map(|member| self.field_schema(member, path, depth + 1))
                            .collect(),
                    }
                }
            }
            // Everything non-data: functions, interfaces, type variables,
            // opaque runtime types, and the TIR sentinels (`Unknown`/`Error`/…)
            // that reliably appear while the user is mid-edit.
            _ => unsupported(ty),
        }
    }
}

fn unsupported(ty: &Ty) -> FieldSchema {
    FieldSchema::Unsupported {
        display: ty.render_user_facing(),
    }
}

fn literal_value(lit: &LiteralValue) -> Option<serde_json::Value> {
    match lit {
        LiteralValue::Int(i) => Some(serde_json::Value::from(*i)),
        LiteralValue::Float(s) => s
            .parse::<f64>()
            .ok()
            .and_then(serde_json::Number::from_f64)
            .map(serde_json::Value::Number),
        LiteralValue::String(s) => Some(serde_json::Value::String(s.clone())),
        LiteralValue::Bool(b) => Some(serde_json::Value::Bool(*b)),
        // A bigint literal can't be expressed in plain JSON args (JSON.parse
        // yields a lossy number, and the encoder's bigint path needs a JS
        // BigInt) — fall back to raw JSON.
        LiteralValue::Bigint(_) => None,
    }
}

#[cfg(test)]
mod tests {
    use serde_json::{Value, json};

    use crate::{db::ProjectDatabase, symbols::list_functions_with_metadata};

    fn db_with(files: &[(&str, &str)]) -> ProjectDatabase {
        let mut db = ProjectDatabase::new();
        db.set_project_root(std::path::Path::new("/tmp"));
        for (path, source) in files {
            let full = format!("/tmp/{path}");
            db.add_or_update_file(std::path::Path::new(&full), source);
        }
        db
    }

    /// The serialized `params` for `fn_name`, exactly as the playground
    /// notification will carry them.
    fn params_json(db: &ProjectDatabase, fn_name: &str) -> Value {
        let functions = list_functions_with_metadata(db);
        let function = functions
            .iter()
            .find(|f| f.name == fn_name)
            .unwrap_or_else(|| panic!("function {fn_name} not found"));
        serde_json::to_value(
            function
                .params
                .as_ref()
                .unwrap_or_else(|| panic!("no params extracted for {fn_name}")),
        )
        .unwrap()
    }

    #[test]
    fn primitives_lists_and_nullable_unions() {
        let db = db_with(&[(
            "main.baml",
            r#"
            function Prim(a: int, b: string, c: bool, d: float?, e: string[], f: map<string, float>) -> int { 1 }
            "#,
        )]);
        assert_eq!(
            params_json(&db, "Prim"),
            json!([
                { "name": "a", "hasDefault": false, "schema": { "type": "int" } },
                { "name": "b", "hasDefault": false, "schema": { "type": "string" } },
                { "name": "c", "hasDefault": false, "schema": { "type": "bool" } },
                { "name": "d", "hasDefault": false,
                  "schema": { "type": "optional", "inner": { "type": "float" } } },
                { "name": "e", "hasDefault": false,
                  "schema": { "type": "list", "item": { "type": "string" } } },
                { "name": "f", "hasDefault": false,
                  "schema": { "type": "map", "key": { "type": "string" },
                              "value": { "type": "float" } } },
            ])
        );
    }

    #[test]
    fn nullary_function_gets_empty_schema_not_none() {
        let db = db_with(&[("main.baml", "function Zero() -> int { 1 }")]);
        assert_eq!(params_json(&db, "Zero"), json!([]));
    }

    #[test]
    fn enum_and_nested_class_expand_inline() {
        let db = db_with(&[(
            "main.baml",
            r#"
            enum Color { Red Green Blue }
            class Nested { x int }
            class Person {
                name string
                age int?
                nested Nested
            }
            function F(p: Person, c: Color) -> int { 1 }
            "#,
        )]);
        assert_eq!(
            params_json(&db, "F"),
            json!([
                { "name": "p", "hasDefault": false, "schema": {
                    "type": "class", "name": "user.Person", "recursive": false,
                    "fields": [
                        { "name": "name", "schema": { "type": "string" } },
                        { "name": "age",
                          "schema": { "type": "optional", "inner": { "type": "int" } } },
                        { "name": "nested", "schema": {
                            "type": "class", "name": "user.Nested", "recursive": false,
                            "fields": [ { "name": "x", "schema": { "type": "int" } } ],
                        } },
                    ],
                } },
                { "name": "c", "hasDefault": false, "schema": {
                    "type": "enum", "name": "user.Color",
                    "values": ["Red", "Green", "Blue"],
                } },
            ])
        );
    }

    #[test]
    fn namespaced_class_uses_canonical_dotted_fqn() {
        let db = db_with(&[
            (
                "ns_shapes/shapes.baml",
                "class Box { w int }\nfunction Make(b: Box) -> int { 1 }",
            ),
            ("main.baml", "function Use(b: shapes.Box) -> int { 1 }"),
        ]);
        let expected_schema = json!({
            "type": "class", "name": "user.shapes.Box", "recursive": false,
            "fields": [ { "name": "w", "schema": { "type": "int" } } ],
        });
        assert_eq!(
            params_json(&db, "shapes.Make")[0]["schema"],
            expected_schema
        );
        assert_eq!(params_json(&db, "Use")[0]["schema"], expected_schema);
    }

    #[test]
    fn recursive_class_is_cut_at_the_cycle() {
        let db = db_with(&[(
            "main.baml",
            r#"
            class Tree {
                value int
                children Tree[]
            }
            function Walk(t: Tree) -> int { 1 }
            "#,
        )]);
        assert_eq!(
            params_json(&db, "Walk")[0]["schema"],
            json!({
                "type": "class", "name": "user.Tree", "recursive": false,
                "fields": [
                    { "name": "value", "schema": { "type": "int" } },
                    { "name": "children", "schema": { "type": "list", "item": {
                        "type": "class", "name": "user.Tree",
                        "recursive": true, "fields": [],
                    } } },
                ],
            })
        );
    }

    #[test]
    fn recursive_alias_expands_once_then_degrades() {
        let db = db_with(&[(
            "main.baml",
            r#"
            type JSON = string | JSON[]
            function G(j: JSON) -> int { 1 }
            "#,
        )]);
        let schema = &params_json(&db, "G")[0]["schema"];
        assert_eq!(schema["type"], "union");
        assert_eq!(schema["variants"][0], json!({ "type": "string" }));
        assert_eq!(schema["variants"][1]["type"], "list");
        assert_eq!(schema["variants"][1]["item"]["type"], "unsupported");
    }

    #[test]
    fn non_nullable_union_lists_variants() {
        let db = db_with(&[(
            "main.baml",
            "function U(x: int | string, y: (int | string)?) -> int { 1 }",
        )]);
        let params = params_json(&db, "U");
        assert_eq!(
            params[0]["schema"],
            json!({ "type": "union",
                    "variants": [ { "type": "int" }, { "type": "string" } ] })
        );
        assert_eq!(
            params[1]["schema"],
            json!({ "type": "optional", "inner": { "type": "union",
                    "variants": [ { "type": "int" }, { "type": "string" } ] } })
        );
    }

    #[test]
    fn unresolved_param_type_degrades_to_unsupported() {
        let db = db_with(&[("main.baml", "function Bad(x: Nope) -> int { 1 }")]);
        assert_eq!(params_json(&db, "Bad")[0]["schema"]["type"], "unsupported");
    }

    #[test]
    fn generic_param_degrades_to_unsupported() {
        let db = db_with(&[("main.baml", "function Id<T>(x: T) -> T { x }")]);
        assert_eq!(params_json(&db, "Id")[0]["schema"]["type"], "unsupported");
    }

    #[test]
    fn dependency_package_class_expands_through_its_interface() {
        let db = db_with(&[(
            "main.baml",
            "function D(d: baml.time.PlainDate) -> int { 1 }",
        )]);
        let schema = &params_json(&db, "D")[0]["schema"];
        assert_eq!(schema["type"], "class");
        assert_eq!(schema["name"], "baml.time.PlainDate");
        assert_eq!(schema["recursive"], false);
        assert_eq!(
            schema["fields"][0],
            json!({ "name": "_days", "schema": { "type": "int" } })
        );
    }

    #[test]
    fn media_param_carries_its_kind() {
        let db = db_with(&[("main.baml", "function Med(i: image) -> int { 1 }")]);
        assert_eq!(
            params_json(&db, "Med")[0]["schema"],
            json!({ "type": "media", "kind": "image" })
        );
    }

    #[test]
    fn param_with_default_sets_has_default() {
        let db = db_with(&[("main.baml", "function Def(x: int, y: int = 3) -> int { 1 }")]);
        let params = params_json(&db, "Def");
        assert_eq!(params[0]["hasDefault"], false);
        assert_eq!(params[1]["hasDefault"], true);
        assert_eq!(params[1]["schema"], json!({ "type": "int" }));
    }
}
