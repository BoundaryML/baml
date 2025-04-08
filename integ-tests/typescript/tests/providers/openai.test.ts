import { b } from '../test-setup'

describe('OpenAI Provider', () => {
  it('should support OpenAI shorthand', async () => {
    const res = await b.TestOpenAIShorthand('Dr. Pepper')
    expect(res.length).toBeGreaterThan(0)
  })

  it('should support openai with default max_tokens', async () => {
    const res = await b.TestOpenAI('Donkey Kong')
    expect(res.toLowerCase()).toContain('donkey')
  })

  it('should support o1 model without max_tokens', async () => {
    const res = await b.TestOpenAIO1NoMaxTokens('Donkey Kong')
    expect(res.toLowerCase()).toContain('donkey')
  })

  it('should fail when setting max_tokens for o1 model', async () => {
    await expect(async () => {
      await b.TestOpenAIO1WithMaxTokens('Donkey Kong')
    }).rejects.toThrow(/max_tokens.*not supported/)
  })

  it('should support non-o1 model with explicit max_tokens', async () => {
    const res = await b.TestOpenAIWithMaxTokens('Donkey Kong')
    expect(res.toLowerCase()).toContain('donkey')
  })

  it('should support o1 model with explicit max_completion_tokens', async () => {
    const res = await b.TestOpenAIO1WithMaxCompletionTokens('Donkey Kong')
    expect(res.toLowerCase()).toContain('donkey')
  })

  it('should support openai with null max_tokens', async () => {
    const res = await b.TestOpenAIWithNullMaxTokens('Donkey Kong')
    expect(res.toLowerCase()).toContain('donkey')
  })

  describe('Streaming', () => {
    it('should support streaming in OpenAI', async () => {
      const stream = b.stream.PromptTestStreaming('Mt Rainier is tall')
      const msgs: string[] = []
      const startTime = performance.now()

      let firstMsgTime: number | null = null
      let lastMsgTime = startTime

      for await (const msg of stream) {
        msgs.push(msg ?? '')
        if (firstMsgTime === null) {
          firstMsgTime = performance.now()
        }
        lastMsgTime = performance.now()
      }
      const final = await stream.getFinalResponse()

      expect(final.length).toBeGreaterThan(0)
      expect(msgs.length).toBeGreaterThan(0)
      expect(firstMsgTime).not.toBeNull()
      expect(firstMsgTime! - startTime).toBeLessThanOrEqual(1500)
      expect(lastMsgTime - startTime).toBeGreaterThan(1000)

      for (let i = 0; i < msgs.length - 2; i++) {
        expect(msgs[i + 1].startsWith(msgs[i])).toBeTruthy()
      }
      expect(msgs.at(-1)).toEqual(final)
    })

    it('should support streaming without iterating', async () => {
      const final = await b.stream.PromptTestStreaming('Mt Rainier is tall').getFinalResponse()
      expect(final.length).toBeGreaterThan(0)
    })
  })
})
