'use client';

// Shared Monaco workbench (file tree + editor + LSP + Playground pane).
export { MonacoEditor } from './MonacoEditor';
export type { MonacoEditorProps } from './MonacoEditor';

// The pluggable transport seam.
export type {
  EditorBackend,
  EditorConnection,
  WorkbenchHandle,
  WorkbenchFs,
} from './backend';

// Remote (no-WASM) backend for `baml-cli playground` and similar servers.
export {
  createRemoteBackend,
  remoteUrlsFromLocation,
} from './remote-backend';
export type { RemoteBackendOptions } from './remote-backend';

// Media helpers (text vs data-URL files).
export {
  isMediaPath,
  mimeFromPath,
  toDataUrl,
  fromDataUrl,
  MIME_TYPES,
} from './media';
