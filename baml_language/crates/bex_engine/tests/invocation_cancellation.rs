//! Cancellation inputs at the runtime invocation boundary. These tests use
//! the generated `CancelToken` class and its complete native source set.
mod common;

use std::sync::{Arc, OnceLock};

use bex_engine::{
    BexEngine, BexExternalValue, CallId, CancellationToken, EngineError,
    FunctionCallContextBuilder, InheritedInvocationState,
};
use bex_external_types::Handle;
use sys_native::SysOpsExt;

fn engine() -> Arc<BexEngine> {
    static PROGRAM: OnceLock<bex_vm_types::Program> = OnceLock::new();
    let program = PROGRAM.get_or_init(|| {
        common::compile_for_engine(
            r#"
        function main() -> int { 42 }
        function wait() -> int {
            baml.sys.sleep(baml.time.Duration.from_seconds(60));
            42
        }
        function recover() -> int {
            { baml.sys.sleep(baml.time.Duration.from_seconds(60)); 0 }
                catch (e) { baml.panics.Cancelled => 42 }
        }
        function make_token() -> baml.spawn.CancelToken { baml.spawn.CancelToken.new() }
        function query_method(token: baml.spawn.CancelToken) -> () -> bool throws never {
            token.is_cancelled
        }
    "#,
        )
    });
    Arc::new(
        BexEngine::new(
            program.clone(),
            Arc::new(sys_ops::SysOps::native()),
            Vec::new(),
        )
        .unwrap(),
    )
}

async fn token(engine: &Arc<BexEngine>) -> Handle {
    let value = engine
        .call_function(
            "make_token",
            vec![],
            FunctionCallContextBuilder::new(CallId::next()).build(),
            false,
        )
        .await
        .unwrap();
    let BexExternalValue::Handle(handle) = value else {
        panic!("expected token handle")
    };
    handle
}

async fn control(
    engine: &Arc<BexEngine>,
    method: &str,
    args: Vec<BexExternalValue>,
    inherited: Option<InheritedInvocationState>,
    copy: bool,
) -> Result<BexExternalValue, EngineError> {
    let mut context = FunctionCallContextBuilder::new(CallId::next());
    if let Some(state) = inherited {
        context = context.with_inherited_state(state);
    }
    engine
        .call_function(
            &format!("baml.spawn.CancelToken.{method}"),
            args,
            context.build(),
            copy,
        )
        .await
}

async fn composite(engine: &Arc<BexEngine>, tokens: &[Handle]) -> Handle {
    let value = control(
        engine,
        "any",
        vec![BexExternalValue::Array {
            element_type: bex_engine::RuntimeTy::Unknown,
            items: tokens
                .iter()
                .cloned()
                .map(BexExternalValue::Handle)
                .collect(),
        }],
        None,
        false,
    )
    .await
    .unwrap();
    let BexExternalValue::Handle(handle) = value else {
        panic!("expected composite handle")
    };
    handle
}

async fn is_cancelled(engine: &Arc<BexEngine>, token: &Handle) -> bool {
    control(
        engine,
        "is_cancelled",
        vec![BexExternalValue::Handle(token.clone())],
        None,
        true,
    )
    .await
    .unwrap()
        == BexExternalValue::Bool(true)
}

fn assert_cancelled(result: Result<BexExternalValue, EngineError>) {
    let Err(EngineError::UnhandledThrow { value, .. }) = result else {
        panic!("expected cancellation, got {result:?}")
    };
    let BexExternalValue::Instance { class_name, .. } = *value else {
        panic!("expected panic")
    };
    assert_eq!(class_name, bex_engine::CANCELLED_PANIC_CLASS);
}

async fn wait_for_admission(engine: &BexEngine, id: CallId) -> InheritedInvocationState {
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        loop {
            if let Ok(state) = engine.invocation_state(id) {
                return state;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("invocation did not admit")
}

fn start_wait(
    engine: &Arc<BexEngine>,
    id: CallId,
    tokens: Vec<Handle>,
    inherited: Option<InheritedInvocationState>,
) -> tokio::task::JoinHandle<Result<BexExternalValue, EngineError>> {
    let engine = Arc::clone(engine);
    tokio::spawn(async move {
        let mut context = FunctionCallContextBuilder::new(id).with_cancel_tokens(tokens);
        if let Some(state) = inherited {
            context = context.with_inherited_state(state);
        }
        engine
            .call_function("wait", vec![], context.build(), true)
            .await
    })
}

#[tokio::test]
async fn shared_token_cancelling_one_invocation_preserves_token_and_sibling() {
    let engine = engine();
    let token = token(&engine).await;
    let first_id = CallId::next();
    let sibling_id = CallId::next();
    let first = start_wait(&engine, first_id, vec![token.clone()], None);
    let sibling = start_wait(&engine, sibling_id, vec![token.clone()], None);
    wait_for_admission(&engine, first_id).await;
    wait_for_admission(&engine, sibling_id).await;
    engine.cancel_function_call(first_id).unwrap();
    assert_cancelled(first.await.unwrap());
    assert!(!is_cancelled(&engine, &token).await);
    assert!(!sibling.is_finished());
    engine.cancel_function_call(sibling_id).unwrap();
    assert_cancelled(sibling.await.unwrap());
}

#[tokio::test]
async fn child_cancellation_preserves_parent_and_parent_cancels_descendants() {
    let engine = engine();
    let parent_id = CallId::next();
    let parent = start_wait(&engine, parent_id, vec![], None);
    let state = wait_for_admission(&engine, parent_id).await;
    let child_id = CallId::next();
    let child = start_wait(&engine, child_id, vec![], Some(state.clone()));
    wait_for_admission(&engine, child_id).await;
    engine.cancel_function_call(child_id).unwrap();
    assert_cancelled(child.await.unwrap());
    assert!(!parent.is_finished());
    let descendant_id = CallId::next();
    let descendant = start_wait(&engine, descendant_id, vec![], Some(state));
    wait_for_admission(&engine, descendant_id).await;
    engine.cancel_function_call(parent_id).unwrap();
    assert_cancelled(parent.await.unwrap());
    assert_cancelled(descendant.await.unwrap());
}

#[tokio::test]
async fn nested_any_observes_every_source_without_reverse_cancellation() {
    for source_index in 0..3 {
        let engine = engine();
        let sources = [
            token(&engine).await,
            token(&engine).await,
            token(&engine).await,
        ];
        let inner = composite(&engine, &sources[..2]).await;
        let outer = composite(&engine, &[inner.clone(), sources[2].clone()]).await;
        let id = CallId::next();
        let call = start_wait(&engine, id, vec![outer.clone()], None);
        wait_for_admission(&engine, id).await;
        control(
            &engine,
            "cancel",
            vec![BexExternalValue::Handle(sources[source_index].clone())],
            None,
            true,
        )
        .await
        .unwrap();
        assert_cancelled(call.await.unwrap());
        assert!(is_cancelled(&engine, &outer).await);
        for (index, source) in sources.iter().enumerate() {
            assert_eq!(is_cancelled(&engine, source).await, index == source_index);
        }
    }
}

#[tokio::test]
async fn cancelling_composite_does_not_cancel_its_sources() {
    let engine = engine();
    let a = token(&engine).await;
    let b = token(&engine).await;
    let any = composite(&engine, &[a.clone(), b.clone()]).await;
    control(
        &engine,
        "cancel",
        vec![BexExternalValue::Handle(any.clone())],
        None,
        true,
    )
    .await
    .unwrap();
    assert!(is_cancelled(&engine, &any).await);
    assert!(!is_cancelled(&engine, &a).await);
    assert!(!is_cancelled(&engine, &b).await);
    assert_cancelled(
        engine
            .call_function(
                "main",
                vec![],
                FunctionCallContextBuilder::new(CallId::next())
                    .with_cancel_tokens(vec![any])
                    .build(),
                true,
            )
            .await,
    );
}

#[tokio::test]
async fn token_controls_remain_usable_under_ambient_cancellation() {
    let engine = engine();
    let token = token(&engine).await;
    // Retain the actual bound method before cancellation, like a returned BAML callable.
    let method = engine
        .call_function(
            "query_method",
            vec![BexExternalValue::Handle(token.clone())],
            FunctionCallContextBuilder::new(CallId::next()).build(),
            false,
        )
        .await
        .unwrap();
    let BexExternalValue::Handle(method) = method else {
        panic!("expected bound method")
    };
    let id = CallId::next();
    let parent = start_wait(&engine, id, vec![], None);
    let state = wait_for_admission(&engine, id).await;
    engine.cancel_function_call(id).unwrap();
    assert_cancelled(parent.await.unwrap());
    // The state survives the parent waiter and registration.
    control(&engine, "new", vec![], Some(state.clone()), false)
        .await
        .unwrap();
    let any = control(
        &engine,
        "any",
        vec![BexExternalValue::Array {
            element_type: bex_engine::RuntimeTy::Unknown,
            items: vec![BexExternalValue::Handle(token.clone())],
        }],
        Some(state.clone()),
        false,
    )
    .await
    .unwrap();
    assert!(matches!(any, BexExternalValue::Handle(_)));
    assert_eq!(
        control(
            &engine,
            "is_cancelled",
            vec![BexExternalValue::Handle(token.clone())],
            Some(state.clone()),
            true
        )
        .await
        .unwrap(),
        BexExternalValue::Bool(false)
    );
    assert_eq!(
        engine
            .call_callable(
                method.clone(),
                vec![],
                FunctionCallContextBuilder::new(CallId::next())
                    .with_inherited_state(state.clone())
                    .build(),
                true,
            )
            .await
            .unwrap(),
        BexExternalValue::Bool(false)
    );
    control(
        &engine,
        "cancel",
        vec![BexExternalValue::Handle(token)],
        Some(state.clone()),
        true,
    )
    .await
    .unwrap();
    assert_eq!(
        engine
            .call_callable(
                method,
                vec![],
                FunctionCallContextBuilder::new(CallId::next())
                    .with_inherited_state(state.clone())
                    .build(),
                true,
            )
            .await
            .unwrap(),
        BexExternalValue::Bool(true)
    );
    assert_cancelled(
        engine
            .call_function(
                "main",
                vec![],
                FunctionCallContextBuilder::new(CallId::next())
                    .with_inherited_state(state)
                    .build(),
                true,
            )
            .await,
    );
}

#[tokio::test]
async fn wrong_runtime_or_non_token_cancellation_inputs_fail_before_admission() {
    let engine = engine();
    let other = self::engine();
    let foreign = token(&other).await;
    let not_token = engine
        .call_function(
            "query_method",
            vec![BexExternalValue::Handle(token(&engine).await)],
            FunctionCallContextBuilder::new(CallId::next()).build(),
            false,
        )
        .await
        .unwrap();
    let BexExternalValue::Handle(not_token) = not_token else {
        panic!("expected handle")
    };
    for input in [foreign, not_token] {
        let result = engine
            .call_function(
                "main",
                vec![],
                FunctionCallContextBuilder::new(CallId::next())
                    .with_cancel_tokens(vec![input])
                    .build(),
                true,
            )
            .await;
        assert!(matches!(result, Err(EngineError::TypeMismatch { .. })));
    }
}

#[tokio::test]
async fn explicit_precancelled_source_still_applies_to_token_controls() {
    let engine = engine();
    let cancel = CancellationToken::new();
    cancel.cancel();
    assert_cancelled(
        engine
            .call_function(
                "baml.spawn.CancelToken.new",
                vec![],
                FunctionCallContextBuilder::new(CallId::next())
                    .with_cancel_token(cancel)
                    .build(),
                false,
            )
            .await,
    );
}

#[tokio::test(start_paused = true)]
async fn invocation_timeout_preserves_shared_token_and_sibling() {
    let engine = engine();
    let token = token(&engine).await;
    let first_id = CallId::next();
    let sibling_id = CallId::next();
    let first = {
        let engine = Arc::clone(&engine);
        let token = token.clone();
        tokio::spawn(async move {
            engine
                .call_function(
                    "wait",
                    vec![],
                    FunctionCallContextBuilder::new(first_id)
                        .with_cancel_tokens(vec![token])
                        .with_timeout(std::time::Duration::from_secs(5))
                        .build(),
                    true,
                )
                .await
        })
    };
    let sibling = start_wait(&engine, sibling_id, vec![token.clone()], None);
    wait_for_admission(&engine, first_id).await;
    wait_for_admission(&engine, sibling_id).await;
    tokio::time::advance(std::time::Duration::from_secs(5)).await;
    assert_cancelled(first.await.unwrap());
    assert!(!is_cancelled(&engine, &token).await);
    assert!(!sibling.is_finished());
    engine.cancel_function_call(sibling_id).unwrap();
    assert_cancelled(sibling.await.unwrap());
}

#[tokio::test(start_paused = true)]
async fn callback_reentry_inherits_absolute_deadline_and_cannot_extend_it() {
    let engine = engine();
    let parent_id = CallId::next();
    let parent = {
        let engine = Arc::clone(&engine);
        tokio::spawn(async move {
            engine
                .call_function(
                    "wait",
                    vec![],
                    FunctionCallContextBuilder::new(parent_id)
                        .with_timeout(std::time::Duration::from_secs(5))
                        .build(),
                    true,
                )
                .await
        })
    };
    let callback_state = wait_for_admission(&engine, parent_id).await;
    tokio::time::advance(std::time::Duration::from_secs(2)).await;
    let child_id = CallId::next();
    let child = {
        let engine = Arc::clone(&engine);
        let callback_state = callback_state.clone();
        tokio::spawn(async move {
            engine
                .call_function(
                    "wait",
                    vec![],
                    FunctionCallContextBuilder::new(child_id)
                        .with_inherited_state(callback_state)
                        .with_timeout(std::time::Duration::from_secs(50))
                        .build(),
                    true,
                )
                .await
        })
    };
    let reentered_state = wait_for_admission(&engine, child_id).await;
    let grandchild_id = CallId::next();
    let grandchild = start_wait(&engine, grandchild_id, vec![], Some(reentered_state));
    wait_for_admission(&engine, grandchild_id).await;
    tokio::time::advance(std::time::Duration::from_secs(3)).await;
    assert_cancelled(parent.await.unwrap());
    assert_cancelled(child.await.unwrap());
    assert_cancelled(grandchild.await.unwrap());
    // A callback that exits later still retains the original expired deadline.
    assert_cancelled(
        engine
            .call_function(
                "main",
                vec![],
                FunctionCallContextBuilder::new(CallId::next())
                    .with_inherited_state(callback_state.clone())
                    .build(),
                true,
            )
            .await,
    );
    // Control operations remain available after a deadline, too.
    control(&engine, "new", vec![], Some(callback_state), false)
        .await
        .unwrap();
}

#[tokio::test(start_paused = true)]
async fn child_can_shorten_deadline_without_cancelling_parent() {
    let engine = engine();
    let parent_id = CallId::next();
    let parent = {
        let engine = Arc::clone(&engine);
        tokio::spawn(async move {
            engine
                .call_function(
                    "wait",
                    vec![],
                    FunctionCallContextBuilder::new(parent_id)
                        .with_timeout(std::time::Duration::from_secs(20))
                        .build(),
                    true,
                )
                .await
        })
    };
    let state = wait_for_admission(&engine, parent_id).await;
    let child_id = CallId::next();
    let child = {
        let engine = Arc::clone(&engine);
        let state = state.clone();
        tokio::spawn(async move {
            engine
                .call_function(
                    "wait",
                    vec![],
                    FunctionCallContextBuilder::new(child_id)
                        .with_inherited_state(state)
                        .with_timeout(std::time::Duration::from_secs(5))
                        .build(),
                    true,
                )
                .await
        })
    };
    wait_for_admission(&engine, child_id).await;
    tokio::time::advance(std::time::Duration::from_secs(5)).await;
    assert_cancelled(child.await.unwrap());
    assert!(!parent.is_finished());
    assert_eq!(
        engine
            .call_function(
                "main",
                vec![],
                FunctionCallContextBuilder::new(CallId::next())
                    .with_inherited_state(state)
                    .build(),
                true
            )
            .await
            .unwrap(),
        BexExternalValue::Int(42)
    );
    engine.cancel_function_call(parent_id).unwrap();
    assert_cancelled(parent.await.unwrap());
}

#[tokio::test(start_paused = true)]
async fn runtime_recovery_result_is_authoritative_after_deadline_cancellation() {
    let engine = engine();
    let id = CallId::next();
    let input = CancellationToken::new();
    let call = {
        let engine = Arc::clone(&engine);
        let input = input.clone();
        tokio::spawn(async move {
            engine
                .call_function(
                    "recover",
                    vec![],
                    FunctionCallContextBuilder::new(id)
                        .with_cancel_token(input)
                        .with_timeout(std::time::Duration::from_secs(5))
                        .build(),
                    true,
                )
                .await
        })
    };
    let state = wait_for_admission(&engine, id).await;
    tokio::time::advance(std::time::Duration::from_secs(5)).await;
    assert_eq!(call.await.unwrap().unwrap(), BexExternalValue::Int(42));
    assert!(!input.is_cancelled());
    assert!(matches!(
        engine.invocation_state(id),
        Err(EngineError::FunctionCallNotFound { .. })
    ));
    // Recovery did not un-cancel the retained context, but it did settle the
    // original invocation successfully. A new invocation still rejects it.
    assert_cancelled(
        engine
            .call_function(
                "main",
                vec![],
                FunctionCallContextBuilder::new(CallId::next())
                    .with_inherited_state(state)
                    .build(),
                true,
            )
            .await,
    );
}

#[tokio::test(start_paused = true)]
async fn bound_context_cannot_move_to_a_different_runtime() {
    let engine = engine();
    let other = self::engine();
    let context = engine
        .bind_invocation_context(
            FunctionCallContextBuilder::new(CallId::next())
                .with_timeout(std::time::Duration::from_secs(5))
                .build(),
        )
        .unwrap();
    let result = other.call_function("main", vec![], context, true).await;
    assert!(matches!(result, Err(EngineError::TypeMismatch { .. })));
}
