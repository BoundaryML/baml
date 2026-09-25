import { expect, it } from "vitest";
import { hello_world, hello_world_async } from "./baml_sdk/index.js";

it("calls hello_world", async () => {
  expect(hello_world()).toBe("hello world");
  expect(await hello_world_async()).toBe("hello world");
});
