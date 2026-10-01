//! BEP-81 conformance through the public generated SDK.
use baml_bridge::OptionalArg::Unset;
use baml_sdk::baml::spawn::CancelToken;
use baml_sdk::host_callable_tests as baml;
use baml_sdk::{
    BamlOptions, hello_world, hello_world_async_with_options, hello_world_with_options, invocation,
    trace,
};

mod invocation_options {
    use super::*;
    #[test]
    fn four_call_forms() {
        let options = BamlOptions::new().timeout_ms(1000);
        assert_eq!(
            baml_sdk::optional_args_probe(1, Unset, Unset).unwrap(),
            vec![Some(1), Some(5), Some(99)]
        );
        assert_eq!(
            baml_sdk::optional_args_probe(1, Some(7), Unset).unwrap(),
            vec![Some(1), Some(7), Some(99)]
        );
        assert_eq!(
            baml_sdk::optional_args_probe_with_options(1, Unset, Unset, &options).unwrap(),
            vec![Some(1), Some(5), Some(99)]
        );
        assert_eq!(
            baml_sdk::optional_args_probe_with_options(1, Some(7), Unset, &options).unwrap(),
            vec![Some(1), Some(7), Some(99)]
        );
    }
    #[tokio::test]
    async fn four_call_forms_async() {
        let options = BamlOptions::new().timeout_ms(1000);
        assert_eq!(
            baml_sdk::optional_args_probe_async(1, Unset, Unset)
                .await
                .unwrap(),
            vec![Some(1), Some(5), Some(99)]
        );
        assert_eq!(
            baml_sdk::optional_args_probe_async(1, Some(7), Unset)
                .await
                .unwrap(),
            vec![Some(1), Some(7), Some(99)]
        );
        assert_eq!(
            baml_sdk::optional_args_probe_async_with_options(1, Unset, Unset, &options)
                .await
                .unwrap(),
            vec![Some(1), Some(5), Some(99)]
        );
        assert_eq!(
            baml_sdk::optional_args_probe_async_with_options(1, Some(7), Unset, &options)
                .await
                .unwrap(),
            vec![Some(1), Some(7), Some(99)]
        );
    }
    #[test]
    fn empty_controls() {
        assert_eq!(
            hello_world().unwrap(),
            hello_world_with_options(BamlOptions::default()).unwrap()
        );
    }
    #[test]
    fn omitted_argument_is_not_null() {
        assert_eq!(
            baml_sdk::optional_args_probe_with_options(1, None, Unset, BamlOptions::default())
                .unwrap(),
            vec![Some(1), None, Some(99)]
        );
    }
    #[test]
    fn invalid_timeout_rejected() {
        assert!(hello_world_with_options(BamlOptions::new().timeout_ms(2147483648)).is_err());
    }
    #[test]
    fn timeout_upper_bound_accepted() {
        assert_eq!(
            hello_world_with_options(BamlOptions::new().timeout_ms(2147483647)).unwrap(),
            "hello world"
        );
    }
}
mod invocation_surfaces {
    use super::*;
    #[test]
    fn methods_accept_controls() {
        let value = baml_sdk::OptBox::make_with_options(1, Unset, BamlOptions::default()).unwrap();
        assert_eq!(
            value
                .probe_with_options(2, Unset, BamlOptions::default())
                .unwrap(),
            vec![Some(8), Some(2), Some(5)]
        );
    }
    #[tokio::test]
    async fn methods_accept_controls_async() {
        let value = baml_sdk::OptBox::make_async_with_options(1, Unset, BamlOptions::default())
            .await
            .unwrap();
        assert_eq!(
            value
                .probe_async_with_options(2, Unset, BamlOptions::default())
                .await
                .unwrap(),
            vec![Some(8), Some(2), Some(5)]
        );
    }
    #[test]
    fn returned_callable_accepts_controls() {
        let add = baml::make_adder(3).unwrap();
        assert_eq!(
            add.call_with_options((4,), BamlOptions::default()).unwrap(),
            7
        );
        let token = CancelToken::new().unwrap();
        assert_eq!(add.call_with_options((4,), &token).unwrap(), 7);
    }
    #[tokio::test]
    async fn returned_callable_accepts_controls_async() {
        let add = baml::make_adder_async(3).await.unwrap();
        assert_eq!(
            add.call_async_with_options((4,), BamlOptions::default())
                .await
                .unwrap(),
            7
        );
    }
}
mod invocation_lifecycle {
    use super::*;
    #[test]
    fn pre_cancelled_call_does_not_enter_callback() {
        let token = CancelToken::new().unwrap();
        token.cancel().unwrap();
        assert!(
            baml::call_int_callback_with_options(
                |_| -> i64 { panic!("must not dispatch") },
                1,
                &token
            )
            .is_err()
        );
    }
    #[test]
    fn zero_timeout_does_not_enter_callback() {
        assert!(
            baml::call_int_callback_with_options(
                |_| -> i64 { panic!("must not dispatch") },
                1,
                BamlOptions::new().timeout_ms(0)
            )
            .is_err()
        );
    }
    #[test]
    fn composite_token_observes_every_source() {
        for index in 0..2 {
            let sources = [CancelToken::new().unwrap(), CancelToken::new().unwrap()];
            let token = CancelToken::any(sources.to_vec()).unwrap();
            sources[index].cancel().unwrap();
            assert!(token.is_cancelled().unwrap());
            assert!(!sources[1 - index].is_cancelled().unwrap());
            assert!(hello_world_with_options(&token).is_err());
        }
    }
    #[test]
    fn child_cancellation_does_not_cancel_input() {
        let source = CancelToken::new().unwrap();
        let captured = source.clone();
        let (checked, finished) = std::sync::mpsc::channel();
        let _ = baml::call_int_callback_with_options(
            move |value: i64| {
                let active = invocation::current().unwrap();
                active.cancel().cancel().unwrap();
                assert!(active.cancel().is_cancelled().unwrap());
                assert!(!captured.is_cancelled().unwrap());
                checked.send(()).unwrap();
                value
            },
            1,
            &source,
        );
        finished
            .recv_timeout(std::time::Duration::from_secs(5))
            .unwrap();
        assert!(!source.is_cancelled().unwrap());
    }
    #[test]
    fn reservation_is_single_use() {
        let reserved = trace::hidden().unwrap().reserve().unwrap();
        assert_eq!(hello_world_with_options(&reserved).unwrap(), "hello world");
        assert!(hello_world_with_options(&reserved).is_err());
    }
    #[test]
    fn failed_admission_does_not_consume_reservation() {
        let reserved = trace::hidden().unwrap().reserve().unwrap();
        assert!(
            hello_world_with_options(BamlOptions::new().trace(reserved.clone()).timeout_ms(0))
                .is_err()
        );
        assert_eq!(hello_world_with_options(&reserved).unwrap(), "hello world");
    }
    #[test]
    fn retained_effective_token_observes_late_parent_cancellation() {
        let source = CancelToken::new().unwrap();
        let retained = std::sync::Arc::new(std::sync::Mutex::new(None));
        let output = retained.clone();
        baml::call_int_callback_with_options(
            move |value: i64| {
                *output.lock().unwrap() = invocation::current();
                value
            },
            1,
            &source,
        )
        .unwrap();
        assert!(invocation::current().is_none());
        source.cancel().unwrap();
        let active = retained.lock().unwrap().take().unwrap();
        assert!(active.cancel().is_cancelled().unwrap());
        assert!(active.run(|| hello_world()).is_err());
    }
}
fn context(name: &str) -> trace::Options {
    trace::context(Unset, Some(name.to_owned())).unwrap()
}
mod invocation_inheritance {
    use super::*;
    #[test]
    fn callback_frame_is_installed_and_restored() {
        let options = context("parent");
        let result = baml::call_int_callback_with_options(
            |value: i64| {
                assert!(invocation::current().is_some());
                assert_eq!(
                    trace::current_context().unwrap().distinct_id.as_deref(),
                    Some("parent")
                );
                assert_eq!(
                    baml::call_int_callback(
                        |inner: i64| {
                            assert_eq!(
                                trace::current_context().unwrap().distinct_id.as_deref(),
                                Some("parent")
                            );
                            inner + 1
                        },
                        value
                    )
                    .unwrap(),
                    2
                );
                value
            },
            1,
            &options,
        )
        .unwrap();
        assert_eq!(result, 1);
        assert!(invocation::current().is_none());
        assert!(trace::current_context().unwrap().distinct_id.is_none());
    }
    #[test]
    fn multi_layer_context_patch_inherits_and_restores() {
        let outer = context("A");
        let patch = context("C");
        baml::call_int_callback_with_options(
            move |value: i64| {
                assert_eq!(
                    trace::current_context().unwrap().distinct_id.as_deref(),
                    Some("A")
                );
                let patch = patch.clone();
                baml::call_int_callback(
                    move |value: i64| {
                        assert_eq!(
                            trace::current_context().unwrap().distinct_id.as_deref(),
                            Some("A")
                        );
                        baml::call_int_callback_with_options(
                            |value: i64| {
                                assert_eq!(
                                    trace::current_context().unwrap().distinct_id.as_deref(),
                                    Some("C")
                                );
                                baml::call_int_callback(
                                    |value: i64| {
                                        assert_eq!(
                                            trace::current_context()
                                                .unwrap()
                                                .distinct_id
                                                .as_deref(),
                                            Some("C")
                                        );
                                        value
                                    },
                                    value,
                                )
                                .unwrap()
                            },
                            value,
                            &patch,
                        )
                        .unwrap();
                        assert_eq!(
                            trace::current_context().unwrap().distinct_id.as_deref(),
                            Some("A")
                        );
                        value
                    },
                    value,
                )
                .unwrap();
                assert_eq!(
                    trace::current_context().unwrap().distinct_id.as_deref(),
                    Some("A")
                );
                value
            },
            1,
            &outer,
        )
        .unwrap();
    }
    #[tokio::test(flavor = "multi_thread")]
    async fn ambient_context_survives_await_rust_only() {
        let options = trace::context_async(Unset, Some("async".to_owned()))
            .await
            .unwrap();
        baml::call_int_callback_async_with_options(
            |value: i64| async move {
                assert_eq!(
                    trace::current_context_async()
                        .await
                        .unwrap()
                        .distinct_id
                        .as_deref(),
                    Some("async")
                );
                tokio::task::yield_now().await;
                assert_eq!(
                    trace::current_context_async()
                        .await
                        .unwrap()
                        .distinct_id
                        .as_deref(),
                    Some("async")
                );
                assert!(
                    tokio::spawn(async { invocation::current().is_none() })
                        .await
                        .unwrap()
                );
                let active = invocation::current().unwrap();
                assert_eq!(
                    tokio::spawn(active.scope(async {
                        trace::current_context_async().await.unwrap().distinct_id
                    }))
                    .await
                    .unwrap()
                    .as_deref(),
                    Some("async")
                );
                baml::call_int_callback_async(
                    |child: i64| async move {
                        assert_eq!(
                            trace::current_context_async()
                                .await
                                .unwrap()
                                .distinct_id
                                .as_deref(),
                            Some("async")
                        );
                        child
                    },
                    value,
                )
                .await
                .unwrap()
            },
            1,
            options,
        )
        .await
        .unwrap();
        assert!(invocation::current().is_none());
    }
    #[tokio::test]
    async fn unpolled_future_starts_nothing_rust_only() {
        let call = baml::call_int_callback_async_with_options(
            |_| -> i64 { panic!("unpolled future must not start") },
            1,
            BamlOptions::default(),
        );
        drop(call);
        assert_eq!(
            hello_world_async_with_options(BamlOptions::default())
                .await
                .unwrap(),
            "hello world"
        );
    }
}

mod invocation_dynamic {
    use super::*;
    use baml_sdk::{Arguments, Input, Target, Type, TypeBindings, invoke, invoke_async};
    #[test]
    fn dynamic_name_and_handle_accept_controls() {
        let result = invoke(
            Target::named("user.hello_world"),
            Arguments::new(),
            TypeBindings::new(),
            BamlOptions::default(),
        )
        .unwrap();
        assert_eq!(result.decode::<String>().unwrap(), "hello world");
        let add = baml::make_adder(3).unwrap();
        let args = Arguments::from([("value".into(), Input::new(4i64))]);
        assert_eq!(
            invoke(
                (&add).into(),
                args.clone(),
                TypeBindings::new(),
                BamlOptions::default()
            )
            .unwrap()
            .decode::<i64>()
            .unwrap(),
            7
        );
        assert!(
            invoke(
                (&add).into(),
                args,
                TypeBindings::from([("T".into(), Type::of::<i64>())]),
                BamlOptions::default()
            )
            .is_err()
        );
        assert!(
            invoke(
                Target::named("user.hello_world"),
                Arguments::new(),
                TypeBindings::new(),
                BamlOptions::new().timeout_ms(0)
            )
            .is_err()
        );
    }
    #[tokio::test]
    async fn dynamic_name_and_handle_accept_controls_async() {
        let result = invoke_async(
            Target::named("user.hello_world"),
            Arguments::new(),
            TypeBindings::new(),
            BamlOptions::default(),
        )
        .await
        .unwrap();
        assert_eq!(result.decode::<String>().unwrap(), "hello world");
        let add = baml::make_adder_async(3).await.unwrap();
        assert_eq!(
            invoke_async(
                (&add).into(),
                Arguments::from([("value".into(), Input::new(4i64))]),
                TypeBindings::new(),
                BamlOptions::default()
            )
            .await
            .unwrap()
            .decode::<i64>()
            .unwrap(),
            7
        );
    }
    #[tokio::test(flavor = "multi_thread")]
    async fn dropping_started_future_cancels_rust_only() {
        let (started, ready) = tokio::sync::oneshot::channel();
        let (release, cleanup) = tokio::sync::oneshot::channel();
        let channels = std::sync::Arc::new(std::sync::Mutex::new(Some((started, cleanup))));
        let call = tokio::spawn(baml::call_int_callback_async(
            move |value: i64| {
                let (started, cleanup) = channels.lock().unwrap().take().unwrap();
                async move {
                    let token = invocation::current().unwrap().cancel();
                    started.send(token).unwrap();
                    cleanup.await.unwrap();
                    value
                }
            },
            1,
        ));
        let token = tokio::time::timeout(std::time::Duration::from_secs(5), ready)
            .await
            .unwrap()
            .unwrap();
        call.abort();
        let _ = call.await;
        tokio::time::timeout(std::time::Duration::from_secs(5), async {
            while !token.is_cancelled_async().await.unwrap() {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        release.send(()).unwrap();
    }
}
