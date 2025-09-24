/// Utilities for analyzing the emit variables and their dependencies.
pub mod emit_options;

pub use crate::emit::emit_options::{EmitSpec, EmitWhen};
use crate::hir::{self, ClassConstructor, ClassConstructorField, Hir};
use baml_types::{BamlMap, TypeIR};
use internal_baml_diagnostics::Diagnostics;
use std::collections::HashSet;

/// The result of analyzing the emit variables in a BAML program.
/// See `EmitChannels::analyze_program` for more details.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EmitChannels {
    pub functions_channels: BamlMap<String, FunctionChannels>,
}

impl EmitChannels {
    /// Walk a BAML program, inferring the emit channels required for each function.
    /// Throws diagnostics errors when channel invariants are violated, but makes
    /// a best-effort attempt to continue in the face of errors (the result will be
    /// an incomplete listing of channels).
    ///
    /// A function will have channels for:
    ///   - Its own markdown headers
    ///   - Its own variables
    ///   - Its transitive subfunctions' markdown headers (under a namespace)
    ///   - Its transitive subfunctions' variables (under a namespace)
    pub fn analyze_program(hir: &Hir, diagnostics: &mut Diagnostics) -> Self {
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
        EmitChannels { functions_channels }
    }

    fn convert_function(
        fn_name: &str,
        function_metadatas: &BamlMap<String, FunctionMetadata>,
        transitive_closures: &BamlMap<String, HashSet<String>>,
    ) -> FunctionChannels {
        let mut channels = HashSet::new();

        let FunctionMetadata {
            emit_vars,
            markdown_headers,
            ..
        } = &function_metadatas[fn_name];

        let md_channels = markdown_headers.into_iter().map(|header| ChannelFQN {
            namespace: None,
            r#type: ChannelType::MarkdownHeader,
            name: header.clone(),
        });
        channels.extend(md_channels);
        let var_channels = emit_vars
            .into_iter()
            .map(|(_, (emit_spec, _chan_type))| ChannelFQN {
                namespace: None,
                r#type: ChannelType::Variable,
                name: emit_spec.name.clone(),
            });
        channels.extend(var_channels);

        let mut dependencies = transitive_closures[fn_name].clone();
        dependencies.remove(fn_name);
        for subfunction in dependencies {
            let FunctionMetadata {
                markdown_headers,
                emit_vars,
                ..
            } = &function_metadatas[&subfunction];
            let sub_md_channels = markdown_headers.iter().map(|header| ChannelFQN {
                namespace: Some(subfunction.clone()),
                r#type: ChannelType::MarkdownHeader,
                name: header.clone(),
            });
            channels.extend(sub_md_channels);
            let sub_var_channels =
                emit_vars
                    .into_iter()
                    .map(|(_, (emit_spec, _chan_type))| ChannelFQN {
                        namespace: Some(subfunction.clone()),
                        r#type: ChannelType::Variable,
                        name: emit_spec.name.clone(),
                    });
            channels.extend(sub_var_channels);
        }

        FunctionChannels { channels }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FunctionChannels {
    pub channels: HashSet<ChannelFQN>,
}

/// Fully qualified name of an emit channel.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ChannelFQN {
    /// The terminal name of the channel, derived directly from the variable,
    /// header, or user-supplied-channel name.
    pub name: String,

    /// Whether the channel is used for markdown header events or variable
    /// emit events.
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
/// The fields are not transitive - they only include functions, emit vars and headers
/// used **directly** by the function.
struct FunctionMetadata {
    subfunctions: HashSet<String>,
    emit_vars: BamlMap<String, (EmitSpec, TypeIR)>,
    markdown_headers: HashSet<String>,
}

impl FunctionMetadata {
    /// Walk the statements of a function to collect metadata.
    pub fn analyze_function(function: &hir::ExprFunction, diagnostics: &mut Diagnostics) -> Self {
        let mut metadata = FunctionMetadata {
            subfunctions: HashSet::new(),
            emit_vars: BamlMap::new(),
            markdown_headers: HashSet::new(),
        };

        let hir::ExprFunction { body, .. } = function;
        for statement in body.statements.iter() {
            metadata.analyze_statement(statement, diagnostics);
        }

        metadata
    }

    /// Walk the parts of a statement, appending metadata.
    pub fn analyze_statement(&mut self, statement: &hir::Statement, diagnostics: &mut Diagnostics) {
        match statement {
            hir::Statement::Let {
                value, emit, name, ..
            } => {
                if let Some(spec) = emit {
                    self.emit_vars.insert(name.clone(), spec.clone());
                }
                self.analyze_expression(value, diagnostics);
            }
            hir::Statement::Declare { .. } => {}
            hir::Statement::Assign { value, .. } => {
                self.analyze_expression(value, diagnostics);
            }
            hir::Statement::AssignOp { value, .. } => {
                self.analyze_expression(value, diagnostics);
            }
            hir::Statement::DeclareAndAssign {
                value, emit, name, ..
            } => {
                if let Some(spec) = emit {
                    self.emit_vars.insert(name.clone(), spec.clone());
                }
                self.analyze_expression(value, diagnostics);
            }
            hir::Statement::Return { expr, .. } => {
                self.analyze_expression(expr, diagnostics);
            }
            hir::Statement::Expression { expr, .. } => {
                self.analyze_expression(expr, diagnostics);
            }
            hir::Statement::Semicolon { expr, .. } => {
                self.analyze_expression(expr, diagnostics);
            }
            hir::Statement::While {
                condition, block, ..
            } => {
                self.analyze_expression(condition, diagnostics);
                for s in block.statements.iter() {
                    self.analyze_statement(&s, diagnostics);
                }
            }
            hir::Statement::ForLoop { block, .. } => {
                for s in block.statements.iter() {
                    self.analyze_statement(&s, diagnostics);
                }
            }
            hir::Statement::CForLoop { block, .. } => {
                for s in block.statements.iter() {
                    self.analyze_statement(&s, diagnostics);
                }
            }
            hir::Statement::Break(_) => {}
            hir::Statement::Continue(_) => {}
            hir::Statement::Assert { condition, .. } => {
                self.analyze_expression(condition, diagnostics);
            }
        };
    }

    /// Walk the parts of an expression, appending metadata.
    pub fn analyze_expression(
        &mut self,
        expression: &hir::Expression,
        diagnostics: &mut Diagnostics,
    ) {
        match expression {
            hir::Expression::ArrayAccess { base, index, .. } => {
                self.analyze_expression(base, diagnostics);
                self.analyze_expression(index, diagnostics);
            }
            hir::Expression::FieldAccess { base, .. } => {
                self.analyze_expression(base, diagnostics);
            }
            hir::Expression::MethodCall { receiver, args, .. } => {
                self.analyze_expression(receiver, diagnostics);
                for arg in args {
                    self.analyze_expression(arg, diagnostics);
                }
            }
            hir::Expression::BoolValue(_, _) => {}
            hir::Expression::NumericValue(_, _) => {}
            hir::Expression::Identifier(_, _) => {}
            hir::Expression::StringValue(_, _) => {}
            hir::Expression::RawStringValue(_, _) => {}
            hir::Expression::If {
                condition,
                if_branch,
                else_branch,
                ..
            } => {
                self.analyze_expression(condition, diagnostics);
                self.analyze_expression(if_branch, diagnostics);
                if let Some(expr) = else_branch {
                    self.analyze_expression(expr, diagnostics);
                }
            }
            hir::Expression::Array(elements, _) => {
                for element in elements {
                    self.analyze_expression(element, diagnostics);
                }
            }
            hir::Expression::Map(kvs, _) => {
                for (_, value) in kvs {
                    self.analyze_expression(value, diagnostics);
                }
            }
            hir::Expression::JinjaExpressionValue(_, _) => {}
            hir::Expression::Call { function, args, .. } => {
                match function.as_ref() {
                    hir::Expression::Identifier(ident, _) => {
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
            hir::Expression::ClassConstructor(ClassConstructor { fields, .. }, _) => {
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
            hir::Expression::Block(block, _) => {
                for stmt in block.statements.iter() {
                    self.analyze_statement(stmt, diagnostics);
                }
            }
            hir::Expression::BinaryOperation { left, right, .. } => {
                self.analyze_expression(left, diagnostics);
                self.analyze_expression(right, diagnostics);
            }
            hir::Expression::UnaryOperation { expr, .. } => {
                self.analyze_expression(expr, diagnostics);
            }
            hir::Expression::Paren(expr, _) => {
                self.analyze_expression(expr, diagnostics);
            }
        };
    }

    /// Given the map from function name to metadata, each function's
    /// transitive closure of function calls.
    pub fn transitive_closures(env: &BamlMap<String, Self>) -> BamlMap<String, HashSet<String>> {
        let mut closures = BamlMap::new();
        for (name, func) in env {
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
    use super::*;
    use crate::hir::Hir;
    use std::path::PathBuf;

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

        let function_metadatas: BamlMap<String, FunctionMetadata> = hir
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
    fn test_emit() {
        let hir = Hir::from_source(
            r#"
            function A() -> int {
                let a_1 = 1 @emit();
                let a_2 = true @emit(name=a_2_renamed);
                B();
                1
            }
            function B() -> int {
                C();
                if (true) {
                  let b_1 = "hey" @emit();
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

        let emit_channels = EmitChannels::analyze_program(&hir, &mut diagnostics);
        let a_channels = emit_channels.functions_channels.get("A").unwrap();
        let b_channels = emit_channels.functions_channels.get("B").unwrap();
        let c_channels = emit_channels.functions_channels.get("C").unwrap();

        // C() has no channels.
        assert_eq!(c_channels.channels.len(), 0);

        // B() has its direct channel for b_1, and its indirect channel for a_1 and a_2.
        assert_eq!(b_channels.channels.len(), 3);
        assert_eq!(
            b_channels
                .channels
                .iter()
                .filter(|channel| channel.name == "b_1"
                    && channel.namespace == None
                    && channel.r#type == ChannelType::Variable)
                .count(),
            1
        );
        assert_eq!(
            b_channels
                .channels
                .iter()
                .filter(|channel| channel.namespace == Some("A".to_string())
                    && channel.r#type == ChannelType::Variable)
                .count(),
            2
        );

        // A() has its direct channel for a_1 and a_2, and its indirect channel for b_1.
        assert_eq!(a_channels.channels.len(), 3);
        assert_eq!(
            a_channels
                .channels
                .iter()
                .filter(|channel| channel.namespace == None)
                .count(),
            2
        );
        assert_eq!(
            a_channels
                .channels
                .iter()
                .filter(|channel| channel.namespace == Some("B".to_string()))
                .count(),
            1
        );
    }
}
