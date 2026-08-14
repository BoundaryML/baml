import ReactDOM from 'react-dom/client';
import App from './App';
import './styles.css';

// NOTE: No <React.StrictMode>. The Monaco workbench (monaco-vscode-api) is a
// page-level singleton that can only be initialized once — StrictMode's
// intentional double-invocation of effects in dev tries to start it twice and
// throws "Cannot register two commands with the same id". StrictMode is a
// dev-only aid with no production effect, so dropping it is safe here.
ReactDOM.createRoot(document.getElementById('root') as HTMLElement).render(
  <App />
);
