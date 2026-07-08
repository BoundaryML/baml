// scenario 44 — suspend/resume: ../../../../../llm-provider/ideas/scenarios/44-workflow-suspend-resume/README.md
// A suspension is a SUM ARM (plan D5), not an error. BLOCKED: P8 — durable
// stores; the offline fake scripts the suspend/reenter round trip.

/// Offline Suspendable provider: suspends at a named point until reentered.
class ScriptedSuspend {
    implements baml.ai.Provider {}

    implements baml.ai.Suspendable {
        function start<T>(self, prompt: string) -> T | baml.ai.Suspend
            throws baml.errors.UnknownError {
            baml.ai.Suspend { payload: prompt, point: "human-approval" }
        }
        function reenter<T>(self, snap: baml.ai.Snapshot, resume: string) -> T | baml.ai.Suspend
            throws baml.errors.UnknownError {
            baml.sap.parse<T>("resumed: " + resume) catch (e) {
                _ => throw baml.errors.UnknownError { data: e, message: ["reenter parse"] },
            }
        }
    }
}

test "44: a suspension surfaces as a sum arm, not an error" {
    let p: baml.ai.Provider = ScriptedSuspend {};
    let label = match (p) {
        let s: baml.ai.Suspendable => {
            match (s.start<string>("draft the contract") catch (e) { _ => "start failed" }) {
                let sus: baml.ai.Suspend => "suspended at " + sus.point,
                let value: string => value,
            }
        },
        _ => "not suspendable",
    };
    assert.equal(label, "suspended at human-approval")
}
