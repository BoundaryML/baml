/**
 * Worker EditorBackend — the in-browser WASM backend for promptfiddle.
 *
 * A dedicated worker (`baml-lsp-worker.ts`) loads the BAML WASM runtime and
 * acts as BOTH:
 *   - the LSP endpoint, over a MessageChannel (BrowserMessageReader/Writer), and
 *   - the playground runtime, over the worker's own postMessage channel
 *     (wrapped by WorkerRuntimePort).
 *
 * This file owns everything WASM-specific so the shared workbench in
 * `@b/pkg-editor` stays free of WASM and can also run against a remote server.
 */

import type {
  EditorBackend,
  EditorConnection,
  WorkbenchHandle,
} from '@b/pkg-editor';
import { fromDataUrl, isMediaPath } from '@b/pkg-editor';
import type { DecorationOptions, TextEditor } from 'vscode';

interface LogDecoration {
  line: number;
  level: string;
  message: string;
  count: number;
}

interface WorkerBackendOptions {
  onReset?: () => void;
}

const RESET_COMMAND = 'baml.promptfiddle.reset';

export function createWorkerBackend(
  options: WorkerBackendOptions = {},
): EditorBackend {
  return {
    async connect(handle: WorkbenchHandle): Promise<EditorConnection> {
      const { BrowserMessageReader, BrowserMessageWriter } = await import(
        'vscode-languageclient/browser'
      );
      const { WorkerRuntimePort } = await import('@b/pkg-playground');

      const vscode = handle.vscode;
      const disposers: Array<() => void> = [];

      const resetStatus = vscode.window.createStatusBarItem(
        vscode.StatusBarAlignment.Left,
        100,
      );
      resetStatus.command = RESET_COMMAND;
      resetStatus.text = '$(discard) Reset';
      resetStatus.tooltip = 'Reset Prompt Fiddle to its default files';
      resetStatus.show();

      let resetConfirmationTimeout: ReturnType<typeof setTimeout> | undefined;
      const clearResetConfirmation = () => {
        if (resetConfirmationTimeout !== undefined) {
          clearTimeout(resetConfirmationTimeout);
          resetConfirmationTimeout = undefined;
        }
        resetStatus.text = '$(discard) Reset';
      };
      const resetCommand = vscode.commands.registerCommand(
        RESET_COMMAND,
        () => {
          if (resetConfirmationTimeout !== undefined) {
            clearResetConfirmation();
            options.onReset?.();
            return;
          }

          resetStatus.text = '$(warning) Click again to reset';
          resetConfirmationTimeout = setTimeout(clearResetConfirmation, 3000);
        },
      );

      const versionStatus = vscode.window.createStatusBarItem(
        vscode.StatusBarAlignment.Left,
        99,
      );
      versionStatus.tooltip = 'Deployed BAML runtime version';

      disposers.push(
        () => clearResetConfirmation(),
        () => resetCommand.dispose(),
        () => resetStatus.dispose(),
        () => versionStatus.dispose(),
      );

      // Spawn worker — WASM loads inside the worker, doesn't block main thread.
      // The `new URL(..., import.meta.url)` must live in app source so the
      // bundler emits the worker as an asset.
      const worker = new Worker(
        new URL('./baml-lsp-worker.ts', import.meta.url),
        { name: 'BAML Worker', type: 'module' },
      );

      const onWorkerReady = (event: MessageEvent) => {
        if (event.data?.type !== 'ready') return;
        worker.removeEventListener('message', onWorkerReady);
        if (typeof event.data.version === 'string') {
          const commit =
            typeof event.data.commit === 'string' &&
            event.data.commit.length > 0
              ? ` (${event.data.commit})`
              : '';
          versionStatus.text = `BAML ${event.data.version}${commit}`;
          versionStatus.show();
        }
      };
      worker.addEventListener('message', onWorkerReady);
      disposers.push(() =>
        worker.removeEventListener('message', onWorkerReady),
      );

      // ── VFS mutations from the WASM runtime (worker → main) ────────────
      let vfsQueue: Promise<void> = Promise.resolve();
      const enqueueVfs = (op: () => Promise<void>) => {
        vfsQueue = vfsQueue.then(op).catch((err) => {
          console.error('[worker-backend] failed to apply VFS event:', err);
        });
      };

      const onVfsChange = (event: MessageEvent) => {
        if (handle.isDisposed()) return;
        const data = event.data;
        if (data?.type === 'vfsFileChanged') {
          const { path: relPath, content } = data as {
            path: string;
            content: string;
          };
          handle.liveFiles[relPath] = content;
          const absPath = `/workspace/${relPath}`;
          const isMedia = isMediaPath(relPath);
          const bytes = isMedia
            ? fromDataUrl(content)
            : handle.encoder.encode(content);
          enqueueVfs(async () => {
            const parts = absPath.split('/');
            for (let i = 2; i < parts.length; i++) {
              const parentPath = parts.slice(0, i).join('/');
              try {
                await handle.fileSystemProvider.mkdir(
                  vscode.Uri.file(parentPath),
                );
              } catch {
                /* exists */
              }
            }
            await handle.fileSystemProvider.writeFile(
              vscode.Uri.file(absPath),
              bytes,
              { atomic: false, create: true, overwrite: true, unlock: false },
            );
            if (isMedia) handle.updateBlobUrl(relPath, bytes);
            handle.notifyFilesChanged();
          });
        } else if (data?.type === 'vfsFileDeleted') {
          const { path: relPath } = data as { path: string };
          delete handle.liveFiles[relPath];
          const absPath = `/workspace/${relPath}`;
          enqueueVfs(async () => {
            await Promise.resolve(
              handle.fileSystemProvider.delete(vscode.Uri.file(absPath), {
                atomic: false,
                recursive: false,
                useTrash: false,
              }),
            ).catch(() => {});
            if (isMediaPath(relPath)) handle.removeBlobUrl(relPath);
            handle.notifyFilesChanged();
          });
        }
      };
      worker.addEventListener('message', onVfsChange);
      disposers.push(() => worker.removeEventListener('message', onVfsChange));

      // ── LSP transport over a MessageChannel ───────────────────────────
      const channel = new MessageChannel();
      worker.postMessage(
        {
          initialFiles: { ...handle.liveFiles },
          port: channel.port2,
          rootPath: '/workspace',
        },
        [channel.port2],
      );

      const reader = new BrowserMessageReader(channel.port1);
      const writer = new BrowserMessageWriter(channel.port1);

      const runtimePort = new WorkerRuntimePort(worker);
      disposers.push(() => runtimePort.dispose());

      // ── Log decorations (inline display like ErrorLens) ───────────────
      let lastDecoratedEditor: TextEditor | undefined;
      const logDecorationTypes = {
        debug: vscode.window.createTextEditorDecorationType({
          after: { color: '#888888', margin: '0 0 0 1em' },
          isWholeLine: true,
        }),
        error: vscode.window.createTextEditorDecorationType({
          after: { color: '#f14c4c', margin: '0 0 0 1em' },
          isWholeLine: true,
        }),
        info: vscode.window.createTextEditorDecorationType({
          after: { color: '#3794ff', margin: '0 0 0 1em' },
          isWholeLine: true,
        }),
        warn: vscode.window.createTextEditorDecorationType({
          after: { color: '#cca700', margin: '0 0 0 1em' },
          isWholeLine: true,
        }),
      };
      disposers.push(
        () => logDecorationTypes.debug.dispose(),
        () => logDecorationTypes.info.dispose(),
        () => logDecorationTypes.warn.dispose(),
        () => logDecorationTypes.error.dispose(),
      );

      const applyLogDecorations = (decorations: LogDecoration[]) => {
        try {
          let editor = vscode.window.activeTextEditor;
          if (!editor) {
            editor = vscode.window.visibleTextEditors.find((e) =>
              e.document.uri.path.endsWith('.baml'),
            );
          }
          if (!editor) return;
          lastDecoratedEditor = editor;

          const byLevel: Record<string, DecorationOptions[]> = {
            debug: [],
            error: [],
            info: [],
            warn: [],
          };
          for (const dec of decorations) {
            const level = dec.level as keyof typeof byLevel;
            if (!(level in byLevel)) continue;
            const line = dec.line - 1; // Convert to 0-indexed
            if (line < 0) continue;
            const countSuffix = dec.count > 1 ? ` ×${dec.count}` : '';
            const text = `  // ${dec.level}: ${dec.message}${countSuffix}`;
            byLevel[level].push({
              range: new vscode.Range(line, 0, line, 0),
              renderOptions: { after: { contentText: text } },
            });
          }
          editor.setDecorations(logDecorationTypes.debug, byLevel.debug);
          editor.setDecorations(logDecorationTypes.info, byLevel.info);
          editor.setDecorations(logDecorationTypes.warn, byLevel.warn);
          editor.setDecorations(logDecorationTypes.error, byLevel.error);
        } catch {
          // Silently ignore decoration errors
        }
      };

      const clearLogDecorations = () => {
        if (!lastDecoratedEditor) return;
        lastDecoratedEditor.setDecorations(logDecorationTypes.debug, []);
        lastDecoratedEditor.setDecorations(logDecorationTypes.info, []);
        lastDecoratedEditor.setDecorations(logDecorationTypes.warn, []);
        lastDecoratedEditor.setDecorations(logDecorationTypes.error, []);
        lastDecoratedEditor = undefined;
      };

      const onLogDecorations = (event: MessageEvent) => {
        if (handle.isDisposed()) return;
        const data = event.data;
        if (data?.type === 'logDecorations') {
          applyLogDecorations(data.decorations);
        } else if (data?.type === 'clearLogDecorations') {
          clearLogDecorations();
        }
      };
      worker.addEventListener('message', onLogDecorations);
      disposers.push(() =>
        worker.removeEventListener('message', onLogDecorations),
      );

      return {
        dispose() {
          for (const d of disposers) {
            try {
              d();
            } catch {
              /* no-op */
            }
          }
          try {
            worker.postMessage({ type: 'dispose' });
          } catch {
            /* already gone */
          }
          worker.terminate();
        },
        lcConnection: {
          messageTransports: { reader, writer },
          options: {
            $type: 'WorkerDirect',
            messagePort: channel.port1,
            worker,
          },
        },
        onCursorMoved(file, line, column) {
          worker.postMessage({ column, file, line, type: 'cursorPosition' });
        },
        onFilesChanged(files) {
          worker.postMessage({ files, type: 'filesChanged' });
        },
        runtimePort,
      };
    },
    supportsReload: true,
  };
}
