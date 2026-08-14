import { describe, it, test } from "vitest";

const helper = () => {};
it("direct_case", helper);
test.only("only_case", helper);
it.skip("skip_case", helper);
it.todo("todo_case");
it.concurrent("concurrent_case", helper);
it.runIf(isTestRuntime("web"))("runtime_case", helper);
describe.runIf(isWebRuntime)("web_suite", () => {
  it("suite_runtime_case", helper);
});
it(`${prefix}_dynamic_case`, helper);
