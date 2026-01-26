import { render, waitFor } from '@testing-library/react'
import { act, createRef, forwardRef, useEffect, useImperativeHandle } from 'react'

import { b } from '../../baml_client'
import {
  type HookInput,
  type HookOutput,
  useAaaSamOutputFormat,
} from '../../baml_client/react/hooks'
import { createFakeRuntimeStream } from '../utils/fake-runtime-stream'

type StreamingHookOutput = HookOutput<'AaaSamOutputFormat', { stream: true }>

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
