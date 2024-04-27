use baml_runtime::{BamlRuntime, FunctionResult, RuntimeContext};
use futures::executor::block_on;
use magnus::{
    class, error::RubyUnavailableError, exception::runtime_error, function, method, prelude::*,
    rb_sys::AsRawValue, scan_args::get_kwargs, value::Value, Error, IntoValue, RHash, Ruby,
};
use rb_sys::bindings::uncategorized::{
    rb_fiber_current, rb_fiber_scheduler_block, rb_fiber_scheduler_current, rb_fiber_scheduler_get,
    rb_fiber_scheduler_kernel_sleep, rb_fiber_scheduler_unblock,
};
use std::collections::HashMap;
use std::path::PathBuf;
use tokio::time::{sleep, Duration};

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
        let rb_qnil = rb_sys::special_consts::Qnil;
        println!("build2 qnil {}", Into::<u64>::into(rb_qnil));
        let rb_scheduler = unsafe { rb_fiber_scheduler_get() };
        println!("current scheduler {}", Into::<u64>::into(rb_scheduler));
        let rb_curr_fiber = unsafe { rb_fiber_current() };
        println!(
            "  fiber={} going to sleep",
            Into::<u64>::into(rb_curr_fiber)
        );
        unsafe {
            let rb_duration = magnus::Integer::from_i64(10).into_value();
            //rb_fiber_scheduler_kernel_sleep(rb_scheduler, rb_duration.as_raw());
        }
        let fut = self.t.spawn(async move {
            async_fn().await;
            println!("  fiber={} done sleeping, pls wake up", rb_curr_fiber);
            unsafe {
                let rb_qnil = rb_sys::special_consts::Qnil;
                if rb_scheduler != Into::<u64>::into(rb_qnil) {
                    rb_fiber_scheduler_unblock(rb_scheduler, rb_qnil.into(), rb_curr_fiber);
                }
            }
        });
        println!(
            "  fiber={} signalling that we're going to block",
            Into::<u64>::into(rb_curr_fiber)
        );
        unsafe {
            if rb_scheduler != Into::<u64>::into(rb_qnil) {
                rb_fiber_scheduler_block(
                    rb_scheduler,
                    rb_qnil.into(),
                    // In theory, according to rb_fiber_scheduler_make_timeout, qnil blocks indefinitely
                    /*timeout:*/
                    rb_qnil.into(),
                );
            }
        }
        println!(
            "  fiber={} blocking until woken up",
            Into::<u64>::into(rb_curr_fiber)
        );
        self.t.block_on(fut);
    }

    fn tokio_test(&self) {
        let f0 = self.t.spawn(async_fn());
        let f1 = self.t.spawn(async_fn());
        let f2 = self.t.spawn(async_fn());
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
                    format!("Failed to initialize BAML runtime:\n{:#}", e),
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

        let mut ctx = match ctx {
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
        ctx.env = std::env::vars_os()
            .map(|(k, v)| {
                (
                    k.to_string_lossy().to_string(),
                    v.to_string_lossy().to_string(),
                )
            })
            .chain(ctx.env.into_iter())
            .collect();

        log::debug!("Calling {function_name} with:\nargs: {args:#?}\nctx: {ctx:#?}");
        let retval = match self
            .t
            .block_on(self.internal.call_function(function_name, args, ctx))
        {
            Ok(res) => {
                log::debug!("LLM call result:\n{res:#?}");
                Ok(ruby_types::FunctionResult::new(res))
            }
            Err(e) => Err(Error::new(
                ruby.exception_runtime_error(),
                format!("error while calling function: {}", e),
            )),
        };

        retval
    }
}

#[magnus::init(name = "ruby_ffi")]
fn init() -> Result<()> {
    if let Err(e) = env_logger::try_init_from_env(
        env_logger::Env::new()
            .filter("BAML_LOG")
            .write_style("BAML_LOG_STYLE"),
    ) {
        eprintln!("Failed to initialize BAML logger: {:#}", e);
    };

    let rb = BamlRuntimeFfi::try_lock_gvl()?;

    let module = rb.define_module("Baml")?;

    // must be kept in sync with the magnus::wrap annotation
    let runtime_class = module.define_class("BamlRuntime", class::object())?;
    runtime_class.define_singleton_method(
        "from_directory",
        function!(BamlRuntimeFfi::from_directory, 1),
    )?;
    runtime_class.define_method("call_function", method!(BamlRuntimeFfi::call_function, 1))?;

    let tokio_demo = module.define_class("TokioDemo", class::object())?;
    tokio_demo.define_singleton_method("new", function!(TokioDemo::new, 0))?;
    tokio_demo.define_method("does_this_yield", method!(TokioDemo::does_this_yield, 0))?;

    ruby_types::FunctionResult::ruby_define_self(&module)?;

    Ok(())
}
