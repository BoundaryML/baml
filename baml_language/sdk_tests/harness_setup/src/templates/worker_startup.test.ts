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
  const response = await server.fetch("/");
  expect(response.status).toBe(200);
  expect(await response.text()).toBe("__EXPECTED_BODY__");
});
