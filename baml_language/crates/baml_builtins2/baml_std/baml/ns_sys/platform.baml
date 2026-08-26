/// Platform capability tokens.
///
/// `platform()` returns the host platform as a capability token: the concrete
/// class (`Linux`, `MacOs`, `Windows`, `Browser`) witnesses which
/// platform-gated operations this host can perform. Platform-specific
/// operations are reachable only by narrowing the token — a `Unix`
/// capability, for example, proves the host speaks `kill(2)`, so
/// `terminate`/`signal_group` exist only on it.
///
/// The token classes carry an opaque native handle and cannot be constructed
/// from BAML code, so there is no way to forge a capability for a platform
/// the program is not running on.

/// A capability token identifying the host platform.
///
/// Obtain one from `platform()` and narrow it with `match`, keeping an
/// explicit arm for the platforms you do not handle:
///
/// ```baml
/// match (baml.sys.platform()) {
///     let u: baml.sys.Unix => u.terminate(pid),
///     _ => { /* explicit unsupported-platform handling */ },
/// }
/// ```
interface Platform {
    /// Returns this platform as a `Unix` capability, or `null` when the host
    /// is not a Unix platform.
    ///
    /// Prefer narrowing `platform()` with `match` (see `Platform`) so the
    /// unsupported-platform case is handled explicitly. Chaining off this
    /// method with `?.` silently no-ops on other platforms — reserve that
    /// form for genuinely optional behavior, such as a cleanup only Unix
    /// needs.
    function unix(self) -> Unix? throws never {
        null
    }

    /// Returns this platform as a `Windows` capability, or `null` when the
    /// host is not Windows.
    ///
    /// Prefer narrowing `platform()` with `match` (see `Platform`) so the
    /// unsupported-platform case is handled explicitly. Chaining off this
    /// method with `?.` silently no-ops on other platforms — reserve that
    /// form for genuinely optional behavior.
    function windows(self) -> Windows? throws never {
        null
    }
}

/// Unix process-control capability.
///
/// Narrows a `Platform` token to the operations every Unix host supports.
/// Obtained from `Platform.unix()` or by matching a `Platform` value against
/// `Unix`.
interface Unix requires Platform {
    /// Sends `SIGTERM` to the process with the given OS process ID.
    ///
    /// `SIGTERM` requests a graceful shutdown — the target may catch or
    /// ignore it. Mirrors `Process.kill`-adjacent control but for an
    /// arbitrary PID, so one BAML invocation can signal a process started by
    /// another.
    ///
    /// Throws `root.errors.Io` when `pid` is not positive, no such process
    /// exists, or the OS denies the signal.
    function terminate(self, pid: int) -> null throws root.errors.Io

    /// Sends a signal to every process in the Unix process group `pgid`
    /// (`kill(-pgid, sig)`).
    ///
    /// Group signalling is the reliable way to take down a whole process
    /// tree whose leader was started in its own process group.
    ///
    /// Throws `root.errors.Io` when `pgid` is not positive, no such process
    /// group exists, or the OS denies the signal.
    function signal_group(self, pgid: int, sig: Signal) -> null throws root.errors.Io
}

/// The signals `Unix.signal_group` can deliver, mapped to their POSIX signal
/// numbers: `Terminate` → `SIGTERM`, `Interrupt` → `SIGINT`, `Kill` →
/// `SIGKILL`, `Hangup` → `SIGHUP`.
enum Signal {
    Terminate,
    Interrupt,
    Kill,
    Hangup,
}

/// Capability token for Linux hosts. Obtained only from `platform()`.
///
/// Also returned on Unix platforms other than Linux and macOS (the BSDs and
/// so on): those share the same POSIX `kill(2)` signalling surface, and the
/// `Unix` capability — not the concrete class — is the gate for those
/// operations.
class Linux {
    /// Opaque native token; user code cannot construct a capability by hand.
    _handle: $rust_type,

    implements Platform {
        function unix(self) -> Unix? throws never {
            self
        }
    }

    implements Unix {
        function terminate(self, pid: int) -> null throws root.errors.Io {
            $rust_io_function
        }

        function signal_group(self, pgid: int, sig: Signal) -> null throws root.errors.Io {
            $rust_io_function
        }
    }
}

/// Capability token for macOS hosts. Obtained only from `platform()`.
class MacOs {
    /// Opaque native token; user code cannot construct a capability by hand.
    _handle: $rust_type,

    implements Platform {
        function unix(self) -> Unix? throws never {
            self
        }
    }

    implements Unix {
        function terminate(self, pid: int) -> null throws root.errors.Io {
            $rust_io_function
        }

        function signal_group(self, pgid: int, sig: Signal) -> null throws root.errors.Io {
            $rust_io_function
        }
    }
}

/// Capability token for Windows hosts. Obtained only from `platform()`.
///
/// No Windows-specific operations ship yet; the class exists so
/// `Platform.windows()` has a capability to narrow to as they are added.
class Windows {
    /// Opaque native token; user code cannot construct a capability by hand.
    _handle: $rust_type,

    implements Platform {
        function windows(self) -> Windows? throws never {
            self
        }
    }
}

/// Capability token for browser (wasm) hosts. Obtained only from
/// `platform()`.
///
/// Browsers expose no process-control surface, so this token narrows to no
/// further capabilities.
class Browser {
    /// Opaque native token; user code cannot construct a capability by hand.
    _handle: $rust_type,

    implements Platform { }
}

/// Returns a capability token for the platform this program is running on.
///
/// Narrow the result with `match` (see `Platform`) to reach platform-gated
/// operations.
function platform() -> Platform throws never {
    $rust_io_function
}
