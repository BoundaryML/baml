// Scenario 03 — Steering: two lanes. send() queues; interrupt() preempts.
// Mirrors pages/02_guides/03_steering.md.

testset "s03 steering" {
    test "messages sent before a run are injected together, before the model call" {
        let fake = root.ai.ScriptedClient<never> { i: 0, turns: [[root.ai.assistant("got both notes")]] };
        let s = plan_trip_session("Japan", fake);
        s.send("also check Kyoto");
        s.send("budget is $3000 total");
        let _ = s.run();
        //# journal order: both UserMessages precede the AssistantMessage
        let kinds = s.journal.entries.map((en) -> {
            match (en.event) {
                let e: root.ai.UserMessage => "user",
                let e: root.ai.AssistantMessage => "assistant",
                _ => "other",
            }
        });
        let flow = kinds.filter((k) -> {
            k != "other"
        });
        assert.equal(flow.at(0) ?? "", "user");
        assert.equal(flow.at(1) ?? "", "user");
        assert.equal(flow.at(2) ?? "", "assistant")
    }

    test "interrupt takes the control lane: recorded, then the run stops" {
        let fake = root.ai.ScriptedClient<never> { i: 0, turns: [] };
        let s = plan_trip_session("Japan", fake);
        s.interrupt("user pressed esc");
        let stopped = (
            s.run() catch_all (e) {
                let t: root.ai.SessionInterrupted => {
                    root.ai.Replied { message: `stopped: ${t.reason}` }
                },
                _ => root.ai.Replied { message: "unexpected" },
            }
        );
        match (stopped) {
            let r: root.ai.Replied => assert.contains(r.message, "user pressed esc"),
            _ => assert.is_true(false),
        };
        //# and the interrupt is in the history
        let has_interrupt = s.journal.entries.some((en) -> {
            en.event is root.ai.Interrupted
        });
        assert.is_true(has_interrupt)
    }

    test "WithSteering middleware buffers until the turn boundary" {
        let eng = root.ai.WithSteering<never> {
            inner: root.ai.ToolLoop<never> { max_steps: 8, task_mode: false },
        };
        let st = eng.init();
        let j = root.ai.new_journal<root.ai.Event>();
        //# buffered: no command issued on arrival
        assert.equal(eng.update(st, j, root.ai.user("also check Kyoto")).length(), 0);
        //# turn boundary: flush + recall the model
        let cmds = eng.update(st, j, root.ai.assistant("draft ready"));
        assert
            .is_true(
                cmds.some((c) -> {
                    c is root.ai.CallModel
                }),
            );
        assert.equal(j.entries.length(), 1)
    }
}
