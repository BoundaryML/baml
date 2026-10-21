import ReactDOM from 'react-dom/client';
import App from './App';
import './styles.css';

declare global {
  interface Window {
    /** Set by a host page to pick an app other than the playground. */
    __BAML_APP_MODE?: 'playground' | 'agent-activity';
  }
}

/**
 * `baml agent watch` serves this bundle and opens `/agent-activity`. The
 * Agent Activity UI is a separate lazy chunk, so the playground never loads
 * it and it never starts the playground.
 */
function isAgentActivityPage(): boolean {
  return (
    window.__BAML_APP_MODE === 'agent-activity' ||
    window.location.pathname.replace(/\/+$/, '') === '/agent-activity'
  );
}

const root = ReactDOM.createRoot(
  document.getElementById('root') as HTMLElement,
);

if (isAgentActivityPage()) {
  void import('@b/pkg-agent-activity').then(({ AgentActivityApp }) => {
    root.render(<AgentActivityApp />);
  });
} else {
  // NOTE: No <React.StrictMode>. The Monaco workbench (monaco-vscode-api) is a
  // page-level singleton that can only be initialized once — StrictMode's
  // intentional double-invocation of effects in dev tries to start it twice and
  // throws "Cannot register two commands with the same id". StrictMode is a
  // dev-only aid with no production effect, so dropping it is safe here.
  root.render(<App />);
}
