/// An explicit signal route owned by a standalone CLI host. Ordinary embedded
/// engines leave this unset and never change the process's signal dispositions.
#[derive(Clone, Debug)]
pub struct ForegroundSignals(pub(crate) tokio::sync::broadcast::Sender<i32>);

impl Default for ForegroundSignals {
    fn default() -> Self {
        Self(tokio::sync::broadcast::channel(16).0)
    }
}

impl ForegroundSignals {
    /// Forward a host signal to active foreground operations. Returns false
    /// when there is no foreground child and the host should handle it itself.
    pub fn forward(&self, signal: i32) -> bool {
        self.0.send(signal).is_ok()
    }
}

#[cfg(unix)]
fn in_terminal_foreground_group() -> bool {
    use std::os::fd::AsRawFd;
    // A piped stdin does not remove the controlling terminal. Inspect all
    // standard descriptors and /dev/tty before deciding to forward SIGINT.
    let tty = std::fs::File::open("/dev/tty").ok();
    [
        Some(0),
        Some(1),
        Some(2),
        tty.as_ref().map(AsRawFd::as_raw_fd),
    ]
    .into_iter()
    .flatten()
    .any(|fd| {
        // SAFETY: these syscalls accept descriptor integers and no pointers.
        #[allow(unsafe_code, reason = "inspect the terminal foreground process group")]
        unsafe {
            libc::tcgetpgrp(fd) == libc::getpgrp()
        }
    })
}

#[cfg(unix)]
fn should_forward_interrupt(pid: u32) -> bool {
    if !in_terminal_foreground_group() {
        return true;
    }
    let Ok(pid) = i32::try_from(pid) else {
        return false;
    };
    // A child that changes its process group no longer receives the terminal's
    // interrupt. Check at signal time, as uv does, rather than only at spawn.
    // SAFETY: these process queries take integer identifiers and no pointers.
    #[allow(unsafe_code, reason = "inspect the live child's process group")]
    unsafe {
        libc::getpgid(pid) != libc::getpgrp()
    }
}

pub(crate) fn forward_child_signal(pid: Option<u32>, signal: i32) {
    #[cfg(unix)]
    if let Some(pid) = pid {
        if signal == libc::SIGINT && !should_forward_interrupt(pid) {
            return;
        }
        if let Ok(pid) = i32::try_from(pid) {
            // SAFETY: this identifier belongs to the reaper's live Child.
            #[allow(unsafe_code, reason = "forward a host-owned signal to its live child")]
            unsafe {
                libc::kill(pid, signal);
            }
        }
    }
    #[cfg(not(unix))]
    let _ = (pid, signal);
}
