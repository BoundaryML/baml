
import { partial_types } from "../baml_client/partial_types";
import { b, b_sync } from "./test-setup";
import TypeBuilder from "../baml_client/type_builder";

describe("streambug", () => {
  it("streambug", async () => {
    let tb = new TypeBuilder();
    tb.StreamBugResult.addProperty("response", tb.string());
    tb.StreamBugResult.addProperty("followup_questions", tb.list(tb.string()));
    const stream = b.stream.StreamBug({ tb });

    let msgs: partial_types.StreamBugResult[] = [];
    for await (const msg of stream) {
      if (msg) {
        console.log(msg);
        msgs.push(msg);
      }
    }
  });
});