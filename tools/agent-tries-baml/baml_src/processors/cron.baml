// Cron — the daily driver. Not a claim processor: a timer loop that periodically
// generates fresh hard benchmark tasks (via the GenerateTasks LLM function) and
// enqueues them. Ports services/cron (task-generation path).
//
// Focused port: the baml-version refresh, cohort fast-fan-in reconciler, and
// changelog poller are deferred; task generation is the core daily action.

class CronTaskDoc {
    source: string,
    prompt: string,
    status: string,
}

function run_cron(svc: ServiceClient, interval_ms: int, gen_count: int, generate: bool) -> int {
    while (true) {
        if (generate) {
            // Recently-run tasks would be rendered here to avoid repeats; "" for now.
            let gen = GenerateTasks("", gen_count) catch (_) {
                _ => TaskList { tasks: [] }
            };
            for (let prompt in gen.tasks) {
                let doc = baml
                    .json
                    .to_string(CronTaskDoc { source: "cron", prompt: prompt, status: "queued" }) catch (_) {
                    _ => ""
                };
                if (doc != "") {
                    let _ = svc.create("tasks", doc) catch (_) {
                        _ => ""
                    };
                }
            }
        }
        let _ = baml.sys.sleep(baml.time.Duration.from_milliseconds(interval_ms)) catch (_) {
            _ => null
        };
    }
    return 0;
}
