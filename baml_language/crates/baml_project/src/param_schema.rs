//! Function-parameter schemas for the playground's dynamic args form.
//!
//! Converts a function's resolved parameter types ([`Ty`]) into a serializable
//! [`FieldSchema`] tree plus a shared per-project type table: named types
//! (classes, enums, aliases) are emitted once into the table and referenced by
//! [`FieldSchema::Ref`], so the wire payload is proportional to the number of
//! distinct types, not the number of paths through the type graph — aliases
//! included, since inlining them would re-expand the target per reference
//! site and blow up on alias DAGs exactly like the class-graph case. Nullable
//! unions fold into `Optional`, and every non-data variant (functions,
//! interfaces, type variables, TIR sentinels, …) degrades to `Unsupported`
//! rather than erroring: users hold invalid intermediate states constantly
//! while editing.
//!
//! Table keys and type `name`s are the canonical dotted FQN the engine
//! registers and emits (`user.shapes.Foo` — [`QualifiedTypeName::render_dotted`]
//! with `user_facing = false`), so a `$baml: { type: name }` marker built from
//! a schema round-trips through the args wire protocol unchanged.

use std::collections::BTreeMap;

use baml_base::Literal as LiteralValue;
use baml_compiler2_hir::{loc::FunctionLoc, package::PackageId};
use baml_compiler2_hir_ty::package_interface::{ExportedType, PackageInterface, package_interface};
use baml_db::Name;
use baml_type::{FunctionParamMode, QualifiedTypeName, Ty};
use serde::Serialize;

use crate::db::ProjectDatabase;

/// Bounds recursion through deeply-nested anonymous types
/// (`map<string, map<string, …>>`) **and** long acyclic named-type chains:
/// class/alias bodies expand nested within the referencing frame, so without
/// a shared bound a generated chain of thousands of classes would overflow
/// the stack (the WASM worker's is ~1 MB). Memoization still makes each named
/// type expand at most once — a body first reached past the bound just bakes
/// `Unsupported` tails into its table entry. Same bound as TIR's
/// `CycleDetector`.
const MAX_DEPTH: usize = 64;

/// The compiler appends a synthetic trailing `client: ai.Client? = null`
/// parameter to every LLM function. `client` is a reserved parameter name on LLM functions
/// (`reject_reserved_llm_params`), so a trailing param with this name
/// can only be the injected one. The form must not render it.
const INJECTED_CLIENT_PARAM_NAME: &str = "client";

/// Schema for one function parameter.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ParamSchema {
    pub name: String,
    /// Whether the parameter has a default value and can be omitted entirely
    /// ([`FunctionParamMode::Optional`]). Distinct from a nullable type, which
    /// shows up as [`FieldSchema::Optional`] in `schema`.
    pub has_default: bool,
    /// The exact, unevaluated source text of the parameter's default expression.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_expression: Option<String>,
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

/// A named type's definition in the per-project table
/// (`ProjectUpdate.types`), keyed by canonical dotted FQN.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum TypeSchema {
    Class { fields: Vec<FieldSchemaField> },
    Enum { values: Vec<String> },
    Alias { schema: FieldSchema },
}

/// A type schema for form rendering. Named types are [`FieldSchema::Ref`]s
/// into the type table; the entry's kind discriminates class/enum/alias.
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
    /// A reference to a named type in the table. A dangling name (mid-edit
    /// inconsistency) degrades to raw JSON in the UI.
    Ref {
        name: String,
    },
    /// A specific-variant type (`s: Status.Active`) — self-contained (no table
    /// entry needed) so the form can emit the enum wire marker directly.
    EnumVariant {
        name: String,
        value: String,
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
/// the user package, inserting every named type it references into `table`
/// (shared across all functions of a project update). `None` means the
/// function is missing from the package interface (mid-edit inconsistency) —
/// the UI treats it as "no schema available", which is distinct from
/// `Some(vec![])` ("takes no arguments").
pub(crate) fn function_param_schemas(
    db: &ProjectDatabase,
    function: FunctionLoc<'_>,
    iface: &PackageInterface,
    namespace_path: &[Name],
    name: &Name,
    is_llm: bool,
    table: &mut BTreeMap<String, TypeSchema>,
) -> Option<Vec<ParamSchema>> {
    let func = iface.lookup_function(namespace_path, name)?;
    let mut params = func.params.as_slice();
    if is_llm && let Some((last, rest)) = params.split_last() {
        let is_injected_client = last
            .name
            .as_ref()
            .is_some_and(|n| n.as_str() == INJECTED_CLIENT_PARAM_NAME);
        if is_injected_client {
            params = rest;
        }
    }
    let mut cx = SchemaCx {
        db,
        user_iface: iface,
        table,
    };
    let parameter_defaults = baml_compiler2_ppir::function_parameter_defaults(db, function);
    let source = function.file(db).text(db);
    let params = params
        .iter()
        .enumerate()
        .map(|(i, param)| ParamSchema {
            name: param
                .name
                .as_ref()
                .map_or_else(|| format!("arg{i}"), ToString::to_string),
            has_default: matches!(param.mode, FunctionParamMode::Optional),
            default_expression: parameter_defaults.param_default(i).map(|default_ref| {
                let range = parameter_defaults
                    .defaults
                    .source_map
                    .expr_span(default_ref.expr.expr());
                source[usize::from(range.start())..usize::from(range.end())].to_owned()
            }),
            schema: cx.field_schema(&param.ty, 0),
        })
        .collect();
    Some(params)
}

struct SchemaCx<'db, 't> {
    db: &'db ProjectDatabase,
    user_iface: &'db PackageInterface,
    /// Named types encountered so far, shared across every function of the
    /// project update. Doubles as the occurs-check for recursive types: a
    /// placeholder entry goes in before a body expands, so self-references
    /// resolve to a `Ref` instead of recursing.
    table: &'t mut BTreeMap<String, TypeSchema>,
}

impl<'db> SchemaCx<'db, '_> {
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

    /// `depth` counts every recursion step — structural nesting and named-type
    /// body expansion alike — so native stack use is bounded by `MAX_DEPTH`
    /// regardless of how the type graph is shaped.
    fn field_schema(&mut self, ty: &Ty, depth: usize) -> FieldSchema {
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
                Some(ExportedType::Enum { variants, .. }) => {
                    let name = qtn.render_dotted(false);
                    let values = variants.iter().map(ToString::to_string).collect();
                    self.table
                        .entry(name.clone())
                        .or_insert(TypeSchema::Enum { values });
                    FieldSchema::Ref { name }
                }
                _ => unsupported(ty),
            },
            Ty::EnumVariant(qtn, variant, _) => match self.lookup_type(qtn) {
                Some(ExportedType::Enum { .. }) => FieldSchema::EnumVariant {
                    name: qtn.render_dotted(false),
                    value: variant.to_string(),
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
                if self.table.contains_key(&name) {
                    return FieldSchema::Ref { name };
                }
                match self.lookup_type(qtn) {
                    Some(ExportedType::Class {
                        fields,
                        generic_params,
                        ..
                    }) if generic_params.is_empty() => {
                        // Placeholder before expanding the body: recursive
                        // references hit the contains_key check above and
                        // resolve to a Ref, so each class expands exactly once.
                        self.table
                            .insert(name.clone(), TypeSchema::Class { fields: Vec::new() });
                        let fields = fields
                            .iter()
                            .map(|(field_name, field_ty, _attrs)| FieldSchemaField {
                                name: field_name.to_string(),
                                schema: self.field_schema(field_ty, depth + 1),
                            })
                            .collect();
                        self.table
                            .insert(name.clone(), TypeSchema::Class { fields });
                        FieldSchema::Ref { name }
                    }
                    _ => unsupported(ty),
                }
            }
            // Aliases are never pre-expanded by TIR lowering
            // (`lower_type_expr_in_ns` emits `Ty::TypeAlias` for every alias
            // reference), so this is the common path. Aliases are memoized
            // into the table exactly like classes — inlining would re-expand
            // the target per reference site, which blows up on alias DAGs
            // just like the class-graph case.
            Ty::TypeAlias(qtn, _) => {
                let name = qtn.render_dotted(false);
                if self.table.contains_key(&name) {
                    return FieldSchema::Ref { name };
                }
                match self.lookup_type(qtn) {
                    Some(ExportedType::TypeAlias { resolved, .. }) => {
                        // Placeholder before expanding the target: recursive
                        // references hit the contains_key check above, so each
                        // alias expands exactly once.
                        self.table.insert(
                            name.clone(),
                            TypeSchema::Alias {
                                schema: FieldSchema::Null,
                            },
                        );
                        let schema = self.field_schema(resolved, depth + 1);
                        self.table
                            .insert(name.clone(), TypeSchema::Alias { schema });
                        FieldSchema::Ref { name }
                    }
                    _ => unsupported(ty),
                }
            }
            Ty::List(item, _) => FieldSchema::List {
                item: Box::new(self.field_schema(item, depth + 1)),
            },
            Ty::Map { key, value, .. } => FieldSchema::Map {
                key: Box::new(self.field_schema(key, depth + 1)),
                value: Box::new(self.field_schema(value, depth + 1)),
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
                            inner: Box::new(self.field_schema(&stripped, depth + 1)),
                        }
                    }
                } else {
                    FieldSchema::Union {
                        variants: members
                            .iter()
                            .map(|member| self.field_schema(member, depth + 1))
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

    use crate::{
        db::ProjectDatabase,
        symbols::{FunctionListing, list_functions_with_metadata},
    };

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
    fn params_json(listing: &FunctionListing, fn_name: &str) -> Value {
        let function = listing
            .functions
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

    fn types_json(listing: &FunctionListing) -> Value {
        serde_json::to_value(&listing.types).unwrap()
    }

    #[test]
    fn primitives_lists_and_nullable_unions() {
        let db = db_with(&[(
            "main.baml",
            r#"
            function Prim(a: int, b: string, c: bool, d: float?, e: string[], f: map<string, float>) -> int { 1 }
            "#,
        )]);
        let listing = list_functions_with_metadata(&db);
        assert_eq!(
            params_json(&listing, "Prim"),
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
        assert_eq!(types_json(&listing), json!({}));
    }

    #[test]
    fn nullary_function_gets_empty_schema_not_none() {
        let db = db_with(&[("main.baml", "function Zero() -> int { 1 }")]);
        let listing = list_functions_with_metadata(&db);
        assert_eq!(params_json(&listing, "Zero"), json!([]));
    }

    #[test]
    fn enum_and_nested_class_become_table_refs() {
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
        let listing = list_functions_with_metadata(&db);
        assert_eq!(
            params_json(&listing, "F"),
            json!([
                { "name": "p", "hasDefault": false,
                  "schema": { "type": "ref", "name": "user.Person" } },
                { "name": "c", "hasDefault": false,
                  "schema": { "type": "ref", "name": "user.Color" } },
            ])
        );
        assert_eq!(
            types_json(&listing),
            json!({
                "user.Color": { "kind": "enum", "values": ["Red", "Green", "Blue"] },
                "user.Nested": { "kind": "class", "fields": [
                    { "name": "x", "schema": { "type": "int" } },
                ] },
                "user.Person": { "kind": "class", "fields": [
                    { "name": "name", "schema": { "type": "string" } },
                    { "name": "age",
                      "schema": { "type": "optional", "inner": { "type": "int" } } },
                    { "name": "nested", "schema": { "type": "ref", "name": "user.Nested" } },
                ] },
            })
        );
    }

    #[test]
    fn shared_class_gets_one_table_entry_across_functions() {
        let db = db_with(&[(
            "main.baml",
            r#"
            class Shared { x int }
            function A(s: Shared) -> int { 1 }
            function B(s: Shared, t: Shared) -> int { 1 }
            "#,
        )]);
        let listing = list_functions_with_metadata(&db);
        let expected_ref = json!({ "type": "ref", "name": "user.Shared" });
        assert_eq!(params_json(&listing, "A")[0]["schema"], expected_ref);
        assert_eq!(params_json(&listing, "B")[0]["schema"], expected_ref);
        assert_eq!(params_json(&listing, "B")[1]["schema"], expected_ref);
        assert_eq!(
            types_json(&listing),
            json!({ "user.Shared": { "kind": "class", "fields": [
                { "name": "x", "schema": { "type": "int" } },
            ] } })
        );
    }

    #[test]
    fn diamond_dag_payload_is_linear_in_types() {
        // The P0.1 reproducer: 13 classes, each with 3 fields of the next —
        // inline expansion serialized this to 88 MB of params JSON. With the
        // table it must stay a few KB, ∝ distinct types.
        use std::fmt::Write;
        let mut src = String::new();
        for i in 0..12 {
            let next = i + 1;
            writeln!(src, "class C{i} {{ a C{next} b C{next} c C{next} }}").unwrap();
        }
        src.push_str("class C12 { x int }\nfunction F(p: C0) -> int { 1 }\n");
        let db = db_with(&[("main.baml", &src)]);
        let listing = list_functions_with_metadata(&db);
        let params_bytes = serde_json::to_string(&params_json(&listing, "F"))
            .unwrap()
            .len();
        let types_bytes = serde_json::to_string(&types_json(&listing)).unwrap().len();
        let total = params_bytes + types_bytes;
        assert!(total < 8 * 1024, "params+types serialized to {total} bytes");
        assert_eq!(
            params_json(&listing, "F")[0]["schema"],
            json!({ "type": "ref", "name": "user.C0" })
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
        let listing = list_functions_with_metadata(&db);
        let expected_ref = json!({ "type": "ref", "name": "user.shapes.Box" });
        assert_eq!(
            params_json(&listing, "shapes.Make")[0]["schema"],
            expected_ref
        );
        assert_eq!(params_json(&listing, "Use")[0]["schema"], expected_ref);
        assert_eq!(
            types_json(&listing)["user.shapes.Box"],
            json!({ "kind": "class", "fields": [
                { "name": "w", "schema": { "type": "int" } },
            ] })
        );
    }

    #[test]
    fn recursive_class_refers_to_itself_through_the_table() {
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
        let listing = list_functions_with_metadata(&db);
        assert_eq!(
            params_json(&listing, "Walk")[0]["schema"],
            json!({ "type": "ref", "name": "user.Tree" })
        );
        assert_eq!(
            types_json(&listing)["user.Tree"],
            json!({ "kind": "class", "fields": [
                { "name": "value", "schema": { "type": "int" } },
                { "name": "children", "schema": {
                    "type": "list", "item": { "type": "ref", "name": "user.Tree" },
                } },
            ] })
        );
    }

    #[test]
    fn recursive_alias_gets_a_table_entry() {
        let db = db_with(&[(
            "main.baml",
            r#"
            type JSON = string | JSON[]
            function G(j: JSON) -> int { 1 }
            "#,
        )]);
        let listing = list_functions_with_metadata(&db);
        assert_eq!(
            params_json(&listing, "G")[0]["schema"],
            json!({ "type": "ref", "name": "user.JSON" })
        );
        assert_eq!(
            types_json(&listing)["user.JSON"],
            json!({ "kind": "alias", "schema": { "type": "union", "variants": [
                { "type": "string" },
                { "type": "list", "item": { "type": "ref", "name": "user.JSON" } },
            ] } })
        );
    }

    #[test]
    fn non_recursive_alias_gets_a_table_entry_too() {
        // Aliases are memoized like classes — inlining would re-expand the
        // target per reference site (exponential on alias DAGs).
        let db = db_with(&[(
            "main.baml",
            "type Age = int\nfunction H(a: Age) -> int { 1 }",
        )]);
        let listing = list_functions_with_metadata(&db);
        assert_eq!(
            params_json(&listing, "H")[0]["schema"],
            json!({ "type": "ref", "name": "user.Age" })
        );
        assert_eq!(
            types_json(&listing),
            json!({ "user.Age": { "kind": "alias", "schema": { "type": "int" } } })
        );
    }

    #[test]
    fn alias_dag_payload_is_linear_in_types() {
        // Alias twin of the diamond-DAG reproducer: each alias references the
        // previous one twice. Inline expansion made this ∝ 2^15 paths
        // (measured 1.9 MB / 4 s); memoized it must stay a few KB.
        use std::fmt::Write;
        let mut src = String::from("type A0 = int\n");
        for i in 1..=14 {
            let prev = i - 1;
            writeln!(src, "type A{i} = A{prev}[] | map<string, A{prev}>").unwrap();
        }
        src.push_str("function F(a: A14) -> int { 1 }\n");
        let db = db_with(&[("main.baml", &src)]);
        let listing = list_functions_with_metadata(&db);
        let total = serde_json::to_string(&params_json(&listing, "F"))
            .unwrap()
            .len()
            + serde_json::to_string(&types_json(&listing)).unwrap().len();
        assert!(total < 8 * 1024, "params+types serialized to {total} bytes");
        assert_eq!(
            params_json(&listing, "F")[0]["schema"],
            json!({ "type": "ref", "name": "user.A14" })
        );
    }

    #[test]
    fn long_acyclic_class_chain_is_depth_bounded() {
        // Class/alias bodies expand nested within the referencing frame, so
        // MAX_DEPTH must count named-type hops too: a generated chain of
        // thousands of classes would otherwise recurse one native frame per
        // hop and overflow the stack (the WASM worker's is ~1 MB). Past the
        // bound the tail degrades to Unsupported instead.
        use std::fmt::Write;
        let mut src = String::new();
        for i in 0..100 {
            let next = i + 1;
            writeln!(src, "class C{i} {{ a C{next} }}").unwrap();
        }
        src.push_str("class C100 { x int }\nfunction F(p: C0) -> int { 1 }\n");
        let db = db_with(&[("main.baml", &src)]);
        let listing = list_functions_with_metadata(&db);
        assert_eq!(
            params_json(&listing, "F")[0]["schema"],
            json!({ "type": "ref", "name": "user.C0" })
        );
        assert!(
            listing.types.len() < 100,
            "expected the chain to be cut at MAX_DEPTH, got {} entries",
            listing.types.len()
        );
        let cut = serde_json::to_string(&types_json(&listing)).unwrap();
        assert!(
            cut.contains("\"unsupported\""),
            "expected an unsupported tail"
        );
    }

    #[test]
    fn pure_alias_cycle_degrades_to_mutual_refs() {
        // `type A = B; type B = A` compiles clean; the table ties the cycle
        // with mutual refs, which the UI resolves to the raw-JSON fallback.
        let db = db_with(&[(
            "main.baml",
            "type A = B\ntype B = A\nfunction C(x: A) -> int { 1 }",
        )]);
        let listing = list_functions_with_metadata(&db);
        assert_eq!(
            params_json(&listing, "C")[0]["schema"],
            json!({ "type": "ref", "name": "user.A" })
        );
        assert_eq!(
            types_json(&listing),
            json!({
                "user.A": { "kind": "alias", "schema": { "type": "ref", "name": "user.B" } },
                "user.B": { "kind": "alias", "schema": { "type": "ref", "name": "user.A" } },
            })
        );
    }

    #[test]
    fn enum_variant_param_is_self_contained() {
        let db = db_with(&[(
            "main.baml",
            r#"
            enum Status { Active Inactive }
            function V(s: Status.Active) -> int { 1 }
            "#,
        )]);
        let listing = list_functions_with_metadata(&db);
        assert_eq!(
            params_json(&listing, "V")[0]["schema"],
            json!({ "type": "enumVariant", "name": "user.Status", "value": "Active" })
        );
    }

    #[test]
    fn non_nullable_union_lists_variants() {
        let db = db_with(&[(
            "main.baml",
            "function U(x: int | string, y: (int | string)?) -> int { 1 }",
        )]);
        let listing = list_functions_with_metadata(&db);
        let params = params_json(&listing, "U");
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
        let listing = list_functions_with_metadata(&db);
        assert_eq!(
            params_json(&listing, "Bad")[0]["schema"]["type"],
            "unsupported"
        );
    }

    #[test]
    fn generic_param_degrades_to_unsupported() {
        let db = db_with(&[("main.baml", "function Id<T>(x: T) -> T { x }")]);
        let listing = list_functions_with_metadata(&db);
        assert_eq!(
            params_json(&listing, "Id")[0]["schema"]["type"],
            "unsupported"
        );
    }

    #[test]
    fn dependency_package_class_expands_through_its_interface() {
        let db = db_with(&[(
            "main.baml",
            "function D(d: baml.time.PlainDate) -> int { 1 }",
        )]);
        let listing = list_functions_with_metadata(&db);
        assert_eq!(
            params_json(&listing, "D")[0]["schema"],
            json!({ "type": "ref", "name": "baml.time.PlainDate" })
        );
        let entry = &types_json(&listing)["baml.time.PlainDate"];
        assert_eq!(entry["kind"], "class");
        assert_eq!(
            entry["fields"][0],
            json!({ "name": "_days", "schema": { "type": "int" } })
        );
    }

    #[test]
    fn media_param_carries_its_kind() {
        let db = db_with(&[("main.baml", "function Med(i: image) -> int { 1 }")]);
        let listing = list_functions_with_metadata(&db);
        assert_eq!(
            params_json(&listing, "Med")[0]["schema"],
            json!({ "type": "media", "kind": "image" })
        );
    }

    #[test]
    fn param_with_default_sets_has_default() {
        let db = db_with(&[(
            "main.baml",
            "function pair(a: int, b: int) -> int { a + b }\nfunction Def(x: int, y: int = pair(b = 2, a = 1)) -> int { 1 }",
        )]);
        let listing = list_functions_with_metadata(&db);
        let params = params_json(&listing, "Def");
        assert_eq!(params[0]["hasDefault"], false);
        assert_eq!(params[1]["hasDefault"], true);
        assert_eq!(params[1]["defaultExpression"], "pair(b = 2, a = 1)");
        assert_eq!(params[1]["schema"], json!({ "type": "int" }));
    }

    const LLM_FIXTURE: &str = r##"
client GPT4 = openai.ResponsesClient.new(model = "gpt-4o");

function Extract(text: string) -> string {
  client: GPT4
  prompt: `${text} ${ctx.output_format()}`
}

function Plain(x: int) -> int { x }
"##;

    #[test]
    fn injected_client_param_is_dropped_from_llm_functions() {
        let db = db_with(&[("main.baml", LLM_FIXTURE)]);
        let listing = list_functions_with_metadata(&db);
        // Only the user-declared param survives; the compiler-injected
        // trailing `client: ai.Client?` must not reach the form.
        assert_eq!(
            params_json(&listing, "Extract"),
            json!([
                { "name": "text", "hasDefault": false, "schema": { "type": "string" } },
            ])
        );
        // Expr functions are unaffected.
        assert_eq!(
            params_json(&listing, "Plain"),
            json!([
                { "name": "x", "hasDefault": false, "schema": { "type": "int" } },
            ])
        );
    }

    /// Pins the exact wire shape of `params` + `types` against the golden
    /// fixture that the TS side (`param-schema-golden.test.ts` in
    /// pkg-playground) validates against its `worker-protocol.ts` mirror —
    /// the FQN and shape contracts are otherwise enforced only by convention.
    /// On drift, update the fixture and both mirrors together.
    #[test]
    fn wire_shape_matches_the_ts_golden_fixture() {
        let db = db_with(&[(
            "main.baml",
            r#"
            enum Color { Red Green Blue }
            class Nested { x int }
            class Person {
                name string
                age int?
                color Color
                nested Nested
            }
            type JSON = string | JSON[]
            enum Status { Active Inactive }
            function Golden(p: Person, c: Color, j: JSON, s: Status.Active, l: string[], m: map<string, float>, u: int | string, i: image, x: int = 3) -> int { 1 }
            "#,
        )]);
        let listing = list_functions_with_metadata(&db);
        let actual = serde_json::json!({
            "params": params_json(&listing, "Golden"),
            "types": types_json(&listing),
        });
        let golden: serde_json::Value = serde_json::from_str(include_str!(
            "../../../../typescript2/pkg-playground/src/__fixtures__/param-schema-golden.json"
        ))
        .expect("golden fixture should contain valid JSON");
        assert_eq!(
            actual,
            golden,
            "wire shape drifted from the golden fixture; actual:\n{}",
            serde_json::to_string_pretty(&actual).unwrap()
        );
    }

    #[test]
    fn extraction_is_skipped_for_sub_functions_and_non_user_origins() {
        let db = db_with(&[("main.baml", LLM_FIXTURE)]);
        let listing = list_functions_with_metadata(&db);
        let skipped: Vec<_> = listing
            .functions
            .iter()
            .filter(|f| f.is_sub_function || f.origin != crate::FunctionOrigin::UserDefined)
            .collect();
        assert!(
            !skipped.is_empty(),
            "fixture should produce companion/internal functions"
        );
        for function in skipped {
            assert!(
                function.params.is_none(),
                "expected no schema for {}",
                function.name
            );
        }
    }
}
