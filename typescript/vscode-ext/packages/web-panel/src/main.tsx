import React from 'react'
import { createRoot } from 'react-dom/client'
import App from './App'
import { AppStateProvider } from './shared/AppStateContext'
import './App.css'

// Create a root.
const container = document.getElementById('root')
const root = createRoot(container!) // TypeScript non-null assertion

// Initial render: Render your app inside the AppStateProvider.
root.render(
  <React.StrictMode>
    <AppStateProvider>
      <App />
    </AppStateProvider>
  </React.StrictMode>,
)
