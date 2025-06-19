import { b } from "../test-setup";

describe("Vertex Provider", () => {
  it("should support vertex", async () => {
    const res = await b.TestVertex(
      "a donkey. You must mention the word donkey.",
    );
    expect(res.toLowerCase()).toContain("donkey");
  });

  it("should support vertex with system_instructions", async () => {
    const res = await b.TestVertexWithSystemInstructions();
    expect(res.length).toBeGreaterThan(0);
  });
});
