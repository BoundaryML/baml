use std::cell::RefCell;
use std::sync::Arc;

use magnus::{block::Proc, class, method, Module, RModule, Ruby};

use crate::function_result::FunctionResult;
use crate::runtime_ctx_manager::RuntimeContextManager;
use crate::Error;
use crate::Result;

#[magnus::wrap(class = "Baml::Ffi::FunctionResultStream", free_immediately, size)]
pub struct FunctionResultStream {
    inner: RefCell<baml_runtime::FunctionResultStream>,
    t: Arc<tokio::runtime::Runtime>,
}

impl FunctionResultStream {
    pub(super) fn new(
        inner: baml_runtime::FunctionResultStream,
        t: Arc<tokio::runtime::Runtime>,
    ) -> Self {
        Self {
            inner: RefCell::new(inner),
            t,
        }
    }

    fn done(
        ruby: &Ruby,
        rb_self: &FunctionResultStream,
        ctx: &RuntimeContextManager,
    ) -> Result<FunctionResult> {
        let on_event = if ruby.block_given() {
            let proc = ruby.block_proc()?;
            Some(move |event: baml_runtime::FunctionResult| {
                // ignore errors if they happen
                let _ = proc.call::<_, magnus::Value>((FunctionResult::new(event),));
                ()
            })
        } else {
            None
        };

        match rb_self
            .t
            .block_on(rb_self.inner.borrow_mut().run(on_event, &ctx.inner, None))
        {
            (Ok(res), _) => Ok(FunctionResult::new(res)),
            (Err(e), _) => Err(Error::new(
                ruby.exception_runtime_error(),
                format!("{:?}", e),
            )),
        }
    }

    /// For usage in magnus::init
    ///
    /// TODO: use traits and macros to implement this
    pub fn define_in_ruby(module: &RModule) -> Result<()> {
        let cls = module.define_class("FunctionResultStream", class::object())?;

        cls.define_method("done", method!(FunctionResultStream::done, 1))?;

        Ok(())
    }
}
