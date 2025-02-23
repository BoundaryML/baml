import { Badge } from '@/components/ui/badge'
import { Separator } from '@/components/ui/separator'
import { cn } from '@/lib/utils'
import { Check, X } from 'lucide-react'
import * as React from 'react'
import type { HookOutput } from '../../../baml_client/react/hooks'

type NetworkState = 'idle' | 'loading' | 'pending' | 'streaming' | 'success' | 'error'

type StateEntry = {
  startTime: number
  endTime?: number
}

type TimelineState = {
  [K in NetworkState]?: StateEntry
}

type TimelineAction =
  | { type: 'STATE_START'; state: NetworkState; timestamp: number }
  | { type: 'STATE_END'; state: NetworkState; timestamp: number }
  | { type: 'RESET' }

function timelineReducer(state: TimelineState, action: TimelineAction): TimelineState {
  switch (action.type) {
    case 'STATE_START': {
      // Only start if not already started
      if (state[action.state]?.startTime && !state[action.state]?.endTime) {
        return state
      }
      return {
        ...state,
        [action.state]: { startTime: action.timestamp },
      }
    }
    case 'STATE_END': {
      const currentEntry = state[action.state]
      if (!currentEntry || currentEntry.endTime) {
        return state
      }
      return {
        ...state,
        [action.state]: { ...currentEntry, endTime: action.timestamp },
      }
    }
    case 'RESET': {
      return {}
    }
    default:
      return state
  }
}

// Individual timeline row component
function TimelineRow({
  label,
  state,
  totalDuration,
  timelineStart,
  colorClass,
  isComplete,
  currentTime,
}: {
  label: string
  state: StateEntry | undefined
  totalDuration: number
  timelineStart: number
  colorClass: string
  isComplete: boolean
  currentTime: number
}) {
  if (!state) {
    return (
      <div className='flex items-center gap-2 text-xs'>
        <div className='w-24 text-muted-foreground'>{label}</div>
        <div className='flex-1 h-4 bg-muted/30 rounded-sm relative' />
        <div className='w-16 text-right text-muted-foreground'>...</div>
      </div>
    )
  }

  const endTime = state.endTime || currentTime
  const duration = (Math.round((endTime - state.startTime) / 100) / 10).toFixed(1)

  // Calculate width based on the state's duration
  const stateDuration = endTime - state.startTime
  const width = Math.min((stateDuration / totalDuration) * 100, 100)

  // Calculate right position based on how far we are in the timeline
  const timeFromEnd = currentTime - state.startTime
  const rightPosition = Math.min(100, Math.max(0, 100 - (timeFromEnd / totalDuration) * 100))

  return (
    <div className='flex items-center gap-2 text-xs'>
      <div className='w-24 text-muted-foreground'>{label}</div>
      <div className='flex-1 h-4 bg-muted/30 rounded-sm relative'>
        <div
          className={cn('absolute h-full rounded-sm', colorClass, {
            // 'opacity-40': state.endTime || isComplete,
          })}
          style={{
            left: `${rightPosition}%`,
            width: `${Math.max(width, 0.5)}%`,
            minWidth: '6px',
          }}
        />
      </div>
      <div className='w-16 text-right text-muted-foreground'>
        <span className='font-mono'>{duration}</span>s
      </div>
    </div>
  )
}

// Duration display component
function DurationDisplay({
  timeline,
  isComplete,
  currentTime,
}: {
  timeline: TimelineState
  isComplete: boolean
  currentTime: number
}) {
  const startTime = Object.values(timeline).reduce(
    (earliest, entry) => (entry?.startTime && (!earliest || entry.startTime < earliest) ? entry.startTime : earliest),
    currentTime,
  )

  if (!startTime) return null

  const totalDuration =
    startTime === currentTime ? '...' : (Math.round((currentTime - startTime) / 100) / 10).toFixed(1)
  const timeToFirstToken = timeline.error?.startTime
    ? '...'
    : timeline.streaming?.startTime
      ? (Math.round((timeline.streaming.startTime - startTime) / 100) / 10).toFixed(1)
      : timeline.success?.startTime
        ? (Math.round((timeline.success.startTime - startTime) / 100) / 10).toFixed(1)
        : startTime === currentTime
          ? '...'
          : totalDuration

  return (
    <div className='flex flex-col items-end text-xs text-muted-foreground'>
      <span>
        Total Duration: {totalDuration === '...' ? '...' : <span className='font-mono'>{totalDuration}</span>}
        {totalDuration !== '...' && 's'}
      </span>
      <span>
        Time to First Parse:{' '}
        {timeToFirstToken === '...' ? '...' : <span className='font-mono'>{timeToFirstToken}</span>}
        {timeToFirstToken !== '...' && 's'}
      </span>
    </div>
  )
}

function StatusBadge({ status }: { status: HookOutput<'TestAws', { stream: true }>['status'] }) {
  const getVariant = () => {
    if (status === 'idle') return 'ghost' as const
    return 'default' as const
  }

  return (
    <div className='w-full flex items-center justify-center text-center'>
      <Badge
        variant={getVariant()}
        className={cn('w-full text-center justify-center', {
          'bg-muted text-muted-foreground': status === 'idle',
          // 'bg-blue-500 hover:bg-blue-500/80 text-white': status === 'loading',
          'bg-yellow-500 hover:bg-yellow-500/80 text-white': status === 'pending',
          'bg-purple-500 hover:bg-purple-500/80 text-white': status === 'streaming',
          'bg-green-500 hover:bg-green-500/80 text-white': status === 'success',
          'bg-red-500 hover:bg-red-500/80 text-white': status === 'error',
        })}
      >
        {status.charAt(0).toUpperCase() + status.slice(1)}
      </Badge>
    </div>
  )
}

function BooleanBadge({ label, value }: { label: string; value: boolean }) {
  const getVariant = () => {
    if (!value) return 'ghost' as const
    return 'default' as const
  }

  return (
    <div className='w-full flex items-center justify-center text-center'>
      <Badge
        variant={getVariant()}
        className={cn('w-full text-center justify-center', {
          'bg-muted text-muted-foreground': !value,
          'bg-red-500 hover:bg-red-500/80 text-white': label === 'IsError' && value,
          'bg-green-500 hover:bg-green-500/80 text-white': label !== 'IsError' && value,
        })}
      >
        {label}
      </Badge>
    </div>
  )
}

export function NetworkTimeline({
  hookResult,
  className,
  hasStarted,
}: {
  hookResult: HookOutput<'TestAws'>
  className?: string
  hasStarted: boolean
}) {
  const [timeline, dispatch] = React.useReducer(timelineReducer, {})
  const [isComplete, setIsComplete] = React.useState(false)
  const hasStartedRef = React.useRef(false)
  const [currentTime, setCurrentTime] = React.useState(Date.now())
  const animationFrameRef = React.useRef<number | null>(null)

  // Use requestAnimationFrame for smoother updates
  React.useEffect(() => {
    if (isComplete) {
      if (animationFrameRef.current) {
        cancelAnimationFrame(animationFrameRef.current)
      }
      return
    }

    function updateTime() {
      setCurrentTime(Date.now())
      animationFrameRef.current = requestAnimationFrame(updateTime)
    }

    animationFrameRef.current = requestAnimationFrame(updateTime)

    return () => {
      if (animationFrameRef.current) {
        cancelAnimationFrame(animationFrameRef.current)
      }
    }
  }, [isComplete])

  // Handle initial idle state when hasStarted changes
  React.useEffect(() => {
    if (hasStarted && !hasStartedRef.current) {
      hasStartedRef.current = true
      const timestamp = Date.now()
      dispatch({ type: 'STATE_START', state: 'idle', timestamp })

      // If we're already in a loading/pending state when hasStarted becomes true,
      // we should immediately end the idle state
      if (hookResult.isLoading || hookResult.isPending) {
        dispatch({ type: 'STATE_END', state: 'idle', timestamp })
      }
    } else if (!hasStarted) {
      hasStartedRef.current = false
    }
  }, [hasStarted, hookResult.isLoading, hookResult.isPending])

  // Track state changes
  React.useEffect(() => {
    const timestamp = Date.now()

    // Check if the request is complete
    if (hookResult.isSuccess || hookResult.isError) {
      setIsComplete(true)
    }

    // Handle loading state
    if (hookResult.isLoading) {
      dispatch({ type: 'STATE_START', state: 'loading', timestamp })
    } else if (timeline.loading && !timeline.loading.endTime && !hookResult.isPending && !hookResult.isStreaming) {
      dispatch({ type: 'STATE_END', state: 'loading', timestamp })
    }

    // Handle pending state
    if (hookResult.isPending) {
      dispatch({ type: 'STATE_START', state: 'pending', timestamp })
    } else if (timeline.pending && !timeline.pending.endTime && !hookResult.isStreaming) {
      dispatch({ type: 'STATE_END', state: 'pending', timestamp })
    }

    // Handle streaming state
    if (hookResult.isStreaming) {
      // End pending state if it exists
      if (timeline.pending && !timeline.pending.endTime) {
        dispatch({ type: 'STATE_END', state: 'pending', timestamp })
      }
      dispatch({ type: 'STATE_START', state: 'streaming', timestamp })
    } else if (timeline.streaming && !timeline.streaming.endTime && !hookResult.isSuccess) {
      dispatch({ type: 'STATE_END', state: 'streaming', timestamp })
    }

    if (hookResult.isSuccess) {
      dispatch({ type: 'STATE_START', state: 'success', timestamp })
      // End all other states when success occurs
      Object.keys(timeline).forEach((state) => {
        if (state !== 'success' && timeline[state as NetworkState] && !timeline[state as NetworkState]!.endTime) {
          dispatch({ type: 'STATE_END', state: state as NetworkState, timestamp })
        }
      })
    }

    if (hookResult.isError) {
      dispatch({ type: 'STATE_START', state: 'error', timestamp })
      // End all other states when error occurs
      Object.keys(timeline).forEach((state) => {
        if (state !== 'error' && timeline[state as NetworkState] && !timeline[state as NetworkState]!.endTime) {
          dispatch({ type: 'STATE_END', state: state as NetworkState, timestamp })
        }
      })
    }
  }, [hookResult.isLoading, hookResult.isPending, hookResult.isStreaming, hookResult.isSuccess, hookResult.isError])

  // Reset timeline when all states are inactive
  React.useEffect(() => {
    if (
      !hookResult.isLoading &&
      !hookResult.isPending &&
      !hookResult.isStreaming &&
      !hookResult.isSuccess &&
      !hookResult.isError
    ) {
      dispatch({ type: 'RESET' })
      setIsComplete(false)
    }
  }, [hookResult])

  const startTime = Object.values(timeline).reduce(
    (earliest, entry) => (entry?.startTime && (!earliest || entry.startTime < earliest) ? entry.startTime : earliest),
    currentTime,
  )

  const totalDuration = Math.max(currentTime - startTime, 100) // Ensure we always have at least 100ms duration

  return (
    <div className={cn('gap-4 flex flex-col px-4 py-2 bg-card rounded-lg border', className)}>
      <div className='flex items-center justify-between'>
        <div className='flex flex-col items-center gap-1'>
          <h3 className='text-sm font-semibold'>LLM Timeline</h3>
          <StatusBadge status={hookResult.status} />
        </div>
        <DurationDisplay timeline={timeline} isComplete={isComplete} currentTime={currentTime} />
      </div>

      <div className='space-y-2'>
        <TimelineRow
          label='Idle'
          state={timeline.idle}
          totalDuration={totalDuration}
          timelineStart={startTime}
          colorClass='bg-gray-500/80'
          isComplete={isComplete}
          currentTime={currentTime}
        />
        <TimelineRow
          label='Loading'
          state={timeline.loading}
          totalDuration={totalDuration}
          timelineStart={startTime}
          colorClass='bg-blue-500/80'
          isComplete={isComplete}
          currentTime={currentTime}
        />
        <TimelineRow
          label='Pending'
          state={timeline.pending}
          totalDuration={totalDuration}
          timelineStart={startTime}
          colorClass='bg-yellow-500/80'
          isComplete={isComplete}
          currentTime={currentTime}
        />
        <TimelineRow
          label='Streaming'
          state={timeline.streaming}
          totalDuration={totalDuration}
          timelineStart={startTime}
          colorClass='bg-purple-500/80'
          isComplete={isComplete}
          currentTime={currentTime}
        />
        <TimelineRow
          label='Success'
          state={timeline.success}
          totalDuration={totalDuration}
          timelineStart={startTime}
          colorClass='bg-green-500/80'
          isComplete={isComplete}
          currentTime={currentTime}
        />
        <TimelineRow
          label='Error'
          state={timeline.error}
          totalDuration={totalDuration}
          timelineStart={startTime}
          colorClass='bg-red-500/80'
          isComplete={isComplete}
          currentTime={currentTime}
        />
      </div>
      <Separator />
      <div className='grid grid-cols-5 gap-2 text-xs text-muted-foreground'>
        <BooleanBadge label='IsLoading' value={hookResult.isLoading} />
        <BooleanBadge label='IsPending' value={hookResult.isPending} />
        <BooleanBadge label='IsStreaming' value={hookResult.isStreaming} />
        <BooleanBadge label='IsSuccess' value={hookResult.isSuccess} />
        <BooleanBadge label='IsError' value={hookResult.isError} />
      </div>
    </div>
  )
}
