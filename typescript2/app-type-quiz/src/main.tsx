import { StrictMode } from 'react';
import { createRoot } from 'react-dom/client';
import App from './app';
import 'highlight.js/styles/github-dark.css';
import './styles.css';

const root = document.getElementById('root');
if (root === null) {
  throw new Error('index.html has no #root to mount into');
}

createRoot(root).render(
  <StrictMode>
    <App />
  </StrictMode>,
);

// The fallback in `index.html` reports anything thrown before this line, and
// stands down once the app is up and reporting for itself.
declare global {
  interface Window {
    __quizMounted?: boolean;
  }
}
window.__quizMounted = true;
