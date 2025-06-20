import { b, b_sync } from "./test-setup";

describe("Expose Request Tests", () => {
  it("should expose request gpt4", async () => {
    const request = await b.request.ExtractReceiptInfo(
      "test@email.com",
      "curiosity",
    );

    expect(request.body.json()).toEqual({
      model: "gpt-4o",
      messages: [
        {
          role: "system",
          content: [
            {
              type: "text",
              text: 'Given the receipt below:\n\n```\ntest@email.com\n```\n\nAnswer in JSON using this schema:\n{\n  items: [\n    {\n      name: string,\n      description: string or null,\n      quantity: int,\n      price: float,\n    }\n  ],\n  total_cost: float or null,\n  venue: "barisa" or "ox_burger",\n}',
            },
          ],
        },
      ],
    });
  });

  it("should expose request gemini", async () => {
    const request = await b.request.TestGeminiSystemAsChat("Dr. Pepper");

    expect(request.body.json()).toEqual({
      system_instruction: {
        parts: [{ text: "You are a helpful assistant" }],
      },
      contents: [
        {
          parts: [
            {
              text: "Write a nice short story about Dr. Pepper. Keep it to 15 words or less.",
            },
          ],
          role: "user",
        },
      ],
      safetySettings: {
        category: "HARM_CATEGORY_HATE_SPEECH",
        threshold: "BLOCK_LOW_AND_ABOVE",
      },
    });
  });

  it("should expose request fallback", async () => {
    // First client in strategy is GPT4Turbo
    const request = await b.request.TestFallbackStrategy("Dr. Pepper");

    expect(request.body.json()).toEqual({
      messages: [
        {
          role: "system",
          content: [
            {
              type: "text",
              text: "You are a helpful assistant.",
            },
          ],
        },
        {
          role: "user",
          content: [
            {
              type: "text",
              text: "Write a nice short story about Dr. Pepper",
            },
          ],
        },
      ],
      model: "gpt-4-turbo",
    });
  });

  it("should expose request round robin", async () => {
    // First client in strategy is Claude
    const request = await b.request.TestRoundRobinStrategy("Dr. Pepper");

    expect(request.body.json()).toEqual({
      messages: [
        {
          role: "user",
          content: [
            {
              type: "text",
              text: "Write a nice short story about Dr. Pepper",
            },
          ],
        },
      ],
      system: [
        {
          type: "text",
          text: "You are a helpful assistant.",
        },
      ],
      model: "claude-3-haiku-20240307",
      max_tokens: 1000,
    });
  });

  it("should expose request gpt4 sync", () => {
    // Assuming there's a sync client in the TypeScript implementation
    const request = b_sync.request.ExtractReceiptInfo(
      "test@email.com",
      "curiosity",
    );

    expect(request.body.json()).toEqual({
      messages: [
        {
          role: "system",
          content: [
            {
              type: "text",
              text: 'Given the receipt below:\n\n```\ntest@email.com\n```\n\nAnswer in JSON using this schema:\n{\n  items: [\n    {\n      name: string,\n      description: string or null,\n      quantity: int,\n      price: float,\n    }\n  ],\n  total_cost: float or null,\n  venue: "barisa" or "ox_burger",\n}',
            },
          ],
        },
      ],
      model: "gpt-4o",
    });
  });

  it("should expose request gpt4 stream", async () => {
    const request = await b.streamRequest.ExtractReceiptInfo(
      "test@email.com",
      "curiosity",
    );

    expect(request.body.json()).toEqual({
      messages: [
        {
          role: "system",
          content: [
            {
              type: "text",
              text: 'Given the receipt below:\n\n```\ntest@email.com\n```\n\nAnswer in JSON using this schema:\n{\n  items: [\n    {\n      name: string,\n      description: string or null,\n      quantity: int,\n      price: float,\n    }\n  ],\n  total_cost: float or null,\n  venue: "barisa" or "ox_burger",\n}',
            },
          ],
        },
      ],
      model: "gpt-4o",
      stream: true,
      stream_options: {
        include_usage: true,
      },
    });
  });

  it("should expose request gpt4 stream sync", () => {
    // Assuming there's a sync stream request in the TypeScript implementation
    const request = b_sync.streamRequest.ExtractReceiptInfo(
      "test@email.com",
      "curiosity",
    );

    expect(request.body.json()).toEqual({
      messages: [
        {
          role: "system",
          content: [
            {
              type: "text",
              text: 'Given the receipt below:\n\n```\ntest@email.com\n```\n\nAnswer in JSON using this schema:\n{\n  items: [\n    {\n      name: string,\n      description: string or null,\n      quantity: int,\n      price: float,\n    }\n  ],\n  total_cost: float or null,\n  venue: "barisa" or "ox_burger",\n}',
            },
          ],
        },
      ],
      model: "gpt-4o",
      stream: true,
      stream_options: {
        include_usage: true,
      },
    });
  });
});
