import { b } from "../test-setup";

describe("OpenAI Provider", () => {
  it("should support openai-responses basic", async () => {
    const res = await b.request.TestOpenAIResponses("lorem ipsum");
    expect(res.body.json()).toMatchObject({
      input: [
        {
          content: [
            {
              text: "Write a short haiku about lorem ipsum. Make it simple and beautiful.",
              type: "input_text",
            },
          ],
          role: "user",
        },
      ],
      model: "gpt-5-mini",
    });
  });

  it("should support openai-responses explicit", async () => {
    const res = await b.request.TestOpenAIResponsesExplicit("lorem ipsum");
    expect(res.body.json()).toMatchObject({
      input: [
        {
          content: [
            {
              text: "Create a brief poem about lorem ipsum. Keep it under 50 words.",
              type: "input_text",
            },
          ],
          role: "user",
        },
      ],
      model: "gpt-4.1",
    });
  });

  it("should support openai-responses custom url", async () => {
    const res = await b.request.TestOpenAIResponsesCustomURL("lorem ipsum");
    expect(res.body.json()).toMatchObject({
      input: [
        {
          content: [
            {
              text: "Tell me an interesting fact about lorem ipsum.",
              type: "input_text",
            },
          ],
          role: "user",
        },
      ],
      model: "gpt-4.1",
    });
  });

  it("should support openai-responses conversation", async () => {
    const res = await b.request.TestOpenAIResponsesConversation("lorem ipsum");
    expect(res.body.json()).toMatchObject({
      input: [
        {
          content: [
            {
              text: "You are a helpful assistant that provides concise answers.",
              type: "input_text",
            },
          ],
          role: "system",
        },
        {
          content: [
            {
              text: "What is lorem ipsum?",
              type: "input_text",
            },
          ],
          role: "user",
        },
        {
          content: [
            {
              text: "lorem ipsum is a fascinating subject. Let me explain briefly.",
              type: "output_text",
            },
          ],
          role: "assistant",
        },
        {
          content: [
            {
              text: "Can you give me a simple example?",
              type: "input_text",
            },
          ],
          role: "user",
        },
      ],
      model: "gpt-5-mini",
    });
  });

  it("should support openai-responses different model", async () => {
    const res = await b.request.TestOpenAIResponsesDifferentModel("lorem ipsum");
    expect(res.body.json()).toMatchObject({
      input: [
        {
          content: [
            {
              text: "Explain lorem ipsum in one sentence.",
              type: "input_text",
            },
          ],
          role: "user",
        },
      ],
      model: "gpt-4",
    });
  });
});
