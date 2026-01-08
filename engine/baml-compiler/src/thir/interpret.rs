use std::{
    collections::HashMap,
    future::Future,
    pin::Pin,
    sync::{Arc, Mutex},
};

use anyhow::{anyhow, bail, Context, Result};
use baml_types::{BamlMap, BamlValue, BamlValueWithMeta};
use internal_baml_diagnostics::Span;

use crate::{
    thir::{Block, ClassConstructorField, Expr, ExprMetadata, Statement, THir},
    watch::SharedWatchHandler,
};

// Type alias for pinned boxed futures - conditionally Send for non-WASM targets
#[cfg(not(target_arch = "wasm32"))]
pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

#[cfg(target_arch = "wasm32")]
pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + 'a>>;

// Context for watch streaming - passed to LLM handler when calling LLM function for @watch variable
#[derive(Clone, Debug)]
pub struct WatchStreamContext {
    pub variable_name: String,
    pub stream_id: String,
}

// Trait aliases for conditional Send bounds
#[cfg(not(target_arch = "wasm32"))]
pub trait LlmHandler<Fut>:
    FnMut(String, Vec<BamlValue>, Option<WatchStreamContext>) -> Fut + Send + Sync
{
}
#[cfg(not(target_arch = "wasm32"))]
impl<F, Fut> LlmHandler<Fut> for F where
    F: FnMut(String, Vec<BamlValue>, Option<WatchStreamContext>) -> Fut + Send + Sync
{
}

#[cfg(target_arch = "wasm32")]
pub trait LlmHandler<Fut>:
    FnMut(String, Vec<BamlValue>, Option<WatchStreamContext>) -> Fut
{
}
#[cfg(target_arch = "wasm32")]
impl<F, Fut> LlmHandler<Fut> for F where
    F: FnMut(String, Vec<BamlValue>, Option<WatchStreamContext>) -> Fut
{
}

#[cfg(not(target_arch = "wasm32"))]
pub trait LlmFuture: Future<Output = Result<BamlValueWithMeta<ExprMetadata>>> + Send {}
#[cfg(not(target_arch = "wasm32"))]
impl<T> LlmFuture for T where T: Future<Output = Result<BamlValueWithMeta<ExprMetadata>>> + Send {}

#[cfg(target_arch = "wasm32")]
pub trait LlmFuture: Future<Output = Result<BamlValueWithMeta<ExprMetadata>>> {}
#[cfg(target_arch = "wasm32")]
impl<T> LlmFuture for T where T: Future<Output = Result<BamlValueWithMeta<ExprMetadata>>> {}

// Watch handler is now a concrete type - no more conditional compilation needed!

// TODO:
//  - Variables should be expressions, not BamlValues. Because we want to be able to
//    mutate them across REPL prompts and see the same downstream effects on their
//    containers that we would for mutating values within functions.

/// Information about a variable with @watch
#[derive(Clone)]
pub struct WatchVariable {
    /// The name of the variable
    pub name: String,
    /// The watch spec from the declaration
    pub spec: crate::watch::WatchSpec,
    /// Reference to the variable's value for change detection
    pub value_ref: Arc<Mutex<BamlValueWithMeta<ExprMetadata>>>,
    /// Last notified value (passed as prev to filter function)
    pub last_notified: Arc<Mutex<Option<BamlValue>>>,
    /// Last checked value (to avoid calling filter multiple times for same value)
    pub last_checked: Arc<Mutex<Option<BamlValue>>>,
}

/// A scope is a map of variable names to their values.
///
/// Variables are stored in refcells to allow for mutation.
pub struct Scope {
    pub variables: BamlMap<String, Arc<Mutex<BamlValueWithMeta<ExprMetadata>>>>,
    /// Track variables with @watch for change detection
    pub watch_variables: Vec<WatchVariable>,
    /// Flag to indicate this scope is for filter function evaluation
    /// When true, check_watch_changes should skip checking watch variables
    pub is_filter_context: bool,
}

/// Register a variable with @watch for tracking
fn register_watch_variable(
    scopes: &mut [Scope],
    name: &str,
    value_ref: Arc<Mutex<BamlValueWithMeta<ExprMetadata>>>,
    watch_spec: crate::watch::WatchSpec,
) {
    if let Some(scope) = scopes.last_mut() {
        scope.watch_variables.push(WatchVariable {
            name: name.to_string(),
            spec: watch_spec,
            value_ref,
            last_notified: Arc::new(Mutex::new(None)),
            last_checked: Arc::new(Mutex::new(None)),
        });
    }
}

/// Convert ExprMetadata value to WatchValueMetadata value
fn expr_value_to_watch_value(
    value: BamlValueWithMeta<ExprMetadata>,
) -> BamlValueWithMeta<crate::watch::WatchValueMetadata> {
    value.map_meta(|(_span, type_ir)| crate::watch::WatchValueMetadata {
        constraints: Vec::new(),
        response_checks: Vec::new(),
        completion: baml_types::Completion::default(),
        r#type: type_ir.clone().unwrap_or(baml_types::TypeIR::string()),
    })
}

/// Fire a watch notification for a specific variable (for manual $watch.notify() calls)
fn fire_watch_notification_for_variable(
    scopes: &[Scope],
    var_name: &str,
    watch_handler: &SharedWatchHandler,
    function_name: &str,
) -> Result<()> {
    // Find the variable in scopes
    for scope in scopes.iter().rev() {
        if let Some(value_ref) = scope.variables.get(var_name) {
            // Find the watch variable to get the current channel name
            let channel_name = scope
                .watch_variables
                .iter()
                .find(|wv| Arc::ptr_eq(&wv.value_ref, value_ref))
                .map(|wv| wv.spec.name.clone())
                .unwrap_or_else(|| var_name.to_string());

            let current_value = value_ref.lock().unwrap();
            let watch_value = expr_value_to_watch_value(current_value.clone());
            let notification = crate::watch::WatchNotification::new_var(
                var_name.to_string(), // variable name
                channel_name,         // current channel name from WatchSpec
                watch_value,
                function_name.to_string(),
            );
            watch_handler.lock().unwrap().notify(notification);
            return Ok(());
        }
    }
    bail!("Variable '{}' not found for $watch.notify()", var_name)
}

enum EvalValue {
    Value(BamlValueWithMeta<ExprMetadata>),
    Reference(Arc<Mutex<BamlValueWithMeta<ExprMetadata>>>),
    Function(usize, Arc<Block<ExprMetadata>>, ExprMetadata),
}

#[derive(Debug)]
enum ControlFlow {
    Normal(BamlValueWithMeta<ExprMetadata>),
    Break,
    Continue,
    Return(BamlValueWithMeta<ExprMetadata>),
}

/// Check all @watch variables for changes and fire notifications
///
/// This function should only be called in the main execution context, not during
/// filter function evaluation to avoid infinite recursion.
#[allow(clippy::type_complexity)]
async fn check_watch_changes<F, Fut>(
    scopes: &mut Vec<Scope>,
    watch_notification_handler: &SharedWatchHandler,
    function_name: &str,
    thir: &THir<ExprMetadata>,
    run_llm_function: &mut F,
) where
    F: LlmHandler<Fut>,
    Fut: LlmFuture,
{
    // Skip watch checking if we're in a filter function evaluation context
    // This prevents infinite recursion when filter functions have local variables
    if scopes.iter().any(|scope| scope.is_filter_context) {
        return;
    }
    // Collect the variables to check and their current values
    // We do this first to avoid holding locks during async operations
    let mut checks: Vec<(
        String,
        crate::watch::WatchSpec,
        BamlValueWithMeta<ExprMetadata>,
        Option<BamlValue>,
        Option<BamlValue>,
    )> = Vec::new();

    for scope in scopes.iter() {
        for watch_var in &scope.watch_variables {
            let current_value = watch_var.value_ref.lock().unwrap().clone();
            let last_notified = watch_var.last_notified.lock().unwrap().clone();
            let last_checked = watch_var.last_checked.lock().unwrap().clone();

            checks.push((
                watch_var.name.clone(),
                watch_var.spec.clone(),
                current_value,
                last_notified,
                last_checked,
            ));
        }
    }

    // Process each check
    for (var_name, spec, current_value, last_notified, last_checked) in checks {
        let current_baml_value = current_value.clone().value();

        // Check if the value has changed since last check
        // This prevents calling the filter multiple times for the same value
        let value_changed_since_last_check = match last_checked.as_ref() {
            None => true,
            Some(last) => last != &current_baml_value,
        };

        if !value_changed_since_last_check {
            // Value hasn't changed since we last checked, skip
            continue;
        }

        // Update last_checked for this variable
        for scope in scopes.iter_mut() {
            for watch_var in &mut scope.watch_variables {
                if watch_var.name == var_name {
                    *watch_var.last_checked.lock().unwrap() = Some(current_baml_value.clone());
                    break;
                }
            }
        }

        // Determine if we should notify the watcher based on the when condition
        let should_notify = match &spec.when {
            crate::watch::WatchWhen::Manual => false, // Manual notification only.
            crate::watch::WatchWhen::Never => false,  // No notifications at all.
            crate::watch::WatchWhen::Auto => {
                // For WatchWhen::Auto, use built-in change detection.
                let has_changed = match last_notified.as_ref() {
                    None => false,                             // First time (declaration), don't notify.
                    Some(last) => last != &current_baml_value, // Compare values.
                };
                has_changed
            }
            crate::watch::WatchWhen::FunctionName(fn_name) => {
                // For filter functions, ALWAYS call the filter - it subsumes change detection
                // Evaluate the filter function
                log::debug!(
                    "Evaluating filter function '{fn_name}' for variable '{var_name}': current={current_baml_value:?}"
                );
                match evaluate_filter_function(
                    fn_name,
                    &current_baml_value,
                    scopes,
                    thir,
                    run_llm_function,
                    function_name,
                )
                .await
                {
                    Ok(result) => {
                        log::debug!("Filter function '{fn_name}' returned: {result}");
                        result
                    }
                    Err(e) => {
                        log::error!(
                            "Error evaluating filter function '{fn_name}' for variable '{var_name}': {e}"
                        );
                        false // Don't notify on error
                    }
                }
            }
        };

        if should_notify {
            // Update last notified value
            for scope in scopes.iter_mut() {
                for watch_var in &mut scope.watch_variables {
                    if watch_var.name == var_name {
                        *watch_var.last_notified.lock().unwrap() = Some(current_baml_value.clone());
                        break;
                    }
                }
            }

            // Fire the notification
            let watch_value = expr_value_to_watch_value(current_value);
            let notification = crate::watch::WatchNotification::new_var(
                var_name.clone(),  // variable name
                spec.name.clone(), // channel name
                watch_value,
                function_name.to_string(),
            );
            watch_notification_handler
                .lock()
                .unwrap()
                .notify(notification);
        }

        // Always update last_notified after checking (whether we notified or not)
        // This ensures that on first declaration (when last_notified is None), we record
        // the initial value so that subsequent changes will be detected
        for scope in scopes.iter_mut() {
            for watch_var in &mut scope.watch_variables {
                if watch_var.name == var_name {
                    *watch_var.last_notified.lock().unwrap() = Some(current_baml_value.clone());
                    break;
                }
            }
        }
    }
}

/// Evaluate a filter function for watch
/// The filter function takes (current_value) -> bool
async fn evaluate_filter_function<F, Fut>(
    fn_name: &internal_baml_ast::ast::Identifier,
    current_value: &BamlValue,
    scopes: &mut Vec<Scope>,
    thir: &THir<ExprMetadata>,
    run_llm_function: &mut F,
    function_name: &str,
) -> Result<bool>
where
    F: LlmHandler<Fut>,
    Fut: LlmFuture,
{
    // Look up the filter function
    let filter_func = thir
        .expr_functions
        .iter()
        .find(|f| f.name == fn_name.to_string())
        .with_context(|| format!("Filter function '{fn_name}' not found"))?;

    // Check arity
    if filter_func.parameters.len() != 1 {
        bail!(
            "Filter function '{}' must take exactly 1 parameter (current value)",
            fn_name
        );
    }

    // Convert BamlValue to BamlValueWithMeta
    log::debug!("Filter function current_value: {current_value:?}");
    let value_with_meta = baml_value_to_value_with_meta(current_value.clone());

    // Create a new scope with the function parameter
    // Mark this as a filter context to prevent infinite recursion
    scopes.push(Scope {
        variables: [(
            filter_func.parameters[0].name.clone(),
            Arc::new(Mutex::new(value_with_meta)),
        )]
        .into_iter()
        .collect(),
        watch_variables: Vec::new(),
        is_filter_context: true,
    });

    // Create a no-op watch handler for the filter function evaluation
    // Filter functions shouldn't send their own notifications
    let noop_watch_handler = crate::watch::shared_noop_handler();

    // Evaluate the function body
    let result = evaluate_block(
        &filter_func.body,
        scopes,
        thir,
        run_llm_function,
        &noop_watch_handler,
        function_name,
    )
    .await?;

    scopes.pop();

    // Extract boolean result
    match result {
        BamlValueWithMeta::Bool(b, _) => Ok(b),
        _ => bail!(
            "Filter function '{}' must return a boolean, got {:?}",
            fn_name,
            result
        ),
    }
}

/// Convert BamlValue to BamlValueWithMeta (with fake metadata)
fn baml_value_to_value_with_meta(value: BamlValue) -> BamlValueWithMeta<ExprMetadata> {
    let meta = (Span::fake(), None);
    match value {
        BamlValue::String(s) => BamlValueWithMeta::String(s, meta),
        BamlValue::Int(i) => BamlValueWithMeta::Int(i, meta),
        BamlValue::Float(f) => BamlValueWithMeta::Float(f, meta),
        BamlValue::Bool(b) => BamlValueWithMeta::Bool(b, meta),
        BamlValue::Map(m) => {
            let converted = m
                .into_iter()
                .map(|(k, v)| (k, baml_value_to_value_with_meta(v)))
                .collect();
            BamlValueWithMeta::Map(converted, meta)
        }
        BamlValue::List(l) => {
            let converted = l.into_iter().map(baml_value_to_value_with_meta).collect();
            BamlValueWithMeta::List(converted, meta)
        }
        BamlValue::Media(m) => BamlValueWithMeta::Media(m, meta),
        BamlValue::Enum(name, val) => BamlValueWithMeta::Enum(name, val, meta),
        BamlValue::Class(name, fields) => {
            let converted = fields
                .into_iter()
                .map(|(k, v)| (k, baml_value_to_value_with_meta(v)))
                .collect();
            BamlValueWithMeta::Class(name, converted, meta)
        }
        BamlValue::Null => BamlValueWithMeta::Null(meta),
    }
}

pub async fn interpret_thir<F, Fut>(
    function_name: String,
    thir: THir<ExprMetadata>,
    expr: Expr<ExprMetadata>,
    mut run_llm_function: F,
    watch_notification_handler: SharedWatchHandler,
    extra_bindings: BamlMap<String, BamlValueWithMeta<ExprMetadata>>,
    env_vars: HashMap<String, String>,
) -> Result<BamlValueWithMeta<ExprMetadata>>
where
    F: LlmHandler<Fut>,
    Fut: LlmFuture,
{
    let env_vars_map = env_vars;
    let mut scopes = vec![Scope {
        variables: BamlMap::from_iter(
            extra_bindings
                .into_iter()
                .map(|(k, v)| (k, Arc::new(Mutex::new(v)))),
        ),
        watch_variables: Vec::new(),
        is_filter_context: false,
    }];

    let mut env_entries = BamlMap::new();
    for (key, value) in env_vars_map {
        env_entries.insert(
            key,
            BamlValueWithMeta::String(value, (internal_baml_diagnostics::Span::fake(), None)),
        );
    }
    scopes[0].variables.insert(
        "__env_vars__".to_string(),
        Arc::new(Mutex::new(BamlValueWithMeta::Map(
            env_entries,
            (Span::fake(), None),
        ))),
    );

    // Seed scope with global assignments
    for (name, g) in thir.global_assignments.iter() {
        let v = expect_value(
            evaluate_expr(
                &g.expr,
                &mut scopes,
                &thir,
                &mut run_llm_function,
                &watch_notification_handler,
                &function_name,
            )
            .await?,
        )?;
        declare(&mut scopes, name, v);
    }

    // Evaluate provided expression
    let result = expect_value(
        evaluate_expr(
            &expr,
            &mut scopes,
            &thir,
            &mut run_llm_function,
            &watch_notification_handler,
            &function_name,
        )
        .await?,
    )?;
    Ok(result)
}

fn evaluate_block_with_control_flow<'a, F, Fut>(
    block: &'a Block<ExprMetadata>,
    scopes: &'a mut Vec<Scope>,
    thir: &'a THir<ExprMetadata>,
    run_llm_function: &'a mut F,
    watch_handler: &'a SharedWatchHandler,
    function_name: &'a str,
) -> BoxFuture<'a, Result<ControlFlow>>
where
    F: LlmHandler<Fut>,
    Fut: LlmFuture,
{
    Box::pin(async move {
        scopes.push(Scope {
            variables: BamlMap::new(),
            watch_variables: Vec::new(),
            is_filter_context: false,
        });

        // Check if we should treat the last statement as the implicit return value
        let use_last_expr_as_return = block.trailing_expr.is_none()
            && matches!(block.statements.last(), Some(Statement::Expression { .. }));

        let statements_to_execute = if use_last_expr_as_return {
            block.statements.len().saturating_sub(1)
        } else {
            block.statements.len()
        };

        fn handle_statement<'a, F, Fut>(
            stmt: &'a Statement<ExprMetadata>,
            scopes: &'a mut Vec<Scope>,
            thir: &'a THir<ExprMetadata>,
            run_llm_function: &'a mut F,
            watch_handler: &'a SharedWatchHandler,
            function_name: &'a str,
        ) -> BoxFuture<'a, Result<Option<ControlFlow>>>
        where
            F: LlmHandler<Fut>,
            Fut: LlmFuture,
        {
            Box::pin(async move {
                match stmt {
                    Statement::HeaderContextEnter(_header) => return Ok(None),
                    Statement::Let {
                        name, value, watch, ..
                    } => {
                        match evaluate_expr(
                            value,
                            scopes,
                            thir,
                            run_llm_function,
                            watch_handler,
                            function_name,
                        )
                        .await?
                        {
                            EvalValue::Value(v) => {
                                declare(scopes, name, v);
                                // Register watch tracking if @watch is present
                                if let Some(watch_spec) = watch {
                                    if let Some(var_ref) = lookup_variable(scopes, name) {
                                        register_watch_variable(
                                            scopes,
                                            name,
                                            var_ref,
                                            watch_spec.clone(),
                                        );
                                    }
                                }
                            }
                            EvalValue::Reference(cell) => {
                                declare_with_cell(scopes, name, cell.clone());
                                // Register watch tracking if @watch is present
                                if let Some(emit_spec) = watch {
                                    register_watch_variable(scopes, name, cell, emit_spec.clone());
                                }
                            }
                            EvalValue::Function(_, _, _) => {
                                bail!("cannot assign function to variable `{}`", name);
                            }
                        }
                        // Check for changes in all watch variables after the let statement
                        check_watch_changes(
                            scopes,
                            watch_handler,
                            function_name,
                            thir,
                            run_llm_function,
                        )
                        .await;
                    }
                    Statement::Declare { name, span } => {
                        declare(scopes, name, BamlValueWithMeta::Null((span.clone(), None)));
                    }
                    Statement::Assign { left, value } => {
                        let assigned_value = expect_value(
                            evaluate_expr(
                                value,
                                scopes,
                                thir,
                                run_llm_function,
                                watch_handler,
                                function_name,
                            )
                            .await?,
                        )?;
                        assign_to_expr(
                            left,
                            assigned_value,
                            scopes,
                            thir,
                            run_llm_function,
                            watch_handler,
                            function_name,
                        )
                        .await?;
                        // Check for changes in watch variables after assignment
                        check_watch_changes(
                            scopes,
                            watch_handler,
                            function_name,
                            thir,
                            run_llm_function,
                        )
                        .await;
                    }
                    Statement::DeclareAndAssign {
                        name, value, watch, ..
                    } => {
                        // Create watch context if @watch is present
                        let watch_ctx = watch.as_ref().map(|_| {
                            use std::time::{SystemTime, UNIX_EPOCH};
                            let timestamp = SystemTime::now()
                                .duration_since(UNIX_EPOCH)
                                .unwrap()
                                .as_millis();
                            WatchStreamContext {
                                variable_name: name.clone(),
                                stream_id: format!("{function_name}_{name}_{timestamp}"),
                            }
                        });

                        match evaluate_expr_with_context(
                            value,
                            scopes,
                            thir,
                            run_llm_function,
                            watch_handler,
                            function_name,
                            watch_ctx.as_ref(),
                        )
                        .await?
                        {
                            EvalValue::Value(v) => {
                                declare(scopes, name, v);
                                // Register watch tracking if @watch is present
                                if let Some(watch_spec) = watch {
                                    if let Some(var_ref) = lookup_variable(scopes, name) {
                                        register_watch_variable(
                                            scopes,
                                            name,
                                            var_ref,
                                            watch_spec.clone(),
                                        );
                                    }
                                }
                            }
                            EvalValue::Reference(cell) => {
                                declare_with_cell(scopes, name, cell.clone());
                                // Register watch tracking if @watch is present
                                if let Some(emit_spec) = watch {
                                    register_watch_variable(scopes, name, cell, emit_spec.clone());
                                }
                            }
                            EvalValue::Function(_, _, _) => {
                                bail!("cannot assign function to variable `{}`", name);
                            }
                        }
                        // Check for changes in all watch variables after the declare and assign
                        check_watch_changes(
                            scopes,
                            watch_handler,
                            function_name,
                            thir,
                            run_llm_function,
                        )
                        .await;
                    }
                    Statement::Return { expr, .. } => {
                        let v = expect_value(
                            evaluate_expr(
                                expr,
                                scopes,
                                thir,
                                run_llm_function,
                                watch_handler,
                                function_name,
                            )
                            .await?,
                        )?;
                        scopes.pop();
                        return Ok(Some(ControlFlow::Return(v)));
                    }
                    Statement::Expression { expr, .. } => {
                        // For expression statements, we still need to evaluate them for side effects
                        // (and the last one might be the implicit return value)
                        let _ = evaluate_expr(
                            expr,
                            scopes,
                            thir,
                            run_llm_function,
                            watch_handler,
                            function_name,
                        )
                        .await?;
                    }
                    Statement::Break(_) => {
                        scopes.pop();
                        return Ok(Some(ControlFlow::Break));
                    }
                    Statement::Continue(_) => {
                        scopes.pop();
                        return Ok(Some(ControlFlow::Continue));
                    }
                    Statement::While {
                        condition, block, ..
                    } => loop {
                        let cond_val = expect_value(
                            evaluate_expr(
                                condition,
                                scopes,
                                thir,
                                run_llm_function,
                                watch_handler,
                                function_name,
                            )
                            .await?,
                        )?;
                        match cond_val {
                            BamlValueWithMeta::Bool(true, _) => {
                                match evaluate_block_with_control_flow(
                                    block,
                                    scopes,
                                    thir,
                                    run_llm_function,
                                    watch_handler,
                                    function_name,
                                )
                                .await?
                                {
                                    ControlFlow::Break => break,
                                    ControlFlow::Continue => continue,
                                    ControlFlow::Normal(_) => {}
                                    ControlFlow::Return(val) => {
                                        scopes.pop();
                                        return Ok(Some(ControlFlow::Return(val)));
                                    }
                                }
                            }
                            BamlValueWithMeta::Bool(false, _) => break,
                            _ => bail!("while condition must be boolean"),
                        }
                    },
                    Statement::ForLoop {
                        identifier,
                        iterator,
                        block,
                        ..
                    } => {
                        let iterable_val = expect_value(
                            evaluate_expr(
                                iterator,
                                scopes,
                                thir,
                                run_llm_function,
                                watch_handler,
                                function_name,
                            )
                            .await?,
                        )?;
                        match iterable_val {
                            BamlValueWithMeta::List(items, _) => {
                                for item_val in items.iter() {
                                    // Create new scope for loop iteration
                                    scopes.push(Scope {
                                        variables: BamlMap::new(),
                                        watch_variables: Vec::new(),
                                        is_filter_context: false,
                                    });
                                    declare(scopes, identifier, item_val.clone());

                                    match evaluate_block_with_control_flow(
                                        block,
                                        scopes,
                                        thir,
                                        run_llm_function,
                                        watch_handler,
                                        function_name,
                                    )
                                    .await?
                                    {
                                        ControlFlow::Break => {
                                            scopes.pop();
                                            return Ok(Some(ControlFlow::Break));
                                        }
                                        ControlFlow::Continue => {
                                            scopes.pop();
                                            continue;
                                        }
                                        ControlFlow::Normal(_) => {
                                            scopes.pop();
                                        }
                                        ControlFlow::Return(val) => {
                                            scopes.pop();
                                            scopes.pop();
                                            return Ok(Some(ControlFlow::Return(val)));
                                        }
                                    }
                                }
                            }
                            _ => bail!("for loop requires iterable (list)"),
                        }
                    }
                    Statement::AssignOp {
                        left,
                        value,
                        assign_op,
                        ..
                    } => {
                        use crate::hir::AssignOp;

                        let current_val = expect_value(
                            evaluate_expr(
                                left,
                                scopes,
                                thir,
                                run_llm_function,
                                watch_handler,
                                function_name,
                            )
                            .await?,
                        )?;

                        // Evaluate the right-hand side expression
                        let rhs_val = expect_value(
                            evaluate_expr(
                                value,
                                scopes,
                                thir,
                                run_llm_function,
                                watch_handler,
                                function_name,
                            )
                            .await?,
                        )?;

                        // Perform the compound assignment operation
                        let result_val = match assign_op {
                            AssignOp::AddAssign => match (current_val.clone(), rhs_val.clone()) {
                                (BamlValueWithMeta::Int(a, meta), BamlValueWithMeta::Int(b, _)) => {
                                    BamlValueWithMeta::Int(a + b, meta)
                                }
                                (
                                    BamlValueWithMeta::Float(a, meta),
                                    BamlValueWithMeta::Float(b, _),
                                ) => BamlValueWithMeta::Float(a + b, meta),
                                (
                                    BamlValueWithMeta::Int(a, meta),
                                    BamlValueWithMeta::Float(b, _),
                                ) => BamlValueWithMeta::Float(a as f64 + b, meta),
                                (
                                    BamlValueWithMeta::Float(a, meta),
                                    BamlValueWithMeta::Int(b, _),
                                ) => BamlValueWithMeta::Float(a + (b as f64), meta),
                                (
                                    BamlValueWithMeta::String(a, meta),
                                    BamlValueWithMeta::String(b, _),
                                ) => BamlValueWithMeta::String(format!("{a}{b}"), meta),
                                _ => bail!("unsupported types for += operator"),
                            },
                            AssignOp::SubAssign => match (current_val.clone(), rhs_val.clone()) {
                                (BamlValueWithMeta::Int(a, meta), BamlValueWithMeta::Int(b, _)) => {
                                    BamlValueWithMeta::Int(a - b, meta)
                                }
                                (
                                    BamlValueWithMeta::Float(a, meta),
                                    BamlValueWithMeta::Float(b, _),
                                ) => BamlValueWithMeta::Float(a - b, meta),
                                (
                                    BamlValueWithMeta::Int(a, meta),
                                    BamlValueWithMeta::Float(b, _),
                                ) => BamlValueWithMeta::Float((a as f64) - b, meta),
                                (
                                    BamlValueWithMeta::Float(a, meta),
                                    BamlValueWithMeta::Int(b, _),
                                ) => BamlValueWithMeta::Float(a - (b as f64), meta),
                                _ => bail!("unsupported types for -= operator"),
                            },
                            AssignOp::MulAssign => match (current_val.clone(), rhs_val.clone()) {
                                (BamlValueWithMeta::Int(a, meta), BamlValueWithMeta::Int(b, _)) => {
                                    BamlValueWithMeta::Int(a * b, meta)
                                }
                                (
                                    BamlValueWithMeta::Float(a, meta),
                                    BamlValueWithMeta::Float(b, _),
                                ) => BamlValueWithMeta::Float(a * b, meta),
                                (
                                    BamlValueWithMeta::Int(a, meta),
                                    BamlValueWithMeta::Float(b, _),
                                ) => BamlValueWithMeta::Float((a as f64) * b, meta),
                                (
                                    BamlValueWithMeta::Float(a, meta),
                                    BamlValueWithMeta::Int(b, _),
                                ) => BamlValueWithMeta::Float(a * (b as f64), meta),
                                _ => bail!("unsupported types for *= operator"),
                            },
                            AssignOp::DivAssign => match (current_val.clone(), rhs_val.clone()) {
                                (BamlValueWithMeta::Int(a, meta), BamlValueWithMeta::Int(b, _)) => {
                                    if b == 0 {
                                        bail!("division by zero in /= operator");
                                    }
                                    BamlValueWithMeta::Float((a as f64) / (b as f64), meta)
                                }
                                (
                                    BamlValueWithMeta::Float(a, meta),
                                    BamlValueWithMeta::Float(b, _),
                                ) => {
                                    if b == 0.0 {
                                        bail!("division by zero in /= operator");
                                    }
                                    BamlValueWithMeta::Float(a / b, meta)
                                }
                                (
                                    BamlValueWithMeta::Int(a, meta),
                                    BamlValueWithMeta::Float(b, _),
                                ) => {
                                    if b == 0.0 {
                                        bail!("division by zero in /= operator");
                                    }
                                    BamlValueWithMeta::Float((a as f64) / b, meta)
                                }
                                (
                                    BamlValueWithMeta::Float(a, meta),
                                    BamlValueWithMeta::Int(b, _),
                                ) => {
                                    if b == 0 {
                                        bail!("division by zero in /= operator");
                                    }
                                    BamlValueWithMeta::Float(a / (b as f64), meta)
                                }
                                _ => bail!("unsupported types for /= operator"),
                            },
                            AssignOp::ModAssign => match (current_val.clone(), rhs_val.clone()) {
                                (BamlValueWithMeta::Int(a, meta), BamlValueWithMeta::Int(b, _)) => {
                                    if b == 0 {
                                        bail!("modulo by zero in %= operator");
                                    }
                                    BamlValueWithMeta::Int(a % b, meta)
                                }
                                _ => bail!("unsupported types for %= operator"),
                            },
                            AssignOp::BitXorAssign => {
                                match (current_val.clone(), rhs_val.clone()) {
                                    (
                                        BamlValueWithMeta::Int(a, meta),
                                        BamlValueWithMeta::Int(b, _),
                                    ) => BamlValueWithMeta::Int(a ^ b, meta),
                                    _ => bail!("bitwise ^= requires integer operands"),
                                }
                            }
                            AssignOp::BitAndAssign => {
                                match (current_val.clone(), rhs_val.clone()) {
                                    (
                                        BamlValueWithMeta::Int(a, meta),
                                        BamlValueWithMeta::Int(b, _),
                                    ) => BamlValueWithMeta::Int(a & b, meta),
                                    _ => bail!("bitwise &= requires integer operands"),
                                }
                            }
                            AssignOp::BitOrAssign => match (current_val.clone(), rhs_val.clone()) {
                                (BamlValueWithMeta::Int(a, meta), BamlValueWithMeta::Int(b, _)) => {
                                    BamlValueWithMeta::Int(a | b, meta)
                                }
                                _ => bail!("bitwise |= requires integer operands"),
                            },
                            AssignOp::ShlAssign => match (current_val.clone(), rhs_val.clone()) {
                                (BamlValueWithMeta::Int(a, meta), BamlValueWithMeta::Int(b, _)) => {
                                    if b < 0 {
                                        bail!("negative shift amount in <<= operator");
                                    }
                                    BamlValueWithMeta::Int(a << b, meta)
                                }
                                _ => bail!("shift <<= requires integer operands"),
                            },
                            AssignOp::ShrAssign => match (current_val.clone(), rhs_val.clone()) {
                                (BamlValueWithMeta::Int(a, meta), BamlValueWithMeta::Int(b, _)) => {
                                    if b < 0 {
                                        bail!("negative shift amount in >>= operator");
                                    }
                                    BamlValueWithMeta::Int(a >> b, meta)
                                }
                                _ => bail!("shift >>= requires integer operands"),
                            },
                        };

                        // Assign the result back to the target expression
                        assign_to_expr(
                            left,
                            result_val,
                            scopes,
                            thir,
                            run_llm_function,
                            watch_handler,
                            function_name,
                        )
                        .await?;
                        // Check for changes in watch variables after compound assignment
                        check_watch_changes(
                            scopes,
                            watch_handler,
                            function_name,
                            thir,
                            run_llm_function,
                        )
                        .await;
                    }
                    Statement::SemicolonExpression { expr, .. } => {
                        let _ = evaluate_expr(
                            expr,
                            scopes,
                            thir,
                            run_llm_function,
                            watch_handler,
                            function_name,
                        )
                        .await?;
                    }
                    Statement::CForLoop {
                        condition,
                        after,
                        block,
                    } => {
                        loop {
                            // Check condition (if present)
                            if let Some(cond_expr) = condition {
                                let cond_val = expect_value(
                                    evaluate_expr(
                                        cond_expr,
                                        scopes,
                                        thir,
                                        run_llm_function,
                                        watch_handler,
                                        function_name,
                                    )
                                    .await?,
                                )?;
                                match cond_val {
                                    BamlValueWithMeta::Bool(false, _) => break,
                                    BamlValueWithMeta::Bool(true, _) => {}
                                    _ => bail!("C-style for loop condition must be boolean"),
                                }
                            }

                            // Execute loop body
                            match evaluate_block_with_control_flow(
                                block,
                                scopes,
                                thir,
                                run_llm_function,
                                watch_handler,
                                function_name,
                            )
                            .await?
                            {
                                ControlFlow::Break => break,
                                ControlFlow::Continue => {
                                    // Execute after statement if present
                                    if let Some(after_stmt) = after {
                                        // Execute the after statement in the current scope context
                                        match after_stmt.as_ref() {
                                            Statement::AssignOp {
                                                left,
                                                value,
                                                assign_op,
                                                ..
                                            } => {
                                                use crate::hir::AssignOp;

                                                let current_val = expect_value(
                                                    evaluate_expr(
                                                        left,
                                                        scopes,
                                                        thir,
                                                        run_llm_function,
                                                        watch_handler,
                                                        function_name,
                                                    )
                                                    .await?,
                                                )?;
                                                let rhs_val = expect_value(
                                                    evaluate_expr(
                                                        value,
                                                        scopes,
                                                        thir,
                                                        run_llm_function,
                                                        watch_handler,
                                                        function_name,
                                                    )
                                                    .await?,
                                                )?;

                                                let result_val = match assign_op {
                                                    AssignOp::AddAssign => {
                                                        match (current_val.clone(), rhs_val.clone()) {
                                                    (
                                                        BamlValueWithMeta::Int(a, meta),
                                                        BamlValueWithMeta::Int(b, _),
                                                    ) => BamlValueWithMeta::Int(a + b, meta),
                                                    _ => bail!(
                                                        "unsupported types for += in C-for after clause"
                                                    ),
                                                }
                                                    }
                                                    _ => bail!(
                                                    "unsupported assign op in C-for after clause"
                                                ),
                                                };
                                                assign_to_expr(
                                                    left,
                                                    result_val,
                                                    scopes,
                                                    thir,
                                                    run_llm_function,
                                                    watch_handler,
                                                    function_name,
                                                )
                                                .await?;
                                            }
                                            Statement::Assign { left, value } => {
                                                let v = expect_value(
                                                    evaluate_expr(
                                                        value,
                                                        scopes,
                                                        thir,
                                                        run_llm_function,
                                                        watch_handler,
                                                        function_name,
                                                    )
                                                    .await?,
                                                )?;
                                                assign_to_expr(
                                                    left,
                                                    v,
                                                    scopes,
                                                    thir,
                                                    run_llm_function,
                                                    watch_handler,
                                                    function_name,
                                                )
                                                .await?;
                                            }
                                            _ => bail!(
                                                "unsupported statement type in C-for after clause"
                                            ),
                                        }
                                    }
                                    continue;
                                }
                                ControlFlow::Normal(_) => {
                                    // Execute after statement if present
                                    if let Some(after_stmt) = after {
                                        // Execute the after statement in the current scope context
                                        match after_stmt.as_ref() {
                                            Statement::AssignOp {
                                                left,
                                                value,
                                                assign_op,
                                                ..
                                            } => {
                                                use crate::hir::AssignOp;

                                                let current_val = expect_value(
                                                    evaluate_expr(
                                                        left,
                                                        scopes,
                                                        thir,
                                                        run_llm_function,
                                                        watch_handler,
                                                        function_name,
                                                    )
                                                    .await?,
                                                )?;
                                                let rhs_val = expect_value(
                                                    evaluate_expr(
                                                        value,
                                                        scopes,
                                                        thir,
                                                        run_llm_function,
                                                        watch_handler,
                                                        function_name,
                                                    )
                                                    .await?,
                                                )?;

                                                let result_val = match assign_op {
                                                    AssignOp::AddAssign => {
                                                        match (current_val.clone(), rhs_val.clone()) {
                                                    (
                                                        BamlValueWithMeta::Int(a, meta),
                                                        BamlValueWithMeta::Int(b, _),
                                                    ) => BamlValueWithMeta::Int(a + b, meta),
                                                    _ => bail!(
                                                        "unsupported types for += in C-for after clause"
                                                    ),
                                                }
                                                    }
                                                    _ => bail!(
                                                    "unsupported assign op in C-for after clause"
                                                ),
                                                };
                                                assign_to_expr(
                                                    left,
                                                    result_val,
                                                    scopes,
                                                    thir,
                                                    run_llm_function,
                                                    watch_handler,
                                                    function_name,
                                                )
                                                .await?;
                                            }
                                            Statement::Assign { left, value } => {
                                                let v = expect_value(
                                                    evaluate_expr(
                                                        value,
                                                        scopes,
                                                        thir,
                                                        run_llm_function,
                                                        watch_handler,
                                                        function_name,
                                                    )
                                                    .await?,
                                                )?;
                                                assign_to_expr(
                                                    left,
                                                    v,
                                                    scopes,
                                                    thir,
                                                    run_llm_function,
                                                    watch_handler,
                                                    function_name,
                                                )
                                                .await?;
                                            }
                                            _ => bail!(
                                                "unsupported statement type in C-for after clause"
                                            ),
                                        }
                                    }
                                }
                                ControlFlow::Return(val) => {
                                    scopes.pop();
                                    return Ok(Some(ControlFlow::Return(val)));
                                }
                            }
                        }
                    }
                    Statement::Assert { condition, .. } => {
                        let cond_val = expect_value(
                            evaluate_expr(
                                condition,
                                scopes,
                                thir,
                                run_llm_function,
                                watch_handler,
                                function_name,
                            )
                            .await?,
                        )?;
                        match cond_val {
                            BamlValueWithMeta::Bool(true, _) => {}
                            BamlValueWithMeta::Bool(false, _) => bail!("assertion failed"),
                            _ => bail!("assert condition must be boolean"),
                        }
                    }
                    Statement::WatchOptions {
                        variable,
                        channel,
                        when,
                        span,
                    } => {
                        // Find and update the watch variable for this variable
                        // We need to find the watch variable by checking which one references the same value
                        for scope in scopes.iter_mut().rev() {
                            if let Some(var_ref) = scope.variables.get(variable) {
                                // Find the watch variable that references this variable
                                if let Some(watch_var) = scope
                                    .watch_variables
                                    .iter_mut()
                                    .find(|wv| Arc::ptr_eq(&wv.value_ref, var_ref))
                                {
                                    // Update the channel name if provided
                                    if let Some(new_channel) = channel {
                                        watch_var.spec.name = new_channel.clone();
                                    }

                                    // Update the when condition if provided
                                    if let Some(when) = when {
                                        watch_var.spec.when = when.clone();
                                    }

                                    watch_var.spec.span = span.clone();
                                    break;
                                }
                            }
                        }
                    }
                    Statement::WatchNotify { variable, .. } => {
                        // Manually trigger a watch notification for this variable
                        fire_watch_notification_for_variable(
                            scopes,
                            variable,
                            watch_handler,
                            function_name,
                        )?;
                    }
                }
                Ok(None)
            })
        }

        for stmt in block.statements.iter().take(statements_to_execute) {
            let result = handle_statement(
                stmt,
                scopes,
                thir,
                run_llm_function,
                watch_handler,
                function_name,
            )
            .await?;

            if let Some(control_flow) = result {
                return Ok(control_flow);
            }
        }

        // Compute the return value
        let ret = if let Some(trailing_expr) = &block.trailing_expr {
            // Explicit trailing expression
            expect_value(
                evaluate_expr(
                    trailing_expr,
                    scopes,
                    thir,
                    run_llm_function,
                    watch_handler,
                    function_name,
                )
                .await?,
            )?
        } else if use_last_expr_as_return {
            // No explicit trailing expression, but last statement is an expression statement,
            // so use that as the implicit return value (handles cases like if-else at the end of a block)
            if let Some(Statement::Expression { expr, .. }) = block.statements.last() {
                expect_value(
                    evaluate_expr(
                        expr,
                        scopes,
                        thir,
                        run_llm_function,
                        watch_handler,
                        function_name,
                    )
                    .await?,
                )?
            } else {
                unreachable!("use_last_expr_as_return is true but last statement is not Expression")
            }
        } else {
            // No trailing expression and last statement is not an expression, return null
            BamlValueWithMeta::Null((internal_baml_diagnostics::Span::fake(), None))
        };
        scopes.pop();
        Ok(ControlFlow::Normal(ret))
    })
}

async fn evaluate_block<F, Fut>(
    block: &Block<ExprMetadata>,
    scopes: &mut Vec<Scope>,
    thir: &THir<ExprMetadata>,
    run_llm_function: &mut F,
    watch_handler: &SharedWatchHandler,
    function_name: &str,
) -> Result<BamlValueWithMeta<ExprMetadata>>
where
    F: LlmHandler<Fut>,
    Fut: LlmFuture,
{
    match evaluate_block_with_control_flow(
        block,
        scopes,
        thir,
        run_llm_function,
        watch_handler,
        function_name,
    )
    .await?
    {
        ControlFlow::Normal(val) => Ok(val),
        ControlFlow::Return(val) => Ok(val),
        ControlFlow::Break => bail!("break statement not in loop context"),
        ControlFlow::Continue => bail!("continue statement not in loop context"),
    }
}

fn declare(scopes: &mut [Scope], name: &str, value: BamlValueWithMeta<ExprMetadata>) {
    if let Some(scope) = scopes.last_mut() {
        scope
            .variables
            .insert(name.to_string(), Arc::new(Mutex::new(value)));
    }
}

fn declare_with_cell(
    scopes: &mut [Scope],
    name: &str,
    cell: Arc<Mutex<BamlValueWithMeta<ExprMetadata>>>,
) {
    if let Some(scope) = scopes.last_mut() {
        scope.variables.insert(name.to_string(), cell);
    }
}

fn assign(scopes: &mut [Scope], name: &str, value: BamlValueWithMeta<ExprMetadata>) -> Result<()> {
    for s in scopes.iter_mut().rev() {
        if let Some(cell) = s.variables.get_mut(name) {
            *cell.lock().unwrap() = value;
            return Ok(());
        }
    }
    bail!("assign to undeclared variable `{}`", name)
}

async fn assign_to_expr<F, Fut>(
    target: &Expr<ExprMetadata>,
    new_value: BamlValueWithMeta<ExprMetadata>,
    scopes: &mut Vec<Scope>,
    thir: &THir<ExprMetadata>,
    run_llm_function: &mut F,
    watch_handler: &SharedWatchHandler,
    function_name: &str,
) -> Result<()>
where
    F: LlmHandler<Fut>,
    Fut: LlmFuture,
{
    let mut current_expr = target;
    let mut value_to_assign = new_value;

    loop {
        match current_expr {
            Expr::Var(name, _) => return assign(scopes, name, value_to_assign),
            Expr::FieldAccess { base, field, .. } => {
                let mut base_value = expect_value(
                    evaluate_expr(
                        base,
                        scopes,
                        thir,
                        run_llm_function,
                        watch_handler,
                        function_name,
                    )
                    .await?,
                )?;

                match &mut base_value {
                    BamlValueWithMeta::Class(_, fields, _) => {
                        let entry = fields
                            .get_mut(field)
                            .with_context(|| format!("field `{field}` not found for assignment"))?;
                        *entry = value_to_assign.clone();
                    }
                    BamlValueWithMeta::Map(fields, _) => {
                        let entry = fields
                            .get_mut(field)
                            .with_context(|| format!("field `{field}` not found for assignment"))?;
                        *entry = value_to_assign.clone();
                    }
                    _ => bail!("field assignment on non-map/class"),
                }

                value_to_assign = base_value;
                current_expr = base.as_ref();
            }
            Expr::ArrayAccess { base, index, meta } => {
                let mut base_value = expect_value(
                    evaluate_expr(
                        base,
                        scopes,
                        thir,
                        run_llm_function,
                        watch_handler,
                        function_name,
                    )
                    .await?,
                )?;
                let index_value = expect_value(
                    evaluate_expr(
                        index,
                        scopes,
                        thir,
                        run_llm_function,
                        watch_handler,
                        function_name,
                    )
                    .await?,
                )?;

                let idx = match index_value {
                    BamlValueWithMeta::Int(i, _) if i >= 0 => i as usize,
                    _ => bail!(
                        "array assignment requires a non-negative integer index at {:?}",
                        meta.0
                    ),
                };

                match &mut base_value {
                    BamlValueWithMeta::List(items, _) => {
                        if idx >= items.len() {
                            bail!("array assignment index out of bounds");
                        }
                        items[idx] = value_to_assign.clone();
                    }
                    _ => bail!("array assignment on non-list value at {:?}", meta.0),
                }

                value_to_assign = base_value;
                current_expr = base.as_ref();
            }
            _ => return Err(anyhow!("Complex assignment targets not yet supported")),
        }
    }
}

fn lookup(scopes: &[Scope], name: &str) -> Option<BamlValueWithMeta<ExprMetadata>> {
    for s in scopes.iter().rev() {
        if let Some(cell) = s.variables.get(name) {
            return Some(cell.lock().unwrap().clone());
        }
    }
    None
}

fn lookup_cell(
    scopes: &[Scope],
    name: &str,
) -> Option<Arc<Mutex<BamlValueWithMeta<ExprMetadata>>>> {
    for s in scopes.iter().rev() {
        if let Some(cell) = s.variables.get(name) {
            return Some(cell.clone());
        }
    }
    None
}

/// Alias for lookup_cell - looks up a variable and returns its Arc<Mutex<>>
fn lookup_variable(
    scopes: &[Scope],
    name: &str,
) -> Option<Arc<Mutex<BamlValueWithMeta<ExprMetadata>>>> {
    lookup_cell(scopes, name)
}

/// Convert BamlValueWithMeta to BamlValue by stripping metadata
fn baml_value_with_meta_to_baml_value(value: BamlValueWithMeta<ExprMetadata>) -> BamlValue {
    match value {
        BamlValueWithMeta::String(s, _) => BamlValue::String(s),
        BamlValueWithMeta::Int(i, _) => BamlValue::Int(i),
        BamlValueWithMeta::Float(f, _) => BamlValue::Float(f),
        BamlValueWithMeta::Bool(b, _) => BamlValue::Bool(b),
        BamlValueWithMeta::Map(m, _) => {
            let converted_map = m
                .into_iter()
                .map(|(k, v)| (k, baml_value_with_meta_to_baml_value(v)))
                .collect();
            BamlValue::Map(converted_map)
        }
        BamlValueWithMeta::List(l, _) => {
            let converted_list = l
                .into_iter()
                .map(baml_value_with_meta_to_baml_value)
                .collect();
            BamlValue::List(converted_list)
        }
        BamlValueWithMeta::Media(m, _) => BamlValue::Media(m),
        BamlValueWithMeta::Enum(name, val, _) => BamlValue::Enum(name, val),
        BamlValueWithMeta::Class(name, fields, _) => {
            let converted_fields = fields
                .into_iter()
                .map(|(k, v)| (k, baml_value_with_meta_to_baml_value(v)))
                .collect();
            BamlValue::Class(name, converted_fields)
        }
        BamlValueWithMeta::Null(_) => BamlValue::Null,
    }
}

// Helper wrapper that calls evaluate_expr_with_context with None context
fn evaluate_expr<'a, F, Fut>(
    expr: &'a Expr<ExprMetadata>,
    scopes: &'a mut Vec<Scope>,
    thir: &'a THir<ExprMetadata>,
    run_llm_function: &'a mut F,
    watch_handler: &'a SharedWatchHandler,
    function_name: &'a str,
) -> BoxFuture<'a, Result<EvalValue>>
where
    F: LlmHandler<Fut>,
    Fut: LlmFuture,
{
    evaluate_expr_with_context(
        expr,
        scopes,
        thir,
        run_llm_function,
        watch_handler,
        function_name,
        None,
    )
}

// Internal function that accepts optional emit context
fn evaluate_expr_with_context<'a, F, Fut>(
    expr: &'a Expr<ExprMetadata>,
    scopes: &'a mut Vec<Scope>,
    thir: &'a THir<ExprMetadata>,
    run_llm_function: &'a mut F,
    watch_handler: &'a SharedWatchHandler,
    function_name: &'a str,
    watch_context: Option<&'a WatchStreamContext>,
) -> BoxFuture<'a, Result<EvalValue>>
where
    F: LlmHandler<Fut>,
    Fut: LlmFuture,
{
    Box::pin(async move {
        Ok(match expr {
            Expr::Value(v) => EvalValue::Value(v.clone()),
            Expr::List(items, meta) => {
                let mut out = Vec::with_capacity(items.len());
                for it in items.iter() {
                    out.push(expect_value(
                        evaluate_expr(
                            it,
                            scopes,
                            thir,
                            run_llm_function,
                            watch_handler,
                            function_name,
                        )
                        .await?,
                    )?);
                }
                EvalValue::Value(BamlValueWithMeta::List(out, meta.clone()))
            }
            Expr::Map(entries, meta) => {
                let mut out: BamlMap<String, BamlValueWithMeta<ExprMetadata>> = BamlMap::new();
                for (k, v) in entries.iter() {
                    out.insert(
                        k.clone(),
                        expect_value(
                            evaluate_expr(
                                v,
                                scopes,
                                thir,
                                run_llm_function,
                                watch_handler,
                                function_name,
                            )
                            .await?,
                        )?,
                    );
                }
                EvalValue::Value(BamlValueWithMeta::Map(out, meta.clone()))
            }
            Expr::Block(block, _meta) => {
                let v = evaluate_block(
                    block,
                    scopes,
                    thir,
                    run_llm_function,
                    watch_handler,
                    function_name,
                )
                .await?;
                EvalValue::Value(v)
            }
            Expr::Var(name, meta) => {
                // First check if it's an LLM function
                if let Some(_llm_func) = thir.llm_functions.iter().find(|f| &f.name == name) {
                    // Return a special marker for LLM functions that can be called
                    // We'll handle the actual calling in the Call expression
                    EvalValue::Function(
                        0,
                        Arc::new(Block {
                            env: BamlMap::new(),
                            statements: vec![],
                            trailing_expr: Some(Expr::Value(BamlValueWithMeta::String(
                                format!("__LLM_FUNCTION__{name}"),
                                meta.clone(),
                            ))),
                            ty: None,
                            span: internal_baml_diagnostics::Span::fake(),
                        }),
                        meta.clone(),
                    )
                }
                // Check if it's a builtin function
                else if name.starts_with("baml.") {
                    // Return a special marker for builtin functions
                    EvalValue::Function(
                        0, // Arity will be checked at call site
                        Arc::new(Block {
                            env: BamlMap::new(),
                            statements: vec![],
                            trailing_expr: Some(Expr::Value(BamlValueWithMeta::String(
                                format!("__BUILTIN_FUNCTION__{name}"),
                                meta.clone(),
                            ))),
                            ty: None,
                            span: internal_baml_diagnostics::Span::fake(),
                        }),
                        meta.clone(),
                    )
                }
                // Check if it's an expression function
                else if let Some(expr_func) = thir.expr_functions.iter().find(|f| &f.name == name)
                {
                    EvalValue::Function(
                        expr_func.parameters.len(),
                        Arc::new(expr_func.body.clone()),
                        meta.clone(),
                    )
                } else {
                    let cell = lookup_cell(scopes, name)
                        .with_context(|| format!("unbound variable `{}` at {:?}", name, meta.0))?;
                    EvalValue::Reference(cell)
                }
            }
            Expr::Function(arity, body, meta) => {
                EvalValue::Function(*arity, body.clone(), meta.clone())
            }
            Expr::Call {
                func,
                type_args,
                args,
                meta: _,
            } => {
                if let Expr::Var(func_name, _) = func.as_ref() {
                    if func_name == "env.get" {
                        if args.len() != 1 {
                            bail!("env.get expects exactly one argument");
                        }

                        let key_val = expect_value(
                            evaluate_expr(
                                &args[0],
                                scopes,
                                thir,
                                run_llm_function,
                                watch_handler,
                                function_name,
                            )
                            .await?,
                        )?;

                        let key = match key_val {
                            BamlValueWithMeta::String(value, _) => value,
                            _ => bail!("env.get argument must be a string"),
                        };

                        let env_map = lookup(scopes, "__env_vars__")
                            .ok_or_else(|| anyhow!("environment context missing"))?;

                        let map = match env_map {
                            BamlValueWithMeta::Map(ref entries, _) => entries,
                            _ => bail!("environment context corrupted"),
                        };

                        if let Some(value) = map.get(&key) {
                            return Ok(EvalValue::Value(value.clone()));
                        } else {
                            bail!("Environment variable '{}' not found", key);
                        }
                    }
                }

                let callee = evaluate_expr(
                    func,
                    scopes,
                    thir,
                    run_llm_function,
                    watch_handler,
                    function_name,
                )
                .await?;
                let (arity, body, meta) = match callee {
                    EvalValue::Function(a, b, m) => (a, b, m),
                    _ => bail!("attempted to call non-function"),
                };

                // Check if this is an LLM function call
                if let Some(Expr::Value(BamlValueWithMeta::String(marker, _))) = &body.trailing_expr
                {
                    if marker.starts_with("__LLM_FUNCTION__") {
                        let fn_name = marker.strip_prefix("__LLM_FUNCTION__").unwrap().to_string();

                        // Evaluate arguments and convert to BamlValue
                        let mut llm_args: Vec<BamlValue> = Vec::with_capacity(args.len());
                        for a in args.iter() {
                            let arg_val = expect_value(
                                evaluate_expr(
                                    a,
                                    scopes,
                                    thir,
                                    run_llm_function,
                                    watch_handler,
                                    function_name,
                                )
                                .await?,
                            )?;
                            llm_args.push(baml_value_with_meta_to_baml_value(arg_val));
                        }

                        // Call the LLM function with watch context if available
                        let result =
                            run_llm_function(fn_name, llm_args, watch_context.cloned()).await?;
                        return Ok(EvalValue::Value(result));
                    }

                    // Check if this is a builtin function call
                    if marker.starts_with("__BUILTIN_FUNCTION__") {
                        let fn_name = marker
                            .strip_prefix("__BUILTIN_FUNCTION__")
                            .unwrap()
                            .to_string();

                        // Evaluate arguments
                        let mut arg_vals: Vec<BamlValueWithMeta<ExprMetadata>> =
                            Vec::with_capacity(args.len());
                        for a in args.iter() {
                            arg_vals.push(expect_value(
                                evaluate_expr(
                                    a,
                                    scopes,
                                    thir,
                                    run_llm_function,
                                    watch_handler,
                                    function_name,
                                )
                                .await?,
                            )?);
                        }

                        // Handle builtin functions
                        let result =
                            evaluate_builtin_function(&fn_name, &arg_vals, type_args, &meta)
                                .await?;
                        return Ok(EvalValue::Value(result));
                    }
                }

                if arity != args.len() {
                    bail!(
                        "arity mismatch: expected {} args, got {}",
                        arity,
                        args.len()
                    );
                }

                // Evaluate arguments first
                let mut arg_vals: Vec<BamlValueWithMeta<ExprMetadata>> =
                    Vec::with_capacity(args.len());
                for a in args.iter() {
                    arg_vals.push(expect_value(
                        evaluate_expr(
                            a,
                            scopes,
                            thir,
                            run_llm_function,
                            watch_handler,
                            function_name,
                        )
                        .await?,
                    )?);
                }

                // Check if this is an expression function call to get parameter names
                // and the actual function name for watch notifications
                let (param_names, called_function_name) = if let Expr::Var(func_name, _) =
                    func.as_ref()
                {
                    if let Some(expr_func) =
                        thir.expr_functions.iter().find(|f| &f.name == func_name)
                    {
                        // Use actual parameter names from expression function
                        let params = expr_func
                            .parameters
                            .iter()
                            .map(|p| p.name.clone())
                            .collect::<Vec<_>>();
                        (params, func_name.as_str())
                    } else {
                        // Use fresh names for anonymous functions
                        let body_expr =
                            Expr::Block(Box::new(Arc::unwrap_or_clone(body.clone())), meta.clone());
                        (body_expr.fresh_names(arity), function_name)
                    }
                } else {
                    // Use fresh names for complex function expressions
                    let body_expr =
                        Expr::Block(Box::new(Arc::unwrap_or_clone(body.clone())), meta.clone());
                    (body_expr.fresh_names(arity), function_name)
                };

                // Create a scope binding parameters to their argument values
                scopes.push(Scope {
                    variables: param_names
                        .into_iter()
                        .zip(arg_vals)
                        .map(|(k, v)| (k, Arc::new(Mutex::new(v))))
                        .collect(),
                    watch_variables: Vec::new(),
                    is_filter_context: false,
                });

                // Execute the function body with the correct function name for watch notifications
                let result = evaluate_block(
                    &body,
                    scopes,
                    thir,
                    run_llm_function,
                    watch_handler,
                    called_function_name,
                )
                .await?;
                scopes.pop();
                EvalValue::Value(result)
            }
            Expr::If(cond, then, else_, meta) => {
                let cv = expect_value(
                    evaluate_expr(
                        cond,
                        scopes,
                        thir,
                        run_llm_function,
                        watch_handler,
                        function_name,
                    )
                    .await?,
                )?;
                let b = match cv {
                    BamlValueWithMeta::Bool(v, _) => v,
                    _ => bail!("condition not bool at {:?}", meta.0),
                };
                if b {
                    EvalValue::Value(expect_value(
                        evaluate_expr(
                            then,
                            scopes,
                            thir,
                            run_llm_function,
                            watch_handler,
                            function_name,
                        )
                        .await?,
                    )?)
                } else if let Some(e) = else_ {
                    EvalValue::Value(expect_value(
                        evaluate_expr(
                            e,
                            scopes,
                            thir,
                            run_llm_function,
                            watch_handler,
                            function_name,
                        )
                        .await?,
                    )?)
                } else {
                    EvalValue::Value(BamlValueWithMeta::Null(meta.clone()))
                }
            }
            Expr::ArrayAccess { base, index, meta } => {
                let b = expect_value(
                    evaluate_expr(
                        base,
                        scopes,
                        thir,
                        run_llm_function,
                        watch_handler,
                        function_name,
                    )
                    .await?,
                )?;
                let i = expect_value(
                    evaluate_expr(
                        index,
                        scopes,
                        thir,
                        run_llm_function,
                        watch_handler,
                        function_name,
                    )
                    .await?,
                )?;
                let arr = match b.clone() {
                    BamlValueWithMeta::List(v, _) => v,
                    _ => bail!("array access on non-list at {:?}", meta),
                };
                let idx = match i {
                    BamlValueWithMeta::Int(ii, _) => ii as usize,
                    _ => bail!("index not int at {:?}", meta),
                };
                let v = arr.get(idx).cloned().context("index out of bounds")?;
                EvalValue::Value(v.clone())
            }
            Expr::FieldAccess { base, field, meta } => {
                let b = expect_value(
                    evaluate_expr(
                        base,
                        scopes,
                        thir,
                        run_llm_function,
                        watch_handler,
                        function_name,
                    )
                    .await?,
                )?;
                match b.clone() {
                    BamlValueWithMeta::Map(m, _) => {
                        let v = m.get(field).context("missing field")?;
                        EvalValue::Value(v.clone())
                    }
                    BamlValueWithMeta::Class(_, m, _) => {
                        let v = m.get(field).context("missing field")?;
                        EvalValue::Value(v.clone())
                    }
                    _ => bail!("field access on non-map/class at {:?}", meta.0),
                }
            }
            Expr::ClassConstructor { name, fields, meta } => {
                let mut field_map: BamlMap<String, BamlValueWithMeta<ExprMetadata>> =
                    BamlMap::new();

                for field in fields {
                    match field {
                        ClassConstructorField::Named { name, value } => {
                            field_map.insert(
                                name.clone(),
                                expect_value(
                                    evaluate_expr(
                                        value,
                                        scopes,
                                        thir,
                                        run_llm_function,
                                        watch_handler,
                                        function_name,
                                    )
                                    .await?,
                                )?,
                            );
                        }

                        ClassConstructorField::Spread { value } => {
                            let spread_val = expect_value(
                                evaluate_expr(
                                    value,
                                    scopes,
                                    thir,
                                    run_llm_function,
                                    watch_handler,
                                    function_name,
                                )
                                .await?,
                            )?;
                            match spread_val.clone() {
                                BamlValueWithMeta::Class(_, spread_fields, _) => {
                                    for (k, v) in spread_fields.iter() {
                                        field_map.insert(k.clone(), v.clone());
                                    }
                                }
                                // // TODO: Allow maps to be spread?
                                // BamlValueWithMeta::Map(spread_fields) => {
                                //     for (k, v) in spread_fields.iter() {
                                //         field_map.insert(k.clone(), v.clone());
                                //     }
                                // }
                                _ => bail!(
                                    "spread operator can only be used on classes at {:?}",
                                    meta.0
                                ),
                            }
                        }
                    }
                }

                EvalValue::Value(BamlValueWithMeta::Class(
                    name.clone(),
                    field_map,
                    meta.clone(),
                ))
            }
            Expr::Builtin(builtin, meta) => {
                use crate::thir::Builtin;
                match builtin {
                    Builtin::FetchValue => {
                        // FetchValue requires network access and is not supported in the interpreter
                        bail!(
                            "builtin function baml.fetch_value is not supported in interpreter at {:?}",
                            meta.0
                        )
                    }
                }
            }
            Expr::BinaryOperation {
                left,
                operator,
                right,
                meta,
            } => {
                // Special handling for instanceof: right operand is a type name, not a value
                if matches!(operator, crate::hir::BinaryOperator::InstanceOf) {
                    let left_val = expect_value(
                        evaluate_expr(
                            left,
                            scopes,
                            thir,
                            run_llm_function,
                            watch_handler,
                            function_name,
                        )
                        .await?,
                    )?;

                    // Extract class name from right side (should be Expr::Var)
                    let class_name = match right.as_ref() {
                        Expr::Var(name, _) => name.clone(),
                        _ => bail!(
                            "instanceof requires a class name on the right side at {:?}",
                            meta.0
                        ),
                    };

                    // Check if left value is a class instance matching the class name
                    let result = match left_val {
                        BamlValueWithMeta::Class(ref left_class, ..) => {
                            BamlValueWithMeta::Bool(left_class == &class_name, meta.clone())
                        }
                        _ => bail!(
                            "instanceof requires a class instance on the left side at {:?}",
                            meta.0
                        ),
                    };

                    EvalValue::Value(result)
                } else {
                    // Normal binary operation: evaluate both sides
                    let left_val = expect_value(
                        evaluate_expr(
                            left,
                            scopes,
                            thir,
                            run_llm_function,
                            watch_handler,
                            function_name,
                        )
                        .await?,
                    )?;
                    let right_val = expect_value(
                        evaluate_expr(
                            right,
                            scopes,
                            thir,
                            run_llm_function,
                            watch_handler,
                            function_name,
                        )
                        .await?,
                    )?;

                    let result = evaluate_binary_op(operator, &left_val, &right_val, meta)?;
                    EvalValue::Value(result)
                }
            }
            Expr::UnaryOperation {
                operator,
                expr,
                meta,
            } => {
                let val = expect_value(
                    evaluate_expr(
                        expr,
                        scopes,
                        thir,
                        run_llm_function,
                        watch_handler,
                        function_name,
                    )
                    .await?,
                )?;

                let result = evaluate_unary_op(operator, &val, meta)?;
                EvalValue::Value(result)
            }
            Expr::MethodCall {
                receiver,
                method,
                args,
                meta,
            } => {
                // Extract method name
                let method_name = match method.as_ref() {
                    Expr::Var(name, _) => name.clone(),
                    _ => bail!("method name must be an identifier at {:?}", meta.0),
                };

                // For mutating methods like push(), we need the cell reference
                if method_name == "push" {
                    // Get the receiver as a reference (cell) if possible
                    let receiver_eval = evaluate_expr(
                        receiver,
                        scopes,
                        thir,
                        run_llm_function,
                        watch_handler,
                        function_name,
                    )
                    .await?;

                    let receiver_cell = match receiver_eval {
                        EvalValue::Reference(cell) => cell,
                        _ => bail!("push() can only be called on a variable at {:?}", meta.0),
                    };

                    // Evaluate arguments
                    let mut arg_vals: Vec<BamlValueWithMeta<ExprMetadata>> =
                        Vec::with_capacity(args.len());
                    for arg in args.iter() {
                        arg_vals.push(expect_value(
                            evaluate_expr(
                                arg,
                                scopes,
                                thir,
                                run_llm_function,
                                watch_handler,
                                function_name,
                            )
                            .await?,
                        )?);
                    }

                    // Mutate the array
                    let mut receiver_val = receiver_cell.lock().unwrap();
                    match &mut *receiver_val {
                        BamlValueWithMeta::List(items, _) => {
                            if arg_vals.len() != 1 {
                                bail!("push() expects exactly one argument at {:?}", meta.0);
                            }
                            items.push(arg_vals[0].clone());
                            // Return void/unit
                            EvalValue::Value(BamlValueWithMeta::Null(meta.clone()))
                        }
                        _ => bail!("push() can only be called on arrays at {:?}", meta.0),
                    }
                } else {
                    // Non-mutating methods
                    let receiver_val = expect_value(
                        evaluate_expr(
                            receiver,
                            scopes,
                            thir,
                            run_llm_function,
                            watch_handler,
                            function_name,
                        )
                        .await?,
                    )?;

                    // Evaluate arguments
                    let mut arg_vals: Vec<BamlValueWithMeta<ExprMetadata>> =
                        Vec::with_capacity(args.len());
                    for arg in args.iter() {
                        arg_vals.push(expect_value(
                            evaluate_expr(
                                arg,
                                scopes,
                                thir,
                                run_llm_function,
                                watch_handler,
                                function_name,
                            )
                            .await?,
                        )?);
                    }

                    let result =
                        evaluate_method_call(&receiver_val, &method_name, &arg_vals, meta)?;
                    EvalValue::Value(result)
                }
            }
            Expr::Paren(inner, _) => {
                evaluate_expr(
                    inner,
                    scopes,
                    thir,
                    run_llm_function,
                    watch_handler,
                    function_name,
                )
                .await?
            }
        })
    })
}

fn expect_value(v: EvalValue) -> Result<BamlValueWithMeta<ExprMetadata>> {
    match v {
        EvalValue::Value(v) => Ok(v),
        EvalValue::Reference(cell) => Ok(cell.lock().unwrap().clone()),
        EvalValue::Function(_, _, _) => bail!("expected value, found function"),
    }
}

fn evaluate_binary_op(
    operator: &crate::hir::BinaryOperator,
    left_val: &BamlValueWithMeta<ExprMetadata>,
    right_val: &BamlValueWithMeta<ExprMetadata>,
    meta: &ExprMetadata,
) -> Result<BamlValueWithMeta<ExprMetadata>> {
    use crate::hir::BinaryOperator;
    Ok(match operator {
        // Arithmetic operations
        BinaryOperator::Add => match (left_val.clone(), right_val.clone()) {
            (BamlValueWithMeta::Int(a, _), BamlValueWithMeta::Int(b, _)) => {
                BamlValueWithMeta::Int(a + b, meta.clone())
            }
            (BamlValueWithMeta::Float(a, _), BamlValueWithMeta::Float(b, _)) => {
                BamlValueWithMeta::Float(a + b, meta.clone())
            }
            (BamlValueWithMeta::Int(a, _), BamlValueWithMeta::Float(b, _)) => {
                BamlValueWithMeta::Float(a as f64 + b, meta.clone())
            }
            (BamlValueWithMeta::Float(a, _), BamlValueWithMeta::Int(b, _)) => {
                BamlValueWithMeta::Float(a + (b as f64), meta.clone())
            }
            (BamlValueWithMeta::String(a, _), BamlValueWithMeta::String(b, _)) => {
                BamlValueWithMeta::String(format!("{a}{b}"), meta.clone())
            }
            _ => bail!("unsupported types for + operator at {:?}", meta.0),
        },
        BinaryOperator::Sub => match (left_val.clone(), right_val.clone()) {
            (BamlValueWithMeta::Int(a, _), BamlValueWithMeta::Int(b, _)) => {
                BamlValueWithMeta::Int(a - b, meta.clone())
            }
            (BamlValueWithMeta::Float(a, _), BamlValueWithMeta::Float(b, _)) => {
                BamlValueWithMeta::Float(a - b, meta.clone())
            }
            (BamlValueWithMeta::Int(a, _), BamlValueWithMeta::Float(b, _)) => {
                BamlValueWithMeta::Float((a as f64) - b, meta.clone())
            }
            (BamlValueWithMeta::Float(a, _), BamlValueWithMeta::Int(b, _)) => {
                BamlValueWithMeta::Float(a - (b as f64), meta.clone())
            }
            _ => bail!("unsupported types for - operator at {:?}", meta.0),
        },
        BinaryOperator::Mul => match (left_val.clone(), right_val.clone()) {
            (BamlValueWithMeta::Int(a, _), BamlValueWithMeta::Int(b, _)) => {
                BamlValueWithMeta::Int(a * b, meta.clone())
            }
            (BamlValueWithMeta::Float(a, _), BamlValueWithMeta::Float(b, _)) => {
                BamlValueWithMeta::Float(a * b, meta.clone())
            }
            (BamlValueWithMeta::Int(a, _), BamlValueWithMeta::Float(b, _)) => {
                BamlValueWithMeta::Float((a as f64) * b, meta.clone())
            }
            (BamlValueWithMeta::Float(a, _), BamlValueWithMeta::Int(b, _)) => {
                BamlValueWithMeta::Float(a * (b as f64), meta.clone())
            }
            _ => bail!("unsupported types for * operator at {:?}", meta.0),
        },
        BinaryOperator::Div => match (left_val.clone(), right_val.clone()) {
            (BamlValueWithMeta::Int(a, _), BamlValueWithMeta::Int(b, _)) => {
                if b == 0 {
                    bail!("division by zero at {:?}", meta.0);
                }
                BamlValueWithMeta::Float((a as f64) / (b as f64), meta.clone())
            }
            (BamlValueWithMeta::Float(a, _), BamlValueWithMeta::Float(b, _)) => {
                if b == 0.0 {
                    bail!("division by zero at {:?}", meta.0);
                }
                BamlValueWithMeta::Float(a / b, meta.clone())
            }
            (BamlValueWithMeta::Int(a, _), BamlValueWithMeta::Float(b, _)) => {
                if b == 0.0 {
                    bail!("division by zero at {:?}", meta.0);
                }
                BamlValueWithMeta::Float((a as f64) / b, meta.clone())
            }
            (BamlValueWithMeta::Float(a, _), BamlValueWithMeta::Int(b, _)) => {
                if b == 0 {
                    bail!("division by zero at {:?}", meta.0);
                }
                BamlValueWithMeta::Float(a / (b as f64), meta.clone())
            }
            _ => bail!("unsupported types for / operator at {:?}", meta.0),
        },
        BinaryOperator::Mod => match (left_val.clone(), right_val.clone()) {
            (BamlValueWithMeta::Int(a, _), BamlValueWithMeta::Int(b, _)) => {
                if b == 0 {
                    bail!("modulo by zero at {:?}", meta.0);
                }
                BamlValueWithMeta::Int(a % b, meta.clone())
            }
            _ => bail!("unsupported types for % operator at {:?}", meta.0),
        },

        // Comparison operations
        BinaryOperator::Eq => {
            let equal = values_equal(&left_val.clone(), &right_val.clone());
            BamlValueWithMeta::Bool(equal, meta.clone())
        }
        BinaryOperator::Neq => {
            let not_equal = !values_equal(&left_val.clone(), &right_val.clone());
            BamlValueWithMeta::Bool(not_equal, meta.clone())
        }
        BinaryOperator::Lt => {
            let ord_opt = compare_values(&left_val.clone(), &right_val.clone())?;
            let less = ord_opt
                .map(|ord| matches!(ord, std::cmp::Ordering::Less))
                .ok_or_else(|| anyhow!("unsupported types for < operator at {:?}", meta.0))?;
            BamlValueWithMeta::Bool(less, meta.clone())
        }
        BinaryOperator::LtEq => {
            let ord_opt = compare_values(&left_val.clone(), &right_val.clone())?;
            let less_eq = ord_opt
                .map(|ord| matches!(ord, std::cmp::Ordering::Less | std::cmp::Ordering::Equal))
                .ok_or_else(|| anyhow!("unsupported types for <= operator at {:?}", meta.0))?;
            BamlValueWithMeta::Bool(less_eq, meta.clone())
        }
        BinaryOperator::Gt => {
            let ord_opt = compare_values(&left_val.clone(), &right_val.clone())?;
            let greater = ord_opt
                .map(|ord| matches!(ord, std::cmp::Ordering::Greater))
                .ok_or_else(|| anyhow!("unsupported types for > operator at {:?}", meta.0))?;
            BamlValueWithMeta::Bool(greater, meta.clone())
        }
        BinaryOperator::GtEq => {
            let ord_opt = compare_values(&left_val.clone(), &right_val.clone())?;
            let greater_eq = ord_opt
                .map(|ord| matches!(ord, std::cmp::Ordering::Greater | std::cmp::Ordering::Equal))
                .ok_or_else(|| anyhow!("unsupported types for >= operator at {:?}", meta.0))?;
            BamlValueWithMeta::Bool(greater_eq, meta.clone())
        }

        // Logical operations
        BinaryOperator::And => match left_val.clone() {
            BamlValueWithMeta::Bool(false, _) => BamlValueWithMeta::Bool(false, meta.clone()),
            BamlValueWithMeta::Bool(true, _) => match right_val.clone() {
                BamlValueWithMeta::Bool(b, _) => BamlValueWithMeta::Bool(b, meta.clone()),
                _ => bail!("right operand of && must be bool at {:?}", meta.0),
            },
            _ => bail!("left operand of && must be bool at {:?}", meta.0),
        },
        BinaryOperator::Or => match left_val.clone() {
            BamlValueWithMeta::Bool(true, _) => BamlValueWithMeta::Bool(true, meta.clone()),
            BamlValueWithMeta::Bool(false, _) => match right_val.clone() {
                BamlValueWithMeta::Bool(b, _) => BamlValueWithMeta::Bool(b, meta.clone()),
                _ => bail!("right operand of || must be bool at {:?}", meta.0),
            },
            _ => bail!("left operand of || must be bool at {:?}", meta.0),
        },

        // Bitwise operations (integer only)
        BinaryOperator::BitAnd => match (left_val.clone(), right_val.clone()) {
            (BamlValueWithMeta::Int(a, _), BamlValueWithMeta::Int(b, _)) => {
                BamlValueWithMeta::Int(a & b, meta.clone())
            }
            _ => bail!("bitwise & requires integer operands at {:?}", meta.0),
        },
        BinaryOperator::BitOr => match (left_val.clone(), right_val.clone()) {
            (BamlValueWithMeta::Int(a, _), BamlValueWithMeta::Int(b, _)) => {
                BamlValueWithMeta::Int(a | b, meta.clone())
            }
            _ => bail!("bitwise | requires integer operands at {:?}", meta.0),
        },
        BinaryOperator::BitXor => match (left_val.clone(), right_val.clone()) {
            (BamlValueWithMeta::Int(a, _), BamlValueWithMeta::Int(b, _)) => {
                BamlValueWithMeta::Int(a ^ b, meta.clone())
            }
            _ => bail!("bitwise ^ requires integer operands at {:?}", meta.0),
        },
        BinaryOperator::Shl => match (left_val.clone(), right_val.clone()) {
            (BamlValueWithMeta::Int(a, _), BamlValueWithMeta::Int(b, _)) => {
                if b < 0 {
                    bail!("negative shift amount at {:?}", meta.0);
                }
                BamlValueWithMeta::Int(a << b, meta.clone())
            }
            _ => bail!("shift << requires integer operands at {:?}", meta.0),
        },
        BinaryOperator::Shr => match (left_val.clone(), right_val.clone()) {
            (BamlValueWithMeta::Int(a, _), BamlValueWithMeta::Int(b, _)) => {
                if b < 0 {
                    bail!("negative shift amount at {:?}", meta.0);
                }
                BamlValueWithMeta::Int(a >> b, meta.clone())
            }
            _ => bail!("shift >> requires integer operands at {:?}", meta.0),
        },
        BinaryOperator::InstanceOf => match (left_val.clone(), right_val.clone()) {
            (BamlValueWithMeta::Class(class, ..), BamlValueWithMeta::Class(right_class, ..)) => {
                BamlValueWithMeta::Bool(class == right_class, meta.clone())
            }
            _ => bail!("instanceof requires class operands at {:?}", meta.0),
        },
    })
}

fn evaluate_unary_op(
    operator: &crate::hir::UnaryOperator,
    val: &BamlValueWithMeta<ExprMetadata>,
    meta: &ExprMetadata,
) -> Result<BamlValueWithMeta<ExprMetadata>> {
    use crate::hir::UnaryOperator;
    Ok(match operator {
        UnaryOperator::Not => match val.clone() {
            BamlValueWithMeta::Bool(b, _) => BamlValueWithMeta::Bool(!b, meta.clone()),
            _ => bail!("! operator requires boolean operand at {:?}", meta.0),
        },
        UnaryOperator::Neg => match val.clone() {
            BamlValueWithMeta::Int(i, _) => BamlValueWithMeta::Int(-i, meta.clone()),
            BamlValueWithMeta::Float(f, _) => BamlValueWithMeta::Float(-f, meta.clone()),
            _ => bail!("- operator requires numeric operand at {:?}", meta.0),
        },
    })
}

fn values_equal(
    left: &BamlValueWithMeta<ExprMetadata>,
    right: &BamlValueWithMeta<ExprMetadata>,
) -> bool {
    match (left, right) {
        (BamlValueWithMeta::Int(a, _), BamlValueWithMeta::Int(b, _)) => a == b,
        (BamlValueWithMeta::Float(a, _), BamlValueWithMeta::Float(b, _)) => a == b,
        (BamlValueWithMeta::Int(a, _), BamlValueWithMeta::Float(b, _)) => *a as f64 == *b,
        (BamlValueWithMeta::Float(a, _), BamlValueWithMeta::Int(b, _)) => *a == *b as f64,
        (BamlValueWithMeta::String(a, _), BamlValueWithMeta::String(b, _)) => a == b,
        (BamlValueWithMeta::Null(_), BamlValueWithMeta::Null(_)) => true,
        _ => false,
    }
}

fn compare_values(
    left: &BamlValueWithMeta<ExprMetadata>,
    right: &BamlValueWithMeta<ExprMetadata>,
) -> Result<Option<std::cmp::Ordering>> {
    Ok(match (left, right) {
        (BamlValueWithMeta::Int(a, _), BamlValueWithMeta::Int(b, _)) => Some(a.cmp(b)),
        (BamlValueWithMeta::Float(a, _), BamlValueWithMeta::Float(b, _)) => a.partial_cmp(b),
        (BamlValueWithMeta::Int(a, _), BamlValueWithMeta::Float(b, _)) => {
            (*a as f64).partial_cmp(b)
        }
        (BamlValueWithMeta::Float(a, _), BamlValueWithMeta::Int(b, _)) => {
            a.partial_cmp(&(*b as f64))
        }
        (BamlValueWithMeta::String(a, _), BamlValueWithMeta::String(b, _)) => Some(a.cmp(b)),
        _ => None,
    })
}

fn parse_json_to_baml_value(
    json_str: &str,
    target_type: &baml_types::TypeIR,
    meta: &ExprMetadata,
) -> Result<BamlValueWithMeta<ExprMetadata>> {
    use baml_types::TypeIR;

    let json_value: serde_json::Value = serde_json::from_str(json_str).with_context(|| {
        format!(
            "baml.fetch_as: failed to parse JSON response at {:?}",
            meta.0
        )
    })?;

    fn json_to_baml(
        json: &serde_json::Value,
        target_type: &TypeIR,
        meta: &ExprMetadata,
    ) -> Result<BamlValueWithMeta<ExprMetadata>> {
        use baml_types::TypeIR;
        use serde_json::Value as JsonValue;

        match (json, target_type) {
            (JsonValue::Null, _) => Ok(BamlValueWithMeta::Null(meta.clone())),
            (JsonValue::Bool(b), TypeIR::Primitive(baml_types::TypeValue::Bool, _)) => {
                Ok(BamlValueWithMeta::Bool(*b, meta.clone()))
            }
            (JsonValue::Number(n), TypeIR::Primitive(baml_types::TypeValue::Int, _)) => {
                if let Some(i) = n.as_i64() {
                    Ok(BamlValueWithMeta::Int(i, meta.clone()))
                } else {
                    bail!("Expected integer, got {}", n)
                }
            }
            (JsonValue::Number(n), TypeIR::Primitive(baml_types::TypeValue::Float, _)) => {
                if let Some(f) = n.as_f64() {
                    Ok(BamlValueWithMeta::Float(f, meta.clone()))
                } else {
                    bail!("Expected float, got {}", n)
                }
            }
            (JsonValue::String(s), TypeIR::Primitive(baml_types::TypeValue::String, _)) => {
                Ok(BamlValueWithMeta::String(s.clone(), meta.clone()))
            }
            (JsonValue::Array(arr), TypeIR::List(elem_type, _)) => {
                let mut baml_list = Vec::new();
                for item in arr {
                    baml_list.push(json_to_baml(item, elem_type, meta)?);
                }
                Ok(BamlValueWithMeta::List(baml_list, meta.clone()))
            }
            (JsonValue::Object(obj), TypeIR::Map(_, value_type, _)) => {
                let mut baml_map = BamlMap::new();
                for (key, value) in obj {
                    baml_map.insert(key.clone(), json_to_baml(value, value_type, meta)?);
                }
                Ok(BamlValueWithMeta::Map(baml_map, meta.clone()))
            }
            (JsonValue::Object(obj), TypeIR::Class { name, .. }) => {
                let mut baml_fields = BamlMap::new();
                for (key, value) in obj {
                    // For now, we'll infer the type from the JSON value
                    // In a real implementation, we'd look up the class definition
                    let field_value = json_to_baml_inferred(value, meta)?;
                    baml_fields.insert(key.clone(), field_value);
                }
                Ok(BamlValueWithMeta::Class(
                    name.clone(),
                    baml_fields,
                    meta.clone(),
                ))
            }
            _ => {
                // Try to infer the type if we can't match
                json_to_baml_inferred(json, meta)
            }
        }
    }

    fn json_to_baml_inferred(
        json: &serde_json::Value,
        meta: &ExprMetadata,
    ) -> Result<BamlValueWithMeta<ExprMetadata>> {
        use serde_json::Value as JsonValue;

        match json {
            JsonValue::Null => Ok(BamlValueWithMeta::Null(meta.clone())),
            JsonValue::Bool(b) => Ok(BamlValueWithMeta::Bool(*b, meta.clone())),
            JsonValue::Number(n) => {
                if let Some(i) = n.as_i64() {
                    Ok(BamlValueWithMeta::Int(i, meta.clone()))
                } else if let Some(f) = n.as_f64() {
                    Ok(BamlValueWithMeta::Float(f, meta.clone()))
                } else {
                    bail!("Invalid number: {}", n)
                }
            }
            JsonValue::String(s) => Ok(BamlValueWithMeta::String(s.clone(), meta.clone())),
            JsonValue::Array(arr) => {
                let mut baml_list = Vec::new();
                for item in arr {
                    baml_list.push(json_to_baml_inferred(item, meta)?);
                }
                Ok(BamlValueWithMeta::List(baml_list, meta.clone()))
            }
            JsonValue::Object(obj) => {
                let mut baml_map = BamlMap::new();
                for (key, value) in obj {
                    baml_map.insert(key.clone(), json_to_baml_inferred(value, meta)?);
                }
                Ok(BamlValueWithMeta::Map(baml_map, meta.clone()))
            }
        }
    }

    json_to_baml(&json_value, target_type, meta)
}

async fn evaluate_builtin_function(
    fn_name: &str,
    args: &[BamlValueWithMeta<ExprMetadata>],
    type_args: &[baml_types::TypeIR],
    meta: &ExprMetadata,
) -> Result<BamlValueWithMeta<ExprMetadata>> {
    match fn_name {
        "baml.media.image.from_url" => {
            if args.len() != 1 {
                bail!(
                    "baml.media.image.from_url expects 1 argument, got {} at {:?}",
                    args.len(),
                    meta.0
                );
            }
            let url = match &args[0] {
                BamlValueWithMeta::String(s, _) => s.clone(),
                _ => bail!(
                    "baml.media.image.from_url expects a string argument at {:?}",
                    meta.0
                ),
            };
            Ok(BamlValueWithMeta::Media(
                baml_types::BamlMedia::url(baml_types::BamlMediaType::Image, url, None),
                meta.clone(),
            ))
        }
        "baml.fetch_as" => {
            if args.len() != 1 {
                bail!(
                    "baml.fetch_as expects 1 argument (url), got {} at {:?}",
                    args.len(),
                    meta.0
                );
            }
            if type_args.len() != 1 {
                bail!(
                    "baml.fetch_as expects 1 type argument, got {} at {:?}",
                    type_args.len(),
                    meta.0
                );
            }

            let url = match &args[0] {
                BamlValueWithMeta::String(s, _) => s.clone(),
                _ => bail!(
                    "baml.fetch_as expects a string URL argument at {:?}",
                    meta.0
                ),
            };

            let target_type = &type_args[0];

            // Make HTTP request
            let response = reqwest::get(&url).await.with_context(|| {
                format!(
                    "baml.fetch_as: failed to fetch URL '{}' at {:?}",
                    url, meta.0
                )
            })?;

            let status = response.status();
            if !status.is_success() {
                let body = response
                    .text()
                    .await
                    .unwrap_or_else(|_| "<failed to read body>".to_string());
                bail!(
                    "baml.fetch_as: HTTP request failed: HTTP {}\nBody: {} at {:?}",
                    status,
                    body,
                    meta.0
                );
            }

            let body = response.text().await.with_context(|| {
                format!(
                    "baml.fetch_as: failed to read response body at {:?}",
                    meta.0
                )
            })?;

            // Parse the JSON body into the target type
            let parsed_value = parse_json_to_baml_value(&body, target_type, meta)?;
            Ok(parsed_value)
        }
        _ => bail!("unknown builtin function '{}' at {:?}", fn_name, meta.0),
    }
}

fn evaluate_method_call(
    receiver: &BamlValueWithMeta<ExprMetadata>,
    method_name: &str,
    args: &[BamlValueWithMeta<ExprMetadata>],
    meta: &ExprMetadata,
) -> Result<BamlValueWithMeta<ExprMetadata>> {
    match method_name {
        "length" => {
            // Array/List/String/Map length method
            match receiver {
                BamlValueWithMeta::List(items, _) => {
                    if !args.is_empty() {
                        bail!("length() method takes no arguments at {:?}", meta.0);
                    }
                    Ok(BamlValueWithMeta::Int(items.len() as i64, meta.clone()))
                }
                BamlValueWithMeta::String(s, _) => {
                    if !args.is_empty() {
                        bail!("length() method takes no arguments at {:?}", meta.0);
                    }
                    Ok(BamlValueWithMeta::Int(
                        s.chars().count() as i64,
                        meta.clone(),
                    ))
                }
                BamlValueWithMeta::Map(map, _) => {
                    if !args.is_empty() {
                        bail!("length() method takes no arguments at {:?}", meta.0);
                    }
                    Ok(BamlValueWithMeta::Int(map.len() as i64, meta.clone()))
                }
                _ => bail!(
                    "length() method not available on type {:?} at {:?}",
                    receiver,
                    meta.0
                ),
            }
        }
        "toLowerCase" => {
            let BamlValueWithMeta::String(s, _) = receiver else {
                bail!(
                    "toLowerCase() method only available on strings at {:?}",
                    meta.0
                );
            };
            if !args.is_empty() {
                bail!("toLowerCase() method takes no arguments at {:?}", meta.0);
            }
            Ok(BamlValueWithMeta::String(s.to_lowercase(), meta.clone()))
        }
        "toUpperCase" => {
            let BamlValueWithMeta::String(s, _) = receiver else {
                bail!(
                    "toUpperCase() method only available on strings at {:?}",
                    meta.0
                );
            };
            if !args.is_empty() {
                bail!("toUpperCase() method takes no arguments at {:?}", meta.0);
            }
            Ok(BamlValueWithMeta::String(s.to_uppercase(), meta.clone()))
        }
        "trim" => {
            let BamlValueWithMeta::String(s, _) = receiver else {
                bail!("trim() method only available on strings at {:?}", meta.0);
            };
            if !args.is_empty() {
                bail!("trim() method takes no arguments at {:?}", meta.0);
            }
            Ok(BamlValueWithMeta::String(
                s.trim().to_string(),
                meta.clone(),
            ))
        }
        "includes" => {
            let BamlValueWithMeta::String(s, _) = receiver else {
                bail!(
                    "includes() method only available on strings at {:?}",
                    meta.0
                );
            };
            if args.len() != 1 {
                bail!("includes() method takes exactly 1 argument at {:?}", meta.0);
            }
            let BamlValueWithMeta::String(search, _) = &args[0] else {
                bail!("includes() argument must be a string at {:?}", meta.0);
            };
            Ok(BamlValueWithMeta::Bool(
                s.contains(search.as_str()),
                meta.clone(),
            ))
        }
        "startsWith" => {
            let BamlValueWithMeta::String(s, _) = receiver else {
                bail!(
                    "startsWith() method only available on strings at {:?}",
                    meta.0
                );
            };
            if args.len() != 1 {
                bail!(
                    "startsWith() method takes exactly 1 argument at {:?}",
                    meta.0
                );
            }
            let BamlValueWithMeta::String(prefix, _) = &args[0] else {
                bail!("startsWith() argument must be a string at {:?}", meta.0);
            };
            Ok(BamlValueWithMeta::Bool(
                s.starts_with(prefix.as_str()),
                meta.clone(),
            ))
        }
        "endsWith" => {
            let BamlValueWithMeta::String(s, _) = receiver else {
                bail!(
                    "endsWith() method only available on strings at {:?}",
                    meta.0
                );
            };
            if args.len() != 1 {
                bail!("endsWith() method takes exactly 1 argument at {:?}", meta.0);
            }
            let BamlValueWithMeta::String(suffix, _) = &args[0] else {
                bail!("endsWith() argument must be a string at {:?}", meta.0);
            };
            Ok(BamlValueWithMeta::Bool(
                s.ends_with(suffix.as_str()),
                meta.clone(),
            ))
        }
        "split" => {
            let BamlValueWithMeta::String(s, _) = receiver else {
                bail!("split() method only available on strings at {:?}", meta.0);
            };
            if args.len() != 1 {
                bail!("split() method takes exactly 1 argument at {:?}", meta.0);
            }
            let BamlValueWithMeta::String(delimiter, _) = &args[0] else {
                bail!("split() argument must be a string at {:?}", meta.0);
            };
            let parts: Vec<BamlValueWithMeta<ExprMetadata>> = s
                .split(delimiter.as_str())
                .map(|part| BamlValueWithMeta::String(part.to_string(), meta.clone()))
                .collect();
            Ok(BamlValueWithMeta::List(parts, meta.clone()))
        }
        "substring" => {
            let BamlValueWithMeta::String(s, _) = receiver else {
                bail!(
                    "substring() method only available on strings at {:?}",
                    meta.0
                );
            };
            if args.len() != 2 {
                bail!(
                    "substring() method takes exactly 2 arguments at {:?}",
                    meta.0
                );
            }
            let BamlValueWithMeta::Int(start, _) = &args[0] else {
                bail!("substring() start argument must be an int at {:?}", meta.0);
            };
            let BamlValueWithMeta::Int(end, _) = &args[1] else {
                bail!("substring() end argument must be an int at {:?}", meta.0);
            };

            let start = (*start as usize).min(s.len());
            let end = (*end as usize).min(s.len()).max(start);

            Ok(BamlValueWithMeta::String(
                s[start..end].to_string(),
                meta.clone(),
            ))
        }
        "replace" => {
            let BamlValueWithMeta::String(s, _) = receiver else {
                bail!("replace() method only available on strings at {:?}", meta.0);
            };
            if args.len() != 2 {
                bail!("replace() method takes exactly 2 arguments at {:?}", meta.0);
            }
            let BamlValueWithMeta::String(search, _) = &args[0] else {
                bail!("replace() search argument must be a string at {:?}", meta.0);
            };
            let BamlValueWithMeta::String(replacement, _) = &args[1] else {
                bail!(
                    "replace() replacement argument must be a string at {:?}",
                    meta.0
                );
            };
            // Replace first occurrence only (matching JavaScript behavior)
            let result = s.replacen(search.as_str(), replacement.as_str(), 1);
            Ok(BamlValueWithMeta::String(result, meta.clone()))
        }
        _ => bail!(
            "unknown method '{}' at {:?}, should have been caught during typechecking",
            method_name,
            meta.0
        ),
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    #[allow(unused_imports)]
    use baml_types::ir_type::TypeIR;
    use internal_baml_ast::parse_standalone_expression;
    use internal_baml_diagnostics::{Diagnostics, SourceFile, Span};

    use super::*;
    use crate::{
        hir::{self, Hir},
        thir,
        thir::{
            typecheck::{typecheck_expression, typecheck_returning_context},
            GlobalAssignment, THir,
        },
    };

    fn meta() -> ExprMetadata {
        (Span::fake(), None)
    }

    fn empty_thir() -> THir<ExprMetadata> {
        THir {
            expr_functions: vec![],
            llm_functions: vec![],
            global_assignments: BamlMap::new(),
            classes: BamlMap::new(),
            enums: BamlMap::new(),
        }
    }

    /// Convenience function for creating THIR test fixtures.
    fn thir_from_src(
        src: &'static str,
        expr: &'static str,
    ) -> (THir<ExprMetadata>, thir::Expr<ExprMetadata>) {
        let parser_db = crate::test::ast(src).unwrap_or_else(|e| panic!("{}", e));
        let hir = Hir::from_ast(&parser_db.ast);
        let mut diagnostics = Diagnostics::new(PathBuf::from("test.baml"));
        diagnostics.set_source(&SourceFile::new_static(PathBuf::from("test.baml"), src));
        let (thir, typing_context) = typecheck_returning_context(&hir, &mut diagnostics);
        let expr_ast = parse_standalone_expression(expr, &mut diagnostics)
            .expect("Failed to parse expression");
        let expr_hir = hir::Expression::from_ast(&expr_ast);
        let expr_thir = typecheck_expression(&expr_hir, &typing_context, &mut diagnostics);
        (thir, expr_thir)
    }

    fn mock_llm_function(
        _fn_name: String,
        _args: Vec<BamlValue>,
        _watch_context: Option<WatchStreamContext>,
    ) -> BoxFuture<'static, Result<BamlValueWithMeta<ExprMetadata>>> {
        // Mock LLM function that returns an error to simulate unsupported operation
        Box::pin(async move { Ok(BamlValueWithMeta::Int(10, (Span::fake(), None))) })
    }

    async fn interpret_thir_ignoring_watch<F>(
        thir: THir<ExprMetadata>,
        expr: Expr<ExprMetadata>,
        handle_llm_call: F,
        extra_bindings: BamlMap<String, BamlValueWithMeta<ExprMetadata>>,
        env_vars: HashMap<String, String>,
    ) -> Result<BamlValueWithMeta<ExprMetadata>>
    where
        F: FnMut(
                String,
                Vec<BamlValue>,
                Option<WatchStreamContext>,
            ) -> BoxFuture<'static, Result<BamlValueWithMeta<ExprMetadata>>>
            + Send
            + Sync,
    {
        let noop_watch_handler = crate::watch::shared_noop_handler();
        interpret_thir(
            "test".to_string(),
            thir,
            expr,
            handle_llm_call,
            noop_watch_handler,
            extra_bindings,
            env_vars,
        )
        .await
    }

    #[tokio::test]
    async fn eval_atom_int() {
        let (thir, expr) = thir_from_src("", "1");
        let out = interpret_thir_ignoring_watch(
            thir,
            expr,
            mock_llm_function,
            BamlMap::new(),
            HashMap::new(),
        )
        .await
        .unwrap();
        match out {
            BamlValueWithMeta::Int(i, _) => assert_eq!(i, 1),
            v => panic!("expected int, got {v:?}"),
        }
    }

    #[tokio::test]
    async fn eval_function_call_identity() {
        let src = r#"
            function ConstantFunction(x: int) -> int {
                99
            }
        "#;

        let (thir, call) = thir_from_src(src, "ConstantFunction(42)");

        let out = interpret_thir_ignoring_watch(
            thir,
            call,
            mock_llm_function,
            BamlMap::new(),
            HashMap::new(),
        )
        .await
        .unwrap();
        match out {
            BamlValueWithMeta::Int(i, _) => assert_eq!(i, 99),
            v => panic!("expected int, got {v:?}"),
        }
    }

    #[tokio::test]
    async fn eval_function_uses_global() {
        let src = r#"
            let x = 7;

            function UseGlobal() -> int {
                x
            }
        "#;

        let (thir, call) = thir_from_src(src, "UseGlobal()");

        let out = interpret_thir_ignoring_watch(
            thir,
            call,
            mock_llm_function,
            BamlMap::new(),
            HashMap::new(),
        )
        .await
        .unwrap();
        match out {
            BamlValueWithMeta::Int(i, _) => assert_eq!(i, 7),
            v => panic!("expected int, got {v:?}"),
        }
    }

    #[tokio::test]
    async fn test_llm_function_call() {
        let src = r##"
            client<llm> GPT35 {
                provider baml-openai-chat
                options {
                    model gpt-3.5-turbo
                    api_key env.OPENAI_API_KEY
                }
            }

            function SummarizeText(text: string) -> string {
                client GPT35
                prompt #"
                    Summarize the following text: {{ text }}
                "#
            }
        "##;

        let (thir, call) = thir_from_src(
            src,
            r#"SummarizeText("This is a long text that needs to be summarized.")"#,
        );

        // Since the interpreter uses our mock LLM function, this should return our mock value
        let result = interpret_thir_ignoring_watch(
            thir,
            call,
            mock_llm_function,
            BamlMap::new(),
            HashMap::new(),
        )
        .await;
        assert!(result.is_ok());
        let out = result.unwrap();
        match out {
            BamlValueWithMeta::Int(i, _) => assert_eq!(i, 10),
            v => panic!("expected int, got {v:?}"),
        }
    }

    #[tokio::test]
    async fn env_get_returns_value() {
        let src = r#"
            function GetEnv() -> string {
                env.get("API_KEY")
            }
        "#;

        let (thir, call) = thir_from_src(src, "GetEnv()");

        let mut env_vars = HashMap::new();
        env_vars.insert("API_KEY".to_string(), "secret123".to_string());

        let result =
            interpret_thir_ignoring_watch(thir, call, mock_llm_function, BamlMap::new(), env_vars)
                .await
                .unwrap();

        match result {
            BamlValueWithMeta::String(value, _) => assert_eq!(value, "secret123"),
            v => panic!("expected string, got {v:?}"),
        }
    }

    #[tokio::test]
    async fn test_method_call_unknown_method() {
        let (thir, expr) = thir_from_src("", r#""hello".unknown_method()"#);

        let result = interpret_thir_ignoring_watch(
            thir,
            expr,
            mock_llm_function,
            BamlMap::new(),
            HashMap::new(),
        )
        .await;
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains(&format!("unknown {}", "method")));
    }

    #[tokio::test]
    async fn test_fibonacci_function() {
        let src = r#"
            function Fib(n: int) -> int {
                let a = 0;
                let b = 1;
                while (n > 0) {
                    n -= 1;
                    let t = a + b;
                    b = a;
                    a = t;
                }
                a
            }
        "#;

        let (thir, fib_call) = thir_from_src(src, "Fib(5)");

        let result = interpret_thir_ignoring_watch(
            thir,
            fib_call,
            mock_llm_function,
            BamlMap::new(),
            HashMap::new(),
        )
        .await
        .unwrap();

        match result {
            BamlValueWithMeta::Int(actual, _) => {
                assert_eq!(actual, 5);
            }
            v => {
                panic!("Expected int result, got {v:?}");
            }
        }
    }

    #[tokio::test]
    async fn test_bool_to_int_with_if_else() {
        // Test if (true) { 1 } else { 0 }
        let (thir, if_expr_true) = thir_from_src("", "if (true) { 1 } else { 0 }");

        let result = interpret_thir_ignoring_watch(
            thir,
            if_expr_true,
            mock_llm_function,
            BamlMap::new(),
            HashMap::new(),
        )
        .await
        .unwrap();

        match result {
            BamlValueWithMeta::Int(actual, _) => {
                assert_eq!(actual, 1, "if (true) should return 1, got {actual}");
            }
            v => panic!("Expected int result for if (true), got {v:?}"),
        }

        // Test if (false) { 1 } else { 0 }
        let (thir, if_expr_false) = thir_from_src("", "if (false) { 1 } else { 0 }");

        let result = interpret_thir_ignoring_watch(
            thir,
            if_expr_false,
            mock_llm_function,
            BamlMap::new(),
            HashMap::new(),
        )
        .await
        .unwrap();

        match result {
            BamlValueWithMeta::Int(actual, _) => {
                assert_eq!(actual, 0, "if (false) should return 0, got {actual}");
            }
            v => panic!("Expected int result for if (false), got {v:?}"),
        }
    }

    #[tokio::test]
    async fn test_if_else_with_function_equivalent() {
        let src = r#"
            function BoolToIntWithIfElse(b: bool) -> int {
                let result = if (b) { 1 } else { 0 };
                result
            }
        "#;

        // Test with true
        let (thir, call_true) = thir_from_src(src, "BoolToIntWithIfElse(true)");

        let result = interpret_thir_ignoring_watch(
            thir,
            call_true,
            mock_llm_function,
            BamlMap::new(),
            HashMap::new(),
        )
        .await
        .unwrap();

        match result {
            BamlValueWithMeta::Int(actual, _) => {
                assert_eq!(
                    actual, 1,
                    "BoolToIntWithIfElse(true) should return 1, got {actual}"
                );
            }
            v => panic!("Expected int result for BoolToIntWithIfElse(true), got {v:?}"),
        }

        // Test with false
        let (thir, call_false) = thir_from_src(src, "BoolToIntWithIfElse(false)");

        let result = interpret_thir_ignoring_watch(
            thir,
            call_false,
            mock_llm_function,
            BamlMap::new(),
            HashMap::new(),
        )
        .await
        .unwrap();

        match result {
            BamlValueWithMeta::Int(actual, _) => {
                assert_eq!(
                    actual, 0,
                    "BoolToIntWithIfElse(false) should return 0, got {actual}"
                );
            }
            v => panic!("Expected int result for BoolToIntWithIfElse(false), got {v:?}"),
        }
    }

    #[tokio::test]
    async fn test_store_fn_call_in_local_var() {
        let src = r#"
            function ReturnNumber(n: int) -> int {
                n
            }

            function StoreFnCallInLocalVar(n: int) -> int {
                let result = ReturnNumber(n);
                result
            }
        "#;

        // Test with value 42
        let (thir, call_expr) = thir_from_src(src, "StoreFnCallInLocalVar(42)");

        let result = interpret_thir_ignoring_watch(
            thir,
            call_expr,
            mock_llm_function,
            BamlMap::new(),
            HashMap::new(),
        )
        .await
        .unwrap();

        match result {
            BamlValueWithMeta::Int(actual, _) => {
                assert_eq!(
                    actual, 42,
                    "StoreFnCallInLocalVar(42) should return 42, got {actual}"
                );
            }
            v => panic!("Expected int result for StoreFnCallInLocalVar(42), got {v:?}"),
        }
    }

    #[tokio::test]
    async fn test_declare_and_assign_exactly_like_thir() {
        let src = r#"
            function AssignElseIfExpr(a: bool, b: bool) -> int {
                let result = if (a) { 1 } else if (b) { 2 } else { 3 };
                result
            }
        "#;

        // Test with (true, false) - should return 1
        let (thir, call_expr) = thir_from_src(src, "AssignElseIfExpr(true, false)");

        let result = interpret_thir_ignoring_watch(
            thir,
            call_expr,
            mock_llm_function,
            BamlMap::new(),
            HashMap::new(),
        )
        .await
        .unwrap();

        match result {
            BamlValueWithMeta::Int(actual, _) => {
                assert_eq!(
                    actual, 1,
                    "AssignElseIfExpr(true, false) should return 1, got {actual}"
                );
            }
            v => panic!("Expected int result for AssignElseIfExpr(true, false), got {v:?}"),
        }
    }

    #[tokio::test]
    async fn test_compile_real_baml_to_thir() {
        // Test compiling real BAML code to see what THIR is actually generated
        use internal_baml_diagnostics::Diagnostics;
        use internal_baml_parser_database::parse_and_diagnostics;

        use crate::{hir::Hir, thir::typecheck::typecheck};

        let baml_code = r#"
            function AssignElseIfExpr(a: bool, b: bool) -> int {
                let result = if (a) {
                    1
                } else if (b) {
                    2
                } else {
                    3
                };

                result
            }
        "#;

        // Parse BAML code to AST
        let (db, parse_diagnostics) = parse_and_diagnostics(baml_code)
            .unwrap_or_else(|_| panic!("Failed to parse BAML {}", "code"));

        if parse_diagnostics.has_errors() {
            let errors = parse_diagnostics.to_pretty_string();
            panic!("Parse errors: {errors}");
        }

        let ast = db.ast().clone();

        // Convert AST to HIR
        let hir = Hir::from_ast(&ast);

        // Convert HIR to THIR
        let mut diagnostics = Diagnostics::new("test".into());
        let thir = typecheck(&hir, &mut diagnostics);

        if diagnostics.has_errors() {
            let errors = diagnostics.to_pretty_string();
            panic!("Compilation errors: {errors}");
        }

        // Test the function by calling it
        let (thir, call_expr) = thir_from_src(baml_code, "AssignElseIfExpr(true, false)");

        let result = interpret_thir_ignoring_watch(
            thir,
            call_expr,
            mock_llm_function,
            BamlMap::new(),
            HashMap::new(),
        )
        .await;

        match result {
            Ok(BamlValueWithMeta::Int(actual, _)) => {
                assert_eq!(
                    actual, 1,
                    "AssignElseIfExpr(true, false) should return 1, got {actual}"
                );
            }
            Ok(v) => panic!("Expected int result, got {v:?}"),
            Err(e) => {
                println!("❌ Real BAML compilation test failed with error: {e}");
                // This might be the actual bug we need to fix
                panic!("Function failed to execute: {e}");
            }
        }
    }

    #[tokio::test]
    async fn test_debug_bool_to_int_with_if_else() {
        // Debug the BoolToIntWithIfElse function that's returning None
        use internal_baml_diagnostics::Diagnostics;
        use internal_baml_parser_database::parse_and_diagnostics;

        use crate::{hir::Hir, thir::typecheck::typecheck};

        let baml_code = r#"
            function BoolToIntWithIfElse(b: bool) -> int {
                let result = if (b) { 1 } else { 0 };
                result
            }
        "#;

        // Parse and compile BAML code
        let (db, parse_diagnostics) = parse_and_diagnostics(baml_code)
            .unwrap_or_else(|_| panic!("Failed to parse BAML {}", "code"));

        if parse_diagnostics.has_errors() {
            let errors = parse_diagnostics.to_pretty_string();
            panic!("Parse errors: {errors}");
        }

        let ast = db.ast().clone();
        let hir = Hir::from_ast(&ast);
        let mut diagnostics = Diagnostics::new("test".into());
        let thir = typecheck(&hir, &mut diagnostics);

        if diagnostics.has_errors() {
            let errors = diagnostics.to_pretty_string();
            panic!("Compilation errors: {errors}");
        }

        // Test with true
        let (thir, call_expr) = thir_from_src(baml_code, "BoolToIntWithIfElse(true)");

        let result = interpret_thir_ignoring_watch(
            thir,
            call_expr,
            mock_llm_function,
            BamlMap::new(),
            HashMap::new(),
        )
        .await;

        match result {
            Ok(value) => {
                println!("Result: {value:?}");
                match value {
                    BamlValueWithMeta::Int(actual, _) => {
                        assert_eq!(actual, 1, "Expected 1, got {actual}");
                    }
                    _ => panic!("Expected int result, got {value:?}"),
                }
            }
            Err(e) => {
                panic!("Function failed: {e}");
            }
        }

        println!("✅ BoolToIntWithIfElse debug test passed!");
    }

    #[tokio::test]
    async fn test_iterative_fibonacci() {
        // Test the iterative Fibonacci function implementation
        use internal_baml_diagnostics::Diagnostics;
        use internal_baml_parser_database::parse_and_diagnostics;

        use crate::{hir::Hir, thir::typecheck::typecheck};

        // function IterativeFibonacci(n: int) -> int {
        //     let a = 0;
        //     let b = 1;
        //
        //     if (n == 0) {
        //         b
        //     } else {
        //         let i = 1;
        //         while (i <= n) {
        //             let c = a + b;
        //             a = b;
        //             b = c;
        //             i += 1;
        //         }
        //         a
        //     }
        // }

        let baml_code = r#"
            function IterativeFibonacci(n: int) -> int {
                let a = 0;
                let b = 1;

                if (n == 0) {
                    b
                } else {
                    let i = 1;
                    while (i <= n) {
                        let c = a + b;
                        a = b;
                        b = c;
                        i += 1;
                    }
                    a
                }
            }
        "#;

        let src = baml_code;

        let (thir, call_expr) = thir_from_src(src, "IterativeFibonacci(5)");

        let result = interpret_thir_ignoring_watch(
            thir,
            call_expr,
            mock_llm_function,
            BamlMap::new(),
            HashMap::new(),
        )
        .await
        .unwrap();

        match result {
            BamlValueWithMeta::Int(actual, _) => {
                assert_eq!(actual, 5);
            }
            v => {
                panic!("Expected int result, got {v:?}");
            }
        }
    }
}
