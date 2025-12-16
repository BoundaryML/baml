mod watch_event;
/// Utilities for analyzing the watch variables and their dependencies.
pub mod watch_options;

use std::collections::HashSet;

use baml_types::{ir_type::UnionConstructor, BamlMap, TypeIR};
use internal_baml_diagnostics::{DatamodelError, Diagnostics};

use crate::thir::{self, typecheck::TypeCompatibility, ClassConstructorField, ExprMetadata, THir};
pub use crate::watch::{
    watch_event::{
        shared_handler, shared_noop_handler, ReducedWatchBamlValue, RuntimeNodeType,
        SharedWatchHandler, VizExecDelta, VizExecEvent, WatchBamlValue, WatchEventReducer,
        WatchHandler, WatchNotification, WatchValueMetadata,
    },
    watch_options::{WatchSpec, WatchWhen},
};

/// The result of analyzing the watch variables in a BAML program.
/// See `WatchChannels::analyze_program` for more details.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WatchChannels {
    pub functions_channels: BamlMap<String, FunctionChannels>,
}

impl WatchChannels {
    /// Walk a BAML program, inferring the watch channels required for each function.
    /// Throws diagnostics errors when channel invariants are violated, but makes
    /// a best-effort attempt to continue in the face of errors (the result will be
    /// an incomplete listing of channels).
    ///
    /// A function will have channels for:
    ///   - Its own markdown headers
    ///   - Its own variables
    ///   - Its transitive subfunctions' markdown headers (under a namespace)
    ///   - Its transitive subfunctions' variables (under a namespace)
    pub fn analyze_program(hir: &THir<ExprMetadata>, diagnostics: &mut Diagnostics) -> Self {
        // Compute the immediate metadata for each function.
        let function_metadatas: BamlMap<String, FunctionMetadata> = hir
            .expr_functions
            .iter()
            .map(|f| {
                (
                    f.name.clone(),
                    FunctionMetadata::analyze_function(f, diagnostics),
                )
            })
            .collect();
        let transitive_closures = FunctionMetadata::transitive_closures(&function_metadatas);

        let functions_channels = function_metadatas
            .iter()
            .map(|(fn_name, _)| {
                (
                    fn_name.clone(),
                    Self::convert_function(fn_name, &function_metadatas, &transitive_closures),
                )
            })
            .collect();
        WatchChannels { functions_channels }
    }

    fn convert_function(
        fn_name: &str,
        function_metadatas: &BamlMap<String, FunctionMetadata>,
        transitive_closures: &BamlMap<String, HashSet<String>>,
    ) -> FunctionChannels {
        let mut channels: HashSet<(ChannelFQN, TypeIR)> = HashSet::new();

        let FunctionMetadata {
            watch_vars,
            markdown_headers,
            ..
        } = &function_metadatas[fn_name];

        let md_channels = markdown_headers.iter().map(|header| {
            (
                ChannelFQN {
                    namespace: None,
                    r#type: ChannelType::MarkdownHeader,
                    name: header.clone(),
                },
                TypeIR::string(),
            )
        });
        channels.extend(md_channels);
        let var_channels =
            watch_vars
                .into_iter()
                .map(|(channel_name, (_watch_spec, chan_type))| {
                    (
                        ChannelFQN {
                            namespace: None,
                            r#type: ChannelType::Variable,
                            name: channel_name.clone(),
                        },
                        chan_type.clone(),
                    )
                });
        channels.extend(var_channels);

        let mut dependencies = transitive_closures[fn_name].clone();
        dependencies.remove(fn_name);
        for subfunction in dependencies {
            if let Some(FunctionMetadata {
                markdown_headers,
                watch_vars,
                ..
            }) = &function_metadatas.get(&subfunction)
            {
                let sub_md_channels = markdown_headers.iter().map(|header| {
                    (
                        ChannelFQN {
                            namespace: Some(subfunction.clone()),
                            r#type: ChannelType::MarkdownHeader,
                            name: header.clone(),
                        },
                        TypeIR::string(),
                    )
                });
                channels.extend(sub_md_channels);
                let sub_var_channels =
                    watch_vars
                        .into_iter()
                        .map(|(channel_name, (_watch_spec, chan_type))| {
                            (
                                ChannelFQN {
                                    namespace: Some(subfunction.clone()),
                                    r#type: ChannelType::Variable,
                                    name: channel_name.clone(),
                                },
                                chan_type.clone(),
                            )
                        });
                channels.extend(sub_var_channels);
            }
        }

        FunctionChannels { channels }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FunctionChannels {
    pub channels: HashSet<(ChannelFQN, TypeIR)>,
}

/// Fully qualified name of a watch channel.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ChannelFQN {
    /// The terminal name of the channel, derived directly from the variable,
    /// header, or user-supplied-channel name.
    pub name: String,

    /// Whether the channel is used for markdown header events or variable
    /// watch notifications.
    pub r#type: ChannelType,

    /// For blocks and variables of subfunctions, the name of the subfunction.
    pub namespace: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ChannelType {
    Variable,
    MarkdownHeader,
}

/// A helper struct. When we analyze a function, we collect this data about the function.
/// The fields are not transitive - they only include functions, watch vars and headers
/// used **directly** by the function.
#[derive(Debug)]
struct FunctionMetadata {
    subfunctions: HashSet<String>,
    watch_vars: BamlMap<String, (WatchSpec, TypeIR)>,
    markdown_headers: HashSet<String>,
}

impl FunctionMetadata {
    /// Check if the next statement is an immediate channel rename for the given variable.
    /// Returns Some(new_channel_name) if the next statement is WatchOptions that:
    /// 1. Operates on the same variable
    /// 2. Sets a new channel name (different from the variable name)
    fn is_immediate_channel_rename(
        next_statement: Option<&thir::Statement<ExprMetadata>>,
        var_name: &str,
    ) -> Option<String> {
        if let Some(thir::Statement::WatchOptions {
            variable,
            channel: Some(new_channel),
            ..
        }) = next_statement
        {
            if variable == var_name && new_channel != var_name {
                return Some(new_channel.clone());
            }
        }
        None
    }

    /// Add a new watch variable to the function metadata.
    /// If a variable with the same name already exists,
    /// use the existing channel and augment its type as
    /// needed (combine the existing and the new types into
    /// a union unless they are already subtypes).
    pub fn push_watch_var(&mut self, name: String, spec: WatchSpec, ty: TypeIR) {
        self.watch_vars
            .entry(name)
            .and_modify(|(_existing_spec, existing_type)| {
                if ty.is_subtype(existing_type) {
                    // Do nothing - the newly added type is already contained in the
                    // existing channel type.
                } else if existing_type.is_subtype(&ty) {
                    // "Grow" the channel type to the supertype.
                    *existing_type = ty.clone();
                } else {
                    // Combine the existing and new types into a union.
                    *existing_type = TypeIR::union(vec![existing_type.clone(), ty.clone()]);
                }
            })
            .or_insert((spec, ty));
    }

    /// Walk the statements of a function to collect metadata.
    pub fn analyze_function(
        function: &thir::ExprFunction<ExprMetadata>,
        diagnostics: &mut Diagnostics,
    ) -> Self {
        let mut metadata = FunctionMetadata {
            subfunctions: HashSet::new(),
            watch_vars: BamlMap::new(),
            markdown_headers: HashSet::new(),
        };

        let thir::ExprFunction { body, .. } = function;
        for (idx, statement) in body.statements.iter().enumerate() {
            let next_statement = body.statements.get(idx + 1);
            metadata.analyze_statement(statement, next_statement, diagnostics);
        }

        metadata
    }

    /// Walk the parts of a statement, appending metadata.
    pub fn analyze_statement(
        &mut self,
        statement: &thir::Statement<ExprMetadata>,
        next_statement: Option<&thir::Statement<ExprMetadata>>,
        diagnostics: &mut Diagnostics,
    ) {
        match statement {
            thir::Statement::HeaderContextEnter(header) => {
                self.markdown_headers.insert(header.title.clone());
            }
            thir::Statement::Let {
                value, watch, name, ..
            } => {
                if let Some(spec) = watch {
                    match &value.meta().1 {
                        Some(var_type) => {
                            // Check if the next statement immediately renames the channel
                            let immediate_rename =
                                Self::is_immediate_channel_rename(next_statement, name);

                            // Only create the default channel if it's not immediately renamed
                            if immediate_rename.is_none() {
                                self.push_watch_var(name.clone(), spec.clone(), var_type.clone());
                            }

                            // If the WatchSpec has a different configured name, create that channel too
                            if &spec.name != name {
                                self.push_watch_var(
                                    spec.name.clone(),
                                    spec.clone(),
                                    var_type.clone(),
                                );
                            }

                            // If there's an immediate rename, create that channel instead
                            if let Some(new_channel) = immediate_rename {
                                self.push_watch_var(new_channel, spec.clone(), var_type.clone());
                            }
                        }
                        None => {
                            diagnostics.push_error(DatamodelError::new_validation_error(
                                &format!("Variable '{name}' has no type"),
                                spec.span.clone(),
                            ));
                        }
                    }
                }
                self.analyze_expression(value, diagnostics);
            }
            thir::Statement::SemicolonExpression { expr, .. } => {
                self.analyze_expression(expr, diagnostics);
            }
            thir::Statement::Declare { .. } => {}
            thir::Statement::Assign { value, .. } => {
                self.analyze_expression(value, diagnostics);
            }
            thir::Statement::AssignOp { value, .. } => {
                self.analyze_expression(value, diagnostics);
            }
            thir::Statement::DeclareAndAssign {
                value, watch, name, ..
            } => {
                if let Some(spec) = watch {
                    match &value.meta().1 {
                        Some(var_type) => {
                            // Check if the next statement immediately renames the channel
                            let immediate_rename =
                                Self::is_immediate_channel_rename(next_statement, name);

                            // Only create the default channel if it's not immediately renamed
                            if immediate_rename.is_none() {
                                self.push_watch_var(name.clone(), spec.clone(), var_type.clone());
                            }

                            // If the WatchSpec has a different configured name, create that channel too
                            if &spec.name != name {
                                self.push_watch_var(
                                    spec.name.clone(),
                                    spec.clone(),
                                    var_type.clone(),
                                );
                            }

                            // If there's an immediate rename, create that channel instead
                            if let Some(new_channel) = immediate_rename {
                                self.push_watch_var(new_channel, spec.clone(), var_type.clone());
                            }
                        }
                        None => {
                            diagnostics.push_error(DatamodelError::new_validation_error(
                                &format!("Variable '{name}' has no type"),
                                spec.span.clone(),
                            ));
                        }
                    }
                }
                self.analyze_expression(value, diagnostics);
            }
            thir::Statement::Return { expr, .. } => {
                self.analyze_expression(expr, diagnostics);
            }
            thir::Statement::Expression { expr, .. } => {
                self.analyze_expression(expr, diagnostics);
            }
            thir::Statement::While {
                condition, block, ..
            } => {
                self.analyze_expression(condition, diagnostics);
                for s in block.statements.iter() {
                    self.analyze_statement(s, None, diagnostics);
                }
            }
            thir::Statement::ForLoop { block, .. } => {
                for s in block.statements.iter() {
                    self.analyze_statement(s, None, diagnostics);
                }
            }
            thir::Statement::CForLoop { block, .. } => {
                for s in block.statements.iter() {
                    self.analyze_statement(s, None, diagnostics);
                }
            }
            thir::Statement::Break(_) => {}
            thir::Statement::Continue(_) => {}
            thir::Statement::Assert { condition, .. } => {
                self.analyze_expression(condition, diagnostics);
            }
            thir::Statement::WatchOptions {
                variable,
                channel,
                span,
                ..
            } => {
                // If a new channel name is configured, we need to create that channel
                if let Some(new_channel) = channel {
                    // Find the existing watch variable for this variable to get its type
                    if let Some((_, var_type)) = self.watch_vars.get(variable) {
                        // Create a channel for the new channel name if it doesn't already exist
                        if !self.watch_vars.contains_key(new_channel) {
                            self.push_watch_var(
                                new_channel.clone(),
                                crate::watch::WatchSpec {
                                    name: new_channel.clone(),
                                    when: crate::watch::WatchWhen::Auto,
                                    span: span.clone(),
                                },
                                var_type.clone(),
                            );
                        }
                    }
                }
            }
            thir::Statement::WatchNotify { .. } => {
                // Manual notification - no static analysis needed
            }
        };
    }

    /// Walk the parts of an expression, appending metadata.
    pub fn analyze_expression(
        &mut self,
        expression: &thir::Expr<ExprMetadata>,
        diagnostics: &mut Diagnostics,
    ) {
        match expression {
            thir::Expr::ArrayAccess { base, index, .. } => {
                self.analyze_expression(base, diagnostics);
                self.analyze_expression(index, diagnostics);
            }
            thir::Expr::FieldAccess { base, .. } => {
                self.analyze_expression(base, diagnostics);
            }
            thir::Expr::MethodCall { receiver, args, .. } => {
                self.analyze_expression(receiver, diagnostics);
                for arg in args {
                    self.analyze_expression(arg, diagnostics);
                }
            }
            thir::Expr::Value(_) => {}
            thir::Expr::Var(_, _) => {}
            thir::Expr::Builtin(_, _) => {}
            thir::Expr::Function(_, body, _) => {
                for statement in &body.statements {
                    self.analyze_statement(statement, None, diagnostics);
                }
            }
            thir::Expr::If(condition, if_branch, else_branch, _) => {
                self.analyze_expression(condition, diagnostics);
                self.analyze_expression(if_branch, diagnostics);
                if let Some(expr) = else_branch {
                    self.analyze_expression(expr, diagnostics);
                }
            }
            thir::Expr::List(elements, _) => {
                for element in elements {
                    self.analyze_expression(element, diagnostics);
                }
            }
            thir::Expr::Map(kvs, _) => {
                for (_, value) in kvs {
                    self.analyze_expression(value, diagnostics);
                }
            }
            thir::Expr::Call { func, args, .. } => {
                match func.as_ref() {
                    thir::Expr::Var(ident, _) => {
                        self.subfunctions.insert(ident.clone());
                    }
                    other_expr => {
                        self.analyze_expression(other_expr, diagnostics);
                    }
                }
                for arg in args {
                    self.analyze_expression(arg, diagnostics);
                }
            }
            thir::Expr::ClassConstructor { fields, .. } => {
                for f in fields {
                    match f {
                        ClassConstructorField::Named { value, .. } => {
                            self.analyze_expression(value, diagnostics);
                        }
                        ClassConstructorField::Spread { value } => {
                            self.analyze_expression(value, diagnostics);
                        }
                    }
                }
            }
            thir::Expr::Block(block, _) => {
                for stmt in block.statements.iter() {
                    self.analyze_statement(stmt, None, diagnostics);
                }
            }
            thir::Expr::BinaryOperation { left, right, .. } => {
                self.analyze_expression(left, diagnostics);
                self.analyze_expression(right, diagnostics);
            }
            thir::Expr::UnaryOperation { expr, .. } => {
                self.analyze_expression(expr, diagnostics);
            }
            thir::Expr::Paren(expr, _) => {
                self.analyze_expression(expr, diagnostics);
            }
        };
    }

    /// Given the map from function name to metadata, each function's
    /// transitive closure of function calls.
    pub fn transitive_closures(env: &BamlMap<String, Self>) -> BamlMap<String, HashSet<String>> {
        let mut closures = BamlMap::new();
        for (name, _func) in env {
            let mut visited = HashSet::new();
            let mut stack = vec![name.clone()];
            while let Some(current) = stack.pop() {
                if visited.contains(&current) {
                    continue;
                }
                visited.insert(current.clone());
                if let Some(func) = env.get(&current) {
                    for call in func.subfunctions.iter() {
                        stack.push(call.clone());
                    }
                }
            }
            closures.insert(name.clone(), visited);
        }
        closures
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;
    use crate::{hir::Hir, thir::typecheck::typecheck};

    #[test]
    // Test that transitive closure is computer correctly.
    // A -> [A, B]
    // B -> [A, B, C]
    // C -> [C]
    fn transitive_closures() {
        let mut diagnostics = Diagnostics::new(PathBuf::from("test"));
        let hir = Hir::from_source(
            r#"
            function A() -> int {
                B();
                1
            }
            function B() -> int {
                C();
                if (true) {
                  A();
                } else {
                  B();
                }
                2
            }
            function C() -> int {
                3
            }
        "#,
        );
        let thir = typecheck(&hir, &mut diagnostics);

        let function_metadatas: BamlMap<String, FunctionMetadata> = thir
            .expr_functions
            .iter()
            .map(|f| {
                (
                    f.name.clone(),
                    FunctionMetadata::analyze_function(f, &mut diagnostics),
                )
            })
            .collect();

        let closures = FunctionMetadata::transitive_closures(&function_metadatas);
        assert_eq!(
            closures["A"],
            HashSet::from_iter(["A".to_string(), "B".to_string(), "C".to_string(),])
        );
        assert_eq!(
            closures["B"],
            HashSet::from_iter(["A".to_string(), "B".to_string(), "C".to_string()])
        );
        assert_eq!(closures["C"], HashSet::from_iter(["C".to_string()]));
    }

    #[test]
    fn test_watch() {
        let hir = Hir::from_source(
            r#"
            function A() -> int {
                watch let a_1 = 1;
                watch let a_2 = true;
                B();
                1
            }
            function B() -> int {
                C();
                if (true) {
                  watch let b_1 = "hey";
                  A();
                } else {
                  B();
                }
                2
            }
            function C() -> int {
                3
            }
        "#,
        );
        let mut diagnostics = Diagnostics::new(PathBuf::from("test"));
        let thir = typecheck(&hir, &mut diagnostics);

        let watch_channels = WatchChannels::analyze_program(&thir, &mut diagnostics);
        let a_channels = watch_channels.functions_channels.get("A").unwrap();
        let b_channels = watch_channels.functions_channels.get("B").unwrap();
        let c_channels = watch_channels.functions_channels.get("C").unwrap();

        // C() has no channels.
        assert_eq!(c_channels.channels.len(), 0);

        // Without .$watch.options() to customize names, channels use variable names
        // B() has its direct channel for b_1, and its indirect channels for a_1 and a_2.
        println!("B channels: {:?}", b_channels.channels);
        println!("A channels: {:?}", a_channels.channels);
        // Just verify we have the right number of channels for now
        assert_eq!(b_channels.channels.len(), 3);
        assert_eq!(
            b_channels
                .channels
                .iter()
                .filter(|channel| channel.0.name == "b_1"
                    && channel.0.namespace.is_none()
                    && channel.0.r#type == ChannelType::Variable)
                .count(),
            1
        );
        assert_eq!(
            b_channels
                .channels
                .iter()
                .filter(|channel| channel.0.namespace == Some("A".to_string())
                    && channel.0.r#type == ChannelType::Variable)
                .count(),
            2
        );

        // A() has its direct channel for a_1 and a_2, and its indirect channel for b_1.
        assert_eq!(a_channels.channels.len(), 3);
        assert_eq!(
            a_channels
                .channels
                .iter()
                .filter(|channel| channel.0.namespace.is_none())
                .count(),
            2
        );
        assert_eq!(
            a_channels
                .channels
                .iter()
                .filter(|channel| channel.0.namespace == Some("B".to_string()))
                .count(),
            1
        );
    }

    #[test]
    fn test_watch_channel_sharing() {
        let hir = Hir::from_source(
            r#"
                function A() -> int {
                    watch let a_1: int = 1;
                    a_1.$watch.options(baml.WatchOptions{channel: "a"});
                    watch let a_2: string = "hi";
                    a_2.$watch.options(baml.WatchOptions{channel: "a"});
                    watch let b_1: int | bool = true;
                    b_1.$watch.options(baml.WatchOptions{channel: "b"});
                    watch let b_2: int = 3;
                    b_2.$watch.options(baml.WatchOptions{channel: "b"});
                    watch let c_1: int = 1;
                    c_1.$watch.options(baml.WatchOptions{channel: "c"});
                    watch let c_2: int | bool = 3;
                    c_2.$watch.options(baml.WatchOptions{channel: "c"});
                    1
                }
            "#,
        );
        let mut diagnostics = Diagnostics::new(PathBuf::from("test"));
        let thir = typecheck(&hir, &mut diagnostics);

        let watch_channels = WatchChannels::analyze_program(&thir, &mut diagnostics);
        let mut a_channels = watch_channels
            .functions_channels
            .get("A")
            .unwrap()
            .clone()
            .channels
            .into_iter()
            .collect::<Vec<_>>();
        a_channels.sort_by(|a, b| a.0.name.cmp(&b.0.name));

        assert_eq!(a_channels.len(), 3);
        let mut a_channels_iter = a_channels.iter();

        if let TypeIR::Union(union_view, _) = &a_channels_iter.next().unwrap().1 {
            let variants = union_view.iter_skip_null();
            assert_eq!(variants, vec![&TypeIR::int(), &TypeIR::string()]);
        }
        if let TypeIR::Union(union_view, _) = &a_channels_iter.next().unwrap().1 {
            let variants = union_view.iter_skip_null();
            assert_eq!(variants, vec![&TypeIR::int(), &TypeIR::bool()]);
        }
        if let TypeIR::Union(union_view, _) = &a_channels_iter.next().unwrap().1 {
            let variants = union_view.iter_skip_null();
            assert_eq!(variants, vec![&TypeIR::int(), &TypeIR::bool()]);
        }
    }

    #[test]
    fn test_let_watch_simple() {
        let hir = Hir::from_source(
            r#"
            function A() -> int {
                watch let a_1 = 1;
                1
            }
        "#,
        );
        assert_eq!(hir.expr_functions.len(), 1);
    }

    #[test]
    fn test_watch_let_with_options() {
        let hir = Hir::from_source(
            r#"
            function A() -> int {
                let a_1 = 1;
                a_1 + 2;
                1
            }
            function B() -> int {
                2
            }
        "#,
        );
        assert_eq!(hir.expr_functions.len(), 2);
    }

    #[test]
    fn test_watch_let_shared_channel() {
        let hir = Hir::from_source(
            r#"
            function A() -> int {
              watch let x = 1;
              x.$watch.notify();
              x.$watch.options(baml.WatchOptions{ channel: "c"});
              watch let y = 1;
              y.$watch.options(baml.WatchOptions{ channel: "c"});
              0
            }
            "#,
        );
        let mut diagnostics = Diagnostics::new(PathBuf::from("test"));
        let thir = typecheck(&hir, &mut diagnostics);
        let watch_channels = WatchChannels::analyze_program(&thir, &mut diagnostics);
        let a_channels = watch_channels.functions_channels.get("A").unwrap();

        // Should have 2 channels: "x" and "c"
        // "y" is NOT created because it's immediately renamed to "c"
        assert_eq!(a_channels.channels.len(), 2);

        // Check that we have a channel named "x"
        assert_eq!(
            a_channels
                .channels
                .iter()
                .filter(|channel| channel.0.name == "x"
                    && channel.0.namespace.is_none()
                    && channel.0.r#type == ChannelType::Variable)
                .count(),
            1
        );

        // Check that we DO NOT have a channel named "y" (immediately renamed)
        assert_eq!(
            a_channels
                .channels
                .iter()
                .filter(|channel| channel.0.name == "y")
                .count(),
            0
        );

        // Check that we have a channel named "c" (shared by both x and y)
        assert_eq!(
            a_channels
                .channels
                .iter()
                .filter(|channel| channel.0.name == "c"
                    && channel.0.namespace.is_none()
                    && channel.0.r#type == ChannelType::Variable)
                .count(),
            1
        );
    }

    #[test]
    #[ignore = "$watch special field not yet implemented"]
    fn test_field_access_statement() {
        let hir = Hir::from_source(
            r#"
            function A() -> int {
                watch let a_1: int = 1;
                a_1.$watch;
                1
            }
        "#,
        );
        assert_eq!(hir.expr_functions.len(), 1);
    }

    #[test]
    fn test_namespaced_constructor() {
        let hir = Hir::from_source(
            r#"
            class WatchOptions {
                channel string
            }

            function A() -> int {
                let opts = baml.WatchOptions{channel: "test"};
                1
            }
        "#,
        );
        assert_eq!(hir.expr_functions.len(), 1);
    }

    #[test]
    fn test_non_namespaced_constructor() {
        let hir = Hir::from_source(
            r#"
            class WatchOptions {
                channel string
            }

            function A() -> int {
                let opts = WatchOptions{channel: "test"};
                1
            }
        "#,
        );
        assert_eq!(hir.expr_functions.len(), 1);
    }

    #[test]
    fn test_immediate_channel_rename_suppresses_default() {
        let hir = Hir::from_source(
            r#"
            function A() -> int {
              watch let x = 1;
              x.$watch.options(baml.WatchOptions{ channel: "custom_name" });
              watch let y = 2;
              y.$watch.options(baml.WatchOptions{ channel: "another_name" });
              watch let z = 3;
              // z is not immediately renamed, so it should create a "z" channel
              0
            }
            "#,
        );
        let mut diagnostics = Diagnostics::new(PathBuf::from("test"));
        let thir = typecheck(&hir, &mut diagnostics);
        let watch_channels = WatchChannels::analyze_program(&thir, &mut diagnostics);
        let a_channels = watch_channels.functions_channels.get("A").unwrap();

        // Should have 3 channels: "custom_name", "another_name", and "z"
        // NOT "x" or "y" since they are immediately renamed
        assert_eq!(a_channels.channels.len(), 3);

        // Check that we DO NOT have a channel named "x"
        assert_eq!(
            a_channels
                .channels
                .iter()
                .filter(|channel| channel.0.name == "x")
                .count(),
            0
        );

        // Check that we DO NOT have a channel named "y"
        assert_eq!(
            a_channels
                .channels
                .iter()
                .filter(|channel| channel.0.name == "y")
                .count(),
            0
        );

        // Check that we DO have a channel named "z" (not immediately renamed)
        assert_eq!(
            a_channels
                .channels
                .iter()
                .filter(|channel| channel.0.name == "z"
                    && channel.0.namespace.is_none()
                    && channel.0.r#type == ChannelType::Variable)
                .count(),
            1
        );

        // Check that we have a channel named "custom_name"
        assert_eq!(
            a_channels
                .channels
                .iter()
                .filter(|channel| channel.0.name == "custom_name"
                    && channel.0.namespace.is_none()
                    && channel.0.r#type == ChannelType::Variable)
                .count(),
            1
        );

        // Check that we have a channel named "another_name"
        assert_eq!(
            a_channels
                .channels
                .iter()
                .filter(|channel| channel.0.name == "another_name"
                    && channel.0.namespace.is_none()
                    && channel.0.r#type == ChannelType::Variable)
                .count(),
            1
        );
    }

    #[test]
    fn test_immediate_rename_with_intervening_statement() {
        let hir = Hir::from_source(
            r#"
            function A() -> int {
              watch let x = 1;
              let y = 2;  // Intervening statement
              x.$watch.options(baml.WatchOptions{ channel: "custom_name" });
              0
            }
            "#,
        );
        let mut diagnostics = Diagnostics::new(PathBuf::from("test"));
        let thir = typecheck(&hir, &mut diagnostics);
        let watch_channels = WatchChannels::analyze_program(&thir, &mut diagnostics);
        let a_channels = watch_channels.functions_channels.get("A").unwrap();

        // Should have both "x" and "custom_name" channels since there's an intervening statement
        assert_eq!(a_channels.channels.len(), 2);

        // Check that we DO have a channel named "x" (because rename is not immediate)
        assert_eq!(
            a_channels
                .channels
                .iter()
                .filter(|channel| channel.0.name == "x")
                .count(),
            1
        );

        // Check that we also have "custom_name"
        assert_eq!(
            a_channels
                .channels
                .iter()
                .filter(|channel| channel.0.name == "custom_name")
                .count(),
            1
        );
    }

    #[test]
    fn test_immediate_rename_with_when_filter() {
        let hir = Hir::from_source(
            r#"
            function MyFilter(val: int) -> bool {
              true
            }

            function A() -> int {
              watch let x = 1;
              x.$watch.options(baml.WatchOptions{ channel: "custom", when: MyFilter });
              0
            }
            "#,
        );
        let mut diagnostics = Diagnostics::new(PathBuf::from("test"));
        let thir = typecheck(&hir, &mut diagnostics);
        let watch_channels = WatchChannels::analyze_program(&thir, &mut diagnostics);
        let a_channels = watch_channels.functions_channels.get("A").unwrap();

        // Should have only "custom" - even with a when filter, if a custom channel is set immediately,
        // the default channel is suppressed
        assert_eq!(a_channels.channels.len(), 1);

        // Check that we DO NOT have a channel named "x" (suppressed by immediate rename)
        assert_eq!(
            a_channels
                .channels
                .iter()
                .filter(|channel| channel.0.name == "x")
                .count(),
            0
        );

        // Check that we have "custom"
        assert_eq!(
            a_channels
                .channels
                .iter()
                .filter(|channel| channel.0.name == "custom")
                .count(),
            1
        );
    }
}
