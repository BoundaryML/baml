use anyhow::Result;
use indicatif::{ProgressBar, ProgressStyle};
use std::borrow::Cow;
use std::future::Future;

pub trait FutureWithProgress<T> {
    async fn with_progress_spinner(
        self,
        start_msg: impl Into<Cow<'static, str>>,
        on_ok_msg: impl FnOnce(&T) -> String,
        on_err_msg: impl Into<Cow<'static, str>>,
    ) -> Result<T>;
}

impl<F: Future<Output = Result<T>>, T> FutureWithProgress<T> for F {
    async fn with_progress_spinner(
        self,
        start_msg: impl Into<Cow<'static, str>>,
        on_ok_msg: impl FnOnce(&T) -> String,
        on_err_msg: impl Into<Cow<'static, str>>,
    ) -> Result<T> {
        let start_msg = start_msg.into();

        let spinner = ProgressBar::new_spinner();
        spinner.set_style(
            ProgressStyle::default_spinner()
                .tick_chars("⠁⠁⠉⠙⠚⠒⠂⠂⠒⠲⠴⠤⠄⠄⠤⠠⠠⠤⠦⠖⠒⠐⠐⠒⠓⠋⠉⠈⠈✓")
                .template("{spinner:.green.bold/cyan.bold} {msg}")?,
        );
        spinner.set_message(format!("{start_msg}..."));
        spinner.enable_steady_tick(std::time::Duration::from_millis(100));

        let result = self.await;
        match &result {
            Ok(ok) => {
                spinner.finish_with_message(format!("{start_msg}... {}", on_ok_msg(ok)));
            }
            Err(_) => {
                spinner.finish_with_message(format!("{start_msg}... {}", on_err_msg.into()));
            }
        }
        result
    }
}
