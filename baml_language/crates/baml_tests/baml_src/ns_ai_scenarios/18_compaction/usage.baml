// scenario 18 — compaction (+ 19 fork/branch): ../../../../../llm-provider/ideas/scenarios/18-compaction/README.md
// BLOCKED: P8 — durable stores; the opaque-Window round trip and session
// fork negotiation are exercised against scripted fakes.

class ScriptedCompaction {
    implements baml.ai.Provider {}

    implements baml.ai.Compaction {
        function window_of(self, session: baml.ai.Session) -> baml.ai.Window
            throws baml.errors.UnknownError {
            baml.ai.Window { _data: "w:" + session._id }
        }
        function compact(self, window: baml.ai.Window) -> baml.ai.Window
            throws baml.errors.UnknownError {
            baml.ai.Window { _data: window._data + ":compacted" }
        }
    }

    implements baml.ai.Branching {
        function fork(self, session: baml.ai.Session) -> baml.ai.Session
            throws baml.errors.UnknownError {
            baml.ai.Session { _id: session._id + "-fork" }
        }
    }
}

test "18: the window round-trips opaquely through compact" {
    let p: baml.ai.Provider = ScriptedCompaction {};
    let label = match (p) {
        let c: baml.ai.Compaction => {
            let w = c.window_of(baml.ai.Session { _id: "s9" }) catch (e) { _ => baml.ai.Window { _data: "failed" } };
            let w2 = c.compact(w) catch (e) { _ => baml.ai.Window { _data: "failed" } };
            w2._data
        },
        _ => "no compaction",
    };
    assert.equal(label, "w:s9:compacted")
}

test "19: forking a session yields a distinct handle" {
    let p: baml.ai.Provider = ScriptedCompaction {};
    let forked = match (p) {
        let b: baml.ai.Branching => {
            let f = b.fork(baml.ai.Session { _id: "root" }) catch (e) { _ => baml.ai.Session { _id: "failed" } };
            f._id
        },
        _ => "no branching",
    };
    assert.equal(forked, "root-fork")
}
