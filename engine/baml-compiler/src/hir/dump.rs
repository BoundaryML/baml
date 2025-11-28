//! Pretty printing for HIR.

use baml_types::ir_type::TypeIR;
use pretty::RcDoc;

use crate::{
    hir::{
        AssignOp, BinaryOperator, Block, Class, ClassConstructorField, Enum, EnumVariant,
        ExprFunction, Expression, Field, Hir, LlmFunction, Parameter, Statement, TypeArg,
        UnaryOperator,
    },
    watch::{WatchSpec, WatchWhen},
};

impl Hir {
    /// Convert HIR to a pretty printing document
    pub fn to_doc(&self) -> RcDoc<'static, ()> {
        let mut docs = Vec::new();
        // Add expression functions
        for func in &self.expr_functions {
            docs.push(func.to_doc());
        }
        // Add LLM functions
        for func in &self.llm_functions {
            docs.push(func.to_doc());
        }
        // Add classes (excluding builtins)
        for class in &self.classes {
            if !crate::builtin::is_builtin_class(&class.name) {
                docs.push(class.to_doc());
            }
        }
        // Add enums (excluding builtins)
        for enum_def in &self.enums {
            if !crate::builtin::is_builtin_enum(&enum_def.name) {
                docs.push(enum_def.to_doc());
            }
        }
        if docs.is_empty() {
            RcDoc::nil()
        } else {
            RcDoc::intersperse(docs, RcDoc::hardline().append(RcDoc::hardline()))
        }
    }

    pub fn pretty_print(&self) -> String {
        self.pretty_print_with_options(80, 2)
    }

    /// Pretty print the HIR with custom line width and indent width
    pub fn pretty_print_with_options(&self, line_width: usize, _indent_width: isize) -> String {
        let doc = self.to_doc();
        let mut output = Vec::new();
        doc.render(line_width, &mut output).unwrap();
        String::from_utf8(output).unwrap()
    }
}

pub trait TypeDocumentRender {
    fn to_doc(&self) -> RcDoc<'static, ()>;
}

impl TypeDocumentRender for TypeIR {
    fn to_doc(&self) -> RcDoc<'static, ()> {
        let meta = self.meta();
        let base = match self {
            TypeIR::Top(_) => RcDoc::text("ANY"),
            TypeIR::Primitive(baml_types::TypeValue::Int, _) => RcDoc::text("int"),
            TypeIR::Primitive(baml_types::TypeValue::Float, _) => RcDoc::text("float"),
            TypeIR::Primitive(baml_types::TypeValue::String, _) => RcDoc::text("string"),
            TypeIR::Primitive(baml_types::TypeValue::Bool, _) => RcDoc::text("bool"),
            TypeIR::Primitive(baml_types::TypeValue::Null, _) => RcDoc::text("null"),
            TypeIR::Primitive(baml_types::TypeValue::Media(media_type), _) => {
                RcDoc::text(format!("{media_type}"))
            }
            TypeIR::List(inner, _) => RcDoc::text("array<")
                .append(inner.to_doc())
                .append(RcDoc::text(">")),
            TypeIR::Map(key, value, _) => RcDoc::text("map<")
                .append(key.to_doc())
                .append(RcDoc::text(", "))
                .append(value.to_doc())
                .append(RcDoc::text(">")),
            TypeIR::Class { name, .. } => RcDoc::text(name.clone()),
            TypeIR::Enum { name, .. } => RcDoc::text(name.clone()),
            TypeIR::Union(union_type, _) => {
                let types = union_type.iter_include_null();
                let mut docs = Vec::new();
                for type_ in types {
                    docs.push(type_.to_doc());
                }
                RcDoc::text("(")
                    .append(RcDoc::intersperse(docs, RcDoc::text(" | ")))
                    .append(RcDoc::text(")"))
            }
            TypeIR::Arrow(arrow, _) => RcDoc::text("(")
                .append(RcDoc::intersperse(
                    arrow.param_types.iter().map(|i| i.to_doc()),
                    RcDoc::text(", "),
                ))
                .append(RcDoc::text(") -> "))
                .append(arrow.return_type.to_doc()),
            TypeIR::Literal(literal, _) => RcDoc::text(format!("{literal}")),
            TypeIR::RecursiveTypeAlias { name, .. } => RcDoc::text(name.clone()),
            TypeIR::Tuple(types, _) => RcDoc::text("(")
                .append(RcDoc::intersperse(
                    types.iter().map(|t| t.to_doc()),
                    RcDoc::text(", "),
                ))
                .append(RcDoc::text(")")),
        };

        let mut doc = base;
        if !meta.constraints.is_empty() {
            doc = doc
                .append(RcDoc::space())
                .append(RcDoc::text("@constrained"));
        }
        if meta.streaming_behavior.done {
            doc = doc.append(RcDoc::text(" @stream.done"));
        }
        if meta.streaming_behavior.state {
            doc = doc.append(RcDoc::text(" @stream.with_state"));
        }
        if meta.streaming_behavior.needed {
            doc = doc.append(RcDoc::text(" @stream.needed"));
        }
        doc
    }
}

impl Statement {
    pub fn to_doc(&self) -> RcDoc<'static, ()> {
        match self {
            Statement::HeaderContextEnter(header) => RcDoc::text("//")
                .append(RcDoc::text("#".repeat(header.level as usize)))
                .append(RcDoc::space())
                .append(RcDoc::text(header.title.clone())),
            Statement::Let {
                name,
                value,
                annotated_type,
                watch,
                ..
            } => RcDoc::text("let")
                .append(RcDoc::space())
                .append(RcDoc::text(name.clone()))
                .append(match annotated_type {
                    Some(t) => RcDoc::text(": ").append(t.to_doc()),
                    None => RcDoc::nil(),
                })
                .append(RcDoc::space())
                .append(RcDoc::text("="))
                .append(RcDoc::space())
                .append(value.to_doc())
                .append(match watch {
                    Some(watch) => watch.to_doc(),
                    None => RcDoc::nil(),
                })
                .append(RcDoc::text(";")),
            Statement::Declare { name, .. } => RcDoc::text("var")
                .append(RcDoc::space())
                .append(RcDoc::text(name.clone()))
                .append(RcDoc::text(";")),
            Statement::Assign { left, value, .. } => left
                .to_doc()
                .append(RcDoc::space())
                .append(RcDoc::text("="))
                .append(RcDoc::space())
                .append(value.to_doc())
                .append(RcDoc::text(";")),
            Statement::AssignOp {
                left,
                value,
                assign_op,
                ..
            } => left
                .to_doc()
                .append(RcDoc::space())
                .append(assign_op.to_doc())
                .append(RcDoc::space())
                .append(value.to_doc())
                .append(RcDoc::text(";")),
            Statement::DeclareAndAssign {
                name,
                value,
                annotated_type,
                watch,
                ..
            } => RcDoc::text("let")
                .append(RcDoc::space())
                .append(RcDoc::text(name.clone()))
                .append(match annotated_type {
                    Some(t) => RcDoc::text(": ").append(t.to_doc()),
                    None => RcDoc::nil(),
                })
                .append(RcDoc::space())
                .append(RcDoc::text("="))
                .append(RcDoc::space())
                .append(value.to_doc())
                .append(match watch {
                    Some(watch) => watch.to_doc(),
                    None => RcDoc::nil(),
                })
                .append(RcDoc::text(";")),
            Statement::Return { expr, .. } => RcDoc::text("return")
                .append(RcDoc::space())
                .append(expr.to_doc())
                .append(RcDoc::text(";")),
            Statement::Assert { condition, .. } => RcDoc::text("assert")
                .append(RcDoc::space())
                .append(condition.to_doc())
                .append(RcDoc::text(";")),
            Statement::Expression { expr, .. } => expr.to_doc(),
            Statement::Semicolon { expr, .. } => expr.to_doc(),
            Statement::While {
                condition, block, ..
            } => RcDoc::text("while")
                .append(RcDoc::space())
                .append(condition.to_doc())
                .append(RcDoc::space())
                .append(RcDoc::text("{"))
                .append(RcDoc::hardline())
                .append(block.to_doc().nest(2))
                .append(RcDoc::hardline())
                .append(RcDoc::text("}")),
            Statement::ForLoop {
                identifier,
                iterator,
                block,
                ..
            } => RcDoc::text("for")
                .append(RcDoc::space())
                .append(RcDoc::text(identifier.clone()))
                .append(RcDoc::space())
                .append(RcDoc::text("in"))
                .append(RcDoc::space())
                .append(iterator.to_doc())
                .append(RcDoc::space())
                .append(RcDoc::text("{"))
                .append(RcDoc::hardline())
                .append(block.to_doc().nest(2))
                .append(RcDoc::hardline())
                .append(RcDoc::text("}")),
            Statement::Break(_) => RcDoc::text("break").append(RcDoc::text(";")),
            Statement::Continue(_) => RcDoc::text("continue").append(RcDoc::text(";")),
            Statement::CForLoop {
                condition,
                after,
                block,
            } => {
                let condition = condition.as_ref().map(Expression::to_doc);
                let after = after.as_ref().map(|b| b.to_doc());
                let block = block.to_doc();

                // for with no init statement.
                let mut cur = RcDoc::text("for")
                    .append(RcDoc::space())
                    .append(RcDoc::text("("))
                    .append(RcDoc::text(";"));

                cur = match condition {
                    Some(cond) => cur.append(cond),
                    None => cur,
                };

                cur = cur.append(RcDoc::text(";"));

                cur = match after {
                    Some(after) => cur.append(after),
                    None => cur,
                };

                cur = cur.append(RcDoc::text(")")).append(RcDoc::space());

                cur.append(RcDoc::text("{"))
                    .append(RcDoc::space())
                    .append(block)
                    .append(RcDoc::text("}"))
            }
            Statement::WatchOptions {
                variable,
                channel,
                when,
                ..
            } => {
                let mut doc = RcDoc::text(variable.clone()).append(RcDoc::text(".$watch.options("));

                let mut parts = vec![];
                if let Some(n) = channel {
                    parts.push(
                        RcDoc::text("name: \"")
                            .append(RcDoc::text(n.clone()))
                            .append(RcDoc::text("\"")),
                    );
                }
                if let Some(w) = when {
                    parts.push(RcDoc::text("when: ").append(RcDoc::text(format!("{:?}", w))));
                }

                doc = doc.append(RcDoc::intersperse(parts, RcDoc::text(", ")));
                doc.append(RcDoc::text(");"))
            }
            Statement::WatchNotify { variable, .. } => {
                RcDoc::text(variable.clone()).append(RcDoc::text(".$watch.notify();"))
            }
        }
    }
}

impl LlmFunction {
    pub fn to_doc(&self) -> RcDoc<'static, ()> {
        RcDoc::text("function")
            .append(RcDoc::space())
            .append(RcDoc::text(self.name.clone()))
            .append(RcDoc::text("("))
            .append(self.parameters_to_doc())
            .append(RcDoc::text(")"))
            .append(RcDoc::space())
            .append(RcDoc::text("->"))
            .append(RcDoc::space())
            .append(self.return_type.to_doc())
            .append(RcDoc::space())
            .append(RcDoc::text("{"))
            .append(RcDoc::hardline())
            .append(
                RcDoc::text("client")
                    .append(RcDoc::space())
                    .append(RcDoc::text(self.client.clone()))
                    .append(RcDoc::hardline())
                    .append(RcDoc::text("prompt"))
                    .append(RcDoc::space())
                    .append(RcDoc::text(self.prompt.clone()))
                    .nest(2),
            )
            .append(RcDoc::hardline())
            .append(RcDoc::text("}"))
    }

    fn parameters_to_doc(&self) -> RcDoc<'static, ()> {
        if self.parameters.is_empty() {
            RcDoc::nil()
        } else {
            let param_docs: Vec<_> = self.parameters.iter().map(|p| p.to_doc()).collect();
            RcDoc::intersperse(param_docs, RcDoc::text(",").append(RcDoc::space()))
        }
    }
}

impl ExprFunction {
    pub fn to_doc(&self) -> RcDoc<'static, ()> {
        // TODO: Why nesting doesn't work if calling self.body.to_doc().nest(2)?
        let mut body_doc = if self.body.statements.is_empty() {
            RcDoc::nil()
        } else {
            // The key is to apply nest() to the entire content that includes line breaks
            RcDoc::hardline()
                .append(RcDoc::intersperse(
                    self.body
                        .statements
                        .iter()
                        .map(|s| s.to_doc())
                        .collect::<Vec<_>>(),
                    RcDoc::hardline(),
                ))
                .append(RcDoc::hardline())
                .nest(2)
        };

        if let Some(expr) = &self.body.trailing_expr {
            body_doc = body_doc.append(
                RcDoc::hardline()
                    .append(expr.to_doc().append(RcDoc::hardline()))
                    .nest(2),
            );
        }

        RcDoc::text("function")
            .append(RcDoc::space())
            .append(RcDoc::text(self.name.clone()))
            .append(RcDoc::text("("))
            .append(self.parameters_to_doc())
            .append(RcDoc::text(")"))
            .append(RcDoc::space())
            .append(RcDoc::text("{"))
            .append(body_doc)
            .append(RcDoc::text("}"))
    }

    fn parameters_to_doc(&self) -> RcDoc<'static, ()> {
        if self.parameters.is_empty() {
            RcDoc::nil()
        } else {
            let param_docs: Vec<_> = self.parameters.iter().map(|p| p.to_doc()).collect();
            RcDoc::intersperse(param_docs, RcDoc::text(",").append(RcDoc::space()))
        }
    }
}

impl Block {
    pub fn to_doc(&self) -> RcDoc<'static, ()> {
        let doc = if self.statements.is_empty() {
            RcDoc::nil()
        } else {
            RcDoc::intersperse(
                self.statements
                    .iter()
                    .map(|s| s.to_doc())
                    .collect::<Vec<_>>(),
                RcDoc::hardline(),
            )
        };

        if let Some(expr) = &self.trailing_expr {
            doc.append(RcDoc::hardline()).append(expr.to_doc())
        } else {
            doc
        }
    }
}

impl Expression {
    pub fn to_doc(&self) -> RcDoc<'static, ()> {
        match self {
            Expression::ArrayAccess { base, index, .. } => base
                .to_doc()
                .append(RcDoc::text("["))
                .append(index.to_doc())
                .append(RcDoc::text("]")),
            Expression::FieldAccess { base, field, .. } => base
                .to_doc()
                .append(RcDoc::text("."))
                .append(RcDoc::text(field.clone())),
            Expression::MethodCall {
                receiver,
                method,
                args,
                ..
            } => receiver
                .to_doc()
                .append(RcDoc::text("."))
                .append(RcDoc::text(method.clone()))
                .append(RcDoc::text("("))
                .append(RcDoc::intersperse(
                    args.iter().map(|a| a.to_doc()),
                    RcDoc::text(",").append(RcDoc::space()),
                ))
                .append(RcDoc::text(")")),
            Expression::BoolValue(val, _) => RcDoc::text(val.to_string()),
            Expression::NumericValue(val, _) => RcDoc::text(val.clone()),
            Expression::Identifier(name, _) => RcDoc::text(name.clone()),
            Expression::StringValue(val, _) => RcDoc::text(format!("\"{val}\"")),
            Expression::RawStringValue(val, _) => RcDoc::text(format!("#\"{val}\"#")),
            Expression::Array(values, _) => RcDoc::text("[")
                .append(if values.is_empty() {
                    RcDoc::nil()
                } else {
                    RcDoc::intersperse(
                        values.iter().map(|v| v.to_doc()).collect::<Vec<_>>(),
                        RcDoc::text(",").append(RcDoc::space()),
                    )
                })
                .append(RcDoc::text("]")),
            Expression::Map(pairs, _) => RcDoc::text("{")
                .append(if pairs.is_empty() {
                    RcDoc::nil()
                } else {
                    RcDoc::space()
                        .append(RcDoc::intersperse(
                            pairs
                                .iter()
                                .map(|(k, v)| {
                                    k.to_doc()
                                        .append(RcDoc::text(":"))
                                        .append(RcDoc::space())
                                        .append(v.to_doc())
                                })
                                .collect::<Vec<_>>(),
                            RcDoc::text(",").append(RcDoc::space()),
                        ))
                        .append(RcDoc::space())
                })
                .append(RcDoc::text("}")),
            Expression::If {
                condition,
                if_branch,
                else_branch,
                ..
            } => {
                let mut doc = RcDoc::text("if")
                    .append(RcDoc::space())
                    .append(condition.to_doc())
                    .append(RcDoc::space())
                    .append(if_branch.to_doc())
                    .append(RcDoc::space());
                if let Some(else_expr) = else_branch {
                    doc = doc
                        .append(RcDoc::text("else"))
                        .append(RcDoc::space())
                        .append(else_expr.to_doc())
                        .append(RcDoc::space());
                }
                doc
            }
            Expression::JinjaExpressionValue(val, _) => RcDoc::text(val.clone()),
            Expression::Call {
                function,
                type_args,
                args,
                ..
            } => {
                let doc = function.to_doc();
                let doc = if !type_args.is_empty() {
                    doc.append(RcDoc::text("<"))
                        .append(RcDoc::intersperse(
                            type_args.iter().map(|arg| arg.to_doc()),
                            RcDoc::text(","),
                        ))
                        .append(RcDoc::text(">"))
                } else {
                    doc
                };
                doc.append(RcDoc::text("("))
                    .append(if args.is_empty() {
                        RcDoc::nil()
                    } else {
                        RcDoc::intersperse(
                            args.iter().map(|arg| arg.to_doc()).collect::<Vec<_>>(),
                            RcDoc::text(",").append(RcDoc::space()),
                        )
                    })
                    .append(RcDoc::text(")"))
            }
            Expression::ClassConstructor(cc, _) => RcDoc::text(cc.class_name.clone())
                .append(RcDoc::space())
                .append(RcDoc::text("{"))
                .append(if cc.fields.is_empty() {
                    RcDoc::nil()
                } else {
                    RcDoc::space()
                        .append(RcDoc::intersperse(
                            cc.fields.iter().map(|f| f.to_doc()).collect::<Vec<_>>(),
                            RcDoc::text(",").append(RcDoc::space()),
                        ))
                        .append(RcDoc::space())
                })
                .append(RcDoc::text("}")),
            Expression::Block(block, _) => RcDoc::text("{")
                .append(RcDoc::hardline())
                .append(block.to_doc().nest(2))
                .append(RcDoc::hardline())
                .append(RcDoc::text("}")),
            Expression::BinaryOperation {
                left,
                operator,
                right,
                ..
            } => left
                .to_doc()
                .append(RcDoc::space())
                .append(operator.to_doc())
                .append(RcDoc::space())
                .append(right.to_doc()),
            Expression::UnaryOperation { operator, expr, .. } => {
                operator.to_doc().append(expr.to_doc())
            }
            Expression::Paren(expr, _) => RcDoc::text("(")
                .append(expr.to_doc())
                .append(RcDoc::text(")")),
        }
    }
}

impl TypeArg {
    pub fn to_doc(&self) -> RcDoc<'static, ()> {
        match self {
            TypeArg::Type(ty) => ty.to_doc(),
            TypeArg::TypeName(name) => RcDoc::text(name.clone()),
        }
    }
}

impl Field {
    pub fn to_doc(&self) -> RcDoc<'static, ()> {
        RcDoc::text(self.name.clone())
            .append(RcDoc::text(": "))
            .append(self.r#type.to_doc())
    }
}

impl Class {
    pub fn to_doc(&self) -> RcDoc<'static, ()> {
        RcDoc::text("class")
            .append(RcDoc::space())
            .append(RcDoc::text(self.name.clone()))
            .append(RcDoc::space())
            .append(RcDoc::text("{"))
            .append(if self.fields.is_empty() {
                RcDoc::nil()
            } else {
                RcDoc::hardline()
                    .append(RcDoc::intersperse(
                        self.fields.iter().map(|f| f.to_doc()).collect::<Vec<_>>(),
                        RcDoc::hardline(),
                    ))
                    .append(RcDoc::hardline())
                    .nest(2)
            })
            .append(RcDoc::text("}"))
    }
}

impl ClassConstructorField {
    pub fn to_doc(&self) -> RcDoc<'static, ()> {
        match self {
            ClassConstructorField::Named { name, value } => RcDoc::text(name.clone())
                .append(RcDoc::text(":"))
                .append(RcDoc::space())
                .append(value.to_doc()),
            ClassConstructorField::Spread { value } => RcDoc::text("..").append(value.to_doc()),
        }
    }
}

impl Enum {
    pub fn to_doc(&self) -> RcDoc<'static, ()> {
        RcDoc::text("enum")
            .append(RcDoc::space())
            .append(RcDoc::text(self.name.clone()))
            .append(RcDoc::space())
            .append(RcDoc::text("{"))
            .append(if self.variants.is_empty() {
                RcDoc::nil()
            } else {
                RcDoc::hardline()
                    .append(RcDoc::intersperse(
                        self.variants.iter().map(|v| v.to_doc()).collect::<Vec<_>>(),
                        RcDoc::hardline(),
                    ))
                    .append(RcDoc::hardline())
                    .nest(2)
            })
            .append(RcDoc::text("}"))
    }
}

impl EnumVariant {
    pub fn to_doc(&self) -> RcDoc<'static, ()> {
        RcDoc::text(self.name.clone())
    }
}
impl Parameter {
    pub fn to_doc(&self) -> RcDoc<'static, ()> {
        // For now, just show the parameter name since types aren't included in HIR
        RcDoc::text(self.name.clone())
    }
}

impl std::fmt::Display for BinaryOperator {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            BinaryOperator::Eq => "==",
            BinaryOperator::Neq => "!=",
            BinaryOperator::Lt => "<",
            BinaryOperator::LtEq => "<=",
            BinaryOperator::Gt => ">",
            BinaryOperator::GtEq => ">=",
            BinaryOperator::Add => "+",
            BinaryOperator::Sub => "-",
            BinaryOperator::Mul => "*",
            BinaryOperator::Div => "/",
            BinaryOperator::And => "&&",
            BinaryOperator::Or => "||",
            BinaryOperator::Mod => "%",
            BinaryOperator::BitAnd => "&",
            BinaryOperator::BitOr => "|",
            BinaryOperator::BitXor => "^",
            BinaryOperator::Shl => "<<",
            BinaryOperator::Shr => ">>",
            BinaryOperator::InstanceOf => "instanceof",
        })
    }
}

impl std::fmt::Display for UnaryOperator {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            UnaryOperator::Not => "!",
            UnaryOperator::Neg => "-",
        })
    }
}

impl std::fmt::Display for AssignOp {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            AssignOp::AddAssign => "+=",
            AssignOp::SubAssign => "-=",
            AssignOp::MulAssign => "*=",
            AssignOp::DivAssign => "/=",
            AssignOp::ModAssign => "%=",
            AssignOp::BitXorAssign => "^=",
            AssignOp::BitAndAssign => "&=",
            AssignOp::BitOrAssign => "|=",
            AssignOp::ShlAssign => "<<=",
            AssignOp::ShrAssign => ">>=",
        })
    }
}

impl BinaryOperator {
    pub fn to_doc(&self) -> RcDoc<'static, ()> {
        RcDoc::text(self.to_string())
    }
}

impl UnaryOperator {
    pub fn to_doc(&self) -> RcDoc<'static, ()> {
        RcDoc::text(self.to_string())
    }
}

impl AssignOp {
    pub fn to_doc(&self) -> RcDoc<'static, ()> {
        RcDoc::text(self.to_string())
    }
}

impl WatchSpec {
    pub fn to_doc(&self) -> RcDoc<'static, ()> {
        let mut args: Vec<String> = Vec::new();
        match &self.when {
            WatchWhen::Manual => args.push("when=manual".to_string()),
            WatchWhen::Auto => {}
            WatchWhen::Never => args.push("when=never".to_string()),
            WatchWhen::FunctionName(fn_name) => args.push(format!("when={fn_name}")),
        }
        args.push(format!("name={}", self.name));
        let args_doc = RcDoc::intersperse(args.iter().cloned().map(RcDoc::text), RcDoc::text(", "));
        let doc = RcDoc::space().append(RcDoc::text("@watch"));
        if args.is_empty() {
            doc
        } else {
            doc.append(RcDoc::text("(").append(args_doc).append(RcDoc::text(")")))
        }
    }
}
