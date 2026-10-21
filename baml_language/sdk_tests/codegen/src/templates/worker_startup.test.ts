import { afterAll, beforeAll, expect, test } from "vitest";
import { createTestHarness } from "wrangler";

const server = createTestHarness({
  workers: [{ configPath: "./wrangler.jsonc" }],
});

beforeAll(async () => {
  await server.listen();
});

afterAll(async () => {
  await server.close();
});

test("starts the configured Worker and executes its generated SDK", async () => {
  for (let request = 0; request < 2; request++) {
    const response = await server.fetch("/");
    expect(response.status).toBe(200);
    expect(await response.text()).toBe("__EXPECTED_BODY__");
  }
});

test.runIf("__FIXTURE__" === "function_calls")("preserves opaque span identity in request handlers", async () => {
  for (let request = 0; request < 2; request++) {
    const response = await server.fetch("/reserved-span");
    expect(response.status).toBe(200);
    expect(await response.text()).toBe("true");
  }
});
