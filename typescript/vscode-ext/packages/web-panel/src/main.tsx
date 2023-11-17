import React from 'react'
import ReactDOM from 'react-dom'
import App from './App'

try {
  ReactDOM.render(
    <React.StrictMode>
      <App />
    </React.StrictMode>,
    document.getElementById('root'),
  )
} catch (error) {
  console.error('REACT error:' + JSON.stringify(error, null, 2))
}
