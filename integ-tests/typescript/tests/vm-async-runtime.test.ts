/**
 * Baml VM / compiler / expression functions with LLM calls. Ignored in CI.
 */

import { b } from "./test-setup";
import { Image } from "@boundaryml/baml";

describe("VM Async Runtime Tests", () => {
  it("should return number calling LLM", async () => {
    const result = await b.ReturnNumberCallingLlm(42);
    expect(result).toBe(42);
  });

  it("should store LLM call in local var", async () => {
    const result = await b.StoreLlmCallInLocalVar(42);
    expect(result).toBe(42);
  });

  it("should convert bool to int with if-else calling LLM", async () => {
    const result1 = await b.BoolToIntWithIfElseCallingLlm(true);
    expect(result1).toBe(1);

    const result2 = await b.BoolToIntWithIfElseCallingLlm(false);
    expect(result2).toBe(0);
  });

  it("should call LLM to describe image", async () => {
    // Call an expression function that calls an LLM function to check if the
    // media type is passed correctly.
    const description = await b.CallLlmDescribeImage(
      Image.fromUrl(
        "https://i.imgur.com/93fWs5R.png"
      )
    );

    expect(description.toLowerCase()).toContain("ogre");
  });

  // it("should execute fetch as", async () => {
  //   const result = await b.ExecFetchAs("https://dummyjson.com/todos/1");

  //   expect(result).toEqual({
  //     id: 1,
  //     todo: "Do something nice for someone you care about",
  //     completed: false,
  //     userId: 152,
  //   } as DummyJsonTodo);
  // });
});
