import { useState, useEffect, useRef, lazy, Suspense } from 'react';
import { ExecutionPanel, WebSocketRuntimePort, type SourceNavigationTarget } from '@b/pkg-playground';

// Monaco workbench is heavy (monaco-vscode-api) and only needed when the user
// opens the editor view; lazy-load it so the plain playground (and the VS Code
// webview, which is already inside an editor) never pull it into the bundle.
const RemoteEditorView = lazy(() => import('./RemoteEditorView'));

declare global {
  interface Window {
    /** Injected by the VS Code extension's webview HTML wrapper. */
    __PLAYGROUND_WS_URL?: string;
    /** Cursor position forwarded by the VS Code extension host. */
    __PLAYGROUND_CURSOR_POSITION?: CursorPositionMessage['position'];
    /** Open target forwarded by the VS Code extension host. */
    __PLAYGROUND_OPEN_TARGET?: OpenPlaygroundMessage['target'];
    /** Injected by the server's `/studio` shell so the app lands on Telemetry. */
    __STUDIO_INITIAL_TAB?: 'telemetry';
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
    testName?: string;
    testsetName?: string;
  };
}

interface OpenInBrowserMessage {
  type: 'openInBrowser';
  project?: string;
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
  const [activeProject, setActiveProject] = useState<string | null>(null);
  const [showEditor, setShowEditor] = useState(false);
  const [editorHasUnsaved, setEditorHasUnsaved] = useState(false);
  // Once the Monaco editor has been opened we keep it MOUNTED (just hidden via
  // CSS when toggled off). monaco-vscode-api can only initialize once per page,
  // so unmounting + remounting it on toggle would throw "Cannot register two
  // commands…". Keeping it mounted means init happens exactly once.
  const [editorEverOpened, setEditorEverOpened] = useState(false);
  const portRef = useRef<WebSocketRuntimePort | null>(null);
  const pendingCursorPositionRef = useRef<CursorPositionMessage['position'] | null>(null);
  const pendingOpenTargetRef = useRef<OpenPlaygroundMessage['target'] | null>(null);
  const inVsCode = getVsCodeApi() !== null;
  // The Monaco editor view is offered only when running standalone in a browser;
  // inside VS Code the user already has a full editor.
  const canShowEditor = !inVsCode;

  useEffect(() => {
    portRef.current = port;
    if (!port) return;

    if (pendingOpenTargetRef.current) {
      const target = pendingOpenTargetRef.current;
      pendingOpenTargetRef.current = null;
      setActiveProject(target.project);
      port.dispatchLocalMessage({
        type: 'playgroundNotification',
        notification: {
          type: 'openPlayground',
          project: target.project,
          ...(target.functionName !== undefined ? { functionName: target.functionName } : {}),
          ...(target.testName !== undefined ? { testName: target.testName } : {}),
          ...(target.testsetName !== undefined ? { testsetName: target.testsetName } : {}),
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
    const scheme = window.location.protocol === 'https:' ? 'wss' : 'ws';
    const wsUrl =
      window.__PLAYGROUND_WS_URL ?? `${scheme}://${window.location.host}/api/ws`;
    const runtimePort = new WebSocketRuntimePort(wsUrl);
    setPort(runtimePort);
    return () => runtimePort.dispose();
  }, []);

  // Track the active project from playground notifications so the editor view
  // can be rooted at (and load source files for) the real project.
  useEffect(() => {
    if (!port) return;
    const unsubscribe = port.onMessage((msg) => {
      if (msg.type !== 'playgroundNotification') return;
      const n = msg.notification;
      if (n.type === 'openPlayground') {
        setActiveProject(n.project);
      } else if (n.type === 'listProjects' && n.projects.length > 0) {
        // Only adopt the first discovered project if we don't have one yet.
        setActiveProject((prev) => prev ?? n.projects[0]);
      }
    });
    // The port buffers incoming messages and replays them only to the FIRST
    // handler that subscribes — which is ExecutionPanel (a child mounts before
    // this parent effect runs). So the initial listProjects/openPlayground is
    // already drained by the time we get here. Ask the server to re-broadcast
    // its state now that our handler is registered.
    port.postMessage({ type: 'requestState' });
    return unsubscribe;
  }, [port]);

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
      setActiveProject(target.project);
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
          ...(target.functionName !== undefined ? { functionName: target.functionName } : {}),
          ...(target.testName !== undefined ? { testName: target.testName } : {}),
          ...(target.testsetName !== undefined ? { testsetName: target.testsetName } : {}),
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

  const editorActive = showEditor && canShowEditor && activeProject !== null;
  if (editorActive && !editorEverOpened) {
    // Mark opened on the render that first activates it; the editor then stays
    // mounted for the rest of the session (hidden via CSS when toggled off).
    setEditorEverOpened(true);
  }

  return (
    <div className="playground-root flex h-full min-h-0 w-full flex-col overflow-hidden">
      {(inVsCode || canShowEditor) && (
        <header className="flex h-10 shrink-0 items-center justify-end gap-2 border-b border-border bg-background px-2">
          {editorActive && (
            <span
              className={
                'flex items-center gap-1.5 rounded-md px-2.5 py-1 text-xs font-semibold ' +
                (editorHasUnsaved
                  ? 'bg-amber-500 text-black'
                  : 'bg-emerald-600/90 text-white')
              }
              title={
                editorHasUnsaved
                  ? 'You have unsaved changes. Press ⌘S / Ctrl+S to write them to disk.'
                  : 'All changes saved to disk.'
              }
            >
              <span className={'inline-block h-2 w-2 rounded-full ' + (editorHasUnsaved ? 'bg-black/80' : 'bg-white/90')} />
              {editorHasUnsaved ? 'Unsaved — ⌘S to save' : 'All saved'}
            </span>
          )}
          {canShowEditor && (
            <>
              <button
                type="button"
                onClick={() => setShowEditor((v) => !v)}
                disabled={!showEditor && activeProject === null}
                title={
                  activeProject === null
                    ? 'Waiting for a BAML project to load…'
                    : showEditor
                      ? 'Hide the code editor'
                      : 'Open the code editor alongside the playground'
                }
                className="h-7 rounded-md border border-border px-3 text-xs font-medium text-foreground hover:bg-muted disabled:cursor-not-allowed disabled:opacity-50"
              >
                {showEditor ? 'Playground only' : 'Open editor'}
              </button>
            </>
          )}
          {inVsCode && (
            <button
              type="button"
              onClick={() => {
                const message: OpenInBrowserMessage = {
                  type: 'openInBrowser',
                  ...(activeProject ? { project: activeProject } : {}),
                };
                getVsCodeApi()?.postMessage(message);
              }}
              className="h-7 rounded-md border border-border px-3 text-xs font-medium text-foreground hover:bg-muted"
            >
              Open in browser
            </button>
          )}
        </header>
      )}
      {/*
        The Monaco workbench hosts its own "Playground" pane, so it fully
        replaces the standalone ExecutionPanel while active. Once opened it stays
        mounted (hidden via CSS) so monaco-vscode-api is never re-initialized.
      */}
      {editorEverOpened && (
        <div className="min-h-0 flex-1" style={{ display: editorActive ? 'flex' : 'none' }}>
          <Suspense
            fallback={
              <div className="flex h-full w-full items-center justify-center text-sm text-muted-foreground">
                Loading editor…
              </div>
            }
          >
            <RemoteEditorView project={activeProject!} onUnsavedChange={setEditorHasUnsaved} />
          </Suspense>
        </div>
      )}
      {!editorActive && (
        <ExecutionPanel
          port={port}
          initialTab={window.__STUDIO_INITIAL_TAB}
          onNavigateToSource={(source: SourceNavigationTarget) => {
            getVsCodeApi()?.postMessage({ type: 'navigateToSource', source });
          }}
        />
      )}
    </div>
  );
};

export default App;
