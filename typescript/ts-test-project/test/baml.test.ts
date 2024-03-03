import { trace, initTracer, setTags, traceAsync } from "baml-client-lib";


describe("describe", () => {
  test("test1", () => {
    expect(1).toBe(1);

    console.log(expect.getState().currentTestName);
  });
});

test("root_test2", async () => {
  // sleep for 4 secs
  await new Promise((resolve) => setTimeout(resolve, 3000));
  expect(1).toBe(1);
  setTags({ "foo": "bar" });
  await callLLM();
});

const callLLM = traceAsync(async () => {
  setTags({ "foo": "bar" });
  // TODO: set tags for the current test
  expect.getState().currentTestName;

  console.log("callLLM");
  return "callLLMResult";
}, 'callLLM', [{ name: "blah", type: "string" }], false, 'string');

