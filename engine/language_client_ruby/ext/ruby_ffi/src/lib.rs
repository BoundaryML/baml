use baml_runtime::{BamlRuntime, FunctionResult, RuntimeContext};
use futures::executor::block_on;
use magnus::{
    class, error::RubyUnavailableError, exception::runtime_error, function, method, prelude::*,
    scan_args::get_kwargs, value::Value, Error, RHash, Ruby,
};
use std::collections::HashMap;
use std::path::PathBuf;
use tokio::time::{sleep, Duration};

mod json_to_ruby;
mod ruby_types;

type Result<T> = std::result::Result<T, magnus::Error>;

async fn async_fn() -> String {
    let duration = Duration::from_secs(2);
    println!("async-BEGIN- sleeping for {duration:#?}");
    sleep(duration).await;
    println!("async-END- slept for {duration:#?}");
    "async-retval".to_string()
}

#[magnus::wrap(class = "Baml::TokioDemo", free_immediately, size)]
struct TokioDemo {
    t: tokio::runtime::Runtime,
}

impl TokioDemo {
    fn new() -> Result<Self> {
        let Ok(tokio_runtime) = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
        else {
            return Err(Error::new(runtime_error(), "Failed to start tokio runtime"));
        };

        Ok(Self { t: tokio_runtime })
    }

    fn does_this_yield(&self) {
        self.t.block_on(async_fn());
    }
}

fn json_to_ruby(any: Value) -> Result<Value> {
    let json = serde_magnus::deserialize::<Value, serde_json::Value>(any);

    let ruby = match Ruby::get() {
        Ok(ruby) => ruby,
        Err(e) => {
            return Err(Error::new(
                runtime_error(),
                format!("Failed to access Ruby runtime: {}", e),
            ))
        }
    };

    match json {
        Ok(json) => {
            return Ok(json_to_ruby::JsonToRuby::to_ruby(&ruby, json)?);
        }
        Err(e) => {
            return Err(Error::new(
                ruby.exception_type_error(),
                format!("Failed to convert input to magnus::Value: {}", e),
            ));
        }
    }
}

// must be kept in sync with rb.define_class in the init() fn
#[magnus::wrap(class = "Baml::BamlRuntime", free_immediately, size)]
struct BamlRuntimeFfi {
    internal: BamlRuntime,
    t: tokio::runtime::Runtime,
}

impl BamlRuntimeFfi {
    fn try_lock_gvl() -> Result<Ruby> {
        match Ruby::get() {
            Ok(ruby) => Ok(ruby),
            Err(e) => match e {
                // TODO(sam): this error handling code doesn't feel right to me - calling `runtime_error()` will
                // panic from a non-Ruby thread - but I'm not sure what the right way to handle this is
                RubyUnavailableError::GvlUnlocked => Err(Error::new(
                    runtime_error(),
                    "Failed to access Ruby runtime: GVL is unlocked",
                )),
                RubyUnavailableError::NonRubyThread => Err(Error::new(
                    runtime_error(),
                    "Failed to access Ruby runtime: calling from a non-Ruby thread",
                )),
            },
        }
    }

    pub fn from_directory(directory: PathBuf) -> Result<Self> {
        let ruby = BamlRuntimeFfi::try_lock_gvl()?;

        let baml_runtime = match BamlRuntime::from_directory(&directory) {
            Ok(br) => br,
            Err(e) => {
                return Err(Error::new(
                    ruby.exception_runtime_error(),
                    format!(
                        "Encountered error while loading BAML files from directory:\n{:#}",
                        e
                    ),
                ))
            }
        };

        // NB: libruby will panic if called from a non-Ruby thread, so we stick to the current thread
        // to avoid causing issues
        let Ok(tokio_runtime) = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
        else {
            return Err(Error::new(
                ruby.exception_runtime_error(),
                "Failed to start tokio runtime",
            ));
        };

        Ok(Self {
            internal: baml_runtime,
            t: tokio_runtime,
        })
    }

    pub fn call_function(&self, call_fn_args: RHash) -> Result<ruby_types::FunctionResult> {
        let ruby = BamlRuntimeFfi::try_lock_gvl()?;

        let call_fn_args = get_kwargs(call_fn_args, &["function_name", "args"], &["ctx"])?;

        let (function_name, args): (String, RHash) = call_fn_args.required;
        let (ctx,): (Option<RHash>,) = call_fn_args.optional;
        let rest: RHash = call_fn_args.splat;

        if !rest.is_empty() {
            return Err(Error::new(
                ruby.exception_syntax_error(),
                format!("unexpected keyword arguments: {}", rest),
            ));
        }

        let Ok(args) = serde_magnus::deserialize::<_, HashMap<String, serde_json::Value>>(args)
        else {
            return Err(Error::new(
                ruby.exception_syntax_error(),
                format!(
                    "expected keyword 'args' to specify a hash, but was: {}",
                    args
                ),
            ));
        };

        println!("args are {:#?}", args);

        let ctx = match ctx {
            Some(ctx) => match serde_magnus::deserialize::<_, RuntimeContext>(ctx) {
                Ok(ctx) => ctx,
                Err(e) => {
                    return Err(Error::new(
                        ruby.exception_syntax_error(),
                        format!("error while parsing ctx: {}", e),
                    ));
                }
            },
            None => RuntimeContext::default(),
        };

        println!("fn trying to call? {}", function_name);

        match self
            .t
            .block_on(self.internal.call_function(function_name, args, ctx))
        {
            Ok(res) => Ok(ruby_types::FunctionResult::new(res)),
            Err(e) => {
                return Err(Error::new(
                    ruby.exception_runtime_error(),
                    format!("error while calling function: {}", e),
                ));
            }
        }
    }
}

#[magnus::init(name = "ruby_ffi")]
fn init() -> Result<()> {
    let rb = BamlRuntimeFfi::try_lock_gvl()?;

    let module = rb.define_module("Baml")?;

    // must be kept in sync with the magnus::wrap annotation
    let runtime_class = module.define_class("BamlRuntime", class::object())?;
    runtime_class.define_singleton_method(
        "from_directory",
        function!(BamlRuntimeFfi::from_directory, 1),
    )?;
    runtime_class.define_method("call_function", method!(BamlRuntimeFfi::call_function, 1))?;

    module.define_module_function("json_to_ruby", function!(json_to_ruby, 1))?;

    let tokio_demo = module.define_class("TokioDemo", class::object())?;
    tokio_demo.define_singleton_method("new", function!(TokioDemo::new, 0))?;
    tokio_demo.define_method("does_this_yield", method!(TokioDemo::does_this_yield, 0))?;

    ruby_types::FunctionResult::define_in(&module)?;

    Ok(())
}
