// Scenario 10 — the `$runner` decision from the appendix
// (05_appendix/02_alternatives_considered.md §1): one entry point, and the
// runner's associated Handle type picks what you get back. The appendix
// calls this projection "the most advanced typing in this BEP and the
// first thing the reference implementation must validate" — these tests
// are that validation.

testset "s10 runners" {
    test "the Blocking runner's handle is the session itself" {
        let fake = root.ai.ScriptedClient<never> {
            i: 0,
            turns: [
                [
                    root.ai
                        .final_produced(
                            `{"destination":"Japan","days":3,"flights":[],"hotel":null,"daily_plan":[]}`,
                        ),
                ],
            ],
        };
        //# same open expression, Handle = Session<Itinerary, never>
        let s: root.ai.Session<Itinerary, never> = root
            .ai
            .open_with(
                root.ai.BlockingRunner<Itinerary> {  },
                plan_trip_session("3 days in Japan", fake, task_mode = true),
            );
        match (s.run()) {
            let d: root.ai.Done<Itinerary> => assert.equal(d.result.days, 3),
            _ => assert.is_true(false),
        }
    }

    test "the job runner's handle is a Job: no send lane, poll for settlement" {
        let fake = root.ai.ScriptedClient<never> {
            i: 0,
            turns: [
                [
                    root.ai
                        .final_produced(
                            `{"destination":"Peru","days":21,"flights":[],"hotel":null,"daily_plan":[]}`,
                        ),
                ],
            ],
        };
        //# same open expression, Handle = Job<Itinerary>
        let job: root.ai.Job<Itinerary> = root
            .ai
            .open_with(
                root.ai.EagerJobRunner<Itinerary> {  },
                plan_trip_session("3 weeks across South America", fake, task_mode = true),
            );
        match (job.poll()) {
            let d: root.ai.JobDone<Itinerary> => assert.equal(d.result.destination, "Peru"),
            _ => assert.is_true(false),
        }
    }

    test "a job that stops conversationally settles Failed(needs_input)" {
        //# session mode + a reply-only model: run() yields Replied, and a
        //# detached job has nobody to answer (02_background_jobs.md)
        let fake = root.ai.ScriptedClient<never> {
            i: 0,
            turns: [[root.ai.assistant("what dates work for you?")]],
        };
        let job: root.ai.Job<Itinerary> = root
            .ai
            .open_with(
                root.ai.EagerJobRunner<Itinerary> {  },
                plan_trip_session("somewhere warm", fake),
            );
        match (job.poll()) {
            let f: root.ai.JobFailed => {
                assert.equal(f.reason, "needs_input");
                assert.contains(f.error, "what dates")
            },
            _ => assert.is_true(false),
        }
    }
}
