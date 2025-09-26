import OpenAI from "openai";
import {
  ChatCompletionCreateParamsNonStreaming,
  ChatCompletionCreateParamsStreaming,
} from "openai/resources";
import Anthropic from "@anthropic-ai/sdk";
import { MessageCreateParamsNonStreaming } from "@anthropic-ai/sdk/resources";
import {
  GenerateContentRequest,
  GoogleGenerativeAI,
} from "@google/generative-ai";
import { SignatureV4 } from "@smithy/signature-v4";
import { fromEnv } from "@aws-sdk/credential-providers";
import { HttpRequest } from "@smithy/protocol-http";
import { Sha256 } from "@aws-crypto/sha256-js";
import { HTTPRequest as BamlHttpRequest } from "@boundaryml/baml";
import { Resume } from "../baml_client/types";
import { b, ClientRegistry } from "./test-setup";

const LONG_CACHEABLE_CONTEXT = Array.from({ length: 600 })
  .map(() => "Reusable cacheable context paragraph.")
  .join(" ");

const JOHN_DOE_TEXT_RESUME = `
  John Doe
  johndoe@example.com
  (123) 456-7890
  Software Engineer
  Python, JavaScript, SQL

  Education
  University of California, Berkeley (Berkeley, CA)
  Master's in Computer Science

  Experience
  Software Engineer at Google (2020 - Present)
`;

const JOHN_DOE_PARSED_RESUME = {
  name: "John Doe",
  email: "johndoe@example.com",
  phone: "(123) 456-7890",
  experience: ["Software Engineer at Google (2020 - Present)"],
  education: [
    {
      institution: "University of California, Berkeley",
      location: "Berkeley, CA",
      degree: "Master's",
      major: ["Computer Science"],
      graduation_date: null,
    },
  ],
  skills: ["Python", "JavaScript", "SQL"],
};

const JANE_SMITH_TEXT_RESUME = `
  Jane Smith
  janesmith@example.com
  (555) 123-4567
  Data Scientist
  Python, R, TensorFlow, PyTorch, SQL

  Education
  Stanford University (Stanford, CA)
  Ph.D. in Statistics

  Experience
  Senior Data Scientist at Netflix (2019 - Present)
  Machine Learning Engineer at Amazon (2016 - 2019)
`;

const JANE_SMITH_PARSED_RESUME = {
  name: "Jane Smith",
  email: "janesmith@example.com",
  phone: "(555) 123-4567",
  experience: [
    "Senior Data Scientist at Netflix (2019 - Present)",
    "Machine Learning Engineer at Amazon (2016 - 2019)",
  ],
  education: [
    {
      institution: "Stanford University",
      location: "Stanford, CA",
      degree: "Ph.D.",
      major: ["Statistics"],
      graduation_date: null,
    },
  ],
  skills: ["Python", "R", "TensorFlow", "PyTorch", "SQL"],
};

describe("Modular API Tests", () => {
  it("modular openai gpt4", async () => {
    const client = new OpenAI();

    // as ChatCompletionCreateParamsNonStreaming not necessary in TS since
    // .json() returns "any".
    const req = await b.request.ExtractResume2(JOHN_DOE_TEXT_RESUME);
    const res = await client.chat.completions.create(
      req.body.json() as ChatCompletionCreateParamsNonStreaming,
    );
    const parsed = b.parse.ExtractResume2(res.choices[0].message.content!);

    expect(parsed).toEqual(JOHN_DOE_PARSED_RESUME);
  });

  it("modular anthropic claude 3 haiku", async () => {
    const client = new Anthropic();

    const clientRegistry = new ClientRegistry();
    clientRegistry.setPrimary("Claude");

    const req = await b.request.ExtractResume2(JOHN_DOE_TEXT_RESUME, {
      clientRegistry,
    });
    const res = await client.messages.create(
      req.body.json() as MessageCreateParamsNonStreaming,
    );

    // Narrow type
    // https://github.com/anthropics/anthropic-sdk-typescript/issues/432
    if (res.content[0].type != "text") {
      throw `Unexpected type for content block: ${res.content[0]}`;
    }

    const parsed = b.parse.ExtractResume2(res.content[0].text);

    expect(parsed).toEqual(JOHN_DOE_PARSED_RESUME);
  });

  it("modular google gemini", async () => {
    const client = new GoogleGenerativeAI(process.env.GOOGLE_API_KEY!);
    const model = client.getGenerativeModel({ model: "gemini-1.5-pro" });

    const clientRegistry = new ClientRegistry();
    clientRegistry.setPrimary("Gemini");

    const req = await b.request.ExtractResume2(JOHN_DOE_TEXT_RESUME, {
      clientRegistry,
    });
    const res = await model.generateContent(
      req.body.json() as GenerateContentRequest,
    );
    const parsed = b.parse.ExtractResume2(res.response.text());

    expect(parsed).toEqual(JOHN_DOE_PARSED_RESUME);
  });

  it("modular openai gpt4 manual http request", async () => {
    const req = await b.request.ExtractResume2(JOHN_DOE_TEXT_RESUME);

    const res = await fetch(req.url, {
      method: req.method,
      headers: req.headers as Record<string, string>,
      body: JSON.stringify(req.body.json()), // req.body.raw() or req.body.text() works as well
    });

    const body = (await res.json()) as any;

    const parsed = b.parse.ExtractResume2(body.choices[0].message.content);

    expect(parsed).toEqual(JOHN_DOE_PARSED_RESUME);
  });

  it("modular openai gpt4 streaming", async () => {
    const client = new OpenAI();

    const req = await b.streamRequest.ExtractResume2(JOHN_DOE_TEXT_RESUME);

    const stream = await client.chat.completions.create(
      req.body.json() as ChatCompletionCreateParamsStreaming,
    );

    let llmResponse: string[] = [];

    for await (const chunk of stream) {
      if (chunk.choices.length > 0 && chunk.choices[0].delta.content) {
        llmResponse.push(chunk.choices[0].delta.content);
      }
    }

    const parsed = b.parseStream.ExtractResume2(llmResponse.join(""));

    expect(parsed).toEqual(JOHN_DOE_PARSED_RESUME);
  });

  it("openai batch api", async () => {
    const client = new OpenAI();

    // Helper function to convert BAML HTTP request to OpenAI batch JSONL format
    const toOpenaiJsonl = (req: BamlHttpRequest): string => {
      const line = JSON.stringify({
        custom_id: req.id,
        method: "POST",
        url: "/v1/chat/completions",
        body: req.body.json(),
      });
      return `${line}\n`;
    };

    // Create requests for both resumes
    const [johnReq, janeReq] = await Promise.all([
      b.request.ExtractResume2(JOHN_DOE_TEXT_RESUME),
      b.request.ExtractResume2(JANE_SMITH_TEXT_RESUME),
    ]);

    const jsonl = toOpenaiJsonl(johnReq) + toOpenaiJsonl(janeReq);

    // Create batch input file
    const batchInputFile = await client.files.create({
      file: new File([jsonl], "batch.jsonl"),
      purpose: "batch",
    });

    // Create batch
    let batch = await client.batches.create({
      input_file_id: batchInputFile.id,
      endpoint: "/v1/chat/completions",
      completion_window: "24h",
      metadata: {
        description: "BAML Modular API TypeScript Batch Integ Test",
      },
    });

    let backoff = 1000; // milliseconds
    let attempts = 0;
    const maxAttempts = 30;

    while (true) {
      batch = await client.batches.retrieve(batch.id);
      attempts += 1;

      if (batch.status === "completed") {
        break;
      }

      if (attempts >= maxAttempts) {
        try {
          await client.batches.cancel(batch.id);
        } finally {
          throw "Batch failed to complete in time";
        }
      }

      await new Promise((resolve) => setTimeout(resolve, backoff));
      // backoff *= 2 // Exponential backoff
    }

    // Get output file
    const output = await client.files.content(batch.output_file_id!);

    // Process results
    const expected: Record<string, Resume> = {
      [johnReq.id]: JOHN_DOE_PARSED_RESUME,
      [janeReq.id]: JANE_SMITH_PARSED_RESUME,
    };

    const received: Record<string, Resume> = {};
    const outputJsonl = await output.text();

    for (const line of outputJsonl
      .split("\n")
      .filter((line) => line.trim().length > 0)) {
      const result = JSON.parse(line.trim());
      const llmResponse = result.response.body.choices[0].message.content;

      const parsed = b.parse.ExtractResume2(llmResponse);
      received[result.custom_id] = parsed;
    }

    expect(received).toEqual(expected);
  });

  it("modular openai responses", async () => {
    // Test openai-responses provider using the modular API
    const client = new OpenAI();

    // Use TestOpenAIResponses from the providers directory
    const req = await b.request.TestOpenAIResponses("mountains");

    // The openai-responses provider should use the /v1/responses endpoint
    const res = (await client.responses.create(req.body.json())) as any;

    // Parse the response from the responses API (uses output_text instead of choices)
    const parsed = b.parse.TestOpenAIResponses(res.output_text);

    expect(typeof parsed).toBe("string");
    expect(parsed.length).toBeGreaterThan(0);
  });

  it("modular aws bedrock custom cache point", async () => {
    const req = await b.request.TestAws("Dr. Pepper");

    const body = req.body.json() as any;
    expect(Array.isArray(body.messages)).toBe(true);
    expect(body.messages.length).toBeGreaterThan(0);

    const content = body.messages[0].content as any[];
    expect(Array.isArray(content)).toBe(true);

    const originalLength = content.length;
    content.splice(1, 0, { text: LONG_CACHEABLE_CONTEXT });
    content.splice(2, 0, { cachePoint: { type: "default" } });

    expect(content[1]).toEqual({ text: LONG_CACHEABLE_CONTEXT });
    expect(content[2]).toEqual({ cachePoint: { type: "default" } });
    expect(content.length).toBe(originalLength + 2);

    body.additionalModelRequestFields = {
      ...(body.additionalModelRequestFields ?? {}),
    };

    const bodyString = JSON.stringify(body);
    const url = new URL(req.url);
    const region =
      process.env.AWS_REGION ?? process.env.AWS_DEFAULT_REGION ?? "us-east-1";

    const signer = new SignatureV4({
      service: "bedrock",
      region,
      credentials: fromEnv(),
      sha256: Sha256,
    });

    const baseHeaders = Object.fromEntries(
      Object.entries(req.headers as Record<string, string | undefined>).filter(
        ([, value]) => value !== undefined,
      ),
    ) as Record<string, string>;

    const headers = {
      ...baseHeaders,
      host: url.host,
      "content-type": "application/json",
      accept: "application/json",
    };

    const unsigned = new HttpRequest({
      protocol: url.protocol,
      hostname: url.hostname,
      path: url.pathname,
      method: req.method,
      headers,
      body: bodyString,
    });

    const signed = await signer.sign(unsigned);
    const signedHeaders = Object.fromEntries(
      Object.entries(signed.headers).map(([key, value]) => [
        key,
        String(value),
      ]),
    ) as Record<string, string>;

    const res = await fetch(req.url, {
      method: req.method,
      headers: signedHeaders,
      body: bodyString,
    });

    if (!res.ok) {
      throw new Error(
        `Bedrock request failed: ${res.status} ${await res.text()}`,
      );
    }

    const payload = (await res.json()) as any;
    const contentBlocks = payload?.output?.message?.content ?? [];
    expect(Array.isArray(contentBlocks)).toBe(true);
    const textBlock =
      contentBlocks.find((block: any) => block.text)?.text ?? "";
    expect(typeof textBlock).toBe("string");
    expect(textBlock.length).toBeGreaterThan(0);
  });
});
