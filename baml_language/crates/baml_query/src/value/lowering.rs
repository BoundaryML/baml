//! Ordinary-SQL lowering for virtual BAML values (D7).
//!
//! Users write ordinary subscripts and comparisons — `args['customer']
//! ['age'] >= 30`, `args = baml_value_cid('bamlv_1_…')` — and this
//! planner lowers them into internal `__baml_*` expressions. Internal
//! names are not public contract: the statement gatekeeper rejects them
//! in user SQL, and only this planner plants them.
//!
//! Frozen surface (IN-Q1-1/2/3):
//! - `args` is a named-argument object: `args['name']`; a numeric
//!   subscript on the `args` root is a planning error with a remedy.
//! - String subscripts select map/class fields, zero-based integer
//!   subscripts select list elements; paths are plan-time constants.
//! - Comparisons are canonical semantics; rendering (`SELECT args[…]`)
//!   is bare text for scalar leaves and canonical JSON for structures.

use std::sync::Arc;

use bex_events::store::canon::CanonValue;
use datafusion::arrow::array::{Array as _, ArrayRef, BinaryArray, BooleanBuilder, StringBuilder};
use datafusion::arrow::datatypes::{DataType, FieldRef};
use datafusion::common::{
    DFSchema, ExprSchema, Result as DfResult, ScalarValue, exec_err, plan_err,
};
use datafusion::logical_expr::expr::ScalarFunction;
use datafusion::logical_expr::planner::{
    ExprPlanner, PlannerResult, RawBinaryExpr, RawFieldAccessExpr,
};
use datafusion::logical_expr::{
    ColumnarValue, Expr, GetFieldAccess, ScalarFunctionArgs, ScalarUDF, ScalarUDFImpl, Signature,
    Volatility,
};
use datafusion::sql::sqlparser::ast::BinaryOperator;

use crate::catalog::{VALUE_META_KEY, VALUE_META_VALUE, VALUE_ROLE_KEY};
use crate::outcome::UnavailableReason;
use crate::value::resolver::{HydrationContext, Resolved};
use crate::value::semantics::{self, CmpOp, Nav, PathSeg};

/// Internal (planner-planted, never user-visible) function names.
pub const FN_PATH: &str = "__baml_path";
pub const FN_VCMP: &str = "__baml_vcmp";
pub const FN_VCMP_VALUE: &str = "__baml_vcmp_value";
pub const FN_VCMP_CID: &str = "__baml_vcmp_cid";
pub const FN_VCMP_JSON: &str = "__baml_vcmp_json";

/// Public value-literal constructors. They exist only as comparison
/// operands; reaching execution is a planning bug surfaced as an error.
pub const FN_VALUE_CID: &str = "baml_value_cid";
pub const FN_VALUE_JSON: &str = "baml_value_json";

/// The full UDF set for one query, bound to its hydration context.
pub struct ValueFunctions {
    pub path: Arc<ScalarUDF>,
    pub vcmp: Arc<ScalarUDF>,
    pub vcmp_value: Arc<ScalarUDF>,
    pub vcmp_cid: Arc<ScalarUDF>,
    pub vcmp_json: Arc<ScalarUDF>,
    pub value_cid: Arc<ScalarUDF>,
    pub value_json: Arc<ScalarUDF>,
}

impl ValueFunctions {
    #[must_use]
    pub fn new(ctx: Arc<HydrationContext>) -> Arc<ValueFunctions> {
        Arc::new(ValueFunctions {
            path: Arc::new(ScalarUDF::from(PathUdf { ctx: ctx.clone() })),
            vcmp: Arc::new(ScalarUDF::from(VcmpUdf { ctx: ctx.clone() })),
            vcmp_value: Arc::new(ScalarUDF::from(VcmpValueUdf { ctx: ctx.clone() })),
            vcmp_cid: Arc::new(ScalarUDF::from(VcmpCidUdf { ctx: ctx.clone() })),
            vcmp_json: Arc::new(ScalarUDF::from(VcmpJsonUdf { ctx })),
            value_cid: Arc::new(ScalarUDF::from(ValueLiteralUdf::cid())),
            value_json: Arc::new(ScalarUDF::from(ValueLiteralUdf::json())),
        })
    }

    #[must_use]
    pub fn all(&self) -> Vec<Arc<ScalarUDF>> {
        vec![
            self.path.clone(),
            self.vcmp.clone(),
            self.vcmp_value.clone(),
            self.vcmp_cid.clone(),
            self.vcmp_json.clone(),
            self.value_cid.clone(),
            self.value_json.clone(),
        ]
    }
}

impl std::fmt::Debug for ValueFunctions {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ValueFunctions").finish_non_exhaustive()
    }
}

// ── planner ────────────────────────────────────────────────────────────

/// A value expression decomposed to its handle column + constant path.
struct ValueExpr {
    column: Expr,
    path: Vec<PathSeg>,
    /// The captured role of the base column (`args`/`return`/`error`).
    role: String,
}

/// The ExprPlanner that recognizes value-typed expressions and lowers
/// subscripts/comparisons.
#[derive(Debug)]
pub struct BamlValuePlanner {
    functions: Arc<ValueFunctions>,
}

impl BamlValuePlanner {
    #[must_use]
    pub fn new(functions: Arc<ValueFunctions>) -> Arc<BamlValuePlanner> {
        Arc::new(BamlValuePlanner { functions })
    }

    /// Decompose `expr` when it is value-typed: a column with the value
    /// field metadata, or an already-planted `__baml_path` chain.
    fn value_expr(&self, expr: &Expr, schema: &DFSchema) -> Option<ValueExpr> {
        match expr {
            Expr::Column(column) => {
                let field = schema.field_from_column(column).ok()?;
                let meta = field.metadata();
                if meta.get(VALUE_META_KEY).map(String::as_str) != Some(VALUE_META_VALUE) {
                    return None;
                }
                Some(ValueExpr {
                    column: expr.clone(),
                    path: Vec::new(),
                    role: meta.get(VALUE_ROLE_KEY).cloned().unwrap_or_default(),
                })
            }
            Expr::ScalarFunction(call) if call.func.name() == FN_PATH => {
                let column = call.args.first()?.clone();
                let path = literal_str(call.args.get(1)?)?;
                let path: Vec<PathSeg> = serde_json::from_str(&path).ok()?;
                // The role annotation rides in the third argument.
                let role = call.args.get(2).and_then(literal_str)?;
                Some(ValueExpr { column, path, role })
            }
            _ => None,
        }
    }

    fn path_expr(&self, value: ValueExpr) -> Expr {
        let path_json = serde_json::to_string(&value.path).expect("path serializes");
        Expr::ScalarFunction(ScalarFunction::new_udf(
            self.functions.path.clone(),
            vec![value.column, utf8_lit(path_json), utf8_lit(value.role)],
        ))
    }
}

impl ExprPlanner for BamlValuePlanner {
    fn plan_field_access(
        &self,
        expr: RawFieldAccessExpr,
        schema: &DFSchema,
    ) -> DfResult<PlannerResult<RawFieldAccessExpr>> {
        let Some(mut value) = self.value_expr(&expr.expr, schema) else {
            return Ok(PlannerResult::Original(expr));
        };
        let seg = match &expr.field_access {
            GetFieldAccess::NamedStructField { name } => match name {
                ScalarValue::Utf8(Some(key)) | ScalarValue::LargeUtf8(Some(key)) => {
                    PathSeg::Key(key.clone())
                }
                other => {
                    return plan_err!(
                        "BAML value subscript keys must be string literals, got {other}"
                    );
                }
            },
            GetFieldAccess::ListIndex { key } => match key.as_ref() {
                Expr::Literal(ScalarValue::Int64(Some(idx)), _) => {
                    if value.path.is_empty() && value.role == "args" {
                        // IN-Q1-1: args is a named-argument object.
                        return plan_err!(
                            "`args` is a named-argument object; subscript it with the \
                             parameter name, e.g. args['customer'] — argument order is \
                             not part of the canonical value"
                        );
                    }
                    PathSeg::Index(*idx)
                }
                _ => {
                    return plan_err!(
                        "BAML value subscripts must be constant integers or string \
                         literals; computed subscripts are not supported in v1"
                    );
                }
            },
            GetFieldAccess::ListRange { .. } => {
                return plan_err!("range subscripts are not supported on BAML values");
            }
        };
        value.path.push(seg);
        Ok(PlannerResult::Planned(self.path_expr(value)))
    }

    fn plan_binary_op(
        &self,
        expr: RawBinaryExpr,
        schema: &DFSchema,
    ) -> DfResult<PlannerResult<RawBinaryExpr>> {
        let Some(op) = cmp_op(&expr.op) else {
            return Ok(PlannerResult::Original(expr));
        };
        let left = self.value_expr(&expr.left, schema);
        let right = self.value_expr(&expr.right, schema);
        match (left, right) {
            (None, None) => Ok(PlannerResult::Original(expr)),
            (Some(l), Some(r)) => {
                let l_path = serde_json::to_string(&l.path).expect("path serializes");
                let r_path = serde_json::to_string(&r.path).expect("path serializes");
                Ok(PlannerResult::Planned(Expr::ScalarFunction(
                    ScalarFunction::new_udf(
                        self.functions.vcmp_value.clone(),
                        vec![
                            l.column,
                            utf8_lit(l_path),
                            r.column,
                            utf8_lit(r_path),
                            utf8_lit(op.as_str()),
                        ],
                    ),
                )))
            }
            (Some(value), None) => self.plan_value_vs_scalar(value, op, expr.right),
            (None, Some(value)) => self.plan_value_vs_scalar(value, mirror(op), expr.left),
        }
    }
}

impl BamlValuePlanner {
    fn plan_value_vs_scalar(
        &self,
        value: ValueExpr,
        op: CmpOp,
        other: Expr,
    ) -> DfResult<PlannerResult<RawBinaryExpr>> {
        let path_json = serde_json::to_string(&value.path).expect("path serializes");
        // Value-literal constructors compare by canonical semantics.
        if let Expr::ScalarFunction(call) = &other {
            let (udf, literal) = match call.func.name() {
                FN_VALUE_CID => (self.functions.vcmp_cid.clone(), call.args.first()),
                FN_VALUE_JSON => (self.functions.vcmp_json.clone(), call.args.first()),
                _ => (self.functions.vcmp.clone(), None),
            };
            if let Some(literal) = literal {
                if !matches!(op, CmpOp::Eq | CmpOp::NotEq) {
                    return plan_err!(
                        "BAML value literals support = and != only; ordering over whole \
                         values is not defined"
                    );
                }
                return Ok(PlannerResult::Planned(Expr::ScalarFunction(
                    ScalarFunction::new_udf(
                        udf,
                        vec![
                            value.column,
                            utf8_lit(path_json),
                            literal.clone(),
                            utf8_lit(op.as_str()),
                        ],
                    ),
                )));
            }
        }
        Ok(PlannerResult::Planned(Expr::ScalarFunction(
            ScalarFunction::new_udf(
                self.functions.vcmp.clone(),
                vec![
                    value.column,
                    utf8_lit(path_json),
                    utf8_lit(op.as_str()),
                    other,
                ],
            ),
        )))
    }
}

fn cmp_op(op: &BinaryOperator) -> Option<CmpOp> {
    Some(match op {
        BinaryOperator::Eq => CmpOp::Eq,
        BinaryOperator::NotEq => CmpOp::NotEq,
        BinaryOperator::Lt => CmpOp::Lt,
        BinaryOperator::LtEq => CmpOp::LtEq,
        BinaryOperator::Gt => CmpOp::Gt,
        BinaryOperator::GtEq => CmpOp::GtEq,
        _ => return None,
    })
}

/// Mirror the operator when the value moved from right to left.
fn mirror(op: CmpOp) -> CmpOp {
    match op {
        CmpOp::Lt => CmpOp::Gt,
        CmpOp::LtEq => CmpOp::GtEq,
        CmpOp::Gt => CmpOp::Lt,
        CmpOp::GtEq => CmpOp::LtEq,
        CmpOp::Eq | CmpOp::NotEq => op,
    }
}

fn utf8_lit(s: impl Into<String>) -> Expr {
    Expr::Literal(ScalarValue::Utf8(Some(s.into())), None)
}

fn literal_str(expr: &Expr) -> Option<String> {
    match expr {
        Expr::Literal(ScalarValue::Utf8(Some(s)), _) => Some(s.clone()),
        _ => None,
    }
}

impl PartialEq for PathUdf {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.ctx, &other.ctx)
    }
}
impl Eq for PathUdf {}
impl std::hash::Hash for PathUdf {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        std::ptr::hash(Arc::as_ptr(&self.ctx), state);
    }
}

impl PartialEq for VcmpUdf {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.ctx, &other.ctx)
    }
}
impl Eq for VcmpUdf {}
impl std::hash::Hash for VcmpUdf {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        std::ptr::hash(Arc::as_ptr(&self.ctx), state);
    }
}

impl PartialEq for VcmpValueUdf {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.ctx, &other.ctx)
    }
}
impl Eq for VcmpValueUdf {}
impl std::hash::Hash for VcmpValueUdf {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        std::ptr::hash(Arc::as_ptr(&self.ctx), state);
    }
}

impl PartialEq for VcmpCidUdf {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.ctx, &other.ctx)
    }
}
impl Eq for VcmpCidUdf {}
impl std::hash::Hash for VcmpCidUdf {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        std::ptr::hash(Arc::as_ptr(&self.ctx), state);
    }
}

impl PartialEq for VcmpJsonUdf {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.ctx, &other.ctx)
    }
}
impl Eq for VcmpJsonUdf {}
impl std::hash::Hash for VcmpJsonUdf {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        std::ptr::hash(Arc::as_ptr(&self.ctx), state);
    }
}

impl PartialEq for ValueLiteralUdf {
    fn eq(&self, other: &Self) -> bool {
        self.name == other.name
    }
}
impl Eq for ValueLiteralUdf {}
impl std::hash::Hash for ValueLiteralUdf {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.name.hash(state);
    }
}

/// Rewrite bare value columns in projections (`SELECT args`, `SELECT *`)
/// into rendered form: the raw handle bytes are provider-private and
/// never part of a public result. Runs between SQL planning and
/// execution; subscripted/compared values were already lowered by the
/// planner hooks.
pub fn rewrite_bare_value_columns(
    plan: datafusion::logical_expr::LogicalPlan,
    functions: &Arc<ValueFunctions>,
) -> DfResult<datafusion::logical_expr::LogicalPlan> {
    use datafusion::common::tree_node::{Transformed, TreeNode};
    use datafusion::logical_expr::{LogicalPlan, Projection};
    plan.transform_up(|node| {
        let LogicalPlan::Projection(projection) = &node else {
            return Ok(Transformed::no(node));
        };
        let input_schema = projection.input.schema().clone();
        let mut changed = false;
        let exprs: Vec<Expr> = projection
            .expr
            .iter()
            .map(|expr| {
                let Expr::Column(column) = expr else {
                    return expr.clone();
                };
                let Ok(field) = input_schema.field_from_column(column) else {
                    return expr.clone();
                };
                let meta = field.metadata();
                if meta.get(VALUE_META_KEY).map(String::as_str) != Some(VALUE_META_VALUE) {
                    return expr.clone();
                }
                changed = true;
                let role = meta.get(VALUE_ROLE_KEY).cloned().unwrap_or_default();
                Expr::ScalarFunction(ScalarFunction::new_udf(
                    functions.path.clone(),
                    vec![expr.clone(), utf8_lit("[]"), utf8_lit(role)],
                ))
                .alias(column.name.clone())
            })
            .collect();
        if !changed {
            return Ok(Transformed::no(node));
        }
        let LogicalPlan::Projection(projection) = node else {
            unreachable!("matched above");
        };
        Ok(Transformed::yes(LogicalPlan::Projection(
            Projection::try_new(exprs, projection.input)?,
        )))
    })
    .map(|transformed| transformed.data)
}

// ── shared UDF machinery ───────────────────────────────────────────────

/// Evaluate one row's handle → navigated value, with full availability
/// accounting. `Ok(None)` = SQL NULL (captured null / absent path / role
/// not applicable / typed-unavailable — the tracker already knows which).
fn eval_navigate(
    ctx: &HydrationContext,
    handle: Option<&[u8]>,
    path: &[PathSeg],
) -> Option<Arc<CanonValue>> {
    // NULL handle: the role is not applicable for this row (a successful
    // call has no error). Not an evidence gap — not counted.
    let handle = handle?;
    match ctx.resolve(handle) {
        Resolved::Unavailable(reason) => {
            ctx.tracker().record_unavailable(reason);
            None
        }
        Resolved::Value(value) => match semantics::navigate(&value, path) {
            Nav::Found(found) => {
                ctx.tracker().record_available();
                // Clone out of the Arc'd tree: leaves are small; whole-value
                // results share the Arc when the path is empty.
                if path.is_empty() {
                    Some(value.clone())
                } else {
                    Some(Arc::new(found.clone()))
                }
            }
            Nav::Null | Nav::Missing => {
                ctx.tracker().record_available();
                None
            }
            Nav::Elided => {
                ctx.tracker()
                    .record_unavailable(UnavailableReason::Truncated);
                None
            }
        },
    }
}

fn parse_path(path_json: &str) -> DfResult<Vec<PathSeg>> {
    serde_json::from_str(path_json)
        .map_err(|e| datafusion::common::DataFusionError::Internal(format!("bad value path: {e}")))
}

fn scalar_arg_str(args: &[ColumnarValue], idx: usize) -> DfResult<String> {
    match args.get(idx) {
        Some(ColumnarValue::Scalar(ScalarValue::Utf8(Some(s)))) => Ok(s.clone()),
        other => exec_err!("internal value fn: expected utf8 literal arg {idx}, got {other:?}"),
    }
}

fn handle_array(args: &[ColumnarValue], idx: usize, rows: usize) -> DfResult<ArrayRef> {
    let array = match &args[idx] {
        ColumnarValue::Array(array) => array.clone(),
        // Scalars broadcast to the batch length (a lone scalar in
        // values_to_arrays would broadcast to length 1, not `rows`).
        ColumnarValue::Scalar(scalar) => scalar.to_array_of_size(rows)?,
    };
    if array.len() == rows {
        Ok(array)
    } else {
        exec_err!("internal value fn: arg {idx} length mismatch")
    }
}

fn as_binary(array: &ArrayRef) -> DfResult<&BinaryArray> {
    array.as_any().downcast_ref::<BinaryArray>().ok_or_else(|| {
        datafusion::common::DataFusionError::Internal(
            "value handle column must be Binary".to_string(),
        )
    })
}

/// Convert one RHS Arrow cell to a canonical value for comparison.
/// `None` = SQL NULL on the RHS (comparison result NULL).
fn arrow_cell_to_canon(array: &ArrayRef, row: usize) -> Option<CanonValue> {
    use datafusion::arrow::array::{
        BooleanArray, Float64Array, Int32Array, Int64Array, StringArray, UInt32Array, UInt64Array,
    };
    if array.is_null(row) {
        return None;
    }
    let any = array.as_any();
    if let Some(a) = any.downcast_ref::<Int64Array>() {
        return Some(CanonValue::Int(a.value(row)));
    }
    if let Some(a) = any.downcast_ref::<Int32Array>() {
        return Some(CanonValue::Int(i64::from(a.value(row))));
    }
    if let Some(a) = any.downcast_ref::<UInt64Array>() {
        return Some(match i64::try_from(a.value(row)) {
            Ok(v) => CanonValue::Int(v),
            Err(_) => CanonValue::Bigint(a.value(row).to_string()),
        });
    }
    if let Some(a) = any.downcast_ref::<UInt32Array>() {
        return Some(CanonValue::Int(i64::from(a.value(row))));
    }
    if let Some(a) = any.downcast_ref::<Float64Array>() {
        return Some(CanonValue::Float(a.value(row)));
    }
    if let Some(a) = any.downcast_ref::<StringArray>() {
        return Some(CanonValue::String(a.value(row).to_string()));
    }
    if let Some(a) = any.downcast_ref::<BooleanArray>() {
        return Some(CanonValue::Bool(a.value(row)));
    }
    None
}

// ── __baml_path ────────────────────────────────────────────────────────

#[derive(Debug)]
struct PathUdf {
    ctx: Arc<HydrationContext>,
}

impl ScalarUDFImpl for PathUdf {
    fn name(&self) -> &str {
        FN_PATH
    }
    fn signature(&self) -> &Signature {
        static SIG: std::sync::OnceLock<Signature> = std::sync::OnceLock::new();
        SIG.get_or_init(|| Signature::any(3, Volatility::Volatile))
    }
    fn return_type(&self, _args: &[DataType]) -> DfResult<DataType> {
        Ok(DataType::Utf8)
    }

    fn invoke_with_args(&self, args: ScalarFunctionArgs) -> DfResult<ColumnarValue> {
        let rows = args.number_rows;
        let path = parse_path(&scalar_arg_str(&args.args, 1)?)?;
        let handles = handle_array(&args.args, 0, rows)?;
        let handles = as_binary(&handles)?;
        let mut out = StringBuilder::new();
        for row in 0..rows {
            let handle = (!handles.is_null(row)).then(|| handles.value(row));
            match eval_navigate(&self.ctx, handle, &path) {
                Some(value) => out.append_value(semantics::render(&value)),
                None => out.append_null(),
            }
        }
        Ok(ColumnarValue::Array(Arc::new(out.finish())))
    }
}

// ── __baml_vcmp (value vs ordinary SQL operand) ────────────────────────

#[derive(Debug)]
struct VcmpUdf {
    ctx: Arc<HydrationContext>,
}

impl ScalarUDFImpl for VcmpUdf {
    fn name(&self) -> &str {
        FN_VCMP
    }
    fn signature(&self) -> &Signature {
        static SIG: std::sync::OnceLock<Signature> = std::sync::OnceLock::new();
        SIG.get_or_init(|| Signature::any(4, Volatility::Volatile))
    }
    fn return_type(&self, _args: &[DataType]) -> DfResult<DataType> {
        Ok(DataType::Boolean)
    }

    fn invoke_with_args(&self, args: ScalarFunctionArgs) -> DfResult<ColumnarValue> {
        let rows = args.number_rows;
        let path = parse_path(&scalar_arg_str(&args.args, 1)?)?;
        let op = CmpOp::parse(&scalar_arg_str(&args.args, 2)?)
            .ok_or_else(|| datafusion::common::DataFusionError::Internal("bad op".into()))?;
        let handles = handle_array(&args.args, 0, rows)?;
        let handles = as_binary(&handles)?;
        let rhs = handle_array(&args.args, 3, rows)?;
        let mut out = BooleanBuilder::new();
        for row in 0..rows {
            let handle = (!handles.is_null(row)).then(|| handles.value(row));
            let left = eval_navigate(&self.ctx, handle, &path);
            let result = match (left, arrow_cell_to_canon(&rhs, row)) {
                (Some(l), Some(r)) => semantics::compare(op, &l, &r),
                _ => None,
            };
            out.append_option(result);
        }
        Ok(ColumnarValue::Array(Arc::new(out.finish())))
    }
}

// ── __baml_vcmp_value (value vs value) ─────────────────────────────────

#[derive(Debug)]
struct VcmpValueUdf {
    ctx: Arc<HydrationContext>,
}

impl ScalarUDFImpl for VcmpValueUdf {
    fn name(&self) -> &str {
        FN_VCMP_VALUE
    }
    fn signature(&self) -> &Signature {
        static SIG: std::sync::OnceLock<Signature> = std::sync::OnceLock::new();
        SIG.get_or_init(|| Signature::any(5, Volatility::Volatile))
    }
    fn return_type(&self, _args: &[DataType]) -> DfResult<DataType> {
        Ok(DataType::Boolean)
    }

    fn invoke_with_args(&self, args: ScalarFunctionArgs) -> DfResult<ColumnarValue> {
        let rows = args.number_rows;
        let l_path = parse_path(&scalar_arg_str(&args.args, 1)?)?;
        let r_path = parse_path(&scalar_arg_str(&args.args, 3)?)?;
        let op = CmpOp::parse(&scalar_arg_str(&args.args, 4)?)
            .ok_or_else(|| datafusion::common::DataFusionError::Internal("bad op".into()))?;
        let l_handles = handle_array(&args.args, 0, rows)?;
        let l_handles = as_binary(&l_handles)?;
        let r_handles = handle_array(&args.args, 2, rows)?;
        let r_handles = as_binary(&r_handles)?;
        let mut out = BooleanBuilder::new();
        for row in 0..rows {
            let l_handle = (!l_handles.is_null(row)).then(|| l_handles.value(row));
            let r_handle = (!r_handles.is_null(row)).then(|| r_handles.value(row));
            let left = eval_navigate(&self.ctx, l_handle, &l_path);
            let right = eval_navigate(&self.ctx, r_handle, &r_path);
            let result = match (left, right) {
                (Some(l), Some(r)) => semantics::compare(op, &l, &r),
                _ => None,
            };
            out.append_option(result);
        }
        Ok(ColumnarValue::Array(Arc::new(out.finish())))
    }
}

// ── __baml_vcmp_cid (value vs canonical reference) ─────────────────────

#[derive(Debug)]
struct VcmpCidUdf {
    ctx: Arc<HydrationContext>,
}

impl ScalarUDFImpl for VcmpCidUdf {
    fn name(&self) -> &str {
        FN_VCMP_CID
    }
    fn signature(&self) -> &Signature {
        static SIG: std::sync::OnceLock<Signature> = std::sync::OnceLock::new();
        SIG.get_or_init(|| Signature::any(4, Volatility::Volatile))
    }
    fn return_type(&self, _args: &[DataType]) -> DfResult<DataType> {
        Ok(DataType::Boolean)
    }

    fn invoke_with_args(&self, args: ScalarFunctionArgs) -> DfResult<ColumnarValue> {
        let rows = args.number_rows;
        let path = parse_path(&scalar_arg_str(&args.args, 1)?)?;
        let cid_wire = scalar_arg_str(&args.args, 2)?;
        let op = CmpOp::parse(&scalar_arg_str(&args.args, 3)?)
            .ok_or_else(|| datafusion::common::DataFusionError::Internal("bad op".into()))?;
        let negate = matches!(op, CmpOp::NotEq);
        let Some(cid) = bex_events::store::gc::parse_cid_wire(&cid_wire) else {
            return exec_err!(
                "baml_value_cid expects a canonical `bamlv_1_…` reference, got {cid_wire:?}"
            );
        };
        let handles = handle_array(&args.args, 0, rows)?;
        let handles = as_binary(&handles)?;
        // The referenced value hydrates once for the whole batch.
        let mut reference: Option<Resolved> = None;
        let mut out = BooleanBuilder::new();
        for row in 0..rows {
            if handles.is_null(row) {
                out.append_null();
                continue;
            }
            let handle = handles.value(row);
            // D7-sanctioned shortcut: canonical CIDs are semantic identity
            // — valid only for whole-value comparisons.
            if path.is_empty()
                && let Some(equal) = self.ctx.cid_shortcut(handle, &cid)
            {
                self.ctx.tracker().record_available();
                out.append_value(equal != negate);
                continue;
            }
            let Some(left) = eval_navigate(&self.ctx, Some(handle), &path) else {
                out.append_null();
                continue;
            };
            let resolved = reference
                .get_or_insert_with(|| self.ctx.resolve_cid(&cid))
                .clone();
            match resolved {
                Resolved::Unavailable(reason) => {
                    self.ctx.tracker().record_unavailable(reason);
                    out.append_null();
                }
                Resolved::Value(right) => {
                    out.append_value(semantics::semantic_eq(&left, &right) != negate);
                }
            }
        }
        Ok(ColumnarValue::Array(Arc::new(out.finish())))
    }
}

// ── __baml_vcmp_json (value vs JSON-built value) ───────────────────────

#[derive(Debug)]
struct VcmpJsonUdf {
    ctx: Arc<HydrationContext>,
}

impl ScalarUDFImpl for VcmpJsonUdf {
    fn name(&self) -> &str {
        FN_VCMP_JSON
    }
    fn signature(&self) -> &Signature {
        static SIG: std::sync::OnceLock<Signature> = std::sync::OnceLock::new();
        SIG.get_or_init(|| Signature::any(4, Volatility::Volatile))
    }
    fn return_type(&self, _args: &[DataType]) -> DfResult<DataType> {
        Ok(DataType::Boolean)
    }

    fn invoke_with_args(&self, args: ScalarFunctionArgs) -> DfResult<ColumnarValue> {
        let rows = args.number_rows;
        let path = parse_path(&scalar_arg_str(&args.args, 1)?)?;
        let json = scalar_arg_str(&args.args, 2)?;
        let op = CmpOp::parse(&scalar_arg_str(&args.args, 3)?)
            .ok_or_else(|| datafusion::common::DataFusionError::Internal("bad op".into()))?;
        let negate = matches!(op, CmpOp::NotEq);
        let parsed: serde_json::Value = serde_json::from_str(&json).map_err(|e| {
            datafusion::common::DataFusionError::Execution(format!(
                "baml_value_json expects valid JSON: {e}"
            ))
        })?;
        let reference = semantics::json_to_canon(&parsed);
        let handles = handle_array(&args.args, 0, rows)?;
        let handles = as_binary(&handles)?;
        let mut out = BooleanBuilder::new();
        for row in 0..rows {
            let handle = (!handles.is_null(row)).then(|| handles.value(row));
            match eval_navigate(&self.ctx, handle, &path) {
                Some(left) => out.append_value(semantics::semantic_eq(&left, &reference) != negate),
                None => out.append_null(),
            }
        }
        Ok(ColumnarValue::Array(Arc::new(out.finish())))
    }
}

// ── public value-literal constructors ──────────────────────────────────

/// `baml_value_cid` / `baml_value_json`: comparison operands only. The
/// planner rewrites comparisons containing them; any surviving direct
/// evaluation is a planning error made visible.
#[derive(Debug)]
struct ValueLiteralUdf {
    name: &'static str,
}

impl ValueLiteralUdf {
    fn cid() -> ValueLiteralUdf {
        ValueLiteralUdf { name: FN_VALUE_CID }
    }
    fn json() -> ValueLiteralUdf {
        ValueLiteralUdf {
            name: FN_VALUE_JSON,
        }
    }
}

impl ScalarUDFImpl for ValueLiteralUdf {
    fn name(&self) -> &str {
        self.name
    }
    fn signature(&self) -> &Signature {
        static SIG: std::sync::OnceLock<Signature> = std::sync::OnceLock::new();
        SIG.get_or_init(|| Signature::exact(vec![DataType::Utf8], Volatility::Volatile))
    }
    fn return_type(&self, _args: &[DataType]) -> DfResult<DataType> {
        Ok(DataType::Binary)
    }

    fn invoke_with_args(&self, _args: ScalarFunctionArgs) -> DfResult<ColumnarValue> {
        exec_err!(
            "{} is a BAML value literal: it can only appear as a comparison \
             operand against a value field (e.g. args = {}('…'))",
            self.name,
            self.name
        )
    }
}

// `FieldRef` is part of the UDF API surface; referenced here so the
// import stays obviously intentional if the API shifts.
const _: fn(&FieldRef) = |_| {};
