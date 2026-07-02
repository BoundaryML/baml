// Stateful, server-authoritative capabilities + examples (scenarios 17, 18, 19, 20, 21,
// 27, 31, 44). These are the fault line the design flags (Family A): cross-call state with
// a lifecycle. The persistence/host surface (durable stores, background webhooks, cache
// resources) is not built, so the handles + interfaces + negotiation are real and compiling,
// with host bodies pending. Methods use the `UnknownError` channel for now (per-capability
// error interfaces — BackgroundError etc. — are a follow-up; see _plan/deviations.md).

// --- 17: within-run history + sessions ---
class Session {
    _id: string,
}

interface Conversational requires root.ai.Provider {
    function chat<T>(self, prompt: string, session: Session) -> T throws root.errors.UnknownError
}

// --- 18: compaction (opaque window round-tripped verbatim) ---
class Window {
    _data: string,
}

interface Compaction requires root.ai.Provider {
    function window_of(self, session: Session) -> Window throws root.errors.UnknownError
    function compact(self, window: Window) -> Window throws root.errors.UnknownError
}

// --- 19: fork / branch ---
interface Branching requires root.ai.Provider {
    function fork(self, session: Session) -> Session throws root.errors.UnknownError
}

// --- 20: server-stored chains + ownership ---
class ChainHandle {
    id: string,
    owner: string,
}

interface Chain requires root.ai.Provider {
    function extend<T>(self, prompt: string, handle: ChainHandle) -> T throws root.errors.UnknownError
}

// --- 21: cross-conversation long-term memory ---
class MemoryHit {
    text: string,
    score: float,
}

interface MemoryStore requires root.ai.Provider {
    function recall(self, query: string, k: int) -> MemoryHit[] throws root.errors.UnknownError
    function remember(self, fact: string) -> null throws root.errors.UnknownError
}

// --- 27: async background + poll (a persistable Job handle) ---
class Job<T> {
    id: string,
    owner: string,
}

interface Background requires root.ai.Provider {
    function submit<T>(self, prompt: string, idempotency_key: string) -> Job<T> throws root.errors.UnknownError
    function poll<T>(self, job: Job<T>) -> T? throws root.errors.UnknownError
}

// --- 31: managed prompt-cache resource (billed while idle -> needs a finalizer) ---
class CacheHandle {
    name: string,
}

interface ManagedCache requires root.ai.Provider {
    function create_cache(self, prefix: string, ttl_secs: int) -> CacheHandle throws root.errors.UnknownError
    function delete_cache(self, handle: CacheHandle) -> null throws root.errors.UnknownError
}

// --- 44: suspend / resume & human-in-the-loop ---
class Snapshot {
    _data: string,
}

class Suspend {
    payload: string,
    point: string,
}

interface Suspendable requires root.ai.Provider {
    function start<T>(self, prompt: string) -> T | Suspend throws root.errors.UnknownError
    function reenter<T>(self, snap: Snapshot, resume: string) -> T | Suspend throws root.errors.UnknownError
}

// --- usage examples: each negotiates its capability over the existential Provider ---

function example_chat(
    p: root.ai.Provider,
    text: string,
    session: Session,
) -> string throws root.errors.UnknownError {
    match (p) {
        let c: Conversational => c.chat<string>(text, session),
        _ => {
            throw root.errors.UnknownError { data: null, message: ["client cannot hold a conversation"] }
        },
    }
}

function example_background(
    p: root.ai.Provider,
    prompt: string,
    key: string,
) -> string throws root.errors.UnknownError {
    match (p) {
        let b: Background => {
            let job: Job<string> = b.submit<string>(prompt, key);
            match (b.poll<string>(job)) {
                let done: string => done,
                _ => "pending",
            }
        },
        _ => {
            throw root.errors.UnknownError { data: null, message: ["client cannot run background jobs"] }
        },
    }
}

function example_suspend(
    p: root.ai.Provider,
    prompt: string,
) -> string throws root.errors.UnknownError {
    match (p) {
        let s: Suspendable => match (s.start<string>(prompt)) {
            let value: string => value,
            let sus: Suspend => "suspended at " + sus.point,
        },
        _ => throw root.errors.UnknownError { data: null, message: ["client is not suspendable"] },
    }
}

function example_managed_cache(
    p: root.ai.Provider,
    prefix: string,
) -> string throws root.errors.UnknownError {
    match (p) {
        let mc: ManagedCache => {
            let h = mc.create_cache(prefix, 3600);
            // NOTE: a real caller must eventually `delete_cache(h)` or leak money — the
            // lifecycle gap (Family A1). `defer { mc.delete_cache(h) }` is the BEP-042 fix.
            mc.delete_cache(h);
            h.name
        },
        _ => throw root.errors.UnknownError { data: null, message: ["client has no managed cache"] },
    }
}
