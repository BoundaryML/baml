use std::{collections::HashMap, fmt::Debug};

use minijinja::machinery::{ast, Span};

pub(super) struct AssignmentTracker<'a> {
    nested_out: HashMap<String, Vec<Span>>,
    assigned: Vec<HashMap<&'a str, Vec<Span>>>,
    deps: Dependencies,
}

#[allow(dead_code)]
impl<'a> AssignmentTracker<'a> {
    fn is_assigned(&self, name: &str) -> bool {
        self.assigned.iter().any(|x| x.contains_key(name))
    }

    fn assign(&mut self, name: &'a str, span: Span) {
        let last = self.assigned.last_mut().unwrap();
        if !last.contains_key(name) {
            last.insert(name, vec![span]);
        } else {
            last.get_mut(name).unwrap().push(span);
        }
    }

    fn assign_nested(&mut self, name: String, span: Span) {
        match self.deps.variables.get_mut(&name) {
            Some(spans) => spans.push(span.start_offset as usize),
            None => {
                self.deps
                    .variables
                    .insert(name.clone(), vec![span.start_offset as usize]);
            }
        }

        if !self.nested_out.contains_key(&name) {
            self.nested_out.insert(name, vec![span]);
        } else {
            self.nested_out.get_mut(&name).unwrap().push(span);
        }
    }

    fn push(&mut self) {
        self.assigned.push(Default::default());
    }

    fn pop(&mut self) {
        self.assigned.pop();
    }
}

fn tracker_visit_expr_opt<'a>(expr: &Option<ast::Expr<'a>>, state: &mut AssignmentTracker<'a>) {
    if let Some(expr) = expr {
        tracker_visit_expr(expr, state);
    }
}

fn tracker_visit_macro<'a>(m: &ast::Macro<'a>, state: &mut AssignmentTracker<'a>) {
    m.args.iter().for_each(|arg| track_assign(arg, state));
    m.defaults
        .iter()
        .for_each(|expr| tracker_visit_expr(expr, state));
    m.body.iter().for_each(|node| track_walk(node, state));
}

fn get_scoped_tracking<'a>(
    expr: &ast::Expr<'a>,
    curr_state: &AssignmentTracker<'a>,
) -> Option<ParameterizedValue> {
    // Determine how many candidates we have for the function call.
    let mut state = AssignmentTracker {
        nested_out: Default::default(),
        assigned: vec![Default::default()],
        deps: Default::default(),
    };
    tracker_visit_expr(expr, &mut state);

    match state.nested_out.len() {
        0 => None,
        1 => {
            let (name, spans) = state.nested_out.iter().next().unwrap();
            if curr_state.is_assigned(name) {
                Some(ParameterizedValue::Temporary(
                    spans[0].start_offset as usize,
                ))
            } else {
                Some(ParameterizedValue::Value(
                    name.clone(),
                    spans[0].start_offset as usize,
                ))
            }
        }
        _ => {
            let mut candidates = vec![];
            for (name, spans) in state.nested_out.iter() {
                candidates.push((
                    name.clone(),
                    spans.iter().map(|x| x.start_offset as usize).collect(),
                ));
            }
            Some(ParameterizedValue::Expression(candidates))
        }
    }
}

fn tracker_visit_expr<'a>(expr: &ast::Expr<'a>, state: &mut AssignmentTracker<'a>) {
    match expr {
        ast::Expr::Var(var) => {
            if !state.is_assigned(var.id) {
                // if we are not tracking nested assignments, we can consider a variable
                // to be assigned the first time we perform a lookup.
                state.assign_nested(var.id.to_string(), var.span());
            }
        }
        ast::Expr::Const(_) => {}
        ast::Expr::UnaryOp(expr) => tracker_visit_expr(&expr.expr, state),
        ast::Expr::BinOp(expr) => {
            tracker_visit_expr(&expr.left, state);
            tracker_visit_expr(&expr.right, state);
        }
        ast::Expr::IfExpr(expr) => {
            tracker_visit_expr(&expr.test_expr, state);
            tracker_visit_expr(&expr.true_expr, state);
            tracker_visit_expr_opt(&expr.false_expr, state);
        }
        ast::Expr::Filter(expr) => {
            tracker_visit_expr_opt(&expr.expr, state);
            expr.args.iter().for_each(|x| tracker_visit_expr(x, state));
        }
        ast::Expr::Test(expr) => {
            tracker_visit_expr(&expr.expr, state);
            expr.args.iter().for_each(|x| tracker_visit_expr(x, state));
        }
        ast::Expr::GetAttr(expr) => {
            // if we are tracking nested, we check if we have a chain of attribute
            // lookups that terminate in a variable lookup.  In that case we can
            // assign the nested lookup.
            let mut attrs = vec![expr.name];
            let mut ptr = &expr.expr;
            loop {
                match ptr {
                    ast::Expr::Var(var) => {
                        if !state.is_assigned(var.id) {
                            let mut rv = var.id.to_string();
                            for attr in attrs.iter().rev() {
                                // TODO: Why is this not working?
                                // write!(rv, ".{}", attr).ok();
                                rv.push('.');
                                rv.push_str(attr);
                            }
                            state.assign_nested(rv, var.span());
                            return;
                        }
                    }
                    ast::Expr::GetAttr(expr) => {
                        attrs.push(expr.name);
                        ptr = &expr.expr;
                        continue;
                    }
                    _ => break,
                }
            }

            tracker_visit_expr(&expr.expr, state)
        }
        ast::Expr::GetItem(expr) => {
            tracker_visit_expr(&expr.expr, state);
            tracker_visit_expr(&expr.subscript_expr, state);
        }
        ast::Expr::Slice(slice) => {
            tracker_visit_expr_opt(&slice.start, state);
            tracker_visit_expr_opt(&slice.stop, state);
            tracker_visit_expr_opt(&slice.step, state);
        }
        ast::Expr::Call(expr) => {
            let fn_name = get_scoped_tracking(&expr.expr, state);
            tracker_visit_expr(&expr.expr, state);
            if matches!(
                fn_name,
                Some(ParameterizedValue::Value(..)) | Some(ParameterizedValue::Expression(..))
            ) {
                // We can likely provider good error messages for this case.
                // So try and track the arguments.
                let fn_name = fn_name.unwrap();

                // If the last arg is a dict, we can track the keys.
                let (positional_args, named_args) = match expr.args.len() {
                    0 => (vec![], Default::default()),
                    1 => {
                        let arg = &expr.args[0];
                        match arg {
                            ast::Expr::Kwargs(kwargs) => (
                                vec![],
                                kwargs
                                    .pairs
                                    .iter()
                                    .map(|(k, val)| {
                                        let res = get_scoped_tracking(val, &state);
                                        (k.to_string(), res)
                                    })
                                    .collect(),
                            ),
                            _ => (vec![get_scoped_tracking(arg, state)], Default::default()),
                        }
                    }
                    _ => {
                        // We have more than 1 arg. Lets see if the last arg is a dict.
                        let last_arg = &expr.args[expr.args.len() - 1];
                        match last_arg {
                            ast::Expr::Kwargs(kwargs) => {
                                let positional_args = expr.args[..expr.args.len() - 1]
                                    .iter()
                                    .map(|x| get_scoped_tracking(x, &state))
                                    .collect();
                                (
                                    positional_args,
                                    kwargs
                                        .pairs
                                        .iter()
                                        .map(|(k, val)| {
                                            let res = get_scoped_tracking(val, &state);
                                            (k.to_string(), res)
                                        })
                                        .collect(),
                                )
                            }
                            _ => (
                                expr.args
                                    .iter()
                                    .map(|x| get_scoped_tracking(x, &state))
                                    .collect(),
                                Default::default(),
                            ),
                        }
                    }
                };

                state.deps.function_calls.push(FunctionCall {
                    candidates: fn_name,
                    args: (positional_args, named_args),
                });

                expr.args.iter().for_each(|x| tracker_visit_expr(x, state));
            } else {
                expr.args.iter().for_each(|x| tracker_visit_expr(x, state));
            }
        }
        ast::Expr::List(expr) => expr.items.iter().for_each(|x| tracker_visit_expr(x, state)),
        ast::Expr::Map(expr) => expr.keys.iter().zip(expr.values.iter()).for_each(|(k, v)| {
            tracker_visit_expr(k, state);
            tracker_visit_expr(v, state);
        }),
        ast::Expr::Kwargs(expr) => expr
            .pairs
            .iter()
            .for_each(|(_, v)| tracker_visit_expr(v, state)),
    }
}

fn track_assign<'a>(expr: &ast::Expr<'a>, state: &mut AssignmentTracker<'a>) {
    match expr {
        ast::Expr::Var(var) => state.assign(var.id, var.span()),
        ast::Expr::List(list) => list.items.iter().for_each(|x| track_assign(x, state)),
        _ => {}
    }
}

fn track_walk<'a>(node: &ast::Stmt<'a>, state: &mut AssignmentTracker<'a>) {
    // println!("node: {:?}", node);
    match node {
        ast::Stmt::Template(stmt) => {
            state.assign("self", stmt.span());
            stmt.children.iter().for_each(|x| track_walk(x, state));
        }
        ast::Stmt::EmitExpr(expr) => tracker_visit_expr(&expr.expr, state),
        ast::Stmt::EmitRaw(_) => {}
        ast::Stmt::ForLoop(stmt) => {
            state.push();
            state.assign("loop", stmt.span());
            tracker_visit_expr(&stmt.iter, state);
            track_assign(&stmt.target, state);
            tracker_visit_expr_opt(&stmt.filter_expr, state);
            stmt.body.iter().for_each(|x| track_walk(x, state));
            state.pop();
            state.push();
            stmt.else_body.iter().for_each(|x| track_walk(x, state));
            state.pop();
        }
        ast::Stmt::IfCond(stmt) => {
            tracker_visit_expr(&stmt.expr, state);
            state.push();
            stmt.true_body.iter().for_each(|x| track_walk(x, state));
            state.pop();
            state.push();
            stmt.false_body.iter().for_each(|x| track_walk(x, state));
            state.pop();
        }
        ast::Stmt::WithBlock(stmt) => {
            state.push();
            for (target, expr) in &stmt.assignments {
                track_assign(target, state);
                tracker_visit_expr(expr, state);
            }
            stmt.body.iter().for_each(|x| track_walk(x, state));
            state.pop();
        }
        ast::Stmt::Set(stmt) => {
            track_assign(&stmt.target, state);
            tracker_visit_expr(&stmt.expr, state);
        }
        ast::Stmt::AutoEscape(stmt) => {
            state.push();
            stmt.body.iter().for_each(|x| track_walk(x, state));
            state.pop();
        }
        ast::Stmt::FilterBlock(stmt) => {
            state.push();
            stmt.body.iter().for_each(|x| track_walk(x, state));
            state.pop();
        }
        ast::Stmt::SetBlock(stmt) => {
            track_assign(&stmt.target, state);
            state.push();
            stmt.body.iter().for_each(|x| track_walk(x, state));
            state.pop();
        }
        ast::Stmt::Macro(stmt) => {
            state.assign(stmt.name, stmt.span());
            tracker_visit_macro(stmt, state);
        }
        ast::Stmt::CallBlock(stmt) => {
            tracker_visit_expr(&stmt.call.expr, state);
            stmt.call
                .args
                .iter()
                .for_each(|x| tracker_visit_expr(x, state));
            tracker_visit_macro(&stmt.macro_decl, state);
        }
        ast::Stmt::Do(stmt) => {
            tracker_visit_expr(&stmt.call.expr, state);
            stmt.call
                .args
                .iter()
                .for_each(|x| tracker_visit_expr(x, state));
        }
    }
}

#[derive(Default, PartialEq, Eq)]
pub struct Dependencies {
    // name: span_start[]
    variables: HashMap<String, Vec<usize>>,
    function_calls: Vec<FunctionCall>,
}

#[derive(PartialEq, Eq)]
pub enum ParameterizedValue {
    Expression(Vec<(String, Vec<usize>)>),
    Value(String, usize),
    Temporary(usize),
}

#[derive(PartialEq, Eq)]
pub struct FunctionCall {
    pub candidates: ParameterizedValue,
    pub args: (
        Vec<Option<ParameterizedValue>>,
        HashMap<String, Option<ParameterizedValue>>,
    ),
}

impl Dependencies {
    #[allow(dead_code)]
    pub fn from(variables: HashMap<String, Vec<usize>>, function_calls: Vec<FunctionCall>) -> Self {
        Self {
            variables,
            function_calls,
        }
    }

    #[cfg(test)]
    pub fn from_test_case(
        variables: &[(&str, &[usize])],
        function_calls: Vec<FunctionCall>,
    ) -> Self {
        Self {
            variables: variables
                .into_iter()
                .map(|(k, v)| (k.to_string(), Vec::from(*v)))
                .collect(),
            function_calls,
        }
    }
}

#[allow(dead_code)]
pub(super) fn get_all_vars<'a>(parsed: &ast::Stmt<'a>) -> Dependencies {
    let mut state = AssignmentTracker {
        nested_out: Default::default(),
        assigned: vec![Default::default()],
        deps: Default::default(),
    };
    track_walk(parsed, &mut state);
    state.deps
}

impl Debug for Dependencies {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Variables:\n")?;
        for (k, v) in &self.variables {
            write!(
                f,
                "    {}: {}\n",
                k,
                match v.len() {
                    0 => "<UNEXPECTED>".to_string(),
                    1 => v[0].to_string(),
                    _ => format!("{:?}", v),
                }
            )?;
        }

        write!(f, "Function Calls:\n")?;
        for call in &self.function_calls {
            write!(f, "    {:?}\n", call)?;
        }

        Ok(())
    }
}

impl Debug for ParameterizedValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ParameterizedValue::Expression(candidates) => {
                write!(
                    f,
                    "{:?}",
                    candidates
                        .iter()
                        .map(|(k, v)| format!("{k}:{:?}", v))
                        .collect::<Vec<_>>()
                        .join("|")
                )
            }
            ParameterizedValue::Value(name, span) => {
                write!(f, "{name}:{span}")
            }
            ParameterizedValue::Temporary(span) => {
                write!(f, "Temporary:{}", span)
            }
        }
    }
}

impl Debug for FunctionCall {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{:?}({})",
            self.candidates,
            self.args
                .0
                .iter()
                .map(|a| {
                    if let Some(a) = a {
                        format!("{:?}", a)
                    } else {
                        "<constant>".to_string()
                    }
                })
                .chain(self.args.1.iter().map(|(k, v)| format!(
                    "{k}={}",
                    if let Some(v) = v {
                        format!("{:?}", v)
                    } else {
                        "<constant>".to_string()
                    }
                )))
                .collect::<Vec<_>>()
                .join(", "),
        )
    }
}
