// Scenario 07 — Custom events, fully typed: define classes, widen the
// union, and the journal / snapshots / folds are compile-checked.
// Mirrors pages/02_guides/10_journal.md.

class TodoUpdated {
    items: string[],
}

type S7Ext = TodoUpdated | ApprovalExt;

//# the pending_permission idiom: session state is a fold over the journal.

//# Two passes — an answer can land after its request in the log.

function pending_permission(j: root.ai.Journal<root.ai.Event | ApprovalExt>) -> string {
    let answered: string[] = [];
    for (let en in j.entries) {
        match (en.event) {
            let g: PermissionGranted => answered.push(g.call_id),
            let d: PermissionDenied => answered.push(d.call_id),
            _ => {},
        }
    }
    let pending = "";
    for (let en in j.entries) {
        if let p: PermissionRequested = en.event {
            if (!answered.includes(p.call_id)) {
                pending = p.call_id;
            }
        }
    }
    pending
}

testset "s07 custom events" {
    test "send_event journals a typed custom event; snapshots carry it" {
        let fake = root.ai.ScriptedClient<S7Ext> { i: 0, turns: [[root.ai.assistant("ok")]] };
        let tb = root.ai.Toolbox { entries: [root.ai.tool(search_flights)] };
        let s = root
            .ai
            .new_session<Itinerary, S7Ext>(
                function_name = "PlanTrip",
                args_json = `{"request":"Japan"}`,
                client = fake,
                toolbox = tb,
                policy = root.ai.ToolLoop<S7Ext> { max_steps: 12, task_mode: false },
                output_schema = itinerary_schema(),
            );
        s.send_event(TodoUpdated { items: ["book flights", "pick hotel"] });
        let _ = s.run();
        //# custom events round-trip through snapshots, typed
        let j2 = root.ai.resume_journal<root.ai.Event | S7Ext>(s.snapshot());
        let items: string[] = [];
        for (let en in j2.entries) {
            if let t: TodoUpdated = en.event {
                for (let i in t.items) {
                    items.push(i);
                }
            }
        }
        assert.equal(items.length(), 2);
        assert.equal(items.at(0) ?? "", "book flights")
    }

    test "custom events are invisible to the transcript by default" {
        let j = root.ai.new_journal<root.ai.Event | S7Ext>();
        j.append(root.ai.user("hello"));
        j.append(TodoUpdated { items: [] });
        let transcript = root.ai.render_transcript(j);
        assert.contains(transcript, "hello");
        assert.is_true(!transcript.includes("Todo"))
    }

    test "reading session state is a typed journal fold" {
        let j = root.ai.new_journal<root.ai.Event | ApprovalExt>();
        j.append(PermissionRequested { call_id: "t1", tool: "run_bash" });
        assert.equal(pending_permission(j), "t1");
        j.append(PermissionGranted { call_id: "t1" });
        assert.equal(pending_permission(j), "");
        j.append(PermissionRequested { call_id: "t2", tool: "write_file" });
        assert.equal(pending_permission(j), "t2")
    }
}
