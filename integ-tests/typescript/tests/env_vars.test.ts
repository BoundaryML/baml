import { resetBamlEnvVars, traceAsync, traceSync } from "../baml_client";
import { b } from "./test-setup";

describe("Env Vars Tests", () => {
  it("should reset environment variables correctly", async () => {
    const envVars = {
      OPENAI_API_KEY: "sk-1234567890",
    };
    resetBamlEnvVars(envVars);

    const topLevelSyncTracing = traceSync("name", () => {
      resetBamlEnvVars(envVars);
    });

    const atopLevelAsyncTracing = traceAsync("name", async () => {
      resetBamlEnvVars(envVars);
    });

    await expect(async () => {
      topLevelSyncTracing();
    }).rejects.toThrow("BamlError");

    await expect(async () => {
      await atopLevelAsyncTracing();
    }).rejects.toThrow("BamlError");

    resetBamlEnvVars(
      Object.fromEntries(
        Object.entries(process.env).filter(([_, v]) => v !== undefined),
      ) as Record<string, string>,
    );
    const people = await b.ExtractPeople(
      "My name is Harrison. My hair is black and I'm 6 feet tall. I'm pretty good around the hoop.",
    );
    expect(people.length).toBeGreaterThan(0);
  });

  it("should reflect API key changes in headers", async () => {
    // First request with initial API key
    process.env.OPENAI_API_KEY = "sk-initial-key";
    const firstResult = await b.request.ExtractPeople(
      "My name is John. I am 30 years old.",
    );
    const firstHeaders = firstResult.headers as Record<string, string>;
    expect(firstHeaders["authorization"]).toBe("Bearer sk-initial-key");

    // Second request with changed API key
    process.env.OPENAI_API_KEY = "sk-new-key";
    const secondResult = await b.request.ExtractPeople(
      "My name is Jane. I am 25 years old.",
    );
    const secondHeaders = secondResult.headers as Record<string, string>;
    expect(secondHeaders["authorization"]).toBe("Bearer sk-new-key");
  });
});
