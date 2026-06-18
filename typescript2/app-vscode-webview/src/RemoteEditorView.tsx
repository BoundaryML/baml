/**
 * RemoteEditorView — the Monaco workbench (file tree + editor + LSP) for the
 * standalone `baml-cli playground`, backed by the real server over WebSockets.
 *
 * Unlike promptfiddle (which runs the BAML runtime in a WASM worker), here:
 *   - language features come from the server's LSP over `/api/lsp`, and
 *   - execution comes from `/api/ws` (the playground runtime),
 * both via @b/pkg-editor's remote backend. No WASM in the browser.
 *
 * The editor is rooted at the project's REAL on-disk path so the file URIs it
 * produces (`file://<project>/<rel>`) match the URIs the server emits in
 * diagnostics — no URI translation needed. Edits stream to the server as LSP
 * didChange/didSave, and the server writes them through to disk.
 *
 * This module is loaded lazily so monaco-vscode-api never enters the bundle
 * for the plain playground view (or the VS Code webview).
 */

import { useEffect, useMemo, useState } from 'react';
import { MonacoEditor, createRemoteBackend, remoteUrlsFromLocation } from '@b/pkg-editor';
import '@b/pkg-editor/views-workbench.css';

interface SourceFile {
  path: string;
  relativePath: string;
  content: string;
}

interface SourceFilesResponse {
  project: string;
  files: SourceFile[];
}

export interface RemoteEditorViewProps {
  /** Absolute path to the BAML project root (as reported by the playground). */
  project: string;
}

const RemoteEditorView: React.FC<RemoteEditorViewProps> = ({ project }) => {
  const [files, setFiles] = useState<Record<string, string> | null>(null);
  const [error, setError] = useState<string | null>(null);

  const backend = useMemo(() => {
    const { lspUrl, runtimeUrl } = remoteUrlsFromLocation();
    return createRemoteBackend({ lspUrl, runtimeUrl, windowLabel: 'BAML Playground' });
  }, []);

  useEffect(() => {
    let cancelled = false;
    setFiles(null);
    setError(null);
    void (async () => {
      try {
        const res = await fetch(`/api/source-files?project=${encodeURIComponent(project)}`);
        if (!res.ok) throw new Error(`Failed to load source files (${res.status})`);
        const data = (await res.json()) as SourceFilesResponse;
        if (cancelled) return;
        const map: Record<string, string> = {};
        for (const f of data.files) map[f.relativePath] = f.content;
        setFiles(map);
      } catch (e) {
        if (!cancelled) setError(e instanceof Error ? e.message : String(e));
      }
    })();
    return () => { cancelled = true; };
  }, [project]);

  if (error) {
    return (
      <div className="flex h-full w-full items-center justify-center p-4 text-sm text-red-400">
        Failed to open editor: {error}
      </div>
    );
  }

  if (!files) {
    return (
      <div className="flex h-full w-full items-center justify-center text-sm text-muted-foreground">
        Loading project…
      </div>
    );
  }

  return (
    <MonacoEditor
      files={files}
      // The server owns the files on disk; edits stream to it via the language
      // client (didChange/didSave). Nothing to persist on the client side.
      onFilesChange={() => {}}
      backend={backend}
      workspaceRoot={project}
      // Auto-save so edits flow to the server (and through to disk) without a
      // manual Cmd+S.
      autoSaveDelayMs={800}
      height="100%"
    />
  );
};

export default RemoteEditorView;
