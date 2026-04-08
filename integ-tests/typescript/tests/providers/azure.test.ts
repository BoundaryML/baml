import { b } from "../test-setup";
import { ClientRegistry } from "../test-setup";

describe("Azure Provider", () => {
  it("should support azure with default max_tokens", async () => {
    const res = await b.TestAzure("Donkey Kong (must use donkey)");
    expect(res.toLowerCase()).toContain("donkey");
  });

  it("should support o1 model without max_tokens", async () => {
    const res = await b.TestAzureO1NoMaxTokens(
      "Donkey Kong. Mention the word donkey.",
    );
    expect(res.toLowerCase()).toContain("donkey");
  });

  it("should fail when setting max_tokens for o1 model", async () => {
    await expect(async () => {
      await b.TestAzureO1WithMaxTokens("Donkey Kong");
    }).rejects.toThrow(/max_tokens.*not supported/);
  });

  it("should support non-o1 model with explicit max_tokens", async () => {
    const res = await b.TestAzureWithMaxTokens("Donkey Kong (must use donkey)");
    expect(res.toLowerCase()).toContain("donkey");
  });

  it("should support o1 model with explicit max_completion_tokens", async () => {
    const res = await b.TestAzureO1WithMaxCompletionTokens("Donkey Kong (must use donkey)");
    expect(res.toLowerCase()).toContain("donkey");
  });

  it("should fail if azure is not configured correctly", async () => {
    await expect(async () => {
      await b.TestAzureFailure("Donkey Kong");
    }).rejects.toThrow("BamlClientError");
  });

  it("should support azure streaming", async () => {
    const stream = b.stream.TestAzure("Donkey Kong");
    const msgs: string[] = [];
    for await (const msg of stream) {
      msgs.push(msg ?? "");
    }
    const final = await stream.getFinalResponse();
    expect(final.length).toBeGreaterThan(0);
  });



  // it('should fail if azure is not configured streaming', async () => {
  //   const stream = b.stream.TestAzureFailure('Donkey Kong')
  //   await expect(async () => {
  //     // this should throw an error, not only when we try to get the final response
  //     for await (const msg of stream) {
  //       console.log('msg', msg)
  //     }
  //     // await stream.getFinalResponse()
  //   }).rejects.toThrow('BamlClientError')
  // })
});

describe("Azure Entra ID Provider", () => {
  // These tests require AZURE_TENANT_ID, AZURE_CLIENT_ID, AZURE_CLIENT_SECRET,
  // AZURE_OPENAI_RESOURCE_NAME, and AZURE_OPENAI_DEPLOYMENT_ID to be set.
  // They are skipped automatically when those env vars are not present.
  const hasEntraIdCreds =
    !!process.env.AZURE_TENANT_ID &&
    !!process.env.AZURE_CLIENT_ID &&
    !!process.env.AZURE_CLIENT_SECRET &&
    !!process.env.AZURE_OPENAI_RESOURCE_NAME &&
    !!process.env.AZURE_OPENAI_DEPLOYMENT_ID;

  const hasSystemDefaultCreds =
    !!process.env.AZURE_TENANT_ID &&
    !!process.env.AZURE_CLIENT_ID &&
    !!process.env.AZURE_OPENAI_RESOURCE_NAME &&
    !!process.env.AZURE_OPENAI_DEPLOYMENT_ID;

  const itEntra = hasEntraIdCreds ? it : it.skip;
  const itSystemDefault = hasSystemDefaultCreds ? it : it.skip;

  itEntra("should support Entra ID service principal authentication", async () => {
    const res = await b.TestAzureEntraId("Autumn leaves falling");
    expect(res.toLowerCase()).toMatch(/autumn|leaves|fall/);
  });

  itSystemDefault("should support Entra ID system default credential chain", async () => {
    const res = await b.TestAzureEntraIdSystemDefault("Mountain stream");
    expect(res.length).toBeGreaterThan(0);
  });
});
