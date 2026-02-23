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

import type { FC } from 'react';
import { usePlayground } from './PlaygroundProvider';
import { MonacoEditor } from './MonacoEditor';

export const SplitPreview: FC = () => {
  const { files, setFiles } = usePlayground();

  return (
    <div className="font-vsc text-vsc-text relative h-full w-full">
      <MonacoEditor
        files={files}
        onFilesChange={setFiles}
        height="100%"
      />
    </div>
  );
};
