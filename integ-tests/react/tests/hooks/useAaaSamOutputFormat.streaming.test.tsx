import { render, waitFor } from '@testing-library/react'
import { act, createRef, forwardRef, useEffect, useImperativeHandle } from 'react'

import { b } from '../../baml_client'
import {
  type HookData,
  type HookInput,
  type HookOutput,
  useAaaSamOutputFormat,
} from '../../baml_client/react/hooks'
import { createFakeRuntimeStream } from '../utils/fake-runtime-stream'

type StreamingHookOutput = HookOutput<'AaaSamOutputFormat', { stream: true }>
type StreamingSemanticData = HookData<'MakeSemanticContainer', { stream: true }>
type NonStreamingSemanticData = HookData<'MakeSemanticContainer', { stream: false }>

const assertStreamingSemanticDataTypes = (data: StreamingSemanticData) => {
  const streamStateValue: string | null = data.class_needed.s_20_words.value
  const streamStateStatus: 'Pending' | 'Incomplete' | 'Complete' = data.class_needed.s_20_words.state
  const maybeDigit: number | null | undefined = data.class_needed.i_16_digits
  const maybeFinalString: string | null | undefined = data.final_string
  const notNullDoneClass: number = data.class_done_needed.i_16_digits

  void streamStateValue
  void streamStateStatus
  void maybeDigit
  void maybeFinalString
  void notNullDoneClass

  // @ts-expect-error streaming data can omit regular fields until they are complete
  const finalString: string = data.final_string
  void finalString

  // @ts-expect-error non-@stream.not_null nested fields still need null handling
  const digit: number = data.class_needed.i_16_digits
  void digit
}

const assertNonStreamingSemanticDataTypes = (data: NonStreamingSemanticData) => {
  const finalString: string = data.final_string
  const finalNestedString: string = data.class_needed.s_20_words
  const finalDigit: number = data.class_needed.i_16_digits

  void finalString
  void finalNestedString
  void finalDigit
}

const streamingSemanticOptions: HookInput<'MakeSemanticContainer', { stream: true }> = {
  stream: true,
  onData: response => {
    if (!response) return

    const streamStateStatus: 'Pending' | 'Incomplete' | 'Complete' = response.class_needed.s_20_words.state
    void streamStateStatus

    // @ts-expect-error streaming onData receives the partial shape, not final data
    const finalNestedString: string = response.class_needed.s_20_words
    void finalNestedString
  },
}

const nonStreamingSemanticOptions: HookInput<'MakeSemanticContainer', { stream: false }> = {
  stream: false,
  onData: response => {
    if (!response) return

    const finalNestedString: string = response.class_needed.s_20_words
    void finalNestedString
  },
}

void assertStreamingSemanticDataTypes
void assertNonStreamingSemanticDataTypes
void streamingSemanticOptions
void nonStreamingSemanticOptions

type HookHarnessProps = {
  options: HookInput<'AaaSamOutputFormat', { stream: true }>
  onStateChange: (state: StreamingHookOutput) => void
}

type HookHarnessHandle = {
  mutate: StreamingHookOutput['mutate']
}

const HookHarness = forwardRef<HookHarnessHandle, HookHarnessProps>(({ options, onStateChange }, ref) => {
  const streamingState = useAaaSamOutputFormat(options) as StreamingHookOutput

  useEffect(() => {
    onStateChange(streamingState)
  }, [onStateChange, streamingState])

  useImperativeHandle(
    ref,
    () => ({ mutate: streamingState.mutate }),
    [streamingState.mutate],
  )

  return null
})

describe('useAaaSamOutputFormat streaming hook', () => {
  it('transitions through pending, streaming, and success states', async () => {
    const runtime = (b as any).runtime
    const originalStreamFunction = runtime.streamFunction

    const partialRecipe = {
      ingredients: {
        Flour: {
          amount: 1,
        },
      },
    }

    const finalRecipe = {
      ingredients: {
        Flour: {
          amount: 1,
          unit: 'cup',
        },
        Eggs: {
          amount: 2,
        },
      },
      recipe_type: 'dinner' as const,
    }

    // Use multiple partials with delays to ensure streaming state is observable
    // FakeRuntimeStream uses delay = delayMs * index, so we need multiple partials
    const partialRecipe2 = {
      ingredients: {
        Flour: {
          amount: 1,
          unit: 'cup',
        },
      },
    }

    runtime.streamFunction = jest.fn((functionName: string) => {
      expect(functionName).toBe('AaaSamOutputFormat')
      // Multiple partials ensure there's time to observe streaming state
      // Delays: partial1 at 0ms, partial2 at 100ms, then final
      return createFakeRuntimeStream([partialRecipe, partialRecipe2], finalRecipe, 100)
    })

    const onStreamData = jest.fn()
    const onFinalData = jest.fn()

    const statusHistory: StreamingHookOutput['status'][] = []
    let latestState: StreamingHookOutput | undefined
    const harnessRef = createRef<HookHarnessHandle>()

    try {
      render(
        <HookHarness
          ref={harnessRef}
          options={{
            stream: true,
            onStreamData,
            onFinalData,
          }}
          onStateChange={state => {
            latestState = state
            statusHistory.push(state.status)
          }}
        />,
      )

      await waitFor(() => {
        expect(statusHistory.at(-1)).toBe('idle')
      })

      await act(async () => {
        const mutatePromise = harnessRef.current?.mutate('recipe input')
        await Promise.resolve(mutatePromise)
      })

      await waitFor(() => {
        expect(statusHistory).toEqual(expect.arrayContaining(['pending']))
      })

      // Wait for and verify streaming state
      await waitFor(() => {
        expect(latestState?.status).toBe('streaming')
      })
      expect(latestState?.streamData).toBeDefined()
      expect(latestState?.isLoading).toBe(true)

      await waitFor(() => {
        expect(latestState?.status).toBe('success')
      })

      // Verify streaming callbacks were called correctly
      // This is the core functionality - even if status transitions are fast
      expect(onStreamData).toHaveBeenCalledTimes(2) // Two partials
      expect(onStreamData).toHaveBeenCalledWith(partialRecipe)
      expect(onStreamData).toHaveBeenCalledWith(partialRecipe2)
      expect(onFinalData).toHaveBeenCalledWith(finalRecipe)

      // Verify final state
      expect(latestState?.finalData).toEqual(finalRecipe)
      expect(latestState?.data).toEqual(finalRecipe)
      expect(latestState?.isLoading).toBe(false)
      expect(latestState?.isSuccess).toBe(true)

      // Status should have gone through all these states
      const uniqueStatuses = statusHistory.filter(
        (status, index, arr) => index === 0 || arr[index - 1] !== status,
      )
      expect(uniqueStatuses).toContain('idle')
      expect(uniqueStatuses).toContain('pending')
      expect(uniqueStatuses).toContain('streaming')
      expect(uniqueStatuses).toContain('success')
    } finally {
      runtime.streamFunction = originalStreamFunction
    }
  })
})
