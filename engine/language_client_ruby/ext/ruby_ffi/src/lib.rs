use baml_runtime::{BamlRuntime, RuntimeContext};
use baml_types::BamlValue;
use indexmap::IndexMap;
use magnus::block::Proc;
use magnus::typed_data::Obj;
use magnus::{
    class, error::RubyUnavailableError, exception::runtime_error, function, method, prelude::*,
    scan_args::get_kwargs, Error, RClass, RHash, RModule, Ruby,
};
use magnus::{ExceptionClass, IntoValue};
use std::collections::HashMap;
use std::ops::Deref;
use std::path::PathBuf;
use std::sync::Arc;

use function_result::FunctionResult;
use function_result_stream::FunctionResultStream;
pub(crate) use image::BamlImage;
use runtime_ctx_manager::RuntimeContextManager;

mod function_result;
mod function_result_stream;
mod image;
mod ruby_to_json;
mod runtime_ctx_manager;
mod type_builder;

type Result<T> = std::result::Result<T, magnus::Error>;

pub fn baml_error_class(ruby: &Ruby) -> Result<ExceptionClass> {
    let baml_error_type = ruby
        .define_module("Baml")?
        .define_class("Error", ruby.exception_runtime_error().as_r_class())?;

    match ExceptionClass::from_value(baml_error_type.into_value()) {
        Some(e) => Ok(e),
        None => Err(Error::new(
            ruby.exception_runtime_error(),
            "Failed to create BamlError class",
        )),
    }
}

pub fn baml_error(ruby: &Ruby, e: impl Into<anyhow::Error>, msg: impl Into<String>) -> Error {
    Error::new(
        // NB(sam): the unwrap_or should never fall back to runtime_error
        baml_error_class(ruby).unwrap_or(ruby.exception_runtime_error()),
        format!("{:?}", e.into().context(msg.into())),
    )
}

trait DefineInRuby {
    fn define_in_ruby(module: &RModule) -> Result<()>;
}

// must be kept in sync with rb.define_class in the init() fn
#[magnus::wrap(class = "Baml::Ffi::BamlRuntime", free_immediately, size)]
struct BamlRuntimeFfi {
    inner: Arc<BamlRuntime>,
    t: Arc<tokio::runtime::Runtime>,
}

impl Drop for BamlRuntimeFfi {
    fn drop(&mut self) {
        use baml_runtime::runtime_interface::ExperimentalTracingInterface;
        match self.inner.flush() {
            Ok(_) => log::info!("Flushed BAML log events"),
            Err(e) => log::error!("Error while flushing BAML log events: {:?}", e),
        }
    }
}

impl BamlRuntimeFfi {
    fn make_tokio_runtime(ruby: &Ruby) -> Result<tokio::runtime::Runtime> {
        // NB: libruby will panic if called from a non-Ruby thread, so we stick to the current thread
        // to avoid causing issues
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|e| baml_error(ruby, e, "Failed to start tokio runtime"))
    }

    pub fn from_directory(
        ruby: &Ruby,
        directory: PathBuf,
        env_vars: HashMap<String, String>,
    ) -> Result<BamlRuntimeFfi> {
        let baml_runtime = match BamlRuntime::from_directory(&directory, env_vars) {
            Ok(br) => br,
            Err(e) => return Err(baml_error(ruby, e, "Failed to initialize BAML runtime")),
        };

        let rt = BamlRuntimeFfi {
            inner: Arc::new(baml_runtime),
            t: Arc::new(Self::make_tokio_runtime(ruby)?),
        };

        Ok(rt)
    }

    pub fn from_files(
        ruby: &Ruby,
        root_path: String,
        files: HashMap<String, String>,
        env_vars: HashMap<String, String>,
    ) -> Result<Self> {
        let baml_runtime = match BamlRuntime::from_file_content(&root_path, &files, env_vars) {
            Ok(br) => br,
            Err(e) => return Err(baml_error(ruby, e, "Failed to initialize BAML runtime")),
        };

        let rt = BamlRuntimeFfi {
            inner: Arc::new(baml_runtime),
            t: Arc::new(Self::make_tokio_runtime(ruby)?),
        };

        Ok(rt)
    }

    pub fn create_context_manager(&self) -> RuntimeContextManager {
        RuntimeContextManager {
            inner: self
                .inner
                .create_ctx_manager(BamlValue::String("ruby".to_string())),
        }
    }

    pub fn call_function(
        ruby: &Ruby,
        rb_self: &BamlRuntimeFfi,
        function_name: String,
        args: RHash,
        ctx: &RuntimeContextManager,
        dynamic_types: magnus::Value,
    ) -> Result<FunctionResult> {
        let args = match ruby_to_json::RubyToJson::convert_hash_to_json(args) {
            Ok(args) => args.into_iter().collect(),
            Err(e) => {
                return Err(baml_error(
                    ruby,
                    e,
                    "Failed to convert call_function args to JSON",
                ))
            }
        };

        log::debug!("Calling {function_name} with:\nargs: {args:#?}\nctx ???");

        let dynamic_types = if dynamic_types.is_nil() {
            None
        } else {
            Some(
                Obj::<type_builder::TypeBuilder>::try_convert(dynamic_types)
                    .map_err(|e| baml_error(ruby, anyhow::Error::msg(format!("{:?}", e)), format!("Expected dynamic_types to be a type_builder::TypeBuilder, but was {:?}", dynamic_types)))?,
            )
        };

        let retval = match rb_self.t.block_on(rb_self.inner.call_function(
            function_name.clone(),
            &args,
            &ctx.inner,
            dynamic_types.as_ref().map(|t| &t.inner),
        )) {
            (Ok(res), _) => Ok(FunctionResult::new(res)),
            (Err(e), _) => Err(baml_error(
                ruby,
                e,
                format!("error while calling {function_name}"),
            )),
        };

        retval
    }

    fn stream_function(
        ruby: &Ruby,
        rb_self: &BamlRuntimeFfi,
        function_name: String,
        args: RHash,
        ctx: &RuntimeContextManager,
        dynamic_types: magnus::Value,
    ) -> Result<FunctionResultStream> {
        let args = match ruby_to_json::RubyToJson::convert_hash_to_json(args) {
            Ok(args) => args.into_iter().collect(),
            Err(e) => {
                return Err(baml_error(
                    ruby,
                    e,
                    "Failed to convert stream_function args to JSON",
                ))
            }
        };

        log::debug!("Streaming {function_name} with:\nargs: {args:#?}\nctx ???");

        let dynamic_types = if dynamic_types.is_nil() {
            None
        } else {
            Some(
                Obj::<type_builder::TypeBuilder>::try_convert(dynamic_types)
                    .map_err(|e| baml_error(ruby, anyhow::Error::msg(format!("{:?}", e)), format!("Expected dynamic_types to be a type_builder::TypeBuilder, but was {:?}", dynamic_types)))?,
            )
        };

        let retval = match rb_self.inner.stream_function(
            function_name.clone(),
            &args,
            &ctx.inner,
            dynamic_types.as_ref().map(|t| &t.inner),
        ) {
            Ok(res) => Ok(FunctionResultStream::new(res, rb_self.t.clone())),
            Err(e) => Err(baml_error(
                ruby,
                e,
                format!("error while streaming {function_name}"),
            )),
        };

        retval
    }
}

fn invoke_runtime_cli(ruby: &Ruby, argv0: String, argv: Vec<String>) -> Result<()> {
    baml_runtime::BamlRuntime::run_cli(
        std::iter::once(argv0).chain(argv.into_iter()).collect(),
        baml_runtime::CallerType::Ruby,
    )
    .map_err(|e| baml_error(ruby, e, "error while invoking baml-cli"))
}

#[magnus::init(name = "ruby_ffi")]
fn init(ruby: &Ruby) -> Result<()> {
    if let Err(e) = env_logger::try_init_from_env(
        env_logger::Env::new()
            .filter("BAML_LOG")
            .write_style("BAML_LOG_STYLE"),
    ) {
        eprintln!("Failed to initialize BAML logger: {:#}", e);
    };

    baml_error_class(ruby)?;

    let public_module = ruby.define_module("Baml")?;
    image::BamlImage::define_in_ruby(&public_module)?;
    type_builder::TypeBuilder::define_in_ruby(&public_module)?;
    type_builder::EnumBuilder::define_in_ruby(&public_module)?;
    type_builder::EnumValueBuilder::define_in_ruby(&public_module)?;
    type_builder::ClassBuilder::define_in_ruby(&public_module)?;
    type_builder::ClassPropertyBuilder::define_in_ruby(&public_module)?;
    type_builder::FieldType::define_in_ruby(&public_module)?;

    let module = public_module.define_module("Ffi")?;

    module.define_module_function("invoke_runtime_cli", function!(invoke_runtime_cli, 2))?;

    // must be kept in sync with the magnus::wrap annotation
    let runtime_class = module.define_class("BamlRuntime", class::object())?;
    runtime_class.define_singleton_method(
        "from_directory",
        function!(BamlRuntimeFfi::from_directory, 2),
    )?;
    runtime_class
        .define_singleton_method("from_files", function!(BamlRuntimeFfi::from_files, 3))?;
    runtime_class.define_method(
        "create_context_manager",
        method!(BamlRuntimeFfi::create_context_manager, 0),
    )?;
    runtime_class.define_method("call_function", method!(BamlRuntimeFfi::call_function, 4))?;
    runtime_class.define_method(
        "stream_function",
        method!(BamlRuntimeFfi::stream_function, 4),
    )?;

    FunctionResult::define_in_ruby(&module)?;
    FunctionResultStream::define_in_ruby(&module)?;
    RuntimeContextManager::define_in_ruby(&module)?;

    // everything below this is for our own testing purposes
    module.define_module_function(
        "roundtrip",
        function!(ruby_to_json::RubyToJson::roundtrip, 1),
    )?;
    module.define_module_function(
        "serialize",
        function!(ruby_to_json::RubyToJson::serialize, 2),
    )?;

    Ok(())
}
