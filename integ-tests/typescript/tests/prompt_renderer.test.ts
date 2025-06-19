import { AliasedEnum } from "../baml_client";
import { b } from "./test-setup";

describe("Prompt Renderer Tests", () => {
  it("should use aliases when serializing input objects - classes", async () => {
    const res = await b.AliasedInputClass({ key: "hello", key2: "world" });
    expect(res).toContain("color");

    const res2 = await b.AliasedInputClassNested({
      key: "hello",
      nested: { key: "nested-hello", key2: "nested-world" },
    });
    expect(res2).toContain("interesting-key");
  });

  it("should use aliases when serializing, but still have original keys in jinja", async () => {
    const res = await b.AliasedInputClass2({ key: "tiger", key2: "world" });
    expect(res.toLowerCase()).toContain("tiger");

    const res2 = await b.AliasedInputClassNested({
      key: "hello",
      nested: { key: "nested-hello", key2: "nested-world" },
    });
    expect(res2).toContain("interesting-key");
  });

  // TODO: Enum aliases are not supported
  it("should use aliases when serializing input objects - enums", async () => {
    const res = await b.AliasedInputEnum(AliasedEnum.KEY_ONE);
    expect(res.toLowerCase()).not.toContain("tiger");
  });

  // TODO: enum aliases are not supported
  it("should use aliases when serializing input objects - lists", async () => {
    const res = await b.AliasedInputList([
      AliasedEnum.KEY_ONE,
      AliasedEnum.KEY_TWO,
    ]);
    expect(res.toLowerCase()).not.toContain("tiger");
  });

  it("maintain field order", async () => {
    const request = await b.request.UseMaintainFieldOrder({
      a: "1",
      b: "2",
      c: "3",
    });

    expect(request.body.json()).toEqual({
      model: "gpt-4o-mini",
      messages: [
        {
          role: "system",
          content: [
            {
              type: "text",
              text: `Return this value back to me: {
    "a": "1",
    "b": "2",
    "c": "3",
}`,
            },
          ],
        },
      ],
    });
  });
});
