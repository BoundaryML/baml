/**
 * Remote EditorBackend — language features + execution come from a real server
 * on the other end of two WebSockets (e.g. `baml-cli playground`):
 *
 *   - LSP JSON-RPC over `ws://host/api/lsp` (diagnostics, hover, completion).
 *     monaco-languageclient's LanguageClientWrapper owns this socket; we just
 *     hand it the URL.
 *   - The playground runtime over `ws://host/api/ws` (runs, tests, env), via
 *     the existing WebSocketRuntimePort.
 *
 * No WASM in the browser — the server owns the files and the BAML runtime.
 * Edits stream to the server as LSP didChange/didSave; the server writes them
 * through to disk.
 */

import { WebSocketRuntimePort } from '@b/pkg-playground';
import type { EditorBackend, EditorConnection, WorkbenchHandle } from './backend';

export interface RemoteBackendOptions {
  /** WebSocket URL for the LSP JSON-RPC endpoint, e.g. `ws://localhost:4265/api/lsp`. */
  lspUrl: string;
  /** WebSocket URL for the playground runtime endpoint, e.g. `ws://localhost:4265/api/ws`. */
  runtimeUrl: string;
  /** Window-indicator label shown in the workbench title bar. */
  windowLabel?: string;
}

/** Build a {@link RemoteBackendOptions} pair from the current page origin. */
export function remoteUrlsFromLocation(
  loc: { host: string; protocol: string } = window.location,
): { lspUrl: string; runtimeUrl: string } {
  const scheme = loc.protocol === 'https:' ? 'wss' : 'ws';
  return {
    lspUrl: `${scheme}://${loc.host}/api/lsp`,
    runtimeUrl: `${scheme}://${loc.host}/api/ws`,
  };
}

export function createRemoteBackend(options: RemoteBackendOptions): EditorBackend {
  return {
    windowLabel: options.windowLabel ?? 'BAML Playground',
    supportsReload: false,
    async connect(_handle: WorkbenchHandle): Promise<EditorConnection> {
      const runtimePort = new WebSocketRuntimePort(options.runtimeUrl);

      return {
        lcConnection: {
          // LanguageClientWrapper opens this socket and wires WebSocketMessage
          // reader/writer once it's connected.
          options: { $type: 'WebSocketUrl', url: options.lspUrl },
        },
        runtimePort,
        onCursorMoved(file, line, column) {
          // Forwarded to the server for playground cursor-context highlighting.
          runtimePort.postMessage({ type: 'cursorPosition', file, line, column });
        },
        // Edits stream to the server via the language client's didChange/didSave;
        // no explicit file push is needed here.
        dispose() {
          runtimePort.dispose();
        },
      };
    },
  };
}
