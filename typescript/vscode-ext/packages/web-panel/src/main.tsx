import React from 'react'
import ReactDOM from 'react-dom'
import App from './App'
import { ErrorBoundary } from 'react-error-boundary'
import CustomErrorBoundary from './utils/ErrorFallback'

try {
  ReactDOM.render(
    <React.StrictMode>
      <CustomErrorBoundary>
        <App />
      </CustomErrorBoundary>
    </React.StrictMode>,
    document.getElementById('root'),
  )
} catch (error) {
  console.error(error)
  console.error('REACT error:' + JSON.stringify(error, null, 2))
}
