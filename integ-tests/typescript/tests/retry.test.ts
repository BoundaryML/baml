import { b } from "./test-setup";
import assert from "assert";

describe("Retry Policies", () => {
  it("works with retries1", async () => {
    try {
      await b.TestRetryConstant();
      assert(false);
    } catch (e) {
      console.log("Expected error", e);
    }
  });

  it("works with retries2", async () => {
    try {
      await b.TestRetryExponential();
      assert(false);
    } catch (e) {
      console.log("Expected error", e);
    }
  });

  it("works with fallbacks", async () => {
    const res = await b.TestFallbackClient();
    expect(res.length).toBeGreaterThan(0);
  });
});
