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
        // Tokio retains its process-wide handlers after dropping the streams.
        // This standalone host restores the inherited policy before shutdown,
        // including the early return if installing SIGTERM fails.
        let _interrupt_policy = SignalDisposition::save(libc::SIGINT).map_err(error)?;
        let mut interrupt = signal(SignalKind::interrupt()).map_err(error)?;
        let _terminate_policy = SignalDisposition::save(libc::SIGTERM).map_err(error)?;
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

#[cfg(unix)]
struct SignalDisposition {
    signal: libc::c_int,
    previous: libc::sigaction,
}

#[cfg(unix)]
impl SignalDisposition {
    #[allow(unsafe_code, reason = "capture the inherited OS signal disposition")]
    fn save(signal: libc::c_int) -> std::io::Result<Self> {
        let mut previous = std::mem::MaybeUninit::<libc::sigaction>::uninit();
        // SAFETY: a null action reads the current disposition into initialized
        // storage and does not change the process's signal policy.
        if unsafe { libc::sigaction(signal, std::ptr::null(), previous.as_mut_ptr()) } != 0 {
            return Err(std::io::Error::last_os_error());
        }
        Ok(Self {
            signal,
            // SAFETY: successful sigaction initialized the entire structure.
            previous: unsafe { previous.assume_init() },
        })
    }
}

#[cfg(unix)]
impl Drop for SignalDisposition {
    #[allow(unsafe_code, reason = "restore the host-owned OS signal disposition")]
    fn drop(&mut self) {
        // SAFETY: restore the valid disposition captured before registration.
        // This host owns these registrations and calls run once per process.
        unsafe { libc::sigaction(self.signal, &raw const self.previous, std::ptr::null_mut()) };
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    #[test]
    fn standalone_dispatch_restores_inherited_signal_handlers() {
        const PROBE: &str = "BAML_PACK_SIGNAL_RESTORE_PROBE";
        if std::env::var_os(PROBE).is_none() {
            // Signal policy is process-wide. Keep the assertion isolated from
            // concurrently running tests and Tokio runtimes in this binary.
            let status = std::process::Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "signals::tests::standalone_dispatch_restores_inherited_signal_handlers",
                ])
                .env(PROBE, "1")
                .status()
                .unwrap();
            assert!(status.success());
            return;
        }
        let interrupt = SignalDisposition::save(libc::SIGINT).unwrap();
        let terminate = SignalDisposition::save(libc::SIGTERM).unwrap();
        let runtime = tokio::runtime::Runtime::new().unwrap();
        assert!(
            runtime
                .block_on(run(sys_native::ForegroundSignals::default(), async {}))
                .is_ok()
        );
        assert_eq!(
            SignalDisposition::save(libc::SIGINT)
                .unwrap()
                .previous
                .sa_sigaction,
            interrupt.previous.sa_sigaction
        );
        assert_eq!(
            SignalDisposition::save(libc::SIGTERM)
                .unwrap()
                .previous
                .sa_sigaction,
            terminate.previous.sa_sigaction
        );
    }
}
