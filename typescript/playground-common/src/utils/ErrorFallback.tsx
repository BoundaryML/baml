import { ErrorBoundary, type FallbackProps } from 'react-error-boundary'
const ErrorFallback: React.FC<FallbackProps> = ({ error, resetErrorBoundary }) => {
  return (
    <div role='alert'>
      <p>
        Something went wrong:<button onClick={resetErrorBoundary}>Try again</button>
      </p>
      <pre className='whitespace-pre-wrap'>{error.message}</pre>
      <pre className='whitespace-pre-wrap'>{error.stack}</pre>
      <pre className='whitespace-pre-wrap'>{JSON.stringify(error, null, 2)}</pre>
    </div>
  )
}

interface MyErrorBoundaryProps {
  children: React.ReactNode
}

const CustomErrorBoundary: React.FC<MyErrorBoundaryProps> = ({ children }) => {
  return (
    <ErrorBoundary
      FallbackComponent={ErrorFallback}
      onReset={() => {
        // Reset the state of your app so the error doesn't happen again
      }}
    >
      {children}
    </ErrorBoundary>
  )
}

export default CustomErrorBoundary
