// scenario 44 — suspend/resume: ../../../../../llm-provider/ideas/scenarios/44-workflow-suspend-resume/README.md
// A suspension is a SUM ARM (plan D5), not an error. BLOCKED: P8 — durable
// stores; the offline fake scripts the suspend/reenter round trip.

/// Offline Suspendable provider: suspends at a named point until reentered.
class s44_Suspend {
    payload: string,
    point: string,
}

class s44_Snapshot {
    payload: string,
}

interface s44_Suspendable requires root.ai.Provider {
    function start<T>(self, prompt: string) -> T | s44_Suspend throws baml.errors.UnknownError
    function reenter<T>(self, snap: s44_Snapshot, resume: string) -> T | s44_Suspend
        throws baml.errors.UnknownError
}

class ScriptedSuspend {
    implements root.ai.Provider {
        function display_name(self) -> string throws never { "scripted-suspend" }
        function render_shorthand(self) -> string throws never { "openai/scripted-suspend" }
    }

    implements s44_Suspendable {
        function start<T>(self, prompt: string) -> T | s44_Suspend
            throws baml.errors.UnknownError {
            s44_Suspend { payload: prompt, point: "human-approval" }
        }
        function reenter<T>(self, snap: s44_Snapshot, resume: string) -> T | s44_Suspend
            throws baml.errors.UnknownError {
            baml.sap.parse<T>("resumed: " + resume) catch (e) {
                _ => throw baml.errors.UnknownError { data: e, message: ["reenter parse"] },
            }
        }
    }
}

test "44: a suspension surfaces as a sum arm, not an error" {
    let p: root.ai.Provider = ScriptedSuspend {};
    let label = match (p) {
        let s: s44_Suspendable => {
            match (s.start<string>("draft the contract") catch (e) { _ => "start failed" }) {
                let sus: s44_Suspend => "suspended at " + sus.point,
                let value: string => value,
            }
        },
        _ => "not suspendable",
    };
    assert.equal(label, "suspended at human-approval")
}
