import { b, events, b_sync } from "./test-setup";

describe("Emit tests", () => {
  it("should emit basic changes", async () => {
    const listener = events.WorkflowEmit();
    const wrong_listener = events.SumFromTo();
    let saw_change = false;
    listener.on_var_x((ev) => {
      console.log(ev);
      saw_change = true;
    });

    const response = await b.WorkflowEmit({ events: listener });
    expect(saw_change).toBe(true);
    // const response2 = await b.WorkflowEmit({events: wrong_listener});
  });
});
