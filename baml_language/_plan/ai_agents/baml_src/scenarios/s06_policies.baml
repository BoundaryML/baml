// Scenario 06 — Policies: pure folds; middleware wraps middleware.
// An approval gate built end to end with TYPED custom events: the event
// union is widened on the policy (type Ev = ApprovalEvent), the session
// carries the extension as a type parameter, and send_event is
// compile-checked. Mirrors pages/02_guides/09_policies.md.

class PermissionRequested {
    call_id: string,
    tool: string,
}

class PermissionGranted {
    call_id: string,
}

class PermissionDenied {
    call_id: string,
}

type ApprovalExt = PermissionRequested | PermissionGranted | PermissionDenied;

type ApprovalEvent = root.ai.Event | ApprovalExt;

class ApprovalGate {
    inner: root.ai.Policy<Ev = ApprovalEvent>,
    needs_ok: string[],
    held: map<string, root.ai.RunTool>,

    implements root.ai.Policy {
        type Ev = ApprovalEvent

        function update(
            self,
            st: root.ai.SessionState,
            j: root.ai.Journal<ApprovalEvent>,
            e: ApprovalEvent,
        ) -> root.ai.Command[] {
            match (e) {
                let g: PermissionGranted => {
                    if let held: root.ai.RunTool = self.held.get(g.call_id) {
                        return [held];
                    }
                    return [];
                },
                let d: PermissionDenied => {
                    j.append(root.ai.ToolFailed { call_id: d.call_id, error: "denied by operator" });
                    return [root.ai.CallModel {  }];
                },
                _ => {
                    self.inner.update(st, j, e).map((cmd) -> {
                        match (cmd) {
                            let r: root.ai.RunTool => {
                                if (self.needs_ok.includes(r.tool)) {
                                    let _ = self.held.set(r.call_id, r);
                                    j.append(
                                        PermissionRequested { call_id: r.call_id, tool: r.tool },
                                    );
                                    root.ai.AwaitInput { note: `approve ${r.tool}?` }
                                } else {
                                    cmd
                                }
                            },
                            _ => cmd,
                        }
                    })
                },
            }
        }
    }
}

function gated_session(
    fake: root.ai.Client<Ev = ApprovalEvent>,
) -> root.ai.Session<Itinerary, ApprovalExt> {
    let tb = root.ai.Toolbox { entries: [root.ai.tool(search_flights), root.ai.tool(search_hotels)] };
    root.ai
        .new_session<Itinerary, ApprovalExt>(
            function_name = "PlanTrip",
            args_json = `{"request":"Japan"}`,
            client = fake,
            toolbox = tb,
            policy = ApprovalGate {
                inner: root.ai.ToolLoop<ApprovalExt> { max_steps: 12, task_mode: false },
                needs_ok: ["search_hotels"],
                held: {},
            },
            output_schema = itinerary_schema(),
        )
}

testset "s06 policies" {
    test "approval gate: hold the tool, wait, release on the typed granted event" {
        let fake = root.ai.ScriptedClient<ApprovalExt> {
            i: 0,
            turns: [
                [
                    root.ai
                        .tool_requested(
                            "t1",
                            "search_hotels",
                            `{"city":"Kyoto","max_nightly":150.0}`,
                        ),
                ],
                [
                    root.ai
                        .final_produced(
                            `{"destination":"Japan","days":5,"flights":[],"hotel":null,"daily_plan":[]}`,
                        ),
                ],
            ],
        };
        let s = gated_session(fake);
        //# the gate holds the call and yields the turn
        let turn1 = s.run();
        assert.is_true(turn1 is root.ai.Replied);
        let requested = s.journal.entries.some((en) -> {
            en.event is PermissionRequested
        });
        assert.is_true(requested);
        //# held means: not executed yet
        assert
            .is_true(
                !s.journal.entries.some((en) -> {
                    en.event is root.ai.ToolCompleted
                }),
            );
        //# approval arrives on the data lane — a TYPED event, compile-checked
        s.send_event(PermissionGranted { call_id: "t1" });
        match (s.run()) {
            let d: root.ai.Done<Itinerary> => assert.equal(d.result.destination, "Japan"),
            _ => assert.is_true(false),
        };
        assert
            .is_true(
                s.journal.entries.some((en) -> {
                    en.event is root.ai.ToolCompleted
                }),
            )
    }

    test "a typed denial becomes a ToolFailed the model can plan around" {
        let fake = root.ai.ScriptedClient<ApprovalExt> {
            i: 0,
            turns: [
                [
                    root.ai
                        .tool_requested(
                            "t1",
                            "search_hotels",
                            `{"city":"Kyoto","max_nightly":150.0}`,
                        ),
                ],
                [
                    root.ai
                        .final_produced(
                            `{"destination":"Japan","days":5,"flights":[],"hotel":null,"daily_plan":["no hotel booked"]}`,
                        ),
                ],
            ],
        };
        let s = gated_session(fake);
        let _ = s.run();
        s.send_event(PermissionDenied { call_id: "t1" });
        match (s.run()) {
            let d: root.ai.Done<Itinerary> => {
                assert.equal(d.result.daily_plan.at(0) ?? "", "no hotel booked")
            },
            _ => assert.is_true(false),
        };
        assert
            .is_true(
                s.journal.entries.some((en) -> {
                    en.event is root.ai.ToolFailed
                }),
            )
    }

    test "budget middleware throws a typed error past the cap" {
        let eng = root.ai.WithBudget<never> {
            inner: root.ai.ToolLoop<never> { max_steps: 8, task_mode: false },
            cap_usd: 0.0001,
            spent: 0.0,
        };
        let st = eng.init();
        let j = root.ai.new_journal<root.ai.Event>();
        let spent = (eng.update(st, j, root.ai.usage_of(100000, 100000)).length() * 1.0) catch (e) {
            let c: root.ai.CostBudgetExceeded => c.spent_usd,
        };
        assert.is_true(spent > 0.0001)
    }

    test "the default loop fans in parallel tools before recalling the model" {
        let eng = root.ai.ToolLoop<never> { max_steps: 8, task_mode: false };
        let st = eng.init();
        let j = root.ai.new_journal<root.ai.Event>();
        let _ = eng.update(st, j, root.ai.tool_requested("a", "search_flights", "{}"));
        let _ = eng.update(st, j, root.ai.tool_requested("b", "search_hotels", "{}"));
        assert.equal(eng.update(st, j, root.ai.tool_completed("a", "[]")).length(), 0);
        let after_both = eng.update(st, j, root.ai.tool_completed("b", "[]"));
        assert
            .is_true(
                after_both.some((c) -> {
                    c is root.ai.CallModel
                }),
            )
    }
}
