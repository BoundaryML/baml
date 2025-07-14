// import { AppStateProvider } from './shared/AppStateContext'
import '@baml/ui/globals.css';
import React from 'react';
import { createRoot } from 'react-dom/client';
import App from './App';

// Create a root.
const container = document.getElementById('root');
if (!container) {
  throw new Error('No container found');
}
const root = createRoot(container);

// Listen for VSCode theme variable updates from parent
window.addEventListener('message', (event) => {
  if (event.data && event.data.type === 'vscode-theme' && event.data.vars) {
    for (const [key, value] of Object.entries(event.data.vars)) {
      document.documentElement.style.setProperty(
        key as string,
        value as string,
      );
    }
  }
});

// Initial render: Render your app inside the AppStateProvider.
root.render(
  <React.StrictMode>
    {/* <AppStateProvider> */}
    <App />
    {/* </AppStateProvider> */}
  </React.StrictMode>,
);
