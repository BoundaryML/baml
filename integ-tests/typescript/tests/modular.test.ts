import OpenAI from 'openai'
import { ChatCompletionCreateParamsNonStreaming, ChatCompletionCreateParamsStreaming } from 'openai/resources'
import Anthropic from '@anthropic-ai/sdk'
import { MessageCreateParamsNonStreaming } from '@anthropic-ai/sdk/resources'
import { GenerateContentRequest, GoogleGenerativeAI } from '@google/generative-ai'
import { HTTPRequest as BamlHttpRequest } from '@boundaryml/baml'
import { Resume } from "../baml_client/types"
import { b, ClientRegistry } from './test-setup'

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
`

const JOHN_DOE_PARSED_RESUME = {
  name: "John Doe",
  email: "johndoe@example.com",
  phone: "(123) 456-7890",
  experience: ["Software Engineer at Google (2020 - Present)"],
  education: [{
    institution: "University of California, Berkeley",
    location: "Berkeley, CA",
    degree: "Master's",
    major: ["Computer Science"],
    graduation_date: null
  }],
  skills: ["Python", "JavaScript", "SQL"]
}

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
`

const JANE_SMITH_PARSED_RESUME = {
  name: "Jane Smith",
  email: "janesmith@example.com",
  phone: "(555) 123-4567",
  experience: [
    "Senior Data Scientist at Netflix (2019 - Present)",
    "Machine Learning Engineer at Amazon (2016 - 2019)"
  ],
  education: [{
    institution: "Stanford University",
    location: "Stanford, CA",
    degree: "Ph.D.",
    major: ["Statistics"],
    graduation_date: null
  }],
  skills: ["Python", "R", "TensorFlow", "PyTorch", "SQL"]
}

describe('Modular API Tests', () => {
  it('modular openai gpt4', async () => {
    const client = new OpenAI()

    // as ChatCompletionCreateParamsNonStreaming not necessary in TS since
    // .json() returns "any".
    const req = await b.request.ExtractResume2(JOHN_DOE_TEXT_RESUME)
    const res = await client.chat.completions.create(req.body.json() as ChatCompletionCreateParamsNonStreaming)
    const parsed = b.parse.ExtractResume2(res.choices[0].message.content!)

    expect(parsed).toEqual(JOHN_DOE_PARSED_RESUME)
  })

  it('modular anthropic claude 3 haiku', async () => {
    const client = new Anthropic()

    const clientRegistry = new ClientRegistry()
    clientRegistry.setPrimary("Claude")

    const req = await b.request.ExtractResume2(JOHN_DOE_TEXT_RESUME, {clientRegistry})
    const res = await client.messages.create(req.body.json() as MessageCreateParamsNonStreaming)

    // Narrow type
    // https://github.com/anthropics/anthropic-sdk-typescript/issues/432
    if (res.content[0].type != "text") {
      throw `Unexpected type for content block: ${res.content[0]}`
    }

    const parsed = b.parse.ExtractResume2(res.content[0].text)

    expect(parsed).toEqual(JOHN_DOE_PARSED_RESUME)
  })

  it('modular google gemini', async () => {
    const client = new GoogleGenerativeAI(process.env.GOOGLE_API_KEY!)
    const model = client.getGenerativeModel({ model: "gemini-1.5-pro-001" })

    const clientRegistry = new ClientRegistry()
    clientRegistry.setPrimary("Gemini")

    const req = await b.request.ExtractResume2(JOHN_DOE_TEXT_RESUME, {clientRegistry})
    const res = await model.generateContent(req.body.json() as GenerateContentRequest)
    const parsed = b.parse.ExtractResume2(res.response.text())

    expect(parsed).toEqual(JOHN_DOE_PARSED_RESUME)
  })

  it('modular openai gpt4 manual http request', async () => {
    const req = await b.request.ExtractResume2(JOHN_DOE_TEXT_RESUME)

    const res = await fetch(req.url, {
      method: req.method,
      headers: req.headers as Record<string, string>,
      body: JSON.stringify(req.body.json()) // req.body.raw() or req.body.text() works as well
    })

    const body = await res.json() as any

    const parsed = b.parse.ExtractResume2(body.choices[0].message.content)

    expect(parsed).toEqual(JOHN_DOE_PARSED_RESUME)
  })

  it('modular openai gpt4 streaming', async () => {
    const client = new OpenAI()

    const req = await b.streamRequest.ExtractResume2(JOHN_DOE_TEXT_RESUME)

    const stream = await client.chat.completions.create(
      req.body.json() as ChatCompletionCreateParamsStreaming
    )

    let llmResponse: string[] = []

    for await (const chunk of stream) {
      if (chunk.choices.length > 0 && chunk.choices[0].delta.content) {
        llmResponse.push(chunk.choices[0].delta.content)
      }
    }

    const parsed = b.parseStream.ExtractResume2(llmResponse.join(''))

    expect(parsed).toEqual(JOHN_DOE_PARSED_RESUME)
  })

  it('openai batch api', async () => {
    const client = new OpenAI()

    // Helper function to convert BAML HTTP request to OpenAI batch JSONL format
    const toOpenaiJsonl = (req: BamlHttpRequest): string => {
      const line = JSON.stringify({
        custom_id: req.id,
        method: 'POST',
        url: '/v1/chat/completions',
        body: req.body.json(),
      })
      return `${line}\n`
    }

    // Create requests for both resumes
    const [johnReq, janeReq] = await Promise.all([
      b.request.ExtractResume2(JOHN_DOE_TEXT_RESUME),
      b.request.ExtractResume2(JANE_SMITH_TEXT_RESUME)
    ])

    const jsonl = toOpenaiJsonl(johnReq) + toOpenaiJsonl(janeReq)

    // Create batch input file
    const batchInputFile = await client.files.create({
      file: new File([jsonl], 'batch.jsonl'),
      purpose: 'batch',
    })

    // Create batch
    let batch = await client.batches.create({
      input_file_id: batchInputFile.id,
      endpoint: '/v1/chat/completions',
      completion_window: '24h',
      metadata: {
        description: 'BAML Modular API TypeScript Batch Integ Test'
      },
    })

    let backoff = 1000 // milliseconds
    let attempts = 0
    const maxAttempts = 30

    while (true) {
      batch = await client.batches.retrieve(batch.id)
      attempts += 1

      if (batch.status === 'completed') {
        break
      }

      if (attempts >= maxAttempts) {
        try {
          await client.batches.cancel(batch.id)
        } finally {
          throw 'Batch failed to complete in time'
        }
      }

      await new Promise(resolve => setTimeout(resolve, backoff))
      // backoff *= 2 // Exponential backoff
    }

    // Get output file
    const output = await client.files.content(batch.output_file_id!)

    // Process results
    const expected: Record<string, Resume> = {
      [johnReq.id]: JOHN_DOE_PARSED_RESUME,
      [janeReq.id]: JANE_SMITH_PARSED_RESUME,
    }

    const received: Record<string, Resume> = {}
    const outputJsonl = await output.text()

    for (const line of outputJsonl.split("\n").filter(line => line.trim().length > 0)) {
      const result = JSON.parse(line.trim())
      const llmResponse = result.response.body.choices[0].message.content

      const parsed = b.parse.ExtractResume2(llmResponse)
      received[result.custom_id] = parsed
    }

    expect(received).toEqual(expected)
  })
})
