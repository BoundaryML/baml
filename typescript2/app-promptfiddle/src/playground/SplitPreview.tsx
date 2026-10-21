// biome-ignore-all lint/style/useFilenamingConvention: Keep the existing public module path.
/**
 * SplitPreview — composition shell for the promptfiddle editor + execution panel.
 *
 * The MonacoEditor takes full screen and hosts the VS Code workbench.
 * The ExecutionPanel is rendered as a custom EditorPane tab inside that
 * workbench (opened automatically when the WASM worker is ready).
 *
 * One worker, one WASM runtime. MonacoEditor owns the worker lifecycle
 * and opens the execution panel pane via ExecutionPanelPane.ts.
 */

import { MonacoEditor } from '@b/pkg-editor';
import { configureProxyEnvVar, initPlaygroundEnv } from '@b/pkg-playground';
import { useSetAtom } from 'jotai';
import type { FC } from 'react';
import { useEffect, useMemo } from 'react';
import { blobUrlsAtom, usePlayground } from './PlaygroundProvider';
import { createWorkerBackend } from './worker-backend';

/**
 * Route the WASM runtime's LLM requests through the promptfiddle proxy (a
 * Cloudflare Worker that injects our provider API keys and bypasses CORS) so
 * users don't have to bring their own keys. The runtime reads BOUNDARY_PROXY_URL
 * and prepends it to each request URL, so it must include a scheme.
 */
const BOUNDARY_PROXY_URL = 'https://proxy.promptfiddle.com';

// promptfiddle surfaces a Boundary-gateway on/off toggle in the env-vars dialog
// (the VS Code extension and CLI playground hide it). Configure at import,
// before the ExecutionPanel pane mounts.
configureProxyEnvVar({ url: BOUNDARY_PROXY_URL, visible: true });

export const SplitPreview: FC = () => {
  const { files, setFiles, resetFiles } = usePlayground();
  const setBlobUrls = useSetAtom(blobUrlsAtom);
  const backend = useMemo(
    () =>
      createWorkerBackend({
        onReset: () => {
          resetFiles();
          window.location.reload();
        },
      }),
    [resetFiles],
  );

  // Seed playground env defaults once: placeholder provider keys + gateway on.
  useEffect(() => {
    initPlaygroundEnv();
  }, []);

  return (
    <div className="font-vsc text-vsc-text h-full w-full">
      <MonacoEditor
        backend={backend}
        files={files}
        height="100%"
        onBlobUrlsChange={setBlobUrls}
        onFilesChange={setFiles}
      />
    </div>
  );
};
