use std::{collections::HashMap, ops::Deref};

use anyhow::Result;
use neon::prelude::*;

mod api_wrapper;
// mod example;
mod otel;
use once_cell::sync::OnceCell;

use otel::span_events::IOEvent;

pub use otel::init_tracer as foo;

// use otel::tracer::{BamlSpanContextManager, FunctionArgs};
use tokio::runtime::Runtime;
use tracing::{info, span::Entered, warn, Instrument};

// Return a global tokio runtime or create one if it doesn't exist.
// Throws a JavaScript exception if the `Runtime` fails to create.
fn runtime<'a, C: Context<'a>>(cx: &mut C) -> NeonResult<&'static Runtime> {
    static RUNTIME: OnceCell<Runtime> = OnceCell::new();

    RUNTIME.get_or_try_init(|| Runtime::new().or_else(|err| cx.throw_error(err.to_string())))
}

fn init_tracer(mut cx: FunctionContext) -> JsResult<JsUndefined> {
    otel::init_tracer();
    Ok(cx.undefined())
}

fn stop_tracer(mut cx: FunctionContext) -> JsResult<JsUndefined> {
    match otel::flush_tracer() {
        Ok(_) => Ok(cx.undefined()),
        Err(e) => cx.throw_error(e.to_string()),
    }
}

fn load_json_stringify<'a, C: Context<'a>>(cx: &mut C) -> JsResult<'a, JsFunction> {
    // Get the global object.
    let json = cx.global::<JsObject>("JSON")?;

    // Get the stringify function from the JSON object.
    json.get(cx, "stringify")
}

fn native_stringify<'a, C: Context<'a>>(
    cx: &mut C,
    stringify_fnx: Handle<JsFunction>,
    value: Handle<JsValue>,
) -> Result<String> {
    let this = cx.undefined();
    match stringify_fnx.call(cx, this, vec![value]) {
        Ok(res) => match res.downcast::<JsString, _>(cx) {
            Ok(s) => Ok(s.value(cx)),
            Err(e) => Err(anyhow::anyhow!(
                "Failed to downcast result to string: {}",
                e
            )),
        },
        Err(e) => Err(anyhow::anyhow!("Failed to call JSON.stringify: {}", e)),
    }
}

pub type ArgAndType = (String, String);
pub type NamedArgs = HashMap<String, ArgAndType>;
pub type PositionalArgs = Vec<(Option<String>, ArgAndType)>;
pub type FunctionArgs = (PositionalArgs, NamedArgs);

fn serialize_args<'a, C: Context<'a>>(
    cx: &mut C,
    as_kwargs: bool,
    arg_definition: &Vec<(String, String)>,
    args: &Vec<Handle<'_, JsValue>>,
    stringify: Handle<'_, JsFunction>,
) -> Result<FunctionArgs> {
    if as_kwargs {
        if !args.len() == 1 {
            Err(anyhow::anyhow!(
                "Expected a single object argument when as_kwargs is true"
            ))
        } else {
            let kwargs = match args[0].downcast::<JsObject, _>(cx) {
                Ok(obj) => obj,
                Err(e) => {
                    return Err(anyhow::anyhow!(
                        "Expected a single object argument when as_kwargs is true: {}",
                        e
                    ))
                }
            };

            // Reach every key-value pair in the object and serialize it
            let named_args = arg_definition
                .iter()
                .map(
                    |(arg_name, arg_type)| match kwargs.get(cx, arg_name.as_str()) {
                        Ok(arg) => match native_stringify(cx, stringify, arg) {
                            Ok(serialized_arg) => {
                                (arg_name.clone(), (serialized_arg, arg_type.clone()))
                            }
                            Err(e) => {
                                warn!("Failed to serialize argument: {}", e);
                                (
                                    arg_name.clone(),
                                    ("<serialization-error>".to_string(), arg_type.clone()),
                                )
                            }
                        },
                        Err(_e) => (arg_name.clone(), ("null".to_string(), arg_type.clone())),
                    },
                )
                .collect();

            Ok((vec![], named_args))
        }
    } else {
        let positional_args = args
            .iter()
            .enumerate()
            .zip(
                arg_definition
                    .iter()
                    .map(|(arg_name, arg_type)| (Some(arg_name), arg_type))
                    .chain(std::iter::repeat((None, &"any".to_string()))),
            )
            .map(|((i, val), (arg_name, arg_type))| {
                let arg_name = Some(arg_name.map(|a| a.clone()).unwrap_or(format!("arg_{}", i)));
                match native_stringify(cx, stringify, *val) {
                    Ok(serialized_arg) => (arg_name, (serialized_arg, arg_type.clone())),
                    Err(e) => {
                        warn!("Failed to serialize argument: {}", e);
                        (
                            arg_name,
                            ("<serialization-error>".to_string(), arg_type.clone()),
                        )
                    }
                }
            })
            .collect();

        Ok((positional_args, HashMap::new()))
    }
}

use std::sync::Arc;

fn args_to_native<'a>(
    cx: &'a mut FunctionContext,
    arg_index: usize,
) -> Result<(bool, Vec<(String, String)>, String), neon::result::Throw> {
    let arg_definitions = cx.argument::<JsArray>(arg_index)?.to_vec(cx)?;
    let as_kwargs = cx.argument::<JsBoolean>(arg_index + 1)?.value(cx);
    let return_type = cx.argument::<JsString>(arg_index + 2)?.value(cx);

    // Convert this to native rust types
    let mut args_native = vec![];
    for i in arg_definitions.iter() {
        let arg = match i.downcast::<JsObject, _>(cx) {
            Ok(arg) => arg,
            Err(e) => {
                return cx.throw_error(format!("Failed to downcast argument to object: {}", e));
            }
        };
        let arg_type: Handle<'_, JsString> = arg.get(cx, "type")?;
        let arg_type = arg_type.value(cx);
        let arg_name: Handle<'_, JsString> = arg.get(cx, "name")?;
        let arg_name = arg_name.value(cx);
        args_native.push((arg_name, arg_type));
    }

    Ok((as_kwargs, args_native, return_type))
}

struct BamlSpanOwner {
    span: tracing::Span,
}

impl BamlSpanOwner {
    fn new(function_name: &str) -> Self {
        Self {
            span: baml_span!(function_name),
        }
    }

    fn enter(&self) -> Entered<'_> {
        self.span.enter()
    }

    fn child_of(&self, name: &str) -> Self {
        let _guard = self.enter();
        let span = baml_span!(name);
        Self { span }
    }
}
impl Finalize for BamlSpanOwner {}

fn get_span(mut cx: FunctionContext) -> JsResult<JsBox<BamlSpanOwner>> {
    let name = cx.argument::<JsString>(0)?.value(&mut cx);
    // See if we have a parent span
    let parent = match cx
        .argument_opt(1)
        .map(|a| a.downcast::<JsBox<BamlSpanOwner>, _>(&mut cx))
    {
        Some(Ok(parent)) => Some(parent),
        Some(Err(_)) => None,
        None => None,
    };
    match parent {
        Some(parent) => {
            let res = cx.boxed(parent.child_of(name.as_str()));
            Ok(res)
        }
        None => {
            let res = cx.boxed(BamlSpanOwner::new(name.as_str()));
            Ok(res)
        }
    }
}

fn run_with_trace_async(mut o_cx: FunctionContext) -> JsResult<JsArray> {
    let fnx = o_cx.argument::<JsFunction>(0)?.root(&mut o_cx);
    let (as_kwargs, args_native, return_type) = args_to_native(&mut o_cx, 1)?;

    let log_error = JsFunction::new(&mut o_cx, move |mut cx| {
        let span = cx.argument::<JsBox<BamlSpanOwner>>(0)?;
        let error_code = cx.argument::<JsValue>(1)?;
        let error_code = match error_code.downcast::<JsNumber, _>(&mut cx) {
            Ok(n) => n.value(&mut cx) as i32,
            Err(_) => -2,
        };
        let error_message = cx.argument::<JsString>(2)?.value(&mut cx);
        let traceback = match cx.argument_opt(3) {
            Some(traceback) => match traceback.downcast::<JsString, _>(&mut cx) {
                Ok(s) => Some(s.value(&mut cx)),
                Err(_) => None,
            },
            None => None,
        };
        let error_message = Some(error_message.as_str());
        let traceback = traceback.as_deref();

        let _guard = span.enter();

        // let traceback = error.
        baml_event!(Exception, error_code, error_message, traceback);

        Ok(cx.undefined())
    })?;

    let log_return_type = JsFunction::new(&mut o_cx, move |mut cx| {
        let span = cx.argument::<JsBox<BamlSpanOwner>>(0)?;
        let result = cx.argument::<JsValue>(1)?;
        let _guard = span.enter();

        let json_stringify = load_json_stringify(&mut cx)?;
        match native_stringify(&mut cx, json_stringify, result) {
            Ok(s) => {
                IOEvent::output_event(&s, &return_type);
            }
            Err(e) => {
                IOEvent::output_event(&e.to_string(), &"error".to_string());
            }
        };

        Ok(cx.undefined())
    })?;

    let log_input_args = JsFunction::new(&mut o_cx, move |mut cx| {
        let span = cx.argument::<JsBox<BamlSpanOwner>>(0)?;
        let args = cx.argument::<JsArray>(1)?.root(&mut cx);
        let _guard: Entered<'_> = span.enter();

        let json_stringify = load_json_stringify(&mut cx)?;
        let args = args.to_inner(&mut cx).to_vec(&mut cx)?;
        match serialize_args(&mut cx, as_kwargs, &args_native, &args, json_stringify) {
            Ok(args) => {
                IOEvent::input_event(&args);
            }
            Err(e) => {
                return cx.throw_error(e.to_string());
            }
        };

        Ok(cx.undefined())
    })?;

    /*
     * Rust seems to lose the AsyncLocalStorage context required to properly do tracing.
     * Becaues of this, we instead let typescript handle actually calling the function, and rust just exposes helper functions to propagate the inputs, outputs of calling those functions, etc.
     */
    let function_arrays = vec![log_input_args, log_return_type, log_error];
    let js_array = JsArray::new(&mut o_cx, function_arrays.len());
    for (i, f) in function_arrays.iter().enumerate() {
        js_array.set(&mut o_cx, i as u32, f.clone())?;
    }
    Ok(js_array)

    // // o_cx is outer_context, pointer to JS engine
    // JsFunction::new(&mut o_cx, move |mut cx| {
    //     let fnx = fnx.clone(&mut cx);
    //     let json_stringify_root = json_stringify.clone(&mut cx);
    //     let current_span = cx.argument::<JsBox<BamlSpanOwner>>(0)?.my_clone();
    //     let args = cx.argument::<JsArray>(1)?.root(&mut cx);

    //     let (deferred, promise) = cx.promise();
    //     let rt = runtime(&mut cx)?;
    //     let channel = cx.channel();

    //     let args_native = args_native.clone();
    //     let return_type = return_type.clone();

    //     // Get the current span
    //     let _outer_span = tracing::info_span!("outer").entered();
    //     rt.spawn(async move {
    //         let _mid_span = tracing::info_span!("current_span");
    //         let response = channel.send(move |mut cx: TaskContext<'_>| {
    //             let _e = _mid_span.enter();
    //             let span = current_span.my_clone();
    //             let _entered = span.enter();

    //             let json_stringify: Handle<'_, JsFunction> = json_stringify_root.to_inner(&mut cx);
    //             let fnx = fnx.to_inner(&mut cx);
    //             let args = args.to_inner(&mut cx).to_vec(&mut cx)?;
    //             let this = cx.undefined();

    //             let serialized_args =
    //                 match serialize_args(&mut cx, as_kwargs, &args_native, &args, json_stringify) {
    //                     Ok(args) => args,
    //                     Err(err) => return cx.throw_error(err.to_string()),
    //                 };

    //             IOEvent::input_event(&serialized_args);

    //             let cb: Handle<JsPromise> =
    //                 fnx.call(&mut cx, this, &args)?.downcast_or_throw(&mut cx)?;
    //             let future = cb.to_future(&mut cx, move |mut cx, result| {
    //                 match result {
    //                     Ok(result) => {
    //                         let json_stringify = json_stringify_root.to_inner(&mut cx);
    //                         let serialized_result =
    //                             match native_stringify(&mut cx, json_stringify, result) {
    //                                 Ok(s) => s,
    //                                 Err(e) => {
    //                                     warn!("Failed to serialize result: {}", e);
    //                                     "null".to_string()
    //                                 }
    //                             };
    //                         let span = current_span.my_clone();
    //                         let _guard = span.enter();
    //                         IOEvent::output_event(&serialized_result, &return_type);
    //                         let _ = deferred.resolve(&mut cx, result);
    //                     }
    //                     Err(e) => {
    //                         let _ = deferred.reject(&mut cx, e);
    //                     }
    //                 }
    //                 Ok(())
    //             })?;
    //             Ok(future)
    //         });

    //         match response.await {
    //             Ok(f) => match f.await {
    //                 Ok(_) => {}
    //                 Err(_e) => {
    //                     // let _ = deferred.reject(&mut cx, cx.string("test"));
    //                 }
    //             },
    //             Err(_e) => {
    //                 // let _ = deferred.reject(&mut cx, cx.string("test"));
    //             }
    //         }
    //     });

    //     Ok(promise)
    // })
}

fn run_with_trace(mut o_cx: FunctionContext) -> JsResult<JsFunction> {
    let fnx = o_cx.argument::<JsFunction>(0)?.root(&mut o_cx);
    let name = o_cx.argument::<JsString>(1)?.value(&mut o_cx);
    let (as_kwargs, args_native, return_type) = args_to_native(&mut o_cx, 2)?;

    let args_native = Arc::new(args_native);

    // Call the function with the arguments and return the result.
    JsFunction::new(&mut o_cx, move |mut cx| {
        let fnx = fnx.to_inner(&mut cx);

        let args = cx.argument::<JsArray>(0)?.to_vec(&mut cx)?;

        let json_stringify = load_json_stringify(&mut cx)?;
        // Serialize each argument to a string.
        let serialized_args =
            match serialize_args(&mut cx, as_kwargs, &args_native, &args, json_stringify) {
                Ok(args) => args,
                Err(err) => return cx.throw_error(err.to_string()),
            };

        let scope = baml_span!(name.as_str());
        let _entered = scope.enter();

        IOEvent::input_event(&serialized_args);
        let this = cx.undefined();
        let result = fnx.call(&mut cx, this, &args)?;
        let serialized_result = match native_stringify(&mut cx, json_stringify, result) {
            Ok(s) => s,
            Err(e) => {
                warn!("Failed to serialize result: {}", e);
                "null".to_string()
            }
        };
        IOEvent::output_event(&serialized_result, &return_type);

        Ok(result)
    })
}

fn set_variant(mut cx: FunctionContext) -> JsResult<JsUndefined> {
    let (span, variant) = match cx.len() {
        1 => {
            let variant = cx.argument::<JsString>(0)?.value(&mut cx);
            (None, variant)
        }
        2 => {
            let span = cx.argument::<JsBox<BamlSpanOwner>>(0)?;
            let variant = cx.argument::<JsString>(1)?.value(&mut cx);
            (Some(span), variant)
        }
        _ => {
            return cx.throw_error("Expected 1 or 2 arguments");
        }
    };

    let variant = variant.as_str();

    match span {
        Some(span) => {
            let _guard = span.enter();
            match baml_event!(Variant, variant) {
                Ok(_) => Ok(cx.undefined()),
                Err(e) => cx.throw_error(e.to_string()),
            }
        }
        None => match baml_event!(Variant, variant) {
            Ok(_) => Ok(cx.undefined()),
            Err(e) => cx.throw_error(e.to_string()),
        },
    }
}

fn set_tags(mut cx: FunctionContext) -> JsResult<JsUndefined> {
    let (span, kv_map) = match cx.len() {
        1 => {
            let kv_map = cx.argument::<JsObject>(0)?;
            (None, kv_map)
        }
        2 => {
            let span = cx.argument::<JsBox<BamlSpanOwner>>(0)?;
            let kv_map = cx.argument::<JsObject>(1)?;
            (Some(span), kv_map)
        }
        _ => {
            return cx.throw_error("Expected 1 or 2 arguments");
        }
    };
    let mut tags = HashMap::new();

    let keys = kv_map
        .get_own_property_names(&mut cx)?
        .to_vec(&mut cx)?
        .iter()
        .filter_map(|key| {
            key.downcast::<JsString, _>(&mut cx)
                .map(|s| s.value(&mut cx))
                .ok()
        })
        .collect::<Vec<String>>();

    for key in keys {
        let value = kv_map.get_value(&mut cx, key.as_str())?;
        // If value is null, set it to None
        if value.is_a::<JsNull, _>(&mut cx) {
            tags.insert(key, None);
        } else if value.is_a::<JsString, _>(&mut cx) {
            let as_str = match value.downcast::<JsString, _>(&mut cx) {
                Ok(s) => s.value(&mut cx),
                Err(e) => {
                    warn!("Failed to downcast value to string: {}", e);
                    continue;
                }
            };
            tags.insert(key, Some(as_str));
        } else {
            let as_str = match value.to_string(&mut cx) {
                Ok(s) => s.value(&mut cx),
                Err(e) => {
                    warn!("Failed to convert value to string: {}", e);
                    continue;
                }
            };
            tags.insert(key, Some(as_str));
        }
    }

    let tags = &tags;
    match span {
        Some(span) => {
            let _guard = span.enter();
            match baml_event!(SetTags, tags) {
                Ok(_) => Ok(cx.undefined()),
                Err(e) => cx.throw_error(e.to_string()),
            }
        }
        None => match baml_event!(SetTags, tags) {
            Ok(_) => Ok(cx.undefined()),
            Err(e) => cx.throw_error(e.to_string()),
        },
    }
}

fn set_llm_event(mut cx: FunctionContext) -> JsResult<JsUndefined> {
    let span = cx.argument::<JsBox<BamlSpanOwner>>(0)?;
    let event = cx.argument::<JsObject>(1)?;

    let name: Handle<JsString> = event
        .get_value(&mut cx, "name")?
        .downcast_or_throw(&mut cx)?;
    let name = name.value(&mut cx);

    let meta: Handle<JsString> = event
        .get_value(&mut cx, "meta")?
        .downcast_or_throw(&mut cx)?;

    let meta = meta.value(&mut cx);

    let _guard = span.enter();
    match otel::log_event(name.as_str(), meta.as_str()) {
        Ok(_) => Ok(cx.undefined()),
        Err(e) => cx.throw_error(e.to_string()),
    }
}

#[neon::main]
fn main(mut cx: ModuleContext) -> NeonResult<()> {
    cx.export_function("initTracer", init_tracer)?;
    cx.export_function("stopTracer", stop_tracer)?;
    cx.export_function("trace", run_with_trace)?;
    cx.export_function("traceAsync", run_with_trace_async)?;
    cx.export_function("getSpanForAsync", get_span)?;
    cx.export_function("setTags", set_tags)?;
    cx.export_function("setVariant", set_variant)?;
    cx.export_function("logLLMEvent", set_llm_event)?;

    Ok(())
}
