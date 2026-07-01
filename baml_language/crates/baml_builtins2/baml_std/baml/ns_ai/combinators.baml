// Combinators (plan Phase 2 / D2): plain classes that wrap providers and forward a
// capability by a runtime `match` over their members. Each combinator statically claims
// the capabilities it forwards and degrades/errors at call time. Reachable fluently via
// the factory methods on the `Provider` marker (`.with_retry`, `.fallback_to`).
//
// The codec methods (`build_request`/`send`/`parse`/`meta_of`) have no single honest
// meaning for a combinator, so they throw `Unsupported`; the combinator overrides the
// primitive `call_with` (from which `call` derives) to do the real routing.

/// Try each member in order; the first HTTP-capable member that succeeds wins.
class Fallback {
    members: root.ai.Provider[],

    implements root.ai.Provider {}

    implements root.ai.HttpProvider {
        type Body = unknown

        function build_request<T>(
            self,
            prompt: string,
        ) -> root.http.Request throws root.errors.CallError | root.errors.UnknownError {
            throw root.errors.UnknownError { data: null, message: ["fallback has no single request"] }
        }
        function send(
            self,
            request: root.http.Request,
        ) -> unknown throws root.errors.CallError | root.errors.UnknownError {
            throw root.errors.UnknownError { data: null, message: ["fallback does not send directly"] }
        }
        function parse<T>(
            self,
            from: unknown,
        ) -> T throws root.errors.CallError | root.errors.UnknownError {
            throw root.errors.UnknownError { data: null, message: ["fallback does not parse directly"] }
        }
        function meta_of(
            self,
            from: unknown,
        ) -> root.ai.ResponseMeta throws root.errors.CallError | root.errors.UnknownError {
            throw root.errors.UnknownError { data: null, message: ["fallback has no meta"] }
        }

        // Route: try each HTTP-capable member; return the first that succeeds.
        function call_with<T, V, E2>(
            self,
            prompt: string,
            project: (root.ai.ResponseMeta) -> V throws E2,
        ) -> root.ai.CallResult<T, V> throws root.errors.CallError | root.errors.UnknownError | E2 {
            for (let m in self.members) {
                match (m) {
                    let h: root.ai.HttpProvider => {
                        let r: root.ai.CallResult<T, V> = h.call_with<T, V, E2>(prompt, project) catch (e) {
                            _ => {
                                continue;
                            }
                        };
                        return r;
                    },
                    _ => {
                        continue;
                    },
                }
            }
            throw root.errors.UnknownError { data: null, message: ["fallback: all providers failed"] };
        }
    }
}

/// Retry the wrapped provider up to `max` extra attempts on failure.
class Retry {
    inner: root.ai.Provider,
    max: int,

    implements root.ai.Provider {}

    implements root.ai.HttpProvider {
        type Body = unknown

        function build_request<T>(
            self,
            prompt: string,
        ) -> root.http.Request throws root.errors.CallError | root.errors.UnknownError {
            throw root.errors.UnknownError { data: null, message: ["retry has no single request"] }
        }
        function send(
            self,
            request: root.http.Request,
        ) -> unknown throws root.errors.CallError | root.errors.UnknownError {
            throw root.errors.UnknownError { data: null, message: ["retry does not send directly"] }
        }
        function parse<T>(
            self,
            from: unknown,
        ) -> T throws root.errors.CallError | root.errors.UnknownError {
            throw root.errors.UnknownError { data: null, message: ["retry does not parse directly"] }
        }
        function meta_of(
            self,
            from: unknown,
        ) -> root.ai.ResponseMeta throws root.errors.CallError | root.errors.UnknownError {
            throw root.errors.UnknownError { data: null, message: ["retry has no meta"] }
        }

        function call_with<T, V, E2>(
            self,
            prompt: string,
            project: (root.ai.ResponseMeta) -> V throws E2,
        ) -> root.ai.CallResult<T, V> throws root.errors.CallError | root.errors.UnknownError | E2 {
            let attempt: int = 0;
            while (true) {
                match (self.inner) {
                    let h: root.ai.HttpProvider => {
                        let r: root.ai.CallResult<T, V> = h.call_with<T, V, E2>(prompt, project) catch (e) {
                            _ => {
                                if (attempt < self.max) {
                                    attempt = attempt + 1;
                                    continue;
                                }
                                throw e;
                            }
                        };
                        return r;
                    },
                    _ => {
                        throw root.errors.UnknownError { data: null, message: ["retry: inner is not callable"] }
                    },
                }
            }
            root.sys.panic("unreachable")
        }
    }
}
