use neon::{
    context::{Context, FunctionContext, TaskContext},
    handle::Handle,
    object::Object,
    result::{JsResult, NeonResult},
    types::{JsArray, JsFunction, JsPromise},
};
use once_cell::sync::OnceCell;
use tokio::runtime::Runtime;

// Return a global tokio runtime or create one if it doesn't exist.
// Throws a JavaScript exception if the `Runtime` fails to create.
fn runtime<'a, C: Context<'a>>(cx: &mut C) -> NeonResult<&'static Runtime> {
    static RUNTIME: OnceCell<Runtime> = OnceCell::new();

    RUNTIME.get_or_try_init(|| Runtime::new().or_else(|err| cx.throw_error(err.to_string())))
}

use tracing::{span, Instrument};

use tracing_subscriber::prelude::*;

pub fn init_tracer() {
    tracing_subscriber::registry::Registry::default()
        .with(CustomLayer)
        .init();
}

pub fn create_wrapped_promise(mut o_cx: FunctionContext) -> JsResult<JsFunction> {
    let fnx = o_cx.argument::<JsFunction>(0)?.root(&mut o_cx);
    JsFunction::new(&mut o_cx, move |mut cx| {
        let fnx = fnx.clone(&mut cx);
        let args = cx.argument::<JsArray>(0)?.root(&mut cx);

        let (deferred, promise) = cx.promise();
        let rt = runtime(&mut cx)?;
        let channel = cx.channel();

        let outer_span = tracing::info_span!("outer").entered();

        rt.spawn(
            async move {
                let curr = tracing::info_span!("current_span");

                let response = channel.send(move |mut cx: TaskContext<'_>| {
                    let entered = curr.enter();

                    let fnx = fnx.clone(&mut cx);
                    let args = args.to_inner(&mut cx).to_vec(&mut cx)?;
                    let this = cx.undefined();
                    let cb: Handle<JsPromise> = fnx
                        .into_inner(&mut cx)
                        .call(&mut cx, this, &args)?
                        .downcast_or_throw(&mut cx)?;
                    let future = cb
                        .to_future(&mut cx, move |mut cx, result| {
                            match result {
                                Ok(r) => {
                                    let _ = deferred.resolve(&mut cx, r);
                                }
                                Err(e) => {
                                    let _ = deferred.reject(&mut cx, e);
                                }
                            }
                            Ok(())
                        })?
                        .instrument(tracing::info_span!("wrapped_future"));
                    Ok(future)
                });

                match response.await {
                    Ok(f) => match f.await {
                        Ok(_) => {}
                        Err(e) => {
                            // let _ = deferred.reject(&mut cx, cx.string("test"));
                        }
                    },
                    Err(e) => {
                        // let _ = deferred.reject(&mut cx, cx.string("test"));
                    }
                }
            }
            .instrument(tracing::info_span!("my_future")),
        );

        Ok(promise)
    })
}

use std::{borrow::BorrowMut, time::Instant};
use tracing::span::{Attributes, Id};
use tracing::Subscriber;
use tracing_subscriber::layer::Layer;
use tracing_subscriber::registry::LookupSpan;

struct Timing {
    started_at: Instant,
}

pub struct CustomLayer;

impl<S> Layer<S> for CustomLayer
where
    S: Subscriber,
    S: for<'lookup> LookupSpan<'lookup>,
{
    fn on_new_span(
        &self,
        _attrs: &Attributes<'_>,
        id: &Id,
        ctx: tracing_subscriber::layer::Context<'_, S>,
    ) {
        let span = ctx.span(id).unwrap();

        // Get the parent span
        // let parent = span.extensions().get::<Arc<tracing::Span>>().unwrap();
        if let Some(parent) = span.parent() {
            println!(
                "parent span: {} ({:?})-> {} ({:?})",
                parent.name(),
                parent.id(),
                span.name(),
                span.id()
            );
        } else {
            println!("parent span: null -> {} ({:?})", span.name(), span.id());
        }

        span.extensions_mut().insert(Timing {
            started_at: Instant::now(),
        });
    }

    fn on_event(
        &self,
        event: &tracing::Event<'_>,
        _ctx: tracing_subscriber::layer::Context<'_, S>,
    ) {
        match event.metadata().name() {
            "input" => {
                event.record();
            }
            "output" => {
                println!("output event: {:?}", event.metadata());
            }
            _ => {}
        }
    }

    fn on_close(&self, id: Id, ctx: tracing_subscriber::layer::Context<'_, S>) {
        let span = ctx.span(&id).unwrap();

        let started_at = span.extensions().get::<Timing>().unwrap().started_at;

        println!(
            "span {} took {}",
            span.metadata().name(),
            (Instant::now() - started_at).as_micros(),
        );
    }
}
