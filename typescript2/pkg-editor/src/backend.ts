/**
 * EditorBackend — the pluggable seam between the shared Monaco workbench
 * (workbench setup, in-memory FS, file tracking, language-client lifecycle)
 * and whatever provides language features + function execution.
 *
 * Two implementations exist:
 *
 *   - Worker backend (app-promptfiddle): an in-browser WASM worker acts as
 *     both the LSP endpoint (over a MessageChannel) and the runtime.
 *
 *   - Remote backend (@b/pkg-editor/remote): a real LSP server on the other
 *     end of a WebSocket (e.g. `baml-cli playground`). No WASM in the browser.
 *
 * The workbench is identical in both cases; only the transports differ.
 */

import type { ConnectionConfig } from 'monaco-languageclient/lcwrapper';
import type { RuntimePort } from '@b/pkg-playground';

/**
 * Minimal view of the in-memory VS Code file system provider, exposing only
 * what backends need to reflect server/worker-initiated file mutations back
 * into the editor.
 */
export interface WorkbenchFs {
  mkdir(uri: unknown): Promise<void> | void;
  // `opts` is typed loosely so the concrete VS Code InMemoryFileSystemProvider
  // (whose write/delete option types are more specific) remains assignable.
  writeFile(uri: unknown, content: Uint8Array, opts: unknown): Promise<void> | void;
  delete(uri: unknown, opts: unknown): Promise<void> | void;
  readFile(uri: unknown): Promise<Uint8Array> | Uint8Array;
}

/**
 * Handle to the live workbench, passed to a backend each time it connects.
 * Mirrors the workbench's mutable context so backends can read the current
 * files and write changes (e.g. files a WASM runtime generates) back into
 * the editor's in-memory FS.
 */
export interface WorkbenchHandle {
  /** The `vscode` API namespace (already imported by the workbench). */
  readonly vscode: typeof import('vscode');
  /**
   * Live file map (relPath -> text content / data-URL). Owned and mutated by
   * the workbench; backends read it and may hand a fresh snapshot to the
   * runtime. Do not mutate directly — use {@link writeFile}/{@link deleteFile}.
   */
  readonly liveFiles: Record<string, string>;
  /** Sandboxed in-memory FS provider (rooted at /workspace). */
  readonly fileSystemProvider: WorkbenchFs;
  readonly encoder: TextEncoder;
  readonly decoder: TextDecoder;
  /** Create/refresh the blob: URL for a media file (for <img> previews). */
  updateBlobUrl(filename: string, bytes: Uint8Array): void;
  /** Drop the blob: URL for a deleted media file. */
  removeBlobUrl(filename: string): void;
  /** Notify the host (MonacoEditor `onFilesChange` prop) of the latest files. */
  notifyFilesChanged(files?: Record<string, string>): void;
  /** True once the editor has been unmounted; backends should bail. */
  isDisposed(): boolean;
}

/**
 * A live connection produced by {@link EditorBackend.connect}: the LSP
 * transport config for the language client, the execution-panel transport,
 * and optional hooks the workbench calls as the user edits.
 */
export interface EditorConnection {
  /** Connection config handed to `monaco-languageclient`'s LanguageClientWrapper. */
  lcConnection: ConnectionConfig;
  /** Transport for the ExecutionPanel (runs/tests/env). */
  runtimePort: RuntimePort;
  /**
   * Called by the workbench after an edit/create/delete with the full current
   * file map. Worker backends push this into the worker VFS; remote backends
   * usually no-op (the language client streams didChange to the server).
   */
  onFilesChanged?(files: Record<string, string>): void;
  /** Called when the caret moves inside a .baml file (0-indexed). */
  onCursorMoved?(file: string, line: number, column: number): void;
  /** Tear down transports (and any worker). */
  dispose(): void | Promise<void>;
}

export interface EditorBackend {
  /**
   * Establish a connection. Called once on mount, and again on reload().
   * The returned connection's transports drive the language client + panel.
   */
  connect(handle: WorkbenchHandle): Promise<EditorConnection>;
  /** Show the dev "reload" button (worker restart). Defaults to false. */
  readonly supportsReload?: boolean;
  /** Window-indicator label shown in the workbench title bar. */
  readonly windowLabel?: string;
}
