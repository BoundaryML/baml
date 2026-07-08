// scenario 17 — history & sessions: ../../../../../llm-provider/ideas/scenarios/17-history-and-sessions/README.md
// (also 31 caching lifecycle). Provider-held conversation negotiated over the
// existential; the offline fake scripts session-held state.

/// Offline Conversational + ManagedCache provider.
class ScriptedSessions {
    implements baml.ai.Provider {}

    implements baml.ai.Conversational {
        function chat<T>(self, prompt: string, session: baml.ai.Session) -> T
            throws baml.errors.UnknownError {
            baml.sap.parse<T>("session " + session._id + ": " + prompt) catch (e) {
                _ => throw baml.errors.UnknownError { data: e, message: ["chat parse"] },
            }
        }
    }

    implements baml.ai.ManagedCache {
        function create_cache(self, prefix: string, ttl_secs: int) -> baml.ai.CacheHandle
            throws baml.errors.UnknownError {
            baml.ai.CacheHandle { name: "cache-" + prefix }
        }
        function delete_cache(self, handle: baml.ai.CacheHandle) -> null
            throws baml.errors.UnknownError {
            null
        }
    }
}

test "17: provider-held conversation threads the session handle" {
    let p: baml.ai.Provider = ScriptedSessions {};
    let reply = match (p) {
        let c: baml.ai.Conversational => {
            c.chat<string>("hello", baml.ai.Session { _id: "s1" }) catch (e) {
                _ => "chat failed"
            }
        },
        _ => "not conversational",
    };
    assert.equal(reply, "session s1: hello")
}

test "31: cache lifecycle is scope-bound with defer (the BEP-042 pattern)" {
    let p: baml.ai.Provider = ScriptedSessions {};
    let name = match (p) {
        let mc: baml.ai.ManagedCache => {
            let h = mc.create_cache("sys-prompt", 3600) catch (e) { _ => baml.ai.CacheHandle { name: "create failed" } };
            defer {
                mc.delete_cache(h) catch (e) { _ => null };
            }
            h.name
        },
        _ => "no managed cache",
    };
    assert.equal(name, "cache-sys-prompt")
}
