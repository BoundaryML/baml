use baml_runtime::{BamlRuntime, RuntimeContext};
use futures::executor::block_on;
use magnus::{
    class, error::RubyUnavailableError, exception::runtime_error, function, method, prelude::*,
    scan_args::get_kwargs, value::Value, Error, RHash, Ruby,
};
use std::collections::HashMap;
use std::path::PathBuf;

mod json_to_ruby;

type Result<T> = std::result::Result<T, magnus::Error>;

fn does_this_yield() {
    println!("BEGIN- sleeping for 2s");
    std::thread::sleep(std::time::Duration::from_secs(2));
    println!("END- slept for 2s");
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

        match BamlRuntime::from_directory(&directory) {
            Ok(br) => Ok(BamlRuntimeFfi { internal: br }),
            Err(e) => Err(Error::new(
                ruby.exception_runtime_error(),
                format!(
                    "Encountered error while loading BAML files from directory:\n{:#}",
                    e
                ),
            )),
        }
    }

    pub fn call_function(&self, call_fn_args: RHash) -> Result<()> {
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

        match block_on(self.internal.call_function(&function_name, &args, ctx)) {
            Ok(_) => Ok(()),
            Err(e) => Err(Error::new(
                ruby.exception_runtime_error(),
                format!("{:#}", e),
            )),
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
    module.define_module_function("does_this_yield", function!(does_this_yield, 0))?;

    Ok(())
}
