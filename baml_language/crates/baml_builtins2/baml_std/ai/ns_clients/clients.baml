// Reliability composes at the client boundary: wrapper clients.
// Mirrors _planv2/pages/02_guides/03_clients/04_reliability.md.

// ---------------------------------------------------------------------------
// Dynamic client selection.
//
// A `client:` position accepts more than a constructed client: the same
// "provider/model" shorthand the compiler understands, and a late-bound
// `baml.env.Ref` so a deployment can pick its model with an environment
// variable that is read at CALL time rather than snapshotted during `$init`.

/// What a `client:` position accepts: an already-built client, a
/// `"provider/model"` shorthand string, or a `baml.env.Ref` naming a variable
/// that holds one. `ai.clients.resolve` turns any of them into a `root.Client`.
type ClientSelector = root.Client | string | baml.env.Ref;

/// Resolve a `ClientSelector` to the client to call, at CALL time.
///
///   * a `root.Client` is returned unchanged (identity);
///   * a string is the `"provider/model"` shorthand, constructed here — the
///     runtime twin of the compiler's literal-`client:` lowering. An unknown
///     prefix throws `root.errors.InvalidRequest` listing the valid ones;
///   * a `baml.env.Ref` is read now and its value takes the string path. An
///     UNSET variable is a typed `root.errors.InvalidRequest` naming it — not
///     a panic, so a `catch_all` at the call boundary can still fall back.
///
/// This is what makes `client: env.MODEL.get() ?? "openai/gpt-4o-mini"` work:
/// nothing about the selection is decided until the call happens.
function resolve(selector: ClientSelector) -> root.Client {
    match (selector) {
        let shorthand: string => root.internal._from_shorthand(shorthand),
        let named: baml.env.Ref => {
            if let value: string = named.get() {
                root.internal._from_shorthand(value)
            } else {
                throw root.errors.InvalidRequest {
                    provider: "ai.clients",
                    status_code: null,
                    detail: `client selector env.${named.name} is not set: point it at a "provider/model" string (valid prefixes are ${root.internal._shorthand_prefixes()}), or give the client: position a value with a fallback — env.${named.name}.get() ?? "openai/gpt-4o-mini"`,
                    raw_body: null,
                }
            }
        },
        let existing: root.Client => existing,
    }
}

// The default judgment: a request the provider rejected does not become
// valid by resending.
function default_retry_judgment(f: root.errors.Failure) -> bool {
    match (f) {
        let r: root.errors.Refused => false,
        let i: root.errors.InvalidRequest => false,
        let p: root.errors.ParseFailed => false,
        _ => true,
    }
}

class Backoff {
    initial_ms: int,
    multiplier: int,
    max_ms: int,

    function new(initial_ms: int = 250, multiplier: int = 2, max_ms: int = 10000) -> Backoff {
        Backoff { initial_ms: initial_ms, multiplier: multiplier, max_ms: max_ms }
    }

    // The delay before attempt N (first retry is attempt 1). A
    // root.errors.RateLimited.retry_after_ms hint overrides the computed value.
    function delay_ms(self, attempt: int, f: root.errors.Failure) -> int {
        let hinted = match (f) {
            let r: root.errors.RateLimited => r.retry_after_ms,
            _ => null,
        };
        if let h: int = hinted {
            h
        } else {
            let d = self.initial_ms;
            let i = 1;
            while (i < attempt) {
                d = d * self.multiplier;
                i = i + 1;
            }
            if (d > self.max_ms) {
                self.max_ms
            } else {
                d
            }
        }
    }
}

class Retry {
    inner: root.Client,
    max_attempts: int,
    retry_if: (root.errors.Failure) -> bool throws never,
    backoff: Backoff,

    function new(
        inner: root.Client,
        max_attempts: int = 3,
        retry_if: ((root.errors.Failure) -> bool throws never)? = null,
        backoff: Backoff? = null,
    ) -> Retry {
        Retry {
            inner: inner,
            max_attempts: max_attempts,
            retry_if: retry_if ?? default_retry_judgment,
            backoff: backoff ?? Backoff.new(),
        }
    }

    function _attempt(self, input: root.ModelTurnInput, remaining: int) -> root.ModelTurn {
        self.inner.invoke(input) catch_all (e) {
            let f: root.errors.Failure => {
                // the safety gate is not overridable; the judgment is
                if (
                    remaining > 1
                        && f.retry_safety() == root.errors.RetrySafety.Safe
                        && self.retry_if(f)
                ) {
                    let attempt = self.max_attempts - remaining + 1;
                    let _ = baml
                        .sys
                        .sleep(
                            baml.time.Duration.from_milliseconds(self.backoff.delay_ms(attempt, f)),
                        ) catch_all (se) {
                        _ => null,
                    };
                    self._attempt(input, remaining - 1)
                } else {
                    throw e
                }
            },
            _ => throw e, // untyped failures are never retried
        }
    }

    implements root.Client {
        function id(self) -> string {
            self.inner.id()
        }

        // Rendering is a pure preview of the inner client. It deliberately
        // does not apply retry policy or backoff because no request is sent.
        function render(self, input: root.ModelTurnInput) -> baml.http.Request {
            self.inner.render(input) catch_all (e) {
                _ => throw root.errors.normalize(e),
            }
        }

        function invoke(self, input: root.ModelTurnInput) -> root.ModelTurn {
            self._attempt(input, self.max_attempts) catch_all (e) {
                _ => throw root.errors.normalize(e),
            }
        }
    }
}

// Rotates invocations across members. Replaces the legacy round-robin
// provider: distribution is a client-boundary concern, exactly like Retry
// and Fallback. The counter mutates on the shared value, so every call
// site holding this client participates in one rotation.
class RoundRobin {
    members: root.Client[],
    counter: int,

    function new(members: root.Client[]) -> RoundRobin {
        if (members.length() == 0) {
            throw baml.errors.InvalidArgument { message: "RoundRobin needs at least one member" }
        }
        RoundRobin { members: members, counter: 0 }
    }

    implements root.Client {
        function id(self) -> string {
            if let c: root.Client = self.members.at(0) {
                c.id()
            } else {
                "round-robin"
            }
        }

        // Rendering is deterministic and side-effect free: preview the first
        // member rather than mutating the invocation rotation counter.
        function render(self, input: root.ModelTurnInput) -> baml.http.Request {
            match (self.members.at(0)) {
                let c: root.Client => c.render(input),
                null => {
                    throw baml.errors.InvalidArgument { message: "RoundRobin needs at least one member" }
                },
            } catch_all (e) {
                _ => throw root.errors.normalize(e),
            }
        }

        function invoke(self, input: root.ModelTurnInput) -> root.ModelTurn {
            {
                let index = self.counter % self.members.length();
                self.counter = self.counter + 1;
                match (self.members.at(index)) {
                    let c: root.Client => c.invoke(input),
                    null => {
                        throw baml.errors.InvalidArgument { message: "RoundRobin needs at least one member" }
                    },
                }
            } catch_all (e) {
                _ => throw root.errors.normalize(e),
            }
        }
    }
}

class Fallback {
    members: root.Client[],

    function _try(self, input: root.ModelTurnInput, index: int) -> root.ModelTurn {
        match (self.members.at(index)) {
            let c: root.Client => {
                c.invoke(input) catch_all (e) {
                    let f: root.errors.Failure => {
                        if (
                            index + 1 < self.members.length()
                                && f.retry_safety() == root.errors.RetrySafety.Safe
                        ) {
                            self._try(input, index + 1)
                        } else {
                            throw e
                        }
                    },
                    _ => throw e,
                }
            },
            null => {
                throw baml.errors.InvalidArgument { message: "Fallback needs at least one member" }
            },
        }
    }

    implements root.Client {
        function id(self) -> string {
            // NOTE: `?.method()` on an interface-typed optional trips a VM bug;
            // if-let is the reliable form.
            if let c: root.Client = self.members.at(0) {
                c.id()
            } else {
                "fallback"
            }
        }

        // Rendering previews the first fallback member. It never probes later
        // members because request construction itself must remain pure.
        function render(self, input: root.ModelTurnInput) -> baml.http.Request {
            match (self.members.at(0)) {
                let c: root.Client => c.render(input),
                null => {
                    throw baml.errors.InvalidArgument { message: "Fallback needs at least one member" }
                },
            } catch_all (e) {
                _ => throw root.errors.normalize(e),
            }
        }

        function invoke(self, input: root.ModelTurnInput) -> root.ModelTurn {
            self._try(input, 0) catch_all (e) {
                _ => throw root.errors.normalize(e),
            }
        }
    }
}
