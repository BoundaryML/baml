//! Signal policy for the standalone packed executable, owned for its lifetime.
//! The stdlib only subscribes while a child is active; embedded native engines
//! do not install or retain any process-wide signal handlers.

pub(crate) async fn run<F: std::future::Future>(
    signals: sys_native::ForegroundSignals,
    future: F,
) -> Result<F::Output, u8> {
    #[cfg(unix)]
    {
        use tokio::signal::unix::{SignalKind, signal};
        let error = |error| {
            baml_exec::print_error(format_args!(
                "failed to install CLI signal handler: {error}"
            ));
            1_u8
        };
        let mut interrupt = signal(SignalKind::interrupt()).map_err(error)?;
        let mut terminate = signal(SignalKind::terminate()).map_err(error)?;
        tokio::pin!(future);
        loop {
            let signal = tokio::select! {
                result = &mut future => return Ok(result),
                Some(()) = interrupt.recv() => libc::SIGINT,
                Some(()) = terminate.recv() => libc::SIGTERM,
            };
            if !signals.forward(signal) {
                return Err(u8::try_from(128 + signal).unwrap_or(1));
            }
        }
    }
    #[cfg(windows)]
    {
        let error = |error| {
            baml_exec::print_error(format_args!(
                "failed to install CLI signal handler: {error}"
            ));
            1_u8
        };
        let mut interrupt = tokio::signal::windows::ctrl_c().map_err(error)?;
        let mut quit = tokio::signal::windows::ctrl_break().map_err(error)?;
        tokio::pin!(future);
        loop {
            let signal = tokio::select! {
                result = &mut future => return Ok(result),
                Some(()) = interrupt.recv() => 2,
                Some(()) = quit.recv() => 3,
            };
            if !signals.forward(signal) {
                return Err(u8::try_from(128 + signal).unwrap_or(1));
            }
        }
    }
    #[cfg(not(any(unix, windows)))]
    {
        let _ = signals;
        Ok(future.await)
    }
}
