use magnus::{
    class, exception::runtime_error, function, method, prelude::*, Error, IntoValue, RModule,
};
#[cfg(ruby_have_ruby_fiber_scheduler_h)]
use rb_sys::bindings::uncategorized::{
    rb_fiber_current, rb_fiber_scheduler_block, rb_fiber_scheduler_current, rb_fiber_scheduler_get,
    rb_fiber_scheduler_kernel_sleep, rb_fiber_scheduler_unblock,
};
use tokio::time::{sleep, Duration};

use crate::Result;

async fn async_fn() -> String {
    let duration = Duration::from_secs(2);
    println!("async-BEGIN- sleeping for {duration:#?}");
    sleep(duration).await;
    println!("async-END- slept for {duration:#?}");
    "async-retval".to_string()
}

#[magnus::wrap(class = "Baml::Ffi::TokioDemo", free_immediately, size)]
/// For testing how to implement tokio in a Ruby extension
pub struct TokioDemo {
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

    #[cfg(ruby_have_ruby_fiber_scheduler_h)]
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

    /// For usage in magnus::init
    ///
    /// TODO: use traits and macros to implement this
    pub fn define_in_ruby(module: &RModule) -> Result<()> {
        let tokio_demo = module.define_class("TokioDemo", class::object())?;
        tokio_demo.define_singleton_method("new", function!(TokioDemo::new, 0))?;
        #[cfg(ruby_have_ruby_fiber_scheduler_h)]
        tokio_demo.define_method("does_this_yield", method!(TokioDemo::does_this_yield, 0))?;

        Ok(())
    }
}
