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

    test "interrupt takes the control lane: cancel now, record it, model reacts" {
        let fake = root.ai.ScriptedClient<never> {
            i: 0,
            turns: [[root.ai.assistant("stopping — what should I change?")]],
        };
        let s = plan_trip_session("Japan", fake);
        //# a queued message must NOT delay the interrupt: control preempts data
        s.send("also check Kyoto");
        s.interrupt("user pressed esc");
        let turn = s.run();
        //# the interrupt is recorded BEFORE the queued message in the journal
        let order = s
            .journal
            .entries
            .map((en) -> {
                match (en.event) {
                    let e: root.ai.Interrupted => "interrupt",
                    let e: root.ai.UserMessage => "user",
                    _ => "other",
                }
            })
            .filter((k) -> {
                k != "other"
            });
        assert.equal(order.at(0) ?? "", "interrupt");
        assert.equal(order.at(1) ?? "", "user");
        //# and the default policy called the model so it could react
        assert.equal(fake.i, 1);
        match (turn) {
            let r: root.ai.Replied => assert.contains(r.message, "stopping"),
            _ => assert.is_true(false),
        }
    }

    test "WithSteering middleware buffers until the turn boundary" {
        let eng = root.ai.WithSteering<never> {
            inner: root.ai.ToolLoop<never> { max_steps: 8, task_mode: false },
        };
        let st = eng.init();
        let j = root.ai.new_journal<root.ai.Event>();
        //# buffered: no command issued on arrival
        assert.equal(eng.update(st, j, root.ai.user("also check Kyoto")).length(), 0);
        //# turn boundary: recall the model. The flush is pure — arrivals are
        //# journaled by the runner on receipt; update never writes the journal
        let cmds = eng.update(st, j, root.ai.assistant("draft ready"));
        assert
            .is_true(
                cmds.some((c) -> {
                    c is root.ai.CallModel
                }),
            );
        assert.equal(j.entries.length(), 0);
        assert.equal(st.queued_msgs.length(), 0)
    }
}
