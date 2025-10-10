import { b, events, b_sync } from "./test-setup";

describe("Emit tests", () => {
  it("should emit basic changes", async () => {
    const listener = events.WorkflowEmit();
    const wrong_listener = events.SumFromTo();
    let saw_change = false;
    listener.on_var("x", (ev) => {
      console.log(ev);
      saw_change = true;
    });
    listener.on_var("once", (ev) => {
      console.log(ev);
    });
    listener.on_var("twice", (ev) => {
      console.log(ev);
    });

    const response = await b.WorkflowEmit({ events: listener });
    // Sleep for 0.5 seconds to allow events to finish streaming in.
    await new Promise((resolve) => setTimeout(resolve, 500));

    expect(saw_change).toBe(true);
    // const response2 = await b.WorkflowEmit({events: wrong_listener});
  });
});
