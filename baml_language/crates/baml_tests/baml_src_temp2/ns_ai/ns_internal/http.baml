// Hidden plumbing for the agent runner. Nothing in ai.internal is part of
// the public surface: helpers here may change shape without notice and never
// appear in documentation examples.
// Shared HTTP-status classification. The status→failure table lives here,
// once, instead of in every provider.

/// Map a non-2xx provider response to a classified failure. `Effects.None` is
/// correct for pure inference calls; effectful endpoints (background jobs,
/// batch submission) should construct their failures directly instead.
function classify_http(
    provider: string,
    status_code: int,
    body: string,
) -> root.ai.Failure throws never {
    //# Rate limiting is the one transient status with a dedicated type
    if (status_code == 429) {
        return root.ai.RateLimited { provider: provider, retry_after_ms: null };
    }
    //# Provider-side trouble: transient, but the request may have been seen
    if (status_code >= 500 || status_code == 408) {
        return root.ai.NetworkFailure { provider: provider, detail: "http " + status_code.to_string() };
    }
    //# Everything else in the 4xx range is a terminal rejection
    root.ai.InvalidRequest { provider: provider, status_code: status_code, detail: body }
}
