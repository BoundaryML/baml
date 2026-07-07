// Stateful-capability usage shapes (scenarios 17, 27, 44, 31), moved out of the stdlib:
// each negotiates its capability over the existential Provider and falls to the
// runtime-promise path when the provider lacks it.

// 17 — provider-held conversation.
function example_chat(
    p: baml.ai.Provider,
    text: string,
    session: baml.ai.Session,
) -> string throws baml.errors.UnknownError {
    match (p) {
        let c: baml.ai.Conversational => c.chat<string>(text, session),
        _ => {
            throw baml.errors.UnknownError { data: null, message: ["client cannot hold a conversation"] }
        },
    }
}

// 27 — background submit + poll.
function example_background(
    p: baml.ai.Provider,
    prompt: string,
    key: string,
) -> string throws baml.errors.UnknownError {
    match (p) {
        let b: baml.ai.Background => {
            let job: baml.ai.Job<string> = b.submit<string>(prompt, key);
            match (b.poll<string>(job)) {
                let done: string => done,
                _ => "pending",
            }
        },
        _ => {
            throw baml.errors.UnknownError { data: null, message: ["client cannot run background jobs"] }
        },
    }
}

// 44 — suspend/resume: a suspension is a value, not an error.
function example_suspend(
    p: baml.ai.Provider,
    prompt: string,
) -> string throws baml.errors.UnknownError {
    match (p) {
        let s: baml.ai.Suspendable => match (s.start<string>(prompt)) {
            let value: string => value,
            let sus: baml.ai.Suspend => "suspended at " + sus.point,
        },
        _ => throw baml.errors.UnknownError { data: null, message: ["client is not suspendable"] },
    }
}

// 31 — managed cache lifecycle.
function example_managed_cache(
    p: baml.ai.Provider,
    prefix: string,
) -> string throws baml.errors.UnknownError {
    match (p) {
        let mc: baml.ai.ManagedCache => {
            let h = mc.create_cache(prefix, 3600);
            // NOTE: a real caller must eventually `delete_cache(h)` or leak money — the
            // lifecycle gap (Family A1). `defer { mc.delete_cache(h) }` is the BEP-042 fix.
            mc.delete_cache(h);
            h.name
        },
        _ => throw baml.errors.UnknownError { data: null, message: ["client has no managed cache"] },
    }
}
