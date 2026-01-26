import { b, b_sync, ClientRegistry } from "./test-setup";

describe("Client Option Tests", () => {
  it("should route to correct client when using client option", async () => {
    // ExtractResume normally uses GPT4 (openai), but we override to Claude
    // Note: ExtractResume has an optional img param, so we pass null for it
    const request = await b.request.ExtractResume("test resume", null, {
      client: "Claude",
    });

    // Should route to Anthropic API
    expect(request.url.toLowerCase()).toContain("anthropic");
  });

  it("client option should take precedence over clientRegistry", async () => {
    const cr = new ClientRegistry();
    cr.setPrimary("GPT4"); // This should be overridden

    const request = await b.request.ExtractResume("test resume", null, {
      client: "Claude",
      clientRegistry: cr,
    });

    // client option should win - should route to Anthropic, not OpenAI
    expect(request.url.toLowerCase()).toContain("anthropic");
  });

  it("clientRegistry without client option should still work", async () => {
    const cr = new ClientRegistry();
    cr.setPrimary("Claude");

    const request = await b.request.ExtractResume("test resume", null, {
      clientRegistry: cr,
    });

    // Should route to Anthropic API
    expect(request.url.toLowerCase()).toContain("anthropic");
  });

  it("sync client should work with client option", () => {
    const request = b_sync.request.ExtractResume("test resume", null, {
      client: "Claude",
    });

    // Should route to Anthropic API
    expect(request.url.toLowerCase()).toContain("anthropic");
  });
});
