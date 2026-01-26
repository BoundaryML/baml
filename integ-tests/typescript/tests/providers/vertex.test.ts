import { b, ClientRegistry } from "../test-setup";

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

  it("should support vertex api key in query params", async () => {

    process.env["BAML_LOG"] = "info";
    const clientRegistry = new ClientRegistry();

    clientRegistry.setPrimary("VertexWithQueryParams");
    const res = await b.TestVertex("Donkey Kong", { clientRegistry });
    expect(res.toLowerCase()).toContain("donkey");
  });

  // it("should support vertex with google/ prefix in model name", async () => {
    // const res = await b.TestVertexWithGooglePrefix(
    //   "a donkey. You must mention the word donkey.",
    // );
    // expect(res.toLowerCase()).toContain("donkey");
  // });
});
