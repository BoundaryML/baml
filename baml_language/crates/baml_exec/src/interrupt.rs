// Termination signals become cancellation of the root call.
//
// SIGINT, SIGTERM and SIGHUP (Ctrl-C and Ctrl-Break on Windows) cancel the
// host-owned token the root call was started with, so the program unwinds
// through its `defer`s like any other cancellation. If the root call ends
// cancelled, the host finishes engine shutdown and then dies by the same
// signal, so the parent sees an ordinary signal death (shell status 128+n).
//
// A second signal, or any signal once the root call has finished (the
// engine shutdown wait), terminates immediately. Once installed the
// handlers stay for the life of the process: the runtime cannot hand the
// default disposition back, so `Interrupts` keeps answering every signal.
//
// A signal the process inherited as ignored (`nohup`, background jobs) stays
// ignored. On Windows the console's inherited ignore-Ctrl-C flag already
// keeps Ctrl-C from reaching the handler.

use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicI32, Ordering},
};

use bex_engine::CancellationToken;

/// A termination signal the host received.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Interrupt(i32);

impl Interrupt {
    /// End the process the way this signal would have without a handler.
    pub fn terminate(self) -> ! {
        use std::io::Write as _;
        let _ = std::io::stdout().flush();
        let _ = std::io::stderr().flush();
        terminate_by_signal(self.0)
    }
}

#[derive(Default)]
struct State {
    received: AtomicI32,
    root_running: AtomicBool,
}

/// Host-side link between termination signals and the root call.
pub struct Interrupts {
    token: CancellationToken,
    state: Arc<State>,
}

impl Interrupts {
    /// Install the handlers. Must be called inside a Tokio runtime with the
    /// IO driver enabled.
    pub fn install() -> std::io::Result<Self> {
        let token = CancellationToken::new();
        let state = Arc::new(State {
            received: AtomicI32::new(0),
            root_running: AtomicBool::new(true),
        });
        let mut signals = Signals::new()?;
        let listener_token = token.clone();
        let listener_state = Arc::clone(&state);
        tokio::spawn(async move {
            loop {
                let signal = signals.recv().await;
                let first = listener_state.root_running.load(Ordering::Acquire)
                    && listener_state
                        .received
                        .compare_exchange(0, signal, Ordering::AcqRel, Ordering::Acquire)
                        .is_ok();
                if !first {
                    Interrupt(signal).terminate();
                }
                listener_token.cancel();
            }
        });
        Ok(Self { token, state })
    }

    /// The token to start the root call with.
    pub fn token(&self) -> CancellationToken {
        self.token.clone()
    }

    /// Mark the root call finished: later signals terminate immediately
    /// instead of cancelling.
    pub fn root_finished(&self) {
        self.state.root_running.store(false, Ordering::Release);
    }

    /// The signal that cancelled the root call, if any.
    pub fn received(&self) -> Option<Interrupt> {
        match self.state.received.load(Ordering::Acquire) {
            0 => None,
            signal => Some(Interrupt(signal)),
        }
    }
}

/// `None` for a signal the process inherited as ignored.
#[cfg(unix)]
struct Signals {
    interrupt: Option<tokio::signal::unix::Signal>,
    terminate: Option<tokio::signal::unix::Signal>,
    hangup: Option<tokio::signal::unix::Signal>,
}

#[cfg(unix)]
impl Signals {
    fn new() -> std::io::Result<Self> {
        use tokio::signal::unix::SignalKind;
        Ok(Self {
            interrupt: listen(libc::SIGINT, SignalKind::interrupt())?,
            terminate: listen(libc::SIGTERM, SignalKind::terminate())?,
            hangup: listen(libc::SIGHUP, SignalKind::hangup())?,
        })
    }

    async fn recv(&mut self) -> i32 {
        tokio::select! {
            () = next(self.interrupt.as_mut()) => libc::SIGINT,
            () = next(self.terminate.as_mut()) => libc::SIGTERM,
            () = next(self.hangup.as_mut()) => libc::SIGHUP,
        }
    }
}

/// Listen for `signal` unless the process started with it ignored, which is
/// how `nohup` and shells running background jobs ask a child not to be
/// stopped by it. Taking the signal over anyway would turn an ignored
/// signal into a shutdown; Python and POSIX shells leave such signals alone
/// for the same reason.
#[cfg(unix)]
fn listen(
    signal: i32,
    kind: tokio::signal::unix::SignalKind,
) -> std::io::Result<Option<tokio::signal::unix::Signal>> {
    if inherited_ignored(signal)? {
        return Ok(None);
    }
    tokio::signal::unix::signal(kind).map(Some)
}

#[cfg(unix)]
#[allow(unsafe_code)]
fn inherited_ignored(signal: i32) -> std::io::Result<bool> {
    // SAFETY: a null new action only reads the current disposition into
    // `current`, which is a valid, zero-initialized `sigaction`.
    let current = unsafe {
        let mut current: libc::sigaction = std::mem::zeroed();
        if libc::sigaction(signal, std::ptr::null(), &raw mut current) != 0 {
            return Err(std::io::Error::last_os_error());
        }
        current
    };
    Ok(current.sa_sigaction == libc::SIG_IGN)
}

#[cfg(unix)]
async fn next(signal: Option<&mut tokio::signal::unix::Signal>) {
    match signal {
        Some(signal) => {
            signal.recv().await;
        }
        None => std::future::pending().await,
    }
}

#[cfg(unix)]
#[allow(unsafe_code, clippy::exit)]
fn terminate_by_signal(signal: i32) -> ! {
    // SAFETY: restoring the default disposition and raising the signal on
    // the current thread are async-signal-safe libc calls with valid
    // arguments; with the default action the process ends here.
    unsafe {
        libc::signal(signal, libc::SIG_DFL);
        libc::raise(signal);
    }
    std::process::exit(128 + signal)
}

#[cfg(windows)]
struct Signals {
    ctrl_c: tokio::signal::windows::CtrlC,
    ctrl_break: tokio::signal::windows::CtrlBreak,
}

#[cfg(windows)]
impl Signals {
    fn new() -> std::io::Result<Self> {
        Ok(Self {
            ctrl_c: tokio::signal::windows::ctrl_c()?,
            ctrl_break: tokio::signal::windows::ctrl_break()?,
        })
    }

    // Both events end the process the same way, so they share one nonzero code.
    async fn recv(&mut self) -> i32 {
        tokio::select! {
            _ = self.ctrl_c.recv() => 1,
            _ = self.ctrl_break.recv() => 1,
        }
    }
}

#[cfg(windows)]
#[allow(clippy::exit)]
fn terminate_by_signal(_signal: i32) -> ! {
    // What a console process killed by Ctrl-C or Ctrl-Break reports.
    const STATUS_CONTROL_C_EXIT: u32 = 0xC000_013A;
    std::process::exit(STATUS_CONTROL_C_EXIT.cast_signed())
}
