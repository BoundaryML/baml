/// Type-checked HIR.
///
use baml_types::ir_type::TypeIR;

use crate::{
    hir::{self, AssignOp, BinaryOperator, HeaderContext, LlmFunction, UnaryOperator},
    watch::{WatchSpec, WatchWhen},
};

pub mod interpret;
pub mod typecheck;

use std::{
    collections::{HashSet, VecDeque},
    sync::Arc,
};

use baml_types::{BamlMap, BamlValueWithMeta};
use internal_baml_diagnostics::Span;
use itertools::join;

/// A full BAML program.
/// This differs from HIR in a few ways:
///   - Expressions are (optionally) typed.
///   - Variables are bound or free, using the locally nameless representation.
///   - Type parameter `T` is used for both `BamlValueWithMeta` and expression meta.
#[derive(Clone, Debug)]
pub struct THir<T> {
    pub expr_functions: Vec<ExprFunction<T>>,
    pub llm_functions: Vec<LlmFunction>,
    pub global_assignments: BamlMap<String, GlobalAssignment<T>>,
    pub classes: BamlMap<String, Class<T>>,
    pub enums: BamlMap<String, Enum>,
}

#[derive(Clone, Debug)]
pub struct ExprFunction<T> {
    pub name: String,
    pub parameters: Vec<Parameter>,
    pub return_type: TypeIR,
    pub body: Block<T>,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub struct Parameter {
    pub name: String,
    pub r#type: TypeIR,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub struct Class<T> {
    pub name: String,
    pub fields: Vec<hir::Field>,
    // TODO: Allow LLM functions here.
    pub methods: Vec<ExprFunction<T>>,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub struct Enum {
    pub name: String,
    pub variants: Vec<hir::EnumVariant>,
    pub span: Span,
    pub ty: TypeIR, // TODO: Used for type checking, but do we need this?
}

#[derive(Clone, Debug)]
pub struct GlobalAssignment<T> {
    pub expr: Expr<T>,
    pub annotated_type: Option<TypeIR>,
}

#[derive(Debug, Clone)]
pub enum ClassConstructorField<T> {
    Named { name: String, value: Expr<T> },
    Spread { value: Expr<T> },
}

/// A BAML expression term.
/// T is the type of the metadata.
#[derive(Debug, Clone)]
pub enum Expr<T> {
    Value(BamlValueWithMeta<T>),
    List(Vec<Expr<T>>, T),
    Map(Vec<(String, Expr<T>)>, T),
    Block(Box<Block<T>>, T),
    ClassConstructor {
        name: String,
        fields: Vec<ClassConstructorField<T>>,
        meta: T,
    },
    Var(Name, T),
    Function(usize, Arc<Block<T>>, T), // number of parameters, body, metadata
    Call {
        func: Arc<Expr<T>>,
        type_args: Vec<TypeIR>,
        args: Vec<Expr<T>>,
        meta: T,
    },
    MethodCall {
        receiver: Arc<Expr<T>>,
        method: Arc<Expr<T>>,
        args: Vec<Expr<T>>,
        meta: T,
    },
    If(Arc<Expr<T>>, Arc<Expr<T>>, Option<Arc<Expr<T>>>, T),
    Builtin(Builtin, T),
    /// Array or map access: `base[index]`
    ArrayAccess {
        base: Arc<Expr<T>>,
        index: Arc<Expr<T>>,
        meta: T,
    },
    /// Field access: `base.field`
    FieldAccess {
        base: Arc<Expr<T>>,
        field: String,
        meta: T,
    },
    BinaryOperation {
        left: Arc<Expr<T>>,
        operator: BinaryOperator,
        right: Arc<Expr<T>>,
        meta: T,
    },
    UnaryOperation {
        operator: UnaryOperator,
        expr: Arc<Expr<T>>,
        meta: T,
    },
    Paren(Arc<Expr<T>>, T),
}

/// A block of statements and a final return value.
#[derive(Clone, Debug)]
pub struct Block<T> {
    pub env: BamlMap<Variable, Expr<T>>,
    /// List of statements.
    pub statements: Vec<Statement<T>>,
    /// Final expression in the block without semicolon (used as return).
    pub trailing_expr: Option<Expr<T>>,
    /// Type of the block.
    pub ty: Option<TypeIR>,
    pub span: Span,
}

impl<T> Block<T> {
    pub fn dump_str(&self) -> String
    where
        T: Clone + std::fmt::Debug,
    {
        let statements = join(self.statements.iter().map(|stmt| stmt.dump_str()), "\n");

        if let Some(expr) = &self.trailing_expr {
            format!("{{ {statements} {} }}", expr.dump_str())
        } else {
            format!("{{ {statements} }}")
        }
    }

    pub fn variables(&self) -> HashSet<Name>
    where
        T: Clone,
    {
        let mut vars = self
            .trailing_expr
            .as_ref()
            .map(|expr| expr.variables())
            .unwrap_or_default();

        for stmt in self.statements.iter() {
            vars.extend(stmt.variables());
        }
        vars
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Builtin {
    FetchValue,
}

pub type Name = String;

/// The locally-nameless index of a bound variable.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VarIndex {
    /// Locally nameless De Bruijn index of a variable.
    /// This is related to the number of lambda binders under
    /// which the variable is located.
    pub de_bruijn: u32,

    /// Our functions all take tuples of arguments, so the index of
    /// a bound variable must specify the tuple index in addition
    /// to the De Bruijn index.
    pub tuple: u32,
}

impl VarIndex {
    pub fn dump_str(&self) -> String {
        format!("({}.{})", self.de_bruijn, self.tuple)
    }
    pub fn deeper(&self) -> Self {
        VarIndex {
            de_bruijn: self.de_bruijn + 1,
            tuple: self.tuple,
        }
    }
}

/// The metadata used during parsing, typechecking and evaluation of BAML expressions.
pub type ExprMetadata = (Span, Option<TypeIR>);

impl<T> Expr<T> {
    pub fn meta(&self) -> &T {
        match self {
            Expr::Value(baml_value) => baml_value.meta(),
            Expr::Block(_, meta) => meta,
            Expr::List(_, meta) => meta,
            Expr::Var(_, meta) => meta,
            Expr::Function(_, _, meta) => meta,
            Expr::Map(_, meta) => meta,
            Expr::ClassConstructor { meta, .. } => meta,
            Expr::Call { meta, .. } => meta,
            Expr::Builtin(_, meta) => meta,
            Expr::If(_, _, _, meta) => meta,
            Expr::ArrayAccess { meta, .. } => meta,
            Expr::FieldAccess { meta, .. } => meta,
            Expr::BinaryOperation { meta, .. } => meta,
            Expr::UnaryOperation { meta, .. } => meta,
            Expr::MethodCall { meta, .. } => meta,
            Expr::Paren(_, meta) => meta,
        }
    }

    pub fn meta_mut(&mut self) -> &mut T {
        match self {
            Expr::Value(baml_value) => baml_value.meta_mut(),
            Expr::Block(_, meta) => meta,
            Expr::List(_, meta) => meta,
            Expr::Map(_, meta) => meta,
            Expr::ClassConstructor { meta, .. } => meta,
            Expr::Var(_, meta) => meta,
            Expr::Function(_, _, meta) => meta,
            Expr::Call { meta, .. } => meta,
            Expr::Builtin(_, meta) => meta,
            Expr::If(_, _, _, meta) => meta,
            Expr::ArrayAccess { meta, .. } => meta,
            Expr::FieldAccess { meta, .. } => meta,
            Expr::BinaryOperation { meta, .. } => meta,
            Expr::UnaryOperation { meta, .. } => meta,
            Expr::MethodCall { meta, .. } => meta,
            Expr::Paren(_, meta) => meta,
        }
    }

    pub fn into_meta(self) -> T
    where
        T: Clone,
    {
        match self {
            Expr::Value(baml_value) => baml_value.meta().clone(),
            Expr::Block(_, meta) => meta,
            Expr::List(_, meta) => meta,
            Expr::Map(_, meta) => meta,
            Expr::ClassConstructor { meta, .. } => meta,
            Expr::Var(_, meta) => meta,
            Expr::Function(_, _, meta) => meta,
            Expr::Call { meta, .. } => meta,
            Expr::Builtin(_, meta) => meta,
            Expr::If(_, _, _, meta) => meta,
            Expr::ArrayAccess { meta, .. } => meta,
            Expr::FieldAccess { meta, .. } => meta,
            Expr::BinaryOperation { meta, .. } => meta,
            Expr::UnaryOperation { meta, .. } => meta,
            Expr::MethodCall { meta, .. } => meta,
            Expr::Paren(_, meta) => meta,
        }
    }
}

impl<T: Clone + std::fmt::Debug> ClassConstructorField<T> {
    pub fn dump_str(&self) -> String {
        match self {
            ClassConstructorField::Named { name, value } => {
                format!("{}: {}", name, value.dump_str())
            }
            ClassConstructorField::Spread { value } => format!("...{}", value.dump_str()),
        }
    }
}

impl<T: Clone + std::fmt::Debug> Expr<T> {
    /// A very rough pretty-printer for debugging expressions.
    pub fn dump_str(&self) -> String {
        match self {
            Expr::Value(atom) => atom.clone().value().to_string(),
            Expr::Block(block, _) => block.dump_str(),
            Expr::Var(name, _) => name.clone(),
            Expr::Function(_, body, meta) => format!(
                "\\. -> {}",
                Expr::Block(Box::new(Arc::unwrap_or_clone(body.clone())), meta.clone()).dump_str()
            ),
            Expr::Call { func, args, .. } => {
                let args_str = itertools::join(args.iter().map(|arg| arg.dump_str()), ", ");
                let func_str = match func.as_ref() {
                    Expr::Var(name, _) => name.clone(),
                    _ => format!("({})", func.dump_str()),
                };
                format!("{func_str}({args_str})")
            }
            Expr::Builtin(builtin, _) => format!("{builtin:?}"),
            Expr::List(items, _) => {
                let items = join(
                    items.iter().map(|item| item.dump_str()).collect::<Vec<_>>(),
                    ", ",
                );
                format!("[{items}]")
            }
            Expr::Map(entries, _) => {
                let entries = entries
                    .iter()
                    .map(|(key, value)| format!("{}: {}", key, value.dump_str()))
                    .collect::<Vec<_>>()
                    .join(", ");
                format!("{{{entries}}}")
            }
            Expr::ClassConstructor { name, fields, .. } => {
                let fields_string = fields
                    .iter()
                    .map(|field| field.dump_str())
                    .collect::<Vec<_>>()
                    .join(", ");

                format!("Class({name} {{ {fields_string}}}")
            }
            Expr::If(cond, then, else_, _) => {
                format!(
                    "If {} {{ {} }} {}",
                    cond.dump_str(),
                    then.dump_str(),
                    else_.as_ref().map(|e| e.dump_str()).unwrap_or_default()
                )
            }
            Expr::ArrayAccess { base, index, .. } => {
                format!("{}[{}]", base.dump_str(), index.dump_str())
            }
            Expr::FieldAccess { base, field, .. } => {
                format!("{}.{}", base.dump_str(), field)
            }
            Expr::BinaryOperation {
                left,
                operator,
                right,
                ..
            } => format!("({} {} {})", left.dump_str(), operator, right.dump_str()),
            Expr::UnaryOperation { operator, expr, .. } => {
                format!("({} {})", operator, expr.dump_str())
            }
            Expr::MethodCall {
                receiver,
                method,
                args,
                ..
            } => format!(
                "{}.{}({})",
                receiver.dump_str(),
                match method.as_ref() {
                    Expr::Var(name, _) => name.clone(),
                    _ => format!("({})", method.dump_str()),
                },
                args.iter()
                    .map(|a| a.dump_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            Expr::Paren(expr, _) => format!("({})", expr.dump_str()),
        }
    }
}

impl<T: Clone> ClassConstructorField<T> {
    pub fn variables(&self) -> HashSet<Name> {
        match self {
            ClassConstructorField::Named { value, .. } => value.variables(),
            ClassConstructorField::Spread { value } => value.variables(),
        }
    }
}

impl<T: Clone> Expr<T> {
    pub fn variables(&self) -> HashSet<Name> {
        match self {
            Expr::Value(_) => HashSet::new(),
            Expr::Block(block, _) => block.variables(),
            Expr::List(items, _) => items.iter().flat_map(|item| item.variables()).collect(),
            Expr::Map(entries, _) => entries
                .iter()
                .flat_map(|(_, value)| value.variables())
                .collect(),
            Expr::ClassConstructor { fields, .. } => {
                fields.iter().flat_map(|field| field.variables()).collect()
            }
            Expr::Builtin(_, _) => HashSet::new(),
            Expr::Var(name, _) => HashSet::from([name.clone()]),
            Expr::Function(_, body, meta) => {
                Expr::Block(Box::new(Arc::unwrap_or_clone(body.clone())), meta.clone()).variables()
            }
            Expr::Call { func, args, .. } => {
                let mut free_vars = func.variables();
                free_vars.extend(args.iter().flat_map(|arg| arg.variables()));
                free_vars
            }
            Expr::MethodCall { receiver, args, .. } => {
                let mut free_vars = receiver.variables();
                free_vars.extend(args.iter().flat_map(|arg| arg.variables()));
                free_vars
            }
            Expr::If(cond, then, else_, _) => {
                let mut free_vars = cond.variables();
                free_vars.extend(then.variables());
                if let Some(else_) = else_ {
                    free_vars.extend(else_.variables());
                }
                free_vars
            }
            Expr::ArrayAccess { base, index, .. } => {
                let mut free_vars = base.variables();
                free_vars.extend(index.variables());
                free_vars
            }
            Expr::FieldAccess { base, .. } => base.variables(),
            Expr::BinaryOperation { left, right, .. } => {
                let mut free_vars = left.variables();
                free_vars.extend(right.variables());
                free_vars
            }
            Expr::UnaryOperation { expr, .. } => expr.variables(),
            Expr::Paren(expr, _) => expr.variables(),
        }
    }
}

/// Special methods for Exprs parameterized by the ExprMetadata type.
impl Expr<ExprMetadata> {
    /// Attempt to smoosh an expression that has been deeply evaluated into a BamlValue.
    /// If it encounters any non-evaluated sub-expressions, it returns None.
    pub fn as_value(&self) -> Option<BamlValueWithMeta<ExprMetadata>> {
        match self {
            Expr::Value(atom) => Some(atom.clone()),
            Expr::List(items, meta) => {
                let atom_items = items
                    .iter()
                    .map(|item| item.as_value())
                    .collect::<Option<Vec<_>>>()?;
                Some(BamlValueWithMeta::List(atom_items, meta.clone()))
            }
            Expr::Map(entries, meta) => {
                let atom_entries = entries
                    .iter()
                    .map(|(key, value)| {
                        let atom = value.as_value()?;
                        Some((key.clone(), atom))
                    })
                    .collect::<Option<BamlMap<String, BamlValueWithMeta<ExprMetadata>>>>()?;
                Some(BamlValueWithMeta::Map(atom_entries, meta.clone()))
            }
            // A class constructor may not be evaluated into an atom if it still contains a spread.
            Expr::ClassConstructor { name, fields, meta } => 'contructor: {
                let mut atom_entries = BamlMap::new();

                for field in fields {
                    match field {
                        // Short circuit on spreads.
                        ClassConstructorField::Spread { .. } => {
                            break 'contructor None;
                        }
                        ClassConstructorField::Named { name, value } => {
                            let atom = value.as_value()?;
                            atom_entries.insert(name.clone(), atom);
                        }
                    }
                }

                Some(BamlValueWithMeta::Class(
                    name.clone(),
                    atom_entries,
                    meta.clone(),
                ))
            }
            _ => None,
        }
    }

    pub fn span(&self) -> &Span {
        &self.meta().0
    }

    pub fn fresh_names(&self, arity: usize) -> Vec<Name> {
        let free_vars = self.variables();
        let mut i = 0;
        let mut names = Vec::new();
        while names.len() < arity {
            let candidate = format!("x_{i}");
            if !free_vars.contains(&candidate) {
                names.push(candidate);
            }
            i += 1;
        }
        names
    }

    pub fn fresh_name(&self) -> Name {
        self.fresh_names(1)[0].clone()
    }
}

/// An iterator over the sub-expressions of an expression.
impl<T: Clone> IntoIterator for Expr<T> {
    type Item = Expr<T>;
    type IntoIter = ExprIterator<T>;

    fn into_iter(self) -> Self::IntoIter {
        ExprIterator::new(self)
    }
}

/// An iterator over the sub-expressions of an expression.
pub struct ExprIterator<T> {
    pub stack: VecDeque<Expr<T>>,
}

impl<T> ExprIterator<T> {
    fn new(root: Expr<T>) -> Self {
        let mut stack = VecDeque::new();
        stack.push_back(root);
        Self { stack }
    }
}

impl<T: Clone> Iterator for ExprIterator<T> {
    type Item = Expr<T>;

    fn next(&mut self) -> Option<Self::Item> {
        let expr = self.stack.pop_back()?;

        // For exprs with sub-exprs, push the sub-exprs onto the stack.
        match expr.clone() {
            Expr::Value(_) => {}
            Expr::Block(block, meta) => {
                self.stack
                    .push_back(Expr::Block(Box::new(*block.clone()), meta));
            }
            Expr::List(items, _) => {
                for item in items.into_iter() {
                    self.stack.push_back(item);
                }
            }
            Expr::Map(entries, _) => {
                for (_, value) in entries.into_iter() {
                    self.stack.push_back(value);
                }
            }
            Expr::ClassConstructor { fields, .. } => {
                for field in fields {
                    match field {
                        ClassConstructorField::Named { value, .. } => {
                            self.stack.push_back(value);
                        }
                        ClassConstructorField::Spread { value } => {
                            self.stack.push_back(value);
                        }
                    }
                }
            }
            Expr::Var(_, _) => {}
            Expr::Function(_, body, meta) => {
                self.stack.push_back(Expr::Block(
                    Box::new(Arc::unwrap_or_clone(body)),
                    meta.clone(),
                ));
            }
            Expr::Call { func, args, .. } => {
                self.stack.push_back(Arc::unwrap_or_clone(func));
                args.into_iter().for_each(|arg| self.stack.push_back(arg))
            }
            Expr::MethodCall {
                receiver,
                method,
                args,
                ..
            } => {
                self.stack.push_back(Arc::unwrap_or_clone(receiver));
                self.stack.push_back(Arc::unwrap_or_clone(method));
                args.into_iter().for_each(|arg| self.stack.push_back(arg));
            }
            Expr::If(cond, then, else_, _) => {
                self.stack.push_back(Arc::unwrap_or_clone(cond));
                self.stack.push_back(Arc::unwrap_or_clone(then));
                if let Some(else_) = else_ {
                    self.stack.push_back(Arc::unwrap_or_clone(else_));
                }
            }
            Expr::ArrayAccess { base, index, .. } => {
                self.stack.push_back(Arc::unwrap_or_clone(base));
                self.stack.push_back(Arc::unwrap_or_clone(index));
            }
            Expr::FieldAccess { base, .. } => {
                self.stack.push_back(Arc::unwrap_or_clone(base));
            }
            Expr::Builtin(_, _) => {}
            Expr::BinaryOperation { left, right, .. } => {
                self.stack.push_back(Arc::unwrap_or_clone(left));
                self.stack.push_back(Arc::unwrap_or_clone(right));
            }
            Expr::UnaryOperation { expr, .. } => {
                self.stack.push_back(Arc::unwrap_or_clone(expr));
            }
            Expr::Paren(expr, _) => {
                self.stack.push_back(Arc::unwrap_or_clone(expr));
            }
        }

        Some(expr.clone())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Variable {
    Bound(VarIndex),
    Free(Name),
}

/// A single unit of execution within a block.
#[derive(Clone, Debug)]
pub enum Statement<T> {
    /// Assign an immutable variable.
    Let {
        name: String,
        value: Expr<T>,
        watch: Option<WatchSpec>,
        span: Span,
    },
    /// Declare a (mutable) reference.
    /// There is no span because it is never present in the source AST.
    /// This is a desugaring from `if` expressions.
    Declare {
        name: String,
        span: Span,
    },
    /// Assign a mutable variable.
    Assign {
        left: Expr<T>,
        value: Expr<T>,
    },
    AssignOp {
        left: Expr<T>,
        value: Expr<T>,
        assign_op: AssignOp,
        span: Span,
    },
    /// Declare and assign a mutable reference in one statement.
    DeclareAndAssign {
        name: String,
        value: Expr<T>,
        watch: Option<WatchSpec>,
        span: Span,
    },
    /// Return from a function.
    Return {
        expr: Expr<T>,
        span: Span,
    },
    /// Evaluate an expression as the final value of a block (without returning from function).
    Expression {
        expr: Expr<T>,
        span: Span,
    },
    SemicolonExpression {
        expr: Expr<T>,
        span: Span,
    },
    While {
        condition: Box<Expr<T>>,
        block: Block<T>,
        span: Span,
    },
    ForLoop {
        identifier: String,
        iterator: Box<Expr<T>>,
        block: Block<T>,
        span: Span,
    },
    /// see [`crate::hir::Statement::CForLoop`]
    CForLoop {
        condition: Option<Expr<T>>,
        after: Option<Box<Statement<T>>>,
        block: Block<T>,
    },
    Break(Span),
    Continue(Span),

    Assert {
        condition: Expr<T>,
        span: Span,
    },

    /// Configure watch options for a watched variable.
    WatchOptions {
        variable: String,
        channel: Option<String>,
        when: Option<WatchWhen>,
        span: Span,
    },

    /// Manually notify watchers of a variable.
    WatchNotify {
        variable: String,
        span: Span,
    },
    HeaderContextEnter(HeaderContext),
}

impl<T: Clone> Statement<T> {
    pub fn dump_str(&self) -> String
    where
        T: std::fmt::Debug,
    {
        match self {
            Statement::HeaderContextEnter(header) => {
                format!("//{} {}", "#".repeat(header.level as usize), header.title)
            }
            Statement::Let {
                name,
                value,
                watch,
                span: _,
            } => {
                format!(
                    "Let {} = {} {}",
                    name,
                    value.dump_str(),
                    watch.as_ref().map_or("", |_| "<emit>")
                )
            }
            Statement::Declare { name, span: _ } => format!("var {name}"),
            Statement::Assign { left, value } => {
                format!("{} <- {}", left.dump_str(), value.dump_str())
            }
            Statement::AssignOp {
                left,
                value,
                assign_op,
                span: _,
            } => format!("{} {} {}", left.dump_str(), assign_op, value.dump_str()),
            Statement::DeclareAndAssign {
                name,
                value,
                watch: emit,
                span: _,
            } => {
                format!(
                    "var {} <- {} {}",
                    name,
                    value.dump_str(),
                    emit.as_ref().map_or("", |_| "<emit>")
                )
            }
            Statement::Return { expr, span: _ } => {
                format!("return {}", expr.dump_str())
            }
            Statement::Expression { expr, span: _ } => expr.dump_str(),
            Statement::SemicolonExpression { expr, span: _ } => expr.dump_str().to_string(),
            Statement::While {
                condition,
                block,
                span: _,
            } => {
                format!("while {} {{ {} }}", condition.dump_str(), block.dump_str())
            }
            Statement::ForLoop {
                identifier,
                iterator,
                block,
                span: _,
            } => {
                format!(
                    "for {} in {} {{ {} }}",
                    identifier,
                    iterator.dump_str(),
                    block.dump_str()
                )
            }
            Statement::Break(_) => "break".to_string(),
            Statement::Continue(_) => "continue".to_string(),
            Statement::CForLoop {
                condition,
                after,
                block,
            } => {
                let condition = condition
                    .as_ref()
                    .map(Expr::dump_str)
                    .unwrap_or_else(String::new);

                let after = after
                    .as_ref()
                    .map(|s| s.dump_str())
                    .unwrap_or_else(String::new);
                let block = block.dump_str();

                format!("for (;{condition};{after}) {{ {block} }}")
            }
            Statement::Assert { condition, .. } => {
                format!("assert {cond}", cond = condition.dump_str())
            }
            Statement::WatchOptions {
                variable,
                channel,
                when,
                ..
            } => {
                let mut parts = vec![];
                if let Some(c) = channel {
                    parts.push(format!("channel: \"{c}\""));
                }
                if let Some(w) = when {
                    parts.push(format!("when: {w:?}"));
                }
                format!("{}.$watch.options({{{}}})", variable, parts.join(", "))
            }
            Statement::WatchNotify { variable, .. } => {
                format!("{variable}.$watch.notify()")
            }
        }
    }

    pub fn variables(&self) -> HashSet<Name>
    where
        T: Clone,
    {
        match self {
            Statement::HeaderContextEnter(_) => HashSet::new(),
            Statement::Declare { .. } | Statement::Break(_) | Statement::Continue(_) => {
                HashSet::new()
            }
            Statement::Let { value, .. }
            | Statement::Assign { value, .. }
            | Statement::AssignOp { value, .. }
            | Statement::DeclareAndAssign { value, .. } => value.variables(),
            Statement::Return { expr, .. }
            | Statement::Expression { expr, .. }
            | Statement::SemicolonExpression { expr, .. } => expr.variables(),
            Statement::While {
                condition: expr,
                block,
                ..
            }
            | Statement::ForLoop {
                iterator: expr,
                block,
                ..
            } => {
                let mut free_vars = expr.variables();
                free_vars.extend(block.variables());
                free_vars
            }
            Statement::CForLoop {
                condition,
                after,
                block,
            } => {
                let condition_vars = condition
                    .as_ref()
                    .map(Expr::variables)
                    .unwrap_or_else(HashSet::new);

                let after_vars = after
                    .as_ref()
                    .map(|s| s.variables())
                    .unwrap_or_else(HashSet::new);

                let mut block_vars = block.variables();

                block_vars.extend(condition_vars);
                block_vars.extend(after_vars);

                block_vars
            }
            Statement::Assert { condition, .. } => condition.variables(),
            Statement::WatchOptions { .. } | Statement::WatchNotify { .. } => {
                // These don't reference variables themselves
                HashSet::new()
            }
        }
    }
}
