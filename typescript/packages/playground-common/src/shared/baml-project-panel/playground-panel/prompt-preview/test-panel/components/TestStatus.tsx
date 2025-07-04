import { AlertCircle, CheckCircle, XCircle } from 'lucide-react'
import { TestState } from '../../../atoms'
import { FinalTestStatus } from '../testStateUtils'
import { Loader } from '../../components'

interface TestStatusProps {
  status: TestState['status']
  finalState?: FinalTestStatus
}

export const TestStatus = ({ status, finalState }: TestStatusProps) => {
  const getStatusColor = (status: TestState['status'], finalState?: FinalTestStatus) => {
    if (status === 'running') return 'text-blue-500'
    if (status === 'done') {
      if (!finalState) return 'text-gray-500'
      return finalState === 'passed'
        ? 'text-green-500'
        : finalState === 'constraints_failed'
          ? 'text-yellow-600'
          : 'text-red-500'
    }
    if (status === 'error') return 'text-red-500'
    return 'text-gray-500'
  }

  const getStatusText = () => {
    if (status === 'running') return 'Running'
    if (status === 'done' && finalState) {
      switch (finalState) {
        case 'passed':
          return 'Passed'
        case 'llm_failed':
          return 'LLM Failed'
        case 'parse_failed':
          return 'Parse Failed'
        case 'constraints_failed':
          return 'Check Failed'
        case 'assert_failed':
          return 'Assert Failed'
        case 'error':
          return 'Error'
      }
    }
    return status
  }

  const getStatusIcon = () => {
    if (status === 'running') return <Loader />
    if (status === 'done') {
      if (finalState === 'passed') return <CheckCircle className='w-4 h-4' />
      if (finalState) return <XCircle className='w-4 h-4' />
    }
    if (status === 'error') return <AlertCircle className='w-4 h-4' />
    return null
  }

  const color = getStatusColor(status, finalState)

  return (
    <div className={`flex items-center gap-1.5 ${color}`}>
      {getStatusIcon()}
      <span className='text-xs md:whitespace-nowrap'>{getStatusText()}</span>
    </div>
  )
}
