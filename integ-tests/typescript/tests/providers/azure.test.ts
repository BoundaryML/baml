import { b } from '../test-setup'

describe('Azure Provider', () => {
  it('should support azure with default max_tokens', async () => {
    const res = await b.TestAzure('Donkey Kong')
    expect(res.toLowerCase()).toContain('donkey')
  })

  it('should support o1 model without max_tokens', async () => {
    const res = await b.TestAzureO1NoMaxTokens('Donkey Kong')
    expect(res.toLowerCase()).toContain('donkey')
  })

  it('should fail when setting max_tokens for o1 model', async () => {
    await expect(async () => {
      await b.TestAzureO1WithMaxTokens('Donkey Kong')
    }).rejects.toThrow(/max_tokens.*not supported/)
  })

  it('should support non-o1 model with explicit max_tokens', async () => {
    const res = await b.TestAzureWithMaxTokens('Donkey Kong')
    expect(res.toLowerCase()).toContain('donkey')
  })

  it('should support o1 model with explicit max_completion_tokens', async () => {
    const res = await b.TestAzureO1WithMaxCompletionTokens('Donkey Kong')
    expect(res.toLowerCase()).toContain('donkey')
  })

  it('should fail if azure is not configured correctly', async () => {
    await expect(async () => {
      await b.TestAzureFailure('Donkey Kong')
    }).rejects.toThrow('BamlClientError')
  })
})
