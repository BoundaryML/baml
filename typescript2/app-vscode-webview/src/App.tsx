import { useState, useEffect } from 'react';
import { ExecutionPanel, WebSocketRuntimePort, type SourceNavigationTarget } from '@b/pkg-playground';

declare global {
  interface Window {
    /** Injected by the VS Code extension's webview HTML wrapper. */
    __PLAYGROUND_WS_URL?: string;
    acquireVsCodeApi?: () => { postMessage: (message: unknown) => void };
  }
}

let vscodeApi: { postMessage: (message: unknown) => void } | null | undefined;

function getVsCodeApi() {
  if (vscodeApi !== undefined) return vscodeApi;
  vscodeApi = typeof window.acquireVsCodeApi === 'function'
    ? window.acquireVsCodeApi()
    : null;
  return vscodeApi;
}

const App: React.FC = () => {
  const [port, setPort] = useState<WebSocketRuntimePort | null>(null);

  useEffect(() => {
    // When loaded directly in a VS Code webview (no iframe), the extension
    // injects __PLAYGROUND_WS_URL. Fall back to location-based URL for
    // standalone / iframe / dev scenarios.
    const wsUrl =
      window.__PLAYGROUND_WS_URL ?? `ws://${window.location.host}/api/ws`;
    const runtimePort = new WebSocketRuntimePort(wsUrl);
    setPort(runtimePort);
    return () => runtimePort.dispose();
  }, []);

  if (!port) {
    return (
      <main className="playground-root w-screen h-screen overflow-hidden flex flex-col items-center justify-center text-sm text-muted-foreground gap-2">
        <p>Connecting to playground server...</p>
      </main>
    );
  }

  return (
    <div className="playground-root h-screen w-screen overflow-hidden">
      <ExecutionPanel
        port={port}
        onNavigateToSource={(source: SourceNavigationTarget) => {
          getVsCodeApi()?.postMessage({ type: 'navigateToSource', source });
        }}
      />
    </div>
  );
};

export default App;
