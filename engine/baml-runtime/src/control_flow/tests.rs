#![cfg(test)]

use std::{
    collections::{BTreeMap, HashMap},
    fs,
    path::{Path, PathBuf},
};

use baml_types::ir_type::{type_meta, TypeIR, TypeValue};
use insta::assert_yaml_snapshot;
use internal_baml_core::feature_flags::FeatureFlags;
use serde_json::{json, Value};

use super::{
    flatten::{hoist_branch_arms, inline_branch_arms_and_scopes, remove_implicit_nodes},
    mermaid::to_mermaid,
    *,
};
use crate::BamlRuntime;

fn simple_function() -> hir::ExprFunction {
    let span = Span::fake();
    let let_stmt = hir::Statement::Let {
        name: "x".into(),
        value: hir::Expression::NumericValue("1".into(), span.clone()),
        annotated_type: None,
        watch: None,
        span: span.clone(),
    };

    let return_expr = hir::Expression::Identifier("x".into(), span.clone());
    let return_stmt = hir::Statement::Return {
        expr: return_expr,
        span: span.clone(),
    };

    hir::ExprFunction {
        name: "Simple".into(),
        parameters: Vec::new(),
        return_type: TypeIR::Primitive(TypeValue::Int, type_meta::IR::default()),
        body: hir::Block {
            statements: vec![let_stmt, return_stmt],
            trailing_expr: None,
        },
        span,
    }
}

#[test]
fn builds_simple_expr_function() {
    let mut hir = hir::Hir::empty();
    hir.expr_functions.push(simple_function());

    let viz = build_from_hir(&hir, "Simple").expect("graph should build");

    assert!(viz
        .nodes
        .values()
        .any(|node| matches!(node.node_type, NodeType::FunctionRoot)));

    let root = viz
        .nodes
        .values()
        .find(|node| matches!(node.node_type, NodeType::FunctionRoot))
        .expect("root node");
    assert_eq!(viz.nodes.len(), 1);
    assert!(viz
        .edges_by_src
        .get(&root.id)
        .map(|edges| edges.is_empty())
        .unwrap_or(true));
}

#[test]
fn missing_function_errors() {
    let hir = hir::Hir::empty();
    let err = build_from_hir(&hir, "DoesNotExist").unwrap_err();
    assert!(format!("{err}").contains("DoesNotExist"));
}

#[test]
fn test_snapshots() {
    insta::glob!(
        // "baml-runtime/src/control_flow",
        "testdata",
        "*.baml",
        |relative| {
            let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join(relative);
            let target_name = Path::new(relative)
                .file_stem()
                .expect("fixture stem")
                .to_string_lossy()
                .to_string();

            let snapshot_value = match load_runtime_from_fixture(&fixture) {
                Ok(runtime) => {
                    let hir = hir::Hir::from_ast(&runtime.db.ast);
                    let has_function = hir
                        .expr_functions
                        .iter()
                        .any(|func| func.name == target_name)
                        || hir
                            .llm_functions
                            .iter()
                            .any(|func| func.name == target_name);

                    if !has_function {
                        json!({
                            "__error": format!(
                                "function `{}` not found in fixture",
                                target_name
                            ),
                        })
                    } else {
                        match build_from_hir(&hir, &target_name) {
                            Ok(viz) => {
                                let pass1 = remove_implicit_nodes(&viz);
                                let pass2 = hoist_branch_arms(&pass1);
                                let pass3 = inline_branch_arms_and_scopes(&pass2);

                                json!({
                                    "hir": format!("{:#?}", &hir.expr_functions.iter().find(|f| f.name == target_name).map(|f| &f.body)),
                                    "expr": viz_snapshot(&viz),
                                    "mermaid": to_mermaid(&viz),
                                    "flattening": {
                                        "pass1_remove_implicit": flatten_stage_snapshot("Remove implicit nodes", &pass1),
                                        "pass2_hoist_branch_arms": flatten_stage_snapshot("Hoist branch arms", &pass2),
                                        "pass3_flatten_scopes": flatten_stage_snapshot("Flatten branch arms & scopes", &pass3),
                                    },
                                })
                            }
                            Err(err) => json!({ "__error": format!("{err}") }),
                        }
                    }
                }
                Err(err) => json!({ "__error": format!("{err}") }),
            };

            let snapshot_name = target_name.replace([' ', '-'], "_");

            assert_yaml_snapshot!(format!("headers__{}", snapshot_name), snapshot_value);
        }
    );
}

fn flatten_stage_snapshot(stage: &str, viz: &ControlFlowVisualization) -> Value {
    let viz_json = viz_snapshot(viz);
    json!({
        "label": stage,
        "expr": viz_json.clone(),
        "json": viz_json,
        "mermaid": to_mermaid(viz),
    })
}

fn load_runtime_from_fixture(path: &Path) -> anyhow::Result<BamlRuntime> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let relative = path
        .strip_prefix(&root)
        .unwrap_or(path)
        .to_string_lossy()
        .to_string();

    let mut files = HashMap::new();
    let contents = fs::read_to_string(path)?;
    files.insert(relative, contents);

    BamlRuntime::from_file_content(
        root.to_str().expect("manifest dir to str"),
        &files,
        HashMap::<String, String>::new(),
        FeatureFlags::default(),
    )
}

fn viz_snapshot(viz: &ControlFlowVisualization) -> Value {
    let mut nodes = BTreeMap::new();
    for node in viz.nodes.values() {
        nodes.insert(
            node.id.encode(),
            json!({
                "label": node.label,
                "log_filter_key": node.log_filter_key,
                "node_type": describe_node_type(&node.node_type),
                "parent": node.parent_node_id.as_ref().map(|id| id.encode()),
                "span": {
                    "file": node.span.file_name(),
                    "start": node.span.start,
                    "end": node.span.end,
                }
            }),
        );
    }

    let mut edges = BTreeMap::new();
    for (src, list) in &viz.edges_by_src {
        let mut dests: Vec<_> = list.iter().map(|edge| edge.dst.encode()).collect();
        dests.sort();
        edges.insert(src.encode(), dests);
    }

    json!({
        "nodes": nodes,
        "edges": edges,
    })
}
