import OpenAI from 'openai'
import { ChatCompletionCreateParamsNonStreaming } from 'openai/resources';
import Anthropic from '@anthropic-ai/sdk'
import { MessageCreateParamsNonStreaming } from '@anthropic-ai/sdk/resources';
import { GenerateContentRequest, GoogleGenerativeAI } from '@google/generative-ai';
import { b, ClientRegistry } from './test-setup';

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
      fail(`Unexpected type for content block: ${res.content[0]}`)
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
      headers: req.headers,
      body: JSON.stringify(req.body.json()) // req.body.raw() or req.body.text() works as well
    })

    const body = await res.json() as any

    const parsed = b.parse.ExtractResume2(body.choices[0].message.content)

    expect(parsed).toEqual(JOHN_DOE_PARSED_RESUME)
  })
})
