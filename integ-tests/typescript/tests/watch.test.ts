import { b, watchers, b_sync } from "./test-setup";

describe("Watch tests", () => {
  it("should watch basic changes", async () => {
    const watcher = watchers.WorkflowWatch();
    const wrong_listener = watchers.SumFromTo();
    let saw_change = false;
    watcher.on_var("x", (ev) => {
      console.log(ev);
      // ev.value is typed as number (inferred from "x" channel)
      saw_change = true;
    });
    watcher.on_var("once", (ev) => {
      // ev.value is typed as string (inferred from "once" channel)
    });
    watcher.on_var("twice", (ev) => {
      // ev.value is typed as string[] (inferred from "twice" channel)
    });

    let snapshots: any[] = [];
    watcher.on_stream("story", async (notification) => {
      // event is VarEvent<BamlStream<number | null, number | null>>
      // event.value is BamlStream<number | null, number | null>
      for await (const chunk of notification.value) {
        console.log("CHUNK!");
        console.log(chunk);
        snapshots.push(chunk);
      }
    });

    const response = await b.WorkflowWatch({ events: watcher });
    // Sleep for 0.5 seconds to allow events to finish streaming in.
    await new Promise((resolve) => setTimeout(resolve, 500));

    expect(saw_change).toBe(true);
    expect(snapshots.length).toBeGreaterThan(1);
    // const response2 = await b.WorkflowWatch({events: wrong_listener});
  });
});
