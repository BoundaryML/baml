import { useState, useEffect } from 'react';
import { ExecutionPanel, WebSocketRuntimePort } from '@b/pkg-playground';

declare global {
  interface Window {
    /** Injected by the VS Code extension's webview HTML wrapper. */
    __PLAYGROUND_WS_URL?: string;
  }
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
      <main className="w-screen h-screen overflow-hidden flex flex-col items-center justify-center text-sm text-gray-400 gap-2">
        <p>Connecting to playground server...</p>
      </main>
    );
  }

  return <ExecutionPanel port={port} />;
};

export default App;
