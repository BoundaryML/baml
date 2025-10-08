import { b, events, b_sync } from "./test-setup";

describe("Emit tests", () => {
  it("should emit basic changes", async () => {
    const listener = events.WorkflowEmit();
    const wrong_listener = events.SumFromTo();
    let saw_change = false;
    listener.on_var("x", (ev) => {
      console.log(ev);
      // ev.value is typed as number (inferred from "x" channel)
      saw_change = true;
    });
    listener.on_var("once", (ev) => {
      // ev.value is typed as string (inferred from "once" channel)
    });
    listener.on_var("twice", (ev) => {
      // ev.value is typed as string[] (inferred from "twice" channel)
    });

    let snapshots: any[] = [];
    listener.on_stream("story", async (event) => {
      // event is VarEvent<BamlStream<number | null, number | null>>
      // event.value is BamlStream<number | null, number | null>
      for await (const chunk of event.value) {
        console.log("CHUNK!");
        console.log(chunk);
        snapshots.push(chunk);
      }
    });

    const response = await b.WorkflowEmit({ events: listener });
    // Sleep for 0.5 seconds to allow events to finish streaming in.
    await new Promise((resolve) => setTimeout(resolve, 500));

    expect(saw_change).toBe(true);
    expect(snapshots.length).toBeGreaterThan(1);
    // const response2 = await b.WorkflowEmit({events: wrong_listener});
  });
});
