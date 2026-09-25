//! User SQL → SQLite SQL.
//!
//! `sqlparser` supplies syntax; this module binds names against the catalog
//! and the query's own scopes, and rewrites BAML-value operations:
//!
//! - `c.args['customer']['age']` becomes a lazy handle with a constant path;
//! - comparisons with a handle use BAML semantics (`__btel_cmp`), never
//!   SQLite's implicit coercions;
//! - handles pass unchanged through CTE and FROM-subquery projections, so an
//!   outer query can keep navigating; they are rendered only where a plain
//!   SQL value is needed, and in the final projection (with a hidden kind
//!   column so typed output formats can restore BAML kinds).
//!
//! Only one read-only SELECT (optionally with CTEs) is accepted. Paths must
//! be constants: computed keys, slices and dot access into values are
//! rejected with an explanation.
use sqlparser::{
    ast::{
        AccessExpr, BinaryOperator, CastKind, Expr, Function, FunctionArg, FunctionArgExpr,
        FunctionArgumentList, FunctionArguments, GroupByExpr, Ident, JoinConstraint, JoinOperator,
        ObjectName, ObjectNamePart, OrderBy, OrderByKind, Query, Select, SelectItem,
        SelectItemQualifiedWildcardKind, SetExpr, Statement, Subscript, TableFactor,
        TableWithJoins, UnaryOperator, Value, WildcardAdditionalOptions, WindowType,
    },
    dialect::{GenericDialect, PostgreSqlDialect},
    parser::Parser,
};

use crate::catalog;

/// A translation problem: the user's SQL, not the system, is at fault.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SqlError(pub String);

impl std::fmt::Display for SqlError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

fn err<T>(message: impl Into<String>) -> Result<T, SqlError> {
    Err(SqlError(message.into()))
}

/// Suffix of the hidden column carrying a value column's BAML kind.
pub const KIND_SUFFIX: &str = "\u{1f}kind";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OutputColumn {
    pub name: String,
    /// A BAML value column; the next result column holds its kind.
    pub value: bool,
}

#[derive(Clone, Debug)]
pub struct Translation {
    pub sql: String,
    pub columns: Vec<OutputColumn>,
}

#[derive(Clone, Debug)]
struct Col {
    name: String,
    value: bool,
}

#[derive(Clone, Debug, Default)]
struct Shape {
    cols: Vec<Col>,
}

impl Shape {
    fn find(&self, name: &str) -> Option<&Col> {
        self.cols.iter().find(|c| c.name.eq_ignore_ascii_case(name))
    }
    fn renamed(mut self, names: &[Ident]) -> Result<Self, SqlError> {
        if names.is_empty() {
            return Ok(self);
        }
        if names.len() != self.cols.len() {
            return err(format!(
                "column list names {} columns but the query returns {}",
                names.len(),
                self.cols.len()
            ));
        }
        for (col, name) in self.cols.iter_mut().zip(names) {
            col.name.clone_from(&name.value);
        }
        Ok(self)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Mode {
    /// The statement's result: values rendered, with kind columns.
    Final,
    /// A CTE or FROM subquery: values stay handles.
    Keep,
    /// An expression subquery (IN, EXISTS, scalar): values rendered.
    Scalar,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    Sql,
    Value,
}

#[derive(Default)]
struct Frame {
    ctes: Vec<(String, Shape)>,
    relations: Vec<(String, Shape)>,
}

struct Translator {
    frames: Vec<Frame>,
    output: Vec<OutputColumn>,
}

/// Parse, check the statement policy, bind and rewrite.
pub fn translate(sql: &str) -> Result<Translation, SqlError> {
    let mut statements = parse(sql)?;
    if statements.len() != 1 {
        return err("exactly one SELECT statement is allowed");
    }
    let Statement::Query(mut query) = statements.remove(0) else {
        return err("only read-only SELECT queries are allowed");
    };
    let mut translator = Translator {
        frames: Vec::new(),
        output: Vec::new(),
    };
    let shape = translator.query(&mut query, Mode::Final)?;
    let sql = if matches!(query.body.as_ref(), SetExpr::Select(_)) {
        query.to_string()
    } else {
        let (sql, output) = render_set_operation(&query, &shape)?;
        translator.output = output;
        sql
    };
    Ok(Translation {
        sql,
        columns: translator.output,
    })
}

fn parse(sql: &str) -> Result<Vec<Statement>, SqlError> {
    match Parser::parse_sql(&GenericDialect {}, sql) {
        Ok(statements) => Ok(statements),
        Err(error) => {
            // sqlparser 0.62 gates AS MATERIALIZED on its PostgreSQL parser,
            // although SQLite supports it too. Keep the existing parser for
            // ordinary SQL; use the alternate only for explicit CTE hints.
            if let Ok(statements) = Parser::parse_sql(&PostgreSqlDialect {}, sql)
                && statements.iter().any(|statement| {
                    matches!(statement, Statement::Query(query) if query.with.as_ref().is_some_and(|with| {
                        with.cte_tables.iter().any(|cte| cte.materialized.is_some())
                    }))
                })
            {
                return Ok(statements);
            }
            Err(SqlError(format!("SQL syntax: {error}")))
        }
    }
}

/// Wrap a translated set operation, whose branches kept value handles, so
/// the final projection renders values with their kind columns:
/// `WITH <ctes>, __set(__c0, ..) AS (<set>) SELECT <rendered> FROM __set
/// <order by> <limit>`. ORDER BY names output columns (checked earlier);
/// positions are remapped past the kind columns.
fn render_set_operation(
    query: &Query,
    shape: &Shape,
) -> Result<(String, Vec<OutputColumn>), SqlError> {
    let mut inner = query.clone();
    inner.with = None;
    inner.order_by = None;
    inner.limit_clause = None;
    let output: Vec<OutputColumn> = shape
        .cols
        .iter()
        .map(|c| OutputColumn {
            name: c.name.clone(),
            value: c.value,
        })
        .collect();
    let slots: Vec<String> = (0..shape.cols.len())
        .map(|i| quoted(&format!("__c{i}")).to_string())
        .collect();
    let mut items = Vec::new();
    for (column, slot) in output.iter().zip(&slots) {
        let name = quoted(&column.name);
        if column.value {
            let kind = quoted(&format!("{}{KIND_SUFFIX}", column.name));
            items.push(format!(
                "__btel_render({slot}) AS {name}, __btel_kind({slot}) AS {kind}"
            ));
        } else {
            items.push(format!("{slot} AS {name}"));
        }
    }
    let ctes = match &query.with {
        Some(with) => format!("{with}, "),
        None => "WITH ".to_owned(),
    };
    let mut sql = format!(
        "{ctes}\"__set\"({}) AS ({inner}) SELECT {} FROM \"__set\"",
        slots.join(", "),
        items.join(", ")
    );
    if let Some(order_by) = &query.order_by {
        let mut order_by = order_by.clone();
        if let OrderByKind::Expressions(exprs) = &mut order_by.kind {
            for item in exprs {
                remap_position(&mut item.expr, &output)?;
            }
        }
        sql.push(' ');
        sql.push_str(&order_by.to_string());
    }
    if let Some(limit) = &query.limit_clause {
        sql.push_str(&limit.to_string());
    }
    Ok((sql, output))
}

/// A positional `ORDER BY`/`GROUP BY` term names a logical output column;
/// each value column occupies two physical columns (value, kind).
fn remap_position(expr: &mut Expr, output: &[OutputColumn]) -> Result<(), SqlError> {
    let Expr::Value(value) = expr else {
        return Ok(());
    };
    let Value::Number(number, _) = &value.value else {
        return Ok(());
    };
    let Ok(position) = number.parse::<usize>() else {
        return Ok(());
    };
    if position == 0 || position > output.len() {
        return err(format!(
            "position {position} is outside the {} result columns",
            output.len()
        ));
    }
    let physical = output[..position - 1]
        .iter()
        .map(|c| if c.value { 2 } else { 1 })
        .sum::<usize>()
        + 1;
    *expr = integer(i64::try_from(physical).expect("column count fits i64"));
    Ok(())
}

fn ident_name(ident: &Ident) -> Result<String, SqlError> {
    reserved(&ident.value)?;
    Ok(ident.value.clone())
}

fn reserved(name: &str) -> Result<(), SqlError> {
    let lower = name.to_ascii_lowercase();
    if lower.starts_with("__") || lower.starts_with("sqlite_") {
        return err(format!("`{name}` is a reserved name"));
    }
    Ok(())
}

/// JSON is a plain JSON value (an object is a map, not a class envelope).
/// Validate even on an empty recording; never defer malformed SQL to rows.
fn json_literal(expr: &Expr) -> Result<Option<String>, SqlError> {
    if let Expr::Nested(inner) = expr {
        return json_literal(inner);
    }
    let Expr::Function(function) = expr else {
        return Ok(None);
    };
    let parts = object_name(&function.name)?;
    if !matches!(parts.as_slice(), [name] if name.eq_ignore_ascii_case("baml_value_json")) {
        return Ok(None);
    }
    let FunctionArguments::List(list) = &function.args else {
        return err("baml_value_json takes one constant JSON string");
    };
    if function.filter.is_some()
        || function.over.is_some()
        || function.null_treatment.is_some()
        || !function.within_group.is_empty()
        || !matches!(function.parameters, FunctionArguments::None)
        || list.duplicate_treatment.is_some()
        || !list.clauses.is_empty()
    {
        return err("baml_value_json is a plain comparison operand");
    }
    let [FunctionArg::Unnamed(FunctionArgExpr::Expr(Expr::Value(value)))] = list.args.as_slice()
    else {
        return err("baml_value_json takes one constant JSON string");
    };
    let Value::SingleQuotedString(text) = &value.value else {
        return err("baml_value_json takes one constant JSON string");
    };
    if text.len() > 1 << 20 {
        return err("baml_value_json exceeds the 1 MiB literal limit");
    }
    serde_json::from_str::<serde_json::Value>(text)
        .map_err(|error| SqlError(format!("baml_value_json expects valid JSON: {error}")))?;
    Ok(Some(text.clone()))
}

fn object_name(name: &ObjectName) -> Result<Vec<String>, SqlError> {
    name.0
        .iter()
        .map(|part| match part {
            ObjectNamePart::Identifier(ident) => ident_name(ident),
            ObjectNamePart::Function(_) => err("dynamic object names are not supported"),
        })
        .collect()
}

pub(crate) fn call(name: &str, args: Vec<Expr>) -> Expr {
    Expr::Function(Function {
        name: ObjectName::from(vec![Ident::new(name)]),
        uses_odbc_syntax: false,
        parameters: FunctionArguments::None,
        args: FunctionArguments::List(FunctionArgumentList {
            duplicate_treatment: None,
            args: args
                .into_iter()
                .map(|e| FunctionArg::Unnamed(FunctionArgExpr::Expr(e)))
                .collect(),
            clauses: Vec::new(),
        }),
        filter: None,
        null_treatment: None,
        over: None,
        within_group: Vec::new(),
    })
}

fn string(value: &str) -> Expr {
    Expr::Value(Value::SingleQuotedString(value.to_owned()).into())
}

fn integer(value: i64) -> Expr {
    Expr::Value(Value::Number(value.to_string(), false).into())
}

fn quoted(name: &str) -> Ident {
    Ident::with_quote('"', name)
}

/// Default output name for an unaliased projection: SQLite's rule for
/// columns, otherwise the user's own expression text.
fn projection_name(expr: &Expr) -> String {
    match expr {
        Expr::Identifier(ident) => ident.value.clone(),
        Expr::CompoundIdentifier(parts) => {
            parts.last().map(|i| i.value.clone()).unwrap_or_default()
        }
        other => other.to_string(),
    }
}

impl Translator {
    fn lookup_relation(&self, name: &str) -> Option<Shape> {
        for frame in self.frames.iter().rev() {
            if let Some((_, shape)) = frame
                .ctes
                .iter()
                .rev()
                .find(|(cte, _)| cte.eq_ignore_ascii_case(name))
            {
                return Some(shape.clone());
            }
        }
        catalog::relation(name).map(|relation| Shape {
            cols: relation
                .columns
                .iter()
                .map(|c| Col {
                    name: c.name.to_owned(),
                    value: c.value,
                })
                .collect(),
        })
    }

    /// Resolve a column reference, innermost scope first. Unknown names
    /// (output aliases, correlated names SQLite will reject) are plain SQL.
    fn column(&self, qualifier: Option<&str>, name: &str) -> Result<Kind, SqlError> {
        for frame in self.frames.iter().rev() {
            let mut found = None;
            for (alias, shape) in &frame.relations {
                if qualifier.is_some_and(|q| !alias.eq_ignore_ascii_case(q)) {
                    continue;
                }
                if let Some(col) = shape.find(name) {
                    if found.is_some() && qualifier.is_none() {
                        return err(format!("column `{name}` is ambiguous; qualify it"));
                    }
                    found = Some(if col.value { Kind::Value } else { Kind::Sql });
                }
            }
            if let Some(kind) = found {
                return Ok(kind);
            }
            if let Some(q) = qualifier
                && frame
                    .relations
                    .iter()
                    .any(|(alias, _)| alias.eq_ignore_ascii_case(q))
            {
                return err(format!("relation `{q}` has no column `{name}`"));
            }
        }
        Ok(Kind::Sql)
    }

    fn query(&mut self, query: &mut Query, mode: Mode) -> Result<Shape, SqlError> {
        if !query.locks.is_empty()
            || query.for_clause.is_some()
            || query.settings.is_some()
            || query.format_clause.is_some()
            || query.fetch.is_some()
            || !query.pipe_operators.is_empty()
        {
            return err("unsupported query clause");
        }
        self.frames.push(Frame::default());
        let result = (|| {
            if let Some(with) = &mut query.with {
                if with.recursive {
                    return err("recursive CTEs are not supported");
                }
                for cte in &mut with.cte_tables {
                    let name = ident_name(&cte.alias.name)?;
                    let columns: Vec<Ident> =
                        cte.alias.columns.iter().map(|c| c.name.clone()).collect();
                    let shape = self.query(&mut cte.query, Mode::Keep)?.renamed(&columns)?;
                    self.frames
                        .last_mut()
                        .expect("query frame")
                        .ctes
                        .push((name, shape));
                }
            }
            let shape = match query.body.as_mut() {
                SetExpr::Select(select) => self.select(select, mode, query.order_by.as_mut())?,
                other if mode == Mode::Final => {
                    // Branches keep handles; values render once, at the
                    // output boundary, with their kind columns.
                    let shape = self.set_expr(other, Mode::Keep)?;
                    if let Some(order_by) = &mut query.order_by {
                        Self::order_by_outputs(order_by, &shape)?;
                    }
                    shape
                }
                other => {
                    let shape = self.set_expr(other, mode)?;
                    if let Some(order_by) = &mut query.order_by {
                        Self::order_by_outputs(order_by, &shape)?;
                    }
                    shape
                }
            };
            if let Some(limit) = &mut query.limit_clause {
                use sqlparser::ast::LimitClause;
                match limit {
                    LimitClause::LimitOffset {
                        limit,
                        offset,
                        limit_by,
                    } => {
                        if let Some(limit) = limit {
                            self.scalar(limit)?;
                        }
                        if let Some(offset) = offset {
                            self.scalar(&mut offset.value)?;
                        }
                        if !limit_by.is_empty() {
                            return err("LIMIT BY is not supported");
                        }
                    }
                    LimitClause::OffsetCommaLimit { offset, limit } => {
                        self.scalar(offset)?;
                        self.scalar(limit)?;
                    }
                }
            }
            Ok(shape)
        })();
        self.frames.pop();
        result
    }

    fn set_expr(&mut self, body: &mut SetExpr, mode: Mode) -> Result<Shape, SqlError> {
        match body {
            SetExpr::Select(select) => self.select(select, mode, None),
            SetExpr::Query(query) => self.query(query, mode),
            SetExpr::SetOperation { left, right, .. } => {
                // Final set operations render values in each branch; no
                // kind columns, so typed formats show them as SQL values.
                let branch = if mode == Mode::Keep {
                    Mode::Keep
                } else {
                    Mode::Scalar
                };
                let shape = self.set_expr(left, branch)?;
                let right = self.set_expr(right, branch)?;
                if right.cols.len() != shape.cols.len() {
                    return err("set operation branches return different column counts");
                }
                if mode == Mode::Final {
                    self.output = shape
                        .cols
                        .iter()
                        .map(|c| OutputColumn {
                            name: c.name.clone(),
                            value: false,
                        })
                        .collect();
                }
                Ok(shape)
            }
            SetExpr::Values(_) => err("VALUES lists are not supported"),
            _ => err("only SELECT queries are allowed"),
        }
    }

    fn order_by_outputs(order_by: &mut OrderBy, shape: &Shape) -> Result<(), SqlError> {
        let OrderByKind::Expressions(exprs) = &mut order_by.kind else {
            return err("ORDER BY ALL is not supported");
        };
        for item in exprs {
            if !matches!(&item.expr, Expr::Identifier(i) if shape.find(&i.value).is_some())
                && !matches!(&item.expr, Expr::Value(_))
            {
                return err("ORDER BY after a set operation may only name output columns");
            }
        }
        Ok(())
    }

    fn select(
        &mut self,
        select: &mut Select,
        mode: Mode,
        order_by: Option<&mut OrderBy>,
    ) -> Result<Shape, SqlError> {
        if select.into.is_some()
            || select.top.is_some()
            || !select.lateral_views.is_empty()
            || select.prewhere.is_some()
            || !select.connect_by.is_empty()
            || !select.cluster_by.is_empty()
            || !select.distribute_by.is_empty()
            || !select.sort_by.is_empty()
            || !select.named_window.is_empty()
            || select.qualify.is_some()
            || select.value_table_mode.is_some()
            || select.exclude.is_some()
        {
            return err("unsupported SELECT clause");
        }
        self.frames.push(Frame::default());
        let result = (|| {
            for table in &mut select.from {
                self.table_with_joins(table)?;
            }
            if let Some(selection) = &mut select.selection {
                self.boolean(selection)?;
            }
            match &mut select.group_by {
                GroupByExpr::All(_) => return err("GROUP BY ALL is not supported"),
                GroupByExpr::Expressions(exprs, modifiers) => {
                    if !modifiers.is_empty() {
                        return err("GROUP BY modifiers are not supported");
                    }
                    for expr in exprs {
                        self.scalar(expr)?;
                    }
                }
            }
            if let Some(having) = &mut select.having {
                self.boolean(having)?;
            }
            let shape = self.projection(&mut select.projection, mode)?;
            if mode == Mode::Final
                && let GroupByExpr::Expressions(exprs, _) = &mut select.group_by
            {
                for expr in exprs {
                    remap_position(expr, &self.output)?;
                }
            }
            if let Some(order_by) = order_by {
                let OrderByKind::Expressions(exprs) = &mut order_by.kind else {
                    return err("ORDER BY ALL is not supported");
                };
                for item in exprs {
                    if mode == Mode::Final {
                        remap_position(&mut item.expr, &self.output)?;
                    }
                    // An output alias sorts by the (rendered) output value.
                    if let Expr::Identifier(ident) = &item.expr
                        && shape.find(&ident.value).is_some()
                        && self.column(None, &ident.value)? == Kind::Sql
                    {
                        continue;
                    }
                    self.scalar(&mut item.expr)?;
                }
            }
            Ok(shape)
        })();
        self.frames.pop();
        result
    }

    fn add_relation(&mut self, alias: String, shape: Shape) {
        self.frames
            .last_mut()
            .expect("select frame")
            .relations
            .push((alias, shape));
    }

    fn table_with_joins(&mut self, table: &mut TableWithJoins) -> Result<(), SqlError> {
        self.table_factor(&mut table.relation)?;
        for join in &mut table.joins {
            self.table_factor(&mut join.relation)?;
            let (JoinOperator::Join(constraint)
            | JoinOperator::Inner(constraint)
            | JoinOperator::Left(constraint)
            | JoinOperator::LeftOuter(constraint)
            | JoinOperator::Right(constraint)
            | JoinOperator::RightOuter(constraint)
            | JoinOperator::FullOuter(constraint)
            | JoinOperator::CrossJoin(constraint)) = &mut join.join_operator
            else {
                return err("unsupported join");
            };
            match constraint {
                JoinConstraint::On(expr) => self.boolean(expr)?,
                JoinConstraint::Using(_) | JoinConstraint::Natural | JoinConstraint::None => {}
            }
        }
        Ok(())
    }

    fn table_factor(&mut self, factor: &mut TableFactor) -> Result<(), SqlError> {
        match factor {
            TableFactor::Table {
                name,
                alias,
                args,
                with_hints,
                version,
                with_ordinality,
                partitions,
                json_path,
                sample,
                index_hints,
            } => {
                if args.is_some()
                    || !with_hints.is_empty()
                    || version.is_some()
                    || *with_ordinality
                    || !partitions.is_empty()
                    || json_path.is_some()
                    || sample.is_some()
                    || !index_hints.is_empty()
                {
                    return err("unsupported table clause");
                }
                let parts = object_name(name)?;
                let [table] = parts.as_slice() else {
                    return err("qualified table names are not supported; use the relation name");
                };
                let Some(shape) = self.lookup_relation(table) else {
                    if let Some(advice) = catalog::retired(table) {
                        return err(format!(
                            "`{table}` from the old tracer is not available: {advice}"
                        ));
                    }
                    let names: Vec<_> = catalog::RELATIONS.iter().map(|r| r.name).collect();
                    return err(format!(
                        "unknown relation `{table}`; available: {}",
                        names.join(", ")
                    ));
                };
                let (alias_name, shape) = match alias {
                    Some(alias) => {
                        let columns: Vec<Ident> =
                            alias.columns.iter().map(|c| c.name.clone()).collect();
                        (ident_name(&alias.name)?, shape.renamed(&columns)?)
                    }
                    None => (table.clone(), shape),
                };
                self.add_relation(alias_name, shape);
                Ok(())
            }
            TableFactor::Derived {
                lateral,
                subquery,
                alias,
                sample,
            } => {
                if *lateral || sample.is_some() {
                    return err("LATERAL subqueries are not supported");
                }
                let shape = self.query(subquery, Mode::Keep)?;
                let (alias_name, shape) = match alias {
                    Some(alias) => {
                        let columns: Vec<Ident> =
                            alias.columns.iter().map(|c| c.name.clone()).collect();
                        (ident_name(&alias.name)?, shape.renamed(&columns)?)
                    }
                    None => (String::new(), shape),
                };
                self.add_relation(alias_name, shape);
                Ok(())
            }
            TableFactor::NestedJoin {
                table_with_joins,
                alias,
            } => {
                if alias.is_some() {
                    return err("aliased parenthesized joins are not supported");
                }
                self.table_with_joins(table_with_joins)
            }
            _ => err("unsupported FROM item; use catalog relations, CTEs or subqueries"),
        }
    }

    fn projection(&mut self, items: &mut Vec<SelectItem>, mode: Mode) -> Result<Shape, SqlError> {
        let mut shape = Shape::default();
        let mut out = Vec::with_capacity(items.len());
        let mut output = Vec::new();
        for item in std::mem::take(items) {
            let expanded = match item {
                SelectItem::Wildcard(options) => {
                    if options != WildcardAdditionalOptions::default() {
                        return err("wildcard options are not supported");
                    }
                    self.expand_wildcard(None)?
                }
                SelectItem::QualifiedWildcard(kind, options) => {
                    if options != WildcardAdditionalOptions::default() {
                        return err("wildcard options are not supported");
                    }
                    let SelectItemQualifiedWildcardKind::ObjectName(name) = kind else {
                        return err("unsupported wildcard");
                    };
                    let parts = object_name(&name)?;
                    let [relation] = parts.as_slice() else {
                        return err("unsupported wildcard");
                    };
                    self.expand_wildcard(Some(relation))?
                }
                SelectItem::UnnamedExpr(expr) => vec![(expr, None)],
                SelectItem::ExprWithAlias { expr, alias } => {
                    vec![(expr, Some(ident_name(&alias)?))]
                }
                SelectItem::ExprWithAliases { .. } => {
                    return err("multiple aliases are not supported");
                }
            };
            for (mut expr, alias) in expanded {
                let original = projection_name(&expr);
                let kind = self.raw(&mut expr)?;
                let name = alias.clone().unwrap_or(original.clone());
                match (kind, mode) {
                    (Kind::Value, Mode::Final) => {
                        out.push(SelectItem::ExprWithAlias {
                            expr: call("__btel_render", vec![expr.clone()]),
                            alias: quoted(&name),
                        });
                        out.push(SelectItem::ExprWithAlias {
                            expr: call("__btel_kind", vec![expr]),
                            alias: quoted(&format!("{name}{KIND_SUFFIX}")),
                        });
                        output.push(OutputColumn {
                            name: name.clone(),
                            value: true,
                        });
                    }
                    (Kind::Value, Mode::Keep) => out.push(SelectItem::ExprWithAlias {
                        expr,
                        alias: quoted(&name),
                    }),
                    (Kind::Value, Mode::Scalar) => out.push(SelectItem::ExprWithAlias {
                        expr: call("__btel_render", vec![expr]),
                        alias: quoted(&name),
                    }),
                    (Kind::Sql, _) => {
                        // Keep the user's spelling as the column name when
                        // translation changed an unaliased expression.
                        let renamed =
                            !matches!(expr, Expr::Identifier(_) | Expr::CompoundIdentifier(_))
                                && expr.to_string() != original;
                        let item = if alias.is_some() || renamed {
                            SelectItem::ExprWithAlias {
                                expr,
                                alias: quoted(&name),
                            }
                        } else {
                            SelectItem::UnnamedExpr(expr)
                        };
                        out.push(item);
                        if mode == Mode::Final {
                            output.push(OutputColumn {
                                name: name.clone(),
                                value: false,
                            });
                        }
                    }
                }
                shape.cols.push(Col {
                    name,
                    value: kind == Kind::Value && mode == Mode::Keep,
                });
            }
        }
        *items = out;
        if mode == Mode::Final {
            self.output = output;
        }
        Ok(shape)
    }

    /// Explicit columns for `*` or `t.*`, so value columns can be rendered.
    fn expand_wildcard(
        &self,
        relation: Option<&str>,
    ) -> Result<Vec<(Expr, Option<String>)>, SqlError> {
        let frame = self.frames.last().expect("select frame");
        let mut items = Vec::new();
        let mut matched = false;
        for (alias, shape) in &frame.relations {
            if relation.is_some_and(|r| !alias.eq_ignore_ascii_case(r)) {
                continue;
            }
            matched = true;
            for col in &shape.cols {
                let expr = if alias.is_empty() {
                    Expr::Identifier(quoted(&col.name))
                } else {
                    Expr::CompoundIdentifier(vec![quoted(alias), quoted(&col.name)])
                };
                items.push((expr, Some(col.name.clone())));
            }
        }
        if !matched {
            return match relation {
                Some(r) => err(format!("unknown relation `{r}` in `{r}.*`")),
                None => err("`*` needs a FROM clause"),
            };
        }
        Ok(items)
    }

    /// Translate an expression, leaving BAML values as handles.
    fn raw(&mut self, expr: &mut Expr) -> Result<Kind, SqlError> {
        match expr {
            Expr::Identifier(ident) => {
                reserved(&ident.value)?;
                self.column(None, &ident.value)
            }
            Expr::CompoundIdentifier(parts) => {
                for part in parts.iter() {
                    reserved(&part.value)?;
                }
                match parts.as_slice() {
                    [relation, column] => self.column(Some(&relation.value), &column.value),
                    _ => err("only `relation.column` references are supported"),
                }
            }
            Expr::CompoundFieldAccess { root, access_chain } => {
                self.navigation(root, access_chain)?;
                let (root, chain) = (take(root), std::mem::take(access_chain));
                let mut args = vec![root];
                for access in chain {
                    let AccessExpr::Subscript(Subscript::Index { index }) = access else {
                        unreachable!("validated path")
                    };
                    args.push(index);
                }
                *expr = call("__btel_nav", args);
                Ok(Kind::Value)
            }
            Expr::Nested(inner) => self.raw(inner),
            Expr::BinaryOp { left, op, right } => {
                let op = op.clone();
                if let Some(replacement) = self.binary(left, &op, right)? {
                    *expr = replacement;
                }
                Ok(Kind::Sql)
            }
            Expr::UnaryOp { op, expr: inner } => {
                match op {
                    UnaryOperator::Not => self.boolean(inner)?,
                    _ => self.scalar(inner)?,
                }
                Ok(Kind::Sql)
            }
            Expr::IsNull(inner) => {
                if self.raw(inner)? == Kind::Value {
                    let handle = take(inner);
                    *expr = call("__btel_is_null", vec![handle]);
                }
                Ok(Kind::Sql)
            }
            Expr::IsNotNull(inner) => {
                if self.raw(inner)? == Kind::Value {
                    let handle = take(inner);
                    *expr = Expr::UnaryOp {
                        op: UnaryOperator::Not,
                        expr: Box::new(call("__btel_is_null", vec![handle])),
                    };
                }
                Ok(Kind::Sql)
            }
            Expr::IsTrue(inner)
            | Expr::IsNotTrue(inner)
            | Expr::IsFalse(inner)
            | Expr::IsNotFalse(inner)
            | Expr::IsUnknown(inner)
            | Expr::IsNotUnknown(inner) => {
                self.boolean(inner)?;
                Ok(Kind::Sql)
            }
            Expr::IsDistinctFrom(a, b) | Expr::IsNotDistinctFrom(a, b) => {
                self.scalar(a)?;
                self.scalar(b)?;
                Ok(Kind::Sql)
            }
            Expr::InList {
                expr: inner,
                list,
                negated,
            } => {
                if self.raw(inner)? == Kind::Value {
                    let mut tests = Vec::new();
                    for item in list.iter_mut() {
                        tests.push(self.compare_value(inner.as_ref().clone(), "=", item)?);
                    }
                    let any = tests
                        .into_iter()
                        .reduce(|a, b| Expr::BinaryOp {
                            left: Box::new(a),
                            op: BinaryOperator::Or,
                            right: Box::new(b),
                        })
                        .unwrap_or_else(|| integer(0));
                    let any = Expr::Nested(Box::new(any));
                    *expr = if *negated {
                        Expr::UnaryOp {
                            op: UnaryOperator::Not,
                            expr: Box::new(any),
                        }
                    } else {
                        any
                    };
                } else {
                    for item in list {
                        self.scalar(item)?;
                    }
                }
                Ok(Kind::Sql)
            }
            Expr::InSubquery {
                expr: inner,
                subquery,
                ..
            } => {
                self.scalar(inner)?;
                self.query(subquery, Mode::Scalar)?;
                Ok(Kind::Sql)
            }
            Expr::Between {
                expr: inner,
                negated,
                low,
                high,
            } => {
                if self.raw(inner)? == Kind::Value {
                    let lower = self.compare_value(inner.as_ref().clone(), ">=", low)?;
                    let upper = self.compare_value(inner.as_ref().clone(), "<=", high)?;
                    let both = Expr::Nested(Box::new(Expr::BinaryOp {
                        left: Box::new(lower),
                        op: BinaryOperator::And,
                        right: Box::new(upper),
                    }));
                    *expr = if *negated {
                        Expr::UnaryOp {
                            op: UnaryOperator::Not,
                            expr: Box::new(both),
                        }
                    } else {
                        both
                    };
                } else {
                    self.scalar(low)?;
                    self.scalar(high)?;
                }
                Ok(Kind::Sql)
            }
            Expr::Like {
                expr: inner,
                pattern,
                ..
            }
            | Expr::ILike {
                expr: inner,
                pattern,
                ..
            }
            | Expr::SimilarTo {
                expr: inner,
                pattern,
                ..
            }
            | Expr::RLike {
                expr: inner,
                pattern,
                ..
            } => {
                self.scalar(inner)?;
                self.scalar(pattern)?;
                Ok(Kind::Sql)
            }
            Expr::Function(function) => {
                if let Some(replacement) = self.value_function(function)? {
                    *expr = replacement;
                } else {
                    self.function(function)?;
                }
                Ok(Kind::Sql)
            }
            Expr::Case {
                operand,
                conditions,
                else_result,
                ..
            } => {
                let simple = operand.is_some();
                if let Some(operand) = operand {
                    self.scalar(operand)?;
                }
                for when in conditions {
                    if simple {
                        self.scalar(&mut when.condition)?;
                    } else {
                        self.boolean(&mut when.condition)?;
                    }
                    self.scalar(&mut when.result)?;
                }
                if let Some(result) = else_result {
                    self.scalar(result)?;
                }
                Ok(Kind::Sql)
            }
            Expr::Exists { subquery, .. } | Expr::Subquery(subquery) => {
                self.query(subquery, Mode::Scalar)?;
                Ok(Kind::Sql)
            }
            Expr::Value(_) => Ok(Kind::Sql),
            Expr::Cast {
                kind,
                expr: inner,
                format,
                ..
            } => {
                if format.is_some() {
                    return err("CAST ... FORMAT is not supported");
                }
                // `x::T` and TRY_CAST are not SQLite syntax.
                *kind = CastKind::Cast;
                self.scalar(inner)?;
                Ok(Kind::Sql)
            }
            Expr::Collate { expr: inner, .. } => {
                self.scalar(inner)?;
                Ok(Kind::Sql)
            }
            Expr::Tuple(items) => {
                for item in items {
                    self.scalar(item)?;
                }
                Ok(Kind::Sql)
            }
            other => err(format!("unsupported SQL expression: {other}")),
        }
    }

    /// Validate a value path: `[alias.]column` then constant subscripts.
    fn navigation(
        &mut self,
        root: &mut Box<Expr>,
        chain: &mut Vec<AccessExpr>,
    ) -> Result<(), SqlError> {
        // `c.args[...]` parses as `c` followed by `.args`: fold leading dot
        // steps into the column reference.
        let dots = chain
            .iter()
            .take_while(|access| matches!(access, AccessExpr::Dot(Expr::Identifier(_))))
            .count();
        if dots > 0 {
            let mut parts = match root.as_ref() {
                Expr::Identifier(ident) => vec![ident.clone()],
                Expr::CompoundIdentifier(parts) => parts.clone(),
                _ => return err("unsupported value reference"),
            };
            for access in chain.drain(..dots) {
                let AccessExpr::Dot(Expr::Identifier(ident)) = access else {
                    unreachable!("counted dot identifiers")
                };
                parts.push(ident);
            }
            **root = Expr::CompoundIdentifier(parts);
        }
        let described = root.to_string();
        if self.raw(root)? != Kind::Value {
            return err(format!(
                "`{described}` is not a BAML value; subscripts apply to args, output and error"
            ));
        }
        for access in chain.iter_mut() {
            match access {
                AccessExpr::Subscript(Subscript::Index { index }) => {
                    let segment = match &*index {
                        Expr::Value(v) => match &v.value {
                            Value::SingleQuotedString(key) => string(key),
                            Value::Number(n, _) => match n.parse::<i64>() {
                                Ok(i) => integer(i),
                                Err(_) => {
                                    return err(format!("list index `{n}` is not an integer"));
                                }
                            },
                            _ => {
                                return err(
                                    "path segments must be 'string' keys or integer indices",
                                );
                            }
                        },
                        Expr::UnaryOp {
                            op: UnaryOperator::Minus,
                            expr: inner,
                        } => match inner.as_ref() {
                            Expr::Value(v) => match &v.value {
                                Value::Number(n, _) => match n.parse::<i64>() {
                                    Ok(i) => integer(-i),
                                    Err(_) => {
                                        return err(format!("list index `-{n}` is not an integer"));
                                    }
                                },
                                _ => return err("path segments must be constants"),
                            },
                            _ => return err("path segments must be constants"),
                        },
                        _ => {
                            return err(
                                "computed paths are not supported; use a constant 'key' or integer index",
                            );
                        }
                    };
                    *index = segment;
                }
                AccessExpr::Subscript(Subscript::Slice { .. }) => {
                    return err("list slices are not supported");
                }
                AccessExpr::Dot(_) => {
                    return err("use ['field'] rather than .field inside a BAML value");
                }
            }
        }
        Ok(())
    }

    /// `left op right` where either side may be a value. Returns a
    /// replacement expression when the comparison must use BAML semantics.
    fn binary(
        &mut self,
        left: &mut Box<Expr>,
        op: &BinaryOperator,
        right: &mut Box<Expr>,
    ) -> Result<Option<Expr>, SqlError> {
        let left_json = json_literal(left)?;
        let right_json = json_literal(right)?;
        if left_json.is_some() || right_json.is_some() {
            if left_json.is_some() && right_json.is_some() {
                return err("baml_value_json must be compared with a captured BAML value");
            }
            let op = match op {
                BinaryOperator::Eq => "=",
                BinaryOperator::NotEq => "!=",
                _ => return err("baml_value_json supports only = and != comparisons"),
            };
            let (value, json) = if let Some(json) = left_json {
                (right, json)
            } else {
                (left, right_json.expect("one literal"))
            };
            if self.raw(value)? != Kind::Value {
                return err("baml_value_json must be compared with a captured BAML value");
            }
            return Ok(Some(call(
                "__btel_cmp_json",
                vec![take(value), string(op), string(&json)],
            )));
        }
        let comparison = match op {
            BinaryOperator::Eq => Some("="),
            BinaryOperator::NotEq => Some("!="),
            BinaryOperator::Lt => Some("<"),
            BinaryOperator::LtEq => Some("<="),
            BinaryOperator::Gt => Some(">"),
            BinaryOperator::GtEq => Some(">="),
            _ => None,
        };
        if matches!(
            op,
            BinaryOperator::And | BinaryOperator::Or | BinaryOperator::Xor
        ) {
            self.boolean(left)?;
            self.boolean(right)?;
            return Ok(None);
        }
        let Some(comparison) = comparison else {
            self.scalar(left)?;
            self.scalar(right)?;
            return Ok(None);
        };
        let left_kind = self.raw(left)?;
        let right_kind = self.raw(right)?;
        Ok(match (left_kind, right_kind) {
            (Kind::Sql, Kind::Sql) => None,
            (Kind::Value, Kind::Value) => Some(call(
                "__btel_cmp_values",
                vec![take(left), string(comparison), take(right)],
            )),
            (Kind::Value, Kind::Sql) => {
                let rhs = take(right);
                Some(compare_call(take(left), comparison, rhs))
            }
            (Kind::Sql, Kind::Value) => {
                let lhs = take(left);
                Some(compare_call(take(right), flip(comparison), lhs))
            }
        })
    }

    /// `handle op item` for an already-translated handle.
    fn compare_value(&mut self, handle: Expr, op: &str, item: &mut Expr) -> Result<Expr, SqlError> {
        if self.raw(item)? == Kind::Value {
            let other = item.clone();
            return Ok(call("__btel_cmp_values", vec![handle, string(op), other]));
        }
        Ok(compare_call(handle, op, item.clone()))
    }

    /// `baml_value_state(v)` and `baml_kind(v)`: inspection functions over a
    /// BAML value. Returns their translation, or `None` for other functions.
    fn value_function(&mut self, function: &mut Function) -> Result<Option<Expr>, SqlError> {
        let parts = object_name(&function.name)?;
        let [name] = parts.as_slice() else {
            return Ok(None);
        };
        let internal = if name.eq_ignore_ascii_case("baml_value_state") {
            "__btel_value_state"
        } else if name.eq_ignore_ascii_case("baml_kind") {
            "__btel_kind"
        } else {
            return Ok(None);
        };
        let FunctionArguments::List(list) = &mut function.args else {
            return err(format!("{name} takes one BAML value"));
        };
        let [FunctionArg::Unnamed(FunctionArgExpr::Expr(arg))] = list.args.as_mut_slice() else {
            return err(format!("{name} takes one BAML value"));
        };
        if function.filter.is_some() || function.over.is_some() || !list.clauses.is_empty() {
            return err(format!("{name} is a plain function"));
        }
        if self.raw(arg)? != Kind::Value {
            return err(format!(
                "{name} takes a BAML value: args, output, error or a path into one"
            ));
        }
        Ok(Some(call(internal, vec![take(arg)])))
    }

    fn function(&mut self, function: &mut Function) -> Result<(), SqlError> {
        let parts = object_name(&function.name)?;
        let [name] = parts.as_slice() else {
            return err("qualified function names are not supported");
        };
        if name.eq_ignore_ascii_case("baml_value_json") {
            return err(
                "baml_value_json is a comparison operand: use value = baml_value_json('...')",
            );
        }
        let counts_handles = name.eq_ignore_ascii_case("count");
        match &mut function.args {
            FunctionArguments::None => {}
            FunctionArguments::Subquery(_) => {
                return err("subquery function arguments are not supported");
            }
            FunctionArguments::List(list) => {
                if !list.clauses.is_empty() {
                    return err("function argument clauses are not supported");
                }
                for arg in &mut list.args {
                    match arg {
                        FunctionArg::Unnamed(FunctionArgExpr::Expr(expr)) => {
                            if counts_handles {
                                // COUNT(output) counts captured values.
                                self.raw(expr)?;
                            } else {
                                self.scalar(expr)?;
                            }
                        }
                        FunctionArg::Unnamed(FunctionArgExpr::Wildcard) => {}
                        _ => return err("named or qualified-wildcard arguments are not supported"),
                    }
                }
            }
        }
        if !matches!(function.parameters, FunctionArguments::None) {
            return err("parameterized functions are not supported");
        }
        if let Some(filter) = &mut function.filter {
            self.boolean(filter)?;
        }
        for order in &mut function.within_group {
            self.scalar(&mut order.expr)?;
        }
        match &mut function.over {
            None => {}
            Some(WindowType::WindowSpec(spec)) => {
                if spec.window_name.is_some() {
                    return err("named windows are not supported");
                }
                for expr in &mut spec.partition_by {
                    self.scalar(expr)?;
                }
                for order in &mut spec.order_by {
                    self.scalar(&mut order.expr)?;
                }
            }
            Some(WindowType::NamedWindow(_)) => return err("named windows are not supported"),
        }
        Ok(())
    }

    /// A position that needs a plain SQL value: render handles.
    fn scalar(&mut self, expr: &mut Expr) -> Result<(), SqlError> {
        if self.raw(expr)? == Kind::Value {
            let handle = take(expr);
            *expr = call("__btel_render", vec![handle]);
        }
        Ok(())
    }

    /// A predicate position: a bare value is true only when it is `true`.
    fn boolean(&mut self, expr: &mut Expr) -> Result<(), SqlError> {
        if self.raw(expr)? == Kind::Value {
            let handle = take(expr);
            *expr = call("__btel_truthy", vec![handle]);
        }
        Ok(())
    }
}

/// Move an expression out, leaving a NULL literal behind.
fn take(expr: &mut Expr) -> Expr {
    std::mem::replace(expr, Expr::Value(Value::Null.into()))
}

fn flip(op: &str) -> &'static str {
    match op {
        "<" => ">",
        "<=" => ">=",
        ">" => "<",
        ">=" => "<=",
        "=" => "=",
        _ => "!=",
    }
}

/// `__btel_cmp(handle, op, rhs, kind)`: boolean literals keep their kind;
/// every other operand compares by its SQL storage class.
fn compare_call(handle: Expr, op: &str, rhs: Expr) -> Expr {
    let kind = match &rhs {
        Expr::Value(v) if matches!(v.value, Value::Boolean(_)) => "bool",
        _ => "sql",
    };
    call("__btel_cmp", vec![handle, string(op), rhs, string(kind)])
}

#[cfg(test)]
mod tests;
