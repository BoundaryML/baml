use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

mod common;
use anyhow::{anyhow, bail, Context, Result};
use baml_compiler::{hir, thir, watch::shared_noop_handler};
use baml_runtime::{async_vm_runtime::BamlAsyncVmRuntime, TripWire};
use baml_types::{BamlMap, BamlValue, BamlValueWithMeta};
use internal_baml_core::internal_baml_ast;
use internal_baml_diagnostics::{Diagnostics, SourceFile, Span};
use internal_baml_parser_database::ParserDatabase;
type ExprMetadata = thir::ExprMetadata;
use thir::interpret::interpret_thir;

/// Helper for constructing small in-memory `baml_src` folders for tests.
#[derive(Clone, Default)]
struct InMemoryBamlProject {
    root: String,
    files: HashMap<String, String>,
    env_vars: HashMap<String, String>,
}

impl InMemoryBamlProject {
    fn new() -> Self {
        Self {
            root: "baml_src".to_string(),
            files: HashMap::new(),
            env_vars: HashMap::new(),
        }
    }

    fn with_file(mut self, path: &str, contents: &str) -> Self {
        self.files.insert(path.to_string(), contents.to_string());
        self
    }

    fn parser_db(&self) -> Result<ParserDatabase> {
        let root_path = std::path::PathBuf::from(&self.root);
        let mut db = ParserDatabase::new();
        let mut diagnostics = Diagnostics::new(root_path.clone());

        for (relative_path, contents) in &self.files {
            let full_path = root_path.join(relative_path);
            let source = SourceFile::from((full_path.clone(), contents.clone()));
            match internal_baml_ast::parse(&root_path, &source) {
                Ok((ast, diag)) => {
                    diagnostics.push(diag);
                    db.add_ast(ast);
                }
                Err(diag) => diagnostics.push(diag),
            }
        }

        if let Err(diag) = db.validate(&mut diagnostics) {
            diagnostics.push(diag);
        }
        db.finalize(&mut diagnostics);

        if diagnostics.has_errors() {
            bail!(diagnostics.to_pretty_string());
        }

        Ok(db)
    }

    fn build_thir(&self) -> Result<thir::THir<ExprMetadata>> {
        let parser_db = self.parser_db()?;
        let hir = hir::Hir::from_ast(&parser_db.ast);
        let mut diagnostics = Diagnostics::new(std::path::PathBuf::from(&self.root));
        let thir = thir::typecheck::typecheck(&hir, &mut diagnostics);
        if diagnostics.has_errors() {
            bail!(diagnostics.to_pretty_string());
        }
        Ok(thir)
    }

    fn build_runtime(&self) -> Result<BamlAsyncVmRuntime> {
        BamlAsyncVmRuntime::from_file_content(&self.root, &self.files, self.env_vars.clone())
            .context("failed to create async VM runtime")
    }
}

#[derive(Clone)]
struct ExpressionCase {
    function: &'static str,
    args: BamlMap<String, BamlValue>,
    expected: Option<BamlValue>,
}

impl ExpressionCase {
    fn new(function: &'static str, args: BamlMap<String, BamlValue>) -> Self {
        Self {
            function,
            args,
            expected: None,
        }
    }

    fn with_expected(mut self, value: BamlValue) -> Self {
        self.expected = Some(value);
        self
    }
}

struct ConformanceSuite {
    project: InMemoryBamlProject,
    runtime: BamlAsyncVmRuntime,
    thir: thir::THir<ExprMetadata>,
}

impl ConformanceSuite {
    fn new(project: InMemoryBamlProject) -> Result<Self> {
        let runtime = project.build_runtime()?;
        let thir = project.build_thir()?;
        Ok(Self {
            project,
            runtime,
            thir,
        })
    }

    async fn assert_cases(&self, cases: &[ExpressionCase]) -> Result<()> {
        for case in cases {
            self.assert_case(case).await.context(format!(
                "conformance failure for expression function '{}'",
                case.function
            ))?;
        }
        Ok(())
    }

    async fn assert_case(&self, case: &ExpressionCase) -> Result<()> {
        let runtime_value = self.evaluate_runtime(case).await?;
        let interpreter_value = self.evaluate_interpreter(case).await?;

        if runtime_value != interpreter_value {
            bail!(
                "runtime and interpreter results differ: runtime={:?}, interpreter={:?}",
                runtime_value,
                interpreter_value
            );
        }

        if let Some(expected) = &case.expected {
            if &runtime_value != expected {
                bail!("expected {:?}, but got {:?}", expected, runtime_value);
            }
        }

        Ok(())
    }

    async fn evaluate_runtime(&self, case: &ExpressionCase) -> Result<BamlValue> {
        let params = case.args.clone();
        let ctx = self
            .runtime
            .create_ctx_manager(BamlValue::String("test".to_string()), None);

        let (result, _) = self
            .runtime
            .call_function(
                case.function.to_string(),
                &params,
                &ctx,
                None,
                None,
                None,
                self.project.env_vars.clone(),
                None,
                TripWire::new(None),
                None,
            )
            .await;

        let function_result = result?;
        let response = function_result.result_with_constraints_content()?;
        Ok(BamlValue::from(response.0.clone()))
    }

    async fn evaluate_interpreter(&self, case: &ExpressionCase) -> Result<BamlValue> {
        let thir = self.thir.clone();
        let expr_fn = thir
            .expr_functions
            .iter()
            .find(|f| f.name == case.function)
            .cloned()
            .ok_or_else(|| anyhow!("expression function '{}' not found", case.function))?;

        let mut args = Vec::with_capacity(expr_fn.parameters.len());
        for param in &expr_fn.parameters {
            let value = case
                .args
                .get(&param.name)
                .with_context(|| format!("missing argument '{}'", param.name))?;
            args.push(thir::Expr::Value(value_to_meta(value)));
        }

        let call = thir::Expr::Call {
            func: Arc::new(thir::Expr::Var(expr_fn.name, fake_meta())),
            type_args: vec![],
            args,
            meta: fake_meta(),
        };

        let result =
            interpret_thir(
                "test".to_string(),
                thir,
                call,
                |name, _args, _emit_context| async move {
                    bail!("unexpected LLM function call: {name}")
                },
                shared_noop_handler(),
                BamlMap::new(),
                self.project.env_vars.clone(),
            )
            .await?;

        Ok(BamlValue::from(result))
    }
}

fn fake_meta() -> ExprMetadata {
    (Span::fake(), None)
}

fn value_to_meta(value: &BamlValue) -> BamlValueWithMeta<ExprMetadata> {
    let meta = fake_meta();
    match value {
        BamlValue::Null => BamlValueWithMeta::Null(meta),
        BamlValue::Int(v) => BamlValueWithMeta::Int(*v, meta),
        BamlValue::Float(v) => BamlValueWithMeta::Float(*v, meta),
        BamlValue::Bool(v) => BamlValueWithMeta::Bool(*v, meta),
        BamlValue::String(v) => BamlValueWithMeta::String(v.clone(), meta),
        BamlValue::Map(entries) => {
            let converted = entries
                .iter()
                .map(|(k, v)| (k.clone(), value_to_meta(v)))
                .collect();
            BamlValueWithMeta::Map(converted, meta)
        }
        BamlValue::List(items) => {
            let converted = items.iter().map(value_to_meta).collect();
            BamlValueWithMeta::List(converted, meta)
        }
        BamlValue::Media(media) => BamlValueWithMeta::Media(media.clone(), meta),
        BamlValue::Enum(name, variant) => {
            BamlValueWithMeta::Enum(name.clone(), variant.clone(), meta)
        }
        BamlValue::Class(name, fields) => {
            let converted = fields
                .iter()
                .map(|(k, v)| (k.clone(), value_to_meta(v)))
                .collect();
            BamlValueWithMeta::Class(name.clone(), converted, meta)
        }
    }
}

fn bm<K: Into<String>, V>(pairs: impl IntoIterator<Item = (K, V)>) -> BamlMap<String, V> {
    pairs.into_iter().map(|(k, v)| (k.into(), v)).collect()
}

#[tokio::test]
async fn arithmetic_and_env_conformance() -> Result<()> {
    let project = InMemoryBamlProject::new().with_file(
        "main.baml",
        r#"
        function Add(a: int, b: int) -> int {
            a + b
        }

        function Multiply(x: int) -> int {
            x * 3
        }

        function Compose(a: int, b: int) -> int {
            Multiply(Add(a, b))
        }
    "#,
    );

    let suite = ConformanceSuite::new(project)?;
    let cases = vec![
        ExpressionCase::new(
            "Add",
            bm(vec![("a", BamlValue::Int(2)), ("b", BamlValue::Int(3))]),
        )
        .with_expected(BamlValue::Int(5)),
        ExpressionCase::new("Multiply", bm(vec![("x", BamlValue::Int(4))]))
            .with_expected(BamlValue::Int(12)),
        ExpressionCase::new(
            "Compose",
            bm(vec![("a", BamlValue::Int(5)), ("b", BamlValue::Int(7))]),
        )
        .with_expected(BamlValue::Int(36)),
    ];

    suite.assert_cases(&cases).await
}

#[tokio::test]
async fn class_and_control_flow_conformance() -> Result<()> {
    let project = InMemoryBamlProject::new().with_file(
        "main.baml",
        r#"
        class Point {
            x int
            y int
        }

        function MakePoint(x: int, y: int) -> Point {
            Point { x: x, y: y }
        }

        function Describe(point: Point) -> string {
            let result = if (point.x == point.y) {
                "square"
            } else {
                "rectangle"
            };

            result
        }
    "#,
    );

    let suite = ConformanceSuite::new(project)?;
    let point_equal = BamlValue::Class(
        "Point".to_string(),
        bm(vec![("x", BamlValue::Int(2)), ("y", BamlValue::Int(2))]),
    );
    let point_rect = BamlValue::Class(
        "Point".to_string(),
        bm(vec![("x", BamlValue::Int(2)), ("y", BamlValue::Int(3))]),
    );

    let cases = vec![
        ExpressionCase::new(
            "MakePoint",
            bm(vec![("x", BamlValue::Int(1)), ("y", BamlValue::Int(4))]),
        )
        .with_expected(BamlValue::Class(
            "Point".to_string(),
            bm(vec![("x", BamlValue::Int(1)), ("y", BamlValue::Int(4))]),
        )),
        ExpressionCase::new("Describe", bm(vec![("point", point_equal.clone())]))
            .with_expected(BamlValue::String("square".to_string())),
        ExpressionCase::new("Describe", bm(vec![("point", point_rect.clone())]))
            .with_expected(BamlValue::String("rectangle".to_string())),
    ];

    suite.assert_cases(&cases).await
}

#[tokio::test]
async fn update_through_reference_conformance() -> Result<()> {
    let project = InMemoryBamlProject::new().with_file(
        "main.baml",
        r#"
        class Foo {
            inner int
        }

        function Main() -> int {
            let x = Foo { inner: 1 };
            let y = x;
            y.inner = y.inner + 1;
            x.inner
        }
    "#,
    );

    let suite = ConformanceSuite::new(project)?;

    let cases = vec![ExpressionCase::new("Main", BamlMap::new())];

    suite.assert_cases(&cases).await
}
