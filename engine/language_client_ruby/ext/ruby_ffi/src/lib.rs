use baml_runtime::{BamlRuntime, RuntimeContext};
use magnus::IntoValue;
use magnus::{
    class, error::RubyUnavailableError, exception::runtime_error, function, method, prelude::*,
    scan_args::get_kwargs, Error, RHash, Ruby,
};
use std::collections::HashMap;
use std::path::PathBuf;

mod ruby_to_json;
mod ruby_types;
mod tokio_demo;

type Result<T> = std::result::Result<T, magnus::Error>;

// must be kept in sync with rb.define_class in the init() fn
#[magnus::wrap(class = "Baml::Ffi::BamlRuntime", free_immediately, size)]
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
                    format!("{:?}", e.context("Failed to initialize BAML runtime")),
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

        let args = match ruby_to_json::RubyToJson::convert_hash_to_json(args) {
            Ok(args) => args.into_iter().collect(),
            Err(e) => {
                return Err(Error::new(
                    ruby.exception_syntax_error(),
                    format!("error while parsing keyword 'args' as JSON:\n{}", e),
                ));
            }
        };

        let mut ctx = match ctx {
            Some(ctx) => serde_magnus::deserialize::<_, RuntimeContext>(ctx)?,
            None => RuntimeContext::default(),
        };
        log::debug!("Calling {function_name} with:\nargs: {args:#?}\nctx (where env is envvar overrides): {ctx:#?}");
        ctx.env = std::env::vars_os()
            .map(|(k, v)| {
                (
                    k.to_string_lossy().to_string(),
                    v.to_string_lossy().to_string(),
                )
            })
            .chain(ctx.env.into_iter())
            .collect();

        let retval = match self.t.block_on(self.internal.call_function(
            function_name.clone(),
            args,
            ctx,
        )) {
            Ok(res) => Ok(ruby_types::FunctionResult::new(res)),
            Err(e) => Err(Error::new(
                ruby.exception_runtime_error(),
                format!(
                    "{:?}",
                    e.context(format!("error while calling {function_name}"))
                ),
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

    let module = rb.define_module("Baml")?.define_module("Ffi")?;

    // must be kept in sync with the magnus::wrap annotation
    let runtime_class = module.define_class("BamlRuntime", class::object())?;
    runtime_class.define_singleton_method(
        "from_directory",
        function!(BamlRuntimeFfi::from_directory, 1),
    )?;
    runtime_class.define_method("call_function", method!(BamlRuntimeFfi::call_function, 1))?;

    ruby_types::define_types(&module)?;

    // everything below this is for our own testing purposes
    tokio_demo::TokioDemo::define_in_ruby(&module)?;
    module.define_module_function(
        "roundtrip",
        function!(ruby_to_json::RubyToJson::roundtrip, 1),
    )?;

    Ok(())
}
