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
