import { b } from "./test-setup";

describe("Literal types tests", () => {
  it("return literal union", async () => {
    const res = await b.LiteralUnionsTest("a");
    expect(res == 1 || res == true || res == "string output").toBeTruthy();
  });

  it("single literal int", async () => {
    const res = await b.TestNamedArgsLiteralInt(1);
    expect(res).toContain("1");
  });

  it("single literal bool", async () => {
    const res = await b.TestNamedArgsLiteralBool(true);
    expect(res).toContain("true");
  });

  it("single literal string", async () => {
    const res = await b.TestNamedArgsLiteralString("My String");
    expect(res).toContain("My String");
  });

  it("single class with literal prop", async () => {
    const res = await b.FnLiteralClassInputOutput({ prop: "hello" });
    expect(res).toEqual({ prop: "hello" });
  });

  it("single class with literal union prop", async () => {
    const res = await b.FnLiteralUnionClassInputOutput({ prop: "one" });
    expect(res).toEqual({ prop: "one" });
  });

  it("literal string union key in map", async () => {
    type MapKey = "one" | "two" | "three" | "four";
    const res = await b.InOutLiteralStringUnionMapKey(
      { one: "1" } as { [K in MapKey]?: string },
      { two: "2" } as { [K in MapKey]?: string },
    );
    expect(res).toHaveProperty("one", "1");
    expect(res).toHaveProperty("two", "2");
  });

  it("single literal string key in map", async () => {
    const res = await b.InOutSingleLiteralStringMapKey({ key: "1" });
    expect(res).toHaveProperty("key", "1");
  });
});
