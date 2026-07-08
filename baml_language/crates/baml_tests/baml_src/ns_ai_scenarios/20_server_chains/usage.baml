// scenario 20 — server-stored chains: ../../../../../llm-provider/ideas/scenarios/20-server-stored-chains/README.md
// The handle carries the owner tag (the D2 owner-bound guard). Live tier =
// responses_live_chain (OpenAiResponses.Chain).

class ScriptedChains {
    implements baml.ai.Provider {}

    implements baml.ai.Chain {
        function extend<T>(self, prompt: string, handle: baml.ai.ChainHandle) -> T
            throws baml.errors.UnknownError {
            if (handle.owner != "scripted") {
                throw baml.errors.UnknownError { data: null, message: ["foreign chain handle: " + handle.owner] };
            }
            baml.sap.parse<T>(handle.id + " += " + prompt) catch (e) {
                _ => throw baml.errors.UnknownError { data: e, message: ["parse"] },
            }
        }
    }
}

test "20: extending a chain threads only the handle" {
    let p: baml.ai.Provider = ScriptedChains {};
    let r = match (p) {
        let c: baml.ai.Chain => c.extend<string>("next turn", baml.ai.ChainHandle { id: "chain-1", owner: "scripted" }) catch (e) { _ => "failed" },
        _ => "no chain capability",
    };
    assert.equal(r, "chain-1 += next turn")
}

test "20: a foreign-owner handle is refused, not silently re-driven" {
    let p: baml.ai.Provider = ScriptedChains {};
    let r = match (p) {
        let c: baml.ai.Chain => c.extend<string>("x", baml.ai.ChainHandle { id: "c9", owner: "other-provider" }) catch (e) { _ => "refused" },
        _ => "no chain capability",
    };
    assert.equal(r, "refused")
}
