// Scenario 09 — Errors and retries: the model is the first retry
// mechanism; budgets throw; impossibility belongs in the return type.
// Mirrors pages/04_advanced/01_errors_and_retries.md.

class CannotPlan {
    reason: string,
    missing_info: string[],
}

//# "letting the agent fail well": failure as a member of the return union —
//# the session is typed over the union, so run() parses either variant.
type PlanOutcome = Itinerary | CannotPlan

function plan_trip_or_cannot(request: string, client: root.ai.Client) -> string {
    let tb = root.ai.Toolbox { entries: [root.ai.tool(search_flights)] };
    let s = root.ai.new_session<PlanOutcome, never>(
        function_name = "PlanTripOrCannot",
        args_json = baml.json.to_string({ "request": request }),
        client = client,
        toolbox = tb,
        policy = root.ai.ToolLoop<never> { max_steps: 12, task_mode: true },
        output_schema = itinerary_schema(),
    );
    match (s.run()) {
        let d: root.ai.Done<PlanOutcome> => {
            match (d.result) {
                let trip: Itinerary => `planned: ${trip.destination}`,
                let cannot: CannotPlan => `cannot plan: ${cannot.reason}`,
            }
        },
        let r: root.ai.Replied => `stopped: ${r.message}`,
    }
}

testset "s09 errors" {
    test "bad tool arguments never crash the loop — the model sees and corrects" {
        let fake = root.ai.ScriptedClient<never> { i: 0, turns: [
            [root.ai.tool_requested("t1", "search_flights", `{"origin":"SFO","wrong_param":"NRT"}`)],
            [root.ai.tool_requested("t2", "search_flights", `{"origin":"SFO","dest":"NRT"}`)],
            [root.ai.final_produced(`{"destination":"Japan","days":14,"flights":[],"hotel":null,"daily_plan":[]}`)],
        ] };
        let trip = plan_trip("2 weeks in Japan", fake);
        assert.equal(trip.days, 14);

        //# the journal shows the whole feedback loop
        let s = plan_trip_session("2 weeks in Japan", root.ai.ScriptedClient<never> { i: 0, turns: [
            [root.ai.tool_requested("t1", "search_flights", `{"origin":"SFO","wrong_param":"NRT"}`)],
            [root.ai.final_produced(`{"destination":"Japan","days":1,"flights":[],"hotel":null,"daily_plan":[]}`)],
        ] }, task_mode = true);
        let _ = s.run();
        //# validation failures are ToolFailed events, readable by the model
        let error_result = s.journal.entries.some((en) -> {
            match (en.event) {
                let t: root.ai.ToolFailed => t.error.includes("bad arguments"),
                _ => false,
            }
        });
        assert.is_true(error_result)
    }

    test "the step budget throws; handle at the call site" {
        // a model that loops on tools forever
        let fake = root.ai.ScriptedClient<never> { i: 0, turns: [
            [root.ai.tool_requested("t1", "search_flights", `{"origin":"A","dest":"B"}`)],
            [root.ai.tool_requested("t2", "search_flights", `{"origin":"A","dest":"B"}`)],
            [root.ai.tool_requested("t3", "search_flights", `{"origin":"A","dest":"B"}`)],
        ] };
        let s = plan_trip_session("Japan", fake, task_mode = true);
        s.set_policy(root.ai.ToolLoop<never> { max_steps: 2, task_mode: true });
        let outcome = (match (s.run()) {
            let d: root.ai.Done<Itinerary> => "done",
            let r: root.ai.Replied => "replied",
        }) catch_all (e) {
            let t: root.ai.StepBudgetExceeded => `budget: ${t.message} (${t.steps} steps)`,
            _ => "unexpected",
        };
        assert.contains(outcome, "step budget exhausted");
        assert.contains(outcome, "(2 steps)")
    }

    test "an exhausted session revives: refold swallows the old throw, a message resets the budget" {
        let fake = root.ai.ScriptedClient<never> { i: 0, turns: [
            [root.ai.tool_requested("t1", "search_flights", `{"origin":"A","dest":"B"}`)],
            [root.ai.tool_requested("t2", "search_flights", `{"origin":"A","dest":"B"}`)],
            [root.ai.tool_requested("t3", "search_flights", `{"origin":"A","dest":"B"}`)],
        ] };
        let s = plan_trip_session("Japan", fake, task_mode = true);
        s.set_policy(root.ai.ToolLoop<never> { max_steps: 2, task_mode: true });
        let _ = (match (s.run()) {
            let d: root.ai.Done<Itinerary> => "done",
            let r: root.ai.Replied => "replied",
        }) catch_all (e) { _ => "budget" };

        //# resuming replays a journal whose live run ABORTED on the budget;
        //# the refold swallows that throw instead of failing the resume
        let fake2 = root.ai.ScriptedClient<never> { i: 0, turns: [
            [root.ai.final_produced(`{"destination":"Japan","days":3,"flights":[],"hotel":null,"daily_plan":[]}`)],
        ] };
        let tb = root.ai.Toolbox { entries: [root.ai.tool(search_flights), root.ai.tool(search_hotels)] };
        let s2: root.ai.Session<Itinerary, never> = root.ai.resume_session<Itinerary, never>(
            s.snapshot(),
            client = fake2,
            toolbox = tb,
            policy = root.ai.ToolLoop<never> { max_steps: 2, task_mode: true },
            output_schema = itinerary_schema(),
        );
        //# a new message resets the per-turn budget and the run completes
        s2.send("try again, just pick something");
        match (s2.run()) {
            let d: root.ai.Done<Itinerary> => assert.equal(d.result.days, 3),
            _ => assert.is_true(false),
        }
    }

    test "with_retry re-runs a transient tool failure, then gives up to the model" {
        //# a tool that always fails; only "flaky" is listed as retryable
        let tb = root.ai.Toolbox { entries: [
            root.ai.tool((city: string) -> { throw baml.errors.Io { message: `weather api down (${city})` } }, name = "flaky"),
        ] };
        let fake = root.ai.ScriptedClient<never> { i: 0, turns: [
            [root.ai.tool_requested("t1", "flaky", `{"city":"Kyoto"}`)],
            [root.ai.final_produced(`{"destination":"Japan","days":2,"flights":[],"hotel":null,"daily_plan":["indoor day"]}`)],
        ] };
        let s = root.ai.new_session<Itinerary, never>(
            function_name = "PlanTrip",
            args_json = `{"request":"Japan"}`,
            client = fake,
            toolbox = tb,
            policy = root.ai.WithRetry<never> {
                inner: root.ai.ToolLoop<never> { max_steps: 8, task_mode: true },
                max_attempts: 2,
                tools: ["flaky"],
            },
            output_schema = itinerary_schema(),
        );
        match (s.run()) {
            let d: root.ai.Done<Itinerary> => assert.equal(d.result.days, 2),
            _ => assert.is_true(false),
        };
        //# both attempts are in the journal; then the model saw the failure
        let failures = s.journal.entries.filter((en) -> {
            en.event is root.ai.ToolFailed
        });
        assert.equal(failures.length(), 2)
    }

    test "impossible tasks return a typed refusal, not a timeout" {
        let fake = root.ai.ScriptedClient<never> { i: 0, turns: [
            [root.ai.final_produced(`{"reason":"no dates given","missing_info":["travel dates","budget"]}`)],
        ] };
        assert.contains(plan_trip_or_cannot("plan something", fake), "cannot plan: no dates given");

        let fake2 = root.ai.ScriptedClient<never> { i: 0, turns: [
            [root.ai.final_produced(`{"destination":"Japan","days":5,"flights":[],"hotel":null,"daily_plan":[]}`)],
        ] };
        assert.contains(plan_trip_or_cannot("5 days in Japan", fake2), "planned: Japan")
    }
}
