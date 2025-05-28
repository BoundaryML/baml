import { b, b_sync } from './test-setup'
import { BamlRuntime, Collector, FunctionLog, Usage } from '@boundaryml/baml'

async function gc() {
  global.gc!()
  // allows node to run finalizers
  await new Promise((resolve) => setTimeout(resolve, 0))
}

describe('Collector Tests', () => {
  beforeEach(() => {
    // Ensure collector is empty before each test
    expect(Collector.__functionSpanCount()).toBe(0)
  })

  afterEach(async () => {
    // Ensure garbage collection and verify all spans are cleaned up
    await gc()
    expect(Collector.__functionSpanCount()).toBe(0)
  })

  it('should collect logs for non-streaming calls', async () => {
    console.log('### function_call_count', Collector.__functionSpanCount())
    // Should be garbage collected
    expect(Collector.__functionSpanCount()).toBe(0)

    const collector = new Collector('my-collector')
    const functionLogs = collector.logs
    expect(functionLogs.length).toBe(0)

    await b.TestOpenAIGPT4oMini('hi there', { collector })

    const updatedLogs = collector.logs
    expect(updatedLogs.length).toBe(1)

    const log = collector.last
    console.log('### log', log?.toString())
    expect(log).not.toBeNull()
    expect(log?.functionName).toBe('TestOpenAIGPT4oMini')
    expect(log?.logType).toBe('call')

    // Verify timing fields
    expect(log?.timing.startTimeUtcMs).toBeGreaterThan(0)
    expect(log?.timing.durationMs).toBeGreaterThan(0)

    // Verify usage fields
    expect(log?.usage.inputTokens).toBeGreaterThan(0)
    expect(log?.usage.outputTokens).toBeGreaterThan(0)

    // Verify calls
    const calls = log?.calls || []

    expect(calls.length).toBe(1)

    const call = calls[0]
    expect(call.provider).toBe('openai')
    expect(call.clientName).toBe('GPT4oMini')
    expect(call.selected).toBe(true)

    // Verify request/response
    const request = call.httpRequest
    expect(request).not.toBeNull()

    const body = request?.body.json()

    expect(typeof body).toBe('object')
    expect(body.messages).toBeDefined()
    expect(body.messages[0].content).not.toBeNull()
    expect(body.model).toBe('gpt-4o-mini')

    // Verify http response
    const response = call.httpResponse
    const responseBody = response?.body.json()
    expect(response).not.toBeNull()
    expect(response?.status).toBe(200)
    expect(responseBody).not.toBeNull()
    expect(responseBody?.choices).toBeDefined()
    expect(responseBody?.choices.length).toBeGreaterThan(0)
    expect(responseBody?.choices[0].message.content).not.toBeNull()

    // Verify call timing
    const callTiming = call.timing
    expect(callTiming.startTimeUtcMs).toBeGreaterThan(0)
    expect(callTiming.durationMs).toBeGreaterThan(0)

    // Verify call usage
    const callUsage = call.usage
    expect(callUsage?.inputTokens).toBeGreaterThan(0)
    expect(callUsage?.outputTokens).toBeGreaterThan(0)

    // Usage matches log usage
    expect(callUsage?.inputTokens).toBe(log?.usage.inputTokens)
    expect(callUsage?.outputTokens).toBe(log?.usage.outputTokens)

    // Verify raw response exists
    expect(log?.rawLlmResponse).not.toBeNull()

    // Collector usage should match log usage
    expect(collector.usage.inputTokens).toBe(log?.usage.inputTokens)
    expect(collector.usage.outputTokens).toBe(log?.usage.outputTokens)

    // Verify metadata
    // expect(typeof log?.metadata).toBe('object');

    // Force garbage collection
    await gc()
    console.log('----- gc.collect() -----')
    // Still not collected because it's in use
    expect(Collector.__functionSpanCount()).toBeGreaterThan(0)
  })

  it('should handle streaming calls correctly', async () => {
    const collector = new Collector('my-collector')
    const functionLogs = collector.logs
    expect(functionLogs.length).toBe(0)

    const stream = b.stream.TestOpenAIGPT4oMini('hi there', { collector })

    const chunks = []
    for await (const chunk of stream) {
      chunks.push(chunk)
      console.log(`### chunk: ${chunk}`)
    }

    const res = await stream.getFinalResponse()
    console.log(`### res: ${res}`)

    const updatedLogs = collector.logs
    expect(updatedLogs.length).toBe(1)

    const log = collector.last
    expect(log).not.toBeNull()
    expect(log?.functionName).toBe('TestOpenAIGPT4oMini')
    expect(log?.logType).toBe('call')

    // Verify timing fields
    expect(log?.timing.startTimeUtcMs).toBeGreaterThan(0)
    expect(log?.timing.durationMs).toBeGreaterThan(0)

    // Verify usage fields
    expect(log?.usage.inputTokens).toBeGreaterThan(0)
    expect(log?.usage.outputTokens).toBeGreaterThan(0)

    // Verify calls
    const calls = log?.calls || []
    expect(calls.length).toBe(1)

    const call = calls[0]
    expect(call.provider).toBe('openai')
    expect(call.clientName).toBe('GPT4oMini')
    expect(call.selected).toBe(true)

    // Verify request
    const request = call.httpRequest
    expect(request).not.toBeNull()
    expect(typeof request?.body).toBe('object')
    expect((request?.body.json()).messages).toBeDefined()

    // For streaming, httpResponse might be null since it's streaming
    const response = call.httpResponse
    expect(response).toBeNull()

    // Verify call timing
    const callTiming = call.timing
    expect(callTiming.startTimeUtcMs).toBeGreaterThan(0)
    expect(callTiming.durationMs).toBeGreaterThan(0)

    // Verify call usage
    const callUsage = call.usage
    expect(callUsage?.inputTokens).toBeGreaterThan(0)
    expect(callUsage?.outputTokens).toBeGreaterThan(0)

    // Verify raw response exists
    expect(log?.rawLlmResponse).not.toBeNull()

    await gc()
    console.log('----- gc.collect() -----')
    // Still not collected because it's in use
    expect(Collector.__functionSpanCount()).toBeGreaterThan(0)
  })

  it('should track cumulative usage across multiple calls', async () => {
    const collector = new Collector('my-collector')

    // First call
    await b.TestOpenAIGPT4oMini('First call', { collector })
    const functionLogs = collector.logs
    expect(functionLogs.length).toBe(1)

    // Capture usage after first call
    const firstCallUsage = functionLogs[0].usage
    expect(collector.usage.inputTokens).toBe(firstCallUsage.inputTokens)
    expect(collector.usage.outputTokens).toBe(firstCallUsage.outputTokens)

    // Second call
    await b.TestOpenAIGPT4oMini('Second call', { collector })
    const updatedLogs = collector.logs
    expect(updatedLogs.length).toBe(2)

    // Capture usage after second call and verify it's the sum of both calls
    const secondCallUsage = updatedLogs[1].usage
    const totalInput = (firstCallUsage?.inputTokens ?? 0) + (secondCallUsage?.inputTokens ?? 0)
    const totalOutput = (firstCallUsage?.outputTokens ?? 0) + (secondCallUsage?.outputTokens ?? 0)
    expect(collector.usage.inputTokens).toBe(totalInput)
    expect(collector.usage.outputTokens).toBe(totalOutput)
  })

  it('should support multiple collectors', async () => {
    const coll1 = new Collector('collector-1')
    const coll2 = new Collector('collector-2')

    // Pass in both collectors for the first call
    await b.TestOpenAIGPT4oMini('First call', { collector: [coll1, coll2] })

    // Check usage/logs after the first call
    const logs1 = coll1.logs
    const logs2 = coll2.logs
    expect(logs1.length).toBe(1)
    expect(logs2.length).toBe(1)

    const usageFirstCallColl1 = logs1[0].usage
    const usageFirstCallColl2 = logs2[0].usage

    // Verify both collectors have the exact same usage for the first call
    expect(usageFirstCallColl1.inputTokens).toBe(usageFirstCallColl2.inputTokens)
    expect(usageFirstCallColl1.outputTokens).toBe(usageFirstCallColl2.outputTokens)

    // Also check that the collector-level usage matches the single call usage for each collector
    expect(coll1.usage.inputTokens).toBe(usageFirstCallColl1.inputTokens)
    expect(coll1.usage.outputTokens).toBe(usageFirstCallColl1.outputTokens)
    expect(coll2.usage.inputTokens).toBe(usageFirstCallColl2.inputTokens)
    expect(coll2.usage.outputTokens).toBe(usageFirstCallColl2.outputTokens)

    // Second call uses only coll1
    await b.TestOpenAIGPT4oMini('Second call', { collector: coll1 })

    // Re-check logs/usage
    const updatedLogs1 = coll1.logs
    const updatedLogs2 = coll2.logs
    expect(updatedLogs1.length).toBe(2)
    expect(updatedLogs2.length).toBe(1)

    // Verify coll1 usage is now the sum of both calls
    const usageSecondCallColl1 = updatedLogs1[1].usage
    const totalInput = (usageFirstCallColl1?.inputTokens ?? 0) + (usageSecondCallColl1?.inputTokens ?? 0)
    const totalOutput = (usageFirstCallColl1?.outputTokens ?? 0) + (usageSecondCallColl1?.outputTokens ?? 0)
    expect(coll1.usage.inputTokens).toBe(totalInput)
    expect(coll1.usage.outputTokens).toBe(totalOutput)

    // Verify coll2 usage remains unchanged (it did not participate in the second call)
    expect(coll2.usage.inputTokens).toBe(usageFirstCallColl2.inputTokens)
    expect(coll2.usage.outputTokens).toBe(usageFirstCallColl2.outputTokens)
  })

  it('should handle parallel async calls correctly', async () => {
    const collector = new Collector('parallel-collector')

    // Execute two calls in parallel
    await Promise.all([
      b.TestOpenAIGPT4oMini('call #1', { collector }),
      b.TestOpenAIGPT4oMini('call #2', { collector }),
    ])
    console.log('------------------------- ended parallel calls')

    // Verify the collector has two function logs
    const logs = collector.logs
    expect(logs.length).toBe(2)

    // Ensure each call is recorded properly
    console.log('------------------------- logs iteration', logs)
    for (const log of logs) {
      expect(log.functionName).toBe('TestOpenAIGPT4oMini')
      expect(log.logType).toBe('call')
    }

    // Check usage for each call
    const usageCall1 = logs[0].usage
    const usageCall2 = logs[1].usage
    expect(usageCall1).not.toBeNull()
    expect(usageCall2).not.toBeNull()

    // Verify that total collector usage equals the sum of the two logs
    const totalInput = (usageCall1?.inputTokens ?? 0) + (usageCall2?.inputTokens ?? 0)
    const totalOutput = (usageCall1?.outputTokens ?? 0) + (usageCall2?.outputTokens ?? 0)
    expect(collector.usage.inputTokens).toBe(totalInput)
    expect(collector.usage.outputTokens).toBe(totalOutput)
  })

  it('should handle sync calls correctly', async () => {
    const collector = new Collector('sync-collector')
    const result = b_sync.TestOpenAIGPT4oMini('sync call', { collector })

    const logs = collector.logs
    expect(logs.length).toBe(1)
    expect(logs[0].functionName).toBe('TestOpenAIGPT4oMini')
    expect(logs[0].logType).toBe('call')
    expect(logs[0].usage).not.toBeNull()
  })
})
