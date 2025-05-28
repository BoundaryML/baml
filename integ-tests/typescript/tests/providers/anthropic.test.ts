import { b } from '../test-setup'

describe('Anthropic Provider', () => {
  it('should support anthropic shorthand', async () => {
    const res = await b.TestAnthropicShorthand('Dr. Pepper')
    expect(res.length).toBeGreaterThan(0)
  })

  describe('Streaming', () => {
    it('should support anthropic shorthand streaming', async () => {
      const res = await b.stream.TestAnthropicShorthand('Dr. Pepper').getFinalResponse()
      expect(res.length).toBeGreaterThan(0)
    })

    it('should support streaming in Claude', async () => {
      const stream = b.stream.PromptTestClaude('Mt Rainier is tall')
      const msgs: string[] = []
      for await (const msg of stream) {
        msgs.push(msg ?? '')
      }
      const final = await stream.getFinalResponse()

      expect(final.length).toBeGreaterThan(0)
      expect(msgs.length).toBeGreaterThan(0)
      for (let i = 0; i < msgs.length - 2; i++) {
        expect(msgs[i + 1].startsWith(msgs[i])).toBeTruthy()
      }
      expect(msgs.at(-1)).toEqual(final)
    })
  })
})
