import { useState, useEffect, useRef } from 'react';
import { ExecutionPanel, WebSocketRuntimePort, type SourceNavigationTarget } from '@b/pkg-playground';

declare global {
  interface Window {
    /** Injected by the VS Code extension's webview HTML wrapper. */
    __PLAYGROUND_WS_URL?: string;
    /** Cursor position forwarded by the VS Code extension host. */
    __PLAYGROUND_CURSOR_POSITION?: CursorPositionMessage['position'];
    /** Open target forwarded by the VS Code extension host. */
    __PLAYGROUND_OPEN_TARGET?: OpenPlaygroundMessage['target'];
    acquireVsCodeApi?: () => { postMessage: (message: unknown) => void };
  }
}

let vscodeApi: { postMessage: (message: unknown) => void } | null | undefined;

interface CursorPositionMessage {
  type: 'cursorPosition';
  position: {
    file: string;
    line: number;
    column: number;
  };
}

interface OpenPlaygroundMessage {
  type: 'openPlayground';
  target: {
    project: string;
    functionName?: string;
  };
}

function getVsCodeApi() {
  if (vscodeApi !== undefined) return vscodeApi;
  vscodeApi = typeof window.acquireVsCodeApi === 'function'
    ? window.acquireVsCodeApi()
    : null;
  return vscodeApi;
}

const App: React.FC = () => {
  const [port, setPort] = useState<WebSocketRuntimePort | null>(null);
  const portRef = useRef<WebSocketRuntimePort | null>(null);
  const pendingCursorPositionRef = useRef<CursorPositionMessage['position'] | null>(null);
  const pendingOpenTargetRef = useRef<OpenPlaygroundMessage['target'] | null>(null);

  useEffect(() => {
    portRef.current = port;
    if (!port) return;

    if (pendingOpenTargetRef.current) {
      const target = pendingOpenTargetRef.current;
      pendingOpenTargetRef.current = null;
      port.dispatchLocalMessage({
        type: 'playgroundNotification',
        notification: {
          type: 'openPlayground',
          project: target.project,
          ...(target.functionName ? { functionName: target.functionName } : {}),
        },
      });
    }

    if (pendingCursorPositionRef.current) {
      const position = pendingCursorPositionRef.current;
      pendingCursorPositionRef.current = null;
      port.postMessage({
        type: 'cursorPosition',
        file: position.file,
        line: position.line,
        column: position.column,
      });
    }
  }, [port]);

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

  useEffect(() => {
    const forwardCursorPosition = (position: CursorPositionMessage['position']) => {
      const currentPort = portRef.current;
      if (!currentPort) {
        pendingCursorPositionRef.current = position;
        return;
      }
      currentPort.postMessage({
        type: 'cursorPosition',
        file: position.file,
        line: position.line,
        column: position.column,
      });
    };

    const forwardOpenPlayground = (target: OpenPlaygroundMessage['target']) => {
      const currentPort = portRef.current;
      if (!currentPort) {
        pendingOpenTargetRef.current = target;
        return;
      }
      currentPort.dispatchLocalMessage({
        type: 'playgroundNotification',
        notification: {
          type: 'openPlayground',
          project: target.project,
          ...(target.functionName ? { functionName: target.functionName } : {}),
        },
      });
    };

    const initialPosition = window.__PLAYGROUND_CURSOR_POSITION;
    if (initialPosition) {
      forwardCursorPosition(initialPosition);
    }
    const initialOpenTarget = window.__PLAYGROUND_OPEN_TARGET;
    if (initialOpenTarget) {
      forwardOpenPlayground(initialOpenTarget);
    }

    const onMessage = (event: MessageEvent<unknown>) => {
      const message = event.data as Partial<CursorPositionMessage | OpenPlaygroundMessage> | undefined;
      if (message?.type === 'cursorPosition' && message.position) {
        forwardCursorPosition(message.position);
      } else if (message?.type === 'openPlayground' && message.target) {
        forwardOpenPlayground(message.target);
      }
    };

    window.addEventListener('message', onMessage);
    getVsCodeApi()?.postMessage({ type: 'webviewReady' });
    return () => window.removeEventListener('message', onMessage);
  }, []);

  if (!port) {
    return (
      <main className="playground-root h-full w-full overflow-hidden flex flex-col items-center justify-center text-sm text-muted-foreground gap-2">
        <p>Connecting to playground server...</p>
      </main>
    );
  }

  return (
    <div className="playground-root flex h-full min-h-0 w-full flex-col overflow-hidden">
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
