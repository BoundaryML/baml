import { SplitPreview } from '@b/pkg-playground';
import { DevTools } from 'jotai-devtools';

if (import.meta.env.DEV) {
  void import('jotai-devtools/styles.css');
}

const App: React.FC = () => (
  <main className="app">
    <header className="app__header">
      <h1>Standalone Playground</h1>
      <p>Shared UI components and state management come from the common package.</p>
    </header>
    <SplitPreview />
    {import.meta.env.DEV ? <DevTools /> : null}
  </main>
);

export default App;
