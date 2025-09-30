import { b, events, b_sync } from "./test-setup";

describe("Emit tests", () => {
  it("should emit basic changes", async() {
    const listener = events.WorkflowEmit();
    listener.on_var_x = (ev) => console.log(ev);
    const response = await b.WorkflowEmit({events: listener});

    expect(workflow)
  })
}
