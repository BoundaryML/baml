import { b, events, b_sync } from "./test-setup";

describe("Emit tests", () => {
  it("should emit basic changes", async() {
    const listener = events.WorkflowEmit();
    const wrong_listener = events.SumFromTo();
    listener.on_var_x = (ev) => console.log(ev);

    const response = await b.WorkflowEmit({events: listener});
    const response2 = await b.WorkflowEmit({events: wrong_listener});

  })
}
