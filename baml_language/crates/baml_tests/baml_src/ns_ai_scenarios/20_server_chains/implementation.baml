// scenario 20 — server-stored chains: ../../../../../llm-provider/ideas/scenarios/20-server-stored-chains/README.md
//   implementation.baml — LIBRARY-author code (the offline fake + custom shapes).
//
// The design's core claim: a conversation the PROVIDER holds. The app sends only the
// new turn plus an opaque HANDLE to everything before it, never the transcript bytes.
// "Who owns the conversation" is a property of the provider + transcript, not of the
// prompt. On the real surface that verb is `baml.ai.Chain.extend<T>(prompt, handle)`
// and the handle is `baml.ai.ChainHandle { id, owner }` — ownership travels with it.
//
// The flagship live provider is `baml.ai.OpenAiResponses` (previous_response_id);
// this file supplies the OFFLINE tier: a scripted Chain that needs no network, so the
// three design invariants — extend-threads-only-the-handle, the owner-bound guard
// (D2: refuse a foreign handle), and handle-as-data (persist an id, not bytes) — are
// all deterministically testable. New decls carry the `s20_` prefix (shared namespace).


// A scripted, server-stored chain. Being a value class it has no real server, so it
// simulates one the only honest way a stateless fake can: the accumulated context
// TRAVELS IN THE HANDLE ID (the fake's stand-in for provider-owned thread state). That
// is enough to prove the invariant that matters — `extend` recalls from the HANDLE, not
// from the follow-up prompt — which is the whole point of a server-stored chain.
class s20_ScriptedChain {
    // The owner tag this provider stamps onto every handle it mints. A handle carrying
    // any other owner was minted elsewhere and MUST be refused (the D2 owner guard).
    owner: string,

    implements baml.ai.Provider {
        // A server-stored chain is stateful: each turn mutates provider-side thread
        // state, so a blind re-drive is not free (mirrors OpenAiResponses).
        function is_effectful(self) -> bool throws never {
            true
        }
    }

    implements baml.ai.Chain {
        // Extend the server-held thread. The app hands in ONLY the new prompt + the
        // handle; all prior context is supplied by the "server" (here: the handle id).
        function extend<T>(self, prompt: string, handle: baml.ai.ChainHandle) -> T
            throws baml.errors.UnknownError {
            // D2 owner guard — the design's cross-provider rejection. A handle minted
            // by a different provider names state THIS provider cannot interpret; refuse
            // it typed rather than silently re-driving against foreign thread state.
            if (handle.owner != self.owner) {
                throw baml.errors.UnknownError {
                    data: handle.owner,
                    message: ["foreign chain handle owned by '" + handle.owner + "'; this provider ('" + self.owner + "') did not mint it and cannot extend it"],
                };
            }
            // Recall the stored context FROM THE HANDLE (not the prompt) and thread the
            // new turn onto it. The recalled seed proves the earlier turn survived only
            // because the handle carried it — no transcript bytes were resent.
            let recalled: string = "recalled[" + handle.id + "] :: " + prompt;
            baml.sap.parse<T>(recalled) catch (e) {
                _ => throw baml.errors.UnknownError { data: e, message: ["scripted chain parse failed"] },
            }
        }
    }

    // Mint a chain (the design's `open`). NOT on the Chain interface — an ordinary
    // class method, mirroring `OpenAiResponses.start_chain`. The seed stands in for the
    // first turn's server-stored context; the returned handle names it.
    function start(self, seed: string) -> baml.ai.ChainHandle throws never {
        baml.ai.ChainHandle { id: seed, owner: self.owner }
    }
}
