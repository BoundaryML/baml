import { b, DO_NOT_USE_DIRECTLY_UNLESS_YOU_KNOW_WHAT_YOURE_DOING_RUNTIME } from './test-setup'
import { traceAsync, traceSync, setTags, flush, onLogEvent } from '../baml_client'
import { scheduler } from 'node:timers/promises'

describe('Tracing Tests', () => {
  describe('Sync Tracing', () => {
    it('supports tracing sync', async () => {
      const dummyFunc = (_myArg: string): string => 'hello world'

      const res = traceSync('myFuncParent', (firstArg: string, secondArg: number) => {
        setTags({ myKey: 'myVal' })

        const res2 = traceSync('dummyFunc', dummyFunc)('dummyFunc')
        const res3 = traceSync('dummyFunc2', dummyFunc)(firstArg)

        return 'hello world'
      })('myFuncParent', 10)

      traceSync('dummyFunc3', dummyFunc)('hi there')
    })
  })

  // You may want to run this one serially
  describe('Async Tracing', () => {
    it('supports tracing async', async () => {
      const nestedDummyFn = async (myArg: string): Promise<string> => {
        await scheduler.wait(100)
        return myArg
      }

      const dummyFn = async (myArg: string): Promise<string> => {
        await scheduler.wait(100)
        const nested = await Promise.all([
          traceAsync('trace:nestedDummyFn1', nestedDummyFn)('nested1'),
          traceAsync('trace:nestedDummyFn2', nestedDummyFn)('nested2'),
          traceAsync('trace:nestedDummyFn3', nestedDummyFn)('nested3'),
        ])
        return myArg
      }

      DO_NOT_USE_DIRECTLY_UNLESS_YOU_KNOW_WHAT_YOURE_DOING_RUNTIME.flush()
      const _ = DO_NOT_USE_DIRECTLY_UNLESS_YOU_KNOW_WHAT_YOURE_DOING_RUNTIME.drainStats()

      await Promise.all([
        traceAsync('trace:dummyFn1', dummyFn)('hi1'),
        traceAsync('trace:dummyFn2', dummyFn)('hi2'),
        traceAsync('trace:dummyFn3', dummyFn)('hi3'),
      ])

      const res = await traceAsync('parentAsync', async (firstArg: string, secondArg: number) => {
        setTags({ myKey: 'myVal' })

        const res1 = traceSync('dummyFunc', dummyFn)('firstDummyFuncArg')
        const res2 = await traceAsync('asyncDummyFunc', dummyFn)('secondDummyFuncArg')

        const llm_res = await Promise.all([
          b.TestFnNamedArgsSingleStringList(['a1', 'b', 'c']),
          b.TestFnNamedArgsSingleStringList(['a2', 'b', 'c']),
          b.TestFnNamedArgsSingleStringList(['a3', 'b', 'c']),
        ])

        const res3 = await traceAsync('asyncDummyFunc', dummyFn)('thirdDummyFuncArg')

        return 'hello world'
      })('hi', 10)

      DO_NOT_USE_DIRECTLY_UNLESS_YOU_KNOW_WHAT_YOURE_DOING_RUNTIME.flush()
      const stats = DO_NOT_USE_DIRECTLY_UNLESS_YOU_KNOW_WHAT_YOURE_DOING_RUNTIME.drainStats()
      expect(stats.started).toBe(28)
      expect(stats.finalized).toEqual(stats.started)
      expect(stats.submitted).toEqual(stats.started)
      expect(stats.sent).toEqual(stats.started)
      expect(stats.done).toEqual(stats.started)
      expect(stats.failed).toEqual(0)
    })
  })

  describe('Events', () => {
    it("should work with 'onLogEvent'", async () => {
      flush() // Wait for all logs to be sent so no calls to onLogEvent are missed.
      onLogEvent((param2) => {
        console.log('onLogEvent', param2)
      })
      const res = await b.TestFnNamedArgsSingleStringList(['a', 'b', 'c'])
      expect(res).toContain('a')
      const res2 = await b.TestFnNamedArgsSingleStringList(['d', 'e', 'f'])
      expect(res2).toContain('d')
      flush() // Wait for all logs to be sent so no calls to onLogEvent are missed.
      onLogEvent(undefined)
    })
  })
})
