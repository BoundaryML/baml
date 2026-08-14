//! Experimental, one-shot GraphQL queries over a stable BAML source snapshot.

use std::{
    collections::BTreeMap,
    io::{IsTerminal as _, Read as _, Write as _},
    path::{Path, PathBuf},
};

use anyhow::{Context as _, Result};
use baml_db::{
    SourceFile,
    baml_compiler2_ast::ast::{
        DeclarativeMeta, FunctionDef, FunctionOrigin, Item, LetOrigin, RawAttribute, TestArgValue,
        TypeExpr, TypeExprKind,
    },
    baml_compiler2_hir,
};
use baml_project::ProjectDatabase;
use clap::Args;
use juniper::{
    DefaultScalarValue, EmptyMutation, EmptySubscription, GraphQLEnum, GraphQLObject, RootNode,
    http::{GraphQLRequest, GraphQLResponse},
};
use serde_json::{Value as JsonValue, json};
use text_size::TextRange;

/// Query a BAML project's source model with GraphQL.
#[derive(Args, Clone, Debug)]
#[command(after_long_help = "\
Examples:
  Query classes and fields:
    baml graphql --query '{ classes { name fields { name type { display } } } }'

  Use variables:
    baml graphql --query 'query Find($name: String!) { functions(name: $name) { qualifiedName } }' --variables '{\"name\":\"Extract\"}'

  Read a document from a file:
    baml graphql --query-file ./queries/project.graphql

  Read a document from stdin:
    printf '%s\\n' '{ files { path definitions { kind name } } }' | baml graphql

  Print the schema:
    baml graphql --schema

  Print standard introspection JSON:
    baml graphql --introspect")]
pub struct GraphqlArgs {
    /// GraphQL document to execute.
    #[arg(long, value_name = "DOCUMENT", conflicts_with_all = ["query_file", "schema", "introspect"])]
    pub query: Option<String>,

    /// Read the GraphQL document from this file.
    #[arg(long, value_name = "PATH", conflicts_with_all = ["query", "schema", "introspect"])]
    pub query_file: Option<PathBuf>,

    /// JSON object containing GraphQL variables.
    #[arg(long, value_name = "JSON", conflicts_with_all = ["schema", "introspect"])]
    pub variables: Option<String>,

    /// Named operation to execute when the document contains more than one.
    #[arg(long, value_name = "NAME", conflicts_with_all = ["schema", "introspect"])]
    pub operation: Option<String>,

    /// Print the deterministic GraphQL schema in SDL format.
    #[arg(long, conflicts_with = "introspect")]
    pub schema: bool,

    /// Print the standard GraphQL introspection response as JSON.
    #[arg(long, conflicts_with = "schema")]
    pub introspect: bool,

    /// Deprecated alias for `--project`.
    #[arg(long, value_name = "PATH", hide = true)]
    pub from: Option<PathBuf>,
}

impl GraphqlArgs {
    pub fn run(&self) -> Result<crate::ExitCode> {
        match self.run_inner() {
            Ok(code) => Ok(code),
            Err(error) => {
                write_error_response("GRAPHQL_COMMAND_FAILED", &format!("{error:#}"), None)?;
                Ok(crate::ExitCode::Other)
            }
        }
    }

    fn run_inner(&self) -> Result<crate::ExitCode> {
        let schema = schema();
        if self.schema {
            let stdout = std::io::stdout();
            let mut stdout = stdout.lock();
            write!(stdout, "{}", schema.as_sdl()).context("failed to write GraphQL schema")?;
            return Ok(crate::ExitCode::Success);
        }

        if self.introspect {
            let context = GraphqlContext::empty();
            let response = GraphQLResponse::from_result(juniper::introspect(
                &schema,
                &context,
                juniper::IntrospectionFormat::All,
            ));
            return write_graphql_response(&response);
        }

        let query = self.read_query()?;
        let variables = match self.parse_variables() {
            Ok(variables) => variables,
            Err(error) => {
                write_error_response("INVALID_VARIABLES", &error.to_string(), None)?;
                return Ok(crate::ExitCode::InvalidArgs);
            }
        };

        let mut session = match crate::project_session::ProjectSession::open(
            self.from.as_deref(),
            crate::project_session::CacheUse::Off,
        ) {
            Ok(session) => session,
            Err(error) => {
                write_error_response(
                    "PROJECT_LOAD_FAILED",
                    &format!("failed to load BAML project: {error:#}"),
                    None,
                )?;
                return Ok(crate::ExitCode::Other);
            }
        };
        session.warm_prep_seeds_only();
        session.prime();

        let diagnostics = baml_project::collect_diagnostics(&session.db);
        let user_file_ids = session
            .db
            .get_source_files()
            .into_iter()
            .map(|file| file.file_id(&session.db))
            .collect::<std::collections::HashSet<_>>();
        let errors = diagnostics
            .iter()
            .filter(|diagnostic| {
                diagnostic.severity == baml_db::baml_compiler_diagnostics::Severity::Error
                    && diagnostic
                        .file_id()
                        .is_none_or(|file_id| user_file_ids.contains(&file_id))
            })
            .collect::<Vec<_>>();
        if !errors.is_empty() {
            let structured = errors
                .into_iter()
                .map(|diagnostic| diagnostic_json(&session.db, session.root(), diagnostic))
                .collect::<Vec<_>>();
            write_error_response(
                "BAML_VALIDATION_FAILED",
                &format!(
                    "BAML project contains {} error{}",
                    structured.len(),
                    if structured.len() == 1 { "" } else { "s" }
                ),
                Some(json!({ "diagnostics": structured })),
            )?;
            return Ok(crate::ExitCode::Other);
        }

        let snapshot = build_snapshot(
            &session.db,
            session.root(),
            session.resolved.manifest.as_deref(),
        )?;
        let context = GraphqlContext { snapshot };
        let request =
            GraphQLRequest::<DefaultScalarValue>::new(query, self.operation.clone(), variables);
        let response = request.execute_sync(&schema, &context);
        write_graphql_response(&response)
    }

    fn read_query(&self) -> Result<String> {
        if let Some(query) = &self.query {
            return Ok(query.clone());
        }
        if let Some(path) = &self.query_file {
            return std::fs::read_to_string(path).with_context(|| {
                format!("failed to read GraphQL document from {}", path.display())
            });
        }
        if std::io::stdin().is_terminal() {
            anyhow::bail!("provide a GraphQL document with `--query`, `--query-file`, or stdin");
        }
        let mut query = String::new();
        std::io::stdin()
            .read_to_string(&mut query)
            .context("failed to read GraphQL document from stdin")?;
        Ok(query)
    }

    fn parse_variables(&self) -> Result<Option<juniper::InputValue<DefaultScalarValue>>> {
        let Some(raw) = &self.variables else {
            return Ok(None);
        };
        let json: JsonValue = serde_json::from_str(raw).context("variables must be valid JSON")?;
        if !json.is_object() {
            anyhow::bail!("variables must be a JSON object");
        }
        serde_json::from_value(json)
            .context("variables contain a value GraphQL cannot represent")
            .map(Some)
    }
}

type Schema = RootNode<QueryRoot, EmptyMutation<GraphqlContext>, EmptySubscription<GraphqlContext>>;

fn schema() -> Schema {
    Schema::new(
        QueryRoot,
        EmptyMutation::<GraphqlContext>::new(),
        EmptySubscription::<GraphqlContext>::new(),
    )
}

struct GraphqlContext {
    snapshot: Snapshot,
}

impl juniper::Context for GraphqlContext {}

impl GraphqlContext {
    fn empty() -> Self {
        Self {
            snapshot: Snapshot::empty(),
        }
    }
}

struct QueryRoot;

#[juniper::graphql_object(Context = GraphqlContext)]
impl QueryRoot {
    /// The loaded BAML project.
    fn project(context: &GraphqlContext) -> &GraphProject {
        &context.snapshot.project
    }

    /// Source packages, optionally filtered by exact package name.
    fn packages(context: &GraphqlContext, name: Option<String>) -> Vec<&GraphPackage> {
        filter_name(
            &context.snapshot.project.packages,
            name.as_deref(),
            |item| &item.name,
        )
    }

    /// Source files, optionally filtered by exact project-relative path.
    fn files(context: &GraphqlContext, path: Option<String>) -> Vec<&GraphSourceFile> {
        context
            .snapshot
            .project
            .files
            .iter()
            .filter(|file| path.as_deref().is_none_or(|path| file.path == path))
            .collect()
    }

    /// All top-level definitions with exact name and kind filters.
    fn definitions(
        context: &GraphqlContext,
        name: Option<String>,
        kind: Option<Vec<DefinitionKind>>,
    ) -> Vec<&GraphDefinition> {
        context
            .snapshot
            .project
            .definitions
            .iter()
            .filter(|item| name.as_deref().is_none_or(|name| item.name == name))
            .filter(|item| kind.as_ref().is_none_or(|kinds| kinds.contains(&item.kind)))
            .collect()
    }

    fn classes(context: &GraphqlContext, name: Option<String>) -> Vec<&GraphClass> {
        filter_name(&context.snapshot.project.classes, name.as_deref(), |item| {
            &item.name
        })
    }

    fn enums(context: &GraphqlContext, name: Option<String>) -> Vec<&GraphEnum> {
        filter_name(&context.snapshot.project.enums, name.as_deref(), |item| {
            &item.name
        })
    }

    fn type_aliases(context: &GraphqlContext, name: Option<String>) -> Vec<&GraphTypeAlias> {
        filter_name(
            &context.snapshot.project.type_aliases,
            name.as_deref(),
            |item| &item.name,
        )
    }

    fn functions(context: &GraphqlContext, name: Option<String>) -> Vec<&GraphFunction> {
        filter_name(
            &context.snapshot.project.functions,
            name.as_deref(),
            |item| &item.name,
        )
    }

    fn clients(context: &GraphqlContext, name: Option<String>) -> Vec<&GraphClient> {
        filter_name(&context.snapshot.project.clients, name.as_deref(), |item| {
            &item.name
        })
    }

    fn generators(context: &GraphqlContext, name: Option<String>) -> Vec<&GraphGenerator> {
        filter_name(
            &context.snapshot.project.generators,
            name.as_deref(),
            |item| &item.name,
        )
    }

    fn tests(context: &GraphqlContext, name: Option<String>) -> Vec<&GraphTest> {
        filter_name(&context.snapshot.project.tests, name.as_deref(), |item| {
            &item.name
        })
    }
}

fn filter_name<'a, T>(
    items: &'a [T],
    name: Option<&str>,
    item_name: impl Fn(&T) -> &String,
) -> Vec<&'a T> {
    items
        .iter()
        .filter(|item| name.is_none_or(|name| item_name(item) == name))
        .collect()
}

#[derive(Clone, Copy, Debug, Eq, GraphQLEnum, PartialEq)]
enum DefinitionKind {
    Class,
    Field,
    Enum,
    EnumValue,
    TypeAlias,
    Function,
    Parameter,
    Client,
    Generator,
    Test,
    Interface,
    TemplateString,
}

#[derive(Clone, Copy, Debug, Eq, GraphQLEnum, PartialEq)]
enum TypeRefKind {
    Named,
    AssociatedType,
    Int,
    Bigint,
    Float,
    String,
    Bool,
    Null,
    Never,
    Void,
    Bytes,
    Media,
    Optional,
    List,
    Map,
    Union,
    Literal,
    Function,
    Unknown,
    Type,
    Rust,
    #[graphql(name = "ERROR")]
    Invalid,
    Infer,
}

#[derive(Clone, GraphQLObject)]
struct SourceLocation {
    /// Project-relative, slash-separated path.
    path: String,
    /// Inclusive, 1-based start line.
    start_line: i32,
    /// Inclusive, 1-based start column.
    start_column: i32,
    /// Exclusive, 1-based end line.
    end_line: i32,
    /// Exclusive, 1-based end column.
    end_column: i32,
}

#[derive(Clone, GraphQLObject)]
#[graphql(name = "AttributeArgument")]
struct GraphAttributeArgument {
    key: Option<String>,
    value: String,
    location: SourceLocation,
}

#[derive(Clone, GraphQLObject)]
#[graphql(name = "Attribute")]
struct GraphAttribute {
    name: String,
    arguments: Vec<GraphAttributeArgument>,
    location: SourceLocation,
}

#[derive(Clone, GraphQLObject)]
#[graphql(name = "TypeRef")]
struct GraphTypeRef {
    kind: TypeRefKind,
    /// Canonical source-like spelling.
    display: String,
    /// Unqualified named type, associated member, literal, or media kind.
    name: Option<String>,
    /// Dotted named-type path as individual segments.
    path: Vec<String>,
    /// Nested type for optional/list/associated-type references.
    element_type: Option<Box<GraphTypeRef>>,
    key_type: Option<Box<GraphTypeRef>>,
    value_type: Option<Box<GraphTypeRef>>,
    /// Union members, generic arguments, or associated interface constraints.
    member_types: Vec<GraphTypeRef>,
    /// Function-type parameter types.
    parameter_types: Vec<GraphTypeRef>,
    return_type: Option<Box<GraphTypeRef>>,
    throws_type: Option<Box<GraphTypeRef>>,
    attributes: Vec<GraphAttribute>,
    location: SourceLocation,
}

#[derive(Clone, GraphQLObject)]
#[graphql(name = "Definition")]
struct GraphDefinition {
    kind: DefinitionKind,
    name: String,
    qualified_name: String,
    documentation: Option<String>,
    attributes: Vec<GraphAttribute>,
    location: SourceLocation,
}

#[derive(Clone, GraphQLObject)]
#[graphql(name = "Field")]
struct GraphField {
    name: String,
    documentation: Option<String>,
    attributes: Vec<GraphAttribute>,
    #[graphql(name = "type")]
    type_ref: GraphTypeRef,
    location: SourceLocation,
}

#[derive(Clone, GraphQLObject)]
#[graphql(name = "EnumValue")]
struct GraphEnumValue {
    name: String,
    documentation: Option<String>,
    attributes: Vec<GraphAttribute>,
    location: SourceLocation,
}

#[derive(Clone, GraphQLObject)]
#[graphql(name = "Parameter")]
struct GraphParameter {
    name: String,
    #[graphql(name = "type")]
    type_ref: Option<GraphTypeRef>,
    has_default: bool,
    location: SourceLocation,
}

#[derive(Clone, GraphQLObject)]
#[graphql(name = "Function")]
struct GraphFunction {
    name: String,
    qualified_name: String,
    documentation: Option<String>,
    attributes: Vec<GraphAttribute>,
    generic_parameters: Vec<String>,
    parameters: Vec<GraphParameter>,
    return_type: Option<GraphTypeRef>,
    throws_type: Option<GraphTypeRef>,
    is_llm: bool,
    client_name: Option<String>,
    location: SourceLocation,
}

#[derive(Clone, GraphQLObject)]
#[graphql(name = "Class")]
struct GraphClass {
    name: String,
    qualified_name: String,
    documentation: Option<String>,
    attributes: Vec<GraphAttribute>,
    generic_parameters: Vec<String>,
    fields: Vec<GraphField>,
    methods: Vec<GraphFunction>,
    location: SourceLocation,
}

#[derive(Clone, GraphQLObject)]
#[graphql(name = "Enum")]
struct GraphEnum {
    name: String,
    qualified_name: String,
    documentation: Option<String>,
    attributes: Vec<GraphAttribute>,
    values: Vec<GraphEnumValue>,
    location: SourceLocation,
}

#[derive(Clone, GraphQLObject)]
#[graphql(name = "TypeAlias")]
struct GraphTypeAlias {
    name: String,
    qualified_name: String,
    documentation: Option<String>,
    #[graphql(name = "type")]
    type_ref: Option<GraphTypeRef>,
    location: SourceLocation,
}

#[derive(Clone, GraphQLObject)]
#[graphql(name = "ConfigEntry")]
struct GraphConfigEntry {
    key: String,
    value: String,
    location: SourceLocation,
}

#[derive(Clone, GraphQLObject)]
#[graphql(name = "Client")]
struct GraphClient {
    name: String,
    qualified_name: String,
    properties: Vec<GraphConfigEntry>,
    location: SourceLocation,
}

#[derive(Clone, GraphQLObject)]
#[graphql(name = "Generator")]
struct GraphGenerator {
    name: String,
    output_type: Option<String>,
    output_dir: Option<String>,
    naming_convention: Option<String>,
    sdk_import_path: Option<String>,
    location: SourceLocation,
}

#[derive(Clone, GraphQLObject)]
#[graphql(name = "TestArgument")]
struct GraphTestArgument {
    name: String,
    value_json: String,
}

#[derive(Clone, GraphQLObject)]
#[graphql(name = "Test")]
struct GraphTest {
    name: String,
    qualified_name: String,
    functions: Vec<String>,
    arguments: Vec<GraphTestArgument>,
    location: SourceLocation,
}

#[derive(Clone, GraphQLObject)]
#[graphql(name = "SourceFile")]
struct GraphSourceFile {
    path: String,
    package: String,
    namespace: Vec<String>,
    definitions: Vec<GraphDefinition>,
    classes: Vec<GraphClass>,
    enums: Vec<GraphEnum>,
    type_aliases: Vec<GraphTypeAlias>,
    functions: Vec<GraphFunction>,
    clients: Vec<GraphClient>,
    tests: Vec<GraphTest>,
}

#[derive(Clone, GraphQLObject)]
#[graphql(name = "Package")]
struct GraphPackage {
    name: String,
    files: Vec<GraphSourceFile>,
    definitions: Vec<GraphDefinition>,
    classes: Vec<GraphClass>,
    enums: Vec<GraphEnum>,
    type_aliases: Vec<GraphTypeAlias>,
    functions: Vec<GraphFunction>,
    clients: Vec<GraphClient>,
    generators: Vec<GraphGenerator>,
    tests: Vec<GraphTest>,
}

#[derive(Clone, GraphQLObject)]
#[graphql(name = "Project")]
struct GraphProject {
    /// Package name from baml.toml, when present.
    name: Option<String>,
    /// Canonical project root.
    root: String,
    packages: Vec<GraphPackage>,
    files: Vec<GraphSourceFile>,
    definitions: Vec<GraphDefinition>,
    classes: Vec<GraphClass>,
    enums: Vec<GraphEnum>,
    type_aliases: Vec<GraphTypeAlias>,
    functions: Vec<GraphFunction>,
    clients: Vec<GraphClient>,
    generators: Vec<GraphGenerator>,
    tests: Vec<GraphTest>,
}

struct Snapshot {
    project: GraphProject,
}

impl Snapshot {
    fn empty() -> Self {
        Self {
            project: GraphProject {
                name: None,
                root: String::new(),
                packages: Vec::new(),
                files: Vec::new(),
                definitions: Vec::new(),
                classes: Vec::new(),
                enums: Vec::new(),
                type_aliases: Vec::new(),
                functions: Vec::new(),
                clients: Vec::new(),
                generators: Vec::new(),
                tests: Vec::new(),
            },
        }
    }
}

fn build_snapshot(
    db: &ProjectDatabase,
    root: &Path,
    manifest_source: Option<&str>,
) -> Result<Snapshot> {
    let mut files = db.get_source_files();
    files.sort_by_key(|file| normalized_relative_path(&file.path(db), root));

    let mut graph_files = Vec::with_capacity(files.len());
    for file in files {
        graph_files.push(build_file(db, root, file));
    }

    let (project_name, generators) = build_generators(manifest_source)?;
    let mut packages = BTreeMap::<String, GraphPackage>::new();
    for file in &graph_files {
        let package = packages
            .entry(file.package.clone())
            .or_insert_with(|| GraphPackage {
                name: file.package.clone(),
                files: Vec::new(),
                definitions: Vec::new(),
                classes: Vec::new(),
                enums: Vec::new(),
                type_aliases: Vec::new(),
                functions: Vec::new(),
                clients: Vec::new(),
                generators: Vec::new(),
                tests: Vec::new(),
            });
        package.files.push(file.clone());
        package.definitions.extend(file.definitions.clone());
        package.classes.extend(file.classes.clone());
        package.enums.extend(file.enums.clone());
        package.type_aliases.extend(file.type_aliases.clone());
        package.functions.extend(file.functions.clone());
        package.clients.extend(file.clients.clone());
        package.tests.extend(file.tests.clone());
    }
    let user_package = packages
        .entry("user".to_string())
        .or_insert_with(|| GraphPackage {
            name: "user".to_string(),
            files: Vec::new(),
            definitions: Vec::new(),
            classes: Vec::new(),
            enums: Vec::new(),
            type_aliases: Vec::new(),
            functions: Vec::new(),
            clients: Vec::new(),
            generators: Vec::new(),
            tests: Vec::new(),
        });
    user_package.generators = generators.clone();
    user_package
        .definitions
        .extend(generators.iter().map(GraphGenerator::summary));

    let mut project = GraphProject {
        name: project_name,
        root: normalize_path(root),
        packages: packages.into_values().collect(),
        files: graph_files,
        definitions: Vec::new(),
        classes: Vec::new(),
        enums: Vec::new(),
        type_aliases: Vec::new(),
        functions: Vec::new(),
        clients: Vec::new(),
        generators,
        tests: Vec::new(),
    };
    for file in &project.files {
        project.definitions.extend(file.definitions.clone());
        project.classes.extend(file.classes.clone());
        project.enums.extend(file.enums.clone());
        project.type_aliases.extend(file.type_aliases.clone());
        project.functions.extend(file.functions.clone());
        project.clients.extend(file.clients.clone());
        project.tests.extend(file.tests.clone());
    }
    project
        .definitions
        .extend(project.generators.iter().map(GraphGenerator::summary));
    Ok(Snapshot { project })
}

fn build_file(db: &ProjectDatabase, root: &Path, file: SourceFile) -> GraphSourceFile {
    let source_path = file.path(db);
    let path = normalized_relative_path(&source_path, root);
    let source = file.text(db);
    let package_info = baml_compiler2_hir::file_package::file_package(db, file);
    let namespace = package_info
        .namespace_path
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>();
    let mut output = GraphSourceFile {
        path: path.clone(),
        package: package_info.package.to_string(),
        namespace: namespace.clone(),
        definitions: Vec::new(),
        classes: Vec::new(),
        enums: Vec::new(),
        type_aliases: Vec::new(),
        functions: Vec::new(),
        clients: Vec::new(),
        tests: Vec::new(),
    };
    let ast = baml_compiler2_hir::file_ast(db, file);
    for item in &ast.items {
        match item {
            Item::Class(item) => {
                let class = GraphClass {
                    name: item.name.to_string(),
                    qualified_name: qualified_name(&namespace, item.name.as_str()),
                    documentation: item.docstring.clone(),
                    attributes: graph_attributes(&path, source, &item.attributes),
                    generic_parameters: item
                        .generic_params
                        .iter()
                        .map(|param| param.name.to_string())
                        .collect(),
                    fields: item
                        .fields
                        .iter()
                        .map(|field| GraphField {
                            name: field.name.to_string(),
                            documentation: field.docstring.clone(),
                            attributes: graph_attributes(&path, source, &field.attributes),
                            type_ref: graph_type_ref(&path, source, &field.type_expr),
                            location: source_location(&path, source, field.span),
                        })
                        .collect(),
                    methods: item
                        .methods
                        .iter()
                        .filter(|method| method.metadata.origin == FunctionOrigin::UserDefined)
                        .map(|method| graph_function(&path, source, &namespace, method))
                        .collect(),
                    location: source_location(&path, source, item.span),
                };
                output.definitions.push(class.summary());
                output
                    .definitions
                    .extend(class.fields.iter().map(|field| GraphDefinition {
                        kind: DefinitionKind::Field,
                        name: field.name.clone(),
                        qualified_name: format!("{}.{}", class.qualified_name, field.name),
                        documentation: field.documentation.clone(),
                        attributes: field.attributes.clone(),
                        location: field.location.clone(),
                    }));
                output
                    .definitions
                    .extend(class.methods.iter().map(|method| method.summary()));
                for method in &class.methods {
                    output
                        .definitions
                        .extend(method.parameters.iter().map(|parameter| GraphDefinition {
                            kind: DefinitionKind::Parameter,
                            name: parameter.name.clone(),
                            qualified_name: format!("{}.{}", method.qualified_name, parameter.name),
                            documentation: None,
                            attributes: Vec::new(),
                            location: parameter.location.clone(),
                        }));
                }
                output.classes.push(class);
            }
            Item::Enum(item) => {
                let graph_enum = GraphEnum {
                    name: item.name.to_string(),
                    qualified_name: qualified_name(&namespace, item.name.as_str()),
                    documentation: item.docstring.clone(),
                    attributes: graph_attributes(&path, source, &item.attributes),
                    values: item
                        .variants
                        .iter()
                        .map(|value| GraphEnumValue {
                            name: value.name.to_string(),
                            documentation: value.docstring.clone(),
                            attributes: graph_attributes(&path, source, &value.attributes),
                            location: source_location(&path, source, value.span),
                        })
                        .collect(),
                    location: source_location(&path, source, item.span),
                };
                output.definitions.push(graph_enum.summary());
                output
                    .definitions
                    .extend(graph_enum.values.iter().map(|value| GraphDefinition {
                        kind: DefinitionKind::EnumValue,
                        name: value.name.clone(),
                        qualified_name: format!("{}.{}", graph_enum.qualified_name, value.name),
                        documentation: value.documentation.clone(),
                        attributes: value.attributes.clone(),
                        location: value.location.clone(),
                    }));
                output.enums.push(graph_enum);
            }
            Item::TypeAlias(item) => {
                let alias = GraphTypeAlias {
                    name: item.name.to_string(),
                    qualified_name: qualified_name(&namespace, item.name.as_str()),
                    documentation: item.docstring.clone(),
                    type_ref: item
                        .type_expr
                        .as_ref()
                        .map(|ty| graph_type_ref(&path, source, ty)),
                    location: source_location(&path, source, item.span),
                };
                output.definitions.push(alias.summary());
                output.type_aliases.push(alias);
            }
            Item::Function(item) if item.metadata.origin == FunctionOrigin::UserDefined => {
                let function = graph_function(&path, source, &namespace, item);
                output.definitions.push(function.summary());
                output
                    .definitions
                    .extend(function.parameters.iter().map(|parameter| GraphDefinition {
                        kind: DefinitionKind::Parameter,
                        name: parameter.name.clone(),
                        qualified_name: format!("{}.{}", function.qualified_name, parameter.name),
                        documentation: None,
                        attributes: Vec::new(),
                        location: parameter.location.clone(),
                    }));
                output.functions.push(function);
            }
            Item::Client(item) => {
                let client = GraphClient {
                    name: item.name.to_string(),
                    qualified_name: qualified_name(&namespace, item.name.as_str()),
                    properties: item
                        .config_items
                        .iter()
                        .map(|property| GraphConfigEntry {
                            key: property.key.to_string(),
                            value: property.value.clone(),
                            location: source_location(&path, source, property.span),
                        })
                        .collect(),
                    location: source_location(&path, source, item.span),
                };
                output.definitions.push(client.summary());
                output.clients.push(client);
            }
            Item::Let(item) if item.origin == LetOrigin::Client => {
                let client = GraphClient {
                    name: item.name.to_string(),
                    qualified_name: qualified_name(&namespace, item.name.as_str()),
                    properties: Vec::new(),
                    location: source_location(&path, source, item.span),
                };
                output.definitions.push(client.summary());
                output.clients.push(client);
            }
            Item::Test(item) => {
                let test = GraphTest {
                    name: item.name.to_string(),
                    qualified_name: qualified_name(&namespace, item.name.as_str()),
                    functions: item.function_refs.iter().map(ToString::to_string).collect(),
                    arguments: item
                        .args
                        .iter()
                        .map(|(name, value)| GraphTestArgument {
                            name: name.to_string(),
                            value_json: test_arg_json(value).to_string(),
                        })
                        .collect(),
                    location: source_location(&path, source, item.span),
                };
                output.definitions.push(test.summary());
                output.tests.push(test);
            }
            Item::Interface(item) => output.definitions.push(GraphDefinition {
                kind: DefinitionKind::Interface,
                name: item.name.to_string(),
                qualified_name: qualified_name(&namespace, item.name.as_str()),
                documentation: item.docstring.clone(),
                attributes: graph_attributes(&path, source, &item.attributes),
                location: source_location(&path, source, item.span),
            }),
            Item::TemplateString(item) => output.definitions.push(GraphDefinition {
                kind: DefinitionKind::TemplateString,
                name: item.name.to_string(),
                qualified_name: qualified_name(&namespace, item.name.as_str()),
                documentation: None,
                attributes: Vec::new(),
                location: source_location(&path, source, item.span),
            }),
            Item::Function(_) | Item::Let(_) | Item::RetryPolicy(_) | Item::ImplementsFor(_) => {}
        }
    }
    output
}

fn graph_function(
    path: &str,
    source: &str,
    namespace: &[String],
    item: &FunctionDef,
) -> GraphFunction {
    let (is_llm, client_name) = match &item.declarative_meta {
        Some(DeclarativeMeta::Llm(meta)) => (true, meta.client.as_ref().map(ToString::to_string)),
        None => (false, None),
    };
    GraphFunction {
        name: item.name.to_string(),
        qualified_name: qualified_name(namespace, item.name.as_str()),
        documentation: item.docstring.clone(),
        attributes: graph_attributes(path, source, &item.attributes),
        generic_parameters: item
            .generic_params
            .iter()
            .map(|param| param.name.to_string())
            .collect(),
        parameters: item
            .params
            .iter()
            .map(|param| GraphParameter {
                name: param.name.to_string(),
                type_ref: param
                    .type_expr
                    .as_ref()
                    .map(|ty| graph_type_ref(path, source, ty)),
                has_default: param.default.is_some(),
                location: source_location(path, source, param.span),
            })
            .collect(),
        return_type: item
            .return_type
            .as_ref()
            .map(|ty| graph_type_ref(path, source, ty)),
        throws_type: item
            .throws
            .as_ref()
            .map(|ty| graph_type_ref(path, source, ty)),
        is_llm,
        client_name,
        location: source_location(path, source, item.span),
    }
}

fn graph_type_ref(path: &str, source: &str, ty: &TypeExpr) -> GraphTypeRef {
    let mut output = GraphTypeRef {
        kind: TypeRefKind::Unknown,
        display: ty.to_string(),
        name: None,
        path: Vec::new(),
        element_type: None,
        key_type: None,
        value_type: None,
        member_types: Vec::new(),
        parameter_types: Vec::new(),
        return_type: None,
        throws_type: None,
        attributes: graph_attributes(path, source, ty.kind.attrs()),
        location: source_location(path, source, ty.span),
    };
    match &ty.kind {
        TypeExprKind::Path {
            segments,
            generic_args,
            associated_type_bindings,
            ..
        } => {
            output.kind = TypeRefKind::Named;
            output.name = segments.last().map(ToString::to_string);
            output.path = segments.iter().map(ToString::to_string).collect();
            output.member_types = generic_args
                .iter()
                .chain(
                    associated_type_bindings
                        .iter()
                        .map(|binding| binding.ty.as_ref()),
                )
                .map(|ty| graph_type_ref(path, source, ty))
                .collect();
        }
        TypeExprKind::AssociatedTypeProjection {
            base,
            interface,
            member,
            ..
        } => {
            output.kind = TypeRefKind::AssociatedType;
            output.name = Some(member.to_string());
            output.element_type = Some(Box::new(graph_type_ref(path, source, base)));
            output.member_types = interface
                .iter()
                .map(|ty| graph_type_ref(path, source, ty))
                .collect();
        }
        TypeExprKind::Int { .. } => output.kind = TypeRefKind::Int,
        TypeExprKind::Bigint { .. } => output.kind = TypeRefKind::Bigint,
        TypeExprKind::Float { .. } => output.kind = TypeRefKind::Float,
        TypeExprKind::String { .. } => output.kind = TypeRefKind::String,
        TypeExprKind::Bool { .. } => output.kind = TypeRefKind::Bool,
        TypeExprKind::Null { .. } => output.kind = TypeRefKind::Null,
        TypeExprKind::Never { .. } => output.kind = TypeRefKind::Never,
        TypeExprKind::Void { .. } => output.kind = TypeRefKind::Void,
        TypeExprKind::Uint8Array { .. } => output.kind = TypeRefKind::Bytes,
        TypeExprKind::Media { kind, .. } => {
            output.kind = TypeRefKind::Media;
            output.name = Some(format!("{kind:?}").to_ascii_lowercase());
        }
        TypeExprKind::Optional { inner, .. } => {
            output.kind = TypeRefKind::Optional;
            output.element_type = Some(Box::new(graph_type_ref(path, source, inner)));
        }
        TypeExprKind::List { inner, .. } => {
            output.kind = TypeRefKind::List;
            output.element_type = Some(Box::new(graph_type_ref(path, source, inner)));
        }
        TypeExprKind::Map { key, value, .. } => {
            output.kind = TypeRefKind::Map;
            output.key_type = Some(Box::new(graph_type_ref(path, source, key)));
            output.value_type = Some(Box::new(graph_type_ref(path, source, value)));
        }
        TypeExprKind::Union { variants, .. } => {
            output.kind = TypeRefKind::Union;
            output.member_types = variants
                .iter()
                .map(|ty| graph_type_ref(path, source, ty))
                .collect();
        }
        TypeExprKind::Literal { value, .. } => {
            output.kind = TypeRefKind::Literal;
            output.name = Some(value.to_string());
        }
        TypeExprKind::Function {
            params,
            ret,
            throws,
            ..
        } => {
            output.kind = TypeRefKind::Function;
            output.parameter_types = params
                .iter()
                .map(|param| graph_type_ref(path, source, &param.ty))
                .collect();
            output.return_type = Some(Box::new(graph_type_ref(path, source, ret)));
            output.throws_type = throws
                .as_ref()
                .map(|ty| Box::new(graph_type_ref(path, source, ty)));
        }
        TypeExprKind::BuiltinUnknown { .. } | TypeExprKind::Unknown { .. } => {
            output.kind = TypeRefKind::Unknown;
        }
        TypeExprKind::Type { .. } => output.kind = TypeRefKind::Type,
        TypeExprKind::Rust { .. } => output.kind = TypeRefKind::Rust,
        TypeExprKind::Error { .. } => output.kind = TypeRefKind::Invalid,
        TypeExprKind::Infer { .. } => output.kind = TypeRefKind::Infer,
    }
    output
}

fn graph_attributes(path: &str, source: &str, attributes: &[RawAttribute]) -> Vec<GraphAttribute> {
    attributes
        .iter()
        .map(|attribute| GraphAttribute {
            name: attribute.name.to_string(),
            arguments: attribute
                .args
                .iter()
                .map(|argument| GraphAttributeArgument {
                    key: argument.key.as_ref().map(ToString::to_string),
                    value: argument.value.clone(),
                    location: source_location(path, source, argument.span),
                })
                .collect(),
            location: source_location(path, source, attribute.span),
        })
        .collect()
}

fn build_generators(source: Option<&str>) -> Result<(Option<String>, Vec<GraphGenerator>)> {
    let Some(source) = source else {
        return Ok((None, Vec::new()));
    };
    let manifest =
        crate::manifest::parse(source).context("failed to parse baml.toml for GraphQL")?;
    let project_name = manifest.package.and_then(|package| package.name);
    let generators = manifest
        .generator
        .into_iter()
        .map(|(name, generator)| {
            let range = text_range(generator.span());
            let generator = generator.into_inner();
            GraphGenerator {
                name,
                output_type: generator.output_type.map(|value| value.into_inner()),
                output_dir: generator.output_dir,
                naming_convention: generator.naming_convention.map(|value| value.into_inner()),
                sdk_import_path: generator.sdk_import_path.map(|value| value.into_inner()),
                location: source_location("baml.toml", source, range),
            }
        })
        .collect();
    Ok((project_name, generators))
}

fn text_range(range: std::ops::Range<usize>) -> TextRange {
    TextRange::new(
        u32::try_from(range.start).unwrap_or(u32::MAX).into(),
        u32::try_from(range.end).unwrap_or(u32::MAX).into(),
    )
}

fn test_arg_json(value: &TestArgValue) -> JsonValue {
    match value {
        TestArgValue::Null => JsonValue::Null,
        TestArgValue::Int(value) => json!(value),
        TestArgValue::FloatBits(bits) => json!(f64::from_bits(*bits)),
        TestArgValue::Bool(value) => json!(value),
        TestArgValue::String(value) => json!(value),
        TestArgValue::Array(values) => JsonValue::Array(values.iter().map(test_arg_json).collect()),
        TestArgValue::Map(entries) => JsonValue::Object(
            entries
                .iter()
                .map(|(key, value)| (key.clone(), test_arg_json(value)))
                .collect(),
        ),
    }
}

impl GraphClass {
    fn summary(&self) -> GraphDefinition {
        definition_summary(
            DefinitionKind::Class,
            &self.name,
            &self.qualified_name,
            self.documentation.clone(),
            self.attributes.clone(),
            self.location.clone(),
        )
    }
}

impl GraphEnum {
    fn summary(&self) -> GraphDefinition {
        definition_summary(
            DefinitionKind::Enum,
            &self.name,
            &self.qualified_name,
            self.documentation.clone(),
            self.attributes.clone(),
            self.location.clone(),
        )
    }
}

impl GraphTypeAlias {
    fn summary(&self) -> GraphDefinition {
        definition_summary(
            DefinitionKind::TypeAlias,
            &self.name,
            &self.qualified_name,
            self.documentation.clone(),
            Vec::new(),
            self.location.clone(),
        )
    }
}

impl GraphFunction {
    fn summary(&self) -> GraphDefinition {
        definition_summary(
            DefinitionKind::Function,
            &self.name,
            &self.qualified_name,
            self.documentation.clone(),
            self.attributes.clone(),
            self.location.clone(),
        )
    }
}

impl GraphClient {
    fn summary(&self) -> GraphDefinition {
        definition_summary(
            DefinitionKind::Client,
            &self.name,
            &self.qualified_name,
            None,
            Vec::new(),
            self.location.clone(),
        )
    }
}

impl GraphGenerator {
    fn summary(&self) -> GraphDefinition {
        definition_summary(
            DefinitionKind::Generator,
            &self.name,
            &self.name,
            None,
            Vec::new(),
            self.location.clone(),
        )
    }
}

impl GraphTest {
    fn summary(&self) -> GraphDefinition {
        definition_summary(
            DefinitionKind::Test,
            &self.name,
            &self.qualified_name,
            None,
            Vec::new(),
            self.location.clone(),
        )
    }
}

fn definition_summary(
    kind: DefinitionKind,
    name: &str,
    qualified_name: &str,
    documentation: Option<String>,
    attributes: Vec<GraphAttribute>,
    location: SourceLocation,
) -> GraphDefinition {
    GraphDefinition {
        kind,
        name: name.to_string(),
        qualified_name: qualified_name.to_string(),
        documentation,
        attributes,
        location,
    }
}

fn qualified_name(namespace: &[String], name: &str) -> String {
    if namespace.is_empty() {
        name.to_string()
    } else {
        format!("{}.{name}", namespace.join("."))
    }
}

fn source_location(path: &str, source: &str, range: TextRange) -> SourceLocation {
    let start = usize::from(range.start()).min(source.len());
    let end = usize::from(range.end()).min(source.len());
    let (start_line, start_column) = line_column(source, start);
    let (end_line, end_column) = line_column(source, end);
    SourceLocation {
        path: path.to_string(),
        start_line,
        start_column,
        end_line,
        end_column,
    }
}

fn line_column(source: &str, offset: usize) -> (i32, i32) {
    let offset = floor_char_boundary(source, offset.min(source.len()));
    let before = &source[..offset];
    let line = before.bytes().filter(|byte| *byte == b'\n').count() + 1;
    let column = before
        .rsplit_once('\n')
        .map_or(before, |(_, line)| line)
        .chars()
        .count()
        + 1;
    (
        i32::try_from(line).unwrap_or(i32::MAX),
        i32::try_from(column).unwrap_or(i32::MAX),
    )
}

fn floor_char_boundary(source: &str, mut offset: usize) -> usize {
    while !source.is_char_boundary(offset) {
        offset = offset.saturating_sub(1);
    }
    offset
}

fn normalized_relative_path(path: &Path, root: &Path) -> String {
    normalize_path(path.strip_prefix(root).unwrap_or(path))
}

fn normalize_path(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

fn diagnostic_json(
    db: &ProjectDatabase,
    root: &Path,
    diagnostic: &baml_db::baml_compiler_diagnostics::Diagnostic,
) -> JsonValue {
    let location = diagnostic.primary_span().and_then(|span| {
        let path = db.file_id_to_path(span.file_id)?;
        let file = db
            .get_source_files()
            .into_iter()
            .find(|file| file.file_id(db) == span.file_id)?;
        Some(source_location(
            &normalized_relative_path(path, root),
            file.text(db),
            span.range,
        ))
    });
    json!({
        "code": diagnostic.code(),
        "severity": format!("{:?}", diagnostic.severity).to_ascii_lowercase(),
        "message": diagnostic.message_with_primary_label(),
        "location": location.map(location_json),
    })
}

fn location_json(location: SourceLocation) -> JsonValue {
    json!({
        "path": location.path,
        "startLine": location.start_line,
        "startColumn": location.start_column,
        "endLine": location.end_line,
        "endColumn": location.end_column,
    })
}

fn write_graphql_response(
    response: &GraphQLResponse<DefaultScalarValue>,
) -> Result<crate::ExitCode> {
    let is_ok = response.is_ok();
    let stdout = std::io::stdout();
    let mut stdout = stdout.lock();
    serde_json::to_writer(&mut stdout, response).context("failed to write GraphQL response")?;
    writeln!(stdout).context("failed to finish GraphQL response")?;
    Ok(if is_ok {
        crate::ExitCode::Success
    } else {
        crate::ExitCode::InvalidArgs
    })
}

fn write_error_response(
    code: &str,
    message: &str,
    extra_extensions: Option<JsonValue>,
) -> Result<()> {
    let mut extensions = serde_json::Map::new();
    extensions.insert("code".to_string(), json!(code));
    if let Some(JsonValue::Object(extra)) = extra_extensions {
        extensions.extend(extra);
    }
    let response = json!({
        "errors": [{
            "message": message,
            "extensions": extensions,
        }],
    });
    let stdout = std::io::stdout();
    let mut stdout = stdout.lock();
    serde_json::to_writer(&mut stdout, &response)
        .context("failed to write GraphQL error response")?;
    writeln!(stdout).context("failed to finish GraphQL error response")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn line_columns_are_one_based_and_unicode_aware() {
        assert_eq!(line_column("αβ\nvalue", 0), (1, 1));
        assert_eq!(line_column("αβ\nvalue", "α".len()), (1, 2));
        assert_eq!(line_column("αβ\nvalue", "αβ\n".len()), (2, 1));
    }

    #[test]
    fn schema_sdl_is_stable_and_has_search_roots() {
        let sdl = schema().as_sdl();
        assert!(sdl.contains("classes(name: String): [Class!]!"), "{sdl}");
        assert!(
            sdl.contains("definitions(name: String, kind: [DefinitionKind!])"),
            "{sdl}"
        );
        assert!(sdl.contains("type TypeRef"), "{sdl}");
        assert_eq!(sdl, schema().as_sdl());
    }
}
