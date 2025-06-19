import { b } from "../test-setup";

describe("Gemini Provider", () => {
  describe("Streaming", () => {
    it("should support streaming in Gemini", async () => {
      const stream = b.stream.TestGemini("Dr. Pepper");
      const msgs: string[] = [];
      for await (const msg of stream) {
        msgs.push(msg ?? "");
      }
      const final = await stream.getFinalResponse();

      expect(final.length).toBeGreaterThan(0);
      expect(msgs.length).toBeGreaterThan(0);
      for (let i = 0; i < msgs.length - 2; i++) {
        expect(msgs[i + 1].startsWith(msgs[i])).toBeTruthy();
      }
      expect(msgs.at(-1)).toEqual(final);
    }, 20_000);
  });

  describe("system message", () => {
    it("should support system_instructions in Gemini", async () => {
      const _ = await b.TestGeminiSystem("Dr. Pepper");
    });
  });
});
