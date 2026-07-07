// Managed prompt-cache resources (scenario 31): a server-side cache billed while idle,
// so the handle needs a finalizer — the lifecycle gap (Family A1); `defer { delete }`
// is the BEP-042 answer. Host surface pending.

/// Handle to a provider-managed prompt cache.
class CacheHandle {
    name: string,
}

/// Create/delete provider-managed cache resources. Creating commits an external,
/// billed effect — remember to `delete_cache` (or leak money).
interface ManagedCache requires root.ai.Provider {
    function create_cache(self, prefix: string, ttl_secs: int) -> CacheHandle throws root.errors.UnknownError
    function delete_cache(self, handle: CacheHandle) -> null throws root.errors.UnknownError
}
